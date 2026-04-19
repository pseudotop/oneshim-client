use super::*;

#[test]
fn new_remote_ocr_empty_key_error() {
    let config = ExternalApiEndpoint {
        endpoint: "https://api.example.com".to_string(),
        api_key: "".to_string(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Generic,
        surface_id: None,
        credential: None,
    };
    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not configured"));
}

#[test]
fn new_remote_ocr_with_key() {
    let config = ExternalApiEndpoint {
        endpoint: "https://api.example.com".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Generic,
        surface_id: None,
        credential: None,
    };
    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_ok());
}

#[test]
fn generic_ocr_uses_spec_shape_and_default_model() {
    let config = ExternalApiEndpoint {
        endpoint: "https://api.example.com".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Generic,
        surface_id: None,
        credential: None,
    };

    let provider = RemoteOcrProvider::new(&config).expect("generic OCR provider should build");
    assert_eq!(
        provider.ocr_request_shape().expect("shape should resolve"),
        ProviderRequestShape::OpenAiVisionChatCompletions
    );
    assert_eq!(provider.model.as_deref(), Some("gpt-5-mini"));
}

#[test]
fn ollama_ocr_initializes_without_api_key() {
    let config = ExternalApiEndpoint {
        endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
        api_key: String::new(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Ollama,
        surface_id: Some("provider_surface.ollama.local_http".to_string()),
        credential: None,
    };

    let provider = RemoteOcrProvider::new(&config).expect("ollama OCR provider should build");
    assert_eq!(
        provider.ocr_request_shape().expect("shape should resolve"),
        ProviderRequestShape::OpenAiVisionChatCompletions
    );
    assert_eq!(provider.model.as_deref(), Some("qwen3-vl:8b"));
}

#[test]
fn ollama_ocr_rejects_known_text_only_model() {
    let config = ExternalApiEndpoint {
        endpoint: "http://localhost:11434/v1/chat/completions".to_string(),
        api_key: String::new(),
        model: Some("qwen3:8b".to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::Ollama,
        surface_id: Some("provider_surface.ollama.local_http".to_string()),
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("OCR-capable"));
}

#[test]
fn local_openai_compatible_ocr_requires_explicit_model_selection() {
    let config = ExternalApiEndpoint {
        endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
        api_key: String::new(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Generic,
        surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("requires an explicit model selection"));
}

#[test]
fn local_openai_compatible_ocr_rejects_model_without_structured_output() {
    let config = ExternalApiEndpoint {
        endpoint: "http://127.0.0.1:1234/v1/chat/completions".to_string(),
        api_key: String::new(),
        model: Some("text-embedding-3-small".to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::Generic,
        surface_id: Some("provider_surface.generic.local_openai_compatible".to_string()),
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("structured JSON output")
            || message.contains("OCR-capable")
            || message.contains("not marked as OCR-capable")
    );
}

#[test]
fn new_remote_ocr_rejects_retired_model_by_policy() {
    let config = ExternalApiEndpoint {
        endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: Some("gpt-3.5-turbo".to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::OpenAi,
        surface_id: None,
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("retired as of"));
}

#[test]
fn google_surface_uses_surface_transport_shape() {
    let config = ExternalApiEndpoint {
        endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: None,
        timeout_secs: 30,
        provider_type: AiProviderType::Google,
        surface_id: Some("provider_surface.google.direct_api".to_string()),
        credential: None,
    };

    let provider = RemoteOcrProvider::new(&config).expect("google OCR provider should build");
    assert_eq!(
        provider.ocr_request_shape().expect("shape should resolve"),
        ProviderRequestShape::GoogleVisionAnnotate
    );
}

#[test]
fn new_remote_ocr_rejects_known_non_ocr_model() {
    let config = ExternalApiEndpoint {
        endpoint: "https://api.openai.com/v1/chat/completions".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: Some("text-embedding-3-small".to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::OpenAi,
        surface_id: Some("provider_surface.openai.direct_api".to_string()),
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not marked as OCR-capable"));
}

#[test]
fn google_ocr_rejects_explicit_model_selection() {
    let config = ExternalApiEndpoint {
        endpoint: "https://vision.googleapis.com/v1/images:annotate".to_string(),
        api_key: "test-api-key-placeholder".to_string(),
        model: Some("gemini-2.5-flash".to_string()),
        timeout_secs: 30,
        provider_type: AiProviderType::Google,
        surface_id: Some("provider_surface.google.direct_api".to_string()),
        credential: None,
    };

    let result = RemoteOcrProvider::new(&config);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("does not support configurable model selection"));
}

#[test]
fn parse_claude_vision_response_valid() {
    let response = r#"{
            "content": [
                {
                    "type": "text",
                    "text": "file\nedit\nview\nsave"
                }
            ]
        }"#;
    let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].text, "file");
    assert_eq!(results[3].text, "save");
}

#[test]
fn parse_claude_vision_response_empty() {
    let response = r#"{"content": [{"type": "text", "text": ""}]}"#;
    let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
    assert!(results.is_empty());
}

#[test]
fn parse_generic_response_valid() {
    let response = r#"{
            "results": [
                {"text": "save", "x": 100, "y": 200, "width": 60, "height": 25, "confidence": 0.95}
            ]
        }"#;
    let results = RemoteOcrProvider::parse_generic_response(response).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "save");
    assert_eq!(results[0].x, 100);
}

#[test]
fn parse_generic_response_empty() {
    let response = r#"{"results": []}"#;
    let results = RemoteOcrProvider::parse_generic_response(response).unwrap();
    assert!(results.is_empty());
}

#[test]
fn parse_openai_vision_response_json_content() {
    let response = r#"{
            "choices": [{
                "message": {
                    "content": "{\"results\":[{\"text\":\"Save\",\"x\":12,\"y\":20,\"width\":48,\"height\":18,\"confidence\":0.93}]}"
                }
            }]
        }"#;

    let results = RemoteOcrProvider::parse_openai_vision_response(response).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "Save");
    assert_eq!(results[0].x, 12);
    assert!((results[0].confidence - 0.93).abs() < f64::EPSILON);
}

