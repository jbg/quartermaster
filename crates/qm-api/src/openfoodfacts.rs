//! Minimal OpenFoodFacts v2 API client.
//!
//! We only touch the product lookup endpoint and we only pull the fields the
//! rest of Quartermaster cares about. Heavy lifting (caching, TTL, fallback to
//! manual entry) lives in the products route handler, not here.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use backoff::{backoff::Backoff, ExponentialBackoffBuilder};
use qm_core::units::UnitFamily;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::ApiConfig;

const FIELDS: &str = "code,product_name,product_name_en,brands,image_url,product_quantity_unit";

#[derive(Debug, Clone)]
pub struct OpenFoodFactsClient {
    http: Client,
    breaker: Arc<OffCircuitBreaker>,
    config: Arc<ApiConfig>,
}

#[derive(Debug, Clone)]
pub struct OffProduct {
    pub barcode: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    /// Raw unit string as OFF reports it, for family inference by the caller.
    pub quantity_unit: Option<String>,
}

#[derive(Debug)]
pub enum OffResult {
    Found(OffProduct),
    NotFound,
    Upstream(String),
}

impl OpenFoodFactsClient {
    pub fn new(http: Client, breaker: Arc<OffCircuitBreaker>, config: Arc<ApiConfig>) -> Self {
        Self {
            http,
            breaker,
            config,
        }
    }

    pub async fn fetch(&self, barcode: &str) -> OffResult {
        let permit = match self.breaker.acquire().await {
            Some(permit) => permit,
            None => {
                warn!(%barcode, breaker_state = "open", "OFF circuit breaker is open");
                return OffResult::Upstream("circuit breaker open".into());
            }
        };

        let mut attempts = 0usize;
        let retry_budget = self.config.off_timeout.mul_f64((self.config.off_max_retries + 1) as f64 + 1.0);
        let policy = ExponentialBackoffBuilder::new()
            .with_initial_interval(self.config.off_retry_base_delay)
            .with_randomization_factor(0.5)
            .with_multiplier(2.0)
            .with_max_elapsed_time(Some(retry_budget))
            .build();

        let mut backoff = policy;
        let result = loop {
            attempts += 1;
            debug!(%barcode, attempt = attempts, "OFF lookup attempt");
            match self.fetch_once(barcode).await {
                FetchOutcome::Found(product) => break Ok(FetchSuccess::Found(product)),
                FetchOutcome::NotFound => break Ok(FetchSuccess::NotFound),
                FetchOutcome::PermanentUpstream(message) => {
                    break Err(FetchError {
                        message,
                        transient: false,
                    });
                }
                FetchOutcome::TransientUpstream(message) => {
                    let should_retry = attempts <= self.config.off_max_retries as usize;
                    if should_retry {
                        if let Some(delay) = backoff.next_backoff() {
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                    }
                    break Err(FetchError {
                        message,
                        transient: true,
                    });
                }
            }
        };

        match result {
            Ok(FetchSuccess::Found(product)) => {
                self.breaker.record_non_transient_success(permit).await;
                info!(%barcode, attempt_count = attempts, breaker_state = "closed", outcome = "found", "OFF lookup succeeded");
                OffResult::Found(product)
            }
            Ok(FetchSuccess::NotFound) => {
                self.breaker.record_non_transient_success(permit).await;
                info!(%barcode, attempt_count = attempts, breaker_state = "closed", outcome = "not_found", "OFF lookup finished");
                OffResult::NotFound
            }
            Err(FetchError {
                message,
                transient: false,
            }) => {
                self.breaker.record_non_transient_success(permit).await;
                warn!(
                    %barcode,
                    attempt_count = attempts,
                    breaker_state = "closed",
                    outcome = "permanent_error",
                    error = %message,
                    "OFF lookup failed without retry"
                );
                OffResult::Upstream(message)
            }
            Err(FetchError {
                message: err,
                transient: true,
            }) => {
                let breaker_state = self
                    .breaker
                    .record_transient_failure(
                        permit,
                        self.config.off_circuit_breaker_failure_threshold,
                        self.config.off_circuit_breaker_open_for,
                    )
                    .await;
                warn!(
                    %barcode,
                    attempt_count = attempts,
                    breaker_state = %breaker_state.as_str(),
                    outcome = "transient_error",
                    error = %err,
                    "OFF lookup exhausted retries"
                );
                OffResult::Upstream(err)
            }
        }
    }

    async fn fetch_once(&self, barcode: &str) -> FetchOutcome {
        let url = format!("{}/{barcode}.json?fields={FIELDS}", self.config.off_api_base_url);
        debug!(%url, "OFF lookup");

        let response = match self.http.get(&url).send().await {
            Ok(r) => r,
            Err(err) => {
                warn!(%barcode, ?err, "OFF request failed");
                return FetchOutcome::TransientUpstream(err.to_string());
            }
        };

        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return FetchOutcome::NotFound;
        }
        if status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
        {
            warn!(%barcode, %status, "OFF transient response");
            return FetchOutcome::TransientUpstream(format!("OFF returned {status}"));
        }
        if !status.is_success() {
            warn!(%barcode, %status, "OFF non-success response");
            return FetchOutcome::PermanentUpstream(format!("OFF returned {status}"));
        }

        let payload: OffResponse = match response.json().await {
            Ok(p) => p,
            Err(err) => {
                warn!(%barcode, ?err, "OFF payload decode failed");
                return FetchOutcome::PermanentUpstream(err.to_string());
            }
        };

        if payload.status != 1 {
            return FetchOutcome::NotFound;
        }
        let Some(product) = payload.product else {
            return FetchOutcome::NotFound;
        };

        let name = product
            .product_name_en
            .filter(|s| !s.trim().is_empty())
            .or(product.product_name)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| format!("Barcode {barcode}"));

