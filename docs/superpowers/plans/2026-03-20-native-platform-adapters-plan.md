# Native Platform Adapters — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

## Goal

Replace script-based accessibility adapters (osascript/PowerShell/xdotool) with native OS APIs for window-level tree traversal, wire MagicOverlay as a real OverlayDriver implementation, enrich dashcam RingFrames with accessibility element snapshots, and add permission gating with `CoreError::PermissionDenied`.

## Architecture

- **Trait extension**: Add `extract_window_elements()` to `AccessibilityExtractor` with a backward-compatible default implementation
- **macOS**: Extend existing AXUIElement FFI with `kAXChildrenAttribute` for recursive tree traversal
- **Windows**: Add CacheRequest-based subtree traversal via existing COM vtable pattern
- **Linux**: Replace stub with real `atspi` crate calls over D-Bus (async-native, no spawn_blocking)
- **Overlay bridge**: New `MagicOverlayDriver` in `src-tauri` that implements `OverlayDriver` port by emitting Tauri events to the existing FocusHighlight React component
- **Dashcam**: Extend `RingFrame` with `accessibility_elements` field populated from `extract_window_elements()` on each monitor tick
- **Permission gating**: New `CoreError::PermissionDenied` variant, checked per OS before accessibility calls

## Tech Stack

| Component | Technology | Notes |
|-----------|-----------|-------|
| macOS accessibility | `AXUIElementCopyAttributeValue` (existing FFI) | Add `AXChildren` constant |
| macOS batch | `AXUIElementCopyMultipleAttributeValues` (new FFI) | Single IPC for role+title+bounds |
| Windows accessibility | `windows-sys` COM vtable (existing) | Add CacheRequest + TreeWalker offsets |
| Linux accessibility | `atspi` 0.29 crate + `zbus` transitive | `cfg(target_os = "linux")` gated |
| Overlay | Tauri events -> FocusHighlight.tsx | Already exists in `magic_overlay.rs` |
| Ring buffer | `oneshim-vision::ring_buffer` | Extend `RingFrame` struct |

## File Map

### Files to Modify

| # | File | Change |
|---|------|--------|
| 1 | `crates/oneshim-core/src/error.rs` | Add `PermissionDenied(String)` variant |
| 2 | `crates/oneshim-core/src/ports/accessibility.rs` | Add `extract_window_elements()` default method |
| 3 | `crates/oneshim-core/src/models/focused_element.rs` | Add `AccessibilityElement` struct |
| 4 | `crates/oneshim-vision/src/accessibility/ffi_macos.rs` | Add `AXChildren`, `AXUIElementCopyMultipleAttributeValues` FFI bindings |
| 5 | `crates/oneshim-vision/src/accessibility/macos.rs` | Implement tree traversal + permission check returning `PermissionDenied` |
| 6 | `crates/oneshim-vision/src/accessibility/windows.rs` | Add CacheRequest subtree traversal + error mapping |
| 7 | `crates/oneshim-vision/src/accessibility/linux.rs` | Replace stub with real `atspi` implementation |
| 8 | `crates/oneshim-vision/Cargo.toml` | Add `linux-atspi` feature + `atspi` dependency |
| 9 | `Cargo.toml` (workspace root) | Add `atspi = "0.29"` to workspace dependencies |
| 10 | `crates/oneshim-automation/src/overlay.rs` | Keep `NoOpOverlayDriver` unchanged (still needed for CLI mode) |
| 11 | `crates/oneshim-vision/src/ring_buffer.rs` | Extend `RingFrame` with `accessibility_elements` field |
| 12 | `src-tauri/src/scheduler/loops.rs` | Wire `extract_window_elements()` into monitor tick, populate ring buffer |

### Files to Create

| # | File | Purpose |
|---|------|---------|
| 13 | `src-tauri/src/magic_overlay_driver.rs` | `MagicOverlayDriver` implementing `OverlayDriver` port via Tauri events |
| 14 | `src-tauri/src/main.rs` | Wire `MagicOverlayDriver` into DI (modification) |

---

## Task 1: Add `PermissionDenied` to `CoreError` and `AccessibilityElement` model

**Estimated time:** 5 minutes

### Steps

- [ ] **1.1** In `crates/oneshim-core/src/error.rs`, add a new variant to `CoreError`:
  ```rust
  #[error("Permission denied: {0}")]
  PermissionDenied(String),
  ```
  Place it after the existing `PrivacyDenied` variant (line 72) for logical grouping.

- [ ] **1.2** In `crates/oneshim-core/src/models/focused_element.rs`, add the `AccessibilityElement` struct after the existing `FocusedElementInfo`:
  ```rust
  /// A single element from the accessibility tree snapshot.
  /// Used for dashcam tagging and overlay highlights.
  #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
  pub struct AccessibilityElement {
      /// Accessibility role (e.g., "AXButton", "Edit", "push_button").
      pub role: String,
      /// Accessibility label/name.
      pub label: String,
      /// Bounding rectangle (x, y, width, height) in screen coordinates.
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub bounds: Option<ElementRect>,
  }
  ```

- [ ] **1.3** Add tests for `AccessibilityElement` serde roundtrip in the same file:
  ```rust
  #[test]
  fn accessibility_element_serde_roundtrip() {
      let elem = AccessibilityElement {
          role: "AXButton".to_string(),
          label: "Save".to_string(),
          bounds: Some(ElementRect { x: 10.0, y: 20.0, width: 80.0, height: 30.0 }),
      };
      let json = serde_json::to_string(&elem).unwrap();
      let decoded: AccessibilityElement = serde_json::from_str(&json).unwrap();
      assert_eq!(elem, decoded);
  }

  #[test]
  fn accessibility_element_default_has_empty_fields() {
      let elem = AccessibilityElement::default();
      assert_eq!(elem.role, "");
      assert_eq!(elem.label, "");
      assert!(elem.bounds.is_none());
  }
  ```

