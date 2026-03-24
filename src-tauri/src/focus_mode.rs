use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Transient focus mode state. Not persisted across app restarts.
pub struct FocusModeState {
    active: AtomicBool,
    activated_at: RwLock<Option<DateTime<Utc>>>,
    duration_minutes: AtomicU32, // 0 = indefinite
}

impl FocusModeState {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            activated_at: RwLock::new(None),
            duration_minutes: AtomicU32::new(0),
        }
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn activate(&self, duration_minutes: u32) {
        self.duration_minutes
            .store(duration_minutes, Ordering::Relaxed);
        *self.activated_at.write() = Some(Utc::now());
        self.active.store(true, Ordering::Release);
    }

    pub fn deactivate(&self) {
        self.active.store(false, Ordering::Release);
        *self.activated_at.write() = None;
        self.duration_minutes.store(0, Ordering::Relaxed);
    }

    pub fn remaining_minutes(&self) -> Option<u32> {
        if !self.is_active() {
            return None;
        }
        let dur = self.duration_minutes.load(Ordering::Relaxed);
        if dur == 0 {
            return None; // indefinite
        }
        let activated = self.activated_at.read();
        let at = (*activated)?;
        let elapsed = (Utc::now() - at).num_minutes() as u32;
        Some(dur.saturating_sub(elapsed))
    }

    /// Check if timed focus mode has expired. Returns true if it was active
    /// and just expired (auto-deactivates).
    pub fn check_expiry(&self) -> bool {
        if !self.is_active() {
            return false;
        }
        let dur = self.duration_minutes.load(Ordering::Relaxed);
        if dur == 0 {
            return false; // indefinite never expires
        }
        let activated = self.activated_at.read();
        let Some(at) = *activated else { return false };
        let elapsed = (Utc::now() - at).num_minutes() as u32;
        if elapsed >= dur {
            drop(activated); // release read lock before write
            self.deactivate();
            true
        } else {
            false
        }
    }

    pub fn activated_at(&self) -> Option<DateTime<Utc>> {
        *self.activated_at.read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_is_inactive() {
        let state = FocusModeState::new();
        assert!(!state.is_active());
        assert!(state.remaining_minutes().is_none());
    }

    #[test]
    fn activate_then_check() {
        let state = FocusModeState::new();
        state.activate(30);
        assert!(state.is_active());
        assert!(state.remaining_minutes().is_some());
    }

    #[test]
    fn indefinite_has_no_remaining() {
        let state = FocusModeState::new();
        state.activate(0);
        assert!(state.is_active());
        assert!(state.remaining_minutes().is_none());
    }

    #[test]
    fn deactivate_clears_state() {
        let state = FocusModeState::new();
        state.activate(30);
        state.deactivate();
        assert!(!state.is_active());
        assert!(state.activated_at().is_none());
    }

    #[test]
    fn indefinite_does_not_expire() {
        let state = FocusModeState::new();
        state.activate(0);
        assert!(!state.check_expiry());
        assert!(state.is_active());
    }
}
