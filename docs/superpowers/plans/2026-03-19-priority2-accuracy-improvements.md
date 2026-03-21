# Priority 2: Accuracy Improvements — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade regime detection with HDBSCAN clustering, enable per-category auto-tuning, and provide retroactive recalibration UI for user-driven noise exclusion.

**Architecture:** `ClusteringStrategy` trait in `oneshim-analysis` with `HdbscanDetector` (using `hdbscan` crate + custom classify/constraints) and `KmeansDetector` (wrapping existing). `AutoTuner` (EMA stats + drift detection) updates `ParamResolver` overrides. `OverrideStore` port in `oneshim-core`, `RecalibrationEngine` orchestrates constraint-based re-clustering. Frontend: inline segment recalibration in DashboardDay + dedicated RecalibrationPage.

**Tech Stack:** Rust, hdbscan crate, React 18, Tailwind CSS

**Spec:** `docs/superpowers/specs/2026-03-19-priority2-accuracy-improvements-design.md`

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/models/recalibration.rs` | RegimeOverride, UserOverrideAction, ClusterConstraint models |
| `crates/oneshim-core/src/ports/override_store.rs` | OverrideStore async port trait |
| `crates/oneshim-analysis/src/clustering_strategy.rs` | ClusteringStrategy trait + ClusteringResult |
| `crates/oneshim-analysis/src/hdbscan_detector.rs` | HdbscanDetector (wraps hdbscan crate + custom classify) |
| `crates/oneshim-analysis/src/kmeans_adapter.rs` | KmeansDetector (wraps existing RegimeDetector) |
| `crates/oneshim-analysis/src/auto_tuner.rs` | EmaStatsTracker + DriftDetector + ThresholdAdapter |
| `crates/oneshim-analysis/src/constraint_builder.rs` | ConstraintBuilder (overrides → constraints) |
| `crates/oneshim-storage/src/sqlite/override_store_impl.rs` | OverrideStore SQLite implementation |
| `crates/oneshim-web/src/handlers/recalibration.rs` | Recalibration REST endpoints |
| `crates/oneshim-web/frontend/src/pages/RecalibrationPage.tsx` | Bulk recalibration page |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-analysis/Cargo.toml` | Add `hdbscan` optional dependency |
| `crates/oneshim-analysis/src/lib.rs` | Export new modules |
| `crates/oneshim-analysis/src/regime_detector.rs` | Refactor to work behind ClusteringStrategy |
| `crates/oneshim-core/src/config/sections/analysis.rs` | Add ClusteringAlgorithm + AutoTuningConfig |
| `crates/oneshim-core/src/models/mod.rs` | Add recalibration module |
| `crates/oneshim-core/src/ports/mod.rs` | Add override_store module |
| `crates/oneshim-storage/src/migration.rs` | V12: regime_overrides table |
| `crates/oneshim-storage/src/sqlite/mod.rs` | Add override_store_impl module |
| `src-tauri/src/scheduler/loops.rs` | Wire AutoTuner + constrained re-clustering |
| `src-tauri/src/commands.rs` | Recalibration Tauri commands |
| `crates/oneshim-web/src/routes.rs` | Register recalibration routes |
| `crates/oneshim-web/frontend/src/components/TimelineView.tsx` | Add segment context menu |
| `crates/oneshim-web/frontend/src/App.tsx` | Add recalibration route |

---

## Phase A: Clustering Infrastructure (Tasks 1-5)

### Task 1: Domain models — recalibration + clustering config

**Files:**
- Create: `crates/oneshim-core/src/models/recalibration.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`
- Modify: `crates/oneshim-core/src/config/sections/analysis.rs`

- [ ] Create `recalibration.rs`: `RegimeOverride`, `UserOverrideAction` (MarkAsNoise, ReassignRegime, MarkAsPersonalTime), `ClusterConstraint` (NoiseLabel, ForceCluster, MustLink, CannotLink). All derive `Debug, Clone, Serialize, Deserialize`.
- [ ] Add `ClusteringAlgorithm` enum (Hdbscan default, Kmeans) + `AutoTuningConfig` to `analysis.rs`
- [ ] Add `clustering_algorithm` and `auto_tuning` fields to `TieredMemoryConfig`
- [ ] `cargo check -p oneshim-core`
- [ ] Commit: `feat(core): add recalibration models and clustering config`

