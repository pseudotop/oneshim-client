# Text-Heavy App Intelligence Phase 3: PII-Level Gated Text Extraction

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the ConsentManager into the scheduler monitor loop to replace the hardcoded `full_text_consent = false`, activate the dormant `_last_focused_element` variable to feed accessibility data into the GUI and analysis pipelines, implement terminal command detection and document title extraction using accessibility text, feed extracted text into ContextAssembler for LLM context enrichment, add audit logging for accessibility-related security events, and write end-to-end integration tests for the full extraction-to-analysis pipeline.

**Architecture:** Phase 2 already implemented the core PII gating logic in `MacOsNativeAccessibility::filter_by_level()` and the `AccessibilityExtractor` port trait. Phase 3 USES that foundation -- it does not redesign extraction or filtering. The work is scheduler-level wiring, analysis pipeline enrichment, and observability. ConsentManager (`Arc<ConsentManager>`) is already available in `agent_runtime.rs` and needs to be threaded through `Scheduler` into the monitor loop async block. The `_last_focused_element` variable in `loops.rs` already captures the extraction result each tick; it just needs the underscore removed and its value consumed by the GUI pipeline and analysis pipeline.

**Tech Stack:** Rust, `oneshim-core` (ConsentManager, FocusedElementInfo), `oneshim-vision` (filter_by_level, privacy), `oneshim-analysis` (ContextAssembler, WorkTypeClassifier), `src-tauri` (scheduler, agent_runtime)

**Spec:** `docs/superpowers/specs/2026-03-19-text-heavy-app-intelligence-design.md` (Sections 6.3-6.5, 9, 12 Phase 3)

**Depends on:** Phase 2 (completed -- `MacOsNativeAccessibility`, `AccessibilityExtractor` trait, `FocusedElementInfo`, `filter_by_level()`, `_last_focused_element` in monitor loop, `full_text_extraction` consent field, `zeroize` dependency)

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-analysis/src/terminal_detector.rs` | Terminal command pattern detection from accessibility text |
| `crates/oneshim-analysis/src/document_heading.rs` | Document title/heading extraction from accessibility text |
| `crates/oneshim-app/tests/text_extraction_e2e.rs` | End-to-end integration tests for accessibility-to-analysis pipeline |

### Modified files

| File | Change |
|------|--------|
| `src-tauri/src/scheduler/mod.rs` | Add `consent_manager: Option<Arc<ConsentManager>>` field to `Scheduler`, add `with_consent_manager()` builder |
| `src-tauri/src/scheduler/loops.rs` | (1) Thread `consent_manager` into monitor loop, (2) replace `full_text_consent = false` with real consent check, (3) rename `_last_focused_element` to `last_focused_element`, (4) feed focused element into GUI pipeline and analysis |
| `src-tauri/src/scheduler/analysis_pipeline.rs` | Accept `Option<&FocusedElementInfo>` param in `run_analysis_tick()`, use it for role-based WorkType refinement |
| `src-tauri/src/scheduler/gui_pipeline.rs` | Accept `Option<&FocusedElementInfo>` param in `run_gui_tick()`, use role to improve element type inference |
| `src-tauri/src/agent_runtime.rs` | Pass `consent_manager` to `scheduler.with_consent_manager()` |
| `crates/oneshim-analysis/src/assembler.rs` | Add `accessibility_text: Option<String>` to `CurrentActivity`, include in LLM context JSON |
| `crates/oneshim-analysis/src/lib.rs` | Add `pub mod terminal_detector;` and `pub mod document_heading;` |

---

## Task 1: Wire ConsentManager into Scheduler and monitor loop

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`
- Modify: `src-tauri/src/agent_runtime.rs`

- [ ] **Step 1: Add `consent_manager` field and builder method to `Scheduler`**

In `src-tauri/src/scheduler/mod.rs`, add a new field to the `Scheduler` struct:

```rust
use oneshim_core::consent::ConsentManager;

pub struct Scheduler {
    // ...existing fields...
    /// ConsentManager for runtime consent checks (e.g., full_text_extraction).
    /// Wrapped in Arc for shared access across async blocks.
    pub(super) consent_manager: Option<Arc<ConsentManager>>,
}
```

Add initialization in `new()`:

```rust
consent_manager: None,
```

Add builder method alongside the existing `with_*` methods:

```rust
pub fn with_consent_manager(mut self, consent_manager: Arc<ConsentManager>) -> Self {
    self.consent_manager = Some(consent_manager);
    self
}
```

- [ ] **Step 2: Pass `consent_manager` into the monitor loop async block**

In `src-tauri/src/scheduler/loops.rs`, in `spawn_monitor_loop()`, clone the consent manager alongside the other `Arc` fields:

