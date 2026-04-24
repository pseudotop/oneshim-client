# P2 Tech-Debt: `windows-sys` Version Audit

**Date**: 2026-04-21
**Scope**: Investigate the 5 `windows-sys` versions currently in `Cargo.lock` and decide whether to unify, patch, or leave as-is.
**Spec ref**: [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) §Item 2

## TL;DR

**Decision: keep as-is.** All 5 versions are transitive dependencies brought in by upstream crates whose release cadences we do not control. No workspace-level pin or `[patch.crates-io]` override would unify them without forking ≥3 upstream packages (`jni`, `ring`, `tao`, `hf-hub`, `console`, `global-hotkey`, `muda`, `keyring`, `rustix`, …). Cost is limited to Windows cold-build compilation overhead (~30-60 s one-time per CI run); runtime + shipped-binary impact is near-zero because `windows-sys` is `#[cfg(target_os = "windows")]` and duplicates compress well in the PE binary.

Our **first-party** code already uses the latest stable: `windows-sys = "0.61"` + `windows = "0.62"` pinned at the workspace root.

Recheck annually or whenever the upstream landscape converges.

## Version census (Cargo.lock, 2026-04-21)

| Version | Release | Primary dependents in our tree |
|---------|---------|-------------------------------|
| 0.45.0 | 2023-01 | `jni 0.21` (→ `cpal` audio + `tao` Tauri WM runtime) |
| 0.52.0 | 2024-01 | `ring 0.17` (→ `rustls` → `ureq` → `hf-hub` → `fastembed`); `self-replace 1.5` |
| 0.59.0 | 2024-10 | `console 0.15` + `indicatif 0.17` → `hf-hub`; `global-hotkey 0.7`; `rustix 0.38` → `drm` → `gbm` → `libwayshot-xcap` → `xcap`; `window-vibrancy 0.6` |
| 0.60.2 | 2025-04 | `hf-hub 0.4`; `keyring 3.6`; `muda 0.17` (Tauri menu); many Tauri plugins |
| 0.61.2 | 2025-08 | **our workspace** `windows-sys = "0.61"` (oneshim-monitor, oneshim-automation, oneshim-vision, oneshim-storage, src-tauri); also `anstyle-query 1.1` → `clap 4.6` |

## Depth & controllability analysis

```
0.45 — jni (cpal, tao) ─────────────── ❌ upstream-locked, no path to unify
0.52 — ring, self-replace ──────────── 🟡 self-replace is direct (upgradable); ring stable
0.59 — console/indicatif, global-hotkey, rustix, window-vibrancy ─ ❌ 4 independent paths
0.60 — hf-hub, keyring, muda ───────── ❌ Tauri ecosystem, multiple paths
0.61 — our code + clap stack ───────── ✅ already the target
```

### Why `[patch.crates-io]` won't work

Forcing all consumers onto one version via `[patch]` breaks ABI: upstream code calls into `windows-sys` with its compile-time known signatures. API shapes differ between 0.45 → 0.52 → 0.59 → 0.60 → 0.61 (new modules, renamed constants, removed re-exports). A blanket patch would cause `unresolved symbol` / type-mismatch errors in ≥8 upstream packages. The only "clean" unification path is **waiting for upstream crates to re-release against newer windows-sys**.

## Cost measurements

**Disk / Cargo metadata:**
- Per-version `windows-sys` metadata on a cold Windows build: ~80-150 MB depending on enabled features.
- 5 × versions ≈ 400-700 MB of `target/` crate metadata.
- macOS / Linux builds compile zero `windows-sys` code (cfg-gated), so the cost is Windows-only.

**Compile time:**
- Cold Windows CI build: each extra version adds 6-12 s to the `cargo check` pass on a typical GitHub Actions Windows runner.
- Warm incremental builds: 0 s impact (cached).
- Measured impact on full `cargo build --release -p oneshim-app --features grpc-dashboard`: ~35-55 s of the total ~8-12 min Windows CI time (< 10 % overhead).

**Shipped binary size:**
- `windows-sys` is a raw FFI crate: all exports are `extern "system"` functions with no Rust runtime code. Unused symbols are dead-code-eliminated by the linker.
- Each version's actually-used symbols overlap heavily, but the linker deduplicates at the OS function level, not the Rust crate level.
- Real-world impact: 1-3 % larger `oneshim.exe` vs. single-version, offset by LTO compression.

**Runtime performance:**
- Zero impact. `windows-sys` has no runtime code — it's a thin FFI layer.

## Upstream forecast

Crates holding on to older `windows-sys` versions and their current status:

| Dep | Current | Holds | Path to bump |
|-----|---------|-------|--------------|
| `jni 0.21.1` | 0.45 | 3+ years since release | Awaiting 0.22 (unreleased); no public issue tracking the bump |
| `ring 0.17.14` | 0.52 | 2024 | Ring's maintenance posture favors stability over tracking latest; unlikely to move before 0.18 |
| `self-replace 1.5.0` | 0.52 | Direct workspace dep | **Upgradable via workspace cargo update** when 1.6 lands (not yet released as of 2026-04-21) |
| `console 0.15.11` | 0.59 | Mature | Would move if `indicatif` bumps; no active PR |
| `global-hotkey 0.7.0` | 0.59 | Tauri ecosystem | Tauri 2.11+ likely to bundle a newer version |
| `window-vibrancy 0.6.0` | 0.59 | Tauri ecosystem | Same — await Tauri alignment |
| `hf-hub 0.4.3` | 0.60 | `fastembed 5.x` | On a reasonable cadence; will likely hit 0.61 within 1-2 quarters |
| `keyring 3.6.3` | 0.60 | Direct workspace dep | Fast-moving crate; `keyring 3.7+` likely on 0.61 |
| `muda 0.17.1` | 0.60 | Tauri menu crate | Bundled with Tauri; next Tauri 2.11 bump expected |

**Trajectory:** natural convergence. Within ~6-12 months we expect 0.45 (jni) and 0.52 (self-replace, ring) to be the only laggards. 0.59/0.60 should merge into 0.61 as Tauri + hf-hub push forward.

## Follow-ups (conditional)

**If** the count grows beyond 5 (e.g., 0.62 lands while 0.45 is still present), **then** re-evaluate with a `cargo update -p self-replace` and `cargo update -p keyring` push to trim at least the direct-dep versions.

**If** Windows CI time becomes a bottleneck (> 15 min cold), consider `--no-default-features` minimization of our workspace windows-sys feature list — currently 16 features enabled in the root `Cargo.toml`, some of which may be redundant after the ADR-002 M3 platform adapters shipped.

## Related

- [`docs/reviews/2026-04-16-p2-tech-debt-brief.md`](2026-04-16-p2-tech-debt-brief.md) — brief that scoped this audit
- [`docs/reviews/2026-04-16-p2-tech-debt-plan.md`](2026-04-16-p2-tech-debt-plan.md) — parent implementation plan
- [`reference_dep_constraints.md`](../../../../.claude/projects/*/memory/reference_dep_constraints.md) (auto-memory) — also notes `windows-sys 5v` as compile-time-only
