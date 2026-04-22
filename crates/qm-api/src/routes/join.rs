use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use serde_json::json;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/join", get(join_landing)).route(
        "/.well-known/apple-app-site-association",
        get(apple_app_site_association),
    )
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct JoinQuery {
    pub invite: Option<String>,
    pub server: Option<String>,
}

#[utoipa::path(
    get,
    path = "/join",
    operation_id = "invite_join_landing",
    tag = "accounts",
    params(JoinQuery),
    responses((status = 200, description = "HTML invite landing page")),
)]
pub async fn join_landing(Query(q): Query<JoinQuery>) -> impl IntoResponse {
    let invite = html_escape(q.invite.as_deref().unwrap_or(""));
    let server = html_escape(q.server.as_deref().unwrap_or(""));
    let invite_url = utf8_percent_encode(q.invite.as_deref().unwrap_or(""), NON_ALPHANUMERIC);
    let server_url = utf8_percent_encode(q.server.as_deref().unwrap_or(""), NON_ALPHANUMERIC);
    let deep_link = if invite.is_empty() && server.is_empty() {
        "quartermaster://join".to_owned()
    } else {
        format!("quartermaster://join?invite={invite_url}&server={server_url}")
    };

    Html(format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Join Quartermaster</title>
  <style>
    :root {{
      color-scheme: light dark;
      font-family: ui-rounded, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }}
    body {{
      margin: 0;
      min-height: 100vh;
      display: grid;
      place-items: center;
      background: linear-gradient(160deg, #f1efe3, #e0ead8);
      color: #1f2a1f;
    }}
    main {{
      width: min(32rem, calc(100vw - 2rem));
      background: rgba(255,255,255,0.92);
      border-radius: 24px;
      padding: 2rem;
      box-shadow: 0 24px 80px rgba(36, 52, 32, 0.16);
    }}
    a.button {{
      display: inline-block;
      margin-top: 1rem;
      padding: 0.9rem 1.1rem;
      border-radius: 999px;
      text-decoration: none;
      background: #24492f;
      color: white;
      font-weight: 600;
    }}
    code {{
      display: block;
      padding: 0.8rem 1rem;
      border-radius: 12px;
      background: rgba(36, 73, 47, 0.08);
      overflow-wrap: anywhere;
    }}
  </style>
</head>
<body>
  <main>
    <h1>Join Quartermaster</h1>
    <p>Open the app to join a household with the invite details below.</p>
    <a class="button" href="{deep_link}">Open in Quartermaster</a>
    <h2>Invite code</h2>
    <code>{invite}</code>
    <h2>Server URL</h2>
    <code>{server}</code>
    <p>If the app does not open automatically, copy the invite code and server URL into Quartermaster manually.</p>
  </main>
</body>
</html>"#
    ))
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

fn html_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