```rust
let consent_manager1 = self.consent_manager.clone();
```

This goes in the block of `let xxx1 = self.xxx.clone();` lines before `tokio::spawn(async move { ... })`.

- [ ] **Step 3: Replace hardcoded `full_text_consent = false` with real consent check**

In the monitor loop body inside `spawn_monitor_loop()`, replace:

```rust
// full_text_extraction consent is managed by ConsentManager
// which is not available in this async block. Default to
// false so the extractor falls back to Standard PII level
// when pii_extraction_level is Off. This is the safe default.
let full_text_consent = false;
```

With:

```rust
let full_text_consent = consent_manager1
    .as_ref()
    .map(|cm| cm.is_permitted(|p| p.full_text_extraction))
    .unwrap_or(false);
```

The `ConsentManager::is_permitted()` method already exists and checks `current_consent().permissions.full_text_extraction`. No new API needed.

- [ ] **Step 4: Wire consent_manager in agent_runtime.rs**

In `src-tauri/src/agent_runtime.rs`, in the `run()` method, after building the scheduler, add:

```rust
if let Some(ref cm) = self.consent_manager {
    scheduler = scheduler.with_consent_manager(cm.clone());
}
```

This goes alongside the existing optional wiring calls (`with_context_analyzer`, `with_event_tx`, etc.).

**Verify:** `cargo check -p oneshim-tauri-app` (or the src-tauri package name) -- no compile errors. The `full_text_consent` variable now reads from ConsentManager at runtime.

---

## Task 2: Activate focused element variable and feed into pipelines

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`
- Modify: `src-tauri/src/scheduler/analysis_pipeline.rs`
- Modify: `src-tauri/src/scheduler/gui_pipeline.rs`

- [ ] **Step 1: Rename `_last_focused_element` to `last_focused_element`**

In `src-tauri/src/scheduler/loops.rs`, rename the variable declaration:

```rust
// Before:
let mut _last_focused_element: Option<
    oneshim_core::models::focused_element::FocusedElementInfo,
> = None;

// After:
let mut last_focused_element: Option<
    oneshim_core::models::focused_element::FocusedElementInfo,
> = None;
```

Also rename the assignment site:

```rust
// Before:
_last_focused_element = info;
// ...
_last_focused_element = None;

// After:
last_focused_element = info;
// ...
last_focused_element = None;
```

- [ ] **Step 2: Pass focused element into `run_analysis_tick()`**

In `loops.rs`, update the call to `run_analysis_tick()`:

```rust
super::analysis_pipeline::run_analysis_tick(
    ts,
    &app_name,
    &focus_window_title,
    &prev_app,
    app_changed,
    &input_snap,
    last_gui_summary.as_ref(),
    last_focused_element.as_ref(), // NEW: pass focused element
    &storage1,
).await;
```

- [ ] **Step 3: Update `run_analysis_tick()` signature to accept focused element**

In `src-tauri/src/scheduler/analysis_pipeline.rs`, add the parameter:

```rust
use oneshim_core::models::focused_element::FocusedElementInfo;

