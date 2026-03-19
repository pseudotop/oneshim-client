//! GUI element detection via OCR-input correlation.
//!
//! Phase 1 foundation with Phase 2 upgrades: word grouping pre-pass,
//! proximity fallback, PII filtering, and resolution-aware thresholds.
//!
//! Integration into the monitor loop pipeline via `gui_pipeline.rs`
//! (see docs/superpowers/specs/2026-03-19-gui-activity-intelligence-design.md).

use oneshim_core::config::PiiFilterLevel;
use oneshim_core::models::frame::{BoundingBox, OcrRegion};
use oneshim_core::models::gui_interaction::{GuiElement, GuiElementType};

use crate::privacy::sanitize_title_with_level;

/// Maximum text length considered a tab label.
const TAB_LABEL_MAX_LEN: usize = 30;

/// Default proximity threshold in pixels for fallback matching.
const DEFAULT_PROXIMITY_THRESHOLD_PX: u32 = 40;

/// Y-tolerance for same-line word grouping (pixels).
const WORD_GROUP_Y_TOLERANCE: u32 = 5;

/// Horizontal gap multiplier for word grouping: merge when gap < factor * avg_char_width.
const WORD_GROUP_GAP_FACTOR: f32 = 1.5;

/// Detects GUI elements from OCR regions and input coordinates.
///
/// Upgraded from Phase 1 `InputOcrCorrelator` with:
/// - Word grouping pre-pass to reduce OCR fragmentation
/// - Proximity fallback when no direct hit is found
/// - PII filtering on element text
/// - Resolution-aware position thresholds
pub struct GuiElementDetector {
    screen_resolution: (u32, u32),
    pii_filter_level: PiiFilterLevel,
    proximity_threshold_px: u32,
}

/// Backward-compatible type alias for Phase 1 callers.
pub type InputOcrCorrelator = GuiElementDetector;

impl GuiElementDetector {
    /// Create a new detector with screen resolution and PII filter level.
    pub fn new(screen_resolution: (u32, u32), pii_filter_level: PiiFilterLevel) -> Self {
        Self {
            screen_resolution,
            pii_filter_level,
            proximity_threshold_px: DEFAULT_PROXIMITY_THRESHOLD_PX,
        }
    }

    /// Override the default proximity threshold.
    pub fn with_proximity_threshold(mut self, px: u32) -> Self {
        self.proximity_threshold_px = px;
        self
    }

