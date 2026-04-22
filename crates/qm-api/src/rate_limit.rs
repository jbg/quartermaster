use std::{
    collections::HashMap,
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
        state.app_state.config.trust_proxy_headers,
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
    trust_proxy_headers: bool,
) -> String {
    if trust_proxy_headers {
        if let Some(forwarded) = headers
            .get(&X_FORWARDED_FOR)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_forwarded_for)
        {
            return forwarded.to_owned();
        }
    }

    request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|info| info.0.ip().to_string())
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
        let key = client_key(request.headers(), &request, false);
        assert_eq!(key, "10.0.0.2");
    }

    #[test]
    fn trusts_leftmost_forwarded_for_when_enabled() {
        let request = Request::builder()
            .uri("/")
            .header("x-forwarded-for", "198.51.100.7, 10.0.0.2")
            .body(())
            .unwrap();
        let key = client_key(request.headers(), &request, true);
        assert_eq!(key, "198.51.100.7");
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
