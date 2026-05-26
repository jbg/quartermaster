//! Supplier integration primitives for Quartermaster.
//!
//! This crate is intentionally provider-neutral. In-tree integrations implement
//! this trait today; the same contract can later shape an out-of-process plugin
//! boundary without letting supplier credentials leak into app feature code.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

pub type SupplierIntegrationRef = Arc<dyn SupplierIntegration>;
pub type SupplierFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SupplierError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupplierId(pub String);

impl SupplierId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierCapability {
    CatalogSearch,
    ItemDetail,
    CartDraft,
    OrderSubmit,
    OrderStatus,
    Cancellation,
    ReceivingHints,
    BrowserAutomation,
    ManualHandoff,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierRequirementKind {
    Secret,
    Configuration,
    Consent,
    BrowserSession,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierRequirement {
    pub name: String,
    pub kind: SupplierRequirementKind,
    pub required: bool,
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierRegion {
    pub country_code: String,
    pub region_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SupplierDescriptor {
    pub id: SupplierId,
    pub display_name: String,
    pub capabilities: Vec<SupplierCapability>,
    pub requirements: Vec<SupplierRequirement>,
    pub supported_regions: Vec<SupplierRegion>,
    pub terms_url: Option<String>,
    pub needs_network: bool,
    pub needs_browser: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogSearchRequest {
    pub query: String,
    pub region: Option<SupplierRegion>,
    pub limit: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogSearchResponse {
    pub items: Vec<CatalogItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogItem {
    pub supplier_id: SupplierId,
    pub supplier_item_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub image_url: Option<String>,
    pub detail_url: Option<String>,
    pub availability: Availability,
    pub price: Option<PriceQuote>,
    pub pack_size: Option<PackSize>,
    pub lead_time: Option<LeadTime>,
    pub minimum_order_quantity: Option<MinimumOrderQuantity>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    InStock,
    Limited,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PriceQuote {
    pub amount: String,
    pub currency: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackSize {
    pub quantity: String,
    pub unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadTime {
    pub min_days: i64,
    pub max_days: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MinimumOrderQuantity {
    pub quantity: String,
    pub unit: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartDraft {
    pub id: Uuid,
    pub supplier_id: SupplierId,
    pub lines: Vec<CartLine>,
    pub status: CartStatus,
    pub intervention: InterventionState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CartLine {
    pub supplier_item_id: String,
    pub product_id: Option<Uuid>,
    pub quantity: String,
    pub unit: Option<String>,
    pub note: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CartStatus {
    Draft,
    NeedsReview,
    Ready,
    Submitted,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionState {
    None,
    ConsentRequired,
    LoginRequired,
    BrowserHandoffRequired,
    ManualHandoffRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderSubmission {
    pub supplier_order_id: String,
    pub status: OrderStatus,
    pub review_url: Option<String>,
    pub raw_summary: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Draft,
    Submitted,
    Confirmed,
    InProgress,
    Delivered,
    Cancelled,
    Failed,
    HumanInterventionRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceivingHint {
    pub supplier_item_id: String,
    pub product_id: Option<Uuid>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub expires_on: Option<String>,
}

#[derive(Debug, Error)]
pub enum SupplierError {
    #[error("supplier is not configured")]
    NotConfigured,

    #[error("supplier capability is unavailable: {0:?}")]
    Unsupported(SupplierCapability),

    #[error("supplier needs human intervention: {0:?}")]
    InterventionRequired(InterventionState),

    #[error("supplier request timed out")]
    Timeout,

    #[error("supplier is rate limited")]
    RateLimited,

    #[error("supplier circuit breaker is open")]
    CircuitOpen,

    #[error("supplier returned a transient error: {message}")]
    Transient { message: String },

    #[error("supplier returned a permanent error: {message}")]
    Permanent { message: String },
}

impl SupplierError {
    pub fn redacted_message(&self) -> String {
        match self {
            Self::Transient { .. } => "supplier returned a transient error".into(),
            Self::Permanent { .. } => "supplier returned a permanent error".into(),
            other => other.to_string(),
        }
    }
}

pub trait SupplierIntegration: std::fmt::Debug + Send + Sync {
    fn descriptor(&self) -> SupplierDescriptor;

    fn search_catalog<'a>(
        &'a self,
        request: CatalogSearchRequest,
    ) -> SupplierFuture<'a, CatalogSearchResponse>;

    fn item_detail<'a>(&'a self, supplier_item_id: &'a str) -> SupplierFuture<'a, CatalogItem>;

    fn validate_cart<'a>(&'a self, draft: CartDraft) -> SupplierFuture<'a, CartDraft>;

    fn submit_order<'a>(&'a self, draft: CartDraft) -> SupplierFuture<'a, OrderSubmission>;

    fn order_status<'a>(&'a self, supplier_order_id: &'a str) -> SupplierFuture<'a, OrderStatus>;

    fn cancel_order<'a>(&'a self, supplier_order_id: &'a str) -> SupplierFuture<'a, OrderStatus>;

    fn receiving_hints<'a>(
        &'a self,
        supplier_order_id: &'a str,
    ) -> SupplierFuture<'a, Vec<ReceivingHint>>;
}

#[derive(Debug)]
pub struct MockSupplierIntegration {
    descriptor: SupplierDescriptor,
    catalog: Vec<CatalogItem>,
    failures: Mutex<HashMap<String, SupplierError>>,
    orders: Mutex<HashMap<String, OrderStatus>>,
}

impl MockSupplierIntegration {
    pub fn demo() -> Self {
        Self::new(vec![
            CatalogItem {
                supplier_id: SupplierId::new("mock"),
                supplier_item_id: "mock-rice-1kg".into(),
                name: "Mock Long Grain Rice".into(),
                brand: Some("Quartermaster Test Kitchen".into()),
                image_url: None,
                detail_url: Some("https://example.invalid/mock-rice-1kg".into()),
                availability: Availability::InStock,
                price: Some(PriceQuote {
                    amount: "3.49".into(),
                    currency: "USD".into(),
                }),
                pack_size: Some(PackSize {
                    quantity: "1000".into(),
                    unit: "g".into(),
                }),
                lead_time: Some(LeadTime {
                    min_days: 1,
                    max_days: Some(2),
                }),
                minimum_order_quantity: Some(MinimumOrderQuantity {
                    quantity: "1".into(),
                    unit: "piece".into(),
                }),
                metadata: Value::Null,
            },
            CatalogItem {
                supplier_id: SupplierId::new("mock"),
                supplier_item_id: "mock-beans-4pk".into(),
                name: "Mock Beans Four Pack".into(),
                brand: Some("Quartermaster Test Kitchen".into()),
                image_url: None,
                detail_url: Some("https://example.invalid/mock-beans-4pk".into()),
                availability: Availability::Limited,
                price: Some(PriceQuote {
                    amount: "5.25".into(),
                    currency: "USD".into(),
                }),
                pack_size: Some(PackSize {
                    quantity: "4".into(),
                    unit: "piece".into(),
                }),
                lead_time: Some(LeadTime {
                    min_days: 2,
                    max_days: Some(3),
                }),
                minimum_order_quantity: Some(MinimumOrderQuantity {
                    quantity: "1".into(),
                    unit: "piece".into(),
                }),
                metadata: Value::Null,
            },
        ])
    }

    pub fn new(catalog: Vec<CatalogItem>) -> Self {
        Self {
            descriptor: SupplierDescriptor {
                id: SupplierId::new("mock"),
                display_name: "Mock Supplier".into(),
                capabilities: vec![
                    SupplierCapability::CatalogSearch,
                    SupplierCapability::ItemDetail,
                    SupplierCapability::CartDraft,
                    SupplierCapability::OrderSubmit,
                    SupplierCapability::OrderStatus,
                    SupplierCapability::Cancellation,
                    SupplierCapability::ReceivingHints,
                    SupplierCapability::ManualHandoff,
                ],
                requirements: vec![SupplierRequirement {
                    name: "api_token".into(),
                    kind: SupplierRequirementKind::Secret,
                    required: false,
                    description: Some("Optional mock token used by integration tests.".into()),
                }],
                supported_regions: vec![SupplierRegion {
                    country_code: "US".into(),
                    region_code: None,
                }],
                terms_url: Some("https://example.invalid/mock-supplier-terms".into()),
                needs_network: false,
                needs_browser: false,
            },
            catalog,
            failures: Mutex::new(HashMap::new()),
            orders: Mutex::new(HashMap::new()),
        }
    }

    pub async fn fail_next(&self, operation: &str, error: SupplierError) {
        self.failures.lock().await.insert(operation.into(), error);
    }

    async fn take_failure(&self, operation: &str) -> Result<(), SupplierError> {
        if let Some(error) = self.failures.lock().await.remove(operation) {
            Err(error)
        } else {
            Ok(())
        }
    }
}

impl SupplierIntegration for MockSupplierIntegration {
    fn descriptor(&self) -> SupplierDescriptor {
        self.descriptor.clone()
    }

    fn search_catalog<'a>(
        &'a self,
        request: CatalogSearchRequest,
    ) -> SupplierFuture<'a, CatalogSearchResponse> {
        Box::pin(async move {
            self.take_failure("search_catalog").await?;
            let query = request.query.trim().to_ascii_lowercase();
            let limit = request.limit.clamp(1, 100);
            let items = self
                .catalog
                .iter()
                .filter(|item| {
                    query.is_empty()
                        || item.name.to_ascii_lowercase().contains(&query)
                        || item
                            .brand
                            .as_ref()
                            .is_some_and(|brand| brand.to_ascii_lowercase().contains(&query))
                })
                .take(limit)
                .cloned()
                .collect();
            Ok(CatalogSearchResponse { items })
        })
    }

    fn item_detail<'a>(&'a self, supplier_item_id: &'a str) -> SupplierFuture<'a, CatalogItem> {
        Box::pin(async move {
            self.take_failure("item_detail").await?;
            self.catalog
                .iter()
                .find(|item| item.supplier_item_id == supplier_item_id)
                .cloned()
                .ok_or_else(|| SupplierError::Permanent {
                    message: "catalog item not found".into(),
                })
        })
    }

    fn validate_cart<'a>(&'a self, mut draft: CartDraft) -> SupplierFuture<'a, CartDraft> {
        Box::pin(async move {
            self.take_failure("validate_cart").await?;
            let item_ids = self
                .catalog
                .iter()
                .map(|item| item.supplier_item_id.as_str())
                .collect::<HashSet<_>>();
            if draft
                .lines
                .iter()
                .any(|line| !item_ids.contains(line.supplier_item_id.as_str()))
            {
                draft.status = CartStatus::NeedsReview;
                draft.intervention = InterventionState::ManualHandoffRequired;
            } else {
                draft.status = CartStatus::Ready;
                draft.intervention = InterventionState::None;
            }
            Ok(draft)
        })
    }

    fn submit_order<'a>(&'a self, draft: CartDraft) -> SupplierFuture<'a, OrderSubmission> {
        Box::pin(async move {
            self.take_failure("submit_order").await?;
            if draft.intervention != InterventionState::None {
                return Err(SupplierError::InterventionRequired(draft.intervention));
            }
            let supplier_order_id = format!("mock-order-{}", Uuid::now_v7());
            self.orders
                .lock()
                .await
                .insert(supplier_order_id.clone(), OrderStatus::Submitted);
            Ok(OrderSubmission {
                supplier_order_id,
                status: OrderStatus::Submitted,
                review_url: None,
                raw_summary: serde_json::json!({
                    "line_count": draft.lines.len(),
                    "redacted": true
                }),
            })
        })
    }

    fn order_status<'a>(&'a self, supplier_order_id: &'a str) -> SupplierFuture<'a, OrderStatus> {
        Box::pin(async move {
            self.take_failure("order_status").await?;
            Ok(*self
                .orders
                .lock()
                .await
                .get(supplier_order_id)
                .unwrap_or(&OrderStatus::HumanInterventionRequired))
        })
    }

    fn cancel_order<'a>(&'a self, supplier_order_id: &'a str) -> SupplierFuture<'a, OrderStatus> {
        Box::pin(async move {
            self.take_failure("cancel_order").await?;
            self.orders
                .lock()
                .await
                .insert(supplier_order_id.into(), OrderStatus::Cancelled);
            Ok(OrderStatus::Cancelled)
        })
    }

    fn receiving_hints<'a>(
        &'a self,
        supplier_order_id: &'a str,
    ) -> SupplierFuture<'a, Vec<ReceivingHint>> {
        Box::pin(async move {
            self.take_failure("receiving_hints").await?;
            if !self.orders.lock().await.contains_key(supplier_order_id) {
                return Err(SupplierError::Permanent {
                    message: "order not found".into(),
                });
            }
            Ok(Vec::new())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_catalog_search_is_deterministic() {
        let supplier = MockSupplierIntegration::demo();
        let result = supplier
            .search_catalog(CatalogSearchRequest {
                query: "rice".into(),
                region: None,
                limit: 10,
            })
            .await
            .unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].supplier_item_id, "mock-rice-1kg");
    }

    #[tokio::test]
    async fn mock_cart_validation_requires_known_items() {
        let supplier = MockSupplierIntegration::demo();
        let draft = supplier
            .validate_cart(CartDraft {
                id: Uuid::now_v7(),
                supplier_id: SupplierId::new("mock"),
                lines: vec![CartLine {
                    supplier_item_id: "unknown".into(),
                    product_id: None,
                    quantity: "1".into(),
                    unit: Some("piece".into()),
                    note: None,
                }],
                status: CartStatus::Draft,
                intervention: InterventionState::None,
            })
            .await
            .unwrap();
        assert_eq!(draft.status, CartStatus::NeedsReview);
        assert_eq!(draft.intervention, InterventionState::ManualHandoffRequired);
    }
}
