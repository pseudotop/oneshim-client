//! GUI Activity Intelligence pipeline for the scheduler.
//!
//! Integrates `GuiElementDetector` and `GuiActivityAggregator` into a single
//! `run_gui_tick()` function that follows the `analysis_pipeline::run_analysis_tick()`
//! pattern. `GuiWorkTypeRefiner` is called from the analysis pipeline, not here
//! (see `GuiPipelineState` doc comment for rationale).
//!
//! Called from the monitor loop after `run_analysis_tick()`. The returned
//! `GuiActivitySummary` is fed into `ContentTracker` on the next tick.

use oneshim_analysis::gui_aggregator::GuiActivityAggregator;
use oneshim_core::models::event::InputActivityEvent;
use oneshim_core::models::focused_element::FocusedElementInfo;
use oneshim_core::models::frame::OcrRegion;
use oneshim_core::models::gui_activity::GuiActivitySummary;
use oneshim_core::models::gui_interaction::{
    GuiElement, GuiElementType, GuiInteractionEvent, GuiInteractionType, InteractionType,
};
use oneshim_vision::gui_detector::GuiElementDetector;

use chrono::Utc;

/// Mutable state for the GUI pipeline, owned by the monitor loop.
///
/// Note: `GuiWorkTypeRefiner` is intentionally NOT included here. The refiner
/// requires an initial `WorkType` from the analysis pipeline, which runs
/// separately. `GuiWorkTypeRefiner::refine()` is called from the analysis
/// pipeline after it receives the `GuiActivitySummary` produced by this
/// pipeline, not from the GUI pipeline itself.
pub(crate) struct GuiPipelineState {
    pub detector: GuiElementDetector,
    pub aggregator: GuiActivityAggregator,
}

