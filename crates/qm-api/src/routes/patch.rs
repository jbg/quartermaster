use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;

use crate::{ApiError, ApiResult};

pub type JsonPatchDocument = Vec<JsonPatchOperation>;

#[derive(Debug, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct JsonPatchOperation {
    /// JSON Patch operation. Quartermaster supports `replace` and `remove`
    /// for product/stock update endpoints.
    pub op: String,
    /// JSON Pointer path to the top-level resource field, e.g. `/brand`.
    pub path: String,
    /// Required for `replace`; omitted for `remove`.
    #[schema(nullable = false)]
    pub value: Option<Value>,
}

pub fn string_value(field: &str, value: Option<&Value>) -> ApiResult<String> {
    value
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::BadRequest(format!("{field} replace value must be a string")))
}

pub fn reject_remove(field: &str) -> ApiError {
    ApiError::BadRequest(format!("{field} cannot be removed"))
}

pub fn reject_value_for_remove(field: &str, value: Option<&Value>) -> ApiResult<()> {
    if value.is_some() {
        return Err(ApiError::BadRequest(format!(
            "{field} remove operation must not include value"
        )));
    }
    Ok(())
}
