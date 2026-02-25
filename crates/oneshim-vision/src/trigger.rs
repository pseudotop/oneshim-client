//!

use chrono::{DateTime, Duration, Utc};
use oneshim_core::models::event::ContextEvent;
use oneshim_core::ports::vision::CaptureRequest;
use oneshim_core::ports::vision::CaptureTrigger;
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

pub struct SmartCaptureTrigger {
    last_capture: Option<DateTime<Utc>>,
    prev_app_name: Option<String>,
    throttle_ms: u64,
}

impl SmartCaptureTrigger {
    pub fn new(throttle_ms: u64) -> Self {
        Self {
            last_capture: None,
            prev_app_name: None,
            throttle_ms,
        }
    }

    fn classify_event(&self, event: &ContextEvent) -> TriggerType {
        let title_lower = event.window_title.to_lowercase();
        if title_lower.contains("error")
            || title_lower.contains("exception")
            || title_lower.contains("error")
            || title_lower.contains("error")
        {
            return TriggerType::ErrorDetected;
        }

        if let Some(prev) = &event.prev_app_name {
            if prev != &event.app_name {
                return TriggerType::ContextSwitch;
            }
        } else if let Some(prev) = &self.prev_app_name {
            if prev != &event.app_name {
                return TriggerType::WindowChange;
            }
        }

        TriggerType::Regular
    }

    fn compute_importance(&self, trigger_type: &TriggerType) -> f32 {
        match trigger_type {
            TriggerType::ErrorDetected => 0.9,
            TriggerType::FormSubmission => 0.8,
            TriggerType::ContextSwitch => 0.7,
            TriggerType::WindowChange => 0.6,
            TriggerType::SignificantAction => 0.5,
            TriggerType::Regular => 0.2,
        }
    }

    fn is_throttled(&self, now: DateTime<Utc>) -> bool {
        match self.last_capture {
            Some(last) => {
                let elapsed = now - last;
                elapsed < Duration::milliseconds(self.throttle_ms as i64)
            }
            None => false,
        }
    }
}

impl CaptureTrigger for SmartCaptureTrigger {
    fn should_capture(&mut self, event: &ContextEvent) -> Option<CaptureRequest> {
        let now = event.timestamp;
        let trigger_type = self.classify_event(event);
        let importance = self.compute_importance(&trigger_type);

        if importance < 0.8 && self.is_throttled(now) {
            debug!(
                "capture: {:?} (in progress {:.1})",
                trigger_type, importance
            );
            return None;
        }

        self.last_capture = Some(now);
        self.prev_app_name = Some(event.app_name.clone());

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
        let mut trigger = SmartCaptureTrigger::new(5000);
        let event = make_event("Code", "test.rs", Some("Firefox"));
        let req = trigger.should_capture(&event);
        assert!(req.is_some());
        let req = req.unwrap();
        assert_eq!(req.trigger_type, "ContextSwitch");
        assert!(req.importance >= 0.7);
    }

    #[test]
    fn error_detection() {
        let mut trigger = SmartCaptureTrigger::new(5000);
        let event = make_event("Terminal", "Error: command failed", None);
        let req = trigger.should_capture(&event);
        assert!(req.is_some());
        assert!(req.unwrap().importance >= 0.8);
    }

    #[test]
    fn throttle_low_importance() {
        let mut trigger = SmartCaptureTrigger::new(5000);

        let event1 = make_event("Code", "main.rs", None);
        assert!(trigger.should_capture(&event1).is_some());

        let event2 = make_event("Code", "lib.rs", None);
        assert!(trigger.should_capture(&event2).is_none());
    }

    #[test]
    fn high_importance_bypasses_throttle() {
        let mut trigger = SmartCaptureTrigger::new(5000);

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
