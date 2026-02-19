//! 알림 관리자.
//!
//! 설정에 따라 조건부로 데스크톱 알림을 발송한다.

use chrono::{DateTime, Utc};
use oneshim_core::config::NotificationConfig;
use oneshim_core::ports::notifier::DesktopNotifier;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// 알림 상태 (중복 방지용)
#[derive(Debug, Default)]
struct NotificationState {
    /// 마지막 유휴 알림 시간
    last_idle_notification: Option<DateTime<Utc>>,
    /// 마지막 장시간 작업 알림 시간
    last_long_session_notification: Option<DateTime<Utc>>,
    /// 마지막 고사용량 알림 시간
    last_high_usage_notification: Option<DateTime<Utc>>,
    /// 현재 세션 시작 시간
    session_start: Option<DateTime<Utc>>,
    /// 마지막 활동 시간
    last_activity: Option<DateTime<Utc>>,
}

/// 알림 관리자
pub struct NotificationManager {
    config: RwLock<NotificationConfig>,
    notifier: Arc<dyn DesktopNotifier>,
    state: RwLock<NotificationState>,
}

#[allow(dead_code)]
impl NotificationManager {
    /// 새 알림 관리자 생성
    pub fn new(config: NotificationConfig, notifier: Arc<dyn DesktopNotifier>) -> Self {
        Self {
            config: RwLock::new(config),
            notifier,
            state: RwLock::new(NotificationState {
                session_start: Some(Utc::now()),
                last_activity: Some(Utc::now()),
                ..Default::default()
            }),
        }
    }

    /// 설정 업데이트
    pub async fn update_config(&self, config: NotificationConfig) {
        let mut current = self.config.write().await;
        *current = config;
        info!("알림 설정 업데이트됨");
    }

    /// 활동 기록 (유휴 상태 해제)
    pub async fn record_activity(&self) {
        let mut state = self.state.write().await;
        state.last_activity = Some(Utc::now());
    }

