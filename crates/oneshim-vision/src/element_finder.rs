//! UI 요소 탐색기 구현.
//!
//! OCR 기반 요소 탐색 + 전략 체인(ChainedElementFinder)을 제공한다.
//! Phase 2 구현: `OcrElementFinder`, `ChainedElementFinder`

use std::sync::Arc;

use async_trait::async_trait;
use tracing::debug;

use oneshim_core::error::CoreError;
use oneshim_core::models::intent::{ElementBounds, FinderSource, UiElement};
use oneshim_core::ports::element_finder::ElementFinder;
use oneshim_core::ports::ocr_provider::{OcrProvider, OcrResult};

// ============================================================
// OcrElementFinder — OCR 기반 UI 요소 탐색
// ============================================================

/// OCR 기반 UI 요소 탐색기
///
/// 화면 캡처 → OCR 추출 → 텍스트 매칭 → UiElement 변환
pub struct OcrElementFinder {
    /// OCR 제공자 (로컬 또는 원격)
    ocr_provider: Arc<dyn OcrProvider>,
    /// 화면 캡처 데이터 (이미지 바이트 + 형식)
    /// 실제 사용 시 ScreenCapture와 연동하여 최신 이미지를 제공받음
    last_image: tokio::sync::RwLock<Option<(Vec<u8>, String)>>,
}

impl OcrElementFinder {
    /// 새 OCR 기반 탐색기 생성
    pub fn new(ocr_provider: Arc<dyn OcrProvider>) -> Self {
        Self {
            ocr_provider,
            last_image: tokio::sync::RwLock::new(None),
        }
    }

    /// 최신 캡처 이미지 설정 (외부에서 주입)
    pub async fn set_image(&self, image_data: Vec<u8>, format: String) {
        let mut img = self.last_image.write().await;
        *img = Some((image_data, format));
    }

    /// OCR 결과 → UiElement 변환 (텍스트 매칭 + 영역 필터)
    fn ocr_to_elements(
        results: &[OcrResult],
        text_query: Option<&str>,
        _role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Vec<UiElement> {
        results
            .iter()
            .filter(|r| {
                // 영역 필터
                if let Some(bounds) = region {
                    let in_region = r.x >= bounds.x
                        && r.y >= bounds.y
                        && (r.x + r.width as i32) <= (bounds.x + bounds.width as i32)
                        && (r.y + r.height as i32) <= (bounds.y + bounds.height as i32);
                    if !in_region {
                        return false;
                    }
                }
                true
            })
            .filter(|r| {
                // 텍스트 매칭 (부분 문자열)
                if let Some(query) = text_query {
                    let query_lower = query.to_lowercase();
                    let text_lower = r.text.to_lowercase();
                    text_lower.contains(&query_lower)
                } else {
                    true
                }
            })
            .map(|r| {
                // 텍스트 유사도 기반 신뢰도 조정
                let text_confidence = if let Some(query) = text_query {
                    text_similarity(&r.text, query)
                } else {
                    1.0
                };
                let combined_confidence = r.confidence * text_confidence;

                UiElement {
                    text: r.text.clone(),
                    bounds: ElementBounds {
                        x: r.x,
                        y: r.y,
                        width: r.width,
                        height: r.height,
                    },
                    role: None,
                    confidence: combined_confidence,
                    source: FinderSource::Ocr,
                }
            })
            .collect()
    }
}

#[async_trait]
impl ElementFinder for OcrElementFinder {
    async fn find_element(
        &self,
        text: Option<&str>,
        role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Result<Vec<UiElement>, CoreError> {
        let image_guard = self.last_image.read().await;
        let (image_data, image_format) = image_guard
            .as_ref()
            .ok_or_else(|| CoreError::Internal("OCR 탐색기: 캡처 이미지가 없습니다".to_string()))?;

        debug!(
            provider = self.ocr_provider.provider_name(),
            text = ?text,
            role = ?role,
            "OCR 요소 탐색 시작"
        );

        let ocr_results = self
            .ocr_provider
            .extract_elements(image_data, image_format)
            .await?;

        let mut elements = Self::ocr_to_elements(&ocr_results, text, role, region);

        // 신뢰도 내림차순 정렬
        elements.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(count = elements.len(), "OCR 요소 탐색 완료");

        Ok(elements)
    }

    fn name(&self) -> &str {
        "ocr"
    }
}

// ============================================================
// ChainedElementFinder — 전략 체인
// ============================================================

/// 전략 체인 요소 탐색기
///
/// 여러 탐색기를 순차적으로 시도하여 첫 번째 성공 결과를 반환한다.
/// 예: OCR → 접근성 API → 템플릿 매칭
pub struct ChainedElementFinder {
    finders: Vec<Box<dyn ElementFinder>>,
}

impl ChainedElementFinder {
    /// 전략 체인 생성
    pub fn new(finders: Vec<Box<dyn ElementFinder>>) -> Self {
        Self { finders }
    }
}

#[async_trait]
impl ElementFinder for ChainedElementFinder {
    async fn find_element(
        &self,
        text: Option<&str>,
        role: Option<&str>,
        region: Option<&ElementBounds>,
    ) -> Result<Vec<UiElement>, CoreError> {
        for finder in &self.finders {
            debug!(finder = finder.name(), "체인 탐색기 시도");
            match finder.find_element(text, role, region).await {
                Ok(elements) if !elements.is_empty() => {
                    debug!(
                        finder = finder.name(),
                        count = elements.len(),
                        "체인 탐색기 성공"
                    );
                    return Ok(elements);
                }
                Ok(_) => {
                    debug!(finder = finder.name(), "결과 없음, 다음 탐색기 시도");
                }
                Err(e) => {
                    debug!(finder = finder.name(), error = %e, "탐색 실패, 다음 탐색기 시도");
                }
            }
        }
        Err(CoreError::ElementNotFound(format!(
            "모든 탐색기에서 요소를 찾지 못함 (text={:?}, role={:?})",
            text, role
        )))
    }

