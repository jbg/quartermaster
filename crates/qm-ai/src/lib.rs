//! Shared AI substrate for Quartermaster intelligence features.
//!
//! Providers live behind this crate so feature code records deterministic task
//! metadata and asks for structured JSON without learning provider-specific
//! request shapes or handling credentials directly.

use std::{future::Future, pin::Pin, sync::Arc};

use reqwest::{header, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

pub type AiProviderRef = Arc<dyn AiProvider>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    Disabled,
    OpenRouter,
}

impl AiProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::OpenRouter => "openrouter",
        }
    }
}

impl std::fmt::Display for AiProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AiProviderKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "disabled" => Ok(Self::Disabled),
            "openrouter" => Ok(Self::OpenRouter),
            other => Err(format!("unknown AI provider: {other}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AiConfig {
    pub provider: AiProviderKind,
    pub model: Option<String>,
    pub retain_raw_responses: bool,
    pub openrouter: OpenRouterConfig,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: AiProviderKind::Disabled,
            model: None,
            retain_raw_responses: false,
            openrouter: OpenRouterConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct OpenRouterConfig {
    pub api_key: Option<String>,
    pub base_url: String,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://openrouter.ai/api/v1".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AiProviderStatus {
    pub provider: AiProviderKind,
    pub enabled: bool,
    pub configured: bool,
    pub model: Option<String>,
    pub structured_outputs: bool,
    pub raw_response_retention: bool,
}

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI provider is disabled")]
    Disabled,

    #[error("AI provider is not configured: {0}")]
    NotConfigured(String),

    #[error("AI provider rejected the request: {0}")]
    ProviderRejected(String),

    #[error("AI provider returned invalid structured output: {0}")]
    InvalidStructuredOutput(String),

    #[error("AI provider HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Clone, Debug)]
pub struct StructuredOutputRequest {
    pub task_type: String,
    pub prompt_version: String,
    pub model: Option<String>,
    pub system_prompt: String,
    pub user_prompt: String,
    pub json_schema_name: String,
    pub json_schema: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct StructuredOutputResponse {
    pub provider: AiProviderKind,
    pub model: String,
    pub output_json: Value,
    pub raw_response_json: Option<Value>,
}

pub trait AiProvider: std::fmt::Debug + Send + Sync {
    fn status(&self) -> AiProviderStatus;

    fn complete_structured<'a>(
        &'a self,
        request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>>;
}

#[derive(Debug)]
pub struct DisabledProvider;

impl AiProvider for DisabledProvider {
    fn status(&self) -> AiProviderStatus {
        AiProviderStatus {
            provider: AiProviderKind::Disabled,
            enabled: false,
            configured: true,
            model: None,
            structured_outputs: false,
            raw_response_retention: false,
        }
    }

    fn complete_structured<'a>(
        &'a self,
        _request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>> {
        Box::pin(async { Err(AiError::Disabled) })
    }
}

#[derive(Debug)]
pub struct OpenRouterProvider {
    http: reqwest::Client,
    api_key: Option<String>,
    base_url: Url,
    default_model: String,
    retain_raw_responses: bool,
}

impl OpenRouterProvider {
    pub fn new(http: reqwest::Client, config: &AiConfig) -> Result<Self, AiError> {
        let mut base_url = Url::parse(&config.openrouter.base_url)
            .map_err(|err| AiError::NotConfigured(format!("invalid OpenRouter base URL: {err}")))?;
        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err(AiError::NotConfigured(
                "OpenRouter base URL must not include query or fragment".into(),
            ));
        }
        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path().trim_end_matches('/'));
            base_url.set_path(&path);
        }
        let default_model = config
            .model
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AiError::NotConfigured("model is required".into()))?
            .to_owned();
        Ok(Self {
            http,
            api_key: config.openrouter.api_key.clone(),
            base_url,
            default_model,
            retain_raw_responses: config.retain_raw_responses,
        })
    }
}

impl AiProvider for OpenRouterProvider {
    fn status(&self) -> AiProviderStatus {
        AiProviderStatus {
            provider: AiProviderKind::OpenRouter,
            enabled: true,
            configured: self.api_key.is_some(),
            model: Some(self.default_model.clone()),
            structured_outputs: true,
            raw_response_retention: self.retain_raw_responses,
        }
    }

