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
#[allow(dead_code)]
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
#[allow(dead_code)]
pub(crate) fn run_gui_tick(
    state: &mut GuiPipelineState,
    ocr_regions: &[OcrRegion],
    input_snap: &InputActivityEvent,
    recent_shortcuts: &[String],
    app_name: &str,
    window_title: &str,
    content_label: &str,
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

        let element = state
            .detector
            .correlate_click(click_x, click_y, ocr_regions);

        let gui_element = element.unwrap_or_else(|| GuiElement {
            text: String::new(),
            bbox: oneshim_core::models::frame::BoundingBox {
                x: click_x,
                y: click_y,
                width: 1,
                height: 1,
            },
            element_type: GuiElementType::Unknown,
            confidence: 0.0,
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
    if input_snap.keyboard.shortcut_count > 0 {
        // Build a synthetic keyboard shortcut event
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
                keys: recent_shortcuts
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string()),
            }),
        };

        if let Some(summary) = state.aggregator.push(shortcut_event, content_label) {
            result = Some(summary);
        }
    }

    // 3. Handle text entry
    if input_snap.keyboard.total_keystrokes > 0 && input_snap.keyboard.shortcut_count == 0 {
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
                char_count: input_snap.keyboard.total_keystrokes,
                duration_ms: 0,
            }),
        };

        if let Some(summary) = state.aggregator.push(text_event, content_label) {
            result = Some(summary);
        }
    }

    result
}