### Task 2: OverrideStore port + V12 migration

**Files:**
- Create: `crates/oneshim-core/src/ports/override_store.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`
- Modify: `crates/oneshim-storage/src/migration.rs`

- [ ] Create `OverrideStore` async trait: `save_override`, `list_overrides`, `delete_override`
- [ ] V12 migration: `regime_overrides` table
- [ ] `cargo test -p oneshim-core -p oneshim-storage`
- [ ] Commit: `feat(core): add OverrideStore port and V12 migration`

### Task 3: ClusteringStrategy trait + HdbscanDetector

**Files:**
- Create: `crates/oneshim-analysis/src/clustering_strategy.rs`
- Create: `crates/oneshim-analysis/src/hdbscan_detector.rs`
- Modify: `crates/oneshim-analysis/Cargo.toml`

- [ ] Create `ClusteringStrategy` trait: `detect`, `classify`, `detect_with_constraints`, `algorithm_name`. Returns `Result<ClusteringResult, CoreError>`.
- [ ] Create `ClusteringResult`: labels, centroids, cluster_count, noise_count, probabilities
- [ ] Add `hdbscan` optional dep to Cargo.toml
- [ ] Implement `HdbscanDetector`:
  - `detect()`: normalize features → `hdbscan::Hdbscan::new().cluster()` → compute centroids per label → store in Mutex
  - `classify()`: nearest-centroid against stored centroids (custom, not crate API)
  - `detect_with_constraints()`: exclude NoiseLabel points → cluster → ForceCluster post-assign
  - Error handling: on hdbscan failure, return ClusteringResult with cluster_count 0
- [ ] Tests: detect produces clusters, classify matches nearest centroid, noise handled, constraints applied
- [ ] `cargo test -p oneshim-analysis`
- [ ] Commit: `feat(analysis): add ClusteringStrategy trait and HdbscanDetector`

### Task 4: KmeansDetector adapter

**Files:**
- Create: `crates/oneshim-analysis/src/kmeans_adapter.rs`

- [ ] Wrap existing `RegimeDetector` to implement `ClusteringStrategy`
- [ ] `detect()`: delegate to existing k-means, convert output to `ClusteringResult` (noise_count=0, probabilities=None)
- [ ] `classify()`: nearest centroid (existing logic)
- [ ] `detect_with_constraints()`: apply NoiseLabel/ForceCluster only, warn on MustLink/CannotLink
- [ ] Tests: detect matches existing behavior, classify works
- [ ] Commit: `feat(analysis): add KmeansDetector adapter for ClusteringStrategy`

### Task 5: OverrideStore SQLite impl + ConstraintBuilder

**Files:**
- Create: `crates/oneshim-storage/src/sqlite/override_store_impl.rs`
- Create: `crates/oneshim-analysis/src/constraint_builder.rs`

- [ ] Implement `OverrideStore for SqliteStorage`: save/list/delete overrides
- [ ] `ConstraintBuilder::build_constraints()`: convert overrides → ClusterConstraint vec
- [ ] Tests: override CRUD, constraint building from different action types
- [ ] Commit: `feat(storage): implement OverrideStore + ConstraintBuilder`

---

## Phase B: Auto-Tuning (Tasks 6-8)

### Task 6: EmaStatsTracker

**Files:**
- Create: `crates/oneshim-analysis/src/auto_tuner.rs`