#[test]
fn parse_openai_vision_response_line_fallback() {
    let response = r#"{
            "choices": [{
                "message": {
                    "content": "File\nEdit\nSave"
                }
            }]
        }"#;

    let results = RemoteOcrProvider::parse_openai_vision_response(response).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[2].text, "Save");
}

#[test]
fn strategy_openai_request_uses_image_url_and_json_mode() {
    let payload =
        OcrProviderStrategy::OpenAi.build_request_body("ZmFrZS1pbWFnZQ==", "image/png", "gpt");
    assert_eq!(payload["response_format"]["type"], "json_object");
    assert_eq!(payload["messages"][0]["content"][1]["type"], "image_url");
    let url = payload["messages"][0]["content"][1]["image_url"]["url"]
        .as_str()
        .unwrap_or("");
    assert!(url.starts_with("data:image/png;base64,"));
}

#[test]
fn remote_ocr_provider_info() {
    let response = r#"{"content": [{"type": "text", "text": "test\nline2"}]}"#;
    let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
    assert_eq!(results.len(), 2);
    assert!((results[0].confidence - 0.8).abs() < f64::EPSILON);
}

/// ADR-019 §3 core security-fix regression guard: `apply_auth_headers`
/// must return a typed error on `AwsSignatureV4`, not silently fall
/// through (original pre-ADR-019 behavior). This is the foundational
/// contract that changed the function's signature from infallible to
/// `Result<_, CoreError>`. A regression that reverted the signature or
/// removed the error would silently send unauthenticated requests to
/// Bedrock endpoints — a real security bug.
#[test]
fn apply_auth_headers_rejects_aws_sigv4() {
    let client = reqwest::Client::new();
    let builder = client.get("https://bedrock-runtime.us-east-1.amazonaws.com");
    let result = apply_auth_headers(ProviderAuthScheme::AwsSignatureV4, builder, "irrelevant");
    match result {
        Err(CoreError::Config { code, message }) => {
            assert_eq!(
                code,
                oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock,
                "expected UnsupportedProviderBedrock code, got {code:?}"
            );
            assert!(
                message.contains("Bedrock"),
                "expected Bedrock-mentioning message, got {message:?}"
            );
        }
        Ok(_) => panic!(
            "apply_auth_headers(AwsSignatureV4, ..) returned Ok — SILENT NO-AUTH FALLTHROUGH REGRESSION"
        ),
        Err(other) => panic!(
            "expected CoreError::Config {{ UnsupportedProviderBedrock, .. }}, got {other:?}"
        ),
    }
}