    fn name(&self) -> &str {
        "chained"
    }
}

// ============================================================
// 유틸리티 — 텍스트 유사도
// ============================================================

/// 간단한 텍스트 유사도 (0.0 ~ 1.0)
///
/// 정확히 일치하면 1.0, 부분 일치하면 길이 비율, 불일치면 0.0
fn text_similarity(text: &str, query: &str) -> f64 {
    let text_lower = text.to_lowercase();
    let query_lower = query.to_lowercase();

    if text_lower == query_lower {
        1.0
    } else if text_lower.contains(&query_lower) {
        query_lower.len() as f64 / text_lower.len() as f64
    } else {
        0.0
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_similarity_exact_match() {
        assert!((text_similarity("저장", "저장") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn text_similarity_partial_match() {
        let sim = text_similarity("파일 저장", "저장");
        assert!(sim > 0.0);
        assert!(sim < 1.0);
    }

    #[test]
    fn text_similarity_no_match() {
        assert!((text_similarity("저장", "열기")).abs() < f64::EPSILON);
    }

    #[test]
    fn text_similarity_case_insensitive() {
        assert!((text_similarity("Save", "save") - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ocr_to_elements_text_filter() {
        let results = vec![
            OcrResult {
                text: "파일".to_string(),
                x: 0,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.9,
            },
            OcrResult {
                text: "저장".to_string(),
                x: 50,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.85,
            },
            OcrResult {
                text: "편집".to_string(),
                x: 100,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.88,
            },
        ];

        let elements = OcrElementFinder::ocr_to_elements(&results, Some("저장"), None, None);
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].text, "저장");
    }

    #[test]
    fn ocr_to_elements_region_filter() {
        let results = vec![
            OcrResult {
                text: "파일".to_string(),
                x: 0,
                y: 0,
                width: 40,
                height: 20,
                confidence: 0.9,
            },
            OcrResult {
                text: "저장".to_string(),
                x: 200,
                y: 200,
                width: 40,
                height: 20,
                confidence: 0.85,
            },
        ];

        let region = ElementBounds {
            x: 0,
            y: 0,
            width: 100,
            height: 50,
        };
        let elements = OcrElementFinder::ocr_to_elements(&results, None, None, Some(&region));
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].text, "파일");
    }

    #[test]
    fn ocr_to_elements_all_filtered() {
        let results = vec![OcrResult {
            text: "파일".to_string(),
            x: 0,
            y: 0,
            width: 40,
            height: 20,
            confidence: 0.9,
        }];

        let elements = OcrElementFinder::ocr_to_elements(&results, Some("없는텍스트"), None, None);
        assert!(elements.is_empty());
    }

    #[test]
    fn ocr_to_elements_no_query_returns_all() {
        let results = vec![
            OcrResult {
                text: "A".to_string(),
                x: 0,
                y: 0,
                width: 20,
                height: 20,
                confidence: 0.8,
            },
            OcrResult {
                text: "B".to_string(),
                x: 30,
                y: 0,
                width: 20,
                height: 20,
                confidence: 0.9,
            },
        ];

        let elements = OcrElementFinder::ocr_to_elements(&results, None, None, None);
        assert_eq!(elements.len(), 2);
    }

    /// ChainedElementFinder 테스트용 Mock
    struct MockFinder {
        name: String,
        results: Vec<UiElement>,
    }

    #[async_trait]
    impl ElementFinder for MockFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Ok(self.results.clone())
        }
        fn name(&self) -> &str {
            &self.name
        }
    }

    struct FailingFinder;

    #[async_trait]
    impl ElementFinder for FailingFinder {
        async fn find_element(
            &self,
            _text: Option<&str>,
            _role: Option<&str>,
            _region: Option<&ElementBounds>,
        ) -> Result<Vec<UiElement>, CoreError> {
            Err(CoreError::Internal("탐색 실패".to_string()))
        }
        fn name(&self) -> &str {
            "failing"
        }
    }

    #[tokio::test]
    async fn chained_finder_returns_first_success() {
        let empty_finder = MockFinder {
            name: "empty".to_string(),
            results: vec![],
        };
        let success_finder = MockFinder {
            name: "success".to_string(),
            results: vec![UiElement {
                text: "저장".to_string(),
                bounds: ElementBounds {
                    x: 0,
                    y: 0,
                    width: 40,
                    height: 20,
                },
                role: None,
                confidence: 0.9,
                source: FinderSource::Ocr,
            }],
        };

        let chained =
            ChainedElementFinder::new(vec![Box::new(empty_finder), Box::new(success_finder)]);

        let result = chained
            .find_element(Some("저장"), None, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "저장");
    }

    #[tokio::test]
    async fn chained_finder_skips_failing() {
        let success_finder = MockFinder {
            name: "success".to_string(),
            results: vec![UiElement {
                text: "확인".to_string(),
                bounds: ElementBounds {
                    x: 10,
                    y: 10,
                    width: 50,
                    height: 25,
                },
                role: Some("button".to_string()),
                confidence: 0.85,
                source: FinderSource::Accessibility,
            }],
        };

        let chained =
            ChainedElementFinder::new(vec![Box::new(FailingFinder), Box::new(success_finder)]);

        let result = chained
            .find_element(Some("확인"), None, None)
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn chained_finder_all_fail_returns_error() {
        let chained = ChainedElementFinder::new(vec![Box::new(FailingFinder)]);

        let result = chained.find_element(Some("없음"), None, None).await;
        assert!(result.is_err());
    }
}
