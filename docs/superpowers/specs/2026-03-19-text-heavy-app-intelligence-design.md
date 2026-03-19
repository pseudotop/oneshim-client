# Text-Heavy App Intelligence — Design Spec

> Created: 2026-03-19
> Status: Draft
> Depends on: GUI Activity Intelligence (Phase 2), Standalone LLM Analysis Pipeline (ADR-011)
> Supersedes: Scattered app lists in AppCategory::from_app_name, TitleBarParser, SENSITIVE_APP_KEYWORDS

## 1. Goal

Upgrade the desktop client's understanding of **text-heavy applications** —
terminals, document editors, chat apps, spreadsheet tools, TUI editors — from
coarse "what app is in focus" to fine-grained "what kind of text work is the
user performing in this app."

The current system conflates all activity inside a terminal as `ActiveCoding`
and all activity inside a document editor as `Writing`. In reality, a terminal
user may be running commands, reading logs, editing files in vim, or tailing a
build output. A Notion user may be actively writing prose or passively reading
a spec. A Slack user may be composing a long message or skimming a channel.

This spec introduces four components that, together, close the gap:

1. **AppRegistry** — a unified, extensible app profile database
2. **InputPatternAnalyzer** — key-category ratio tracking (Enter, Tab, Arrow, Backspace)
3. **AccessibilityExtractor** — OS accessibility API integration for element-level context
4. **WorkType extension** — five new work types for text-heavy activities

The pipeline: AppRegistry identifies the app subcategory, InputPatternAnalyzer
characterizes the keystroke profile, AccessibilityExtractor (when available)
provides element-level context, and WorkTypeClassifier uses all three signals
to select the correct fine-grained WorkType.

## 2. Problem Statement

### 2.1 Current limitations

| Scenario | Current classification | Ideal classification |
|----------|----------------------|---------------------|
| Terminal: user types `cargo test`, hits Enter, waits | `ActiveCoding` | `TerminalCommands` |
| Terminal: tailing a log file, scrolling, no typing | `ActiveCoding` | `LogReading` |
| Terminal: editing in vim with heavy arrow/hjkl usage | `ActiveCoding` | `ActiveCoding` (confirmed) |
| Notion: typing a 2000-word document | `Writing` | `DocumentWriting` |
| Notion: reading a spec, scrolling, no typing | `Writing` | `DocumentReading` |
| Slack: composing a long message with Enter to send | `ActiveMeeting` | `ChatComposing` |
| Excel: entering data into cells with Tab navigation | `Writing` | `FormFilling` (existing) |

### 2.2 Root causes

1. **No app subcategory**: `AppCategory::Development` lumps terminals, IDEs, and
   TUI editors together. `AppCategory::Documentation` lumps Notion, Word, and
   Obsidian together. There is no distinction between a terminal and an IDE.

2. **No keystroke profile**: `KeyboardActivity` tracks total keystrokes, shortcuts,
   and corrections, but not *which keys* are dominant. A terminal command workflow
   has a high Enter-to-keystroke ratio. A spreadsheet workflow has a high
   Tab-to-keystroke ratio. These signals are invisible today.

3. **Scattered app lists**: App detection logic is duplicated in three places:
   - `AppCategory::from_app_name()` in `oneshim-core/src/models/work_session.rs`
   - `TitleBarParser` app detection in `oneshim-analysis/src/title_bar_parser.rs`
   - `SENSITIVE_APP_KEYWORDS` in `oneshim-vision/src/privacy.rs`

   Adding a new app requires updating all three. There is no single source of truth.

4. **No accessibility context**: The system cannot determine which UI element has
   focus (a text editor panel vs. a terminal panel in an IDE, a message input vs.
   a channel list in Slack). OCR + click correlation (GUI Intelligence Phase 2)
   partially addresses this, but accessibility APIs provide higher-fidelity data
   for text-heavy apps where screen content changes rapidly.

### 2.3 Research context

| Approach | Tool/Paper | Relevance |
|----------|-----------|-----------|
| Shell-level hooks | WakaTime heartbeat model | No content capture, shell-level timing only. Validates that command frequency is a useful signal. |
| Window title only | RescueTime, ActivityWatch | Title parsing is necessary but not sufficient for text-heavy apps. Titles do not change during long editing sessions. |
| Keystroke dynamics | Academic IKI/burst analysis (Gunetti & Picardi 2005, Killourhy & Maxion 2009) | Inter-key interval (IKI), burst patterns, and backspace ratio distinguish writing from reading from command entry. We adapt the *ratios* (not raw IKI). |
| macOS accessibility | AXUIElement (Core Accessibility) | `AXUIElementCopyElementAtPosition()` + `AXUIElementCopyAttributeValue()` provide role, title, and value of focused element. Used by VoiceOver, Hammerspoon, yabai. |
| Windows accessibility | UIAutomation ITextRangeProvider | `ElementFromPoint()` + `IUIAutomationElement` provide role, name, bounding rectangle. Used by Narrator, AutoHotkey. |
| Accessibility tree inspection | MacPaw macapptree | Open-source tool for browsing macOS accessibility trees. Useful for validating AXUIElement assumptions during development. |
| Screen understanding | Microsoft OmniParser | Vision-language model for UI parsing. Too heavy for edge deployment (100MB+), but validates the value of element-level understanding. |
| Privacy-preserving approaches | Metadata-only, aggregate input, time-windowed | Our approach: aggregate keystroke *ratios* (not sequences), PII-filtered accessibility text, time-windowed snapshots. No raw keystroke logging. |

## 3. Architecture Overview