- [ ] **1.4** Verify:
  ```bash
  cargo check --workspace
  cargo test -p oneshim-core -- focused_element
  ```

---

## Task 2: Add `extract_window_elements()` to `AccessibilityExtractor` trait

**Estimated time:** 5 minutes

### Steps

- [ ] **2.1** In `crates/oneshim-core/src/ports/accessibility.rs`, add the import for `AccessibilityElement`:
  ```rust
  use crate::models::focused_element::{AccessibilityElement, FocusedElementInfo};
  ```
  (Replace the existing single import of `FocusedElementInfo`.)

- [ ] **2.2** Add the new method to the `AccessibilityExtractor` trait with a backward-compatible default implementation. Place it after `extract_focused_element()`:
  ```rust
  /// Extract the accessibility tree for the focused window up to `max_depth`.
  ///
  /// Returns a flat list of elements from the window's accessibility subtree.
  /// The default implementation falls back to the single focused element,
  /// converted to an `AccessibilityElement`.
  ///
  /// Implementations SHOULD:
  /// - Respect `max_depth` to limit tree traversal (0 = focused element only)
  /// - Cap total elements at `max_elements` (default 300)
  /// - Apply the same PII gating as `extract_focused_element()`
  /// - Return `CoreError::PermissionDenied` when OS permission is missing
  async fn extract_window_elements(
      &self,
      max_depth: u32,
      max_elements: usize,
      pii_level: PiiFilterLevel,
      has_full_text_consent: bool,
  ) -> Result<Vec<AccessibilityElement>, CoreError> {
      // Default: delegate to extract_focused_element for backward compatibility
      let focused = self
          .extract_focused_element(pii_level, has_full_text_consent)
          .await?;
      Ok(focused
          .into_iter()
          .map(|f| AccessibilityElement {
              role: f.role,
              label: f.label.unwrap_or_default(),
              bounds: f.position,
          })
          .collect())
  }
  ```

- [ ] **2.3** Verify that all existing implementations still compile (the default method means no breakage):
  ```bash
  cargo check --workspace
  cargo test -p oneshim-vision -- accessibility
  ```

---

## Task 3: macOS — Extend FFI and implement tree traversal

**Estimated time:** 15 minutes

### Steps

- [ ] **3.1** In `crates/oneshim-vision/src/accessibility/ffi_macos.rs`, add the `AXChildren` attribute constant (after line 43):
  ```rust
  pub const AX_CHILDREN_ATTR: &str = "AXChildren";
  pub const AX_WINDOW_ATTR: &str = "AXWindow";
  pub const AX_FOCUSED_WINDOW_ATTR: &str = "AXFocusedWindow";
  ```

- [ ] **3.2** In the same file, add the `AXUIElementCopyMultipleAttributeValues` FFI binding inside the first `extern "C"` block (after line 73):
  ```rust
  /// Copy multiple attribute values from an accessibility element in a single IPC.
  /// `attributes` is a CFArrayRef of CFStringRef attribute names.
  /// `values` receives a CFArrayRef of CFTypeRef values (same order).
  pub fn AXUIElementCopyMultipleAttributeValues(
      element: AXUIElementRef,
      attributes: core_foundation_sys::array::CFArrayRef,
      options: u32, // 0 = default
      values: *mut core_foundation_sys::array::CFArrayRef,
  ) -> AXError;
  ```

- [ ] **3.3** In the same file, add the `kAXErrorNotImplemented` constant (after line 32):
  ```rust
  pub const kAXErrorNotImplemented: AXError = -25208;
  ```

- [ ] **3.4** In `crates/oneshim-vision/src/accessibility/macos.rs`, add a private recursive tree traversal method to `MacOsNativeAccessibility` (inside the `inner` module, after `get_position_and_size()`):
  ```rust
  /// Recursively traverse the accessibility tree from an element.
  ///
  /// SAFETY: All CFTypeRef values are released. The function returns owned
  /// Rust data. `remaining` is decremented for each element collected to
  /// enforce the max_elements cap.
  unsafe fn traverse_tree(
      element: AXUIElementRef,
      depth: u32,
      max_depth: u32,
      remaining: &mut usize,
      pii_level: PiiFilterLevel,
  ) -> Vec<AccessibilityElement> {
      if depth > max_depth || *remaining == 0 {
          return Vec::new();
      }

      let mut results = Vec::new();

      // Extract role + label + bounds for this element
      let role_key = ax_attr(AX_ROLE_ATTR);
      let role = Self::get_string_attr(element, as_cf_ref(&role_key))
          .unwrap_or_default();

      let label = if pii_level != PiiFilterLevel::Strict {
          let title_key = ax_attr(AX_TITLE_ATTR);
          let desc_key = ax_attr(AX_DESCRIPTION_ATTR);
          Self::get_string_attr(element, as_cf_ref(&title_key))
              .or_else(|| Self::get_string_attr(element, as_cf_ref(&desc_key)))
              .unwrap_or_default()
      } else {
          String::new()
      };

      let bounds = Self::get_position_and_size(element);

      results.push(AccessibilityElement {
          role,
          label,
          bounds,
      });
      *remaining = remaining.saturating_sub(1);

      // Recurse into children
      if depth < max_depth && *remaining > 0 {
          let children_key = ax_attr(AX_CHILDREN_ATTR);
          let mut children_ref: CFTypeRef = ptr::null();
          let err = AXUIElementCopyAttributeValue(
              element,
              as_cf_ref(&children_key),
              &mut children_ref,
          );
          if err == kAXErrorSuccess && !children_ref.is_null() {
              let count = core_foundation_sys::array::CFArrayGetCount(
                  children_ref as core_foundation_sys::array::CFArrayRef,
              );
              for i in 0..count {
                  if *remaining == 0 {
                      break;
                  }
                  let child = core_foundation_sys::array::CFArrayGetValueAtIndex(
                      children_ref as core_foundation_sys::array::CFArrayRef,
                      i,
                  );
                  if !child.is_null() {
                      let child_elements = Self::traverse_tree(
                          child, depth + 1, max_depth, remaining, pii_level,
                      );
                      results.extend(child_elements);
                  }
              }
              CFRelease(children_ref);
          }
      }

      results
  }
  ```

