//! ONESHIM 핵심 에러 타입.
//!
//! 모든 어댑터 crate는 자체 에러 타입에서 `#[from] CoreError`로 래핑한다.

use thiserror::Error;

/// 코어 레이어 에러.
/// 직렬화, 설정, 유효성 검증 등 도메인 공통 에러를 정의한다.
#[derive(Debug, Error)]
pub enum CoreError {
    /// JSON 직렬화/역직렬화 실패
    #[error("직렬화 에러: {0}")]
    Serialization(#[from] serde_json::Error),

    /// 설정값 오류
    #[error("설정 에러: {0}")]
    Config(String),

    /// 필드 유효성 검증 실패
    #[error("유효성 검증 실패 — {field}: {message}")]
    Validation {
        /// 검증 실패한 필드명
        field: String,
        /// 실패 사유
        message: String,
    },

    /// 인증 실패 (토큰 만료, 자격증명 오류 등)
    #[error("인증 에러: {0}")]
    Auth(String),

    /// 리소스를 찾을 수 없음
    #[error("{resource_type} 미발견: {id}")]
    NotFound {
        /// 리소스 종류 (예: "Session", "Suggestion")
        resource_type: String,
        /// 리소스 식별자
        id: String,
    },

    /// 내부 에러 (예상치 못한 상황)
    #[error("내부 에러: {0}")]
    Internal(String),

    /// 네트워크 에러 (연결 실패, 타임아웃)
    #[error("네트워크 에러: {0}")]
    Network(String),

    /// Rate Limit 초과 (429)
    #[error("요청 한도 초과, {retry_after_secs}초 후 재시도")]
    RateLimit {
        /// 재시도 대기 시간 (초)
        retry_after_secs: u64,
    },

    /// 서비스 일시 불가 (503)
    #[error("서비스 일시 불가: {0}")]
    ServiceUnavailable(String),

    /// 정책에 의해 거부됨 (자동화 명령 등)
    #[error("정책 거부: {0}")]
    PolicyDenied(String),

    /// 허가되지 않은 프로세스 실행 시도
    #[error("허가되지 않은 프로세스: {0}")]
    ProcessNotAllowed(String),

    /// 잘못된 인자 (정책 검증 실패)
    #[error("잘못된 인자: {0}")]
    InvalidArguments(String),

    /// 바이너리 해시 불일치 (변조 감지)
    #[error("바이너리 해시 불일치: expected={expected}, actual={actual}")]
    BinaryHashMismatch {
        /// 예상 해시값
        expected: String,
        /// 실제 해시값
        actual: String,
    },

    /// 동의가 필요함 (GDPR/EU AI Act)
    #[error("동의 필요: {0}")]
    ConsentRequired(String),

    /// 동의 만료
    #[error("동의 만료 — 재동의 필요")]
    ConsentExpired,

    /// I/O 에러
    #[error("I/O 에러: {0}")]
    Io(#[from] std::io::Error),

    /// 샌드박스 초기화 실패
    #[error("샌드박스 초기화 실패: {0}")]
    SandboxInit(String),

    /// 샌드박스 실행 실패
    #[error("샌드박스 실행 실패: {0}")]
    SandboxExecution(String),

    /// 샌드박스 미지원 플랫폼
    #[error("샌드박스 미지원 플랫폼: {0}")]
    SandboxUnsupported(String),

    /// 실행 타임아웃
    #[error("실행 타임아웃: {timeout_ms}ms 초과")]
    ExecutionTimeout {
        /// 초과된 타임아웃 시간 (밀리초)
        timeout_ms: u64,
    },

    /// UI 요소를 찾을 수 없음
    #[error("UI 요소 미발견: {0}")]
    ElementNotFound(String),

    /// 프라이버시 정책에 의해 거부됨 (외부 API 전송 차단)
    #[error("프라이버시 거부: {0}")]
    PrivacyDenied(String),

    /// OCR 처리 실패
    #[error("OCR 에러: {0}")]
    OcrError(String),
}
