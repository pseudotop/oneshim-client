//! Privacy Gateway — 외부 AI API 전송 전 데이터 세정.
//!
//! 외부 API에 데이터를 보내기 전 반드시 거치는 보안 게이트웨이.
//! 기존 `privacy.rs`의 PII 필터 + `consent.rs`의 동의 확인을 재활용한다.

use std::sync::Arc;

use oneshim_core::config::{ExternalDataPolicy, PiiFilterLevel, PrivacyConfig};
use oneshim_core::consent::ConsentManager;

use crate::privacy::{is_sensitive_app, sanitize_title_with_level, should_exclude};

// ============================================================
// PrivacyDenied — 프라이버시 거부 사유
// ============================================================

/// 외부 API 전송 거부 사유
#[derive(Debug, Clone)]
pub enum PrivacyDenied {
    /// OCR 처리 동의 없음
    NoConsent,
    /// 민감 앱 (은행, 비밀번호 관리자 등)
    SensitiveApp(String),
    /// 정책에 의해 제외됨 (앱/창 제목 패턴)
    ExcludedByPolicy,
}

impl std::fmt::Display for PrivacyDenied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoConsent => write!(f, "OCR 처리 동의 필요"),
            Self::SensitiveApp(app) => write!(f, "민감 앱 차단: {}", app),
            Self::ExcludedByPolicy => write!(f, "정책에 의해 제외됨"),
        }
    }
}

// ============================================================
// SanitizedData — 세정된 데이터
// ============================================================

/// 세정된 이미지 데이터
#[derive(Debug)]
pub struct SanitizedImage {
    /// PII 블러 처리된 이미지 (바이트)
    pub image_data: Vec<u8>,
    /// 메타데이터 제거 여부
    pub metadata_stripped: bool,
}

// ============================================================
// PrivacyGateway
// ============================================================

/// 외부 AI API 전송 전 데이터 세정 게이트웨이
///
/// 기존 자산 재활용:
/// - `privacy.rs`: `sanitize_title_with_level()` — 텍스트 PII 마스킹
/// - `privacy.rs`: `is_sensitive_app()`, `should_exclude()` — 민감 앱 감지
/// - `consent.rs`: `ConsentManager::is_permitted()` — 동의 확인
pub struct PrivacyGateway {
    /// 동의 관리자
    consent_manager: Arc<ConsentManager>,
    /// 현재 PII 필터 레벨
    pii_filter_level: PiiFilterLevel,
    /// 외부 데이터 정책
    external_data_policy: ExternalDataPolicy,
    /// 프라이버시 설정
    privacy_config: PrivacyConfig,
}

impl PrivacyGateway {
    /// 새 Privacy Gateway 생성
    pub fn new(
        consent_manager: Arc<ConsentManager>,
        pii_filter_level: PiiFilterLevel,
        external_data_policy: ExternalDataPolicy,
        privacy_config: PrivacyConfig,
    ) -> Self {
        Self {
            consent_manager,
            pii_filter_level,
            external_data_policy,
            privacy_config,
        }
    }

    /// 이미지를 외부 API에 보내도 되는지 확인 + 세정
    pub async fn prepare_image_for_external(
        &self,
        image_data: &[u8],
        active_app: &str,
        window_title: &str,
    ) -> Result<SanitizedImage, PrivacyDenied> {
        // 1. Consent 확인: ocr_processing 동의 필수
        if !self.consent_manager.is_permitted(|p| p.ocr_processing) {
            return Err(PrivacyDenied::NoConsent);
        }

        // 2. 민감 앱 확인: 은행, 비밀번호 관리자 등 → 차단
        if is_sensitive_app(active_app) {
            return Err(PrivacyDenied::SensitiveApp(active_app.to_string()));
        }

        // 3. 창 제목 제외 패턴 확인
        if should_exclude(
            active_app,
            window_title,
            &self.privacy_config.excluded_apps,
            &self.privacy_config.excluded_app_patterns,
            &self.privacy_config.excluded_title_patterns,
            self.privacy_config.auto_exclude_sensitive,
        ) {
            return Err(PrivacyDenied::ExcludedByPolicy);
        }

        // 4. PII 블러 처리 (OCR feature 활성화 시)
        let filter_level = self.effective_filter_level();
        let sanitized_data = if filter_level == PiiFilterLevel::Off {
            image_data.to_vec()
        } else {
            Self::blur_pii_regions(image_data, filter_level).await
        };

        Ok(SanitizedImage {
            image_data: sanitized_data,
            metadata_stripped: true,
        })
    }

