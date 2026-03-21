# Architecture Improvements — Index

**Date:** 2026-03-21
**Status:** Split into individual specs (see below)

---

## Active Specs

| # | Spec | Priority | Effort | File |
|---|------|----------|--------|------|
| 1 | SQLite Performance Tuning | **P1** | 4 days | [`2026-03-21-sqlite-performance-tuning-design.md`](2026-03-21-sqlite-performance-tuning-design.md) |
| 2 | USearch HNSW Vector Index | **P1** | 5 days | [`2026-03-21-usearch-hnsw-vector-index-design.md`](2026-03-21-usearch-hnsw-vector-index-design.md) |
| 3 | Tauri IPC Optimization | **P3** | 2.5 days | [`2026-03-21-tauri-ipc-optimization-design.md`](2026-03-21-tauri-ipc-optimization-design.md) |
| 4 | Cross-Cutting Improvements | **P1** | 1.5 days | [`2026-03-21-cross-cutting-improvements-design.md`](2026-03-21-cross-cutting-improvements-design.md) |
| | **Total** | | **13 days** | |

## Deferred Specs

| Spec | Reason | File |
|------|--------|------|
| Audio/STT (`oneshim-stt`) | Customer demand-driven, privacy risk, +350MB deps | [`deferred/audio-stt-research.md`](deferred/audio-stt-research.md) |
| MCP Server | Security concerns + Skills trend | — |

## Recommended Execution Order

1. **Cross-cutting** — Observability + validation (1.5 days)
2. **SQLite Tuning** — Foundation improvement (4 days)
3. **USearch HNSW** — Search quality (5 days)
4. **Tauri IPC** — Only if profiling justifies (2.5 days)

## Dependency Graph

```
Cross-cutting (independent, start immediately)

SQLite Tuning (independent, start immediately)
    │
    ▼
USearch HNSW (benefits from tuned SQLite)

Tauri IPC (independent, low priority)
```