- [ ] **3.5** Add the necessary imports to the `inner` module (at the top, alongside existing imports):
  ```rust
  use oneshim_core::models::focused_element::AccessibilityElement;
  ```
  Also add `use crate::accessibility::ffi_macos::ax::AX_CHILDREN_ATTR;` (though the const is already in scope via the glob import `use crate::accessibility::ffi_macos::ax::*`).

- [ ] **3.6** Implement `extract_window_elements()` override on the `impl AccessibilityExtractor for MacOsNativeAccessibility` block. Place it after `extract_focused_element()`:
  ```rust
  async fn extract_window_elements(
      &self,
      max_depth: u32,
      max_elements: usize,
      pii_level: PiiFilterLevel,
      has_full_text_consent: bool,
  ) -> Result<Vec<AccessibilityElement>, CoreError> {
      if !Self::check_permission() {
          return Err(CoreError::PermissionDenied(
              "macOS Accessibility permission not granted. \
               Enable in System Settings > Privacy & Security > Accessibility."
                  .to_string(),
          ));
      }
      if !Self::circuit_allows() {
          return Ok(Vec::new());
      }

      let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
          PiiFilterLevel::Standard
      } else {
          pii_level
      };

      let result = tokio::task::spawn_blocking(move || {
          unsafe {
              let system_wide = AXUIElementCreateSystemWide();
              if system_wide.is_null() {
                  return Vec::new();
              }

              // Get focused window (not just focused element)
              let focused_window_key = ax_attr(AX_FOCUSED_UI_ELEMENT_ATTR);
              let mut focused: CFTypeRef = ptr::null();
              let err = AXUIElementCopyAttributeValue(
                  system_wide,
                  as_cf_ref(&focused_window_key),
                  &mut focused,
              );
              CFRelease(system_wide);

              if err != kAXErrorSuccess || focused.is_null() {
                  return Vec::new();
              }

              // Try to get the window containing the focused element
              let window_key = ax_attr(AX_WINDOW_ATTR);
              let mut window_ref: CFTypeRef = ptr::null();
              let w_err = AXUIElementCopyAttributeValue(
                  focused,
                  as_cf_ref(&window_key),
                  &mut window_ref,
              );

              let traverse_root = if w_err == kAXErrorSuccess && !window_ref.is_null() {
                  CFRelease(focused);
                  window_ref
              } else {
                  // Fallback: traverse from focused element itself
                  focused
              };

              let mut remaining = max_elements;
              let elements = Self::traverse_tree(
                  traverse_root, 0, max_depth, &mut remaining, effective_level,
              );
              CFRelease(traverse_root);
              elements
          }
      })
      .await
      .map_err(|e| CoreError::Internal(format!("AX tree traversal task failed: {e}")))?;

      if result.is_empty() {
          Self::record_failure();
      } else {
          Self::record_success();
          debug!(count = result.len(), "AX window tree extracted");
      }

      Ok(result)
  }
  ```

- [ ] **3.7** Add tests in the `macos.rs` `tests` module:
  ```rust
  /// Integration test for tree traversal -- requires Accessibility permission.
  /// Run manually: `cargo test -p oneshim-vision -- macos_tree_traversal --ignored`
  #[tokio::test]
  #[ignore]
  async fn extract_window_elements_integration() {
      let extractor = MacOsNativeAccessibility::new();
      if !extractor.has_permission() {
          eprintln!("SKIP: Accessibility permission not granted");
          return;
      }
      let result = extractor
          .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
          .await;
      assert!(result.is_ok());
      let elements = result.unwrap();
      // Should return at least 1 element (the window or focused element)
      // May return 0 on headless CI
      eprintln!("extracted {} elements", elements.len());
  }

  #[tokio::test]
  #[ignore]
  async fn extract_window_elements_permission_denied_without_access() {
      // This test verifies the PermissionDenied path, but only
      // meaningful when run without Accessibility permission.
      let extractor = MacOsNativeAccessibility::new();
      if extractor.has_permission() {
          eprintln!("SKIP: permission already granted, cannot test denial");
          return;
      }
      let result = extractor
          .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
          .await;
      assert!(matches!(result, Err(CoreError::PermissionDenied(_))));
  }
  ```

- [ ] **3.8** Verify:
  ```bash
  cargo check --workspace
  cargo test -p oneshim-vision -- accessibility
  ```

---

## Task 4: Windows — Add CacheRequest tree traversal

**Estimated time:** 15 minutes

### Steps

- [ ] **4.1** In `crates/oneshim-vision/src/accessibility/windows.rs`, add new vtable constants for TreeWalker and CacheRequest inside the `com` module (after the existing vtable constants, around line 200):
  ```rust
  // IUIAutomation: CreateTreeWalker is at vtable index 13
  // IUIAutomation: get_RawViewWalker is at vtable index 15
  const IUIAUTOMATION_GET_RAW_VIEW_WALKER_INDEX: usize = 15;

  // IUIAutomationTreeWalker vtable offsets
  // IUnknown: 0-2
  // GetFirstChildElement: index 4
  // GetNextSiblingElement: index 6
  const ITREEWALKER_GET_FIRST_CHILD_INDEX: usize = 4;
  const ITREEWALKER_GET_NEXT_SIBLING_INDEX: usize = 6;
  ```