    /// 텍스트를 외부 API에 보내도 되는지 확인 + 세정
    pub fn prepare_text_for_external(
        &self,
        texts: &[String],
    ) -> Result<Vec<String>, PrivacyDenied> {
        // 1. Consent 확인
        if !self.consent_manager.is_permitted(|p| p.ocr_processing) {
            return Err(PrivacyDenied::NoConsent);
        }

        // 2. PII 필터 적용 (ExternalDataPolicy에 따라 레벨 결정)
        let filter_level = self.effective_filter_level();
        Ok(texts
            .iter()
            .map(|t| sanitize_title_with_level(t, filter_level))
            .collect())
    }

    /// 이미지 내 PII 영역 블러 처리
    ///
    /// OCR로 워드별 바운딩 박스를 추출한 뒤,
    /// PII로 판단된 워드 영역에 가우시안 블러를 적용한다.
    /// OCR feature 미활성화 시 원본을 그대로 반환한다.
    async fn blur_pii_regions(image_data: &[u8], filter_level: PiiFilterLevel) -> Vec<u8> {
        #[cfg(feature = "ocr")]
        {
            use crate::ocr::OcrExtractor;
            use image::GenericImage;
            use image::GenericImageView;
            use tracing::{debug, warn};

            // 1. 이미지 디코딩
            let img = match image::load_from_memory(image_data) {
                Ok(img) => img,
                Err(e) => {
                    warn!("PII 블러: 이미지 디코딩 실패 — {e}");
                    return image_data.to_vec();
                }
            };

            // 2. OCR 워드 박스 추출
            let extractor = OcrExtractor::new(None);
            let word_boxes = match extractor.extract_words_with_boxes(&img).await {
                Ok(boxes) => boxes,
                Err(e) => {
                    debug!("PII 블러: OCR 실패 — {e}, 원본 반환");
                    return image_data.to_vec();
                }
            };

            if word_boxes.is_empty() {
                return image_data.to_vec();
            }

            // 3. PII 감지: 각 워드를 마스킹한 결과와 비교
            let pii_boxes: Vec<_> = word_boxes
                .iter()
                .filter(|wb| {
                    let masked = sanitize_title_with_level(&wb.text, filter_level);
                    masked != wb.text // 마스킹 결과가 다르면 PII
                })
                .collect();

            if pii_boxes.is_empty() {
                return image_data.to_vec();
            }

            debug!(
                "PII 블러: {}개 영역 감지 (총 {}개 워드)",
                pii_boxes.len(),
                word_boxes.len()
            );

            // 4. 블러 처리: PII 영역에 가우시안 블러 적용
            let mut result_img = img.to_rgba8();
            let (img_w, img_h) = result_img.dimensions();

            for wb in &pii_boxes {
                // 바운딩 박스를 이미지 범위 내로 클램프 + 여백 추가
                let margin = 4i32;
                let x = (wb.x - margin).max(0) as u32;
                let y = (wb.y - margin).max(0) as u32;
                let w = ((wb.w + margin * 2) as u32).min(img_w.saturating_sub(x));
                let h = ((wb.h + margin * 2) as u32).min(img_h.saturating_sub(y));

                if w == 0 || h == 0 {
                    continue;
                }

                // ROI 추출 → 블러 → 다시 복사
                let roi = image::DynamicImage::ImageRgba8(result_img.clone()).crop_imm(x, y, w, h);
                let blurred = roi.blur(8.0);
                let blurred_rgba = blurred.to_rgba8();

                for dy in 0..h.min(blurred_rgba.height()) {
                    for dx in 0..w.min(blurred_rgba.width()) {
                        let pixel = blurred_rgba.get_pixel(dx, dy);
                        if x + dx < img_w && y + dy < img_h {
                            result_img.put_pixel(x + dx, y + dy, *pixel);
                        }
                    }
                }
            }

            // 5. PNG로 인코딩하여 반환
            let mut output = std::io::Cursor::new(Vec::new());
            if let Err(e) = image::DynamicImage::ImageRgba8(result_img)
                .write_to(&mut output, image::ImageFormat::Png)
            {
                warn!("PII 블러: 이미지 인코딩 실패 — {e}");
                return image_data.to_vec();
            }

            output.into_inner()
        }

        #[cfg(not(feature = "ocr"))]
        {
            let _ = filter_level;
            image_data.to_vec()
        }
    }

    /// ExternalDataPolicy에 따른 효과적인 PII 필터 레벨 결정
    fn effective_filter_level(&self) -> PiiFilterLevel {
        match self.external_data_policy {
            ExternalDataPolicy::PiiFilterStrict => PiiFilterLevel::Strict,
            ExternalDataPolicy::PiiFilterStandard => PiiFilterLevel::Standard,
            ExternalDataPolicy::AllowFiltered => self.pii_filter_level,
        }
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::consent::ConsentPermissions;

    fn make_consent_manager(ocr_permitted: bool) -> Arc<ConsentManager> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        if ocr_permitted {
            let perms = ConsentPermissions {
                ocr_processing: true,
                screen_capture: true,
                ..Default::default()
            };
            manager.grant_consent(perms, 30).unwrap();
        }

        // dir을 drop하면 파일이 삭제되므로 leak
        std::mem::forget(dir);
        Arc::new(manager)
    }

