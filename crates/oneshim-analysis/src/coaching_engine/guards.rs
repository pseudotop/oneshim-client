use chrono::{Local, NaiveTime, Utc};
use oneshim_core::config::CoachingConfig;
use oneshim_core::models::coaching::CoachingProfile;
use std::time::Instant;
use tracing::debug;

use super::CoachingEngine;

impl CoachingEngine {
    /// Check if the current time falls within any configured quiet hour range.
    pub(super) fn is_quiet_hour(config: &CoachingConfig) -> bool {
        if config.quiet_hours.is_empty() {
            return false;
        }

        let now = Local::now().time();

        for range in &config.quiet_hours {
            let start = match NaiveTime::parse_from_str(&range.start, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };
            let end = match NaiveTime::parse_from_str(&range.end, "%H:%M") {
                Ok(t) => t,
                Err(_) => continue,
            };

            if start <= end {
                // Normal range: e.g. 22:00 - 23:00
                if now >= start && now < end {
                    return true;
                }
            } else {
                // Overnight range: e.g. 22:00 - 06:00
                if now >= start || now < end {
                    return true;
                }
            }
        }

        false
    }

    /// Check if enough time has passed since the last alert for this profile.
    /// Returns `true` if the message should be allowed (cooldown passed).
    pub(super) async fn check_cooldown(
        &self,
        config: &CoachingConfig,
        profile: &CoachingProfile,
    ) -> bool {
        let profile_name = format!("{:?}", profile);
        let min_interval = config
            .profiles
            .get(&profile_name)
            .map(|p| p.min_interval_secs)
            .unwrap_or(300);

        let last = self.last_alert.read().await;
        match last.get(&profile_name) {
            Some(last_time) => {
                let elapsed = (Utc::now() - *last_time).num_seconds();
                elapsed >= min_interval as i64
            }
            None => true,
        }
    }

    /// Record the current time as the last alert for this profile.
    pub(super) async fn record_alert(&self, profile: &CoachingProfile) {
        let profile_name = format!("{:?}", profile);
        let mut last = self.last_alert.write().await;
        last.insert(profile_name, Utc::now());
    }

    /// Clear an expired snooze if present. Called before trigger detection.
    pub(super) async fn clear_expired_snooze(&self) {
        let mut guard = self.snoozed_until.write().await;
        if let Some((_, until)) = guard.as_ref() {
            if Instant::now() >= *until {
                *guard = None;
            }
        }
    }

    /// Check if the matched profile is currently snoozed.
    /// Returns `true` if snoozed (should suppress).
    pub(super) async fn is_profile_snoozed(&self, profile: &CoachingProfile) -> bool {
        let guard = self.snoozed_until.read().await;
        if let Some((ref snoozed_profile, until)) = *guard {
            let matched_profile_name = format!("{:?}", profile);
            if Instant::now() < until && matched_profile_name == *snoozed_profile {
                debug!(profile = %snoozed_profile, "coaching suppressed: snoozed");
                return true;
            }
        }
        false
    }
}
