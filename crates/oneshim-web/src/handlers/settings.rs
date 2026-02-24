//! 설정 API 핸들러.

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, services::settings_service, AppState};

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
    #[serde(default)]
    pub update: UpdateSettings,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSettings {
    pub enabled: bool,
    pub check_interval_hours: u32,
    pub include_prerelease: bool,
    pub auto_install: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_hours: 24,
            include_prerelease: false,
            auto_install: false,
        }
    }
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
    /// AI 접근 모드
    pub access_mode: String,
    /// OCR 제공자 ("Local" | "Remote")
    pub ocr_provider: String,
    /// LLM 제공자 ("Local" | "Remote")
    pub llm_provider: String,
    /// 외부 데이터 정책 ("PiiFilterStrict" | "PiiFilterStandard" | "AllowFiltered")
    pub external_data_policy: String,
    /// 원격 OCR 전송 시 원본 이미지 전송 허용 (opt-out)
    #[serde(default)]
    pub allow_unredacted_external_ocr: bool,
    /// OCR calibration/validation 설정
    #[serde(default)]
    pub ocr_validation: OcrValidationSettings,
    /// Scene action 민감 입력 오버라이드 설정
    #[serde(default)]
    pub scene_action_override: SceneActionOverrideSettings,
    /// 로컬 폴백 활성화
    pub fallback_to_local: bool,
    /// OCR API 설정 (Remote 선택 시)
    pub ocr_api: Option<ExternalApiSettings>,
    /// LLM API 설정 (Remote 선택 시)
    pub llm_api: Option<ExternalApiSettings>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OcrValidationSettings {
    pub enabled: bool,
    pub min_confidence: f64,
    pub max_invalid_ratio: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SceneActionOverrideSettings {
    pub enabled: bool,
    pub reason: String,
    pub approved_by: String,
    pub expires_at: Option<String>,
}

impl Default for OcrValidationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            min_confidence: 0.25,
            max_invalid_ratio: 0.6,
        }
    }
}

impl Default for AiProviderSettings {
    fn default() -> Self {
        Self {
            access_mode: "ProviderApiKey".to_string(),
            ocr_provider: "Local".to_string(),
            llm_provider: "Local".to_string(),
            external_data_policy: "PiiFilterStrict".to_string(),
            allow_unredacted_external_ocr: false,
            ocr_validation: OcrValidationSettings::default(),
            scene_action_override: SceneActionOverrideSettings::default(),
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
    /// Provider 타입 ("Anthropic" | "OpenAi" | "Google" | "Generic")
    #[serde(default = "default_provider_type")]
    pub provider_type: String,
    /// 타임아웃 (초)
    #[serde(default = "default_external_timeout")]
    pub timeout_secs: u64,
}

fn default_external_timeout() -> u64 {
    30
}

fn default_provider_type() -> String {
    "Generic".to_string()
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
            update: UpdateSettings::default(),
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
    Ok(Json(settings_service::get_storage_stats(&state)?))
}

/// GET /api/settings - 현재 설정 조회
pub async fn get_settings(State(state): State<AppState>) -> Result<Json<AppSettings>, ApiError> {
    Ok(Json(settings_service::get_settings(&state)))
}

/// POST /api/settings - 설정 저장
pub async fn update_settings(
    State(state): State<AppState>,
    Json(settings): Json<AppSettings>,
) -> Result<Json<AppSettings>, ApiError> {
    settings_service::update_settings(&state, &settings)?;
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::settings_service;
    use oneshim_core::config::AppConfig;

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
        assert_eq!(settings.ai_provider.access_mode, "ProviderApiKey");
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
        assert_eq!(
            settings_service::mask_api_key("sk-1234567890abcdef"),
            "sk...cdef"
        );
        assert_eq!(settings_service::mask_api_key("short"), "***");
        assert_eq!(settings_service::mask_api_key("12345678"), "***");
        assert_eq!(settings_service::mask_api_key("123456789"), "12...6789");
    }

    #[test]
    fn is_masked_key_detection() {
        assert!(settings_service::is_masked_key("sk...cdef"));
        assert!(settings_service::is_masked_key("ab...1234"));
        assert!(!settings_service::is_masked_key("sk-1234567890abcdef"));
        assert!(!settings_service::is_masked_key(""));
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

    #[test]
    fn apply_settings_to_config_validates_remote_ai_requirements() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();

        settings.ai_provider.ocr_provider = "Remote".to_string();
        settings.ai_provider.ocr_api = Some(ExternalApiSettings {
            endpoint: "https://api.example.com/ocr".to_string(),
            api_key_masked: "".to_string(),
            model: None,
            provider_type: "Generic".to_string(),
            timeout_secs: 30,
        });

        settings_service::apply_settings_to_config(&mut app_config, &settings).unwrap();
        let result = app_config.ai_provider.validate_selected_remote_endpoints();
        assert!(result.is_err());
    }

    #[test]
    fn apply_settings_to_config_rejects_unknown_sandbox_profile() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();
        settings.sandbox.profile = "Unknown".to_string();

        let result = settings_service::apply_settings_to_config(&mut app_config, &settings);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn apply_settings_to_config_rejects_unknown_weekday() {
        let mut app_config = AppConfig::default_config();
        let mut settings = AppSettings::default();
        settings.schedule.active_days = vec!["Mon".to_string(), "Funday".to_string()];

        let result = settings_service::apply_settings_to_config(&mut app_config, &settings);
        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }
}