        FetchOutcome::Found(OffProduct {
            barcode: barcode.to_owned(),
            name,
            brand: product.brands.filter(|s| !s.trim().is_empty()),
            image_url: product.image_url.filter(|s| !s.trim().is_empty()),
            quantity_unit: product.product_quantity_unit.filter(|s| !s.trim().is_empty()),
        })
    }
}

#[derive(Debug)]
enum FetchSuccess {
    Found(OffProduct),
    NotFound,
}

#[derive(Debug)]
enum FetchOutcome {
    Found(OffProduct),
    NotFound,
    PermanentUpstream(String),
    TransientUpstream(String),
}

#[derive(Debug)]
struct FetchError {
    message: String,
    transient: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreakerStateName {
    Closed,
    Open,
    HalfOpen,
}

impl BreakerStateName {
    pub fn as_str(self) -> &'static str {
        match self {
            BreakerStateName::Closed => "closed",
            BreakerStateName::Open => "open",
            BreakerStateName::HalfOpen => "half_open",
        }
    }
}

#[derive(Debug, Default)]
pub struct OffCircuitBreaker {
    state: Mutex<BreakerState>,
}

#[derive(Debug)]
struct BreakerState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
    probe_in_flight: bool,
}

impl Default for BreakerState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            open_until: None,
            probe_in_flight: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BreakerPermit {
    Closed,
    HalfOpenProbe,
}

impl OffCircuitBreaker {
    pub(crate) async fn acquire(&self) -> Option<BreakerPermit> {
        let mut state = self.state.lock().await;
        let now = Instant::now();

        if let Some(open_until) = state.open_until {
            if now < open_until {
                return None;
            }
            if state.probe_in_flight {
                return None;
            }
            state.probe_in_flight = true;
            return Some(BreakerPermit::HalfOpenProbe);
        }

        Some(BreakerPermit::Closed)
    }

    pub(crate) async fn record_non_transient_success(&self, permit: BreakerPermit) {
        let mut state = self.state.lock().await;
        state.consecutive_failures = 0;
        state.open_until = None;
        if matches!(permit, BreakerPermit::HalfOpenProbe) {
            state.probe_in_flight = false;
        }
    }