```
                    ┌──────────────────────────────┐
                    │         AppRegistry           │
                    │      (oneshim-core)           │
                    │                              │
                    │  Built-in: 50+ app profiles  │
                    │  User override: JSON file     │
                    │                              │
                    │  Output: AppProfile           │
                    │    .category: AppCategory     │
                    │    .subcategory: AppSubcategory│
                    │    .sensitive: bool           │
                    │    .title_hints: Vec<String>  │
                    └──────────┬───────────────────┘
                               │
              ┌────────────────┼─────────────────┐
              │                │                  │
    ┌─────────▼──────┐  ┌─────▼────────┐  ┌─────▼──────────────┐
    │InputPatternAnalyzer│  │ Accessibility │  │ Existing signals   │
    │  (oneshim-monitor) │  │  Extractor   │  │  (keystrokes/min,  │
    │                    │  │(oneshim-vision)│  │   scroll, clicks)  │
    │ 5 new counters:    │  │              │  │                    │
    │  enter_count       │  │ PII-gated    │  │ KeyboardActivity   │
    │  tab_count         │  │ role+position│  │ MouseActivity      │
    │  arrow_count       │  │ label, value │  │                    │
    │  backspace_count   │  │ length       │  │                    │
    │  special_count     │  │              │  │                    │
    │                    │  │ macOS: AX FFI│  │                    │
    │ Output:            │  │ Win: UIA     │  │                    │
    │  KeystrokeProfile  │  │              │  │                    │
    │   .enter_ratio     │  │ Output:      │  │                    │
    │   .tab_ratio       │  │ FocusedElement│  │                    │
    │   .arrow_ratio     │  │   Info       │  │                    │
    │   .backspace_ratio │  │              │  │                    │
    └────────┬───────────┘  └──────┬───────┘  └────────┬───────────┘
             │                     │                    │
             └─────────────┬───────┘────────────────────┘
                           │
              ┌────────────▼──────────────┐
              │   WorkTypeClassifier      │
              │     (oneshim-analysis)    │
              │                          │
              │  Extended rules using:    │
              │  - AppSubcategory         │
              │  - KeystrokeProfile       │
              │  - Accessibility context  │
              │  - Existing engagement    │
              │                          │
              │  5 new WorkType variants: │
              │  TerminalCommands         │
              │  LogReading               │
              │  DocumentWriting          │
              │  DocumentReading          │
              │  ChatComposing            │
              └───────────────────────────┘
```

## 4. Component 1: AppRegistry (`oneshim-core`)

### 4.1 Purpose

A single source of truth for application identification, classification, and
behavioral hints. Replaces the three scattered app lists with one extensible
registry.

### 4.2 AppSubcategory enum

```rust
/// Fine-grained application subcategory within an AppCategory.
///
/// AppCategory remains unchanged (no breaking change). AppSubcategory
/// provides the additional granularity needed for text-heavy app
/// intelligence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSubcategory {
    // Development subcategories
    Terminal,       // iTerm2, Warp, Alacritty, Terminal.app, kitty, Hyper
    Ide,            // VSCode, Cursor, IntelliJ, Xcode, Android Studio
    TuiEditor,      // vim, neovim, emacs (when run inside a terminal)
    ApiTool,        // Postman, Insomnia, Bruno
    GitGui,         // SourceTree, GitKraken, Fork

    // Documentation subcategories
    DocumentEditor, // Notion, Word, Google Docs, Pages, Obsidian, Typora
    Spreadsheet,    // Excel, Numbers, Google Sheets
    Presentation,   // PowerPoint, Keynote, Google Slides

    // Communication subcategories
    Chat,           // Slack, Discord, Teams, KakaoTalk, Telegram, WhatsApp
    Email,          // Mail, Outlook, Gmail, Thunderbird
    VideoCall,      // Zoom, Meet, FaceTime

    // Browser subcategories
    Browser,        // Chrome, Safari, Firefox, Edge, Arc, Brave, Opera

    // Design subcategories
    Design,         // Figma, Sketch, Photoshop, Illustrator, Canva

    // Media subcategories
    Media,          // Spotify, YouTube, Netflix, VLC

    // System subcategories
    System,         // Finder, Explorer, Settings, Activity Monitor

    // Fallback
    Other,
}
```

### 4.3 AppProfile struct

```rust
/// Profile describing an application's characteristics for text intelligence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppProfile {
    /// Display name (e.g., "Visual Studio Code").
    pub name: String,

    /// Case-insensitive patterns to match against the OS-reported app name
    /// or process name. First match wins. Supports substring matching.
    /// Example: ["code", "visual studio code", "cursor"]
    ///
    /// **Ordering**: Built-in profiles MUST be ordered from most specific to
    /// least specific (e.g., "Xcode" before "code") because `AppRegistry::lookup()`
    /// returns the first matching profile. A less-specific pattern listed first
    /// would shadow more-specific ones.
    pub name_patterns: Vec<String>,

    /// Coarse category (existing enum, no breaking change).
    pub category: AppCategory,

    /// Fine-grained subcategory for text intelligence.
    pub subcategory: AppSubcategory,

    /// Hints for TitleBarParser to extract content from this app's titles.
    /// Format: separator string (e.g., " - ", " \u{2013} ", " | ").
    /// Position: "first" (content is before separator) or "last" (after).
    #[serde(default)]
    pub title_hints: Vec<TitleParseHint>,

    /// Preferred accessibility extraction strategy.
    #[serde(default)]
    pub accessibility_strategy: AccessibilityStrategy,

    /// Whether this app handles sensitive data (passwords, banking, etc.).
    /// When true, screen capture and OCR are suppressed.
    #[serde(default)]
    pub sensitive: bool,

    /// Whether this profile is active. User overrides can disable built-in
    /// profiles by setting enabled = false.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitleParseHint {
    /// Separator string in the title bar (e.g., " - ", " | ").
    pub separator: String,
    /// Which segment contains the content: "first", "last", or index.
    pub content_position: String,
    /// Content type to assign when this hint matches.
    pub content_type: ContentType,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityStrategy {
    /// Do not attempt accessibility extraction for this app.
    #[default]
    None,
    /// Use the OS accessibility API (AXUIElement / UIA).
    Native,
    /// Use the existing osascript shim (macOS only).
    Osascript,
}
```

### 4.4 Registry loading strategy

```rust
/// Application profile registry.
///
/// Loading order:
/// 1. Built-in profiles (compiled into the binary, ~50 apps)
/// 2. User override file (~/.oneshim/app_profiles.json) merged on top
///
/// User overrides can:
/// - Add new profiles for apps not in the built-in set
/// - Modify existing profiles (matched by name_patterns overlap)
/// - Disable built-in profiles (set enabled = false)
pub struct AppRegistry {
    profiles: Vec<AppProfile>,
}

impl AppRegistry {
    /// Create a registry with built-in profiles only.
    pub fn new() -> Self { ... }

    /// Load user overrides from JSON file and merge with built-in profiles.
    pub fn load_user_overrides(&mut self, path: &Path) -> Result<(), CoreError> { ... }

    /// Look up the profile for a given app name. Returns the first matching
    /// enabled profile. O(n) scan — profiles are small (~100 entries).
    pub fn lookup(&self, app_name: &str) -> Option<&AppProfile> { ... }

    /// Convenience: get category + subcategory for an app name.
    pub fn classify(&self, app_name: &str) -> (AppCategory, AppSubcategory) { ... }

    /// Check if an app is sensitive (should suppress capture).
    pub fn is_sensitive(&self, app_name: &str) -> bool { ... }
}
```

### 4.5 Built-in profile examples (excerpt)