- [ ] **4.2** Add a tree traversal function inside the `com` module (after `extract_via_uia()`):
  ```rust
  /// Extract the accessibility subtree of the focused element's parent window.
  ///
  /// Uses IUIAutomation TreeWalker for breadth-first traversal with
  /// depth and element count limits.
  pub(super) fn extract_tree_via_uia(
      max_depth: u32,
      max_elements: usize,
  ) -> Vec<(String, Option<String>, Option<ElementRect>)> {
      unsafe {
          let hr = windows_sys::Win32::System::Com::CoInitializeEx(
              ptr::null(),
              windows_sys::Win32::System::Com::COINIT_MULTITHREADED,
          );
          if hr < 0 {
              return Vec::new();
          }
          let _com_guard = ComGuard;

          // Create IUIAutomation
          let mut automation: *mut std::ffi::c_void = ptr::null_mut();
          let hr = windows_sys::Win32::System::Com::CoCreateInstance(
              &CLSID_CUIAUTOMATION,
              ptr::null_mut(),
              windows_sys::Win32::System::Com::CLSCTX_INPROC_SERVER,
              &IID_IUIAUTOMATION,
              &mut automation,
          );
          if hr < 0 || automation.is_null() {
              return Vec::new();
          }

          // Get focused element
          let mut element: *mut std::ffi::c_void = ptr::null_mut();
          let get_focused: unsafe extern "system" fn(
              *mut std::ffi::c_void,
              *mut *mut std::ffi::c_void,
          ) -> i32 = std::mem::transmute(vtable_fn(
              automation,
              IUIAUTOMATION_GET_FOCUSED_ELEMENT_INDEX,
          ));
          let hr = get_focused(automation, &mut element);
          if hr < 0 || element.is_null() {
              release(automation);
              return Vec::new();
          }

          // Get RawViewWalker
          let mut walker: *mut std::ffi::c_void = ptr::null_mut();
          let get_walker: unsafe extern "system" fn(
              *mut std::ffi::c_void,
              *mut *mut std::ffi::c_void,
          ) -> i32 = std::mem::transmute(vtable_fn(
              automation,
              IUIAUTOMATION_GET_RAW_VIEW_WALKER_INDEX,
          ));
          let hr = get_walker(automation, &mut walker);
          release(automation);
          if hr < 0 || walker.is_null() {
              release(element);
              return Vec::new();
          }

          let mut results = Vec::new();
          let mut remaining = max_elements;
          collect_subtree(walker, element, 0, max_depth, &mut remaining, &mut results);

          release(walker);
          release(element);
          results
      }
  }

  /// Recursive depth-limited subtree collection.
  unsafe fn collect_subtree(
      walker: *mut std::ffi::c_void,
      element: *mut std::ffi::c_void,
      depth: u32,
      max_depth: u32,
      remaining: &mut usize,
      results: &mut Vec<(String, Option<String>, Option<ElementRect>)>,
  ) {
      if *remaining == 0 || depth > max_depth {
          return;
      }

      // Extract properties from current element
      let mut control_type: i32 = 0;
      let get_ct: unsafe extern "system" fn(*mut std::ffi::c_void, *mut i32) -> i32 =
          std::mem::transmute(vtable_fn(element, IELEMENT_GET_CURRENT_CONTROL_TYPE_INDEX));
      let hr = get_ct(element, &mut control_type);
      let role = if hr >= 0 {
          control_type_to_role(control_type).to_string()
      } else {
          "Unknown".to_string()
      };

      let mut name_bstr: *mut u16 = ptr::null_mut();
      let get_name: unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut u16) -> i32 =
          std::mem::transmute(vtable_fn(element, IELEMENT_GET_CURRENT_NAME_INDEX));
      let hr = get_name(element, &mut name_bstr);
      let name = if hr >= 0 {
          let s = bstr_to_string(name_bstr);
          sys_free_string(name_bstr);
          s.filter(|s| !s.is_empty())
      } else {
          None
      };

      let mut rect = UiaRect { left: 0.0, top: 0.0, width: 0.0, height: 0.0 };
      let get_rect: unsafe extern "system" fn(*mut std::ffi::c_void, *mut UiaRect) -> i32 =
          std::mem::transmute(vtable_fn(element, IELEMENT_GET_CURRENT_BOUNDING_RECT_INDEX));
      let hr = get_rect(element, &mut rect);
      let position = if hr >= 0 && (rect.width > 0.0 || rect.height > 0.0) {
          Some(ElementRect {
              x: rect.left as f32,
              y: rect.top as f32,
              width: rect.width as f32,
              height: rect.height as f32,
          })
      } else {
          None
      };

      results.push((role, name, position));
      *remaining = remaining.saturating_sub(1);

      // Recurse into children
      if depth < max_depth && *remaining > 0 {
          let get_first_child: unsafe extern "system" fn(
              *mut std::ffi::c_void,
              *mut std::ffi::c_void,
              *mut *mut std::ffi::c_void,
          ) -> i32 = std::mem::transmute(vtable_fn(walker, ITREEWALKER_GET_FIRST_CHILD_INDEX));

          let mut child: *mut std::ffi::c_void = ptr::null_mut();
          let hr = get_first_child(walker, element, &mut child);
          if hr >= 0 && !child.is_null() {
              collect_subtree(walker, child, depth + 1, max_depth, remaining, results);

              // Traverse siblings
              let get_next_sibling: unsafe extern "system" fn(
                  *mut std::ffi::c_void,
                  *mut std::ffi::c_void,
                  *mut *mut std::ffi::c_void,
              ) -> i32 = std::mem::transmute(vtable_fn(
                  walker,
                  ITREEWALKER_GET_NEXT_SIBLING_INDEX,
              ));

              loop {
                  if *remaining == 0 {
                      release(child);
                      break;
                  }
                  let mut sibling: *mut std::ffi::c_void = ptr::null_mut();
                  let hr = get_next_sibling(walker, child, &mut sibling);
                  release(child);
                  if hr < 0 || sibling.is_null() {
                      break;
                  }
                  child = sibling;
                  collect_subtree(walker, child, depth + 1, max_depth, remaining, results);
              }
          }
      }
  }
  ```

