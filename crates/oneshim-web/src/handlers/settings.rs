//! 설정 API 핸들러.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, AppState};

/// 저장소 통계 응답
#[derive(Debug, Serialize)]
pub struct StorageStats {
    /// 데이터베이스 파일 크기 (bytes)
    pub db_size_bytes: u64,
    /// 프레임 이미지 총 크기 (bytes)
    pub frames_size_bytes: u64,
    /// 총 사용 용량 (bytes)
    pub total_size_bytes: u64,
    /// 총 프레임 수
    pub frame_count: u64,
    /// 총 이벤트 수
    pub event_count: u64,
    /// 총 메트릭 수
    pub metric_count: u64,
    /// 가장 오래된 데이터 날짜
    pub oldest_data_date: Option<String>,
    /// 최신 데이터 날짜
    pub newest_data_date: Option<String>,
}

/// 앱 설정 응답/요청
#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettings {
    /// 데이터 보존 기간 (일)
    pub retention_days: u32,
    /// 최대 저장소 용량 (MB)
    pub max_storage_mb: u32,
    /// 웹 대시보드 포트
    pub web_port: u16,
    /// 외부 접근 허용
    pub allow_external: bool,
    /// 스크린샷 캡처 활성화
    pub capture_enabled: bool,
    /// 유휴 감지 임계값 (초)
    pub idle_threshold_secs: u32,
    /// 메트릭 수집 간격 (초)
    pub metrics_interval_secs: u32,
    /// 프로세스 스냅샷 간격 (초)
    pub process_interval_secs: u32,
    /// 알림 설정
    #[serde(default)]
    pub notification: NotificationSettings,
    /// 텔레메트리 설정
    #[serde(default)]
    pub telemetry: TelemetrySettings,
    /// 모니터링 제어 설정
    #[serde(default)]
    pub monitor: MonitorControlSettings,
    /// 프라이버시 설정
    #[serde(default)]
    pub privacy: PrivacySettings,
    /// 스케줄 설정
    #[serde(default)]
    pub schedule: ScheduleSettings,
    /// 자동화 설정
    #[serde(default)]
    pub automation: AutomationSettings,
    /// 샌드박스 설정
    #[serde(default)]
    pub sandbox: SandboxSettings,
    /// AI 제공자 설정
    #[serde(default)]
    pub ai_provider: AiProviderSettings,
}

/// 알림 설정
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct NotificationSettings {
    /// 알림 전체 활성화
    pub enabled: bool,
    /// 유휴 알림 활성화
    pub idle_notification: bool,
    /// 유휴 알림 임계값 (분)
    pub idle_notification_mins: u32,
    /// 장시간 작업 알림 활성화
    pub long_session_notification: bool,
    /// 장시간 작업 임계값 (분)
    pub long_session_mins: u32,
    /// 고사용량 알림 활성화
    pub high_usage_notification: bool,
    /// 고사용량 임계값 (%)
    pub high_usage_threshold: u32,
}

/// 텔레메트리 설정
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TelemetrySettings {
    /// 텔레메트리 전체 활성화
    pub enabled: bool,
    /// 크래시 리포트 전송
    pub crash_reports: bool,
    /// 사용 통계 전송
    pub usage_analytics: bool,
    /// 성능 메트릭 전송
    pub performance_metrics: bool,
}

/// 모니터링 제어 설정
#[derive(Debug, Serialize, Deserialize)]
pub struct MonitorControlSettings {
    /// 프로세스 목록 수집 활성화
    pub process_monitoring: bool,
    /// 키보드/마우스 활동 수집 활성화
    pub input_activity: bool,
    /// 프라이버시 모드 (전체 캡처 일시정지)
    pub privacy_mode: bool,
}

impl Default for MonitorControlSettings {
    fn default() -> Self {
        Self {
            process_monitoring: true,
            input_activity: true,
            privacy_mode: false,
        }
    }
}

/// 프라이버시 설정
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PrivacySettings {
    /// 제외할 앱 이름 목록
    pub excluded_apps: Vec<String>,
    /// 제외할 앱 이름 패턴
    pub excluded_app_patterns: Vec<String>,
    /// 제외할 창 제목 패턴
    pub excluded_title_patterns: Vec<String>,
    /// 민감 앱 자동 감지
    pub auto_exclude_sensitive: bool,
    /// PII 필터 레벨
    pub pii_filter_level: String,
}

