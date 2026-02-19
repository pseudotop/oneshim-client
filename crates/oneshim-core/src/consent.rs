//! 동의 관리 시스템 (GDPR/EU AI Act 준수).
//!
//! 사용자 동의 기록, 검증, 철회, 데이터 내보내기/삭제 요청을 처리한다.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::CoreError;

/// 현재 동의 정책 버전 — 정책 변경 시 증가하여 재동의 요구
pub const CURRENT_POLICY_VERSION: &str = "1.0.0";

// ============================================================
// 동의 권한 모델
// ============================================================

/// 사용자가 부여한 권한 목록
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentPermissions {
    // --- Tier 1 ---
    /// 화면 캡처 허용
    #[serde(default)]
    pub screen_capture: bool,
    /// OCR 처리 허용
    #[serde(default)]
    pub ocr_processing: bool,
    /// 텔레메트리 전송 허용
    #[serde(default)]
    pub telemetry: bool,
    /// 프로세스 모니터링 허용
    #[serde(default)]
    pub process_monitoring: bool,
    /// 입력 활동 수집 허용
    #[serde(default)]
    pub input_activity: bool,

    // --- Tier 2 ---
    /// 창 제목 수집 허용
    #[serde(default)]
    pub window_title_collection: bool,
    /// 앱 사용 분석 허용
    #[serde(default)]
    pub app_usage_analytics: bool,

    // --- Tier 3 ---
    /// 클립보드 모니터링 허용
    #[serde(default)]
    pub clipboard_monitoring: bool,
    /// 파일 접근 모니터링 허용
    #[serde(default)]
    pub file_access_monitoring: bool,
}

impl Default for ConsentPermissions {
    /// 기본값: 최소 권한 (모두 비활성)
    fn default() -> Self {
        Self {
            screen_capture: false,
            ocr_processing: false,
            telemetry: false,
            process_monitoring: false,
            input_activity: false,
            window_title_collection: false,
            app_usage_analytics: false,
            clipboard_monitoring: false,
            file_access_monitoring: false,
        }
    }
}

// ============================================================
// 동의 기록
// ============================================================

/// 동의 기록 — JSON 파일로 로컬 저장
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// 동의 고유 ID
    pub consent_id: String,
    /// 동의 정책 버전
    pub version: String,
    /// 동의 시각
    pub granted_at: DateTime<Utc>,
    /// 동의 만료 시각 (None이면 무기한)
    pub expires_at: Option<DateTime<Utc>>,
    /// 부여된 권한 목록
    pub permissions: ConsentPermissions,
    /// 데이터 보관 기간 (일)
    pub data_retention_days: u32,
}

// ============================================================
// 동의 상태
// ============================================================

/// 동의 상태
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentStatus {
    /// 동의하지 않음 (첫 실행)
    NotGranted,
    /// 유효한 동의
    Valid,
    /// 동의 만료
    Expired,
    /// 정책 버전 변경으로 재동의 필요
    UpdateRequired,
}

// ============================================================
// 데이터 내보내기 / 삭제 모델
// ============================================================

/// GDPR Article 20 — 데이터 이동권 (내보내기 결과)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDataExport {
    /// 내보내기 시각
    pub exported_at: DateTime<Utc>,
    /// 동의 기록
    pub consent: Option<ConsentRecord>,
    /// 설정 데이터
    pub settings: serde_json::Value,
    /// 수집된 이벤트 수
    pub event_count: u64,
    /// 수집된 프레임 수
    pub frame_count: u64,
    /// 내보내기 경로
    pub export_path: PathBuf,
}

/// GDPR Article 17 — 삭제 요청 결과
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionResult {
    /// 삭제 시각
    pub deleted_at: DateTime<Utc>,
    /// 삭제된 이벤트 수
    pub events_deleted: u64,
    /// 삭제된 프레임 수
    pub frames_deleted: u64,
    /// 삭제된 메트릭 수
    pub metrics_deleted: u64,
    /// 설정 초기화 여부
    pub settings_reset: bool,
    /// 동의 철회 여부
    pub consent_revoked: bool,
}

// ============================================================
// ConsentManager
// ============================================================

/// 동의 관리자 — 로컬 JSON 파일 기반 동의 기록 관리
pub struct ConsentManager {
    /// 동의 파일 저장 경로
    storage_path: PathBuf,
    /// 현재 동의 기록
    current_consent: Option<ConsentRecord>,
}

impl ConsentManager {
    /// 새 ConsentManager 생성 + 기존 파일 로드
    pub fn new(storage_path: PathBuf) -> Self {
        let current_consent = Self::load_from_file(&storage_path);
        Self {
            storage_path,
            current_consent,
        }
    }

