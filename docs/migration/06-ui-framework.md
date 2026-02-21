[English](./06-ui-framework.md) | [한국어](./06-ui-framework.ko.md)

# 6. UI Framework Selection

[← Migration Phases](./05-migration-phases.md) | [Code Sketches →](./07-code-sketches.md)

---

## Candidate Comparison

| Framework | Pros | Cons | Fit |
|-----------|------|------|-----|
| **iced** | Elm architecture, cross-platform, native rendering | Requires GPU, tray handled separately | ★★★★☆ |
| **egui** (+ eframe) | Immediate mode, lightweight and fast, WebAssembly support | Lacks native UX | ★★★★☆ |
| **gtk4-rs** | Native GTK, high maturity | Complex Windows deployment, non-native on macOS | ★★★☆☆ |
| **slint** | Declarative UI, designer tools | License issues (GPL/commercial) | ★★★☆☆ |

## Recommendation: iced or egui

**If choosing iced**:
- Elm-like architecture → clear state management
- Easy custom widgets
- Native tokio integration

**If choosing egui**:
- Immediate mode → fast prototyping
- Better suited for AI code generation (simpler API)
- Lightweight and fast rendering

> Decision to be finalized at Phase 3 entry. Phase 1-2 proceed without UI, so no blocking.
