//! GUI element type inference and app-specific overrides.

use oneshim_core::models::frame::BoundingBox;
use oneshim_core::models::frame::OcrRegion;
use oneshim_core::models::gui_interaction::{GuiElement, GuiElementType};

use super::{GuiElementDetector, TAB_LABEL_MAX_LEN};

impl GuiElementDetector {
    pub(super) fn infer_element_type(&self, text: &str, bbox: &BoundingBox) -> GuiElementType {
        let lower = text.to_lowercase();
        let (screen_w, screen_h) = self.screen_resolution;

        // Proportional thresholds based on screen height
        let title_bar_max_y = (screen_h as f64 * 0.04) as u32; // ~4% from top
        let tab_bar_max_y = (screen_h as f64 * 0.09) as u32; // ~9% from top
        let status_bar_min_y = (screen_h as f64 * 0.95) as u32; // ~95% from top

        // Very top of screen — likely title bar or menu bar
        if bbox.y < title_bar_max_y {
            return GuiElementType::TitleBar;
        }

        // Toolbar icon: near top (below title bar, within 2× title bar height),
        // small bounding box (<50×50), no text or very short text (1-2 chars)
        let toolbar_max_y = title_bar_max_y * 2;
        if bbox.y < toolbar_max_y
            && bbox.width < 50
            && bbox.height < 50
            && text.chars().count() <= 2
        {
            return GuiElementType::ToolbarIcon;
        }

        // Tab-like text: short, near top but below title bar
        if bbox.y < tab_bar_max_y && text.len() < TAB_LABEL_MAX_LEN {
            return GuiElementType::TabLabel;
        }

        // Bottom of screen — likely status bar
        if bbox.y >= status_bar_min_y {
            return GuiElementType::StatusBar;
        }

        // Scroll bar: narrow element at far right or bottom edge
        let scrollbar_narrow_threshold = 20;
        let right_edge = screen_w.saturating_sub(scrollbar_narrow_threshold);
        let bottom_edge = screen_h.saturating_sub(scrollbar_narrow_threshold);
        if (bbox.x >= right_edge && bbox.width < scrollbar_narrow_threshold)
            || (bbox.y >= bottom_edge && bbox.height < scrollbar_narrow_threshold)
        {
            return GuiElementType::ScrollBar;
        }

        // URLs
        if lower.contains("http") || lower.contains("://") {
            return GuiElementType::Link;
        }

        // Text with keyboard shortcut patterns (e.g., "Ctrl+S", "⌘N", "Alt+F4")
        if Self::looks_like_menu_item(text) {
            return GuiElementType::MenuItem;
        }

        // Common button labels
        if text.len() < 15
            && (lower.contains("save")
                || lower.contains("cancel")
                || lower.contains("ok")
                || lower.contains("submit")
                || lower.contains("close")
                || lower.contains("apply")
                || lower.contains("delete"))
        {
            return GuiElementType::Button;
        }

        // Phase 2: tree items (indented text with tree-like prefixes)
        if lower.starts_with("▸")
            || lower.starts_with("▾")
            || lower.starts_with("►")
            || lower.starts_with("▼")
        {
            return GuiElementType::TreeItem;
        }

        // Text region: multi-word text (3+ words) that doesn't match any other
        // pattern — fallback before Unknown
        let word_count = text.split_whitespace().count();
        if word_count >= 3 {
            return GuiElementType::TextRegion;
        }

        GuiElementType::Unknown
    }

    /// Check if text looks like a menu item with a keyboard shortcut.
    pub(super) fn looks_like_menu_item(text: &str) -> bool {
        let shortcut_prefixes = ["Ctrl+", "Cmd+", "Alt+", "Shift+", "⌘", "⇧"];
        shortcut_prefixes.iter().any(|prefix| text.contains(prefix))
    }

    /// Like `correlate_click`, but applies app-specific element type overrides.
    pub fn correlate_click_with_app(
        &self,
        click_x: u32,
        click_y: u32,
        regions: &[OcrRegion],
        app_name: &str,
    ) -> Option<GuiElement> {
        self.correlate_click(click_x, click_y, regions)
            .map(|mut e| {
                if let Some(override_type) = self.app_override(app_name, &e.text, &e.bbox) {
                    e.element_type = override_type;
                }
                e
            })
    }

    /// App-specific element type overrides for well-known applications.
    fn app_override(
        &self,
        app_name: &str,
        text: &str,
        bbox: &BoundingBox,
    ) -> Option<GuiElementType> {
        let lower = app_name.to_lowercase();
        let (screen_w, screen_h) = self.screen_resolution;

        // IDE apps
        if [
            "code",
            "visual studio",
            "intellij",
            "pycharm",
            "webstorm",
            "xcode",
            "cursor",
            "zed",
        ]
        .iter()
        .any(|k| lower.contains(k))
        {
            let sidebar_max_x = screen_w / 5;
            if bbox.x < sidebar_max_x && bbox.y > (screen_h as f64 * 0.09) as u32 {
                return Some(GuiElementType::TreeItem);
            }
        }

        // Browser apps
        if ["chrome", "safari", "firefox", "edge", "arc", "brave"]
            .iter()
            .any(|k| lower.contains(k))
        {
            let url_bar_max_y = (screen_h as f64 * 0.08) as u32;
            if bbox.y < url_bar_max_y && (text.contains('.') || text.contains('/')) {
                return Some(GuiElementType::Link);
            }
        }

        // Chat/Communication apps
        if ["slack", "teams", "discord", "telegram", "messages"]
            .iter()
            .any(|k| lower.contains(k))
        {
            let sidebar_max_x = screen_w / 4;
            if bbox.x < sidebar_max_x && bbox.y > (screen_h as f64 * 0.09) as u32 {
                return Some(GuiElementType::TreeItem);
            }
        }

        None
    }
}