```json
[
  {
    "name": "iTerm2",
    "name_patterns": ["iterm"],
    "category": "development",
    "subcategory": "terminal",
    "title_hints": [
      { "separator": ": ", "content_position": "last", "content_type": "FILE" }
    ],
    "accessibility_strategy": "native",
    "sensitive": false
  },
  {
    "name": "Visual Studio Code",
    "name_patterns": ["code", "visual studio code"],
    "category": "development",
    "subcategory": "ide",
    "title_hints": [
      { "separator": " - ", "content_position": "first", "content_type": "FILE" }
    ],
    "accessibility_strategy": "native",
    "sensitive": false
  },
  {
    "name": "Notion",
    "name_patterns": ["notion"],
    "category": "documentation",
    "subcategory": "document_editor",
    "title_hints": [
      { "separator": " / ", "content_position": "first", "content_type": "WEB_PAGE" },
      { "separator": " - ", "content_position": "first", "content_type": "WEB_PAGE" }
    ],
    "accessibility_strategy": "native",
    "sensitive": false
  },
  {
    "name": "Slack",
    "name_patterns": ["slack"],
    "category": "communication",
    "subcategory": "chat",
    "title_hints": [
      { "separator": " | ", "content_position": "first", "content_type": "CHANNEL" },
      { "separator": " - ", "content_position": "last", "content_type": "CHANNEL" }
    ],
    "accessibility_strategy": "native",
    "sensitive": false
  },
  {
    "name": "1Password",
    "name_patterns": ["1password"],
    "category": "system",
    "subcategory": "system",
    "title_hints": [],
    "accessibility_strategy": "none",
    "sensitive": true
  }
]
```

The full built-in set covers 50+ apps spanning all subcategories. See the
implementation for the complete list.

### 4.6 Unification of existing app lists

The `AppRegistry` replaces three scattered lists:

| Current location | Current purpose | Migration |
|-----------------|----------------|-----------|
| `AppCategory::from_app_name()` (work_session.rs) | Category classification | Delegate to `AppRegistry::classify()`. Keep `from_app_name()` as a fallback for callers that do not have access to the registry. |
| `TitleBarParser` app detection (title_bar_parser.rs) | Title format hints per app family | Use `AppProfile.title_hints` instead of hardcoded `parse_ide()`, `parse_browser()`, etc. Existing parser methods become fallbacks for apps without a profile. |
| `SENSITIVE_APP_KEYWORDS` (privacy.rs) | Sensitive app suppression | Use `AppProfile.sensitive` flag. Keep `SENSITIVE_APP_KEYWORDS` as a secondary fallback for apps not in the registry. |

**Migration strategy**: The registry is additive. Existing code paths remain
functional as fallbacks. New code paths consult the registry first. Migration
is incremental and non-breaking.

### 4.7 Crate placement

- `AppSubcategory`, `AppProfile`, `TitleParseHint`, `AccessibilityStrategy`: `oneshim-core/src/models/app_registry.rs`
- `AppRegistry` struct + built-in data: `oneshim-core/src/app_registry.rs`
- User override JSON path: `~/.oneshim/app_profiles.json` (managed by `ConfigManager`)

This follows the hexagonal architecture: domain models and the registry are in
the core crate. No adapter-level dependencies.

## 5. Component 2: InputPatternAnalyzer (`oneshim-monitor`)

### 5.1 Purpose

Track key-category ratios within each snapshot period to distinguish between
different text interaction modes (command entry, log reading, document writing,
spreadsheet navigation, chat composing).

### 5.2 New counters on InputActivityCollector

```rust
// Added to InputActivityCollector alongside existing AtomicU32 counters:

/// Enter/Return key presses. High ratio → command entry or chat composing.
enter_count: AtomicU32,

/// Tab key presses. High ratio → spreadsheet navigation or code indentation.
tab_count: AtomicU32,

/// Arrow key presses (up/down/left/right). High ratio → code navigation or
/// TUI editor usage.
arrow_count: AtomicU32,

/// Backspace/Delete key presses. High ratio → active editing with corrections.
backspace_count: AtomicU32,

/// Non-alphanumeric, non-modifier special keys (Escape, Home, End, Page Up/Down,
/// function keys). High ratio → power user navigation.
special_count: AtomicU32,
```

### 5.3 Recording API

```rust
impl InputActivityCollector {
    /// Record a keystroke with key category classification.
    ///
    /// The caller (platform input hook) classifies each key into one of:
    /// - enter (Enter, Return)
    /// - tab (Tab)
    /// - arrow (Up, Down, Left, Right)
    /// - backspace (Backspace, Delete)
    /// - special (Escape, Home, End, PageUp, PageDown, F1-F12)
    /// - regular (all other keys)
    ///
    /// Regular keys increment only total_keystrokes.
    /// Category keys increment both their counter AND total_keystrokes.
    pub fn record_categorized_keystroke(
        &self,
        category: KeyCategory,
        is_shortcut: bool,
        is_correction: bool,
    ) {
        self.total_keystrokes.fetch_add(1, Ordering::Relaxed);

        match category {
            KeyCategory::Enter => { self.enter_count.fetch_add(1, Ordering::Relaxed); }
            KeyCategory::Tab => { self.tab_count.fetch_add(1, Ordering::Relaxed); }
            KeyCategory::Arrow => { self.arrow_count.fetch_add(1, Ordering::Relaxed); }
            KeyCategory::Backspace => {
                self.backspace_count.fetch_add(1, Ordering::Relaxed);
                // Also counts as a correction in the existing system
                self.correction_count.fetch_add(1, Ordering::Relaxed);
            }
            KeyCategory::Special => { self.special_count.fetch_add(1, Ordering::Relaxed); }
            KeyCategory::Regular => { /* only total_keystrokes */ }
        }

        if is_shortcut {
            self.shortcut_count.fetch_add(1, Ordering::Relaxed);
        }
        // is_correction is already handled by Backspace branch above;
        // for non-backspace corrections (e.g., Ctrl+Z), the caller still
        // passes is_correction=true to the existing record_keystroke() path.

        self.record_activity();
    }
}

/// Key category for input pattern analysis.
///
/// Crate placement: `oneshim-core/src/models/` (domain model consumed by
/// analysis and monitor crates). `KeystrokeProfile` is a sub-struct of
/// `InputActivityEvent` in `oneshim-core/src/models/event.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCategory {
    Enter,
    Tab,
    Arrow,
    Backspace,
    Special,
    Regular,
}
```

### 5.4 KeystrokeProfile output

```rust
/// Keystroke profile computed from per-category counters.
///
/// Each ratio is `category_count / total_keystrokes`. When total_keystrokes
/// is 0, all ratios are 0.0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeystrokeProfile {
    pub enter_ratio: f32,
    pub tab_ratio: f32,
    pub arrow_ratio: f32,
    pub backspace_ratio: f32,
    pub special_ratio: f32,
    pub total_keystrokes: u32,
}
```

### 5.5 Snapshot integration

`take_snapshot()` is extended to include the new counters. The `KeystrokeProfile`
is computed and attached to `InputActivityEvent`:

```rust
// In InputActivityEvent (or as a sub-struct):
pub struct InputActivityEvent {
    // ...existing fields...

