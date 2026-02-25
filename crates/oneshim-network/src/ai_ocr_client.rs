//!

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};

///
/// - Claude Vision (Anthropic): `POST /v1/messages` + image content block
/// - Google Cloud Vision: `POST /v1/images:annotate` + TEXT_DETECTION
///
#[derive(Debug)]
pub struct RemoteOcrProvider {
    http_client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: Option<String>,
    provider_type: AiProviderType,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl RemoteOcrProvider {
    ///
    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        if config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI OCR API key is not configured. Set it in Settings.".into(),
            ));
        }
        let api_key = config.api_key.clone();

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP client create failure: {}", e)))?;

        debug!(
            endpoint = %config.endpoint,
            model = ?config.model,
            timeout = config.timeout_secs,
            "RemoteOcrProvider initialize"
        );

        Ok(Self {
            http_client,
            endpoint: config.endpoint.clone(),
            api_key,
            model: config.model.clone(),
            provider_type: config.provider_type,
            timeout_secs: config.timeout_secs,
        })
    }

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

    fn parse_generic_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        #[derive(Deserialize)]
        struct GenericResponse {
            #[serde(default)]
            results: Vec<OcrResult>,
        }

        let response: GenericResponse = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Failed to parse generic response: {}", e)))?;

        Ok(response.results)
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

        let model = self
            .model
            .as_deref()
            .unwrap_or("claude-sonnet-4-5-20250929");

        let request_body = match self.provider_type {
            AiProviderType::Google => serde_json::json!({
                "requests": [{
                    "image": { "content": encoded },
                    "features": [{
                        "type": "TEXT_DETECTION",
                        "maxResults": 64
                    }]
                }]
            }),
            AiProviderType::Anthropic | AiProviderType::OpenAi | AiProviderType::Generic => {
                serde_json::json!({
                    "model": model,
                    "max_tokens": 4096,
                    "messages": [{
                        "role": "user",
                        "content": [
                            {
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": media_type,
                                    "data": encoded
                                }
                            },
                            {
                                "type": "text",
                                "text": "List all visible text from the image line by line. Output exactly one text item per line."
                            }
                        ]
                    }]
                })
            }
        };

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

        match self.provider_type {
            AiProviderType::Anthropic => {
                builder = builder
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01");
            }
            AiProviderType::Google => {
                builder = builder.header("x-goog-api-key", &self.api_key);
            }
            AiProviderType::OpenAi | AiProviderType::Generic => {
                builder = builder.header("Authorization", format!("Bearer {}", self.api_key));
            }
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

        let results = match self.provider_type {
            AiProviderType::Anthropic => Self::parse_claude_vision_response(&body)?,
            AiProviderType::Google => Self::parse_google_vision_response(&body)?,
            AiProviderType::OpenAi | AiProviderType::Generic => {
                Self::parse_generic_response(&body)?
            }
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_remote_ocr_empty_key_error() {
        let config = ExternalApiEndpoint {
            endpoint: "https://api.example.com".to_string(),
            api_key: "".to_string(),
            model: None,
            timeout_secs: 30,
            provider_type: AiProviderType::Generic,
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
        };
        let result = RemoteOcrProvider::new(&config);
        assert!(result.is_ok());
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
    fn remote_ocr_provider_info() {
        let response = r#"{"content": [{"type": "text", "text": "test\nline2"}]}"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert_eq!(results.len(), 2);
        assert!((results[0].confidence - 0.8).abs() < f64::EPSILON);
    }
}
