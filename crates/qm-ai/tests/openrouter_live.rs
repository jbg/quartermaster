use std::time::Duration;

use qm_ai::{AiConfig, AiProviderKind, OpenRouterConfig, StructuredOutputRequest};
use serde_json::json;

#[tokio::test]
#[ignore = "requires QM_AI_OPENROUTER_API_KEY and makes a live OpenRouter request"]
async fn openrouter_live_structured_output_smoke() {
    let api_key = std::env::var("QM_AI_OPENROUTER_API_KEY")
        .expect("QM_AI_OPENROUTER_API_KEY must be set for the live OpenRouter smoke test");
    let model = std::env::var("QM_AI_MODEL").unwrap_or_else(|_| "openai/gpt-4.1-mini".to_owned());
    let base_url = std::env::var("QM_AI_OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_owned());
    let provider = qm_ai::build_provider(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP client should build"),
        &AiConfig {
            provider: AiProviderKind::OpenRouter,
            model: Some(model),
            retain_raw_responses: false,
            openrouter: OpenRouterConfig {
                api_key: Some(api_key),
                base_url,
            },
        },
    )
    .expect("OpenRouter provider should build");

    let response = provider
        .complete_structured(StructuredOutputRequest {
            task_type: "live_smoke".into(),
            prompt_version: "openrouter-live-test.v1".into(),
            model: None,
            max_output_tokens: Some(64),
            system_prompt: "Return only structured JSON that matches the supplied schema.".into(),
            user_prompt: "Set status to ok.".into(),
            json_schema_name: "openrouter_live_smoke".into(),
            json_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["status"],
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["ok"]
                    }
                }
            }),
        })
        .await
        .expect("OpenRouter should return structured JSON");

    assert_eq!(response.output_json, json!({ "status": "ok" }));
}
