[English](./README.md) | [한국어](./README.ko.md)

# ONESHIM Client — Rust Native Migration Plan

> **⚠️ This document was moved from `client/docs/rust-migration/`.** (2026-01-28)
>
> **Created**: 2026-01-28
> **Completed**: 2026-01-28
> **Status**: ✅ **Migration Complete** (Phase 0-6, historical snapshot: 163 tests, GA Ready)
> **Decision**: Python Client → Pure Rust Native (not Tauri/Sidecar)
> **UI**: Pure Rust (iced)
> **Tests**: Full Rust (#[test], #[tokio::test])

---

## Document Structure

Open only the relevant file when working on a task.

| # | Document | Content | Status |
|---|----------|---------|--------|
| 1 | [Rationale](./01-rationale.md) | Why Rust, why Full Native (not Sidecar) | ✅ Done |
| 2 | [Project Structure + Dependencies](./02-project-structure.md) | 8 crate structure, Cargo.toml, platform deps | ✅ Done |
| 3 | [Python → Rust Mapping](./03-module-mapping.md) | 180+ Python files → Rust module mapping | ✅ Done |
| 4 | [Server API Integration](./04-server-api.md) | 29 endpoints, SSE event types | ✅ Done |
| 5 | [Migration Phases + Success Criteria](./05-migration-phases.md) | Phase 0-6, checklist, completion criteria | ✅ Done |
| 6 | [UI Framework](./06-ui-framework.md) | iced (better accessibility, desktop optimized vs egui) | ✅ Done |
| 7 | [Code Sketches](./07-code-sketches.md) | Core Rust implementation sketches (models, SSE, suggestions) | 📚 Reference |
| 8 | [Edge Vision Pipeline](./08-edge-vision.md) | Image preprocessing, delta encoding, OCR, timeline | ✅ Done |
| 9 | [Testing Strategy](./09-testing.md) | Per-crate testing, example code | ✅ Done |
| 10 | [Build/Deploy + Risks](./10-build-deploy.md) | Cross-compilation, installers, CI/CD | ✅ Done |

---

## Summary

```
Python Client (current)          →    Rust Client (target)
─────────────────────────────────────────────────────
~100MB+ deployment             →    ~15-25MB single binary
Python + venv install          →    Double-click install
psutil (wrapper)               →    sysinfo (native)
aiohttp (GIL constrained)     →    reqwest + tokio (true async)
No SSE ❌                     →    eventsource-client ✅
No suggestion reception ❌    →    SSE → queue → notification → feedback ✅
Raw JPEG transfer (150-300KB) →    Edge preprocessing: delta/thumbnail/OCR (~10-100KB) ✅
No image rewind ❌            →    Timeline + text search + thumbnail scroll ✅
mss + Pillow (wrapper)        →    xcap + image + webp (native, SIMD)
No OCR ❌                     →    Tesseract FFI (optional feature)
CustomTkinter                  →    iced/egui (pure Rust)
pytest                         →    #[test] + #[tokio::test]
GPL dependency risk            →    MIT/Apache-2.0 only (open-source safe)
```

**Key insight**: Once SSE connection is established in Phase 1, the server's complete Proactive Suggestion pipeline activates immediately. When the Edge Vision pipeline (delta encoding + local OCR + smart trigger) is added in Phase 2, the client can deliver visual context to the server at 1/30 to 1/100 of video bandwidth. **Mixed transmission of metadata + preprocessed images** is the core of ONESHIM Edge processing.

---

## ✅ Migration Complete (2026-01-28)

All phases have been completed:

| Phase | Content | Status |
|-------|---------|--------|
| Phase 0 | Workspace setup, CI/CD | ✅ |
| Phase 1 | Core domain models + Ports | ✅ |
| Phase 2 | Network adapters (HTTP/SSE/WS) | ✅ |
| Phase 3 | Storage + Monitor | ✅ |
| Phase 4 | Vision (Edge image processing) | ✅ |
| Phase 4.5 | Auto-start + OCR + test hardening | ✅ |
| Phase 5 | Auto-update | ✅ |
| Phase 6 | GA preparation (CI/CD, installers, docs) | ✅ |

**Results**:
- 8 crates, 68 source files, ~8,103 lines
- 163 tests, 0 failures, 0 clippy warnings
- GA Ready

**Python client**: `client/` folder has been marked **DEPRECATED** (2026-01-28)

> **📌 Note**: Features have continued to expand since the migration.
> The single source of truth for current quality metrics (test counts, failures, lint/build status) is [STATUS.md](../STATUS.md).
> For the latest development guide, see [CLAUDE.md](../../CLAUDE.md).
> Korean docs are maintained as companion documents, kept semantically aligned with the English primary docs.
