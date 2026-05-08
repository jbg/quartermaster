//! Minimal OpenFoodFacts v2 API client.
//!
//! We only touch the product lookup endpoint and we only pull the fields the
//! rest of Quartermaster cares about. Heavy lifting (caching, TTL, fallback to
//! manual entry) lives in the products route handler, not here.

use std::{
    collections::HashMap,
    sync::Arc,
    sync::OnceLock,
    time::{Duration, Instant},
};

use backon::{BackoffBuilder, ExponentialBuilder};
use metrics::counter;
use qm_core::units::UnitFamily;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::ApiConfig;

const FIELDS: &str =
    "code,product_name,product_name_en,brands,image_url,product_quantity,product_quantity_unit";
const MOCK_OFF_BASE_URL_PREFIX: &str = "mock://off/";

type MockOffHits = Arc<Mutex<HashMap<String, usize>>>;

static MOCK_OFF_SESSIONS: OnceLock<Mutex<HashMap<String, MockOffHits>>> = OnceLock::new();

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
    /// Numeric amount in `quantity_unit` for one retail package, as OFF reports it.
    pub quantity: Option<String>,
    /// Raw unit string as OFF reports it, for family inference by the caller.
    pub quantity_unit: Option<String>,
}

#[derive(Debug)]
pub enum OffResult {
    Found(OffProduct),
    NotFound,
    Upstream(String),
}

#[derive(Debug, Clone)]
pub struct OffWriteCredentials {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct OffContributionForm {
    pub barcode: String,
    pub product_name: Option<String>,
    pub brands: Option<Option<String>>,
    pub product_quantity: Option<String>,
    pub product_quantity_unit: Option<String>,
    pub app_uuid: String,
}

#[derive(Debug)]
pub enum OffWriteResult {
    Saved { status_verbose: String },
    AuthFailed,
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
                counter!("qm_off_lookups_total", "outcome" => "circuit_breaker_open").increment(1);
                return OffResult::Upstream("circuit breaker open".into());
            }
        };

        let mut attempts = 0usize;
        let retry_budget = self
            .config
            .off_timeout
            .mul_f64((self.config.off_max_retries + 1) as f64 + 1.0);
        let mut backoff = ExponentialBuilder::new()
            .with_min_delay(self.config.off_retry_base_delay)
            .with_factor(2.0)
            .with_jitter()
            .with_total_delay(Some(retry_budget))
            .with_max_times(self.config.off_max_retries as usize)
            .build();
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
                        if let Some(delay) = backoff.next() {
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
                counter!("qm_off_lookups_total", "outcome" => "found").increment(1);
                OffResult::Found(product)
            }
            Ok(FetchSuccess::NotFound) => {
                self.breaker.record_non_transient_success(permit).await;
                info!(%barcode, attempt_count = attempts, breaker_state = "closed", outcome = "not_found", "OFF lookup finished");
                counter!("qm_off_lookups_total", "outcome" => "not_found").increment(1);
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
                counter!("qm_off_lookups_total", "outcome" => "permanent_error").increment(1);
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
                counter!("qm_off_lookups_total", "outcome" => "transient_error").increment(1);
                OffResult::Upstream(err)
            }
        }
    }

    async fn fetch_once(&self, barcode: &str) -> FetchOutcome {
        if let Some(session_id) = self
            .config
            .off_api_base_url
            .strip_prefix(MOCK_OFF_BASE_URL_PREFIX)
        {
            return mock_fetch_once(session_id, barcode).await;
        }

        let url = format!(
            "{}/{barcode}.json?fields={FIELDS}",
            self.config.off_api_base_url
        );
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
            quantity: product.product_quantity.and_then(normalize_quantity),
            quantity_unit: product
                .product_quantity_unit
                .filter(|s| !s.trim().is_empty()),
        })
    }

    pub async fn contribute(
        &self,
        credentials: &OffWriteCredentials,
        form: &OffContributionForm,
    ) -> OffWriteResult {
        let mut params: Vec<(&str, String)> = vec![
            ("code", form.barcode.clone()),
            ("user_id", credentials.username.clone()),
            ("password", credentials.password.clone()),
            ("app_name", "Quartermaster".to_owned()),
            ("app_version", env!("CARGO_PKG_VERSION").to_owned()),
            ("app_uuid", form.app_uuid.clone()),
        ];
        if let Some(value) = &form.product_name {
            params.push(("product_name", value.clone()));
        }
        if let Some(value) = &form.brands {
            params.push(("brands", value.clone().unwrap_or_default()));
        }
        if let Some(value) = &form.product_quantity {
            params.push(("product_quantity", value.clone()));
        }
        if let Some(value) = &form.product_quantity_unit {
            params.push(("product_quantity_unit", value.clone()));
        }

        let response = match self
            .http
            .post(&self.config.off_write_url)
            .form(&params)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                warn!(?err, "OFF contribution request failed");
                return OffWriteResult::Upstream(err.to_string());
            }
        };

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return OffWriteResult::AuthFailed;
        }
        if status == StatusCode::NOT_FOUND {
            return OffWriteResult::NotFound;
        }
        if status == StatusCode::REQUEST_TIMEOUT
            || status == StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error()
        {
            return OffWriteResult::Upstream(format!("OFF returned {status}"));
        }
        if !status.is_success() {
            return OffWriteResult::Upstream(format!("OFF returned {status}"));
        }

        let payload: OffWriteResponse = match response.json().await {
            Ok(payload) => payload,
            Err(err) => return OffWriteResult::Upstream(err.to_string()),
        };
        if payload.status == Some(1) {
            OffWriteResult::Saved {
                status_verbose: payload
                    .status_verbose
                    .unwrap_or_else(|| "fields saved".into()),
            }
        } else {
            let message = payload
                .status_verbose
                .unwrap_or_else(|| "OFF write failed".into());
            if message.to_ascii_lowercase().contains("user")
                || message.to_ascii_lowercase().contains("password")
                || message.to_ascii_lowercase().contains("auth")
            {
                OffWriteResult::AuthFailed
            } else {
                OffWriteResult::Upstream(message)
            }
        }
    }
}

