use chrono::{DateTime, Duration, Utc};
use oneshim_core::models::event::ContextEvent;
use oneshim_core::ports::vision::CaptureRequest;
use oneshim_core::ports::vision::CaptureTrigger;
use std::sync::Mutex;
use tracing::debug;

#[derive(Debug, Clone, PartialEq)]
pub enum TriggerType {
    WindowChange,
    ErrorDetected,
    SignificantAction,
    FormSubmission,
    ContextSwitch,
    Regular,
}

struct TriggerState {
    last_capture: Option<DateTime<Utc>>,
    prev_app_name: Option<String>,
}

pub struct SmartCaptureTrigger {
    state: Mutex<TriggerState>,
    throttle_ms: u64,
}

impl SmartCaptureTrigger {
    pub fn new(throttle_ms: u64) -> Self {
        Self {
            state: Mutex::new(TriggerState {
                last_capture: None,
                prev_app_name: None,
            }),
            throttle_ms,
        }
    }

    fn classify_event(event: &ContextEvent, prev_app_name: &Option<String>) -> TriggerType {
        let title_lower = event.window_title.to_lowercase();
        if title_lower.contains("error") || title_lower.contains("exception") {
            return TriggerType::ErrorDetected;
        }

        if let Some(prev) = &event.prev_app_name {
            if prev != &event.app_name {
                return TriggerType::ContextSwitch;
            }
        } else if let Some(prev) = prev_app_name {
            if prev != &event.app_name {
                return TriggerType::WindowChange;
            }
        }

        TriggerType::Regular
    }

    pub fn compute_importance(&self, trigger_type: &TriggerType) -> f32 {
        match trigger_type {
            TriggerType::ErrorDetected => 0.9,
            TriggerType::FormSubmission => 0.8,
            TriggerType::ContextSwitch => 0.7,
            TriggerType::WindowChange => 0.6,
            TriggerType::SignificantAction => 0.5,
            TriggerType::Regular => 0.2,
        }
    }

    fn is_throttled(
        last_capture: &Option<DateTime<Utc>>,
        now: DateTime<Utc>,
        throttle_ms: u64,
    ) -> bool {
        match last_capture {
            Some(last) => {
                let elapsed = now - *last;
                elapsed < Duration::milliseconds(throttle_ms as i64)
            }
            None => false,
        }
    }
}

impl CaptureTrigger for SmartCaptureTrigger {
    fn should_capture(&self, event: &ContextEvent) -> Option<CaptureRequest> {
        let mut state = self
            .state
            .lock()
            .expect("SmartCaptureTrigger state lock was poisoned by a panicking thread");
        let now = event.timestamp;
        let trigger_type = Self::classify_event(event, &state.prev_app_name);
        let importance = self.compute_importance(&trigger_type);

        if importance < 0.8 && Self::is_throttled(&state.last_capture, now, self.throttle_ms) {
            debug!(
                "capture: {:?} (in progress {:.1})",
                trigger_type, importance
            );
            return None;
        }

        state.last_capture = Some(now);
        state.prev_app_name = Some(event.app_name.clone());

        let trigger_type_str = format!("{:?}", trigger_type);
        debug!(
            "capture approval: {} (in progress {:.1})",
            trigger_type_str, importance
        );

        Some(CaptureRequest {
            trigger_type: trigger_type_str,
            importance,
            app_name: event.app_name.clone(),
            window_title: event.window_title.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(app: &str, title: &str, prev: Option<&str>) -> ContextEvent {
        ContextEvent {
            app_name: app.to_string(),
            window_title: title.to_string(),
            prev_app_name: prev.map(String::from),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn window_change_trigger() {
        let trigger = SmartCaptureTrigger::new(5000);
        let event = make_event("Code", "test.rs", Some("Firefox"));
        let req = trigger.should_capture(&event);
        assert!(req.is_some());
        let req = req.unwrap();
        assert_eq!(req.trigger_type, "ContextSwitch");
        assert!(req.importance >= 0.7);
    }

    #[test]
    fn error_detection() {
        let trigger = SmartCaptureTrigger::new(5000);
        let event = make_event("Terminal", "Error: command failed", None);
        let req = trigger.should_capture(&event);
        assert!(req.is_some());
        assert!(req.unwrap().importance >= 0.8);
    }

    #[test]
    fn throttle_low_importance() {
        let trigger = SmartCaptureTrigger::new(5000);

        let event1 = make_event("Code", "main.rs", None);
        assert!(trigger.should_capture(&event1).is_some());

        let event2 = make_event("Code", "lib.rs", None);
        assert!(trigger.should_capture(&event2).is_none());
    }

    #[test]
    fn high_importance_bypasses_throttle() {
        let trigger = SmartCaptureTrigger::new(5000);

        let event1 = make_event("Code", "main.rs", None);
        trigger.should_capture(&event1);

        let event2 = make_event("Terminal", "Error: panic", None);
        assert!(trigger.should_capture(&event2).is_some());
    }

    #[test]
    fn importance_scores() {
        let trigger = SmartCaptureTrigger::new(5000);
        assert_eq!(trigger.compute_importance(&TriggerType::ErrorDetected), 0.9);
        assert_eq!(trigger.compute_importance(&TriggerType::ContextSwitch), 0.7);
        assert_eq!(trigger.compute_importance(&TriggerType::Regular), 0.2);
    }
}