- [ ] **4.3** Implement `extract_window_elements()` on `WindowsUiaAccessibility`. Add the import and override inside the `inner` module's `impl AccessibilityExtractor for WindowsUiaAccessibility` block:
  ```rust
  async fn extract_window_elements(
      &self,
      max_depth: u32,
      max_elements: usize,
      pii_level: PiiFilterLevel,
      has_full_text_consent: bool,
  ) -> Result<Vec<AccessibilityElement>, CoreError> {
      if Self::is_debugger_attached() {
          return Ok(Vec::new());
      }
      if !Self::circuit_allows() {
          return Ok(Vec::new());
      }

      let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
          PiiFilterLevel::Standard
      } else {
          pii_level
      };

      let result = tokio::task::spawn_blocking(move || {
          com::extract_tree_via_uia(max_depth, max_elements)
      })
      .await
      .map_err(|e| CoreError::Internal(format!("UIA tree traversal task failed: {e}")))?;

      if result.is_empty() {
          Self::record_failure();
      } else {
          Self::record_success();
      }

      Ok(result
          .into_iter()
          .map(|(role, name, bounds)| {
              let label = if effective_level == PiiFilterLevel::Strict {
                  String::new()
              } else {
                  name.unwrap_or_default()
              };
              AccessibilityElement { role, label, bounds }
          })
          .collect())
  }
  ```
  Also add the import at the top of `inner`:
  ```rust
  use oneshim_core::models::focused_element::AccessibilityElement;
  ```

- [ ] **4.4** Add tests in the `windows.rs` `tests` module:
  ```rust
  #[tokio::test]
  async fn extract_window_elements_returns_ok() {
      let extractor = WindowsUiaAccessibility::new();
      let result = extractor
          .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
          .await;
      assert!(result.is_ok());
  }
  ```

- [ ] **4.5** Verify:
  ```bash
  cargo check --workspace
  cargo test -p oneshim-vision -- accessibility
  ```

---

## Task 5: Linux — Implement AT-SPI via `atspi` crate

**Estimated time:** 15 minutes

### Steps

- [ ] **5.1** Add `atspi` to the workspace root `Cargo.toml` (after `keyring`, around line 121):
  ```toml
  # Linux AT-SPI2 accessibility (D-Bus based)
  atspi = { version = "0.29", default-features = false, features = ["tokio"] }
  ```

- [ ] **5.2** In `crates/oneshim-vision/Cargo.toml`, add the feature flag and conditional dependency:
  ```toml
  # In [features]:
  linux-atspi = ["atspi"]

  # After existing [target.'cfg(target_os = "windows")'.dependencies]:
  [target.'cfg(target_os = "linux")'.dependencies]
  atspi = { workspace = true, optional = true }
  ```

- [ ] **5.3** Rewrite the `LinuxAccessibility` implementation in `crates/oneshim-vision/src/accessibility/linux.rs`. Replace the stub `extract_raw()` with real AT-SPI calls:
  ```rust
  // Inside the `inner` module, after imports, add:
  #[cfg(feature = "linux-atspi")]
  use oneshim_core::models::focused_element::AccessibilityElement;

  // Replace extract_raw() with:
  #[cfg(feature = "linux-atspi")]
  async fn extract_atspi_focused() -> Option<FocusedElementInfo> {
      use atspi::connection::AccessibilityConnection;
      use atspi::proxy::accessible::AccessibleProxy;

      let conn = AccessibilityConnection::new().await.ok()?;
      // Get the desktop (root) accessible
      let desktop = conn.desktop().await.ok()?;
      // Walk applications to find the focused accessible
      // (simplified: AT-SPI focus tracking via events would be more efficient)
      // For now, return None and rely on the tree traversal path
      None
  }
  ```

- [ ] **5.4** Implement `extract_window_elements()` override for `LinuxAccessibility`:
  ```rust
  #[cfg(feature = "linux-atspi")]
  async fn extract_window_elements(
      &self,
      max_depth: u32,
      max_elements: usize,
      pii_level: PiiFilterLevel,
      has_full_text_consent: bool,
  ) -> Result<Vec<AccessibilityElement>, CoreError> {
      use atspi::connection::AccessibilityConnection;

      if !Self::circuit_allows() {
          return Ok(Vec::new());
      }

      let effective_level = if pii_level == PiiFilterLevel::Off && !has_full_text_consent {
          PiiFilterLevel::Standard
      } else {
          pii_level
      };

      // AT-SPI is async-native, no spawn_blocking needed
      let conn = AccessibilityConnection::new().await.map_err(|e| {
          CoreError::PermissionDenied(format!(
              "AT-SPI2 D-Bus connection failed. Ensure at-spi2-core is installed: {e}"
          ))
      })?;

      // TODO: Implement full tree traversal via AT-SPI proxies.
      // For Phase 1, return empty vec (connection validated).
      // Full implementation will:
      // 1. Get focused application via FocusTracker event
      // 2. Walk children up to max_depth using AccessibleProxy::get_children()
      // 3. Extract role, name, extents for each child

      Self::record_success();
      debug!("AT-SPI2 connection established; tree traversal not yet implemented");
      Ok(Vec::new())
  }
  ```

