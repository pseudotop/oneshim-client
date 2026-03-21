# Architecture Improvements — Design Spec Index

> Created: 2026-03-21
> Revised: 2026-03-21 (post-review)
> Status: Proposed
> Scope: Cross-workspace architecture improvements identified during codebase review

## Overview

Four architecture improvement specs addressing performance, correctness, and compliance gaps across the oneshim-client workspace. All specs have been revised based on review feedback to remove duplicate work, correct factual errors, and expand scope where gaps were identified.

## Spec Summary

| Spec | Priority | Effort | Key Changes in Review |
|------|----------|--------|-----------------------|
| [SQLite Performance Tuning](2026-03-21-sqlite-performance-tuning-design.md) | P1 | 4 days | Corrected PRAGMA values (cache_size=8000 is pages not KB), removed already-implemented items (mmap, page_size, retention), added prepare_cached() and open_in_memory() parity |
| [usearch HNSW Vector Index](2026-03-21-usearch-hnsw-vector-index-design.md) | P1 | 8 days | Added AnnIndex port trait in oneshim-core (key hex arch fix), metadata SQL join, graceful degradation, corruption recovery, retention integration, recall@10 benchmark |
| [Tauri IPC Optimization](2026-03-21-tauri-ipc-optimization-design.md) | P3 (CompressionLayer P1) | 4 days | CompressionLayer moved to first item (15-min ship-immediately fix), corrected dead code count (3/38 not all), added frontend refetchInterval/staleTime as biggest waste, removed simd-json |
| [Cross-Cutting Improvements](2026-03-21-cross-cutting-improvements-design.md) | P1 | 4.5 days | Added frame file deletion (CRITICAL GDPR gap), chose transaction model (Connection::transaction), identified 8 caller sites for vector validation, added log file retention cleanup, PII audit |
| **Total** | | **20.5 days** | |

## Execution Priority

1. **CompressionLayer** (IPC spec, Section A) — Ship immediately, 15-minute fix, highest ROI
2. **GDPR Transaction + Frame Files** (Cross-cutting spec, Phase A) — Compliance-critical, 2.5 days
3. **SQLite PRAGMAs + WAL Checkpoint** (SQLite spec, Phase 1) — Low risk, measurable gains, 2 days
4. **HNSW Port Trait + Adapter** (HNSW spec) — Largest effort, highest long-term value, 8 days
5. **Vector Validation** (Cross-cutting spec, Phase B) — Silent bug fix, 1 day
6. **Frontend Cache Fix** (IPC spec, Phase 2) — High-impact UX optimization, 1.5 days
7. **Remaining SQLite + IPC + Observability** — Incremental, can be interleaved

## Key Review Corrections

### Factual Errors Fixed
- `cache_size=8000` is positive (8000 pages = 32MB), not negative (-8000 = 8MB)
- `mmap_size=256MB` already set — was proposed as new
- `page_size=4096` already set — was proposed as new
- `busy_timeout` does not currently exist (was listed as existing)
- Edge Intelligence retention already exists (was listed as gap)
- usearch Rust API is `reserve(capacity)`, not `reserve_capacity_and_threads`
- Only 3/38 IPC commands are dead, not a broad set
- `EmbeddingError` type does not exist — use `CoreError::Validation`

### Scope Expansions
- HNSW: +AnnIndex port trait, +metadata join, +graceful degradation, +corruption recovery (3 days added)
- Cross-cutting: +frame file deletion, +port trait breaking change, +8 caller migration, +log retention (3 days added)
- IPC: +frontend cache configuration as primary optimization, +CompressionLayer as P1 quick win (1.5 days added)

### Scope Reductions
- SQLite: Removed 5 sections proposing already-implemented features
- IPC: Removed simd-json (not a drop-in replacement, alignment requirements)
- Cross-cutting: Removed QuantizedVector.dimensions field (redundant, serde-breaking)
