//! 애플리케이션 설정 구조체.
//!
//! 서버 URL, 모니터링 주기, 저장소 경로, 프라이버시/텔레메트리/스케줄 설정 등
//! 런타임 설정을 정의한다. `config` crate를 통해 파일/환경변수에서 로드.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// 최상위 애플리케이션 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 서버 연결 설정
    pub server: ServerConfig,
    /// 모니터링 설정
    pub monitor: MonitorConfig,
    /// 로컬 저장소 설정
    pub storage: StorageConfig,
    /// 비전(이미지 처리) 설정
    pub vision: VisionConfig,
    /// 자동 업데이트 설정
    #[serde(default)]
    pub update: UpdateConfig,
    /// 웹 대시보드 설정
    #[serde(default)]
    pub web: WebConfig,
    /// 알림 설정
    #[serde(default)]
    pub notification: NotificationConfig,
    /// gRPC 설정
    #[serde(default)]
    pub grpc: GrpcConfig,
    /// 텔레메트리 설정
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// 프라이버시 설정
    #[serde(default)]
    pub privacy: PrivacyConfig,
    /// 스케줄 설정 (활동 시간대)
    #[serde(default)]
    pub schedule: ScheduleConfig,
    /// 파일 접근 모니터링 설정
    #[serde(default)]
    pub file_access: FileAccessConfig,
    /// 자동화 설정
    #[serde(default)]
    pub automation: AutomationConfig,
    /// AI 제공자 설정 (OCR/LLM)
    #[serde(default)]
    pub ai_provider: AiProviderConfig,
}

// ============================================================
// 텔레메트리 설정
// ============================================================

/// 텔레메트리 설정 — 서버로 전송되는 원격 측정 데이터 제어
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// 텔레메트리 전체 활성화 여부
    #[serde(default)]
    pub enabled: bool,
    /// 크래시 리포트 전송
    #[serde(default)]
    pub crash_reports: bool,
    /// 사용 통계 전송 (기능 사용 빈도 등)
    #[serde(default)]
    pub usage_analytics: bool,
    /// 성능 메트릭 전송 (CPU/메모리 사용량 등)
    #[serde(default)]
    pub performance_metrics: bool,
}

// Default: 모든 필드 false (derive로 자동 생성)

// ============================================================
// 프라이버시 설정
// ============================================================

/// PII 필터 레벨
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiFilterLevel {
    /// 필터 없음
    Off,
    /// 이메일, 전화번호만 필터
    Basic,
    /// + 신용카드, 주민번호
    #[default]
    Standard,
    /// + API 키, IP 주소, 사용자 경로
    Strict,
}

/// 프라이버시 설정 — 앱 블랙리스트, 창 제목 필터, PII 보호
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyConfig {
    /// 제외할 앱 이름 목록 (정확한 이름)
    #[serde(default)]
    pub excluded_apps: Vec<String>,
    /// 제외할 앱 이름 패턴 (glob: "*bank*", "*wallet*")
    #[serde(default)]
    pub excluded_app_patterns: Vec<String>,
    /// 제외할 창 제목 패턴
    #[serde(default)]
    pub excluded_title_patterns: Vec<String>,
    /// 민감 앱 자동 감지 (은행, 비밀번호 관리자 등)
    #[serde(default = "default_true")]
    pub auto_exclude_sensitive: bool,
    /// PII 필터 레벨
    #[serde(default)]
    pub pii_filter_level: PiiFilterLevel,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            excluded_apps: Vec::new(),
            excluded_app_patterns: Vec::new(),
            excluded_title_patterns: Vec::new(),
            auto_exclude_sensitive: true,
            pii_filter_level: PiiFilterLevel::Standard,
        }
    }
}

// ============================================================
// 스케줄 설정
// ============================================================

