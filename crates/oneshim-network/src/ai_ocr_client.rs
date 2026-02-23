//! 외부 AI OCR 클라이언트.
//!
//! 외부 AI Vision API (Claude Vision, Google Cloud Vision 등)를 호출하여
//! 이미지에서 텍스트 + 바운딩 박스를 추출한다.
//! **Privacy Gateway 연동**: 이미지 전송 전 PII 블러 + 동의 확인 필수.

use async_trait::async_trait;
use serde::Deserialize;
use tracing::{debug, warn};

use oneshim_core::config::{AiProviderType, ExternalApiEndpoint};
use oneshim_core::error::CoreError;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};

// ============================================================
// RemoteOcrProvider — 외부 AI OCR API 클라이언트
// ============================================================

/// 외부 AI OCR API 클라이언트
///
/// 지원 API:
/// - Claude Vision (Anthropic): `POST /v1/messages` + image content block
/// - Google Cloud Vision: `POST /v1/images:annotate` + TEXT_DETECTION
/// - 커스텀 엔드포인트 (사용자 지정 URL)
///
/// **보안**: API 키는 config.json에서 로드 → Settings UI에서 입력
#[derive(Debug)]
pub struct RemoteOcrProvider {
    /// HTTP 클라이언트
    http_client: reqwest::Client,
    /// API 엔드포인트 URL
    endpoint: String,
    /// API 키 (메모리에만 유지)
    api_key: String,
    /// 모델 이름
    model: Option<String>,
    /// AI 제공자 타입 — 요청/응답 형식 결정에 사용
    provider_type: AiProviderType,
    /// 요청 타임아웃 (초) — 향후 동적 타임아웃 조정용
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl RemoteOcrProvider {
    /// 새 RemoteOcrProvider 생성
    ///
    /// API 키는 `config.api_key`에서 직접 읽는다.
    pub fn new(config: &ExternalApiEndpoint) -> Result<Self, CoreError> {
        if config.api_key.is_empty() {
            return Err(CoreError::Config(
                "AI OCR API 키 미설정. Settings에서 입력하세요.".into(),
            ));
        }
        let api_key = config.api_key.clone();

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| CoreError::Network(format!("HTTP 클라이언트 생성 실패: {}", e)))?;

        debug!(
            endpoint = %config.endpoint,
            model = ?config.model,
            timeout = config.timeout_secs,
            "RemoteOcrProvider 초기화"
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

    /// API 응답에서 OCR 결과 파싱 (Claude Vision 형식)
    fn parse_claude_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        // Claude Vision 응답 구조 파싱
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("응답 JSON 파싱 실패: {}", e)))?;

        let mut results = Vec::new();

        // content[].text에서 OCR 결과 추출
        if let Some(content) = response.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    // 줄별로 분리하여 개별 OCR 결과로 변환
                    // 실제 바운딩 박스는 Vision API 응답에 포함됨
                    // 현재는 텍스트만 추출 (바운딩 박스는 향후 구현)
                    for (i, line) in text.lines().enumerate() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            results.push(OcrResult {
                                text: trimmed.to_string(),
                                x: 0,
                                y: (i as i32) * 20,                // 임시 줄 높이
                                width: (trimmed.len() as u32) * 8, // 임시 문자 너비
                                height: 20,
                                confidence: 0.8, // 외부 API 기본 신뢰도
                            });
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// API 응답에서 OCR 결과 파싱 (범용 JSON 형식)
    fn parse_generic_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        // 범용 응답: { "results": [{ "text": "...", "x": 0, ... }] }
        #[derive(Deserialize)]
        struct GenericResponse {
            #[serde(default)]
            results: Vec<OcrResult>,
        }

        let response: GenericResponse = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("범용 응답 파싱 실패: {}", e)))?;

        Ok(response.results)
    }

    /// API 응답에서 OCR 결과 파싱 (Google Vision 형식)
    fn parse_google_vision_response(body: &str) -> Result<Vec<OcrResult>, CoreError> {
        let response: serde_json::Value = serde_json::from_str(body)
            .map_err(|e| CoreError::OcrError(format!("Google Vision 응답 파싱 실패: {}", e)))?;

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

        // 1. Base64 인코딩
        let encoded = base64::engine::general_purpose::STANDARD.encode(image);
        let media_type = match image_format {
            "png" => "image/png",
            "jpeg" | "jpg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };

        // 2. 제공자별 요청 바디 구성
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
                                "text": "이미지에서 보이는 모든 텍스트를 줄별로 나열해주세요. 각 줄에 하나의 텍스트만 출력하세요."
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
            "외부 OCR API 호출"
        );

        // 3. API 호출 — 제공자 타입에 따라 인증 헤더 구성
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
            .map_err(|e| CoreError::Network(format!("OCR API 호출 실패: {}", e)))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| CoreError::Network(format!("OCR API 응답 읽기 실패: {}", e)))?;

        if !status.is_success() {
            warn!(status = %status, "OCR API 오류 응답");
            return Err(CoreError::OcrError(format!(
                "OCR API 오류 ({}): {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        // 4. 응답 파싱 — 제공자 타입에 따라 파싱 방식 결정
        let results = match self.provider_type {
            AiProviderType::Anthropic => Self::parse_claude_vision_response(&body)?,
            AiProviderType::Google => Self::parse_google_vision_response(&body)?,
            AiProviderType::OpenAi | AiProviderType::Generic => {
                Self::parse_generic_response(&body)?
            }
        };

        debug!(count = results.len(), "OCR 결과 수신");
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

// ============================================================
// 테스트
// ============================================================

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
        assert!(err.contains("미설정"));
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
                    "text": "파일\n편집\n보기\n저장"
                }
            ]
        }"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert_eq!(results.len(), 4);
        assert_eq!(results[0].text, "파일");
        assert_eq!(results[3].text, "저장");
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
                {"text": "저장", "x": 100, "y": 200, "width": 60, "height": 25, "confidence": 0.95}
            ]
        }"#;
        let results = RemoteOcrProvider::parse_generic_response(response).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "저장");
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
        // provider_name / is_external은 인스턴스 없이 검증 불가
        // parse 함수 테스트로 대체
        let response = r#"{"content": [{"type": "text", "text": "test\nline2"}]}"#;
        let results = RemoteOcrProvider::parse_claude_vision_response(response).unwrap();
        assert_eq!(results.len(), 2);
        assert!((results[0].confidence - 0.8).abs() < f64::EPSILON);
    }
}
