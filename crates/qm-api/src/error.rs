use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("unknown unit: {0}")]
    UnknownUnit(String),

    #[error("unit {unit} does not belong to this product's family ({product_family})")]
    UnitFamilyMismatch {
        product_family: String,
        unit: String,
    },

    #[error("insufficient stock: requested {requested}, have {available}")]
    InsufficientStock { requested: String, available: String },

    #[error("this product has active stock (delete or consume it first)")]
    ProductHasStock,

    #[error(
        "this product has active stock with incompatible units for the new family: {conflicting_units:?}"
    )]
    ProductHasIncompatibleStock { conflicting_units: Vec<String> },

    #[error("OpenFoodFacts products are read-only from the client — refresh instead")]
    OffProductReadOnly,

    #[error("this action is only available on OpenFoodFacts-sourced products")]
    ManualProductNotRefreshable,

    #[error("this batch can't be restored — only discarded batches can be undone")]
    BatchNotRestorable,

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("not found")]
    NotFound,

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("registration is disabled")]
    RegistrationDisabled,

    #[error("upstream service unavailable")]
    BadGateway,

    #[error("database error")]
    Database(#[from] sqlx::Error),

    #[error("domain error: {0}")]
    Domain(#[from] qm_core::QmError),

    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorBody {
    /// Stable error code, machine-readable.
    pub code: &'static str,
    /// Human-readable description. Not localised; clients should prefer `code`.
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match &self {
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::UnknownUnit(_) => (StatusCode::BAD_REQUEST, "unknown_unit"),
            ApiError::UnitFamilyMismatch { .. } => (StatusCode::BAD_REQUEST, "unit_family_mismatch"),
            ApiError::InsufficientStock { .. } => (StatusCode::BAD_REQUEST, "insufficient_stock"),
            ApiError::ProductHasStock => (StatusCode::CONFLICT, "product_has_stock"),
            ApiError::ProductHasIncompatibleStock { .. } => {
                (StatusCode::CONFLICT, "product_has_incompatible_stock")
            }
            ApiError::OffProductReadOnly => (StatusCode::FORBIDDEN, "off_product_read_only"),
            ApiError::ManualProductNotRefreshable => {
                (StatusCode::BAD_REQUEST, "manual_product_not_refreshable")
            }
            ApiError::BatchNotRestorable => (StatusCode::CONFLICT, "batch_not_restorable"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::RegistrationDisabled => (StatusCode::FORBIDDEN, "registration_disabled"),
            ApiError::BadGateway => (StatusCode::BAD_GATEWAY, "upstream"),
            ApiError::Database(err) => {
                tracing::error!(?err, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
            ApiError::Domain(_) => (StatusCode::BAD_REQUEST, "domain"),
            ApiError::Internal(err) => {
                tracing::error!(?err, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let body = ApiErrorBody {
            code,
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