/// 요일
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// 스케줄 설정 — 활동 시간대 제한
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// 활동 시간대 제한 활성화
    #[serde(default)]
    pub active_hours_enabled: bool,
    /// 활동 시작 시간 (0-23)
    #[serde(default = "default_active_start_hour")]
    pub active_start_hour: u8,
    /// 활동 종료 시간 (0-23)
    #[serde(default = "default_active_end_hour")]
    pub active_end_hour: u8,
    /// 활동 요일 목록
    #[serde(default = "default_active_days")]
    pub active_days: Vec<Weekday>,
    /// 화면 잠금 시 일시정지
    #[serde(default = "default_true")]
    pub pause_on_screen_lock: bool,
    /// 배터리 세이버 시 일시정지
    #[serde(default)]
    pub pause_on_battery_saver: bool,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self {
            active_hours_enabled: false,
            active_start_hour: default_active_start_hour(),
            active_end_hour: default_active_end_hour(),
            active_days: default_active_days(),
            pause_on_screen_lock: true,
            pause_on_battery_saver: false,
        }
    }
}

fn default_active_start_hour() -> u8 {
    9
}

fn default_active_end_hour() -> u8 {
    18
}

fn default_active_days() -> Vec<Weekday> {
    vec![
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
    ]
}

// ============================================================
// 파일 접근 모니터링 설정
// ============================================================

/// 파일 접근 모니터링 설정 — 화이트리스트 기반 폴더 모니터링
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessConfig {
    /// 파일 접근 모니터링 활성화
    #[serde(default)]
    pub enabled: bool,
    /// 모니터링 대상 폴더 (화이트리스트)
    #[serde(default)]
    pub monitored_folders: Vec<PathBuf>,
    /// 제외할 파일 확장자
    #[serde(default = "default_excluded_extensions")]
    pub excluded_extensions: Vec<String>,
    /// 분당 최대 이벤트 수 (레이트 리밋)
    #[serde(default = "default_max_events_per_minute")]
    pub max_events_per_minute: u32,
}

impl Default for FileAccessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            monitored_folders: Vec::new(),
            excluded_extensions: default_excluded_extensions(),
            max_events_per_minute: default_max_events_per_minute(),
        }
    }
}

// ============================================================
// 자동화 설정
// ============================================================

/// 자동화 설정 — 샌드박스 포함
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutomationConfig {
    /// 자동화 기능 활성화 여부
    #[serde(default)]
    pub enabled: bool,
    /// 샌드박스 설정
    #[serde(default)]
    pub sandbox: SandboxConfig,
    /// 사용자 정의 워크플로우 프리셋
    #[serde(default)]
    pub custom_presets: Vec<crate::models::intent::WorkflowPreset>,
}

/// 샌드박스 프로필
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxProfile {
    /// 최소 제한: 파일시스템 읽기 허용, 네트워크 허용
    Permissive,
    /// 표준: 지정 경로만 읽기, 네트워크 차단
    #[default]
    Standard,
    /// 엄격: 최소 경로만, 네트워크 차단, 리소스 제한
    Strict,
}

/// 샌드박스 설정 — OS 네이티브 커널 격리
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// 샌드박스 활성화 여부
    #[serde(default)]
    pub enabled: bool,
    /// 샌드박스 프로필 (Permissive / Standard / Strict)
    #[serde(default)]
    pub profile: SandboxProfile,
    /// 파일시스템 허용 경로 (읽기 전용)
    #[serde(default)]
    pub allowed_read_paths: Vec<String>,
    /// 파일시스템 허용 경로 (읽기/쓰기)
    #[serde(default)]
    pub allowed_write_paths: Vec<String>,
    /// 네트워크 접근 허용 여부
    #[serde(default)]
    pub allow_network: bool,
    /// 최대 메모리 (bytes, 0 = 무제한)
    #[serde(default)]
    pub max_memory_bytes: u64,
    /// 최대 CPU 시간 (ms, 0 = 무제한)
    #[serde(default)]
    pub max_cpu_time_ms: u64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: SandboxProfile::Standard,
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            allow_network: false,
            max_memory_bytes: 0,
            max_cpu_time_ms: 0,
        }
    }
}

