//! 프레임(스크린샷) 메타데이터 모델.
//!
//! Edge 이미지 처리 파이프라인에서 사용하는 프레임 메타데이터,
//! 이미지 페이로드, 델타 영역 등을 정의.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 프레임 메타데이터 (항상 서버에 전송)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetadata {
    /// 캡처 시각
    pub timestamp: DateTime<Utc>,
    /// 캡처 트리거 유형
    pub trigger_type: String,
    /// 활성 앱 이름
    pub app_name: String,
    /// 창 제목 (PII 새니타이징 적용됨)
    pub window_title: String,
    /// 원본 해상도 (width, height)
    pub resolution: (u32, u32),
    /// 중요도 점수 (0.0 ~ 1.0)
    pub importance: f32,
}

/// 전처리된 이미지 페이로드 (조건부 전송)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImagePayload {
    /// 전체 프레임 (에러, 중요 이벤트) — WebP ~80%
    Full {
        /// Base64 인코딩된 이미지 데이터
        data: String,
        /// 이미지 포맷 (예: "webp")
        format: String,
        /// Edge OCR 추출 텍스트
        #[serde(skip_serializing_if = "Option::is_none")]
        ocr_text: Option<String>,
    },
    /// 변경 영역만 (델타) — WebP ~75%
    Delta {
        /// Base64 인코딩된 변경 영역 이미지
        data: String,
        /// 변경 영역 좌표
        region: Rect,
        /// 전체 대비 변경 비율 (0.0 ~ 1.0)
        changed_ratio: f32,
    },
    /// 썸네일 (일반 컨텍스트) — WebP ~60%
    Thumbnail {
        /// Base64 인코딩된 썸네일 이미지
        data: String,
        /// 썸네일 너비 (픽셀)
        width: u32,
        /// 썸네일 높이 (픽셀)
        height: u32,
    },
}

/// 직사각형 영역 (델타 인코딩 바운딩 박스)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 전처리 완료된 프레임 (메타 + 조건부 이미지)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedFrame {
    /// 프레임 메타데이터 (항상 포함)
    pub metadata: FrameMetadata,
    /// 전처리된 이미지 (None이면 메타만 전송)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_payload: Option<ImagePayload>,
}

/// 서버 전송용 컨텍스트 업로드 데이터
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUpload {
    /// 세션 ID
    pub session_id: String,
    /// 캡처 시각
    pub timestamp: DateTime<Utc>,
    /// 프레임 메타데이터
    pub metadata: FrameMetadata,
    /// Edge OCR 추출 텍스트
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_text: Option<String>,
    /// 전처리된 이미지
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImagePayload>,
}
