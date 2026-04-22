use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::State,
    http::{header::HeaderName, HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use tokio::sync::Mutex;

use crate::{ApiError, ApiResult, AppState, RateLimitConfig};

const ENTRY_TTL_MULTIPLIER: u32 = 10;
static X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientIpMode {
    Socket,
    XForwardedFor,
}

impl ClientIpMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Socket => "socket",
            Self::XForwardedFor => "x-forwarded-for",
        }
    }
}

impl FromStr for ClientIpMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "socket" => Ok(Self::Socket),
            "x-forwarded-for" => Ok(Self::XForwardedFor),
            other => Err(format!("unknown rate_limit_client_ip_mode: {other}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrustedProxyNet {
    V4 { network: u32, prefix: u8 },
    V6 { network: u128, prefix: u8 },
}

impl TrustedProxyNet {
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self, ip) {
            (Self::V4 { network, prefix }, IpAddr::V4(ip)) => masked_v4(ip, *prefix) == *network,
            (Self::V6 { network, prefix }, IpAddr::V6(ip)) => masked_v6(ip, *prefix) == *network,
            _ => false,
        }
    }
}

impl FromStr for TrustedProxyNet {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (ip, prefix) = value
            .split_once('/')
            .ok_or_else(|| format!("trusted proxy CIDR must include prefix length: {value}"))?;
        let ip = ip
            .parse::<IpAddr>()
            .map_err(|_| format!("invalid trusted proxy address: {value}"))?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(|_| format!("invalid trusted proxy prefix length: {value}"))?;

        match ip {
            IpAddr::V4(ip) => {
                if prefix > 32 {
                    return Err(format!("IPv4 trusted proxy prefix must be <= 32: {value}"));
                }
                Ok(Self::V4 {
                    network: masked_v4(ip, prefix),
                    prefix,
                })
            }
            IpAddr::V6(ip) => {
                if prefix > 128 {
                    return Err(format!("IPv6 trusted proxy prefix must be <= 128: {value}"));
                }
                Ok(Self::V6 {
                    network: masked_v6(ip, prefix),
                    prefix,
                })
            }
        }
    }
}

pub fn parse_trusted_proxy_cidrs(value: &str) -> Result<Vec<TrustedProxyNet>, String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(TrustedProxyNet::from_str)
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub enum RateLimitTarget {
    Auth,
    Barcode,
    History,
}

#[derive(Clone)]
pub struct RateLimitLayerState {
    app_state: AppState,
    target: RateLimitTarget,
}

impl RateLimitLayerState {
    pub fn new(app_state: AppState, target: RateLimitTarget) -> Self {
        Self { app_state, target }
    }
}

#[derive(Debug)]
pub struct RateLimiters {
    auth: KeyedRateLimiter,
    barcode: KeyedRateLimiter,
    history: KeyedRateLimiter,
}

impl RateLimiters {
    pub fn new(config: &crate::ApiConfig) -> Self {
        Self {
            auth: KeyedRateLimiter::new(config.rate_limit_auth.clone()),
            barcode: KeyedRateLimiter::new(config.rate_limit_barcode.clone()),
            history: KeyedRateLimiter::new(config.rate_limit_history.clone()),
        }
    }

    fn for_target(&self, target: RateLimitTarget) -> &KeyedRateLimiter {
        match target {
            RateLimitTarget::Auth => &self.auth,
            RateLimitTarget::Barcode => &self.barcode,
            RateLimitTarget::History => &self.history,
        }
    }
}

pub async fn enforce(
    State(state): State<RateLimitLayerState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> ApiResult<Response> {
    let key = client_key(
        request.headers(),
        &request,
        state.app_state.config.rate_limit_client_ip_mode,
        &state.app_state.config.rate_limit_trusted_proxy_cidrs,
    );
    let limiter = state.app_state.rate_limiters.for_target(state.target);
    if !limiter.allow(&key).await {
        return Err(ApiError::RateLimited);
    }
    Ok(next.run(request).await)
}

pub fn client_key<B>(
    headers: &HeaderMap,
    request: &Request<B>,
    client_ip_mode: ClientIpMode,
    trusted_proxy_cidrs: &[TrustedProxyNet],
) -> String {
    let socket_ip = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip());

    if client_ip_mode == ClientIpMode::XForwardedFor
        && socket_ip
            .as_ref()
            .is_some_and(|ip| trusted_proxy_cidrs.iter().any(|cidr| cidr.contains(*ip)))
    {
        if let Some(forwarded) = headers
            .get(&X_FORWARDED_FOR)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_forwarded_for)
        {
            return forwarded.to_owned();
        }
    }

    socket_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".to_owned())
}