pub(super) async fn run_analysis_tick(
    ts: &mut AdaptiveTriggerState,
    app_name: &str,
    window_title: &str,
    prev_app: &Option<String>,
    app_changed: bool,
    input_snap: &InputActivityEvent,
    gui_summary: Option<&GuiActivitySummary>,
    focused_element: Option<&FocusedElementInfo>, // NEW
    storage: &Arc<dyn StorageService>,
) {
```

- [ ] **Step 4: Use focused element role to refine WorkType classification**

In `run_analysis_tick()`, after the work type classification block (step 4), add role-based refinement:

```rust
// 4c. Refine work type using accessibility role when focused element has
//     a text-input role (AXTextArea, AXTextField). This helps distinguish
//     e.g., terminal panel vs. editor panel when the app is an IDE.
let work_type = if let Some(ref fe) = focused_element {
    match fe.role.as_str() {
        "AXTextArea" | "AXTextField" | "edit" | "document" => {
            // Element is a text input -- classification stands or can be
            // strengthened. No override needed; the subcategory rules from
            // classify_extended() already handle this.
            work_type
        }
        "AXStaticText" | "text" => {
            // Focused on static text (likely reading). If current type
            // is an active type, consider downgrading.
            match work_type {
                oneshim_core::models::tiered_memory::WorkType::ActiveCoding
                | oneshim_core::models::tiered_memory::WorkType::Writing
                | oneshim_core::models::tiered_memory::WorkType::DocumentWriting => {
                    if engagement.keystrokes_per_min < 5.0 {
                        oneshim_core::models::tiered_memory::WorkType::Reading
                    } else {
                        work_type
                    }
                }
                _ => work_type,
            }
        }
        _ => work_type,
    }
} else {
    work_type
};
```

- [ ] **Step 5: Pass focused element into `run_gui_tick()`**

In `loops.rs`, update the call to `run_gui_tick()`:

```rust
let gui_summary = super::gui_pipeline::run_gui_tick(
    gui_state,
    &last_ocr_regions,
    &input_snap,
    &recent_shortcuts,
    &app_name,
    &focus_window_title,
    &parsed_content_label,
    last_focused_element.as_ref(), // NEW
);
```

- [ ] **Step 6: Update `run_gui_tick()` signature and use focused element**

In `src-tauri/src/scheduler/gui_pipeline.rs`:

```rust
use oneshim_core::models::focused_element::FocusedElementInfo;

pub(crate) fn run_gui_tick(
    state: &mut GuiPipelineState,
    ocr_regions: &[OcrRegion],
    input_snap: &InputActivityEvent,
    recent_shortcuts: &[String],
    app_name: &str,
    window_title: &str,
    content_label: &str,
    focused_element: Option<&FocusedElementInfo>, // NEW
) -> Option<GuiActivitySummary> {
```

When correlating clicks, if the `GuiElementDetector` returns `None` but we have a `focused_element` with a label, use the label as fallback element text:

```rust
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
```

- [ ] **Step 7: Update gui_pipeline tests to pass `None` for focused_element**

Update all existing calls to `run_gui_tick()` in the test module to add `, None` as the last argument so they compile.

**Verify:** `cargo test -p oneshim-tauri-app` (scheduler tests pass), `cargo check --workspace`

---

## Task 3: Terminal command detection

**Files:**
- New: `crates/oneshim-analysis/src/terminal_detector.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Create `terminal_detector.rs`**

```rust
//! Terminal command detection from accessibility-extracted text.
//!
//! When AppSubcategory is Terminal and extracted_text is available
//! (Basic/Off PII level), detects terminal prompt patterns and extracts
//! the current command line. Simple pattern matching, not shell parsing.

/// Result of terminal command detection.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCommandInfo {
    /// The detected command (first word after the prompt).
    /// e.g., "cargo", "git", "docker", "npm"
    pub command: String,
    /// Full command line after prompt (truncated to 120 chars).
    pub command_line: String,
    /// The prompt pattern that was matched.
    pub prompt_char: char,
}

/// Terminal prompt characters to detect.
const PROMPT_CHARS: &[char] = &['$', '%', '#', '>'];

/// Maximum command line length to capture (privacy bound).
const MAX_COMMAND_LINE_LEN: usize = 120;

/// Detect a terminal command from accessibility-extracted text.
///
/// Looks for prompt patterns (`$`, `%`, `#`, `>`) at the start of
/// lines or after whitespace, then extracts the text following the
/// prompt as the command line.
///
/// Returns `None` if no prompt pattern is detected or if the text
/// after the prompt is empty.
pub fn detect_terminal_command(text: &str) -> Option<TerminalCommandInfo> {
    // Process lines in reverse order to find the most recent command
    // (terminal output accumulates upward).
    for line in text.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        for &prompt in PROMPT_CHARS {
            // Match patterns: "$ cmd", "% cmd", "# cmd", "> cmd"
            // Also match: "user@host:~$ cmd", "PS1> cmd"
            if let Some(pos) = trimmed.rfind(prompt) {
                let after = &trimmed[pos + prompt.len_utf8()..].trim_start();
                if after.is_empty() {
                    continue;
                }

                // Extract the command (first whitespace-delimited token)
                let command = after
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string();

                if command.is_empty() {
                    continue;
                }

                // Truncate full command line for privacy
                let command_line = if after.len() > MAX_COMMAND_LINE_LEN {
                    format!("{}...", &after[..MAX_COMMAND_LINE_LEN])
                } else {
                    after.to_string()
                };

                return Some(TerminalCommandInfo {
                    command,
                    command_line,
                    prompt_char: prompt,
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_simple_dollar_prompt() {
        let text = "$ cargo test --workspace";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "cargo");
        assert_eq!(result.command_line, "cargo test --workspace");
        assert_eq!(result.prompt_char, '$');
    }

    #[test]
    fn detect_percent_prompt() {
        let text = "% git status";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "git");
        assert_eq!(result.prompt_char, '%');
    }

    #[test]
    fn detect_hash_prompt_root() {
        let text = "# apt-get update";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "apt-get");
        assert_eq!(result.prompt_char, '#');
    }

    #[test]
    fn detect_chevron_prompt() {
        let text = "> docker compose up";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "docker");
        assert_eq!(result.prompt_char, '>');
    }

    #[test]
    fn detect_user_host_prompt() {
        let text = "user@host:~/projects$ npm install";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "npm");
        assert_eq!(result.prompt_char, '$');
    }

    #[test]
    fn multiline_picks_last_command() {
        let text = "output line 1\noutput line 2\n$ ls -la\n";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "ls");
    }

    #[test]
    fn empty_prompt_returns_none() {
        let text = "$ ";
        assert!(detect_terminal_command(text).is_none());
    }

    #[test]
    fn no_prompt_returns_none() {
        let text = "just some output text without a prompt";
        assert!(detect_terminal_command(text).is_none());
    }

    #[test]
    fn long_command_truncated() {
        let long_cmd = format!("$ {}", "x".repeat(200));
        let result = detect_terminal_command(&long_cmd).unwrap();
        assert!(result.command_line.len() <= MAX_COMMAND_LINE_LEN + 3); // +3 for "..."
        assert!(result.command_line.ends_with("..."));
    }

    #[test]
    fn blank_lines_skipped() {
        let text = "\n\n\n$ cargo build\n\n";
        let result = detect_terminal_command(text).unwrap();
        assert_eq!(result.command, "cargo");
    }
}
```

- [ ] **Step 2: Add module declaration in `lib.rs`**

In `crates/oneshim-analysis/src/lib.rs`, add:

```rust
pub mod terminal_detector;
```

**Verify:** `cargo test -p oneshim-analysis -- terminal_detector`

---

## Task 4: Document title/heading extraction

**Files:**
- New: `crates/oneshim-analysis/src/document_heading.rs`
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] **Step 1: Create `document_heading.rs`**

```rust
//! Document title and heading extraction from accessibility text.
//!
//! When AppSubcategory is DocumentEditor and extracted_text is available,
//! attempts to extract a document title or current heading for richer
//! content labels in the analysis pipeline.