/// 스케줄 설정
#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleSettings {
    /// 활동 시간대 제한 활성화
    pub active_hours_enabled: bool,
    /// 활동 시작 시간 (0-23)
    pub active_start_hour: u8,
    /// 활동 종료 시간 (0-23)
    pub active_end_hour: u8,
    /// 활동 요일 (Mon, Tue, Wed, Thu, Fri, Sat, Sun)
    pub active_days: Vec<String>,
    /// 화면 잠금 시 일시정지
    pub pause_on_screen_lock: bool,
    /// 배터리 세이버 시 일시정지
    pub pause_on_battery_saver: bool,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            active_hours_enabled: false,
            active_start_hour: 9,
            active_end_hour: 18,
            active_days: vec![
                "Mon".to_string(),
                "Tue".to_string(),
                "Wed".to_string(),
                "Thu".to_string(),
                "Fri".to_string(),
            ],
            pause_on_screen_lock: true,
            pause_on_battery_saver: false,
        }
    }
}

/// 자동화 설정
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AutomationSettings {
    /// 자동화 활성화
    pub enabled: bool,
}

/// 샌드박스 설정
#[derive(Debug, Serialize, Deserialize)]
pub struct SandboxSettings {
    /// 샌드박스 활성화
    pub enabled: bool,
    /// 프로필 ("Permissive" | "Standard" | "Strict")
    pub profile: String,
    /// 읽기 허용 경로
    pub allowed_read_paths: Vec<String>,
    /// 쓰기 허용 경로
    pub allowed_write_paths: Vec<String>,
    /// 네트워크 허용
    pub allow_network: bool,
    /// 최대 메모리 (bytes)
    pub max_memory_bytes: u64,
    /// 최대 CPU 시간 (ms)
    pub max_cpu_time_ms: u64,
}

impl Default for SandboxSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            profile: "Standard".to_string(),
            allowed_read_paths: Vec::new(),
            allowed_write_paths: Vec::new(),
            allow_network: false,
            max_memory_bytes: 0,
            max_cpu_time_ms: 0,
        }
    }
}

/// AI 제공자 설정
#[derive(Debug, Serialize, Deserialize)]
pub struct AiProviderSettings {
    /// OCR 제공자 ("Local" | "Remote")
    pub ocr_provider: String,
    /// LLM 제공자 ("Local" | "Remote")
    pub llm_provider: String,
    /// 외부 데이터 정책 ("PiiFilterStrict" | "PiiFilterStandard" | "AllowFiltered")
    pub external_data_policy: String,
    /// 로컬 폴백 활성화
    pub fallback_to_local: bool,
    /// OCR API 설정 (Remote 선택 시)
    pub ocr_api: Option<ExternalApiSettings>,
    /// LLM API 설정 (Remote 선택 시)
    pub llm_api: Option<ExternalApiSettings>,
}

impl Default for AiProviderSettings {
    fn default() -> Self {
        Self {
            ocr_provider: "Local".to_string(),
            llm_provider: "Local".to_string(),
            external_data_policy: "PiiFilterStrict".to_string(),
            fallback_to_local: true,
            ocr_api: None,
            llm_api: None,
        }
    }
}

/// 외부 API 설정
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExternalApiSettings {
    /// API 엔드포인트 URL
    pub endpoint: String,
    /// API 키 (GET: 마스킹 / POST: 전체 키)
    pub api_key_masked: String,
    /// 모델 이름
    pub model: Option<String>,
    /// 타임아웃 (초)
    #[serde(default = "default_external_timeout")]
    pub timeout_secs: u64,
}

fn default_external_timeout() -> u64 {
    30
}

/// API 키 마스킹 — 앞 2자 + "..." + 뒤 4자
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    format!("{}...{}", &key[..2], &key[key.len() - 4..])
}

/// POST에서 받은 값이 마스킹된 값인지 확인
fn is_masked_key(value: &str) -> bool {
    value.contains("...") && value.len() <= 12
}

/// ExternalApiEndpoint → ExternalApiSettings 변환 (GET용, 키 마스킹)
fn endpoint_to_api_settings(
    endpoint: &oneshim_core::config::ExternalApiEndpoint,
) -> ExternalApiSettings {
    ExternalApiSettings {
        endpoint: endpoint.endpoint.clone(),
        api_key_masked: mask_api_key(&endpoint.api_key),
        model: endpoint.model.clone(),
        timeout_secs: endpoint.timeout_secs,
    }
}