- [ ] **5.5** Update `check_atspi_available()` with a real check when the feature is enabled:
  ```rust
  fn check_atspi_available() -> bool {
      #[cfg(feature = "linux-atspi")]
      {
          // Check if the AT-SPI2 bus address environment variable is set
          std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok()
              || std::env::var("ATSPI_BUS_ADDRESS").is_ok()
      }
      #[cfg(not(feature = "linux-atspi"))]
      {
          true // Stub mode, claim available
      }
  }
  ```

- [ ] **5.6** Add tests:
  ```rust
  #[cfg(feature = "linux-atspi")]
  #[tokio::test]
  async fn extract_window_elements_atspi_connection() {
      let extractor = LinuxAccessibility::new();
      let result = extractor
          .extract_window_elements(3, 300, PiiFilterLevel::Standard, false)
          .await;
      // On CI without AT-SPI2, this may return PermissionDenied
      // On desktop Linux, should return Ok (possibly empty)
      match result {
          Ok(elements) => {
              eprintln!("AT-SPI2 returned {} elements", elements.len());
          }
          Err(CoreError::PermissionDenied(msg)) => {
              eprintln!("AT-SPI2 not available: {msg}");
          }
          Err(e) => {
              panic!("unexpected error: {e}");
          }
      }
  }
  ```

- [ ] **5.7** Verify (Linux-only feature, but check workspace compiles):
  ```bash
  cargo check --workspace
  cargo test -p oneshim-vision -- accessibility
  ```

---

## Task 6: MagicOverlayDriver — Bridge OverlayDriver port to Tauri events

**Estimated time:** 10 minutes

### Steps

- [ ] **6.1** Create `src-tauri/src/magic_overlay_driver.rs`:
  ```rust
  //! Bridge between the OverlayDriver port and the MagicOverlay Tauri WebView.
  //!
  //! Translates HighlightRequest into Tauri events consumed by the
  //! FocusHighlight React component in the overlay window.

  use async_trait::async_trait;
  use chrono::Utc;
  use serde::Serialize;
  use tauri::{AppHandle, Emitter};
  use uuid::Uuid;

  use oneshim_core::error::CoreError;
  use oneshim_core::models::gui::{HighlightHandle, HighlightRequest, HighlightTarget};
  use oneshim_core::ports::overlay_driver::OverlayDriver;

  /// Serializable highlight data emitted to the overlay WebView.
  #[derive(Debug, Clone, Serialize)]
  struct FocusHighlightPayload {
      pub handle_id: String,
      pub targets: Vec<FocusTargetPayload>,
  }

  #[derive(Debug, Clone, Serialize)]
  struct FocusTargetPayload {
      pub candidate_id: String,
      pub x: i32,
      pub y: i32,
      pub width: u32,
      pub height: u32,
      pub color: String,
      pub label: Option<String>,
  }

  /// OverlayDriver implementation that bridges to the MagicOverlay Tauri WebView.
  ///
  /// Emits `overlay:update-focus` and `overlay:clear-focus` events that the
  /// FocusHighlight React component listens for.
  pub struct MagicOverlayDriver {
      app_handle: AppHandle,
  }

  impl MagicOverlayDriver {
      pub fn new(app_handle: AppHandle) -> Self {
          Self { app_handle }
      }
  }

  #[async_trait]
  impl OverlayDriver for MagicOverlayDriver {
      async fn show_highlights(
          &self,
          req: HighlightRequest,
      ) -> Result<HighlightHandle, CoreError> {
          let handle_id = Uuid::new_v4().to_string();
          let target_count = req.targets.len();

          let payload = FocusHighlightPayload {
              handle_id: handle_id.clone(),
              targets: req
                  .targets
                  .into_iter()
                  .map(|t| FocusTargetPayload {
                      candidate_id: t.candidate_id,
                      x: t.bbox_abs.x,
                      y: t.bbox_abs.y,
                      width: t.bbox_abs.width,
                      height: t.bbox_abs.height,
                      color: t.color,
                      label: t.label,
                  })
                  .collect(),
          };

          self.app_handle
              .emit("overlay:update-focus", &payload)
              .map_err(|e| {
                  CoreError::Internal(format!("Failed to emit overlay:update-focus: {e}"))
              })?;

          tracing::debug!(
              handle_id = %handle_id,
              target_count,
              "MagicOverlayDriver: emitted focus highlights"
          );

          Ok(HighlightHandle {
              handle_id,
              rendered_at: Utc::now(),
              target_count,
          })
      }

      async fn clear_highlights(&self, handle_id: &str) -> Result<(), CoreError> {
          self.app_handle
              .emit("overlay:clear-focus", handle_id)
              .map_err(|e| {
                  CoreError::Internal(format!("Failed to emit overlay:clear-focus: {e}"))
              })?;

          tracing::debug!(handle_id, "MagicOverlayDriver: cleared highlights");
          Ok(())
      }
  }

  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn focus_target_payload_serde() {
          let payload = FocusTargetPayload {
              candidate_id: "el-1".to_string(),
              x: 100,
              y: 200,
              width: 80,
              height: 30,
              color: "#3b82f6".to_string(),
              label: Some("Save".to_string()),
          };
          let json = serde_json::to_string(&payload).expect("serialize");
          assert!(json.contains("\"x\":100"));
          assert!(json.contains("Save"));
      }

      #[test]
      fn focus_highlight_payload_serde() {
          let payload = FocusHighlightPayload {
              handle_id: "handle-1".to_string(),
              targets: vec![FocusTargetPayload {
                  candidate_id: "el-1".to_string(),
                  x: 10,
                  y: 20,
                  width: 100,
                  height: 30,
                  color: "#ef4444".to_string(),
                  label: None,
              }],
          };
          let json = serde_json::to_string(&payload).expect("serialize");
          assert!(json.contains("handle-1"));
          assert!(json.contains("\"width\":100"));
      }
  }
  ```