fn parse_forwarded_for(value: &str) -> Option<&str> {
    value
        .split(',')
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
}

#[derive(Debug)]
struct KeyedRateLimiter {
    config: RateLimitConfig,
    refill_per_second: f64,
    entry_ttl: Duration,
    buckets: Arc<Mutex<HashMap<String, BucketState>>>,
}

impl KeyedRateLimiter {
    fn new(config: RateLimitConfig) -> Self {
        let refill_per_second = config.requests_per_minute as f64 / 60.0;
        let ttl_seconds = ((60 * ENTRY_TTL_MULTIPLIER) / config.requests_per_minute.max(1)).max(60);
        Self {
            config,
            refill_per_second,
            entry_ttl: Duration::from_secs(ttl_seconds as u64),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().await;
        buckets.retain(|_, state| now.duration_since(state.last_seen) <= self.entry_ttl);

        let bucket = buckets
            .entry(key.to_owned())
            .or_insert_with(|| BucketState {
                tokens: self.config.burst as f64,
                last_refill: now,
                last_seen: now,
            });
        refill(bucket, now, self.refill_per_second, self.config.burst);
        bucket.last_seen = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last_refill: Instant,
    last_seen: Instant,
}

fn refill(bucket: &mut BucketState, now: Instant, refill_per_second: f64, burst: u32) {
    let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
    if elapsed <= 0.0 {
        return;
    }

    bucket.tokens = (bucket.tokens + elapsed * refill_per_second).min(burst as f64);
    bucket.last_refill = now;
}

fn masked_v4(ip: Ipv4Addr, prefix: u8) -> u32 {
    let raw = u32::from(ip);
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    raw & mask
}

fn masked_v6(ip: Ipv6Addr, prefix: u8) -> u128 {
    let raw = u128::from(ip);
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    raw & mask
}

#[cfg(test)]
mod tests {
    use axum::http::Request;

    use super::*;

    #[test]
    fn falls_back_to_socket_address_when_proxy_headers_disabled() {
        let mut request = Request::builder().uri("/").body(()).unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "10.0.0.2:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let key = client_key(request.headers(), &request, ClientIpMode::Socket, &[]);
        assert_eq!(key, "10.0.0.2");
    }

    #[test]
    fn trusts_leftmost_forwarded_for_when_enabled() {
        let mut request = Request::builder()
            .uri("/")
            .header("x-forwarded-for", "198.51.100.7, 10.0.0.2")
            .body(())
            .unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "10.0.0.2:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let trusted = [TrustedProxyNet::from_str("10.0.0.0/8").unwrap()];
        let key = client_key(
            request.headers(),
            &request,
            ClientIpMode::XForwardedFor,
            &trusted,
        );
        assert_eq!(key, "198.51.100.7");
    }

    #[test]
    fn falls_back_to_socket_address_when_forwarded_header_missing() {
        let mut request = Request::builder().uri("/").body(()).unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "10.0.0.3:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let trusted = [TrustedProxyNet::from_str("10.0.0.0/8").unwrap()];
        let key = client_key(
            request.headers(),
            &request,
            ClientIpMode::XForwardedFor,
            &trusted,
        );
        assert_eq!(key, "10.0.0.3");
    }

    #[test]
    fn falls_back_to_socket_address_when_forwarded_header_is_blank() {
        let mut request = Request::builder()
            .uri("/")
            .header("x-forwarded-for", "  , ")
            .body(())
            .unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "10.0.0.4:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let trusted = [TrustedProxyNet::from_str("10.0.0.0/8").unwrap()];
        let key = client_key(
            request.headers(),
            &request,
            ClientIpMode::XForwardedFor,
            &trusted,
        );
        assert_eq!(key, "10.0.0.4");
    }

    #[test]
    fn ignores_forwarded_header_from_untrusted_proxy() {
        let mut request = Request::builder()
            .uri("/")
            .header("x-forwarded-for", "198.51.100.7")
            .body(())
            .unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            "10.0.0.4:1234".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let trusted = [TrustedProxyNet::from_str("127.0.0.0/8").unwrap()];
        let key = client_key(
            request.headers(),
            &request,
            ClientIpMode::XForwardedFor,
            &trusted,
        );
        assert_eq!(key, "10.0.0.4");
    }

    #[tokio::test]
    async fn limiter_refills_after_waiting() {
        let limiter = KeyedRateLimiter::new(RateLimitConfig {
            requests_per_minute: 60,
            burst: 2,
        });
        assert!(limiter.allow("client").await);
        assert!(limiter.allow("client").await);
        assert!(!limiter.allow("client").await);

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(limiter.allow("client").await);
    }
}