/// ExternalApiSettings → ExternalApiEndpoint 역변환 (POST용)
/// 마스킹된 키가 오면 existing_key를 유지
fn api_settings_to_endpoint(
    settings: &ExternalApiSettings,
    existing_key: &str,
) -> oneshim_core::config::ExternalApiEndpoint {
    let api_key = if is_masked_key(&settings.api_key_masked) || settings.api_key_masked.is_empty() {
        existing_key.to_string()
    } else {
        settings.api_key_masked.clone()
    };
    oneshim_core::config::ExternalApiEndpoint {
        endpoint: settings.endpoint.clone(),
        api_key,
        model: settings.model.clone(),
        timeout_secs: settings.timeout_secs,
        provider_type: Default::default(),
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            retention_days: 30,
            max_storage_mb: 500,
            web_port: 9090,
            allow_external: false,
            capture_enabled: true,
            idle_threshold_secs: 300,
            metrics_interval_secs: 5,
            process_interval_secs: 10,
            notification: NotificationSettings {
                enabled: true,
                idle_notification: true,
                idle_notification_mins: 30,
                long_session_notification: true,
                long_session_mins: 60,
                high_usage_notification: false,
                high_usage_threshold: 90,
            },
            telemetry: TelemetrySettings::default(),
            monitor: MonitorControlSettings::default(),
            privacy: PrivacySettings {
                auto_exclude_sensitive: true,
                pii_filter_level: "Standard".to_string(),
                ..Default::default()
            },
            schedule: ScheduleSettings::default(),
            automation: AutomationSettings::default(),
            sandbox: SandboxSettings::default(),
            ai_provider: AiProviderSettings::default(),
        }
    }
}

/// GET /api/storage/stats - 저장소 통계 조회
pub async fn get_storage_stats(
    State(state): State<AppState>,
) -> Result<Json<StorageStats>, ApiError> {
    let conn = state
        .storage
        .conn_ref()
        .lock()
        .map_err(|e| ApiError::Internal(format!("DB 잠금 실패: {e}")))?;

    // 각 테이블의 레코드 수 조회
    let frame_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM frames", [], |row| row.get(0))
        .unwrap_or(0);

    let event_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap_or(0);

    let metric_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM system_metrics", [], |row| row.get(0))
        .unwrap_or(0);

    // 가장 오래된/최신 데이터 날짜
    let oldest_date: Option<String> = conn
        .query_row(
            "SELECT MIN(timestamp) FROM (
                SELECT timestamp FROM events
                UNION ALL
                SELECT timestamp FROM frames
                UNION ALL
                SELECT timestamp FROM system_metrics
            )",
            [],
            |row| row.get(0),
        )
        .ok();

    let newest_date: Option<String> = conn
        .query_row(
            "SELECT MAX(timestamp) FROM (
                SELECT timestamp FROM events
                UNION ALL
                SELECT timestamp FROM frames
                UNION ALL
                SELECT timestamp FROM system_metrics
            )",
            [],
            |row| row.get(0),
        )
        .ok();

    // DB 파일 크기 (페이지 수 * 페이지 크기)
    let page_count: u64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .unwrap_or(0);
    let page_size: u64 = conn
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .unwrap_or(4096);
    let db_size_bytes = page_count * page_size;

    // 프레임 이미지 폴더 크기 계산
    let frames_size_bytes = if let Some(ref frames_dir) = state.frames_dir {
        calculate_dir_size(frames_dir)
    } else {
        0
    };

    Ok(Json(StorageStats {
        db_size_bytes,
        frames_size_bytes,
        total_size_bytes: db_size_bytes + frames_size_bytes,
        frame_count,
        event_count,
        metric_count,
        oldest_data_date: oldest_date,
        newest_data_date: newest_date,
    }))
}