/// Result of document heading extraction.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentHeadingInfo {
    /// Extracted heading text (trimmed, max 100 chars).
    pub heading: String,
    /// Heading level if detectable (1 = title, 2 = H2, etc.). 0 = unknown.
    pub level: u8,
}

/// Maximum heading length to capture.
const MAX_HEADING_LEN: usize = 100;

/// Extract a document heading from accessibility-extracted text.
///
/// Detection heuristics (ordered by priority):
/// 1. Markdown headings: lines starting with `#`, `##`, etc.
/// 2. First non-empty line if it is short enough to be a title (< 80 chars)
///    and the second line is empty or a separator
///
/// Returns `None` if no heading pattern is detected.
pub fn extract_document_heading(text: &str) -> Option<DocumentHeadingInfo> {
    let lines: Vec<&str> = text.lines().collect();

    // Strategy 1: Markdown headings
    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count() as u8;
            let heading = trimmed[level as usize..].trim().trim_matches('#').trim();
            if !heading.is_empty() {
                return Some(DocumentHeadingInfo {
                    heading: truncate(heading, MAX_HEADING_LEN),
                    level,
                });
            }
        }
    }

    // Strategy 2: First short line followed by empty line or separator
    if let Some(first) = lines.first().map(|l| l.trim()) {
        if !first.is_empty() && first.len() < 80 {
            let second = lines.get(1).map(|l| l.trim()).unwrap_or("");
            let is_title_like = second.is_empty()
                || second.chars().all(|c| c == '=' || c == '-' || c == '_');
            if is_title_like {
                return Some(DocumentHeadingInfo {
                    heading: truncate(first, MAX_HEADING_LEN),
                    level: 0,
                });
            }
        }
    }

    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_h1() {
        let text = "# Project Overview\n\nSome content here.";
        let result = extract_document_heading(text).unwrap();
        assert_eq!(result.heading, "Project Overview");
        assert_eq!(result.level, 1);
    }

    #[test]
    fn markdown_h2() {
        let text = "## Architecture\nThe system uses...";
        let result = extract_document_heading(text).unwrap();
        assert_eq!(result.heading, "Architecture");
        assert_eq!(result.level, 2);
    }

    #[test]
    fn title_with_separator() {
        let text = "Meeting Notes\n=============\nAttendees: ...";
        let result = extract_document_heading(text).unwrap();
        assert_eq!(result.heading, "Meeting Notes");
        assert_eq!(result.level, 0);
    }

    #[test]
    fn title_with_empty_second_line() {
        let text = "Budget Report Q4\n\nTotal spending...";
        let result = extract_document_heading(text).unwrap();
        assert_eq!(result.heading, "Budget Report Q4");
        assert_eq!(result.level, 0);
    }

    #[test]
    fn no_heading_in_prose() {
        let text = "This is a long paragraph that continues for a while and does not look like a heading at all because it is too long to be one.";
        assert!(extract_document_heading(text).is_none());
    }

    #[test]
    fn empty_text() {
        assert!(extract_document_heading("").is_none());
    }

    #[test]
    fn heading_with_trailing_hashes() {
        let text = "## Design Spec ##\n\nContent...";
        let result = extract_document_heading(text).unwrap();
        assert_eq!(result.heading, "Design Spec");
        assert_eq!(result.level, 2);
    }
}
```

- [ ] **Step 2: Add module declaration in `lib.rs`**

In `crates/oneshim-analysis/src/lib.rs`, add:

```rust
pub mod document_heading;
```

**Verify:** `cargo test -p oneshim-analysis -- document_heading`

---

## Task 5: Feed extracted text into ContextAssembler

**Files:**
- Modify: `crates/oneshim-analysis/src/assembler.rs`
- Modify: `src-tauri/src/scheduler/analysis_pipeline.rs`

- [ ] **Step 1: Add `accessibility_text` field to `CurrentActivity`**

In `crates/oneshim-analysis/src/assembler.rs`, add to `CurrentActivity`:

```rust
pub struct CurrentActivity {
    pub app_name: String,
    pub window_title: String,
    pub ocr_hint: Option<String>,
    pub focus_score: f32,
    pub deep_work_mins: u32,
    /// Accessibility-extracted text from the focused element.
    /// Only present at Basic/Off PII levels. PII-filtered before
    /// reaching this struct (filtered by AccessibilityExtractor).
    pub accessibility_text: Option<String>,
}
```

Update the `Default` impl:

```rust
impl Default for CurrentActivity {
    fn default() -> Self {
        Self {
            app_name: String::new(),
            window_title: String::new(),
            ocr_hint: None,
            focus_score: 0.0,
            deep_work_mins: 0,
            accessibility_text: None,
        }
    }
}
```

- [ ] **Step 2: Include accessibility_text in the LLM context JSON**

In `CurrentSnapshot`:

```rust
#[derive(Serialize)]
struct CurrentSnapshot {
    app: String,
    window: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ocr_hint: Option<String>,
    focus_score: f32,
    deep_work_mins: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    accessibility_text: Option<String>,
}
```

In `build_with_history()`, where `CurrentSnapshot` is constructed:

```rust
current: CurrentSnapshot {
    app: current.app_name.clone(),
    window: self.filter_pii(&current.window_title),
    ocr_hint: current.ocr_hint.as_ref().map(|t| self.filter_pii(t)),
    focus_score: current.focus_score,
    deep_work_mins: current.deep_work_mins,
    accessibility_text: current.accessibility_text.as_ref().map(|t| self.filter_pii(t)),
},
```

- [ ] **Step 3: Populate accessibility_text in the analysis pipeline**

In `src-tauri/src/scheduler/analysis_pipeline.rs`, where `CurrentActivity` is constructed (or passed through), set the `accessibility_text` from the focused element. Find the site where the ContextAnalyzer is fed activity data, and include:

```rust
// When building CurrentActivity for the ContextAssembler:
let accessibility_text = focused_element
    .and_then(|fe| fe.extracted_text.clone());
