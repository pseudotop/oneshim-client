# Vision.framework FFI Wiring

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `crates/oneshim-vision/`, `src-tauri/`

## Problem

macOS Vision.framework FFI implementations for rectangle detection (`native_detect/macos.rs`) and text recognition (`native_ocr/macos.rs`) exist as complete code but are:

1. **Missing dependencies**: `objc2`, `objc2-foundation`, `objc2-core-foundation` not in `oneshim-vision/Cargo.toml`
2. **Not exported**: `native_ocr` module not declared in `lib.rs`
3. **Not wired**: Neither module connected to the actual provider pipelines

## Design

### Changes Required

#### 1. Add objc2 Dependencies

```toml
# crates/oneshim-vision/Cargo.toml [target.'cfg(target_os = "macos")'.dependencies]
objc2 = { workspace = true }
objc2-foundation = { workspace = true, features = ["NSData", "NSDictionary", "NSString", "NSArray"] }
objc2-core-foundation = { workspace = true, features = ["CGGeometry"] }
```

Must verify these crates exist in workspace `Cargo.toml`. If not, add them.

#### 2. Export native_ocr Module

```rust
// crates/oneshim-vision/src/lib.rs
#[cfg(feature = "native-vision")]
pub mod native_ocr;
```

#### 3. Wire Native OCR into Provider Pipeline

In `src-tauri/src/provider_adapters/ocr_resolver.rs`, add native OCR as highest-priority provider (before Tesseract):

```
Priority: Native (macOS/Windows) → Tesseract → Remote → Subprocess
```

When `native-vision` feature is enabled on macOS, `create_native_ocr()` returns `Some(Arc<dyn OcrProvider>)` which takes priority over Tesseract.

#### 4. Wire Rectangle Detector into ElementFinder

In `src-tauri/src/automation_runtime.rs`, pass `create_rectangle_detector()` to `OcrElementFinder.with_rectangle_detector()`.

### Files Changed

| File | Description |
|------|-------------|
| `Cargo.toml` (workspace root) | Add objc2 workspace deps if missing |
| `crates/oneshim-vision/Cargo.toml` | Add objc2 macOS deps |
| `crates/oneshim-vision/src/lib.rs` | Export `native_ocr` module |
| `src-tauri/src/provider_adapters/ocr_resolver.rs` | Wire native OCR provider |
| `src-tauri/src/automation_runtime.rs` | Wire rectangle detector |
