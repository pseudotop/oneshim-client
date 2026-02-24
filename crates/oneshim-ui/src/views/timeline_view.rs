
#[derive(Debug, Clone)]
pub struct TimelineViewState {
    pub is_visible: bool,
    pub filter_app: Option<String>,
    pub filter_text: Option<String>,
    pub selected_index: Option<usize>,
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

    pub fn set_app_filter(&mut self, app: Option<String>) {
        self.filter_app = app;
    }

    pub fn set_text_filter(&mut self, text: Option<String>) {
        self.filter_text = text;
    }

    pub fn select_frame(&mut self, index: usize) {
        self.selected_index = Some(index);
    }
}

impl Default for TimelineViewState {
    fn default() -> Self {
        Self::new()
    }
}