```

This respects PII gating because `FocusedElementInfo.extracted_text` is already filtered by `filter_by_level()` in Phase 2. It is `None` at Strict/Standard levels and sanitized at Basic level.

- [ ] **Step 4: Add test for accessibility_text in JSON output**

Append to the tests in `assembler.rs`:

```rust
#[test]
fn build_with_accessibility_text_included() {
    let assembler = ContextAssembler::new(noop_filter());
    let current = CurrentActivity {
        app_name: "Terminal".to_string(),
        window_title: "iTerm2".to_string(),
        ocr_hint: None,
        focus_score: 0.6,
        deep_work_mins: 10,
        accessibility_text: Some("$ cargo test --workspace".to_string()),
    };
    let ctx = assembler.build(&current, &[], &[], &make_metrics());
    let parsed: serde_json::Value = serde_json::from_str(&ctx.user_context_json).unwrap();
    assert_eq!(
        parsed["current"]["accessibility_text"],
        "$ cargo test --workspace"
    );
}

#[test]
fn build_without_accessibility_text_omits_key() {
    let assembler = ContextAssembler::new(noop_filter());
    let ctx = assembler.build(&make_current(), &[], &[], &make_metrics());
    let parsed: serde_json::Value = serde_json::from_str(&ctx.user_context_json).unwrap();
    assert!(parsed["current"].get("accessibility_text").is_none());
}