/// GET /api/settings - 현재 설정 조회
pub async fn get_settings(State(state): State<AppState>) -> Result<Json<AppSettings>, ApiError> {
    // 설정 관리자에서 로드
    if let Some(ref config_manager) = state.config_manager {
        let config = config_manager.get();
        Ok(Json(AppSettings {
            retention_days: config.storage.retention_days,
            max_storage_mb: config.storage.max_storage_mb as u32,
            web_port: config.web.port,
            allow_external: config.web.allow_external,
            capture_enabled: config.vision.capture_enabled,
            idle_threshold_secs: config.monitor.idle_threshold_secs as u32,
            metrics_interval_secs: (config.monitor.poll_interval_ms / 1000) as u32,
            process_interval_secs: config.monitor.process_interval_secs as u32,
            notification: NotificationSettings {
                enabled: config.notification.enabled,
                idle_notification: config.notification.idle_notification,
                idle_notification_mins: config.notification.idle_notification_mins,
                long_session_notification: config.notification.long_session_notification,
                long_session_mins: config.notification.long_session_mins,
                high_usage_notification: config.notification.high_usage_notification,
                high_usage_threshold: config.notification.high_usage_threshold,
            },
            telemetry: TelemetrySettings {
                enabled: config.telemetry.enabled,
                crash_reports: config.telemetry.crash_reports,
                usage_analytics: config.telemetry.usage_analytics,
                performance_metrics: config.telemetry.performance_metrics,
            },
            monitor: MonitorControlSettings {
                process_monitoring: config.monitor.process_monitoring,
                input_activity: config.monitor.input_activity,
                privacy_mode: config.vision.privacy_mode,
            },
            privacy: PrivacySettings {
                excluded_apps: config.privacy.excluded_apps.clone(),
                excluded_app_patterns: config.privacy.excluded_app_patterns.clone(),
                excluded_title_patterns: config.privacy.excluded_title_patterns.clone(),
                auto_exclude_sensitive: config.privacy.auto_exclude_sensitive,
                pii_filter_level: format!("{:?}", config.privacy.pii_filter_level),
            },
            schedule: ScheduleSettings {
                active_hours_enabled: config.schedule.active_hours_enabled,
                active_start_hour: config.schedule.active_start_hour,
                active_end_hour: config.schedule.active_end_hour,
                active_days: config
                    .schedule
                    .active_days
                    .iter()
                    .map(|d| format!("{:?}", d))
                    .collect(),
                pause_on_screen_lock: config.schedule.pause_on_screen_lock,
                pause_on_battery_saver: config.schedule.pause_on_battery_saver,
            },
            // 자동화 설정
            automation: AutomationSettings {
                enabled: config.automation.enabled,
            },
            sandbox: SandboxSettings {
                enabled: config.automation.sandbox.enabled,
                profile: format!("{:?}", config.automation.sandbox.profile),
                allowed_read_paths: config.automation.sandbox.allowed_read_paths.clone(),
                allowed_write_paths: config.automation.sandbox.allowed_write_paths.clone(),
                allow_network: config.automation.sandbox.allow_network,
                max_memory_bytes: config.automation.sandbox.max_memory_bytes,
                max_cpu_time_ms: config.automation.sandbox.max_cpu_time_ms,
            },
            ai_provider: AiProviderSettings {
                ocr_provider: format!("{:?}", config.ai_provider.ocr_provider),
                llm_provider: format!("{:?}", config.ai_provider.llm_provider),
                external_data_policy: format!("{:?}", config.ai_provider.external_data_policy),
                fallback_to_local: config.ai_provider.fallback_to_local,
                ocr_api: config
                    .ai_provider
                    .ocr_api
                    .as_ref()
                    .map(endpoint_to_api_settings),
                llm_api: config
                    .ai_provider
                    .llm_api
                    .as_ref()
                    .map(endpoint_to_api_settings),
            },
        }))
    } else {
        // 설정 관리자가 없으면 기본값 반환
        Ok(Json(AppSettings::default()))
    }
}

