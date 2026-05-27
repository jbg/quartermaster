//! Shared AI substrate for Quartermaster intelligence features.
//!
//! Providers live behind this crate so feature code records deterministic task
//! metadata and asks for structured JSON without learning provider-specific
//! request shapes or handling credentials directly.

use std::{error::Error as _, future::Future, pin::Pin, sync::Arc, time::Instant};

use metrics::counter;
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

    #[error("AI provider truncated structured output before it was complete: {0}")]
    OutputTruncated(String),

    #[error("AI provider HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Clone, Debug)]
pub struct StructuredOutputRequest {
    pub task_type: String,
    pub prompt_version: String,
    pub model: Option<String>,
    pub max_output_tokens: Option<u32>,
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
            let task_type = request.task_type.clone();
            let prompt_version = request.prompt_version.clone();
            let schema_name = request.json_schema_name.clone();
            let system_prompt_bytes = request.system_prompt.len();
            let user_prompt_bytes = request.user_prompt.len();
            let max_output_tokens = request.max_output_tokens;
            let schema_bytes = serde_json::to_vec(&request.json_schema)
                .map(|bytes| bytes.len())
                .unwrap_or_default();
            let mut body = json!({
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
                },
                "reasoning": {
                    "effort": "minimal",
                    "exclude": true
                }
            });
            if let Some(max_output_tokens) = max_output_tokens {
                body["max_completion_tokens"] = json!(max_output_tokens);
            }
            let request_started = Instant::now();
            tracing::info!(
                provider = %AiProviderKind::OpenRouter,
                model = %model,
                task_type = %task_type,
                prompt_version = %prompt_version,
                schema_name = %schema_name,
                system_prompt_bytes,
                user_prompt_bytes,
                schema_bytes,
                max_output_tokens = max_output_tokens.unwrap_or_default(),
                "sending structured AI provider request"
            );
            let response = self
                .http
                .post(url)
                .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
                .header(header::ACCEPT, "application/json")
                .header("HTTP-Referer", "https://github.com/jbg/quartermaster")
                .header("X-Title", "Quartermaster")
                .json(&body)
                .send()
                .await?;
            let status = response.status();
            let content_type = header_value(response.headers(), header::CONTENT_TYPE);
            let content_encoding = header_value(response.headers(), header::CONTENT_ENCODING);
            tracing::info!(
                provider = %AiProviderKind::OpenRouter,
                model = %model,
                task_type = %task_type,
                status = %status,
                content_type = content_type.as_deref().unwrap_or("unknown"),
                content_encoding = content_encoding.as_deref().unwrap_or("identity"),
                elapsed_ms = request_started.elapsed().as_millis() as u64,
                "received structured AI provider response headers"
            );
            let body = response.bytes().await.map_err(|err| {
                provider_body_read_failed(
                    status,
                    content_type.as_deref(),
                    content_encoding.as_deref(),
                    err,
                )
            })?;
            if !status.is_success() {
                return Err(provider_rejected(status, &body));
            }
            tracing::info!(
                provider = %AiProviderKind::OpenRouter,
                model = %model,
                task_type = %task_type,
                response_body_bytes = body.len(),
                elapsed_ms = request_started.elapsed().as_millis() as u64,
                "read structured AI provider response body"
            );
            let raw: Value = serde_json::from_slice(&body).map_err(|err| {
                AiError::InvalidStructuredOutput(format!(
                    "provider response was not JSON: {err}; body: {}",
                    body_preview(&body)
                ))
            })?;
            let response_meta = StructuredResponseMeta::from_raw(&raw);
            tracing::info!(
                provider = %AiProviderKind::OpenRouter,
                model = %model,
                task_type = %task_type,
                finish_reason = response_meta.finish_reason.unwrap_or("unknown"),
                native_finish_reason = response_meta.native_finish_reason.unwrap_or("unknown"),
                prompt_tokens = response_meta.prompt_tokens.unwrap_or_default(),
                completion_tokens = response_meta.completion_tokens.unwrap_or_default(),
                reasoning_tokens = response_meta.reasoning_tokens.unwrap_or_default(),
                total_tokens = response_meta.total_tokens.unwrap_or_default(),
                content_chars = response_meta.content_chars.unwrap_or_default(),
                "parsed structured AI provider response metadata"
            );
            let output_json_result = extract_structured_output(&raw, &response_meta);
            record_structured_response_metrics(&task_type, &response_meta, &output_json_result);
            let output_json = output_json_result?;
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

fn provider_body_read_failed(
    status: StatusCode,
    content_type: Option<&str>,
    content_encoding: Option<&str>,
    err: reqwest::Error,
) -> AiError {
    let content_type = content_type.unwrap_or("unknown");
    let content_encoding = content_encoding.unwrap_or("identity");
    let is_timeout = err.is_timeout();
    let is_body = err.is_body();
    let is_decode = err.is_decode();
    let mut sources = Vec::new();
    let mut source = err.source();
    while let Some(err) = source {
        sources.push(err.to_string());
        source = err.source();
    }
    let source_detail = if sources.is_empty() {
        "none".into()
    } else {
        sources.join(" <- ")
    };
    AiError::InvalidStructuredOutput(format!(
        "could not read provider response body: {err}; status: {status}; content-type: {content_type}; content-encoding: {content_encoding}; timeout: {is_timeout}; body: {is_body}; decode: {is_decode}; source: {source_detail}"
    ))
}