    /// Update the screen resolution used for proportional thresholds.
    ///
    /// Called when `WindowLayoutTracker` reports a new resolution so that
    /// element type inference remains accurate across monitor changes.
    /// Ignores zero-dimension values to protect against invalid inputs.
    pub fn update_resolution(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.screen_resolution = (width, height);
        }
    }

    /// Return the current screen resolution.
    pub fn resolution(&self) -> (u32, u32) {
        self.screen_resolution
    }

    /// Word grouping pre-pass: merge adjacent OCR words on the same line
    /// into unified regions.
    ///
    /// 1. Sort by Y (±tolerance for same-line), then X.
    /// 2. Merge when horizontal gap < 1.5× average char width.
    /// 3. Merged region inherits the bounding-box union and concatenated text.
    pub fn group_words(regions: &[OcrRegion]) -> Vec<OcrRegion> {
        if regions.is_empty() {
            return Vec::new();
        }

        let mut sorted: Vec<&OcrRegion> = regions.iter().collect();
        sorted.sort_by(|a, b| {
            let ay = a.bbox.y / (WORD_GROUP_Y_TOLERANCE + 1);
            let by = b.bbox.y / (WORD_GROUP_Y_TOLERANCE + 1);
            ay.cmp(&by).then(a.bbox.x.cmp(&b.bbox.x))
        });

        let mut grouped: Vec<OcrRegion> = Vec::new();

        for region in sorted {
            let should_merge = grouped.last().is_some_and(|prev: &OcrRegion| {
                // Same line check
                let y_diff = (prev.bbox.y as i64 - region.bbox.y as i64).unsigned_abs() as u32;
                if y_diff > WORD_GROUP_Y_TOLERANCE {
                    return false;
                }

                // Gap check
                let prev_right = prev.bbox.x + prev.bbox.width;
                let gap = region.bbox.x.saturating_sub(prev_right);

                // Estimate avg char width from the previous region
                let char_count = prev.text.len().max(1) as f32;
                let avg_char_width = prev.bbox.width as f32 / char_count;
                let max_gap = (avg_char_width * WORD_GROUP_GAP_FACTOR) as u32;

                gap <= max_gap
            });

            if should_merge {
                let prev = grouped.last_mut().unwrap();
                // Merge bounding boxes
                let min_x = prev.bbox.x.min(region.bbox.x);
                let min_y = prev.bbox.y.min(region.bbox.y);
                let max_x = (prev.bbox.x + prev.bbox.width).max(region.bbox.x + region.bbox.width);
                let max_y =
                    (prev.bbox.y + prev.bbox.height).max(region.bbox.y + region.bbox.height);
                prev.bbox = BoundingBox {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                };
                // Concatenate text with space
                prev.text.push(' ');
                prev.text.push_str(&region.text);
                // Keep the lower confidence
                prev.confidence = prev.confidence.min(region.confidence);
            } else {
                grouped.push(region.clone());
            }
        }

        grouped
    }

    /// Given a mouse click position and OCR regions from the current frame,
    /// find which OCR region (if any) the click landed on.
    ///
    /// 1. Direct hit: smallest region containing the point.
    /// 2. Proximity fallback: nearest region within `proximity_threshold_px`.
    ///
    /// PII filter is applied to the resulting element text.
    pub fn correlate_click(
        &self,
        click_x: u32,
        click_y: u32,
        regions: &[OcrRegion],
    ) -> Option<GuiElement> {
        // 1. Direct hit — smallest containing region
        let direct = regions
            .iter()
            .filter(|r| r.bbox.contains_point(click_x, click_y))
            .min_by_key(|r| r.bbox.area());

        let matched = if let Some(r) = direct {
            Some(r)
        } else {
            // 2. Proximity fallback — nearest within threshold
            let threshold = self.proximity_threshold_px as f64;
            regions
                .iter()
                .filter_map(|r| {
                    let dist = Self::distance_to_bbox(click_x, click_y, &r.bbox);
                    if dist <= threshold {
                        Some((r, dist))
                    } else {
                        None
                    }
                })
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(r, _)| r)
        };

        matched.map(|r| {
            let filtered_text = sanitize_title_with_level(&r.text, self.pii_filter_level);
            GuiElement {
                text: filtered_text,
                bbox: r.bbox.clone(),
                element_type: self.infer_element_type(&r.text, &r.bbox),
                confidence: r.confidence,
            }
        })
    }

    /// Given keyboard activity and the cursor position, identify a text input element.
    ///
    /// Finds the OCR region at the cursor position and marks it as `TextInput`.
    pub fn correlate_typing(
        &self,
        regions: &[OcrRegion],
        cursor_x: u32,
        cursor_y: u32,
    ) -> Option<GuiElement> {
        self.correlate_click(cursor_x, cursor_y, regions)
            .map(|mut e| {
                e.element_type = GuiElementType::TextInput;
                e
            })
    }

    /// Euclidean distance from a point to the nearest edge of a bounding box.
    fn distance_to_bbox(px: u32, py: u32, bbox: &BoundingBox) -> f64 {
        let cx = px as f64;
        let cy = py as f64;
        let bx_min = bbox.x as f64;
        let by_min = bbox.y as f64;
        let bx_max = (bbox.x + bbox.width) as f64;
        let by_max = (bbox.y + bbox.height) as f64;

        let dx = if cx < bx_min {
            bx_min - cx
        } else if cx > bx_max {
            cx - bx_max
        } else {
            0.0
        };
        let dy = if cy < by_min {
            by_min - cy
        } else if cy > by_max {
            cy - by_max
        } else {
            0.0
        };

        (dx * dx + dy * dy).sqrt()
    }

    /// Infer GUI element type from text content and position heuristics.
    ///
    /// Uses `screen_resolution` for proportional thresholds instead of
    /// hardcoded pixel values.
    fn infer_element_type(&self, text: &str, bbox: &BoundingBox) -> GuiElementType {
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
    fn looks_like_menu_item(text: &str) -> bool {
        let shortcut_prefixes = ["Ctrl+", "Cmd+", "Alt+", "Shift+", "⌘", "⇧"];
        shortcut_prefixes.iter().any(|prefix| text.contains(prefix))
    }
}

