use chrono::{DateTime, Duration, Utc};
use oneshim_core::models::work_session::{FocusMetrics, LocalSuggestion};
use tracing::{info, warn};

use crate::workflow_intelligence::PlaybookSignal;

use super::FocusAnalyzer;

impl FocusAnalyzer {
    pub(super) fn calculate_focus_score(&self, metrics: &FocusMetrics) -> f32 {
        if metrics.total_active_secs == 0 {
            return 0.0;
        }

        let deep_work_ratio = metrics.deep_work_secs as f32 / metrics.total_active_secs as f32;
        let interruption_penalty = (metrics.interruption_count as f32
            * self.config.focus_score_interruption_penalty)
            .min(0.5);

        ((deep_work_ratio * self.config.focus_score_deep_work_weight) - interruption_penalty)
            .clamp(0.0, 1.0)
    }

    pub(super) async fn maybe_suggest_break(&self) {
        let tracker = self.tracker.read().await;
        let continuous_mins = (tracker.continuous_deep_work_secs / 60) as u32;

        if continuous_mins < self.config.break_suggestion_mins {
            return;
        }

        if !self.check_cooldown("break").await {
            return;
        }

        let suggestion = LocalSuggestion::TakeBreak {
            continuous_work_mins: continuous_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("suggestion save failure: {e}");
                return;
            }
        };

        let title = "☕ 휴식 시간";
        let body = format!(
            "{}분 동안 집중하셨습니다. 잠시 휴식을 취해보세요!",
            continuous_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!("suggestion sent: {}min consecutive", continuous_mins);
        }

        self.update_cooldown("break").await;
    }

    pub(super) async fn maybe_suggest_focus_time(&self, metrics: &FocusMetrics) {
        let comm_ratio = metrics.communication_ratio();

        if comm_ratio < self.config.excessive_communication_threshold {
            return;
        }

        if !self.check_cooldown("focus_time").await {
            return;
        }

        let suggested_focus_mins = (metrics.communication_secs / 60).max(30) as u32;

        let suggestion = LocalSuggestion::NeedFocusTime {
            communication_ratio: comm_ratio,
            suggested_focus_mins,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("in progress hour suggestion save failure: {e}");
                return;
            }
        };

        let title = "🎯 집중 시간 필요";
        let body = format!(
            "오늘 소통에 {:.0}%의 시간을 사용했습니다. {}분의 집중 시간을 확보해보세요.",
            comm_ratio * 100.0,
            suggested_focus_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("in progress hour notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!(
                "in progress hour suggestion sent: {:.1}%",
                comm_ratio * 100.0
            );
        }

        self.update_cooldown("focus_time").await;
    }

    pub(super) async fn maybe_suggest_restore_context(&self, app: &str, now: DateTime<Utc>) {
        if !self.check_cooldown("restore_context").await {
            return;
        }

        let interruption = match self.storage.get_pending_interruption() {
            Ok(Some(int)) => int,
            _ => return,
        };

        if (now - interruption.interrupted_at).num_minutes() > 30 {
            return;
        }

        let suggestion = LocalSuggestion::RestoreContext {
            interrupted_app: app.to_string(),
            interrupted_at: interruption.interrupted_at,
            snapshot_frame_id: interruption.snapshot_frame_id.unwrap_or(0),
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("context restore suggestion save failure: {e}");
                return;
            }
        };

        let title = "🔄 작업 context";
        let duration_mins = (now - interruption.interrupted_at).num_minutes();
        let body = format!(
            "{}에서 {}분 전 중단되었습니다. 이전 작업으로 돌아가시겠습니까?",
            app, duration_mins
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("context restore notification failure: {e}");
        } else {
            let _ = self.storage.mark_suggestion_shown(suggestion_id);
            info!(
                "context 복원 suggestion 발송: {} ({}분 전 중단)",
                app, duration_mins
            );
        }

        self.update_cooldown("restore_context").await;
    }

    pub(super) async fn maybe_suggest_pattern_detected(&self, signal: PlaybookSignal) {
        if !self.check_cooldown("pattern_detected").await {
            return;
        }

        let suggestion = LocalSuggestion::PatternDetected {
            pattern_description: signal.description.clone(),
            confidence: signal.confidence,
        };

        let suggestion_id = match self.storage.save_local_suggestion(&suggestion) {
            Ok(id) => id,
            Err(e) => {
                warn!("suggestion save failure: {e}");
                return;
            }
        };

        let title = "🧭 반복 플레이북";
        let confidence_percent = (signal.confidence * 100.0).round() as i32;
        let body = format!(
            "{} (confidence {}%)",
            signal.description, confidence_percent
        );

        if let Err(e) = self.notifier.show_notification(title, &body).await {
            warn!("suggestion notification failure: {e}");
            return;
        }

        let _ = self.storage.mark_suggestion_shown(suggestion_id);
        info!(
            confidence = signal.confidence,
            description = %signal.description,
            "플레이북 패턴 suggestion 발송"
        );
        self.update_cooldown("pattern_detected").await;
    }

    pub(super) async fn check_cooldown(&self, suggestion_type: &str) -> bool {
        let cooldowns = self.cooldowns.read().await;
        let now = Utc::now();
        let cooldown_duration = Duration::seconds(self.config.suggestion_cooldown_secs as i64);

        let last_time = match suggestion_type {
            "break" => cooldowns.last_break,
            "focus_time" => cooldowns.last_focus_time,
            "restore_context" => cooldowns.last_restore_context,
            "excessive_comm" => cooldowns.last_excessive_comm,
            "pattern_detected" => cooldowns.last_pattern_detected,
            _ => None,
        };

        match last_time {
            Some(last) => now - last > cooldown_duration,
            None => true,
        }
    }

    pub(super) async fn update_cooldown(&self, suggestion_type: &str) {
        let mut cooldowns = self.cooldowns.write().await;
        let now = Utc::now();

        match suggestion_type {
            "break" => cooldowns.last_break = Some(now),
            "focus_time" => cooldowns.last_focus_time = Some(now),
            "restore_context" => cooldowns.last_restore_context = Some(now),
            "excessive_comm" => cooldowns.last_excessive_comm = Some(now),
            "pattern_detected" => cooldowns.last_pattern_detected = Some(now),
            _ => {}
        }
    }
}