    fn complete_structured<'a>(
        &'a self,
        request: StructuredOutputRequest,
    ) -> Pin<Box<dyn Future<Output = Result<StructuredOutputResponse, AiError>> + Send + 'a>> {
        Box::pin(async move {
            let api_key = self
                .api_key
                .as_deref()
                .ok_or_else(|| AiError::NotConfigured("OpenRouter API key is required".into()))?;
            let url = self
                .base_url
                .join("chat/completions")
                .map_err(|err| AiError::NotConfigured(format!("invalid OpenRouter URL: {err}")))?;
            let model = request
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&self.default_model)
                .to_owned();
            let body = json!({
                "model": model,
                "messages": [
                    {"role": "system", "content": request.system_prompt},
                    {"role": "user", "content": request.user_prompt}
                ],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": request.json_schema_name,
                        "strict": true,
                        "schema": request.json_schema
                    }
                }
            });
            let response = self
                .http
                .post(url)
                .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
                .header("HTTP-Referer", "https://github.com/jbg/quartermaster")
                .header("X-Title", "Quartermaster")
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            let body = response.bytes().await?;
            if !status.is_success() {
                return Err(provider_rejected(status, &body));
            }
            let raw: Value = serde_json::from_slice(&body).map_err(|err| {
                AiError::InvalidStructuredOutput(format!(
                    "provider response was not JSON: {err}; body: {}",
                    body_preview(&body)
                ))
            })?;
            let output_json = extract_structured_output(&raw)?;
            Ok(StructuredOutputResponse {
                provider: AiProviderKind::OpenRouter,
                model,
                output_json,
                raw_response_json: self.retain_raw_responses.then_some(raw),
            })
        })
    }
}

pub fn build_provider(http: reqwest::Client, config: &AiConfig) -> Result<AiProviderRef, AiError> {
    match config.provider {
        AiProviderKind::Disabled => Ok(Arc::new(DisabledProvider)),
        AiProviderKind::OpenRouter => Ok(Arc::new(OpenRouterProvider::new(http, config)?)),
    }
}

fn provider_rejected(status: StatusCode, body: &[u8]) -> AiError {
    let detail = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| body_preview(body));
    AiError::ProviderRejected(format!("{status}: {detail}"))
}

fn extract_structured_output(raw: &Value) -> Result<Value, AiError> {
    let content = raw.pointer("/choices/0/message/content").ok_or_else(|| {
        AiError::InvalidStructuredOutput("missing choices[0].message.content".into())
    })?;
    match content {
        Value::String(content) => serde_json::from_str(content)
            .map_err(|err| AiError::InvalidStructuredOutput(err.to_string())),
        Value::Object(_) | Value::Array(_) => Ok(content.clone()),
        _ => Err(AiError::InvalidStructuredOutput(
            "choices[0].message.content was not JSON text or an object".into(),
        )),
    }
}

fn body_preview(body: &[u8]) -> String {
    const MAX_PREVIEW_CHARS: usize = 500;
    let text = String::from_utf8_lossy(body);
    let mut preview = text.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    if text.chars().count() > MAX_PREVIEW_CHARS {
        preview.push_str("...");
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_provider_reports_not_enabled() {
        let status = DisabledProvider.status();
        assert_eq!(status.provider, AiProviderKind::Disabled);
        assert!(!status.enabled);
        assert!(status.configured);
    }

    #[test]
    fn openrouter_requires_model() {
        let config = AiConfig {
            provider: AiProviderKind::OpenRouter,
            model: None,
            retain_raw_responses: false,
            openrouter: OpenRouterConfig::default(),
        };
        let err = OpenRouterProvider::new(reqwest::Client::new(), &config).unwrap_err();
        assert!(err.to_string().contains("model is required"));
    }

    #[test]
    fn openrouter_extracts_structured_output_from_chat_content() {
        let raw = json!({
            "choices": [{
                "message": {
                    "content": "{\"ideas\":[{\"name\":\"Soup\"}]}"
                }
            }]
        });
        assert_eq!(
            extract_structured_output(&raw).unwrap(),
            json!({"ideas": [{"name": "Soup"}]})
        );
    }

    #[test]
    fn openrouter_provider_rejection_includes_error_body_message() {
        let err = provider_rejected(
            StatusCode::BAD_REQUEST,
            br#"{"error":{"message":"response_format is not supported by this model"}}"#,
        );
        assert_eq!(
            err.to_string(),
            "AI provider rejected the request: 400 Bad Request: response_format is not supported by this model"
        );
    }

    #[test]
    fn openrouter_invalid_provider_json_includes_body_preview() {
        let err = serde_json::from_slice::<Value>(b"<html>not json</html>").map_err(|err| {
            AiError::InvalidStructuredOutput(format!(
                "provider response was not JSON: {err}; body: {}",
                body_preview(b"<html>not json</html>")
            ))
        });
        assert!(err
            .unwrap_err()
            .to_string()
            .contains("<html>not json</html>"));
    }
}
