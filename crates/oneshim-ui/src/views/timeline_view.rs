//! 타임라인 뷰 (리와인드).

/// 타임라인 뷰 상태
#[derive(Debug, Clone)]
pub struct TimelineViewState {
    /// 표시 여부
    pub is_visible: bool,
    /// 현재 필터: 앱 이름
    pub filter_app: Option<String>,
    /// 현재 필터: 텍스트 검색
    pub filter_text: Option<String>,
    /// 선택된 프레임 인덱스
    pub selected_index: Option<usize>,
    /// 총 프레임 수
    pub total_frames: usize,
}

impl TimelineViewState {
    pub fn new() -> Self {
        Self {
            is_visible: false,
            filter_app: None,
            filter_text: None,
            selected_index: None,
            total_frames: 0,
        }
    }

    /// 필터 설정
    pub fn set_app_filter(&mut self, app: Option<String>) {
        self.filter_app = app;
    }

    /// 텍스트 검색 설정
    pub fn set_text_filter(&mut self, text: Option<String>) {
        self.filter_text = text;
    }

    /// 프레임 선택
    pub fn select_frame(&mut self, index: usize) {
        self.selected_index = Some(index);
    }
}

impl Default for TimelineViewState {
    fn default() -> Self {
        Self::new()
    }
}