    pub(crate) async fn record_transient_failure(
        &self,
        permit: BreakerPermit,
        threshold: u32,
        open_for: Duration,
    ) -> BreakerStateName {
        let mut state = self.state.lock().await;
        let should_open = match permit {
            BreakerPermit::HalfOpenProbe => true,
            BreakerPermit::Closed => {
                state.consecutive_failures += 1;
                state.consecutive_failures >= threshold
            }
        };

        if should_open {
            state.consecutive_failures = 0;
            state.open_until = Some(Instant::now() + open_for);
            state.probe_in_flight = false;
            BreakerStateName::Open
        } else {
            BreakerStateName::Closed
        }
    }
}

/// Map an OFF unit hint to a Quartermaster unit family. Unknown or missing
/// hints fall back to `Count` — the least-wrong default given that a mass or
/// volume guess would be silently wrong.
pub fn infer_family(hint: Option<&str>) -> UnitFamily {
    let Some(raw) = hint else { return UnitFamily::Count };
    let lowered = raw.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return UnitFamily::Count;
    }
    if let Ok(u) = qm_core::units::lookup(&lowered) {
        return u.family;
    }
    // A few OFF conventions that don't round-trip cleanly through our unit
    // table (plural, long-form). Normalise the common ones.
    match lowered.as_str() {
        "grams" | "gram" => UnitFamily::Mass,
        "kilograms" | "kilogram" => UnitFamily::Mass,
        "ounces" | "ounce" => UnitFamily::Mass,
        "pounds" | "pound" => UnitFamily::Mass,
        "milliliters" | "milliliter" | "millilitre" | "millilitres" => UnitFamily::Volume,
        "liters" | "liter" | "litre" | "litres" => UnitFamily::Volume,
        "pieces" | "piece" | "units" | "unit" | "ct" | "count" => UnitFamily::Count,
        _ => UnitFamily::Count,
    }
}

#[derive(Debug, Deserialize)]
struct OffResponse {
    #[serde(default)]
    status: i64,
    #[serde(default)]
    product: Option<OffInnerProduct>,
}

#[derive(Debug, Deserialize)]
struct OffInnerProduct {
    product_name: Option<String>,
    product_name_en: Option<String>,
    brands: Option<String>,
    image_url: Option<String>,
    product_quantity_unit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_family_from_known_units() {
        assert_eq!(infer_family(Some("g")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("kg")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("ml")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("l")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("piece")), UnitFamily::Count);
    }

    #[test]
    fn infer_family_from_long_forms() {
        assert_eq!(infer_family(Some("grams")), UnitFamily::Mass);
        assert_eq!(infer_family(Some("Milliliters")), UnitFamily::Volume);
        assert_eq!(infer_family(Some("units")), UnitFamily::Count);
    }

    #[test]
    fn infer_family_falls_back_to_count() {
        assert_eq!(infer_family(None), UnitFamily::Count);
        assert_eq!(infer_family(Some("")), UnitFamily::Count);
        assert_eq!(infer_family(Some("  ")), UnitFamily::Count);
        assert_eq!(infer_family(Some("whatever")), UnitFamily::Count);
    }

    #[tokio::test]
    async fn breaker_opens_after_threshold_and_recovers_after_probe() {
        let breaker = OffCircuitBreaker::default();
        let permit = breaker.acquire().await.unwrap();
        let state = breaker
            .record_transient_failure(permit, 1, Duration::from_millis(20))
            .await;
        assert_eq!(state, BreakerStateName::Open);
        assert!(breaker.acquire().await.is_none());

        tokio::time::sleep(Duration::from_millis(25)).await;
        let permit = breaker.acquire().await.unwrap();
        breaker.record_non_transient_success(permit).await;
        assert!(breaker.acquire().await.is_some());
    }

    #[tokio::test]
    async fn breaker_stays_closed_below_threshold() {
        let breaker = OffCircuitBreaker::default();
        let permit = breaker.acquire().await.unwrap();
        let state = breaker
            .record_transient_failure(permit, 3, Duration::from_secs(1))
            .await;
        assert_eq!(state, BreakerStateName::Closed);
        assert!(breaker.acquire().await.is_some());
    }
}
