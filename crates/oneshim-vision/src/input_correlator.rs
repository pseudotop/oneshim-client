//! GUI element detection via OCR-input correlation.
//!
//! Phase 1 foundation: standalone detection logic.
//! Integration into the monitor loop pipeline is Phase 2
//! (see docs/superpowers/specs/2026-03-19-gui-activity-intelligence-design.md).

use oneshim_core::models::frame::{BoundingBox, OcrRegion};
use oneshim_core::models::gui_interaction::{GuiElement, GuiElementType};

/// Title bar detection threshold in pixels. Covers both standard (30px)
/// and 2x HiDPI displays. Adjust if targeting higher DPI configurations.
const TITLE_BAR_MAX_Y: u32 = 60;

/// Tab region: below title bar but still near top of screen.
const TAB_BAR_MAX_Y: u32 = 120;

/// Maximum text length considered a tab label.
const TAB_LABEL_MAX_LEN: usize = 30;

/// Minimum Y coordinate considered bottom-of-screen for status bar detection.
/// This is a heuristic; actual screen height is unknown at this layer.
const STATUS_BAR_MIN_Y: u32 = 900;

/// Correlates input events (mouse clicks, keyboard activity) with OCR-extracted
/// regions to determine which GUI element the user is interacting with.
pub struct InputOcrCorrelator;

impl InputOcrCorrelator {
    /// Given a mouse click position and OCR regions from the current frame,
    /// find which OCR region (if any) the click landed on.
    ///
    /// When multiple regions overlap at the click point, the smallest region
    /// (by area) is selected — this is typically the most specific element.
    pub fn correlate_click(
        click_x: u32,
        click_y: u32,
        regions: &[OcrRegion],
    ) -> Option<GuiElement> {
        regions
            .iter()
            .filter(|r| r.bbox.contains_point(click_x, click_y))
            .min_by_key(|r| r.bbox.area())
            .map(|r| GuiElement {
                text: r.text.clone(),
                bbox: r.bbox.clone(),
                element_type: Self::infer_element_type(&r.text, &r.bbox),
                confidence: r.confidence,
            })
    }

    /// Given keyboard activity and the cursor position, identify a text input element.
    ///
    /// Finds the OCR region at the cursor position and marks it as `TextInput`.
    pub fn correlate_typing(
        regions: &[OcrRegion],
        cursor_x: u32,
        cursor_y: u32,
    ) -> Option<GuiElement> {
        Self::correlate_click(cursor_x, cursor_y, regions).map(|mut e| {
            e.element_type = GuiElementType::TextInput;
            e
        })
    }

