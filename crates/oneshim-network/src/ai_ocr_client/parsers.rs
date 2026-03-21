    fn parse_claude_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse response JSON: {}", e)))?;

        let mut results = Vec::new();

        if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    for (i, line) in text.lines().enumerate() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            results.push(OcrResult {
                                text: trimmed.to_string(),
                                x: 0,
                                y: (i as i32) * 20, // temporary line height
                                width: (trimmed.len() as u32) * 8, // temporary char width
                                height: 20,
                                confidence: 0.8, // API default confidence
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    fn parse_openai_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        if let Ok(results) = Self::parse_generic_response(body) {
            return Ok(results);
        }

        let response: Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse OpenAI response: {e}")))?;

        let text = Self::extract_openai_text(&response).ok_or_else(|| {
            CoreError::OcrError("No text content found in OpenAI OCR response".to_string())
        })?;

        if let Some(json_fragment) = extract_json_fragment(&text) {
            if let Ok(results) = Self::parse_generic_response(&json_fragment) {
                return Ok(results);
            }
        }

        Ok(parse_text_lines_to_results(&text))
    }

    fn parse_generic_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        #[derive(Deserialize)]
        struct GenericResponse {
            results: Option<Vec<OcrResult>>,
        }

        let response: GenericResponse = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse generic response: {}", e)))?;

        response.results.ok_or_else(|| {
            CoreError::OcrError("Generic OCR response missing `results` field".to_string())
        })
    }

    fn extract_openai_text(response: &Value) -> Option<String> {
        if let Some(content) = response
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
        {
            if let Some(text) = value_to_text(content) {
                return Some(text);
            }
        }

        let mut chunks = Vec::new();
        if let Some(outputs) = response.get("output").and_then(|o| o.as_array()) {
            for output in outputs {
                if let Some(parts) = output.get("content").and_then(|c| c.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                chunks.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        if chunks.is_empty() {
            None
        } else {
            Some(chunks.join("\n"))
        }
    }

    fn parse_google_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body).map_err(|e| {
            CoreError::OcrError(format!("Failed to parse Google Vision response: {}", e))
        })?;

        let mut results = Vec::new();
        let annotations = response
            .get("responses")
            .and_then(|r| r.as_array())
            .and_then(|arr| arr.first())
            .and_then(|entry| entry.get("textAnnotations"))
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();

        for annotation in annotations {
            let text = annotation
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                continue;
            }

            let vertices = annotation
                .get("boundingPoly")
                .and_then(|poly| poly.get("vertices"))
                .and_then(|v| v.as_array());
            let (x, y, width, height) = parse_bounding_vertices(vertices);

            results.push(OcrResult {
                text: text.to_string(),
                x,
                y,
                width,
                height,
                confidence: 0.8,
            });
        }

        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    details: Option<OllamaShowDetails>,
    #[serde(default)]
    projector_info: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct OllamaShowDetails {
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    families: Vec<String>,
}

fn derive_ollama_show_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    for suffix in [
        "/v1/responses",
        "/v1/chat/completions",
        "/api/tags",
        "/api/show",
    ] {
        if let Some(prefix) = trimmed.strip_suffix(suffix) {
            return format!("{prefix}/api/show");
        }
    }
    format!("{trimmed}/api/show")
}

fn infer_ollama_vision_support(model: &str) -> bool {
    let normalized = model.trim().to_ascii_lowercase();
    [
        "vision",
        "vl",
        "llava",
        "bakllava",
        "moondream",
        "minicpm-v",
        "minicpmv",
        "gemma3",
    ]
    .iter()
    .any(|token| normalized.contains(token))
}

fn parse_ollama_show_supports_ocr(body: &str, model: &str) -> Result<Option<bool>, CoreError> {
    let parsed: OllamaShowResponse = serde_json::from_str(body).map_err(|error| {
        CoreError::Network(format!("Failed to parse Ollama model details: {error}"))
    })?;
    let mut capabilities = parsed.capabilities;
    if let Some(details) = parsed.details {
        capabilities.extend(details.capabilities);
        capabilities.extend(details.families);
    }
    if parsed.projector_info.is_some() {
        capabilities.push("projector".to_string());
    }

    if capabilities.is_empty() {
        return Ok(Some(infer_ollama_vision_support(model)));
    }

    let supports_vision = capabilities.iter().any(|entry| {
        let normalized = entry.trim().to_ascii_lowercase();
        normalized.contains("vision")
            || normalized.contains("clip")
            || normalized.contains("projector")
            || normalized.contains("vl")
            || normalized.contains("llava")
    });
    Ok(Some(supports_vision))
}

