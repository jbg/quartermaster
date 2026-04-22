mod support;

use axum::http::{Method, StatusCode};
use qm_api::ApiConfig;
use support::TestApp;

#[tokio::test]
async fn request_ids_are_generated_and_propagated() {
    let app = TestApp::start(ApiConfig::default()).await;

    let (status, headers, _) = app
        .send_with_request_id(Method::GET, "/healthz", None, None, None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let generated = headers.get("x-request-id").unwrap().to_str().unwrap();
    assert!(!generated.is_empty());

    let (status, headers, _) = app
        .send_with_request_id(Method::GET, "/healthz", None, None, Some("test-request-id"))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("x-request-id").unwrap(), "test-request-id");
}