    /// Infer GUI element type from text content and position heuristics.
    ///
    /// Position thresholds are defined as module-level constants
    /// (`TITLE_BAR_MAX_Y`, `TAB_BAR_MAX_Y`, `STATUS_BAR_MIN_Y`).
    fn infer_element_type(text: &str, bbox: &BoundingBox) -> GuiElementType {
        let lower = text.to_lowercase();

        // Very top of screen — likely title bar or menu bar
        if bbox.y < TITLE_BAR_MAX_Y {
            return GuiElementType::TitleBar;
        }

        // Tab-like text: short, near top but below title bar
        if bbox.y < TAB_BAR_MAX_Y && text.len() < TAB_LABEL_MAX_LEN {
            return GuiElementType::TabLabel;
        }

        // Bottom of screen — likely status bar
        if bbox.y >= STATUS_BAR_MIN_Y {
            return GuiElementType::StatusBar;
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

        GuiElementType::Unknown
    }

    /// Check if text looks like a menu item with a keyboard shortcut.
    fn looks_like_menu_item(text: &str) -> bool {
        // Keyboard shortcut patterns: Ctrl+X, Cmd+X, Alt+X, ⌘X, ⇧⌘X
        let shortcut_prefixes = ["Ctrl+", "Cmd+", "Alt+", "Shift+", "⌘", "⇧"];
        shortcut_prefixes.iter().any(|prefix| text.contains(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_region(text: &str, x: u32, y: u32, w: u32, h: u32, confidence: f32) -> OcrRegion {
        OcrRegion {
            text: text.to_string(),
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
            confidence,
        }
    }

    #[test]
    fn correlate_click_finds_matching_region() {
        let regions = vec![
            make_region("Save", 100, 200, 60, 30, 0.9),
            make_region("Cancel", 180, 200, 80, 30, 0.85),
        ];

        let result = InputOcrCorrelator::correlate_click(120, 210, &regions);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.text, "Save");
        assert_eq!(elem.element_type, GuiElementType::Button);
    }

    #[test]
    fn correlate_click_returns_none_on_miss() {
        let regions = vec![make_region("Save", 100, 200, 60, 30, 0.9)];

        let result = InputOcrCorrelator::correlate_click(500, 500, &regions);
        assert!(result.is_none());
    }

    #[test]
    fn correlate_click_selects_smallest_overlapping_region() {
        // Large region containing a smaller one
        let regions = vec![
            make_region("Dialog", 50, 50, 300, 200, 0.8),
            make_region("OK", 150, 120, 40, 20, 0.9),
        ];

        let result = InputOcrCorrelator::correlate_click(160, 125, &regions);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.text, "OK");
    }

    #[test]
    fn correlate_click_empty_regions() {
        let result = InputOcrCorrelator::correlate_click(100, 100, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn correlate_typing_marks_as_text_input() {
        let regions = vec![make_region("Username", 100, 100, 200, 30, 0.85)];

        let result = InputOcrCorrelator::correlate_typing(&regions, 150, 110);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.element_type, GuiElementType::TextInput);
    }

    #[test]
    fn infer_element_type_title_bar() {
        let bbox = BoundingBox {
            x: 0,
            y: 10,
            width: 200,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("My Application", &bbox);
        assert_eq!(t, GuiElementType::TitleBar);
    }

    #[test]
    fn infer_element_type_link() {
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 200,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("https://example.com", &bbox);
        assert_eq!(t, GuiElementType::Link);
    }

    #[test]
    fn infer_element_type_button() {
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 60,
            height: 30,
        };
        let t = InputOcrCorrelator::infer_element_type("Save", &bbox);
        assert_eq!(t, GuiElementType::Button);
    }

    #[test]
    fn infer_element_type_unknown() {
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 400,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type(
            "The quick brown fox jumps over the lazy dog",
            &bbox,
        );
        assert_eq!(t, GuiElementType::Unknown);
    }

    #[test]
    fn infer_element_type_tab_label() {
        let bbox = BoundingBox {
            x: 100,
            y: 80,
            width: 80,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("main.rs", &bbox);
        assert_eq!(t, GuiElementType::TabLabel);
    }

    #[test]
    fn infer_element_type_status_bar() {
        let bbox = BoundingBox {
            x: 0,
            y: 950,
            width: 200,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("Ln 42, Col 10", &bbox);
        assert_eq!(t, GuiElementType::StatusBar);
    }

    #[test]
    fn infer_element_type_menu_item_shortcut() {
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 150,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("Save  Ctrl+S", &bbox);
        assert_eq!(t, GuiElementType::MenuItem);
    }

    #[test]
    fn infer_element_type_menu_item_mac_shortcut() {
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 150,
            height: 20,
        };
        let t = InputOcrCorrelator::infer_element_type("New File  ⌘N", &bbox);
        assert_eq!(t, GuiElementType::MenuItem);
    }

    #[test]
    fn looks_like_menu_item_detection() {
        assert!(InputOcrCorrelator::looks_like_menu_item("Save  Ctrl+S"));
        assert!(InputOcrCorrelator::looks_like_menu_item("⌘N"));
        assert!(InputOcrCorrelator::looks_like_menu_item("⇧⌘P"));
        assert!(InputOcrCorrelator::looks_like_menu_item("Alt+F4"));
        assert!(!InputOcrCorrelator::looks_like_menu_item("Save"));
        assert!(!InputOcrCorrelator::looks_like_menu_item("Hello World"));
    }
}