/// POST /api/settings - 설정 저장
pub async fn update_settings(
    State(state): State<AppState>,
    Json(settings): Json<AppSettings>,
) -> Result<Json<AppSettings>, ApiError> {
    // 유효성 검사
    if settings.retention_days == 0 || settings.retention_days > 365 {
        return Err(ApiError::BadRequest(
            "보존 기간은 1-365일 사이여야 합니다".to_string(),
        ));
    }

    if settings.max_storage_mb < 100 || settings.max_storage_mb > 10000 {
        return Err(ApiError::BadRequest(
            "최대 저장소 용량은 100MB-10GB 사이여야 합니다".to_string(),
        ));
    }

    if settings.web_port < 1024 {
        return Err(ApiError::BadRequest(
            "포트는 1024 이상이어야 합니다".to_string(),
        ));
    }

    // 설정 관리자에 저장
    if let Some(ref config_manager) = state.config_manager {
        config_manager
            .update_with(|config| {
                config.storage.retention_days = settings.retention_days;
                config.storage.max_storage_mb = settings.max_storage_mb as u64;
                config.web.port = settings.web_port;
                config.web.allow_external = settings.allow_external;
                // 비전 설정
                config.vision.capture_enabled = settings.capture_enabled;
                // 모니터 설정
                config.monitor.poll_interval_ms = (settings.metrics_interval_secs as u64) * 1000;
                config.monitor.idle_threshold_secs = settings.idle_threshold_secs as u64;
                config.monitor.process_interval_secs = settings.process_interval_secs as u64;
                // 알림 설정
                config.notification.enabled = settings.notification.enabled;
                config.notification.idle_notification = settings.notification.idle_notification;
                config.notification.idle_notification_mins =
                    settings.notification.idle_notification_mins;
                config.notification.long_session_notification =
                    settings.notification.long_session_notification;
                config.notification.long_session_mins = settings.notification.long_session_mins;
                config.notification.high_usage_notification =
                    settings.notification.high_usage_notification;
                config.notification.high_usage_threshold =
                    settings.notification.high_usage_threshold;
                // 텔레메트리 설정
                config.telemetry.enabled = settings.telemetry.enabled;
                config.telemetry.crash_reports = settings.telemetry.crash_reports;
                config.telemetry.usage_analytics = settings.telemetry.usage_analytics;
                config.telemetry.performance_metrics = settings.telemetry.performance_metrics;
                // 모니터링 제어
                config.monitor.process_monitoring = settings.monitor.process_monitoring;
                config.monitor.input_activity = settings.monitor.input_activity;
                config.vision.privacy_mode = settings.monitor.privacy_mode;
                // 프라이버시 설정
                config.privacy.excluded_apps = settings.privacy.excluded_apps.clone();
                config.privacy.excluded_app_patterns =
                    settings.privacy.excluded_app_patterns.clone();
                config.privacy.excluded_title_patterns =
                    settings.privacy.excluded_title_patterns.clone();
                config.privacy.auto_exclude_sensitive = settings.privacy.auto_exclude_sensitive;
                // PII 필터 레벨 파싱 (문자열 → enum)
                config.privacy.pii_filter_level = match settings.privacy.pii_filter_level.as_str() {
                    "Off" => oneshim_core::config::PiiFilterLevel::Off,
                    "Basic" => oneshim_core::config::PiiFilterLevel::Basic,
                    "Strict" => oneshim_core::config::PiiFilterLevel::Strict,
                    _ => oneshim_core::config::PiiFilterLevel::Standard,
                };
                // 스케줄 설정
                config.schedule.active_hours_enabled = settings.schedule.active_hours_enabled;
                config.schedule.active_start_hour = settings.schedule.active_start_hour;
                config.schedule.active_end_hour = settings.schedule.active_end_hour;
                config.schedule.active_days = settings
                    .schedule
                    .active_days
                    .iter()
                    .filter_map(|d| match d.as_str() {
                        "Mon" => Some(oneshim_core::config::Weekday::Mon),
                        "Tue" => Some(oneshim_core::config::Weekday::Tue),
                        "Wed" => Some(oneshim_core::config::Weekday::Wed),
                        "Thu" => Some(oneshim_core::config::Weekday::Thu),
                        "Fri" => Some(oneshim_core::config::Weekday::Fri),
                        "Sat" => Some(oneshim_core::config::Weekday::Sat),
                        "Sun" => Some(oneshim_core::config::Weekday::Sun),
                        _ => None,
                    })
                    .collect();
                config.schedule.pause_on_screen_lock = settings.schedule.pause_on_screen_lock;
                config.schedule.pause_on_battery_saver = settings.schedule.pause_on_battery_saver;
                // 자동화 설정
                config.automation.enabled = settings.automation.enabled;
                // 샌드박스 설정
                config.automation.sandbox.enabled = settings.sandbox.enabled;
                config.automation.sandbox.profile = match settings.sandbox.profile.as_str() {
                    "Permissive" => oneshim_core::config::SandboxProfile::Permissive,
                    "Strict" => oneshim_core::config::SandboxProfile::Strict,
                    _ => oneshim_core::config::SandboxProfile::Standard,
                };
                config.automation.sandbox.allowed_read_paths =
                    settings.sandbox.allowed_read_paths.clone();
                config.automation.sandbox.allowed_write_paths =
                    settings.sandbox.allowed_write_paths.clone();
                config.automation.sandbox.allow_network = settings.sandbox.allow_network;
                config.automation.sandbox.max_memory_bytes = settings.sandbox.max_memory_bytes;
                config.automation.sandbox.max_cpu_time_ms = settings.sandbox.max_cpu_time_ms;
                // AI 제공자 설정
                config.ai_provider.ocr_provider = match settings.ai_provider.ocr_provider.as_str() {
                    "Remote" => oneshim_core::config::OcrProviderType::Remote,
                    _ => oneshim_core::config::OcrProviderType::Local,
                };
                config.ai_provider.llm_provider = match settings.ai_provider.llm_provider.as_str() {
                    "Remote" => oneshim_core::config::LlmProviderType::Remote,
                    _ => oneshim_core::config::LlmProviderType::Local,
                };
                config.ai_provider.external_data_policy =
                    match settings.ai_provider.external_data_policy.as_str() {
                        "PiiFilterStandard" => {
                            oneshim_core::config::ExternalDataPolicy::PiiFilterStandard
                        }
                        "AllowFiltered" => oneshim_core::config::ExternalDataPolicy::AllowFiltered,
                        _ => oneshim_core::config::ExternalDataPolicy::PiiFilterStrict,
                    };
                config.ai_provider.fallback_to_local = settings.ai_provider.fallback_to_local;
                // OCR API — 키 마스킹 감지
                if let Some(ref ocr_settings) = settings.ai_provider.ocr_api {
                    let existing_key = config
                        .ai_provider
                        .ocr_api
                        .as_ref()
                        .map(|e| e.api_key.as_str())
                        .unwrap_or("");
                    config.ai_provider.ocr_api =
                        Some(api_settings_to_endpoint(ocr_settings, existing_key));
                } else {
                    config.ai_provider.ocr_api = None;
                }
                // LLM API — 키 마스킹 감지
                if let Some(ref llm_settings) = settings.ai_provider.llm_api {
                    let existing_key = config
                        .ai_provider
                        .llm_api
                        .as_ref()
                        .map(|e| e.api_key.as_str())
                        .unwrap_or("");
                    config.ai_provider.llm_api =
                        Some(api_settings_to_endpoint(llm_settings, existing_key));
                } else {
                    config.ai_provider.llm_api = None;
                }
            })
            .map_err(|e| ApiError::Internal(format!("설정 저장 실패: {e}")))?;
    }

    Ok(Json(settings))
}