// ============================================================
// AI 제공자 설정
// ============================================================

/// AI 제공자 설정 — OCR/LLM 제공자 타입 및 외부 API 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    /// OCR 제공자 타입
    #[serde(default)]
    pub ocr_provider: OcrProviderType,
    /// LLM 제공자 타입
    #[serde(default)]
    pub llm_provider: LlmProviderType,
    /// 외부 OCR API 설정 (ocr_provider=Remote일 때)
    #[serde(default)]
    pub ocr_api: Option<ExternalApiEndpoint>,
    /// 외부 LLM API 설정 (llm_provider=Remote일 때)
    #[serde(default)]
    pub llm_api: Option<ExternalApiEndpoint>,
    /// 외부 API 전송 전 데이터 정책
    #[serde(default)]
    pub external_data_policy: ExternalDataPolicy,
    /// 외부 API 실패 시 로컬 폴백
    #[serde(default = "default_true")]
    pub fallback_to_local: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            ocr_provider: OcrProviderType::default(),
            llm_provider: LlmProviderType::default(),
            ocr_api: None,
            llm_api: None,
            external_data_policy: ExternalDataPolicy::default(),
            fallback_to_local: true,
        }
    }
}

/// OCR 제공자 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OcrProviderType {
    /// 로컬 Tesseract (기본값)
    #[default]
    Local,
    /// 외부 AI OCR API (Claude Vision, Google Vision 등)
    Remote,
}

/// LLM 제공자 타입
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LlmProviderType {
    /// 로컬 LLM / 규칙 기반 매칭 (기본값)
    #[default]
    Local,
    /// 외부 AI API (Claude, GPT 등)
    Remote,
}

// ============================================================
// AI API 제공자 타입
// ============================================================

/// AI API 제공자 타입 — URL 문자열 매칭 대신 명시적 enum으로 제공자 구분
///
/// OSS 아키텍처에서 특정 벤더가 특권을 갖지 않도록 config 주도 방식으로 설계.
/// 새 제공자 추가 시 이 enum에 variant를 추가하고 클라이언트에서 분기하면 된다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderType {
    /// Anthropic Claude API — `x-api-key` 헤더 + `/v1/messages` 형식
    Anthropic,
    /// OpenAI 호환 API — `Authorization: Bearer` 헤더 + `/v1/chat/completions` 형식
    OpenAi,
    /// 기타 제공자 — 커스텀 헤더 없음, 범용 응답 파싱 사용
    #[default]
    Generic,
}

/// 외부 AI API 엔드포인트 설정
///
/// **Standalone 앱**: API 키를 config.json에 직접 저장 → Settings UI에서 입력
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalApiEndpoint {
    /// API URL (예: "https://api.example.com/v1/messages")
    pub endpoint: String,
    /// API 키 (로컬 config.json에 직접 저장)
    #[serde(default)]
    pub api_key: String,
    /// 모델 이름 (예: "claude-sonnet-4-5-20250929")
    pub model: Option<String>,
    /// 요청 타임아웃 (초)
    #[serde(default = "default_api_timeout_secs")]
    pub timeout_secs: u64,
    /// AI 제공자 타입 — 요청/응답 형식 및 인증 헤더 결정에 사용
    /// 기본값: Generic (범용 파싱, Bearer 토큰 인증)
    #[serde(default)]
    pub provider_type: AiProviderType,
}

/// 외부 API 전송 시 데이터 보호 정책
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ExternalDataPolicy {
    /// PII 필터 Strict 강제 + 민감 앱 이미지 차단
    #[default]
    PiiFilterStrict,
    /// PII 필터 Standard 적용
    PiiFilterStandard,
    /// 사용자 설정 PiiFilterLevel 그대로 적용
    AllowFiltered,
}

fn default_api_timeout_secs() -> u64 {
    30
}