    /// Keystroke profile with key-category ratios.
    /// Present only when `input_pattern_detail` is enabled in config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keystroke_profile: Option<KeystrokeProfile>,
}
```

All five new counters are atomically swapped to 0 on `take_snapshot()`, identical
to existing counter handling.

### 5.6 Config gating

```rust
// In TextIntelligenceConfig (see Section 8):
/// Enable key-category counters (Enter, Tab, Arrow, Backspace, Special).
/// When false, only total_keystrokes is tracked (existing behavior).
pub input_pattern_detail: bool,  // default: true
```

### 5.7 Consent

Input pattern detail is covered by the existing `activity_pattern_learning`
consent permission (Tier 4). No new consent field is required because:

- No individual key sequences are recorded (only aggregate ratios per period)
- The data granularity is equivalent to existing `shortcut_count` and `correction_count`
- Ratios are computed per snapshot period (typically 5-30 seconds) and then discarded

## 6. Component 3: AccessibilityExtractor (`oneshim-vision`)

### 6.1 Purpose

Extract focused UI element information using OS accessibility APIs to provide
richer context for text-heavy apps. When the user clicks in a terminal panel
inside VSCode, the accessibility API reveals that the focused element has
role "text area" with title "Terminal" — information invisible to OCR alone.

### 6.2 FocusedElementInfo struct

```rust
/// Information about the currently focused UI element, extracted via
/// OS accessibility API. All text fields are PII-filtered before storage.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FocusedElementInfo {
    /// Accessibility role (e.g., "AXTextField", "AXTextArea", "AXButton",
    /// "AXStaticText", "edit", "document").
    pub role: String,

    /// Position and size of the element on screen.
    pub position: Option<ElementRect>,

    /// Accessibility label (e.g., "Search", "Terminal", "Message input").
    /// Filtered by PII level. None at Strict level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Length of the element's text value in characters (not the content itself).
    /// Useful for distinguishing empty fields from filled ones.
    /// Available at Standard and Basic levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_length: Option<u32>,

    /// Extracted text content from the element.
    /// Only available at Basic level (with email/phone masking) or Off level
    /// (full text, requires additional consent).
    /// Uses Zeroizing<String> internally; serialized as plain String after
    /// PII filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ElementRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
```

### 6.3 PII level gating

The `PiiFilterLevel` (existing enum: `Off`, `Basic`, `Standard`, `Strict`)
controls extraction scope:

| PII Level | role | position | label | value_length | extracted_text |
|-----------|------|----------|-------|-------------|----------------|
| **Strict** | Yes | Yes | No | No | No |
| **Standard** | Yes | Yes | Yes | Yes (count, not content) | No |
| **Basic** | Yes | Yes | Yes | Yes | Yes (email/phone masked via `oneshim_vision::privacy::sanitize_title_with_level(text, PiiFilterLevel::Basic)`) |
| **Off** | Yes | Yes | Yes | Yes | Yes (full text, requires `full_text_extraction` consent) |

```rust
impl AccessibilityExtractor {
    /// Extract focused element information at the given PII level.
    ///
    /// SECURITY: When pii_level is Off, the caller MUST verify that
    /// full_text_extraction consent has been granted. If consent is missing,
    /// this method silently falls back to Standard level.
    pub fn extract_focused_element(
        &self,
        pii_level: PiiFilterLevel,
        has_full_text_consent: bool,
    ) -> Option<FocusedElementInfo> {
        let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
            PiiFilterLevel::Standard  // silent fallback
        } else {
            pii_level
        };

        // Platform-specific extraction...
        // NOTE: platform_extract() must handle runtime permission revocation
        // gracefully. On macOS, if Accessibility permission is revoked while the
        // app is running, AXUIElement calls will fail with kAXErrorAPIDisabled.
        // In that case, platform_extract() returns None and logs a warning:
        //   warn!("Accessibility permission revoked at runtime; returning None");
        // This ensures the caller always receives Option<FocusedElementInfo>
        // without panics, regardless of OS permission state changes.
        let raw = self.platform_extract()?;

        // Apply PII level filtering
        self.filter_by_level(raw, effective_level)
    }
}
```

### 6.4 Platform implementation

#### macOS (Phase 2): AXUIElement FFI

```rust
#[cfg(target_os = "macos")]
mod macos_accessibility {
    use core_foundation::base::*;
    use core_foundation::string::*;

    /// Extract the focused UI element using macOS Accessibility API.
    ///
    /// Requires "Accessibility" permission in System Settings > Privacy & Security.
    /// Returns None if permission is not granted or no element has focus.
    ///
    /// API calls:
    /// - AXUIElementCreateSystemWide()
    /// - AXUIElementCopyAttributeValue(kAXFocusedUIElementAttribute)
    /// - AXUIElementCopyAttributeValue(kAXRoleAttribute)
    /// - AXUIElementCopyAttributeValue(kAXTitleAttribute)
    /// - AXUIElementCopyAttributeValue(kAXValueAttribute)
    /// - AXUIElementCopyAttributeValue(kAXPositionAttribute)
    /// - AXUIElementCopyAttributeValue(kAXSizeAttribute)
    pub fn extract_focused() -> Option<RawFocusedElement> { ... }

    /// Check if Accessibility permission is granted.
    /// Uses AXIsProcessTrustedWithOptions().
    pub fn has_accessibility_permission() -> bool { ... }
}
```

#### Windows (Phase 2): UIAutomation

```rust
#[cfg(target_os = "windows")]
mod windows_accessibility {
    /// Extract the focused UI element using Windows UIAutomation.
    ///
    /// API calls:
    /// - CoCreateInstance(CLSID_CUIAutomation)
    /// - IUIAutomation::GetFocusedElement()
    /// - IUIAutomationElement::get_CurrentControlType()
    /// - IUIAutomationElement::get_CurrentName()
    /// - IUIAutomationElement::get_CurrentBoundingRectangle()
    pub fn extract_focused() -> Option<RawFocusedElement> { ... }
}
```

#### Fallback: existing osascript shim (macOS)

For Phase 1 (before native FFI), the existing `osascript` shim in
`oneshim-monitor/src/macos.rs` provides basic window-level information.
This is already implemented and serves as the fallback when native
accessibility is unavailable or permission is denied.

### 6.5 Security: zeroize for raw text

Raw text extracted from the accessibility API may contain sensitive content
(passwords in transit, financial data, private messages). The `zeroize` crate
provides `Zeroizing<String>`, a wrapper that auto-zeros memory on drop.

```rust
use zeroize::Zeroizing;