    /// 유휴 상태 확인 및 알림
    ///
    /// 유휴 시간이 임계값을 초과하면 알림 발송.
    /// 알림 쿨다운: 10분
    pub async fn check_idle(&self, idle_secs: u64) {
        let config = self.config.read().await;

        if !config.enabled || !config.idle_notification {
            return;
        }

        let threshold_secs = config.idle_notification_mins as u64 * 60;
        if idle_secs < threshold_secs {
            return;
        }

        // 쿨다운 확인 (10분)
        let mut state = self.state.write().await;
        let now = Utc::now();
        if let Some(last) = state.last_idle_notification {
            if (now - last).num_seconds() < 600 {
                return;
            }
        }

        let mins = idle_secs / 60;
        let title = "💤 유휴 상태 알림";
        let body = format!("{}분 동안 활동이 없습니다. 휴식 중이신가요?", mins);

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            debug!("유휴 알림 실패: {e}");
        } else {
            state.last_idle_notification = Some(now);
            info!("유휴 알림 발송: {}분", mins);
        }
    }

    /// 장시간 작업 확인 및 알림
    ///
    /// 연속 작업 시간이 임계값을 초과하면 휴식 권고 알림.
    /// 알림 쿨다운: 30분
    pub async fn check_long_session(&self) {
        let config = self.config.read().await;

        if !config.enabled || !config.long_session_notification {
            return;
        }

        let mut state = self.state.write().await;
        let now = Utc::now();

        // 세션 시작 시간이 없으면 현재로 설정
        let session_start = state.session_start.get_or_insert(now);
        let session_mins = (now - *session_start).num_minutes() as u64;

        if session_mins < config.long_session_mins as u64 {
            return;
        }

        // 쿨다운 확인 (30분)
        if let Some(last) = state.last_long_session_notification {
            if (now - last).num_seconds() < 1800 {
                return;
            }
        }

        let hours = session_mins / 60;
        let mins = session_mins % 60;
        let title = "⏰ 휴식 시간 알림";
        let body = if hours > 0 {
            format!(
                "{}시간 {}분 동안 작업 중입니다. 잠시 휴식을 취해보세요!",
                hours, mins
            )
        } else {
            format!("{}분 동안 작업 중입니다. 잠시 휴식을 취해보세요!", mins)
        };

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            debug!("장시간 작업 알림 실패: {e}");
        } else {
            state.last_long_session_notification = Some(now);
            info!("장시간 작업 알림 발송: {}분", session_mins);
        }
    }

    /// 고사용량 확인 및 알림
    ///
    /// CPU 또는 메모리 사용률이 임계값을 초과하면 알림.
    /// 알림 쿨다운: 5분
    pub async fn check_high_usage(&self, cpu_percent: f32, memory_percent: f32) {
        let config = self.config.read().await;

        if !config.enabled || !config.high_usage_notification {
            return;
        }

        let threshold = config.high_usage_threshold as f32;
        if cpu_percent < threshold && memory_percent < threshold {
            return;
        }

        // 쿨다운 확인 (5분)
        let mut state = self.state.write().await;
        let now = Utc::now();
        if let Some(last) = state.last_high_usage_notification {
            if (now - last).num_seconds() < 300 {
                return;
            }
        }

        let title = "⚠️ 시스템 리소스 경고";
        let body = if cpu_percent >= threshold && memory_percent >= threshold {
            format!(
                "CPU {:.1}%, 메모리 {:.1}% 사용 중입니다.",
                cpu_percent, memory_percent
            )
        } else if cpu_percent >= threshold {
            format!("CPU 사용률이 {:.1}%입니다.", cpu_percent)
        } else {
            format!("메모리 사용률이 {:.1}%입니다.", memory_percent)
        };

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            debug!("고사용량 알림 실패: {e}");
        } else {
            state.last_high_usage_notification = Some(now);
            info!(
                "고사용량 알림 발송: CPU {:.1}%, Memory {:.1}%",
                cpu_percent, memory_percent
            );
        }
    }

    /// 세션 리셋 (유휴 복귀 시)
    pub async fn reset_session(&self) {
        let mut state = self.state.write().await;
        state.session_start = Some(Utc::now());
        state.last_activity = Some(Utc::now());
        debug!("세션 리셋됨");
    }

    /// 일반 알림 발송
    pub async fn notify(&self, title: &str, body: &str) {
        let config = self.config.read().await;
        if !config.enabled {
            return;
        }

        if let Err(e) = self.notifier.show_notification(title, body).await {
            debug!("알림 발송 실패: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::models::suggestion::Suggestion;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// 테스트용 목 알림기
    struct MockNotifier {
        call_count: AtomicU32,
    }

    impl MockNotifier {
        fn new() -> Self {
            Self {
                call_count: AtomicU32::new(0),
            }
        }

        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl DesktopNotifier for MockNotifier {
        async fn show_suggestion(
            &self,
            _suggestion: &Suggestion,
        ) -> Result<(), oneshim_core::error::CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn show_notification(
            &self,
            _title: &str,
            _body: &str,
        ) -> Result<(), oneshim_core::error::CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn show_error(&self, _message: &str) -> Result<(), oneshim_core::error::CoreError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn idle_notification_triggers() {
        let config = NotificationConfig {
            enabled: true,
            idle_notification: true,
            idle_notification_mins: 1, // 1분
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // 60초 미만 - 알림 없음
        manager.check_idle(30).await;
        assert_eq!(notifier.calls(), 0);

        // 60초 이상 - 알림 발송
        manager.check_idle(60).await;
        assert_eq!(notifier.calls(), 1);

        // 쿨다운 중 - 알림 없음
        manager.check_idle(120).await;
        assert_eq!(notifier.calls(), 1);
    }

    #[tokio::test]
    async fn disabled_notification_no_trigger() {
        let config = NotificationConfig {
            enabled: false,
            idle_notification: true,
            idle_notification_mins: 1,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        manager.check_idle(120).await;
        assert_eq!(notifier.calls(), 0);
    }

    #[tokio::test]
    async fn high_usage_notification_triggers() {
        let config = NotificationConfig {
            enabled: true,
            high_usage_notification: true,
            high_usage_threshold: 80,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // 임계값 미만 - 알림 없음
        manager.check_high_usage(50.0, 60.0).await;
        assert_eq!(notifier.calls(), 0);

        // CPU 임계값 초과 - 알림 발송
        manager.check_high_usage(85.0, 60.0).await;
        assert_eq!(notifier.calls(), 1);
    }

    // --- 추가 테스트 ---

    #[tokio::test]
    async fn memory_high_usage_triggers() {
        let config = NotificationConfig {
            enabled: true,
            high_usage_notification: true,
            high_usage_threshold: 80,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // 메모리만 임계값 초과 - 알림 발송
        manager.check_high_usage(50.0, 90.0).await;
        assert_eq!(notifier.calls(), 1);
    }

    #[tokio::test]
    async fn both_cpu_memory_high_triggers_once() {
        let config = NotificationConfig {
            enabled: true,
            high_usage_notification: true,
            high_usage_threshold: 80,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // CPU와 메모리 모두 임계값 초과 - 알림 1회만
        manager.check_high_usage(85.0, 90.0).await;
        assert_eq!(notifier.calls(), 1);
    }

    #[tokio::test]
    async fn long_session_disabled_no_trigger() {
        let config = NotificationConfig {
            enabled: true,
            long_session_notification: false, // 비활성
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        manager.check_long_session().await;
        assert_eq!(notifier.calls(), 0);
    }

    #[tokio::test]
    async fn notify_disabled_skips() {
        let config = NotificationConfig {
            enabled: false,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        manager.notify("테스트", "알림 본문").await;
        assert_eq!(notifier.calls(), 0);
    }

    #[tokio::test]
    async fn notify_enabled_sends() {
        let config = NotificationConfig {
            enabled: true,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        manager.notify("테스트", "알림 본문").await;
        assert_eq!(notifier.calls(), 1);
    }

    #[tokio::test]
    async fn reset_session_updates_state() {
        let config = NotificationConfig {
            enabled: true,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // 세션 리셋
        manager.reset_session().await;
        // 리셋 후에도 정상 동작 확인
        manager.notify("테스트", "리셋 후").await;
        assert_eq!(notifier.calls(), 1);
    }

    #[tokio::test]
    async fn update_config_changes_behavior() {
        let config = NotificationConfig {
            enabled: false,
            idle_notification: true,
            idle_notification_mins: 1,
            ..Default::default()
        };
        let notifier = Arc::new(MockNotifier::new());
        let manager = NotificationManager::new(config, notifier.clone());

        // 비활성 → 알림 없음
        manager.check_idle(120).await;
        assert_eq!(notifier.calls(), 0);

        // 설정 업데이트: 활성화
        manager
            .update_config(NotificationConfig {
                enabled: true,
                idle_notification: true,
                idle_notification_mins: 1,
                ..Default::default()
            })
            .await;

        // 활성 → 알림 발송
        manager.check_idle(120).await;
        assert_eq!(notifier.calls(), 1);
    }
}