/// Run a single tick of the GUI activity intelligence pipeline.
///
/// Steps:
/// 1. Correlate mouse clicks with OCR regions via `GuiElementDetector`
/// 2. Build `GuiInteractionEvent`s
/// 3. Push events into `GuiActivityAggregator`
/// 4. If aggregator flushes, return the summary
///
/// The caller (monitor loop) feeds the returned summary into
/// `ContentTracker::update()`.
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn run_gui_tick(
    state: &mut GuiPipelineState,
    ocr_regions: &[OcrRegion],
    input_snap: &InputActivityEvent,
    recent_shortcuts: &[String],
    app_name: &str,
    window_title: &str,
    content_label: &str,
    focused_element: Option<&FocusedElementInfo>,
) -> Option<GuiActivitySummary> {
    let now = Utc::now();
    let mut result: Option<GuiActivitySummary> = None;

    // 1. Correlate mouse clicks with OCR regions
    if input_snap.mouse.click_count > 0 {
        // Use the last known mouse position from the input snapshot.
        // InputActivityEvent provides aggregate counts; we generate one
        // representative event per tick when clicks are detected.
        let (click_x, click_y) = input_snap
            .mouse
            .last_position
            .map(|(x, y)| (x as u32, y as u32))
            .unwrap_or((0, 0));

        let element =
            state
                .detector
                .correlate_click_with_app(click_x, click_y, ocr_regions, app_name);

        let gui_element = element.unwrap_or_else(|| {
            // If accessibility provides a focused element label, use it as
            // a better fallback than a completely empty element.
            let (text, element_type) = focused_element
                .and_then(|fe| {
                    fe.label.as_ref().map(|label| {
                        let etype = match fe.role.as_str() {
                            "AXButton" => GuiElementType::Button,
                            "AXTextField" | "AXTextArea" | "edit" => GuiElementType::TextInput,
                            "AXMenuItem" | "AXMenu" => GuiElementType::MenuItem,
                            _ => GuiElementType::Unknown,
                        };
                        (label.clone(), etype)
                    })
                })
                .unwrap_or((String::new(), GuiElementType::Unknown));

            GuiElement {
                text,
                bbox: oneshim_core::models::frame::BoundingBox {
                    x: click_x,
                    y: click_y,
                    width: 1,
                    height: 1,
                },
                element_type,
                confidence: if focused_element.is_some() { 0.6 } else { 0.0 },
            }
        });

        let interaction_event = GuiInteractionEvent {
            timestamp: now,
            element: gui_element,
            interaction_type: GuiInteractionType::Click,
            app_name: app_name.to_string(),
            window_title: Some(window_title.to_string()),
            screen_position: Some((click_x, click_y)),
            interaction: None,
        };

        if let Some(summary) = state.aggregator.push(interaction_event, content_label) {
            result = Some(summary);
        }
    }

    // 2. Handle keyboard shortcuts (if detected in input snapshot)
    //    Iterate over ALL shortcuts that occurred this tick, not just the first.
    if input_snap.keyboard.shortcut_count > 0 {
        for shortcut_keys in recent_shortcuts {
            let shortcut_event = GuiInteractionEvent {
                timestamp: now,
                element: GuiElement {
                    text: String::new(),
                    bbox: oneshim_core::models::frame::BoundingBox {
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 0,
                    },
                    element_type: GuiElementType::Unknown,
                    confidence: 0.0,
                },
                interaction_type: GuiInteractionType::Type,
                app_name: app_name.to_string(),
                window_title: Some(window_title.to_string()),
                screen_position: None,
                interaction: Some(InteractionType::KeyboardShortcut {
                    keys: shortcut_keys.clone(),
                }),
            };

            if let Some(summary) = state.aggregator.push(shortcut_event, content_label) {
                result = Some(summary);
            }
        }
    }

    // 3. Handle text entry — detect remaining keystrokes after subtracting
    //    shortcut keystrokes so text entry is not suppressed when shortcuts
    //    are also present in the same tick.
    let text_keystrokes = input_snap
        .keyboard
        .total_keystrokes
        .saturating_sub(input_snap.keyboard.shortcut_count);
    if text_keystrokes > 0 {
        let text_event = GuiInteractionEvent {
            timestamp: now,
            element: GuiElement {
                text: String::new(),
                bbox: oneshim_core::models::frame::BoundingBox {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                },
                element_type: GuiElementType::TextInput,
                confidence: 0.5,
            },
            interaction_type: GuiInteractionType::Type,
            app_name: app_name.to_string(),
            window_title: Some(window_title.to_string()),
            screen_position: None,
            interaction: Some(InteractionType::TextEntry {
                char_count: text_keystrokes,
                duration_ms: 0,
            }),
        };

        if let Some(summary) = state.aggregator.push(text_event, content_label) {
            result = Some(summary);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use oneshim_core::config::{GuiIntelligenceConfig, PiiFilterLevel};
    use oneshim_core::models::event::{KeyboardActivity, MouseActivity};
    use oneshim_core::models::frame::BoundingBox;

    /// Helper: build a `GuiPipelineState` with 1920x1080 detector and a
    /// short aggregation window for test-friendly flushing.
    fn make_state(window_secs: u64, max_events: usize) -> GuiPipelineState {
        let config = GuiIntelligenceConfig {
            enabled: true,
            aggregation_window_secs: window_secs,
            max_events_per_segment: max_events,
            proximity_threshold_px: 40,
        };
        GuiPipelineState {
            detector: GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off),
            aggregator: GuiActivityAggregator::new(&config),
        }
    }

    /// Helper: build an `InputActivityEvent` with the given click/keyboard params.
    fn make_input(
        click_count: u32,
        last_pos: Option<(f32, f32)>,
        total_keystrokes: u32,
        shortcut_count: u32,
    ) -> InputActivityEvent {
        InputActivityEvent {
            timestamp: Utc::now(),
            period_secs: 3,
            mouse: MouseActivity {
                click_count,
                move_distance: 0.0,
                scroll_count: 0,
                last_position: last_pos,
                double_click_count: 0,
                right_click_count: 0,
            },
            keyboard: KeyboardActivity {
                keystrokes_per_min: 0,
                total_keystrokes,
                typing_bursts: 0,
                shortcut_count,
                correction_count: 0,
            },
            app_name: "VS Code".to_string(),
            keystroke_profile: None,
        }
    }

    fn make_ocr_region(text: &str, x: u32, y: u32, w: u32, h: u32) -> OcrRegion {
        OcrRegion {
            text: text.to_string(),
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
            confidence: 0.9,
        }
    }

    #[test]
    fn click_with_ocr_produces_correlated_event() {
        let mut state = make_state(60, 100);

        // Place an OCR region ("Save" button) near the click position
        let regions = vec![make_ocr_region("Save", 490, 290, 60, 30)];
        let input = make_input(1, Some((500.0, 300.0)), 0, 0);

        // First tick: event goes into aggregator buffer (no flush yet)
        let result = run_gui_tick(
            &mut state,
            &regions,
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        // No flush yet (only 1 event, window not expired)
        assert!(result.is_none());

        // Force flush via content label change
        let input2 = make_input(1, Some((500.0, 300.0)), 0, 0);
        let result = run_gui_tick(
            &mut state,
            &regions,
            &input2,
            &[],
            "VS Code",
            "lib.rs",
            "lib.rs", // different content_label triggers flush
            None,
        );

        let summary = result.expect("content label change should flush");
        assert_eq!(summary.content_label, "main.rs");
        assert!(summary.button_clicks > 0 || summary.save_count > 0);
    }

    #[test]
    fn click_with_empty_ocr_produces_unknown_element() {
        // max_events=1 so the 2nd event triggers a flush of the 1st window
        let mut state = make_state(60, 1);

        let input = make_input(1, Some((500.0, 300.0)), 0, 0);

        // Push first event (fills the 1-event window)
        run_gui_tick(
            &mut state,
            &[],
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        // Push second event — triggers flush due to max_events=1
        let result = run_gui_tick(
            &mut state,
            &[],
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        let summary = result.expect("max_events should trigger flush");
        // No OCR regions → the click lands on an Unknown element
        assert_eq!(summary.unmatched_click_count, 1);
    }

    #[test]
    fn keyboard_only_produces_text_entry() {
        let mut state = make_state(60, 1);

        // No clicks, just keystrokes
        let input = make_input(0, None, 20, 0);

        run_gui_tick(
            &mut state,
            &[],
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        // Second event to flush
        let result = run_gui_tick(
            &mut state,
            &[],
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        let summary = result.expect("should flush via max_events");
        assert!(summary.text_entries > 0);
    }

    #[test]
    fn shortcuts_iterate_all() {
        let mut state = make_state(60, 100);

        let input = make_input(0, None, 3, 3);
        let shortcuts = vec![
            "Cmd+S".to_string(),
            "Cmd+F".to_string(),
            "Cmd+Z".to_string(),
        ];

        // Push events
        run_gui_tick(
            &mut state,
            &[],
            &input,
            &shortcuts,
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        // Flush via content change
        let input2 = make_input(1, Some((100.0, 100.0)), 0, 0);
        let result = run_gui_tick(
            &mut state,
            &[],
            &input2,
            &[],
            "VS Code",
            "lib.rs",
            "lib.rs",
            None,
        );

        let summary = result.expect("content change should flush");
        // All 3 shortcuts were fed as events
        assert_eq!(summary.save_count, 1); // Cmd+S
        assert_eq!(summary.search_count, 1); // Cmd+F
        assert_eq!(summary.undo_redo_count, 1); // Cmd+Z
    }

    #[test]
    fn mixed_clicks_and_typing() {
        let mut state = make_state(60, 100);

        // Click + typing in same tick
        let input = make_input(1, Some((500.0, 300.0)), 15, 0);
        let regions = vec![make_ocr_region("Search", 490, 290, 80, 30)];

        run_gui_tick(
            &mut state,
            &regions,
            &input,
            &[],
            "Chrome",
            "Google",
            "search",
            None,
        );

        // Flush via content change
        let input2 = make_input(1, Some((100.0, 100.0)), 0, 0);
        let result = run_gui_tick(
            &mut state,
            &[],
            &input2,
            &[],
            "Chrome",
            "Results",
            "results",
            None,
        );

        let summary = result.expect("should flush on content change");
        assert!(summary.button_clicks > 0 || summary.search_count > 0);
        assert!(summary.text_entries > 0);
    }

    #[test]
    fn no_input_produces_nothing() {
        let mut state = make_state(60, 100);

        // Zero clicks, zero keystrokes
        let input = make_input(0, None, 0, 0);

        let result = run_gui_tick(
            &mut state,
            &[],
            &input,
            &[],
            "VS Code",
            "main.rs",
            "main.rs",
            None,
        );

        assert!(result.is_none());
        // Even flushing should return None since no events were pushed
        let flushed = state.aggregator.flush();
        assert!(flushed.is_none());
    }
}