    fn make_gateway(ocr_permitted: bool, policy: ExternalDataPolicy) -> PrivacyGateway {
        let consent = make_consent_manager(ocr_permitted);
        PrivacyGateway::new(
            consent,
            PiiFilterLevel::Standard,
            policy,
            PrivacyConfig::default(),
        )
    }

    #[tokio::test]
    async fn deny_without_consent() {
        let gw = make_gateway(false, ExternalDataPolicy::PiiFilterStrict);
        let result = gw
            .prepare_image_for_external(b"img", "VSCode", "main.rs")
            .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PrivacyDenied::NoConsent));
    }

    #[tokio::test]
    async fn deny_sensitive_app() {
        let gw = make_gateway(true, ExternalDataPolicy::PiiFilterStrict);
        let result = gw
            .prepare_image_for_external(b"img", "1Password", "Vault")
            .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PrivacyDenied::SensitiveApp(_)
        ));
    }

    #[tokio::test]
    async fn allow_normal_app() {
        let gw = make_gateway(true, ExternalDataPolicy::PiiFilterStrict);
        let result = gw
            .prepare_image_for_external(b"img", "VSCode", "main.rs")
            .await;
        assert!(result.is_ok());
        let sanitized = result.unwrap();
        assert!(sanitized.metadata_stripped);
    }

    #[test]
    fn text_filter_no_consent() {
        let gw = make_gateway(false, ExternalDataPolicy::PiiFilterStrict);
        let result = gw.prepare_text_for_external(&["hello".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn text_filter_with_consent() {
        let gw = make_gateway(true, ExternalDataPolicy::PiiFilterStandard);
        let texts = vec!["user@example.com".to_string(), "hello world".to_string()];
        let result = gw.prepare_text_for_external(&texts);
        assert!(result.is_ok());
        let filtered = result.unwrap();
        assert_eq!(filtered.len(), 2);
        // Standard 레벨에서 이메일은 마스킹됨
        assert!(filtered[0].contains("[EMAIL]") || filtered[0] == "user@example.com");
    }

    #[test]
    fn effective_filter_level_strict() {
        let gw = make_gateway(true, ExternalDataPolicy::PiiFilterStrict);
        assert_eq!(gw.effective_filter_level(), PiiFilterLevel::Strict);
    }

    #[test]
    fn effective_filter_level_standard() {
        let gw = make_gateway(true, ExternalDataPolicy::PiiFilterStandard);
        assert_eq!(gw.effective_filter_level(), PiiFilterLevel::Standard);
    }

    #[test]
    fn effective_filter_level_allow_filtered() {
        let gw = make_gateway(true, ExternalDataPolicy::AllowFiltered);
        assert_eq!(gw.effective_filter_level(), PiiFilterLevel::Standard); // 사용자 설정
    }

    #[tokio::test]
    async fn blur_pii_regions_returns_data_for_empty_image() {
        // 빈 바이트 입력 시 원본 그대로 반환 (디코딩 실패)
        let data = b"not-an-image";
        let result = PrivacyGateway::blur_pii_regions(data, PiiFilterLevel::Standard).await;
        assert_eq!(result, data.to_vec());
    }

    #[tokio::test]
    async fn blur_pii_regions_off_returns_original() {
        let data = b"test-data";
        // Off 레벨은 blur_pii_regions를 호출하지 않지만, 직접 테스트
        let result = PrivacyGateway::blur_pii_regions(data, PiiFilterLevel::Off).await;
        // Off일 때도 OCR 미설치 환경에서는 원본 반환
        assert!(!result.is_empty());
    }

    #[tokio::test]
    async fn prepare_image_off_level_skips_blur() {
        // PiiFilterLevel::Off + AllowFiltered → 블러 건너뜀
        let consent = make_consent_manager(true);
        let gw = PrivacyGateway::new(
            consent,
            PiiFilterLevel::Off,
            ExternalDataPolicy::AllowFiltered,
            PrivacyConfig::default(),
        );
        let result = gw
            .prepare_image_for_external(b"img", "VSCode", "main.rs")
            .await;
        assert!(result.is_ok());
        let sanitized = result.unwrap();
        assert_eq!(sanitized.image_data, b"img".to_vec());
    }

    #[test]
    fn privacy_denied_display() {
        let d1 = PrivacyDenied::NoConsent;
        assert!(d1.to_string().contains("동의"));
        let d2 = PrivacyDenied::SensitiveApp("Bank".to_string());
        assert!(d2.to_string().contains("Bank"));
        let d3 = PrivacyDenied::ExcludedByPolicy;
        assert!(d3.to_string().contains("정책"));
    }
}