    /// 동의 상태 확인
    pub fn check_consent(&self) -> ConsentStatus {
        match &self.current_consent {
            None => ConsentStatus::NotGranted,
            Some(record) => {
                // 만료 확인
                if let Some(expires) = record.expires_at {
                    if Utc::now() > expires {
                        return ConsentStatus::Expired;
                    }
                }
                // 정책 버전 확인
                if record.version != CURRENT_POLICY_VERSION {
                    return ConsentStatus::UpdateRequired;
                }
                ConsentStatus::Valid
            }
        }
    }

    /// 현재 동의 기록 반환
    pub fn current_consent(&self) -> Option<&ConsentRecord> {
        self.current_consent.as_ref()
    }

    /// 동의 부여
    pub fn grant_consent(
        &mut self,
        permissions: ConsentPermissions,
        data_retention_days: u32,
    ) -> Result<(), CoreError> {
        let record = ConsentRecord {
            consent_id: uuid::Uuid::new_v4().to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now(),
            expires_at: None,
            permissions,
            data_retention_days,
        };

        self.save_to_file(&record)?;
        self.current_consent = Some(record);
        Ok(())
    }

    /// 동의 철회 (파일 삭제)
    pub fn revoke_consent(&mut self) -> Result<(), CoreError> {
        if self.storage_path.exists() {
            std::fs::remove_file(&self.storage_path)?;
        }
        self.current_consent = None;
        Ok(())
    }

    /// 특정 권한 허용 여부 확인
    pub fn is_permitted(&self, check: impl Fn(&ConsentPermissions) -> bool) -> bool {
        self.current_consent
            .as_ref()
            .map(|r| check(&r.permissions))
            .unwrap_or(false)
    }

    // --- 내부 유틸 ---

    /// 파일에서 동의 기록 로드
    fn load_from_file(path: &PathBuf) -> Option<ConsentRecord> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// 동의 기록을 파일에 저장
    fn save_to_file(&self, record: &ConsentRecord) -> Result<(), CoreError> {
        // 부모 디렉토리 생성
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(record)?;
        std::fs::write(&self.storage_path, json)?;
        Ok(())
    }
}

// ============================================================
// 테스트
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_permissions_default_all_false() {
        let perms = ConsentPermissions::default();
        assert!(!perms.screen_capture);
        assert!(!perms.telemetry);
        assert!(!perms.clipboard_monitoring);
    }

    #[test]
    fn consent_record_serde_roundtrip() {
        let record = ConsentRecord {
            consent_id: "test-001".to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now(),
            expires_at: None,
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: ConsentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.consent_id, "test-001");
        assert_eq!(deserialized.data_retention_days, 30);
    }

    #[test]
    fn consent_status_not_granted_when_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let manager = ConsentManager::new(path);
        assert_eq!(manager.check_consent(), ConsentStatus::NotGranted);
    }

    #[test]
    fn consent_grant_and_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let perms = ConsentPermissions {
            screen_capture: true,
            ..Default::default()
        };
        manager.grant_consent(perms, 30).unwrap();

        assert_eq!(manager.check_consent(), ConsentStatus::Valid);
        assert!(manager.is_permitted(|p| p.screen_capture));
        assert!(!manager.is_permitted(|p| p.clipboard_monitoring));
    }

    #[test]
    fn consent_revoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let perms = ConsentPermissions::default();
        manager.grant_consent(perms, 30).unwrap();
        assert_eq!(manager.check_consent(), ConsentStatus::Valid);

        manager.revoke_consent().unwrap();
        assert_eq!(manager.check_consent(), ConsentStatus::NotGranted);
    }

    #[test]
    fn consent_expired() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        // 이미 만료된 기록 직접 설정
        let record = ConsentRecord {
            consent_id: "expired-001".to_string(),
            version: CURRENT_POLICY_VERSION.to_string(),
            granted_at: Utc::now() - chrono::Duration::days(365),
            expires_at: Some(Utc::now() - chrono::Duration::days(1)),
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };
        manager.current_consent = Some(record);
        assert_eq!(manager.check_consent(), ConsentStatus::Expired);
    }

    #[test]
    fn consent_update_required_on_version_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        let record = ConsentRecord {
            consent_id: "old-001".to_string(),
            version: "0.9.0".to_string(), // 이전 버전
            granted_at: Utc::now(),
            expires_at: None,
            permissions: ConsentPermissions::default(),
            data_retention_days: 30,
        };
        manager.current_consent = Some(record);
        assert_eq!(manager.check_consent(), ConsentStatus::UpdateRequired);
    }
}
