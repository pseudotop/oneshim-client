//! 제안 팝업 뷰.
//!
//! 제안 내용 + 수락/거절 버튼.

use oneshim_suggestion::presenter::SuggestionView;

/// 팝업 상태
#[derive(Debug, Clone)]
pub struct SuggestionPopupState {
    /// 현재 표시 중인 제안
    pub current: Option<SuggestionView>,
    /// 표시 여부
    pub is_visible: bool,
}

/// 팝업 액션
#[derive(Debug, Clone)]
pub enum PopupAction {
    /// 제안 수락
    Accept(String),
    /// 제안 거절
    Reject(String),
    /// 나중에 보기
    Defer(String),
    /// 닫기
    Dismiss,
}

impl SuggestionPopupState {
    /// 새 팝업 상태
    pub fn new() -> Self {
        Self {
            current: None,
            is_visible: false,
        }
    }

    /// 제안 표시
    pub fn show(&mut self, view: SuggestionView) {
        self.current = Some(view);
        self.is_visible = true;
    }

    /// 팝업 숨기기
    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    /// 팝업 닫기 (제안도 제거)
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
            title: "테스트".to_string(),
            body: "제안 내용".to_string(),
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
        assert!(state.current.is_some()); // 제안은 유지

        state.dismiss();
        assert!(state.current.is_none());
    }
}