async fn probe_ollama_model_supports_ocr(
    client: &reqwest::Client,
    endpoint: &str,
    model: &str,
) -> Result<Option<bool>, CoreError> {
    let response = client
        .post(derive_ollama_show_endpoint(endpoint))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "model": model }))
        .send()
        .await
        .map_err(|error| {
            CoreError::Network(format!("Ollama model capability probe failed: {error}"))
        })?;
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        CoreError::Network(format!(
            "Failed to read Ollama model capability probe response: {error}"
        ))
    })?;
    if !status.is_success() {
        return Err(CoreError::Network(format!(
            "Ollama model capability probe failed ({status}): {body}"
        )));
    }

    parse_ollama_show_supports_ocr(&body, model)
}

#[async_trait]
impl OcrProvider for RemoteOcrProvider {
    async fn extract_elements(
        &self,
        image: &[u8],
        image_format: &str,
    ) -> Result<Vec<OcrResult>, CoreError> {
        use base64::Engine;

        let encoded = base64::engine::general_purpose::STANDARD.encode(image);
        let media_type = match image_format {
            "png" => "image/png",
            "jpeg" | "jpg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };

        let model = self.model.as_deref().unwrap_or("");
        self.ensure_runtime_ocr_model_ready(model).await?;
        let request_shape = self.ocr_request_shape()?;
        match request_shape {
            ProviderRequestShape::AnthropicMessages
            | ProviderRequestShape::AnthropicVisionMessages => {
                self.ensure_ocr_parameters_supported(&["model", "max_tokens", "messages"])?;
            }
            ProviderRequestShape::OpenAiChatCompletions
            | ProviderRequestShape::OpenAiVisionChatCompletions
            | ProviderRequestShape::OpenAiResponses => {
                self.ensure_ocr_parameters_supported(&[
                    "model",
                    "max_tokens",
                    "response_format",
                    "messages",
                ])?;
            }
            ProviderRequestShape::GoogleGenerateContent
            | ProviderRequestShape::GoogleVisionAnnotate => {
                self.ensure_ocr_parameters_supported(&[
                    "requests",
                    "TEXT_DETECTION",
                    "maxResults",
                ])?;
            }
        }
        let strategy = OcrProviderStrategy::try_from(request_shape)?;

        let request_body = strategy.build_request_body(&encoded, media_type, model);

        debug!(
            endpoint = %self.endpoint,
            model = model,
            image_size = image.len(),
            "Calling external OCR API"
        );

        let mut builder = self
            .http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&request_body);

        let auth_scheme = self.ocr_auth_scheme()?;
        if matches!(auth_scheme, ProviderAuthScheme::None) {
            builder = apply_auth_headers(auth_scheme, builder, "");
        } else {
            let bearer_token = self.credential.resolve_bearer_token().await?;
            builder = apply_auth_headers(auth_scheme, builder, &bearer_token);
        }

        // ChatGPT OAuth requires a version header for model access (GPT-5.4 etc.).
        // Only applies to OpenAI-compatible providers, matching LLM client behaviour.
        // Ref: openai/codex codex-rs/core/src/model_provider_info.rs
        if self.credential.is_managed() && matches!(auth_scheme, ProviderAuthScheme::Bearer) {
            builder = builder.header("version", env!("CARGO_PKG_VERSION"));
        }

        let response = builder
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("OCR API request failed: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| CoreError::Network(format!("OCR API response read failure: {}", e)))?;

        if !status.is_success() {
            warn!(status = %status, "OCR API error response");
            return Err(CoreError::OcrError(format!(
                "OCR API error ({}): {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let results = strategy.parse_response(&body)?;

        debug!(count = results.len(), "OCR received");
        Ok(results)
    }

    fn provider_name(&self) -> &str {
        "remote-ocr"
    }

    fn is_external(&self) -> bool {
        true
    }
}

fn parse_bounding_vertices(vertices: Option<&Vec<serde_json::Value>>) -> (i32, i32, u32, u32) {
    let Some(vertices) = vertices else {
        return (0, 0, 0, 0);
    };

    let points: Vec<(i32, i32)> = vertices
        .iter()
        .map(|vertex| {
            let x = vertex.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = vertex.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            (x, y)
        })
        .collect();

    if points.is_empty() {
        return (0, 0, 0, 0);
    }

    let min_x = points.iter().map(|(x, _)| *x).min().unwrap_or(0);
    let max_x = points.iter().map(|(x, _)| *x).max().unwrap_or(0);
    let min_y = points.iter().map(|(_, y)| *y).min().unwrap_or(0);
    let max_y = points.iter().map(|(_, y)| *y).max().unwrap_or(0);

    (
        min_x,
        min_y,
        (max_x - min_x).max(0) as u32,
        (max_y - min_y).max(0) as u32,
    )
}

fn value_to_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(items) = value.as_array() {
        let mut parts = Vec::new();
        for item in items {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }

    None
}

fn extract_json_fragment(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    Some(text[start..=end].to_string())
}

fn parse_text_lines_to_results(text: &str) -> Vec<OcrResult> {
    text.lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(OcrResult {
                text: trimmed.to_string(),
                x: 0,
                y: (idx as i32) * 20,
                width: (trimmed.len() as u32) * 8,
                height: 20,
                confidence: 0.8,
            })
        })
        .collect()
}