struct RawFocusedElement {
    role: String,
    title: Option<String>,
    // Raw value text — zeroed on drop before PII filtering captures the result
    value: Option<Zeroizing<String>>,
    position: Option<ElementRect>,
}
```

**Application points:**
- `AccessibilityExtractor` return values before PII filtering
- Any intermediate `String` holding raw accessibility text
- After PII filtering produces the sanitized `FocusedElementInfo`, the raw
  `Zeroizing<String>` is dropped and memory is zeroed

**Dependency**: Add `zeroize = "1"` to `oneshim-vision/Cargo.toml`.

### 6.6 Crate placement

- `FocusedElementInfo`, `ElementRect`: `oneshim-core/src/models/focused_element.rs`
- `AccessibilityExtractor` (trait): `oneshim-core/src/ports/accessibility.rs`
- macOS implementation: `oneshim-vision/src/accessibility/macos.rs`
- Windows implementation: `oneshim-vision/src/accessibility/windows.rs`
- Fallback (osascript): existing `oneshim-monitor/src/macos.rs`

## 7. Component 4: WorkType Extension (`oneshim-analysis`)

### 7.1 New WorkType variants

Five new variants are added to the existing `WorkType` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkType {
    // Existing variants (unchanged)
    ActiveCoding,
    CodeReview,
    Writing,
    Reading,
    Designing,
    FormFilling,
    Browsing,
    PassiveMeeting,
    ActiveMeeting,
    Navigation,
    #[default]
    Unknown,

    // New variants for text-heavy app intelligence
    TerminalCommands,   // Shell command entry: high enter_ratio, moderate keystrokes
    LogReading,         // Log/output watching: near-zero keystrokes, high scroll
    DocumentWriting,    // Active prose writing: high keystrokes, low enter_ratio
    DocumentReading,    // Passive document reading: low keystrokes, moderate scroll
    ChatComposing,      // Chat message composition: moderate keystrokes, high enter_ratio
}
```

### 7.2 Classification rules

Extended rules in `WorkTypeClassifier::infer_work_type()`. These new rules are
evaluated **before** the existing rules, using the app subcategory from
`AppRegistry` and the `KeystrokeProfile` from `InputPatternAnalyzer`.

```
Rule table (new rules, evaluated first when subcategory is available):

Subcategory     | Condition                                        | WorkType
----------------|--------------------------------------------------|------------------
Terminal        | enter_ratio > 0.15 AND keystrokes > 5/min        | TerminalCommands
Terminal        | keystrokes < 5/min AND scroll > 5/min            | LogReading
Terminal        | keystrokes > 40/min AND arrow_ratio > 0.2         | ActiveCoding (TUI)
Terminal        | keystrokes > 40/min                               | ActiveCoding
DocumentEditor  | keystrokes > 40/min AND enter_ratio < 0.05        | DocumentWriting
DocumentEditor  | keystrokes < 5/min AND scroll > 3/min             | DocumentReading
DocumentEditor  | keystrokes > 20/min AND enter_ratio > 0.1         | Writing (list/outline)
Chat            | keystrokes > 20/min AND enter_ratio > 0.1         | ChatComposing
Chat            | keystrokes < 5/min                                | Reading
Spreadsheet     | tab_ratio > 0.15 AND keystrokes > 10/min          | FormFilling
Spreadsheet     | scroll > 5/min AND keystrokes < 5/min             | Reading
TuiEditor       | keystrokes > 40/min                               | ActiveCoding
Ide             | (fall through to existing IDE rules)              | (existing behavior)
```

**Threshold rationale:**

| Threshold | Rationale |
|-----------|-----------|
| `enter_ratio > 0.15` | A shell command every ~7 keystrokes. Typical for `ls`, `cd`, `git status` workflows. |
| `keystrokes < 5/min` | Below 1 key per 12 seconds. User is observing, not typing. |
| `scroll > 5/min` | More than one scroll event per 12 seconds. Active reading. |
| `keystrokes > 40/min` | Sustained typing. Matches existing `moderate_keystrokes` threshold. |
| `arrow_ratio > 0.2` | More than 1 in 5 keys is an arrow. Characteristic of vim/TUI navigation. |
| `tab_ratio > 0.15` | More than 1 in 7 keys is Tab. Characteristic of spreadsheet cell navigation. |
| `enter_ratio < 0.05` | Less than 1 Enter per 20 keys. Continuous prose, not command entry. |

### 7.3 Classifier API extension

```rust
impl WorkTypeClassifier {
    /// Extended classify method that accepts app subcategory and keystroke profile.
    ///
    /// Falls back to the existing classify() behavior when subcategory is None
    /// or keystroke_profile is None.
    pub fn classify_extended(
        &self,
        keyboard: &KeyboardActivity,
        mouse: &MouseActivity,
        content_label: &str,
        app_category: AppCategory,
        app_subcategory: Option<AppSubcategory>,
        keystroke_profile: Option<&KeystrokeProfile>,
    ) -> (WorkType, EngagementMetrics) {
        let engagement = self.compute_engagement(keyboard, mouse);

        // Try subcategory-aware rules first
        if let (Some(subcategory), Some(profile)) = (app_subcategory, keystroke_profile) {
            if let Some(work_type) = self.infer_from_subcategory(
                &engagement, subcategory, profile,
            ) {
                return (work_type, engagement);
            }
        }

        // Fall back to existing rules
        let work_type = self.infer_work_type(&engagement, content_label, app_category);
        (work_type, engagement)
    }
}
```

### 7.4 Interaction with GuiWorkTypeRefiner

The existing `GuiWorkTypeRefiner` (from GUI Activity Intelligence spec)
applies post-hoc corrections using GUI activity summary data. The new
subcategory-aware rules run *before* the GUI refiner:

```
Pipeline:
  1. WorkTypeClassifier::classify_extended()  -- subcategory + keystroke rules
  2. GuiWorkTypeRefiner::refine()             -- GUI interaction corrections
```

This layered approach ensures:
- Text-heavy app rules provide the first-pass classification using input patterns
- GUI interaction data provides corrections when element-level signals contradict
  the input-pattern classification

## 8. Configuration (`oneshim-core`)

### 8.1 TextIntelligenceConfig