fn header_value(headers: &header::HeaderMap, name: header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

#[derive(Debug, Default)]
struct StructuredResponseMeta<'a> {
    finish_reason: Option<&'a str>,
    native_finish_reason: Option<&'a str>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    reasoning_tokens: Option<i64>,
    total_tokens: Option<i64>,
    content_chars: Option<usize>,
}

impl<'a> StructuredResponseMeta<'a> {
    fn from_raw(raw: &'a Value) -> Self {
        let content_chars = raw
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(|value| value.chars().count());
        Self {
            finish_reason: raw
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str),
            native_finish_reason: raw
                .pointer("/choices/0/native_finish_reason")
                .and_then(Value::as_str),
            prompt_tokens: raw.pointer("/usage/prompt_tokens").and_then(Value::as_i64),
            completion_tokens: raw
                .pointer("/usage/completion_tokens")
                .and_then(Value::as_i64),
            reasoning_tokens: raw
                .pointer("/usage/completion_tokens_details/reasoning_tokens")
                .and_then(Value::as_i64),
            total_tokens: raw.pointer("/usage/total_tokens").and_then(Value::as_i64),
            content_chars,
        }
    }

    fn truncation_detail(&self) -> String {
        format!(
            "finish_reason={}; native_finish_reason={}; completion_tokens={}; reasoning_tokens={}; content_chars={}",
            self.finish_reason.unwrap_or("unknown"),
            self.native_finish_reason.unwrap_or("unknown"),
            self.completion_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            self.reasoning_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into()),
            self.content_chars
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".into())
        )
    }
}

fn record_structured_response_metrics(
    task_type: &str,
    meta: &StructuredResponseMeta<'_>,
    output_json_result: &Result<Value, AiError>,
) {
    let outcome = match output_json_result {
        Ok(_) => "success",
        Err(AiError::OutputTruncated(_)) => "truncated",
        Err(AiError::InvalidStructuredOutput(_)) => "invalid_structured_output",
        Err(_) => "error",
    };
    let finish_reason = meta.finish_reason.unwrap_or("unknown").to_owned();
    counter!(
        "qm_ai_structured_requests_total",
        "provider" => AiProviderKind::OpenRouter.as_str(),
        "task_type" => task_type.to_owned(),
        "outcome" => outcome,
        "finish_reason" => finish_reason.clone()
    )
    .increment(1);
    record_token_counter(task_type, &finish_reason, "prompt", meta.prompt_tokens);
    record_token_counter(
        task_type,
        &finish_reason,
        "completion",
        meta.completion_tokens,
    );
    record_token_counter(
        task_type,
        &finish_reason,
        "reasoning",
        meta.reasoning_tokens,
    );
    record_token_counter(task_type, &finish_reason, "total", meta.total_tokens);
}

fn record_token_counter(
    task_type: &str,
    finish_reason: &str,
    token_type: &'static str,
    tokens: Option<i64>,
) {
    let Some(tokens) = tokens.and_then(|value| u64::try_from(value).ok()) else {
        return;
    };
    counter!(
        "qm_ai_structured_tokens_total",
        "provider" => AiProviderKind::OpenRouter.as_str(),
        "task_type" => task_type.to_owned(),
        "token_type" => token_type,
        "finish_reason" => finish_reason.to_owned()
    )
    .increment(tokens);
}

fn extract_structured_output(
    raw: &Value,
    meta: &StructuredResponseMeta<'_>,
) -> Result<Value, AiError> {
    if meta.finish_reason == Some("length") {
        return Err(AiError::OutputTruncated(meta.truncation_detail()));
    }
    let content = raw.pointer("/choices/0/message/content").ok_or_else(|| {
        AiError::InvalidStructuredOutput("missing choices[0].message.content".into())
    })?;
    match content {
        Value::String(content) => serde_json::from_str(content).map_err(|err| {
            if err.is_eof() {
                AiError::OutputTruncated(format!("{}; parse_error={err}", meta.truncation_detail()))
            } else {
                AiError::InvalidStructuredOutput(err.to_string())
            }
        }),
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
        let meta = StructuredResponseMeta::from_raw(&raw);
        assert_eq!(
            extract_structured_output(&raw, &meta).unwrap(),
            json!({"ideas": [{"name": "Soup"}]})
        );
    }

    #[test]
    fn openrouter_reports_length_finish_as_truncation() {
        let raw = json!({
            "choices": [{
                "finish_reason": "length",
                "native_finish_reason": "length",
                "message": {
                    "content": "{\"ideas\":[{\"name\":\"Half"
                }
            }],
            "usage": {
                "completion_tokens": 2000,
                "completion_tokens_details": {
                    "reasoning_tokens": 123
                }
            }
        });
        let meta = StructuredResponseMeta::from_raw(&raw);
        let err = extract_structured_output(&raw, &meta).unwrap_err();
        assert!(matches!(err, AiError::OutputTruncated(_)));
        assert!(err.to_string().contains("finish_reason=length"));
        assert!(err.to_string().contains("reasoning_tokens=123"));
    }

    #[test]
    fn openrouter_reports_eof_json_content_as_truncation() {
        let raw = json!({
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "content": "{\"ideas\":[{\"name\":\"Half"
                }
            }]
        });
        let meta = StructuredResponseMeta::from_raw(&raw);
        let err = extract_structured_output(&raw, &meta).unwrap_err();
        assert!(matches!(err, AiError::OutputTruncated(_)));
        assert!(err.to_string().contains("EOF while parsing a string"));
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
