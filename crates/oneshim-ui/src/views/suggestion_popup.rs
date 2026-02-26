//!

use oneshim_suggestion::presenter::SuggestionView;

#[derive(Debug, Clone)]
pub struct SuggestionPopupState {
    pub current: Option<SuggestionView>,
    pub is_visible: bool,
}

#[derive(Debug, Clone)]
pub enum PopupAction {
    Accept(String),
    Reject(String),
    Defer(String),
    Dismiss,
}

impl SuggestionPopupState {
    pub fn new() -> Self {
        Self {
            current: None,
            is_visible: false,
        }
    }

    pub fn show(&mut self, view: SuggestionView) {
        self.current = Some(view);
        self.is_visible = true;
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    pub fn dismiss(&mut self) {
        self.current = None;
        self.is_visible = false;
    }
}

impl Default for SuggestionPopupState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view() -> SuggestionView {
        SuggestionView {
            id: "sug_001".to_string(),
            title: "test".to_string(),
            body: "suggestion within용".to_string(),
            priority_label: "높음".to_string(),
            priority_color: "#F97316".to_string(),
            type_icon: "compass".to_string(),
            confidence_text: "95%".to_string(),
            is_actionable: true,
            time_text: "방금 전".to_string(),
        }
    }

    #[test]
    fn show_and_hide() {
        let mut state = SuggestionPopupState::new();
        assert!(!state.is_visible);

        state.show(make_view());
        assert!(state.is_visible);
        assert!(state.current.is_some());

        state.hide();
        assert!(!state.is_visible);
        assert!(state.current.is_some()); // suggestion
        state.dismiss();
        assert!(state.current.is_none());
    }
}