```rust
/// Configuration for the Text-Heavy App Intelligence subsystem.
///
/// **Privacy**: accessibility_extraction requires `activity_pattern_learning`
/// consent (GDPR Tier 4). pii_extraction_level: Off requires ADDITIONAL
/// `full_text_extraction` consent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextIntelligenceConfig {
    /// Master switch for text-heavy app intelligence.
    /// When false, the system uses existing coarse classification only.
    #[serde(default)]
    pub enabled: bool,

    /// Enable key-category counters (Enter, Tab, Arrow, Backspace, Special).
    /// When false, only aggregate keystroke counts are tracked.
    #[serde(default = "default_input_pattern_detail")]
    pub input_pattern_detail: bool,

    /// Enable OS accessibility API extraction.
    /// Requires Accessibility permission on macOS, no special permission on Windows.
    /// Requires `activity_pattern_learning` consent.
    #[serde(default)]
    pub accessibility_extraction: bool,

    /// PII filter level for accessibility-extracted text.
    /// Controls how much element content is retained (see Section 6.3).
    #[serde(default = "default_pii_extraction_level")]
    pub pii_extraction_level: PiiFilterLevel,
}

impl Default for TextIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            input_pattern_detail: default_input_pattern_detail(),
            accessibility_extraction: false,
            pii_extraction_level: default_pii_extraction_level(),
        }
    }
}

fn default_input_pattern_detail() -> bool {
    true
}

fn default_pii_extraction_level() -> PiiFilterLevel {
    PiiFilterLevel::Standard
}
```

### 8.2 Placement in AnalysisConfig

```rust
pub struct AnalysisConfig {
    // ...existing fields...
    #[serde(default)]
    pub tiered_memory: TieredMemoryConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub gui_intelligence: GuiIntelligenceConfig,
    #[serde(default)]
    pub text_intelligence: TextIntelligenceConfig,  // NEW
}
```

### 8.3 Consent model

| Config field | Required consent | Rationale |
|-------------|-----------------|-----------|
| `enabled` | None | Master switch, no data collection change |
| `input_pattern_detail` | `activity_pattern_learning` (Tier 4) | Aggregate key ratios, same granularity as existing shortcut tracking |
| `accessibility_extraction` | `activity_pattern_learning` (Tier 4) | Element-level context from OS accessibility API |
| `pii_extraction_level: Off` | `full_text_extraction` (NEW Tier 6 field) | Full raw text from focused elements |

### 8.4 New consent field

```rust
pub struct ConsentPermissions {
    // ...existing Tier 1-5 fields...
    // (Tier 5 is cross_device_sync, defined in crates/oneshim-core/src/consent.rs)

    // --- Tier 6: Text Intelligence ---
    /// Permits extraction of full text content from focused UI elements.
    /// Required only when pii_extraction_level is set to Off.
    /// GDPR Article 6 -- explicit consent for processing text content
    /// that may contain personal data.
    #[serde(default)]
    pub full_text_extraction: bool,
}
```

**Double-gate for Off level**: When `pii_extraction_level` is `Off`, the system
checks BOTH the config setting AND the `full_text_extraction` consent flag. If
consent is missing, the system silently falls back to `Standard` level (see
Section 6.3). This ensures that a configuration change alone cannot enable full
text extraction — the user must also explicitly grant consent.

## 9. Security Hardening

### 9.1 Hardened Runtime (macOS)

The Tauri app must enable Hardened Runtime entitlements for the signed release
build. This prevents debugger attachment and dylib injection that could be used
to intercept accessibility API data in transit.

**Required entitlements** (in `src-tauri/Info.plist` or signing configuration):

| Entitlement | Value | Purpose |
|-------------|-------|---------|
| `com.apple.security.cs.disable-library-validation` | `false` (default) | Prevent unsigned dylib injection |
| `com.apple.security.cs.allow-dyld-environment-variables` | `false` (default) | Block DYLD_INSERT_LIBRARIES attacks |
| `com.apple.security.cs.debugger` | `false` (default) | Prevent debugger attach to running process |
| `com.apple.security.automation.apple-events` | `true` | Required for osascript fallback |

**Note**: These are defaults when Hardened Runtime is enabled. The key action item
is ensuring Hardened Runtime is enabled in the Tauri build configuration and the
code signing workflow, not adding explicit entitlement entries.

### 9.2 zeroize for raw text

All raw text from the accessibility API passes through `Zeroizing<String>` before
PII filtering. See Section 6.5 for details.

**Memory exposure window**: From the moment `AXUIElementCopyAttributeValue` returns
the raw string to the moment `Zeroizing<String>` is dropped after PII filtering.
Typical duration: < 1ms. This is acceptable for the threat model (local process
memory, not network-exposed).

### 9.3 Consent double-gate

The `Off` PII level requires both a config setting and a consent flag. See
Section 8.4 for details. The silent fallback to `Standard` when consent is
missing ensures defense-in-depth.

### 9.4 Audit logging

The following events are logged to the existing audit infrastructure (via
`oneshim-automation/src/audit.rs` or the Tauri audit channel):

| Event | Trigger | Log level |
|-------|---------|-----------|
| `pii_extraction_level` changed | Config file update detected | `info` |
| `accessibility_extraction` toggled | Config file update detected | `info` |
| `full_text_extraction` consent granted | `ConsentManager::grant_consent()` with flag set | `info` |
| `full_text_extraction` consent revoked | `ConsentManager::revoke_consent()` or flag cleared | `warn` |
| Accessibility permission denied by OS | `AXIsProcessTrustedWithOptions()` returns false | `warn` |
| Silent fallback from Off to Standard | Missing consent for Off level | `debug` |

## 10. Data Flow (End-to-End)

```
Scheduler tick (every 5-30 seconds)
│
├─ InputActivityCollector.take_snapshot()
│  ├─ Existing: KeyboardActivity, MouseActivity
│  └─ New: KeystrokeProfile (enter_ratio, tab_ratio, arrow_ratio, ...)
│
├─ ProcessTracker.get_active_window()
│  └─ app_name, window_title
│
├─ AppRegistry.lookup(app_name)
│  └─ AppProfile { category, subcategory, sensitive, ... }
│
├─ [If accessibility_extraction enabled AND consent granted]
│  AccessibilityExtractor.extract_focused_element(pii_level, consent)
│  └─ FocusedElementInfo { role, label, value_length, ... }
│
├─ TitleBarParser.parse(app_name, window_title)
│  └─ ParsedContent { content_label, content_type, confidence }
│
├─ WorkTypeClassifier.classify_extended(
│      keyboard, mouse, content_label, category,
│      subcategory, keystroke_profile
│  )
│  └─ (WorkType, EngagementMetrics)
│
├─ [If GUI Intelligence enabled]
│  GuiWorkTypeRefiner.refine(work_type, gui_summary)
│  └─ Refined WorkType
│
└─ ContentTracker.update(content_activity)
   └─ ContentActivity with enriched work_type
```