fn default_excluded_extensions() -> Vec<String> {
    vec![
        ".tmp".to_string(),
        ".log".to_string(),
        ".lock".to_string(),
        ".swp".to_string(),
    ]
}

fn default_max_events_per_minute() -> u32 {
    100
}

// ============================================================
// gRPC 설정
// ============================================================

/// gRPC 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// gRPC 인증 사용 여부
    #[serde(default)]
    pub use_grpc_auth: bool,
    /// gRPC 컨텍스트 전송 사용 여부
    #[serde(default)]
    pub use_grpc_context: bool,
    /// gRPC 서버 엔드포인트 (기본 포트)
    #[serde(default = "default_grpc_endpoint")]
    pub grpc_endpoint: String,
    /// gRPC fallback 포트 목록 (기본 포트 연결 실패 시 순차 시도)
    #[serde(default = "default_grpc_fallback_ports")]
    pub grpc_fallback_ports: Vec<u16>,
    /// 연결 타임아웃 (초)
    #[serde(default = "default_grpc_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// 요청 타임아웃 (초)
    #[serde(default = "default_grpc_request_timeout")]
    pub request_timeout_secs: u64,
    /// TLS 사용 여부
    #[serde(default)]
    pub use_tls: bool,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            use_grpc_auth: false,
            use_grpc_context: false,
            grpc_endpoint: default_grpc_endpoint(),
            grpc_fallback_ports: default_grpc_fallback_ports(),
            connect_timeout_secs: default_grpc_connect_timeout(),
            request_timeout_secs: default_grpc_request_timeout(),
            use_tls: false,
        }
    }
}

fn default_grpc_endpoint() -> String {
    "http://localhost:50051".to_string()
}

/// gRPC fallback 포트 목록 (서버가 다른 포트에서 실행될 수 있음)
/// 50051: gRPC 표준 포트 (tonic 기본값)
/// 50052: Python betterproto/grpclib 서버 포트
fn default_grpc_fallback_ports() -> Vec<u16> {
    vec![50052, 50053]
}

fn default_grpc_connect_timeout() -> u64 {
    10
}

fn default_grpc_request_timeout() -> u64 {
    30
}

// ============================================================
// 알림 설정
// ============================================================

/// 알림 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    /// 알림 전체 활성화 여부
    #[serde(default = "default_notification_enabled")]
    pub enabled: bool,
    /// 유휴 감지 알림 (N분 유휴 후 알림)
    #[serde(default = "default_idle_notification")]
    pub idle_notification: bool,
    /// 유휴 알림 임계값 (분)
    #[serde(default = "default_idle_notification_mins")]
    pub idle_notification_mins: u32,
    /// 장시간 작업 알림 (N분 연속 작업 후 휴식 권고)
    #[serde(default = "default_long_session_notification")]
    pub long_session_notification: bool,
    /// 장시간 작업 임계값 (분)
    #[serde(default = "default_long_session_mins")]
    pub long_session_mins: u32,
    /// 고사용량 경고 (CPU/메모리 N% 이상)
    #[serde(default = "default_high_usage_notification")]
    pub high_usage_notification: bool,
    /// 고사용량 임계값 (%)
    #[serde(default = "default_high_usage_threshold")]
    pub high_usage_threshold: u32,
    /// 일일 요약 알림
    #[serde(default)]
    pub daily_summary_notification: bool,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enabled: default_notification_enabled(),
            idle_notification: default_idle_notification(),
            idle_notification_mins: default_idle_notification_mins(),
            long_session_notification: default_long_session_notification(),
            long_session_mins: default_long_session_mins(),
            high_usage_notification: default_high_usage_notification(),
            high_usage_threshold: default_high_usage_threshold(),
            daily_summary_notification: false,
        }
    }
}

// ============================================================
// 웹 대시보드 설정
// ============================================================