/// 디렉토리 크기 계산 (재귀)
fn calculate_dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0;

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    total += metadata.len();
                }
            } else if path.is_dir() {
                total += calculate_dir_size(&path);
            }
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_valid() {
        let settings = AppSettings::default();
        assert_eq!(settings.retention_days, 30);
        assert_eq!(settings.max_storage_mb, 500);
        assert_eq!(settings.web_port, 9090);
        assert!(!settings.allow_external);
        assert!(settings.capture_enabled);
    }

    #[test]
    fn default_settings_includes_automation() {
        let settings = AppSettings::default();
        assert!(!settings.automation.enabled);
        assert!(!settings.sandbox.enabled);
        assert_eq!(settings.sandbox.profile, "Standard");
        assert_eq!(settings.ai_provider.ocr_provider, "Local");
        assert_eq!(settings.ai_provider.llm_provider, "Local");
        assert!(settings.ai_provider.fallback_to_local);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let settings = AppSettings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let deser: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.automation.enabled, settings.automation.enabled);
        assert_eq!(deser.sandbox.profile, settings.sandbox.profile);
        assert_eq!(
            deser.ai_provider.ocr_provider,
            settings.ai_provider.ocr_provider
        );
    }

    #[test]
    fn mask_api_key_works() {
        assert_eq!(mask_api_key("sk-1234567890abcdef"), "sk...cdef");
        assert_eq!(mask_api_key("short"), "***");
        assert_eq!(mask_api_key("12345678"), "***");
        assert_eq!(mask_api_key("123456789"), "12...6789");
    }

    #[test]
    fn is_masked_key_detection() {
        assert!(is_masked_key("sk...cdef"));
        assert!(is_masked_key("ab...1234"));
        assert!(!is_masked_key("sk-1234567890abcdef"));
        assert!(!is_masked_key(""));
    }

    #[test]
    fn storage_stats_serializes() {
        let stats = StorageStats {
            db_size_bytes: 1024 * 1024,
            frames_size_bytes: 5 * 1024 * 1024,
            total_size_bytes: 6 * 1024 * 1024,
            frame_count: 100,
            event_count: 500,
            metric_count: 1000,
            oldest_data_date: Some("2024-01-01T00:00:00Z".to_string()),
            newest_data_date: Some("2024-01-30T23:59:59Z".to_string()),
        };

        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("db_size_bytes"));
        assert!(json.contains("frame_count"));
    }
}