## 11. Migration Notes

### 11.1 WorkType enum serde compatibility

New `WorkType` variants use `SCREAMING_SNAKE_CASE` serialization, matching the
existing convention. Deserialization of old data (which lacks the new variants)
continues to work because old data only contains existing variant names.

**Forward compatibility concern**: If older client versions receive data with
new variant names (e.g., from a sync mechanism), they will fail to deserialize.
Mitigation: add `#[serde(other)]` fallback on the `Unknown` variant:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkType {
    ActiveCoding,
    // ...existing...
    TerminalCommands,
    LogReading,
    DocumentWriting,
    DocumentReading,
    ChatComposing,
    #[default]
    #[serde(other)]
    Unknown,
}
```

### 11.2 InputActivityEvent backward compatibility

The new `keystroke_profile` field is `Option<KeystrokeProfile>` with
`#[serde(default, skip_serializing_if = "Option::is_none")]`. Old events
deserialize with `keystroke_profile: None`. No migration needed.

### 11.3 ConsentPermissions backward compatibility

The new `full_text_extraction` field uses `#[serde(default)]` (defaults to
`false`). Existing consent records deserialize correctly with the field missing.
No migration needed.

### 11.4 AppRegistry fallback

`AppCategory::from_app_name()` remains functional for all existing callers.
The `AppRegistry` is an opt-in upgrade. Callers that do not have access to the
registry (e.g., static utility functions) continue to use the existing method.

### 11.5 SQLite schema

No new SQLite tables or columns are required for Phase 1. The `KeystrokeProfile`
is embedded in the existing `InputActivityEvent` JSON blob stored in the
`events` table. The `WorkType` field in `content_activities` (or segment
summaries) already supports string-serialized enum values.

Phase 2 (AccessibilityExtractor) may introduce an `accessibility_snapshots`
table, but that is scoped to the Phase 2 implementation plan.

## 12. Phased Implementation

### Phase 1: AppRegistry + InputPatternAnalyzer + WorkType Rules

**Scope**: Algorithms, data structures, tests, and scheduler integration with
placeholder key-category data. No accessibility API. No OS permissions required.
Real key-category population requires platform hook updates (Phase 1.5).

> **Note on `record_categorized_keystroke()`**: The `KeyCategory` classification
> framework and all associated tests are implemented in this phase using
> simulated / hardcoded key-category data. Wiring `record_categorized_keystroke()`
> into actual platform key event handlers — `CGEventTap` on macOS, Raw Input on
> Windows — is a **Phase 1.5** follow-up. Until Phase 1.5, the counters
> (enter_count, tab_count, etc.) remain at zero in production and all
> subcategory-aware WorkType rules will fall through to existing classification
> behavior, ensuring no regression.

**Deliverables:**
- `AppSubcategory` enum and `AppProfile` struct in `oneshim-core`
- `AppRegistry` with 50+ built-in profiles and JSON override loading
- 5 new `AtomicU32` counters in `InputActivityCollector`
- `KeyCategory` enum and `record_categorized_keystroke()` method (framework only; not yet wired to platform input hooks)
- `KeystrokeProfile` struct and snapshot integration
- 5 new `WorkType` variants: `TerminalCommands`, `LogReading`, `DocumentWriting`, `DocumentReading`, `ChatComposing`
- `classify_extended()` method on `WorkTypeClassifier`
- `TextIntelligenceConfig` section in `AnalysisConfig`
- Unit tests for all new components (using simulated key-category data)
- Scheduler wiring to pass subcategory and keystroke profile through the classification pipeline

**Estimated effort**: 2-3 days

### Phase 1.5: Platform Key-Category Hooks

**Scope**: Wire `record_categorized_keystroke()` into real platform input event
handlers so that key-category counters are populated in production.

**Deliverables:**
- macOS: `CGEventTap` callback classifies key events into `KeyCategory` and calls `record_categorized_keystroke()`
- Windows: Raw Input handler classifies key events into `KeyCategory` and calls `record_categorized_keystroke()`
- Linux: X11/XInput2 handler (best-effort; X11 only)
- Integration tests verifying end-to-end counter population on each platform

**Estimated effort**: 1-2 days

**Verification criteria:**
- `cargo test --workspace` passes
- Terminal app correctly classified as `TerminalCommands` when enter_ratio > 0.15
- Document editor correctly classified as `DocumentWriting` vs `DocumentReading`
- Chat app correctly classified as `ChatComposing` when typing
- Existing classification behavior unchanged when subcategory is `None`

### Phase 2: AccessibilityExtractor (macOS FFI, Windows stub)

**Scope**: OS accessibility API integration for focused element extraction.

**Deliverables:**
- `FocusedElementInfo` and `ElementRect` structs in `oneshim-core`
- `AccessibilityExtractor` port trait in `oneshim-core`
- macOS implementation using AXUIElement FFI (`#[cfg(target_os = "macos")]`)
- Windows stub implementation (`#[cfg(target_os = "windows")]`)
- Linux stub (returns `None` — no standard accessibility API)
- Fallback to existing osascript shim when native extraction fails
- PII level gating (Strict/Standard/Basic scope control)
- `accessibility_extraction` config flag and consent check
- Integration with `classify_extended()` — accessibility context as an additional signal
- Integration tests with mock accessibility data

**Estimated effort**: 3-5 days (macOS FFI is the bulk of the work)

**External dependencies:**
- macOS: `core-foundation` and `accessibility-sys` crates (or raw FFI bindings)
- Windows: `windows-sys` crate (already a dependency)

### Phase 3: PII-Level Gated Text Extraction + Zeroize + Audit

**Scope**: Full text extraction at Basic/Off levels, memory safety hardening,
audit trail.

**Deliverables:**
- `zeroize` dependency added to `oneshim-vision`
- `Zeroizing<String>` wrapper on all raw accessibility text
- `full_text_extraction` consent field in `ConsentPermissions`
- Double-gate enforcement (config + consent) for Off level
- Silent fallback to Standard when consent is missing
- Audit logging for all security-relevant config/consent changes
- Hardened Runtime documentation and verification
- User override JSON file (`~/.oneshim/app_profiles.json`) loading and validation
- Performance benchmarks for accessibility extraction latency

**Estimated effort**: 2-3 days

## 13. Performance Budget

