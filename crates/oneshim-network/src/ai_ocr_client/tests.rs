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