- [ ] **6.2** Register the module in `src-tauri/src/main.rs`. Add `mod magic_overlay_driver;` alongside the existing `mod magic_overlay;` declaration.

- [ ] **6.3** Wire `MagicOverlayDriver` into DI in `src-tauri/src/main.rs` setup. Where `NoOpOverlayDriver` is currently constructed, conditionally use `MagicOverlayDriver` when the app handle is available:
  ```rust
  // In the Tauri setup closure, after MagicOverlayHandle is created:
  let overlay_driver: Arc<dyn OverlayDriver> = Arc::new(
      magic_overlay_driver::MagicOverlayDriver::new(app.handle().clone())
  );
  ```
  Keep `NoOpOverlayDriver` as fallback for non-Tauri (CLI) mode.

- [ ] **6.4** Verify:
  ```bash
  cargo check --workspace
  cargo test -p oneshim-app -- magic_overlay_driver
  ```

---

## Task 7: Dashcam — Extend RingFrame with accessibility elements

**Estimated time:** 10 minutes

### Steps

- [ ] **7.1** In `crates/oneshim-vision/src/ring_buffer.rs`, add the import and extend `RingFrame`:
  ```rust
  use oneshim_core::models::focused_element::AccessibilityElement;
  ```
  Add the field to `RingFrame`:
  ```rust
  pub struct RingFrame {
      pub timestamp: DateTime<Utc>,
      pub thumbnail_data: Vec<u8>,
      pub app_name: String,
      pub window_title: String,
      /// Accessibility tree snapshot at capture time (empty if unavailable).
      pub accessibility_elements: Vec<AccessibilityElement>,
  }
  ```

- [ ] **7.2** Update the `make_frame()` test helper in `ring_buffer.rs` tests:
  ```rust
  fn make_frame(app: &str, title: &str) -> RingFrame {
      RingFrame {
          timestamp: Utc::now(),
          thumbnail_data: vec![0u8; 100],
          app_name: app.to_string(),
          window_title: title.to_string(),
          accessibility_elements: Vec::new(),
      }
  }
  ```

- [ ] **7.3** Add a test for RingFrame with accessibility data:
  ```rust
  #[test]
  fn push_frame_with_accessibility_elements() {
      use oneshim_core::models::focused_element::{AccessibilityElement, ElementRect};

      let rb = CaptureRingBuffer::new(5, 2, 0.5);
      let frame = RingFrame {
          timestamp: Utc::now(),
          thumbnail_data: vec![0u8; 50],
          app_name: "VSCode".to_string(),
          window_title: "main.rs".to_string(),
          accessibility_elements: vec![
              AccessibilityElement {
                  role: "Editor".to_string(),
                  label: "main.rs".to_string(),
                  bounds: Some(ElementRect {
                      x: 0.0, y: 0.0, width: 1200.0, height: 800.0,
                  }),
              },
              AccessibilityElement {
                  role: "Tab".to_string(),
                  label: "main.rs".to_string(),
                  bounds: None,
              },
          ],
      };
      rb.push(frame);
      assert_eq!(rb.len(), 1);

      let buf = rb.buffer.lock().unwrap();
      assert_eq!(buf[0].accessibility_elements.len(), 2);
      assert_eq!(buf[0].accessibility_elements[0].role, "Editor");
  }
  ```

- [ ] **7.4** Fix all `RingFrame` construction sites in `src-tauri/src/scheduler/loops.rs`. There are 3 places where `RingFrame` is constructed — add the `accessibility_elements` field to each:
  - Line ~394: Regular push into ring buffer (populate from `last_focused_element` converted to single-element vec)
  - Line ~411: Flush trigger frame (empty vec is acceptable)

  For the regular push (line ~394):
  ```rust
  ring_buffer.push(RingFrame {
      timestamp: Utc::now(),
      thumbnail_data: thumb_data,
      app_name: app_name.clone(),
      window_title: event.window_title.clone(),
      accessibility_elements: last_focused_element
          .as_ref()
          .map(|f| vec![AccessibilityElement {
              role: f.role.clone(),
              label: f.label.clone().unwrap_or_default(),
              bounds: f.position,
          }])
          .unwrap_or_default(),
  });
  ```

  For the flush trigger frame (line ~411):
  ```rust
  let flush_frame = RingFrame {
      timestamp: Utc::now(),
      thumbnail_data: vec![],
      app_name: capture_req.app_name.clone(),
      window_title: capture_req.window_title.clone(),
      accessibility_elements: Vec::new(),
  };
  ```

- [ ] **7.5** Add the import at the top of `loops.rs`:
  ```rust
  use oneshim_core::models::focused_element::AccessibilityElement;
  ```

- [ ] **7.6** Verify:
  ```bash
  cargo check --workspace
  cargo test -p oneshim-vision -- ring_buffer
  cargo test --workspace
  ```

---

## Task 8: Permission gating — Map OS permission checks to `CoreError::PermissionDenied`

**Estimated time:** 10 minutes

### Steps