| Operation | Budget | Approach |
|-----------|--------|----------|
| `AppRegistry::lookup()` | < 0.1ms | Linear scan over ~100 profiles, case-insensitive substring match |
| `record_categorized_keystroke()` | < 0.001ms | Single atomic increment (same as existing `record_keystroke()`) |
| `KeystrokeProfile` computation | < 0.01ms | 5 divisions in `take_snapshot()` |
| AccessibilityExtractor (macOS) | < 5ms | Single AXUIElement query, no tree traversal |
| AccessibilityExtractor (Windows) | < 5ms | Single UIAutomation GetFocusedElement |
| `classify_extended()` overhead | < 0.01ms | One additional rule cascade before existing rules |
| `AppRegistry` initialization | < 10ms | JSON parse of ~50 profiles at startup |
| User override JSON loading | < 5ms | JSON parse of user file + merge |

**Memory overhead:**
- `AppRegistry`: ~50KB for 100 profiles (heap-allocated strings)
- 5 new `AtomicU32` in `InputActivityCollector`: 20 bytes
- `KeystrokeProfile` per snapshot: 24 bytes
- `FocusedElementInfo` per extraction: ~200 bytes (with label and role strings)

## 14. Testing Strategy

### Unit tests

| Component | Test cases |
|-----------|-----------|
| `AppRegistry` | Lookup by name, case insensitivity, user override merge, disabled profile skip, sensitive flag, subcategory assignment |
| `AppSubcategory` | Serde roundtrip, all variants covered |
| `KeyCategory` | Correct counter incremented for each category |
| `KeystrokeProfile` | Ratio computation with zero keystrokes, normal ratios, edge cases |
| `InputActivityCollector` | New counters reset on snapshot, ratios correct |
| `WorkTypeClassifier::classify_extended()` | All 11 subcategory rules in the table, fallback to existing behavior |
| `FocusedElementInfo` | PII level filtering at each level |
| `TextIntelligenceConfig` | Serde defaults, backward compatibility with missing field |
| `ConsentPermissions` | Legacy JSON without `full_text_extraction` deserializes correctly |

### Integration tests

| Scenario | Components involved |
|----------|-------------------|
| Terminal command entry | AppRegistry + InputPatternAnalyzer + WorkTypeClassifier |
| Log reading detection | AppRegistry + InputPatternAnalyzer + WorkTypeClassifier |
| Document writing vs reading | AppRegistry + InputPatternAnalyzer + WorkTypeClassifier |
| Chat composing | AppRegistry + InputPatternAnalyzer + WorkTypeClassifier |
| Accessibility extraction gating | Config + Consent + AccessibilityExtractor |
| PII level fallback | Config Off + missing consent = Standard behavior |
| User override loading | AppRegistry + JSON file + merged lookup |

### Property tests

- For any `KeystrokeProfile`, all ratios sum to <= 1.0
- For any `AppProfile` in the built-in set, `classify()` returns matching `(category, subcategory)`
- `classify_extended()` with `subcategory: None` produces identical output to `classify()`

## 15. Open Questions

| Question | Options | Leaning |
|----------|---------|---------|
| Should `AppSubcategory` be stored in `WorkSession`? | Yes (richer session data) / No (computed on the fly) | Yes — store alongside `AppCategory` for historical queries |
| Should `AppRegistry` be behind a port trait? | Yes (testability) / No (it is pure data, no I/O) | No for Phase 1 — it is a pure data lookup. Reconsider if user overrides need async file watching. |
| Should we track inter-key interval (IKI) distributions? | Yes (richer typing dynamics) / No (privacy concern, complexity) | No — ratios are sufficient. IKI would reveal typing patterns that could be used for biometric identification. |
| vim mode detection inside terminals? | Detect escape sequences / rely on arrow_ratio | Rely on arrow_ratio. Escape sequence detection requires key sequence analysis which violates privacy principles. |
| Should `TitleBarParser` be fully replaced by `AppRegistry` hints? | Full replacement / fallback only | Fallback only for Phase 1. Registry hints supplement but do not replace the parser's app-specific logic. |

## 16. Non-Goals

- **Keystroke logging**: We track aggregate category ratios per snapshot period,
  never individual key sequences. This is a hard privacy boundary.
- **Application-specific protocol integration**: We do not hook into Slack's API,
  VS Code's extension API, or shell hooks (like WakaTime). The system is purely
  observational via OS-level signals.
- **Machine learning models**: All classification is rule-based. No model files,
  no training data, no inference latency. The rule table in Section 7.2 is the
  complete classifier.
- **Clipboard monitoring**: Excluded from this spec. Clipboard is a separate
  consent tier (Tier 3) and has different privacy implications.
- **Cross-device profile sync**: App profiles are local. If cross-device sync
  (P3 spec) is implemented, profile sync can be added later as a separate concern.

## 17. Glossary

| Term | Definition |
|------|-----------|
| **AppCategory** | Existing coarse classification (8 variants): Communication, Development, Documentation, Browser, Design, Media, System, Other |
| **AppSubcategory** | New fine-grained classification (16 variants) within AppCategory |
| **AppProfile** | Complete metadata record for one application in the AppRegistry |
| **KeystrokeProfile** | Per-snapshot-period key-category ratios (enter, tab, arrow, backspace, special) |
| **FocusedElementInfo** | Accessibility API output: role, position, label, value_length, extracted_text |
| **PII level gating** | Using `PiiFilterLevel` to control how much accessibility data is retained |
| **Double-gate** | Requiring both config setting AND consent flag for sensitive operations |
| **Zeroize** | Memory-safety pattern: zero out sensitive data on drop before garbage collection |

## 18. Related Documents

| Document | Relevance |
|----------|-----------|
| [GUI Activity Intelligence Design](2026-03-19-gui-activity-intelligence-design.md) | Phase 2 GUI pipeline that this spec integrates with. `GuiWorkTypeRefiner` runs after our `classify_extended()`. |
| [Priority 2 Accuracy Improvements](2026-03-19-priority2-accuracy-improvements-design.md) | Broader accuracy improvement roadmap that includes text-heavy app intelligence as a component. |
| [ADR-001: Rust Client Architecture Patterns](../../architecture/ADR-001-rust-client-architecture-patterns.md) | Async trait, DI, and error handling patterns for new port traits. |
| [ADR-003: Directory Module Pattern](../../architecture/ADR-003-directory-module-pattern.md) | If `AccessibilityExtractor` grows beyond 500 lines, apply directory module pattern. |
| [Standalone LLM Analysis Pipeline](2026-03-18-standalone-llm-analysis-pipeline-design.md) | LLM analysis pipeline that consumes enriched `ContentActivity` with new WorkType variants. |
| [Adaptive Tiered Memory](2026-03-18-adaptive-tiered-memory-design.md) | Tiered memory system that stores segment summaries with WorkType classification. |