/// 웹 대시보드 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebConfig {
    /// 웹 대시보드 활성화 여부
    #[serde(default = "default_web_enabled")]
    pub enabled: bool,
    /// 웹 서버 포트 (기본: 9090)
    #[serde(default = "default_web_port")]
    pub port: u16,
    /// 외부 접근 허용 여부 (false: 127.0.0.1 only)
    #[serde(default)]
    pub allow_external: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: default_web_enabled(),
            port: default_web_port(),
            allow_external: false,
        }
    }
}

// ============================================================
// 자동 업데이트 설정
// ============================================================

/// 자동 업데이트 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// 자동 업데이트 활성화 여부
    #[serde(default = "default_update_enabled")]
    pub enabled: bool,
    /// GitHub 저장소 소유자 — 포크 시 변경 필요 (설정 파일에서 오버라이드 가능)
    #[serde(default = "default_repo_owner")]
    pub repo_owner: String,
    /// GitHub 저장소 이름 (예: "oneshim-client")
    #[serde(default = "default_repo_name")]
    pub repo_name: String,
    /// 업데이트 확인 주기 (시간)
    #[serde(default = "default_check_interval_hours")]
    pub check_interval_hours: u32,
    /// 사전 릴리즈 포함 여부
    #[serde(default)]
    pub include_prerelease: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: default_update_enabled(),
            repo_owner: default_repo_owner(),
            repo_name: default_repo_name(),
            check_interval_hours: default_check_interval_hours(),
            include_prerelease: false,
        }
    }
}

// ============================================================
// 서버/모니터/저장소/비전 설정
// ============================================================

/// 서버 연결 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// API 서버 기본 URL (예: "https://api.example.com")
    pub base_url: String,
    /// 요청 타임아웃 (밀리초)
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    /// SSE 재연결 최대 지연 (초)
    #[serde(default = "default_sse_max_retry_secs")]
    pub sse_max_retry_secs: u64,
}

/// 시스템 모니터링 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// 컨텍스트 수집 주기 (밀리초)
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    /// 서버 동기화 주기 (밀리초)
    #[serde(default = "default_sync_interval_ms")]
    pub sync_interval_ms: u64,
    /// 하트비트 주기 (밀리초)
    #[serde(default = "default_heartbeat_interval_ms")]
    pub heartbeat_interval_ms: u64,
    /// 유휴 감지 임계값 (초)
    #[serde(default = "default_idle_threshold_secs")]
    pub idle_threshold_secs: u64,
    /// 프로세스 스냅샷 주기 (초)
    #[serde(default = "default_process_interval_secs")]
    pub process_interval_secs: u64,
    /// 프로세스 목록 수집 활성화
    #[serde(default = "default_true")]
    pub process_monitoring: bool,
    /// 키보드/마우스 활동 수집 활성화
    #[serde(default = "default_true")]
    pub input_activity: bool,
}

/// 로컬 저장소 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// SQLite DB 파일 경로 (None이면 플랫폼 기본 경로)
    pub db_path: Option<PathBuf>,
    /// 데이터 보존 기간 (일)
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// 최대 저장소 크기 (MB)
    #[serde(default = "default_max_storage_mb")]
    pub max_storage_mb: u64,
}

/// 비전(이미지 처리) 설정
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// 스크린샷 캡처 활성화 여부
    #[serde(default = "default_capture_enabled")]
    pub capture_enabled: bool,
    /// 캡처 쓰로틀 간격 (밀리초)
    #[serde(default = "default_capture_throttle_ms")]
    pub capture_throttle_ms: u64,
    /// 썸네일 너비 (픽셀)
    #[serde(default = "default_thumbnail_width")]
    pub thumbnail_width: u32,
    /// 썸네일 높이 (픽셀)
    #[serde(default = "default_thumbnail_height")]
    pub thumbnail_height: u32,
    /// OCR 활성화 여부
    #[serde(default)]
    pub ocr_enabled: bool,
    /// 프라이버시 모드 (전체 캡처 일시정지)
    #[serde(default)]
    pub privacy_mode: bool,
}

