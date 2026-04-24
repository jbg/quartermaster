use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde_json::json;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/.well-known/apple-app-site-association",
        get(apple_app_site_association),
    )
}

pub async fn apple_app_site_association(State(state): State<AppState>) -> impl IntoResponse {
    let Some(body) = apple_app_site_association_body(state.config.ios_release_identity.as_ref())
    else {
        return (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain")],
            String::new(),
        )
            .into_response();
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

pub fn apple_app_site_association_app_id(
    identity: Option<&crate::IosReleaseIdentity>,
) -> Option<String> {
    identity.map(crate::IosReleaseIdentity::app_id)
}

pub fn apple_app_site_association_body(
    identity: Option<&crate::IosReleaseIdentity>,
) -> Option<serde_json::Value> {
    let app_id = apple_app_site_association_app_id(identity)?;
    Some(json!({
        "applinks": {
            "apps": [],
            "details": [
                {
                    "appID": app_id,
                    "paths": ["/join", "/join*"]
                }
            ]
        }
    }))
}