#[test]
fn accessibility_text_pii_filtered() {
    let assembler = ContextAssembler::new(test_pii_filter());
    let current = CurrentActivity {
        app_name: "Terminal".to_string(),
        window_title: "iTerm2".to_string(),
        ocr_hint: None,
        focus_score: 0.6,
        deep_work_mins: 10,
        accessibility_text: Some("ssh user@example.com".to_string()),
    };
    let ctx = assembler.build(&current, &[], &[], &make_metrics());
    assert!(ctx.user_context_json.contains("[EMAIL]"));
    assert!(!ctx.user_context_json.contains("user@example.com"));
}
```

**Verify:** `cargo test -p oneshim-analysis -- assembler`

---

## Task 6: Integrate terminal detection and document heading into analysis pipeline

**Files:**
- Modify: `src-tauri/src/scheduler/analysis_pipeline.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 1: Use terminal detection in the analysis pipeline**

In `analysis_pipeline.rs`, after work type classification, when the focused element has `extracted_text` and the app subcategory is `Terminal`:

```rust
use oneshim_analysis::terminal_detector;
use oneshim_core::models::app_registry::AppSubcategory;

// 4d. Enrich terminal commands with accessibility text
let terminal_command = focused_element
    .and_then(|fe| fe.extracted_text.as_deref())
    .and_then(|text| {
        // Only detect terminal commands when the app is a terminal
        let parsed_profile = ts.title_bar_parser.lookup_profile(app_name);
        let is_terminal = parsed_profile
            .map(|p| p.subcategory == AppSubcategory::Terminal)
            .unwrap_or(false);
        if is_terminal {
            terminal_detector::detect_terminal_command(text)
        } else {
            None
        }
    });

if let Some(ref cmd_info) = terminal_command {
    debug!(
        command = %cmd_info.command,
        "Terminal command detected from accessibility text"
    );
}
```

The `terminal_command` info can be used to enrich content labels in `ContentTracker` -- the command name becomes part of the content activity metadata.

- [ ] **Step 2: Use document heading extraction**

Similarly, for document editors:

```rust
use oneshim_analysis::document_heading;

// 4e. Extract document heading from accessibility text for document editors
let doc_heading = focused_element
    .and_then(|fe| fe.extracted_text.as_deref())
    .and_then(|text| {
        let parsed_profile = ts.title_bar_parser.lookup_profile(app_name);
        let is_doc_editor = parsed_profile
            .map(|p| p.subcategory == AppSubcategory::DocumentEditor)
            .unwrap_or(false);
        if is_doc_editor {
            document_heading::extract_document_heading(text)
        } else {
            None
        }
    });

if let Some(ref heading) = doc_heading {
    debug!(
        heading = %heading.heading,
        level = heading.level,
        "Document heading detected from accessibility text"
    );
}
```

Note: If `TitleBarParser` does not expose a `lookup_profile()` method, fall back to checking `AppCategory::from_app_name()` combined with known terminal/document app names. The exact integration point depends on what the `TitleBarParser` API provides. If `AppRegistry` is available in scope, use `app_registry.lookup(app_name)` instead.

**Verify:** `cargo check --workspace`

---

## Task 7: Audit logging for security-relevant events

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs`

- [ ] **Step 1: Add audit log for PII level fallback**

The fallback from Off to Standard already logs at `debug` level inside `MacOsNativeAccessibility::extract_focused_element()` (line 279 of `macos.rs`):

```rust
debug!("PII Off requested but full_text_extraction consent missing; falling back to Standard");
```

This satisfies spec Section 9.6 row "Silent fallback from Off to Standard". No additional work needed -- verify the log line exists.

- [ ] **Step 2: Add audit log for consent state changes**

In the monitor loop, after the consent check, add a one-time log when the consent state changes:

```rust
// Track previous consent state for audit logging
static PREV_CONSENT_STATE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
```

Better approach: use a local variable in the async block:

```rust
// Inside the monitor loop async block, before the loop:
let mut prev_full_text_consent = false;
```

Inside the tick, after computing `full_text_consent`:

```rust
if full_text_consent != prev_full_text_consent {
    if full_text_consent {
        info!(
            event = "full_text_extraction_consent_granted",
            "User granted full_text_extraction consent — Off PII level now effective"
        );
    } else {
        warn!(
            event = "full_text_extraction_consent_revoked",
            "User revoked full_text_extraction consent — falling back to Standard PII level"
        );
    }
    prev_full_text_consent = full_text_consent;
}
```

- [ ] **Step 3: Add audit log for accessibility permission denial**

This is already handled in `MacOsNativeAccessibility::extract_raw()` (line 108 of `macos.rs`):

```rust
if err == kAXErrorAPIDisabled {
    warn!("Accessibility permission revoked at runtime; returning None");
}
```

This satisfies spec Section 9.6 row "Accessibility permission denied by OS". No additional work needed -- verify the log line exists.

- [ ] **Step 4: Add audit log for PII extraction level config changes**

In the monitor loop, track the previous PII extraction level and log changes:

```rust
// Inside the monitor loop async block, before the loop:
let mut prev_pii_level = config_manager1
    .as_ref()
    .map(|cm| cm.get().analysis.text_intelligence.pii_extraction_level)
    .unwrap_or_default();