// ============================================================
// AppConfig impl
// ============================================================

impl AppConfig {
    /// 기본 설정값 반환
    pub fn default_config() -> Self {
        Self {
            server: ServerConfig {
                base_url: "http://localhost:8000".to_string(),
                request_timeout_ms: default_request_timeout_ms(),
                sse_max_retry_secs: default_sse_max_retry_secs(),
            },
            monitor: MonitorConfig {
                poll_interval_ms: default_poll_interval_ms(),
                sync_interval_ms: default_sync_interval_ms(),
                heartbeat_interval_ms: default_heartbeat_interval_ms(),
                idle_threshold_secs: default_idle_threshold_secs(),
                process_interval_secs: default_process_interval_secs(),
                process_monitoring: true,
                input_activity: true,
            },
            storage: StorageConfig {
                db_path: None,
                retention_days: default_retention_days(),
                max_storage_mb: default_max_storage_mb(),
            },
            vision: VisionConfig {
                capture_enabled: default_capture_enabled(),
                capture_throttle_ms: default_capture_throttle_ms(),
                thumbnail_width: default_thumbnail_width(),
                thumbnail_height: default_thumbnail_height(),
                ocr_enabled: false,
                privacy_mode: false,
            },
            update: UpdateConfig::default(),
            web: WebConfig::default(),
            notification: NotificationConfig::default(),
            grpc: GrpcConfig::default(),
            telemetry: TelemetryConfig::default(),
            privacy: PrivacyConfig::default(),
            schedule: ScheduleConfig::default(),
            file_access: FileAccessConfig::default(),
            automation: AutomationConfig::default(),
            ai_provider: AiProviderConfig::default(),
        }
    }

    /// 서버 요청 타임아웃을 Duration으로 반환
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.server.request_timeout_ms)
    }

    /// 모니터링 폴링 주기를 Duration으로 반환
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.monitor.poll_interval_ms)
    }

    /// 서버 동기화 주기를 Duration으로 반환
    pub fn sync_interval(&self) -> Duration {
        Duration::from_millis(self.monitor.sync_interval_ms)
    }
}

// ============================================================
// 기본값 함수
// ============================================================

fn default_true() -> bool {
    true
}

fn default_request_timeout_ms() -> u64 {
    30_000
}
fn default_sse_max_retry_secs() -> u64 {
    30
}
fn default_poll_interval_ms() -> u64 {
    1_000
}
fn default_sync_interval_ms() -> u64 {
    10_000
}
fn default_heartbeat_interval_ms() -> u64 {
    30_000
}
fn default_retention_days() -> u32 {
    30
}
fn default_max_storage_mb() -> u64 {
    500
}
fn default_capture_throttle_ms() -> u64 {
    5_000
}
fn default_thumbnail_width() -> u32 {
    480
}
fn default_thumbnail_height() -> u32 {
    270
}
fn default_update_enabled() -> bool {
    true
}
fn default_repo_owner() -> String {
    "pseudotop".to_string()
}
fn default_repo_name() -> String {
    "oneshim-client".to_string()
}
fn default_check_interval_hours() -> u32 {
    24
}
fn default_web_enabled() -> bool {
    true
}
fn default_web_port() -> u16 {
    9090
}
fn default_notification_enabled() -> bool {
    true
}
fn default_idle_notification() -> bool {
    true
}
fn default_idle_notification_mins() -> u32 {
    30
}
fn default_long_session_notification() -> bool {
    true
}
fn default_long_session_mins() -> u32 {
    60
}
fn default_high_usage_notification() -> bool {
    false
}
fn default_high_usage_threshold() -> u32 {
    90
}
fn default_idle_threshold_secs() -> u64 {
    300 // 5분
}
fn default_process_interval_secs() -> u64 {
    10
}
fn default_capture_enabled() -> bool {
    true
}