pub fn app_uuid_for_user(user_id: Uuid) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(user_id.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}

fn mock_off_sessions() -> &'static Mutex<HashMap<String, MockOffHits>> {
    MOCK_OFF_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn register_mock_session(session_id: &str) {
    mock_off_sessions()
        .lock()
        .await
        .insert(session_id.to_owned(), Arc::new(Mutex::new(HashMap::new())));
}

pub async fn unregister_mock_session(session_id: &str) {
    mock_off_sessions().lock().await.remove(session_id);
}

pub async fn mock_session_hit_count(session_id: &str, barcode: &str) -> usize {
    let hits = {
        let sessions = mock_off_sessions().lock().await;
        sessions.get(session_id).cloned()
    };
    let Some(hits) = hits else {
        return 0;
    };
    let count = hits.lock().await.get(barcode).copied().unwrap_or_default();
    count
}

async fn mock_fetch_once(session_id: &str, barcode: &str) -> FetchOutcome {
    let hits = {
        let sessions = mock_off_sessions().lock().await;
        sessions.get(session_id).cloned()
    };
    let Some(hits) = hits else {
        return FetchOutcome::PermanentUpstream("mock OFF session missing".into());
    };

    let attempt = {
        let mut hits = hits.lock().await;
        let count = hits.entry(barcode.to_owned()).or_insert(0);
        *count += 1;
        *count
    };

    match barcode {
        "1111111111111" if attempt < 3 => {
            FetchOutcome::TransientUpstream("OFF returned 503 Service Unavailable".into())
        }
        "1111111111111" => {
            let payload = json!({
                "code": barcode,
                "status": 1,
                "product": {
                    "product_name": "Retry Beans",
                    "brands": "Acme",
                    "image_url": Value::Null,
                    "product_quantity": "400",
                    "product_quantity_unit": "g",
                }
            });
            let product: OffResponse = serde_json::from_value(payload).expect("mock OFF payload");
            decode_mock_payload(barcode, product)
        }
        "2222222222222" => FetchOutcome::NotFound,
        "3333333333333" => {
            FetchOutcome::TransientUpstream("OFF returned 503 Service Unavailable".into())
        }
        "4444444444444" => {
            let payload = json!({
                "code": barcode,
                "status": 1,
                "product": {
                    "product_name": "Big Orange Juice",
                    "brands": "Acme",
                    "image_url": Value::Null,
                    "product_quantity": "1",
                    "product_quantity_unit": "bottle",
                }
            });
            let product: OffResponse = serde_json::from_value(payload).expect("mock OFF payload");
            decode_mock_payload(barcode, product)
        }
        _ => FetchOutcome::NotFound,
    }
}

fn decode_mock_payload(barcode: &str, payload: OffResponse) -> FetchOutcome {
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
        quantity: product.product_quantity.and_then(normalize_quantity),
        quantity_unit: product
            .product_quantity_unit
            .filter(|s| !s.trim().is_empty()),
    })
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
    let Some(raw) = hint else {
        return UnitFamily::Count;
    };
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

pub fn normalize_package(quantity: Option<&str>, unit: Option<&str>) -> Option<(String, String)> {
    let quantity = quantity?.trim();
    let unit = unit?.trim().to_ascii_lowercase();
    if quantity.is_empty() || unit.is_empty() {
        return None;
    }
    let normalized_quantity = normalize_quantity(Value::String(quantity.to_owned()))?;
    let unit = normalize_unit(&unit)?;
    Some((normalized_quantity, unit))
}

fn normalize_quantity(value: Value) -> Option<String> {
    let raw = match value {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.trim().replace(',', "."),
        _ => return None,
    };
    let decimal = raw.parse::<rust_decimal::Decimal>().ok()?;
    if decimal <= rust_decimal::Decimal::ZERO {
        return None;
    }
    Some(decimal.normalize().to_string())
}

fn normalize_unit(raw: &str) -> Option<String> {
    if let Ok(unit) = qm_core::units::lookup(raw) {
        return Some(unit.code.to_owned());
    }
    match raw {
        "grams" | "gram" => Some("g".into()),
        "kilograms" | "kilogram" => Some("kg".into()),
        "ounces" | "ounce" => Some("oz".into()),
        "pounds" | "pound" => Some("lb".into()),
        "milliliters" | "milliliter" | "millilitre" | "millilitres" => Some("ml".into()),
        "liters" | "liter" | "litre" | "litres" => Some("l".into()),
        "pieces" | "units" | "unit" | "ct" | "count" => Some("piece".into()),
        _ => None,
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
    product_quantity: Option<Value>,
    product_quantity_unit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OffWriteResponse {
    status: Option<i64>,
    status_verbose: Option<String>,
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

    #[test]
    fn normalize_package_keeps_known_positive_quantities() {
        assert_eq!(
            normalize_package(Some("400.0"), Some("grams")),
            Some(("400".into(), "g".into()))
        );
        assert_eq!(
            normalize_package(Some("1,5"), Some("LITRE")),
            Some(("1.5".into(), "l".into()))
        );
    }

    #[test]
    fn normalize_package_rejects_unknown_or_empty_values() {
        assert_eq!(normalize_package(Some("0"), Some("g")), None);
        assert_eq!(normalize_package(Some("400"), Some("can")), None);
        assert_eq!(normalize_package(None, Some("g")), None);
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
