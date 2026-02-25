use std::env;
use std::time::Duration;

use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::ports::llm_provider::{LlmProvider, ScreenContext};
use oneshim_core::ports::ocr_provider::OcrProvider;
use oneshim_network::ai_llm_client::RemoteLlmProvider;
use oneshim_network::ai_ocr_client::RemoteOcrProvider;
use tokio::time::sleep;

const RUN_SMOKE_ENV: &str = "ONESHIM_RUN_AI_LIVE_SMOKE";
const RUN_OCR_ENV: &str = "ONESHIM_AI_SMOKE_RUN_OCR";

const SAMPLE_PNG_1X1: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8,
    6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 96, 0, 0, 0, 2, 0,
    1, 226, 33, 188, 51, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ai_provider_live_smoke() {
    if !should_run_live_smoke() {
        eprintln!(
            "Skipping AI provider live smoke test because {} is not enabled.",
            RUN_SMOKE_ENV
        );
        return;
    }

    run_llm_smoke().await;

    if parse_bool_env(RUN_OCR_ENV).unwrap_or(false) {
        run_ocr_smoke().await;
    } else {
        eprintln!(
            "Skipping OCR live smoke because {} is disabled.",
            RUN_OCR_ENV
        );
    }
}

async fn run_llm_smoke() {
    let endpoint = build_endpoint("ONESHIM_AI_SMOKE_LLM");
    let provider = RemoteLlmProvider::new(&endpoint).expect("LLM provider setup failed");

    let screen_context = ScreenContext {
        visible_texts: vec![
            "File".to_string(),
            "Save".to_string(),
            "Cancel".to_string(),
            "Settings".to_string(),
        ],
        active_app: "SmokeApp".to_string(),
        active_window_title: "AI Provider Live Smoke".to_string(),
        layout_description: Some("Save button is in the lower-right area".to_string()),
    };

    let mut last_err = None;
    for attempt in 1..=2 {
        match provider
            .interpret_intent(
                &screen_context,
                "click the save button and ignore cancel button",
            )
            .await
        {
            Ok(action) => {
                assert!(
                    !action.action_type.trim().is_empty(),
                    "LLM returned empty action_type"
                );
                assert!(
                    action.confidence.is_finite() && (0.0..=1.0).contains(&action.confidence),
                    "LLM confidence must be finite and in 0.0..=1.0, got {}",
                    action.confidence
                );
                return;
            }
            Err(err) => {
                last_err = Some(err);
                if attempt < 2 {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    panic!(
        "LLM smoke test failed after retries: {}",
        last_err.expect("last error missing")
    );
}

async fn run_ocr_smoke() {
    let endpoint = build_endpoint("ONESHIM_AI_SMOKE_OCR");
    let provider = RemoteOcrProvider::new(&endpoint).expect("OCR provider setup failed");

    let mut last_err = None;
    for attempt in 1..=2 {
        match provider.extract_elements(SAMPLE_PNG_1X1, "png").await {
            Ok(results) => {
                assert!(
                    results.len() <= 2048,
                    "OCR result count too large: {}",
                    results.len()
                );
                for item in results.iter().take(16) {
                    assert!(item.confidence.is_finite(), "OCR confidence must be finite");
                    assert!(
                        (0.0..=1.0).contains(&item.confidence),
                        "OCR confidence must be in 0.0..=1.0, got {}",
                        item.confidence
                    );
                }
                return;
            }
            Err(err) => {
                last_err = Some(err);
                if attempt < 2 {
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    }

    panic!(
        "OCR smoke test failed after retries: {}",
        last_err.expect("last error missing")
    );
}

fn build_endpoint(prefix: &str) -> ExternalApiEndpoint {
    let endpoint = required_env(format!("{prefix}_ENDPOINT").as_str());
    let api_key = required_env(format!("{prefix}_KEY").as_str());
    let model = optional_env(format!("{prefix}_MODEL").as_str());
    let timeout_secs = env::var(format!("{prefix}_TIMEOUT_SECS"))
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(45);
    let provider_type = env::var(format!("{prefix}_PROVIDER_TYPE"))
        .ok()
        .map(|raw| parse_provider_type(&raw))
        .unwrap_or(AiProviderType::Generic);

    ExternalApiEndpoint {
        endpoint,
        api_key,
        model,
        timeout_secs,
        provider_type,
    }
}

fn parse_provider_type(raw: &str) -> AiProviderType {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => AiProviderType::Anthropic,
        "openai" | "open_ai" | "open-ai" => AiProviderType::OpenAi,
        "google" | "gemini" => AiProviderType::Google,
        "generic" => AiProviderType::Generic,
        other => panic!("Unsupported provider type: {other}"),
    }
}

fn should_run_live_smoke() -> bool {
    parse_bool_env(RUN_SMOKE_ENV).unwrap_or(false)
}

fn parse_bool_env(key: &str) -> Option<bool> {
    env::var(key).ok().map(|raw| {
        matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn required_env(key: &str) -> String {
    let value = env::var(key).unwrap_or_else(|_| panic!("Missing required env: {key}"));
    if value.trim().is_empty() {
        panic!("Required env is empty: {key}");
    }
    value
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}