// TODO Phase 3: Per-word OCR confidence scores.
// The `leptess` crate does not currently expose per-word confidence via
// `TessBaseAPIGetIterator` + `RIL_WORD`. To obtain per-word confidence,
// we would need to either:
// 1. Contribute upstream to `leptess` to expose `ResultIterator` with
//    confidence per word-level iterator, or
// 2. Use raw Tesseract FFI (`TessBaseAPIGetIterator`, `TessPageIteratorGetConfidence`)
//    directly, bypassing `leptess`.
// For now, `OcrRegion.confidence` uses `mean_text_conf()` for the entire page.

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

    fn detector() -> GuiElementDetector {
        // 1920x1080 standard resolution, PII off for tests
        GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off)
    }

    #[test]
    fn correlate_click_finds_matching_region() {
        let d = detector();
        let regions = vec![
            make_region("Save", 100, 200, 60, 30, 0.9),
            make_region("Cancel", 180, 200, 80, 30, 0.85),
        ];

        let result = d.correlate_click(120, 210, &regions);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.text, "Save");
        assert_eq!(elem.element_type, GuiElementType::Button);
    }

    #[test]
    fn correlate_click_proximity_fallback() {
        let d = detector();
        // Click is 20px away from region — within default 40px threshold
        let regions = vec![make_region("Save", 100, 200, 60, 30, 0.9)];

        let result = d.correlate_click(80, 210, &regions);
        assert!(result.is_some());
        assert_eq!(result.unwrap().text, "Save");
    }

    #[test]
    fn correlate_click_returns_none_beyond_threshold() {
        let d = detector();
        let regions = vec![make_region("Save", 100, 200, 60, 30, 0.9)];

        // Click is far outside threshold
        let result = d.correlate_click(500, 500, &regions);
        assert!(result.is_none());
    }

    #[test]
    fn correlate_click_selects_smallest_overlapping_region() {
        let d = detector();
        let regions = vec![
            make_region("Dialog", 50, 50, 300, 200, 0.8),
            make_region("OK", 150, 120, 40, 20, 0.9),
        ];

        let result = d.correlate_click(160, 125, &regions);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.text, "OK");
    }

    #[test]
    fn correlate_click_empty_regions() {
        let d = detector();
        let result = d.correlate_click(100, 100, &[]);
        assert!(result.is_none());
    }

    #[test]
    fn correlate_typing_marks_as_text_input() {
        let d = detector();
        let regions = vec![make_region("Username", 100, 200, 200, 30, 0.85)];

        let result = d.correlate_typing(&regions, 150, 210);
        assert!(result.is_some());
        let elem = result.unwrap();
        assert_eq!(elem.element_type, GuiElementType::TextInput);
    }

    #[test]
    fn infer_element_type_title_bar() {
        let d = detector();
        let bbox = BoundingBox {
            x: 0,
            y: 10,
            width: 200,
            height: 20,
        };
        let t = d.infer_element_type("My Application", &bbox);
        assert_eq!(t, GuiElementType::TitleBar);
    }

    #[test]
    fn infer_element_type_link() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 200,
            height: 20,
        };
        let t = d.infer_element_type("https://example.com", &bbox);
        assert_eq!(t, GuiElementType::Link);
    }

    #[test]
    fn infer_element_type_button() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 60,
            height: 30,
        };
        let t = d.infer_element_type("Save", &bbox);
        assert_eq!(t, GuiElementType::Button);
    }

    #[test]
    fn infer_element_type_text_region_multiword() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 400,
            height: 20,
        };
        // Multi-word text (3+ words) that doesn't match any other pattern → TextRegion
        let t = d.infer_element_type("The quick brown fox jumps over the lazy dog", &bbox);
        assert_eq!(t, GuiElementType::TextRegion);
    }

    #[test]
    fn infer_element_type_unknown_short_text() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 60,
            height: 20,
        };
        // Short non-matching text (< 3 words) → Unknown
        let t = d.infer_element_type("xy", &bbox);
        assert_eq!(t, GuiElementType::Unknown);
    }

    #[test]
    fn infer_element_type_tab_label() {
        let d = detector();
        let bbox = BoundingBox {
            x: 100,
            y: 80,
            width: 80,
            height: 20,
        };
        let t = d.infer_element_type("main.rs", &bbox);
        assert_eq!(t, GuiElementType::TabLabel);
    }

    #[test]
    fn infer_element_type_status_bar() {
        let d = detector();
        let bbox = BoundingBox {
            x: 0,
            y: 1050,
            width: 200,
            height: 20,
        };
        let t = d.infer_element_type("Ln 42, Col 10", &bbox);
        assert_eq!(t, GuiElementType::StatusBar);
    }

    #[test]
    fn infer_element_type_menu_item_shortcut() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 150,
            height: 20,
        };
        let t = d.infer_element_type("Save  Ctrl+S", &bbox);
        assert_eq!(t, GuiElementType::MenuItem);
    }

    #[test]
    fn infer_element_type_menu_item_mac_shortcut() {
        let d = detector();
        let bbox = BoundingBox {
            x: 50,
            y: 300,
            width: 150,
            height: 20,
        };
        let t = d.infer_element_type("New File  ⌘N", &bbox);
        assert_eq!(t, GuiElementType::MenuItem);
    }

    #[test]
    fn looks_like_menu_item_detection() {
        assert!(GuiElementDetector::looks_like_menu_item("Save  Ctrl+S"));
        assert!(GuiElementDetector::looks_like_menu_item("⌘N"));
        assert!(GuiElementDetector::looks_like_menu_item("⇧⌘P"));
        assert!(GuiElementDetector::looks_like_menu_item("Alt+F4"));
        assert!(!GuiElementDetector::looks_like_menu_item("Save"));
        assert!(!GuiElementDetector::looks_like_menu_item("Hello World"));
    }

    #[test]
    fn infer_element_type_tree_item() {
        let d = detector();
        let bbox = BoundingBox {
            x: 20,
            y: 300,
            width: 150,
            height: 20,
        };
        let t = d.infer_element_type("▸ src", &bbox);
        assert_eq!(t, GuiElementType::TreeItem);
    }

    #[test]
    fn word_grouping_merges_adjacent() {
        let regions = vec![
            make_region("Hello", 10, 100, 50, 20, 0.9),
            make_region("World", 65, 100, 50, 20, 0.9),
        ];

        let grouped = GuiElementDetector::group_words(&regions);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].text, "Hello World");
        assert_eq!(grouped[0].bbox.x, 10);
        assert_eq!(grouped[0].bbox.width, 105); // 10..115
    }

    #[test]
    fn word_grouping_splits_distant_words() {
        let regions = vec![
            make_region("Hello", 10, 100, 50, 20, 0.9),
            make_region("World", 500, 100, 50, 20, 0.9),
        ];

        let grouped = GuiElementDetector::group_words(&regions);
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn word_grouping_splits_different_lines() {
        let regions = vec![
            make_region("Line1", 10, 100, 50, 20, 0.9),
            make_region("Line2", 10, 200, 50, 20, 0.9),
        ];

        let grouped = GuiElementDetector::group_words(&regions);
        assert_eq!(grouped.len(), 2);
    }

    #[test]
    fn word_grouping_empty() {
        let grouped = GuiElementDetector::group_words(&[]);
        assert!(grouped.is_empty());
    }

    #[test]
    fn pii_filter_applied_to_element_text() {
        let d = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Basic);
        let regions = vec![make_region("user@example.com", 100, 200, 200, 30, 0.9)];

        let result = d.correlate_click(150, 210, &regions);
        assert!(result.is_some());
        let elem = result.unwrap();
        // Basic PII filter masks emails
        assert!(!elem.text.contains("user@example.com"));
    }

    #[test]
    fn infer_element_type_toolbar_icon() {
        let d = detector();
        // Near top of window (within 2× title bar height), small box, no text
        // Title bar max = 1080 * 0.04 = 43, toolbar max = 86
        let bbox = BoundingBox {
            x: 200,
            y: 50,
            width: 30,
            height: 30,
        };
        let t = d.infer_element_type("", &bbox);
        assert_eq!(t, GuiElementType::ToolbarIcon);
    }

    #[test]
    fn infer_element_type_toolbar_icon_single_char() {
        let d = detector();
        let bbox = BoundingBox {
            x: 200,
            y: 60,
            width: 24,
            height: 24,
        };
        // Single-char icon label (e.g., "X" close icon)
        let t = d.infer_element_type("X", &bbox);
        assert_eq!(t, GuiElementType::ToolbarIcon);
    }

    #[test]
    fn infer_element_type_scrollbar_right_edge() {
        let d = detector();
        // 1920×1080 screen — right edge starts at 1920-20=1900
        let bbox = BoundingBox {
            x: 1905,
            y: 200,
            width: 15,
            height: 400,
        };
        let t = d.infer_element_type("", &bbox);
        assert_eq!(t, GuiElementType::ScrollBar);
    }

    #[test]
    fn infer_element_type_scrollbar_bottom_edge() {
        let d = detector();
        // 1920×1080 screen — bottom edge starts at 1080-20=1060
        // Also must be below status_bar_min_y (1026), but scrollbar check
        // is after status bar, so test at the right edge instead
        let bbox = BoundingBox {
            x: 1905,
            y: 500,
            width: 12,
            height: 300,
        };
        let t = d.infer_element_type("", &bbox);
        assert_eq!(t, GuiElementType::ScrollBar);
    }

    #[test]
    fn distance_to_bbox_inside() {
        let bbox = BoundingBox {
            x: 100,
            y: 100,
            width: 50,
            height: 30,
        };
        assert_eq!(GuiElementDetector::distance_to_bbox(120, 110, &bbox), 0.0);
    }

    #[test]
    fn distance_to_bbox_outside() {
        let bbox = BoundingBox {
            x: 100,
            y: 100,
            width: 50,
            height: 30,
        };
        // 10px to the left of the bbox
        let dist = GuiElementDetector::distance_to_bbox(90, 110, &bbox);
        assert!((dist - 10.0).abs() < 0.01);
    }

    #[test]
    fn update_resolution_changes_thresholds() {
        let mut d = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off);
        assert_eq!(d.resolution(), (1920, 1080));

        // Title bar threshold at 1080p: 1080 * 0.04 = 43px
        let bbox_at_30 = BoundingBox {
            x: 0,
            y: 30,
            width: 200,
            height: 20,
        };
        assert_eq!(
            d.infer_element_type("File", &bbox_at_30),
            GuiElementType::TitleBar
        );

        // Switch to 4K resolution
        d.update_resolution(3840, 2160);
        assert_eq!(d.resolution(), (3840, 2160));

        // Same bbox at y=30 is now well within the title bar (2160 * 0.04 = 86px)
        assert_eq!(
            d.infer_element_type("File", &bbox_at_30),
            GuiElementType::TitleBar
        );

        // y=50 was NOT title bar at 1080p (43px threshold), but IS at 4K (86px threshold)
        let bbox_at_50 = BoundingBox {
            x: 200,
            y: 50,
            width: 30,
            height: 30,
        };
        // At 1080p this would be ToolbarIcon (below 43px title bar, within 86px toolbar)
        let d_1080 = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off);
        assert_eq!(
            d_1080.infer_element_type("", &bbox_at_50),
            GuiElementType::ToolbarIcon
        );
        // At 4K this is TitleBar (below 86px threshold)
        assert_eq!(
            d.infer_element_type("", &bbox_at_50),
            GuiElementType::TitleBar
        );
    }

    #[test]
    fn update_resolution_ignores_zero() {
        let mut d = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off);

        // Zero dimensions should be rejected
        d.update_resolution(0, 0);
        assert_eq!(d.resolution(), (1920, 1080));

        d.update_resolution(0, 1080);
        assert_eq!(d.resolution(), (1920, 1080));

        d.update_resolution(1920, 0);
        assert_eq!(d.resolution(), (1920, 1080));
    }

    #[test]
    fn zero_resolution_bug_regression() {
        // Regression test: with (0,0) resolution, ALL elements are classified
        // as TitleBar because title_bar_max_y = 0*0.04 = 0, and bbox.y < 0 is
        // always false, so the first threshold is "passed". Actually,
        // title_bar_max_y=0 means bbox.y < 0 is never true for u32, so this
        // test verifies the fix works correctly.
        let d_bad = GuiElementDetector::new((0, 0), PiiFilterLevel::Off);
        let bbox_mid = BoundingBox {
            x: 100,
            y: 300,
            width: 60,
            height: 30,
        };
        // With (0,0): status_bar_min_y = 0*0.95 = 0, so bbox.y(300) >= 0 → StatusBar!
        let t = d_bad.infer_element_type("Save", &bbox_mid);
        assert_eq!(
            t,
            GuiElementType::StatusBar,
            "bug: (0,0) misclassifies mid-screen as StatusBar"
        );

        // With proper resolution, "Save" button at y=300 should be Button
        let d_good = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off);
        let t = d_good.infer_element_type("Save", &bbox_mid);
        assert_eq!(
            t,
            GuiElementType::Button,
            "with proper resolution, Save button is correctly identified"
        );
    }
}