- [ ] `EmaStatsTracker`: per-category/process running EMA of event_rate, importance, variance (Welford's)
- [ ] `update(category, process, event_rate, importance)`
- [ ] `threshold(category, sigma_multiplier) -> Option<f32>`
- [ ] `generate_overrides() -> HashMap<String, TriggerParams>`: t_high = mean + 0.674σ, t_low = mean - 0.674σ
- [ ] Tests: EMA convergence, variance tracking, override generation
- [ ] Commit: `feat(analysis): add EmaStatsTracker for per-category auto-tuning`

### Task 7: DriftDetector

**Files:**
- Modify: `crates/oneshim-analysis/src/auto_tuner.rs`

- [ ] `DriftDetector`: EWMA of error rate, triggers when deviation > threshold_sigma * sqrt(variance)
- [ ] `observe(value) -> bool`: returns true if drift detected
- [ ] `reset()`: after acknowledged drift
- [ ] Tests: no drift in stable data, drift detected on shift, reset works
- [ ] Commit: `feat(analysis): add DriftDetector for regime shift detection`

### Task 8: AutoTuner integration — scheduler + ParamResolver

**Files:**
- Modify: `src-tauri/src/scheduler/loops.rs` or `src-tauri/src/scheduler/analysis_pipeline.rs`

- [ ] In monitor loop: call `ema_tracker.update()` per event with category/process stats
- [ ] Periodically: call `drift_detector.observe()` with classification accuracy
- [ ] If drift: flag for re-clustering
- [ ] Call `generate_overrides()` → feed into `ParamResolver::set_category_override()` (parse String→AppCategory)
- [ ] Wire constrained re-clustering in daily loop: load overrides → build constraints → `clustering_strategy.detect_with_constraints()` → update RegimeManager
- [ ] `cargo check -p oneshim-app`
- [ ] Commit: `feat(scheduler): wire AutoTuner and constrained re-clustering`

---

## Phase C: Recalibration API + Export (Tasks 9-11)

### Task 9: Recalibration REST handlers

**Files:**
- Create: `crates/oneshim-web/src/handlers/recalibration.rs`
- Modify: `crates/oneshim-web/src/handlers/mod.rs`
- Modify: `crates/oneshim-web/src/routes.rs`

- [ ] `POST /api/recalibration/override` → create override
- [ ] `DELETE /api/recalibration/override/:id` → delete override
- [ ] `GET /api/recalibration/overrides?from=...&to=...` → list overrides
- [ ] `POST /api/recalibration/recluster` → trigger on-demand re-clustering
- [ ] Register routes, add handler module
- [ ] Commit: `feat(web): add recalibration REST endpoints`

### Task 10: Tauri commands + manifest

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `docs/contracts/http-interface-manifest.v1.json`

- [ ] Tauri: `create_override`, `delete_override`, `list_overrides`, `trigger_recluster`
- [ ] Register in generate_handler
- [ ] Update HTTP manifest + run verify script + generate OpenAPI
- [ ] Commit: `feat(tauri): add recalibration commands and update manifest`

### Task 11: Export new modules from lib.rs

**Files:**
- Modify: `crates/oneshim-analysis/src/lib.rs`

- [ ] Export: clustering_strategy, hdbscan_detector, kmeans_adapter, auto_tuner, constraint_builder
- [ ] `cargo check --workspace`
- [ ] Commit: `feat(analysis): export Priority 2 modules`

---

## Phase D: Frontend (Tasks 12-14)

### Task 12: Inline recalibration in TimelineView

**Files:**
- Modify: `crates/oneshim-web/frontend/src/components/TimelineView.tsx`

- [ ] Add gear icon / context menu to each timeline block
- [ ] Menu options: "Mark as personal time", "Change regime to..." (dropdown)
- [ ] On action: POST to `/api/recalibration/override`
- [ ] Visual: overridden segments show strikethrough badge
- [ ] Commit: `feat(frontend): add inline segment recalibration to TimelineView`

### Task 13: RecalibrationPage

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/RecalibrationPage.tsx`
- Modify: `crates/oneshim-web/frontend/src/App.tsx`

- [ ] Date range picker
- [ ] Segment list with current regime + override controls
- [ ] "Mark range as personal time" bulk action
- [ ] "Trigger re-clustering" button
- [ ] Override history with undo
- [ ] Route: `/recalibration`
- [ ] Commit: `feat(frontend): add RecalibrationPage for bulk correction`

### Task 14: Final verification

- [ ] `cargo test --workspace`
- [ ] `cargo fmt --check && cargo clippy --workspace`
- [ ] Frontend build
- [ ] `scripts/verify-http-interface-manifest.sh`
- [ ] `git push`

---

## Deferred (Phase 2+)
- MustLink/CannotLink constraint implementation (requires distance matrix access)
- Bayesian GMM as third clustering option
- Automated constraint generation (without user input)
- Regime merge/split UI