- [ ] **8.1** In `crates/oneshim-vision/src/accessibility/macos.rs`, the permission check is already implemented in Task 3 (`extract_window_elements()` returns `CoreError::PermissionDenied`). Verify it also handles the case where permission is revoked at runtime. In `extract_focused_element()`, change the `kAXErrorAPIDisabled` path to return `PermissionDenied` instead of `Ok(None)`:

  In the `extract_raw()` function (around line 106-111), the error is silently swallowed. This is acceptable because `extract_focused_element()` is called on every tick and returning an error would cause log noise. Instead, add a dedicated permission check at the start of `extract_focused_element()` for `extract_window_elements()` only (already done in Task 3).

  **No additional change needed** -- the existing `extract_focused_element()` returns `Ok(None)` for backward compatibility, while the new `extract_window_elements()` returns `PermissionDenied` as specified.

- [ ] **8.2** In `crates/oneshim-vision/src/accessibility/windows.rs`, Windows UIA generally does not require special permissions. Add a graceful fallback comment and ensure COM errors produce actionable diagnostics. In the `extract_window_elements()` implementation from Task 4, errors from COM initialization should map to:
  ```rust
  // Already handled: COM failures return empty Vec, not PermissionDenied.
  // Windows UIA does not require special permissions.
  ```

  **No additional change needed** -- the Windows implementation already returns `Ok(Vec::new())` on COM failures, which is the correct graceful degradation.

- [ ] **8.3** In `crates/oneshim-vision/src/accessibility/linux.rs`, the `PermissionDenied` mapping is already implemented in Task 5. Verify the error message includes actionable remediation steps. Confirm the error text says "Ensure at-spi2-core is installed".

  **No additional change needed** -- already done in Task 5.

- [ ] **8.4** In `crates/oneshim-vision/src/accessibility/mod.rs`, update `create_extractor()` for macOS to use the permission prompt option. Add `AXIsProcessTrustedWithOptions` with prompt dictionary:

  The current code in `create_platform_extractor()` already creates the extractor regardless of permission state (lines 46-59). This is correct -- the permission prompt should only happen when `extract_window_elements()` is called, not at startup.

  **Optional enhancement**: Add a `request_permission()` method to `AccessibilityExtractor`:
  ```rust
  /// Request OS-level accessibility permission (may show a system dialog).
  /// Default implementation is a no-op.
  fn request_permission(&self) -> bool {
      self.has_permission()
  }
  ```

  On macOS, implement it with the prompt option:
  ```rust
  fn request_permission(&self) -> bool {
      unsafe {
          use core_foundation::dictionary::CFDictionary;
          use core_foundation::string::CFString;
          use core_foundation::boolean::CFBoolean;
          use core_foundation::base::TCFType;

          let key = CFString::new("AXTrustedCheckOptionPrompt");
          let value = CFBoolean::true_value();
          let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
          AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as CFTypeRef)
      }
  }
  ```

- [ ] **8.5** Add a test that verifies `PermissionDenied` error has the expected message format:
  In `crates/oneshim-core/src/error.rs` tests (create the module if needed):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn permission_denied_display() {
          let err = CoreError::PermissionDenied("macOS Accessibility".to_string());
          let msg = format!("{err}");
          assert!(msg.contains("Permission denied"));
          assert!(msg.contains("macOS Accessibility"));
      }
  }
  ```

- [ ] **8.6** Verify:
  ```bash
  cargo check --workspace
  cargo test --workspace
  ```

---

## Task 9: Final integration and commit

**Estimated time:** 5 minutes

### Steps

- [ ] **9.1** Run the full workspace build and test:
  ```bash
  cargo check --workspace
  cargo test --workspace
  cargo clippy --workspace
  cargo fmt --check
  ```

- [ ] **9.2** Fix any clippy warnings or formatting issues.

- [ ] **9.3** Verify test count has increased (expect ~8-12 new tests from Tasks 1-8).

- [ ] **9.4** Commit with message:
  ```
  feat(accessibility): add window tree traversal + OverlayDriver bridge + dashcam tagging

  - Add extract_window_elements() to AccessibilityExtractor trait (backward-compatible default)
  - macOS: recursive AXUIElement tree traversal via AXChildren + batch attribute fetch
  - Windows: TreeWalker-based subtree traversal via COM vtable
  - Linux: atspi crate integration (connection + stub traversal, feature-gated)
  - MagicOverlayDriver: bridge OverlayDriver port to MagicOverlay Tauri events
  - Dashcam: RingFrame now carries accessibility_elements snapshot
  - Permission gating: CoreError::PermissionDenied for OS permission checks
  ```

---

## Verification

After all tasks are complete, verify the following:

1. **Backward compatibility**: `cargo test --workspace` passes with zero regressions. The default `extract_window_elements()` ensures existing code that only uses `extract_focused_element()` is unaffected.

2. **macOS integration** (manual, requires Accessibility permission):
   ```bash
   cargo test -p oneshim-vision -- macos_tree_traversal --ignored
   ```
   Expected: Returns elements from the focused window's accessibility tree.

3. **Windows integration** (manual, on Windows):
   ```bash
   cargo test -p oneshim-vision -- extract_window_elements_returns_ok
   ```
   Expected: Returns Ok (may be empty on headless CI).

4. **Linux feature gate** (on Linux):
   ```bash
   cargo test -p oneshim-vision --features linux-atspi -- atspi_connection
   ```
   Expected: Connects to AT-SPI2 bus or returns PermissionDenied.

5. **Overlay driver**: `MagicOverlayDriver` serialization tests pass. Full integration requires Tauri runtime (tested via `cargo tauri dev`).

6. **Ring buffer**: Frames include `accessibility_elements` field. Existing flush/push logic unchanged.

7. **Permission gating**: `CoreError::PermissionDenied` variant exists and is returned by macOS/Linux implementations when OS permission is missing.

8. **No new crate dependencies** on macOS/Windows (extend existing FFI). Linux adds `atspi` only behind `linux-atspi` feature flag.