```

Inside the tick, after reading `text_config`:

```rust
if text_config.pii_extraction_level != prev_pii_level {
    info!(
        event = "pii_extraction_level_changed",
        old = ?prev_pii_level,
        new = ?text_config.pii_extraction_level,
        "PII extraction level changed"
    );
    prev_pii_level = text_config.pii_extraction_level;
}
```

**Verify:** `cargo check --workspace`, then manually test by changing config values and observing log output.

---

## Task 8: End-to-end integration tests

**Files:**
- New: `crates/oneshim-app/tests/text_extraction_e2e.rs` (or `src-tauri/tests/` if that is the integration test location)

- [ ] **Step 1: Create integration test file**

```rust
//! End-to-end integration tests for the text extraction pipeline.
//!
//! Tests the flow: mock accessibility -> PII filter -> work type refinement
//! -> context assembly. Does not require OS accessibility permissions.

use oneshim_core::config::PiiFilterLevel;
use oneshim_core::models::focused_element::FocusedElementInfo;

#[test]
fn terminal_command_detected_from_basic_pii_text() {
    // Simulate Basic-level extraction: email masked, command preserved
    let element = FocusedElementInfo {
        role: "AXTextArea".to_string(),
        label: Some("Terminal".to_string()),
        value_length: Some(50),
        extracted_text: Some("user@host:~$ cargo test --workspace".to_string()),
        position: None,
    };

    let cmd = oneshim_analysis::terminal_detector::detect_terminal_command(
        element.extracted_text.as_deref().unwrap(),
    );
    assert!(cmd.is_some());
    let cmd = cmd.unwrap();
    assert_eq!(cmd.command, "cargo");
    assert_eq!(cmd.command_line, "cargo test --workspace");
}

#[test]
fn document_heading_detected_from_editor_text() {
    let element = FocusedElementInfo {
        role: "AXTextArea".to_string(),
        label: Some("Editor".to_string()),
        value_length: Some(200),
        extracted_text: Some("# Sprint Planning\n\nGoals for this week...".to_string()),
        position: None,
    };

    let heading = oneshim_analysis::document_heading::extract_document_heading(
        element.extracted_text.as_deref().unwrap(),
    );
    assert!(heading.is_some());
    let heading = heading.unwrap();
    assert_eq!(heading.heading, "Sprint Planning");
    assert_eq!(heading.level, 1);
}

#[test]
fn pii_basic_level_masks_email_preserves_command() {
    use oneshim_vision::privacy::sanitize_title_with_level;

    let raw_text = "user@example.com:~/projects$ git push origin main";
    let filtered = sanitize_title_with_level(raw_text, PiiFilterLevel::Basic);

    // Email should be masked
    assert!(filtered.contains("[EMAIL]"));
    assert!(!filtered.contains("user@example.com"));

    // But the command should still be detectable
    let cmd = oneshim_analysis::terminal_detector::detect_terminal_command(&filtered);
    assert!(cmd.is_some());
    assert_eq!(cmd.unwrap().command, "git");
}

#[test]
fn pii_strict_level_no_text_no_detection() {
    // At Strict level, extracted_text is None
    let element = FocusedElementInfo {
        role: "AXTextArea".to_string(),
        label: None,
        value_length: None,
        extracted_text: None,
        position: None,
    };

    // No text available for detection
    assert!(element.extracted_text.is_none());
}

#[test]
fn pii_standard_level_no_text_but_has_metadata() {
    // At Standard level, we get role + label + value_length but no text
    let element = FocusedElementInfo {
        role: "AXTextArea".to_string(),
        label: Some("Terminal".to_string()),
        value_length: Some(50),
        extracted_text: None,
        position: None,
    };

    assert!(element.label.is_some());
    assert!(element.value_length.is_some());
    assert!(element.extracted_text.is_none());
}

#[test]
fn context_assembler_includes_accessibility_text() {
    use oneshim_analysis::assembler::{ContextAssembler, CurrentActivity, SessionMetrics};

    let assembler = ContextAssembler::new(Box::new(|t: &str| t.to_string()));
    let current = CurrentActivity {
        app_name: "iTerm2".to_string(),
        window_title: "zsh".to_string(),
        ocr_hint: None,
        focus_score: 0.7,
        deep_work_mins: 15,
        accessibility_text: Some("$ cargo clippy --workspace".to_string()),
    };
    let metrics = SessionMetrics {
        total_work_mins: 60,
        context_switches: 5,
        communication_ratio: 0.1,
    };

    let ctx = assembler.build(&current, &[], &[], &metrics);
    let parsed: serde_json::Value =
        serde_json::from_str(&ctx.user_context_json).unwrap();
    assert_eq!(
        parsed["current"]["accessibility_text"],
        "$ cargo clippy --workspace"
    );
}