/// Positive control: non-AwsSignatureV4 schemes must still succeed after
/// the signature change, so we don't accidentally regress valid auth paths.
#[test]
fn apply_auth_headers_succeeds_for_supported_schemes() {
    let client = reqwest::Client::new();
    for scheme in [
        ProviderAuthScheme::None,
        ProviderAuthScheme::Bearer,
        ProviderAuthScheme::XApiKey,
        ProviderAuthScheme::XGoogApiKey,
    ] {
        let builder = client.get("https://api.example.com");
        let result = apply_auth_headers(scheme, builder, "test-key");
        assert!(
            result.is_ok(),
            "apply_auth_headers({scheme:?}, ..) unexpectedly failed: {:?}",
            result.err()
        );
    }
}

/// ADR-019 §3 regression guard: OcrProviderStrategy::try_from must reject
/// BedrockConverse shape at the strategy-dispatch boundary. Unlike the
/// other OCR paths that fail on catalog lookup (Bedrock absent), this one
/// takes the shape as direct input, so it's the only defense if someone
/// constructs the shape manually (e.g., unit-test fixture, re-introduction
/// without SigV4 work).
#[test]
fn ocr_strategy_try_from_rejects_bedrock_converse() {
    let result = OcrProviderStrategy::try_from(ProviderRequestShape::BedrockConverse);
    match result {
        Err(CoreError::Config { code, message }) => {
            assert_eq!(
                code,
                oneshim_core::error_codes::ConfigCode::UnsupportedProviderBedrock,
                "expected UnsupportedProviderBedrock code, got {code:?}"
            );
            assert!(
                message.contains("Bedrock"),
                "expected Bedrock-mentioning message, got {message:?}"
            );
        }
        Ok(strategy) => panic!(
            "expected Err for BedrockConverse but got Ok({strategy:?}) — a regression would enable OCR dispatch to an unsupported provider"
        ),
        Err(other) => panic!(
            "expected CoreError::Config {{ UnsupportedProviderBedrock, .. }}, got {other:?}"
        ),
    }
}

/// Positive control: every non-Bedrock shape must round-trip to a
/// valid strategy variant, so the dispatch table stays exhaustive after
/// the BedrockConverse Err arm.
#[test]
fn ocr_strategy_try_from_accepts_supported_shapes() {
    for shape in [
        ProviderRequestShape::AnthropicMessages,
        ProviderRequestShape::AnthropicVisionMessages,
        ProviderRequestShape::OpenAiChatCompletions,
        ProviderRequestShape::OpenAiVisionChatCompletions,
        ProviderRequestShape::OpenAiResponses,
        ProviderRequestShape::GoogleGenerateContent,
        ProviderRequestShape::GoogleVisionAnnotate,
    ] {
        let result = OcrProviderStrategy::try_from(shape);
        assert!(
            result.is_ok(),
            "OcrProviderStrategy::try_from({shape:?}) unexpectedly failed: {:?}",
            result.err()
        );
    }
}