#[test]
fn full_pipeline_terminal_basic_level() {
    use oneshim_vision::privacy::sanitize_title_with_level;

    // Simulate the full pipeline:
    // 1. Raw text from accessibility API
    let raw = "admin@prod-server:~$ kubectl get pods -n staging";

    // 2. PII filter at Basic level
    let filtered = sanitize_title_with_level(raw, PiiFilterLevel::Basic);

    // 3. Terminal command detection
    let cmd = oneshim_analysis::terminal_detector::detect_terminal_command(&filtered);
    assert!(cmd.is_some());
    let cmd = cmd.unwrap();
    assert_eq!(cmd.command, "kubectl");

    // 4. This would feed into ContextAssembler.accessibility_text
    assert!(!filtered.is_empty());
}
```

- [ ] **Step 2: Ensure integration test compiles and passes**

Add necessary dependencies to the test crate's `Cargo.toml` if not already present (`oneshim-analysis`, `oneshim-vision`, `oneshim-core`, `serde_json`).

**Verify:** `cargo test --test text_extraction_e2e` (or wherever the test lives)

---

## Task 9: Final verification

- [ ] **Step 1: Full workspace build and test**

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace
cargo fmt --check
```

- [ ] **Step 2: Verify consent wiring manually**

1. Run the app with `full_text_extraction: false` in consent -- verify `full_text_consent` is `false` in logs
2. Grant `full_text_extraction` consent -- verify the `info` audit log fires
3. Set `pii_extraction_level: off` without consent -- verify silent fallback `debug` log
4. Set `pii_extraction_level: off` with consent -- verify full text appears in accessibility extraction

- [ ] **Step 3: Verify no regression in existing tests**

All existing scheduler tests, gui_pipeline tests, analysis_pipeline tests, and assembler tests must pass unchanged (except for the added `None` parameter in gui_pipeline test calls).

---

## Sequencing & Dependencies

```
Task 1 (ConsentManager wiring)
  |
  v
Task 2 (Activate focused element, wire into pipelines)  <-- depends on Task 1
  |
  +---> Task 3 (Terminal detection)      -- independent, can parallel with Task 4
  +---> Task 4 (Document heading)        -- independent, can parallel with Task 3
  |
  v
Task 5 (Feed into ContextAssembler)      <-- depends on Task 2
  |
  v
Task 6 (Integration into analysis)       <-- depends on Tasks 3, 4, 5
  |
  v
Task 7 (Audit logging)                   <-- depends on Task 1
  |
  v
Task 8 (Integration tests)              <-- depends on all above
  |
  v
Task 9 (Final verification)
```

**Parallel opportunities:** Tasks 3 and 4 are fully independent of each other and can be implemented simultaneously. Task 7 can also be worked in parallel with Tasks 3-6 since it only touches loops.rs logging.

---

## Estimated Effort

| Task | Effort |
|------|--------|
| Task 1: ConsentManager wiring | 30 min |
| Task 2: Activate focused element | 1 hour |
| Task 3: Terminal detection | 30 min |
| Task 4: Document heading | 30 min |
| Task 5: ContextAssembler integration | 30 min |
| Task 6: Analysis pipeline integration | 45 min |
| Task 7: Audit logging | 30 min |
| Task 8: Integration tests | 45 min |
| Task 9: Final verification | 30 min |
| **Total** | **~5.5 hours** |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| `ConsentManager` is not `Send + Sync` for async block | It is already `Arc<ConsentManager>` in `agent_runtime.rs`. `ConsentManager` does not contain non-Send types. If needed, read consent state before entering the async block and pass a bool. |
| `TitleBarParser` does not expose `lookup_profile()` | Fall back to `AppCategory::from_app_name()` combined with hardcoded terminal/doc app name checks. File an issue to add `lookup_profile()` in a follow-up. |
| Terminal prompt detection false positives (e.g., `>` in email quotes) | The detection only fires when `AppSubcategory::Terminal` is confirmed. Markdown quote `>` in document editors will not trigger terminal detection because the subcategory gate prevents it. |
| `accessibility_text` field breaks existing `CurrentActivity` callers | The field is `Option<String>` with `Default` providing `None`. Existing constructors using `..Default::default()` or explicit field lists will compile. The `make_current()` test helper needs `accessibility_text: None` added. |
