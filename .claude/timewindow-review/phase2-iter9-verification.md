# TimeWindow Phase 2 iter-10 Verification — Plan v9

**Date**: 2026-04-25 ~18:30
**Reviewer**: Independent verifier subagent
**Worktree**: `.claude/worktrees/timewindow-primitive/` (branch `refactor/timewindow-primitive`)
**Plan reviewed**: `docs/superpowers/plans/2026-04-25-timewindow-primitive-plan.md` (commit `ccef4916`, 2798 lines)
**Recent commits in scope**:
- `218072c3` Task 6 service-layer scope expansion (7 service files)
- `6ccf0bda` Task 7 service-layer correction (data + reports)
- `c8ca9d43` Step 4C.5 PRESERVE-BODY + half-open boundary
- `22729b2e` Step 4C.1 list_segment_time_ranges containment semantic
- `ccef4916` cross-layer audit (Tauri/gRPC/network clean)

---

## VERIFIED (v9 corrections)

### NEW-C1 Task 6 (REST Handler + Service Layer Migration)

**✓ All v9 prescriptions correctly applied:**

- ✓ Task 6 renamed to "REST Handler + Service Layer Migration (Phase 2 iter-9 SCOPE EXPANSION)" at line 1889.
- ✓ Estimate raised to 5h (was 3h) at line 1891.
- ✓ Step 6.0 enumerates 6 handler files + 7 service files via grep at lines 1907–1921.
- ✓ Steps 6.1–6.7 enumerate the 7 services (frames/events/metrics/focus/idle/processes/timeline) at lines 1923–1980.
- ✓ Step 6.8 documents helper deprecation decision (option (b) keep) at lines 1982–1990.
- ✓ Step 6.10 commit message references service files (NOT handler files) at line 2002.
- ✓ Step 6.11 explicitly says handler files need ZERO changes at lines 2035–2037.

**Cross-check grep confirms enumeration accuracy:**
```
$ grep -rln "\.from_datetime()\|\.to_datetime()" crates/oneshim-web/src/services/
crates/oneshim-web/src/services/frames_service.rs       (2 sites: lines 24-25)
crates/oneshim-web/src/services/timeline_service.rs     (2 sites: lines 24-25)
crates/oneshim-web/src/services/focus_service.rs        (4 sites: lines 53-54, 68-69)
crates/oneshim-web/src/services/metrics_service.rs      (2 sites: lines 25-26)
crates/oneshim-web/src/services/idle_service.rs         (2 sites: lines 22-23)
crates/oneshim-web/src/services/events_service.rs       (2 sites: lines 27-28)
crates/oneshim-web/src/services/processes_service.rs    (2 sites: lines 22-23)
```
**Total: 7 files, 16 helper-call sites (matches plan v9 Step 6.0 expectation exactly).**

**Production handler files do NOT use the helpers** — `crates/oneshim-web/src/handlers/timeline.rs:57-58` and `crates/oneshim-web/src/handlers/mod.rs:61-62, 80` are inside `#[cfg(test)]` blocks (verified via Read of both files). So Step 6.11's "handler files need ZERO changes" is correct for production code.

### Task 7 service-layer correction (Steps 7.4 + 7.5)

**✓ All correctly retargeted to services:**

- ✓ Step 7.4 targets `data_web_service.rs` (NOT `handlers/data.rs`); refactors lines 36+51 via `request.period()` accessor; explicitly says handler unchanged at line 2209.
- ✓ Step 7.5 targets `reports_service.rs` (NOT `handlers/reports.rs`); moves period dispatch to service; explicitly says handler unchanged at line 2238.
- ✓ Commit `git add` at line 2250 references `services/{data_web_service,reports_service}.rs` (NOT handlers).

**Handler thin-delegate verification (Read of actual files):**
- `crates/oneshim-web/src/handlers/data.rs:9-16`: thin pass-through `DataCommandService::new(context).delete_data_range(&request)?` ✓
- `crates/oneshim-web/src/handlers/reports.rs:11-19`: thin pass-through `ReportQueryService::new(context).generate_report(&params).await?` ✓

### Step 4C.5 work_sessions half-open boundary

**✓ Correctly preserved per NG6:**

- ✓ Plan line 1375: "**⚠ Half-open boundary preserved per NG6**: This query uses `started_at >= ?1 AND started_at < ?2` (half-open `[from, to)` upper bound), NOT closed-closed like the other range helpers."
- ✓ Diff at line 1395 shows the SQL line preserved verbatim with explicit comment `-- ← HALF-OPEN preserved per NG6`.
- ✓ Path corrected to `crates/oneshim-storage/src/sqlite/edge_intelligence/work_sessions.rs:216` at line 1373 (matches actual filesystem location).
- ✓ Actual SQL at edge_intelligence/work_sessions.rs:231 confirms `started_at >= ?1 AND started_at < ?2` is current production code (matches plan claim).

### Step 4C.1 list_segment_time_ranges containment semantic

**✓ Correctly preserved per NG6 + iter-9 documentation:**

- ✓ Plan line 1152: "**⚠ Containment semantic (preserve)**: The query at line 262 uses `WHERE start_time >= ?1 AND end_time <= ?2` — DIFFERENT columns on each side. This is a 'fully contained' semantic..."
- ✓ Actual SQL at calibration_store_impl.rs:262 confirms `WHERE start_time >= ?1 AND end_time <= ?2` (matches plan claim).
- ✓ Method 3 PRESERVE-BODY diff at lines 1155+ preserves the SQL verbatim, only swaps signature + adds locals binding.

### Cross-layer audit (Tauri/gRPC/network clean)

**✓ Independent grep confirms phase3-readiness-state.md claim:**

```
$ grep -rln "TimeRangeQuery\|count_events_in_range\|count_frames_in_range\|delete_data_in_range\|get_daily_active_secs\|list_frame_file_paths_in_range\|flag_noise_range\|\.from_datetime()\|\.to_datetime()" \
    src-tauri/src/commands/ crates/oneshim-web/src/grpc/ crates/oneshim-network/src/
[zero matches]
```

No additional layer requires migration. Plan v9 scope is bounded to service+handler+storage+scheduler+api-contracts, as documented.

---

## NEW CRITICAL ISSUES (introduced by v9)

### NEW-C1 (Critical) — Silent default-window-size regression (24h → 7d / 30d)

**Severity**: Critical (silent user-facing behavior change, not disclosed in commit/PHASE-HISTORY)

**File evidence**:

Existing helper behavior (verified at `crates/oneshim-api-contracts/src/common.rs:27-50`):
```rust
pub fn from_datetime(&self) -> DateTime<Utc> {
    self.from
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - Duration::hours(24))   // ← 24 HOURS
}
pub fn to_datetime(&self) -> DateTime<Utc> {
    // ... .unwrap_or_else(Utc::now)
}
```

Effective default for default-no-bounds requests (e.g. `GET /frames` with no `?from`/`?to`):
- **Today (production)**: `[now - 24h, now]`  → returns last **24 hours** of data
- **After plan v9 Task 6**: `[now - 7d, now]` for frames/events/metrics/idle/processes/timeline; `[now - 30d, now]` for focus

That's a **7× widening** for the most common case (and **30× widening** for focus). Affects any caller hitting REST endpoints without explicit `from`/`to`.

**What's in the plan**:
- Plan lines 1900–1905 explicitly call out the **parse-error change** (invalid timestamps → HTTP 400 vs. silent 24h fallback) as "behavior change … strict API contract improvement".
- Plan does NOT mention the **default-window-size change** (24h → 7d/30d).
- Step 6.10 commit message (lines 2023–2026) only documents the parse-error change.
- Task 11 PHASE-HISTORY entry (lines 2611–2621) documents the 2 new wire codes + 37 new tests but is silent on default-window-size.

**Why Critical**:
1. Silent change to a public REST API contract — frontend/integrators that depend on the 24h default get unexpectedly larger payloads.
2. Performance regression risk — a `/frames` call with default bounds now scans/serializes 7× more data per request.
3. The spec line 81 (`U5`) and line 511 ("Verify default lookback values match prior code") implies the implementer should *match* prior behavior. But "prior behavior" is uniformly `Duration::hours(24)`, not 7d/30d. Either:
   - The lookback test will fail (if implementer reads spec §8.3 literally), OR
   - The implementer silently approves the new defaults by writing the test against the new values.

**Recommended fix in iter-11**:
- Either (a) align the Task 6 default lookback values to the *current* `Duration::hours(24)` to preserve behavior, OR (b) explicitly call out the design choice in plan (and Step 6.10 commit / Step 11.3 PHASE-HISTORY) that defaults are intentionally widening to 7d / 30d. Add a regression test that asserts the new default matches the new prescribed value, with a comment noting the prior default.
- Spec §8.3 line 511 ("Verify default lookback values match prior code") should be updated to say "match plan-prescribed default lookback (intentionally widened from `Duration::hours(24)` legacy default)".
- Add an explicit bullet in Step 11.3 PHASE-HISTORY.md entry: "Default REST query window widened from `Duration::hours(24)` to domain-specific (frames/events/metrics/idle/processes/timeline=7d, focus=30d) when no `?from`/`?to` provided."

### NEW-C2 (Critical) — Step 6 misleading "(Task 4 already updated their sigs)" — only 1 of 8 storage call paths is actually migrated

**Severity**: Critical (will cause non-trivial implementer churn during execution)

Step 6.1's example code (lines 1942–1949) prescribes:
```rust
pub fn get_frames(&self, params: &TimeRangeQuery) -> Result<...> {
    let window = params.to_time_window(Duration::days(7))
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    // ... pass &window to storage methods (Task 4 already updated their signatures)
}
```

Step 6.10 commit message (line 2021): "then passes &window to storage methods (Task 4 already updated their sigs)."

**Reality (verified via Read of each service):**

| Service | Storage method called | Signature today | Task 4 migrated? |
|---------|----------------------|-----------------|------------------|
| `frames_service.rs:33` | `get_frames(from: DateTime, to: DateTime, limit)` | `DateTime` | **❌ NO** (only `count_frames_in_range`/`list_frame_file_paths_in_range` are in Task 4 — `get_frames` is not) |
| `events_service.rs:35` | `count_events_in_range(&str, &str)` | `&str` (today) | ✅ YES |
| `events_service.rs:42` | `get_events(from: DateTime, to: DateTime, limit)` | `DateTime` | **❌ NO** |
| `metrics_service.rs:25-30` | `get_metrics(from: DateTime, to: DateTime, limit)` | `DateTime` | **❌ NO** |
| `metrics_service.rs:67-70` | `list_hourly_metrics_since(&str)` | single `&str` | **❌ NO** |
| `focus_service.rs:59` | `list_work_sessions(&str, &str, limit)` | `&str` | **❌ NO** |
| `focus_service.rs:74` | `list_interruptions(&str, &str, limit)` | `&str` | **❌ NO** |
| `idle_service.rs:25` | `get_idle_periods(from: DateTime, to: DateTime)` | `DateTime` | **❌ NO** |
| `processes_service.rs:25` | `get_process_snapshots(from: DateTime, to: DateTime, limit)` | `DateTime` | **❌ NO** |
| `timeline_service.rs:40-42` | `get_events(...)`, `get_frames(...)`, `get_idle_periods(...)` | `DateTime` | **❌ NO** |

**Only 1 of 10+ storage call paths used by these 7 services is actually `&TimeWindow`-migrated by Task 4.** All other call sites still take `DateTime` or `&str` parameters and require the implementer to decompose `&window` back via `window.start, window.end` or `window.to_sql_pair()`.

**Why Critical**:
- Step 6.1's "pass &window directly" comment is factually wrong for 9 of 10 call sites.
- The implementer will hit a chain of compile errors after applying Step 6.1's pattern, then either (a) re-decompose `window` back to `DateTime`/`String` ad-hoc (clean but undocumented), or (b) panic-extend Task 4 scope to migrate the missing storage methods (scope creep into a 5h task).
- Step 6.10 commit message inherits the same misleading wording ("passes &window to storage methods (Task 4 already updated their sigs)").

**Recommended fix in iter-11**:
- Either (a) remove the "(Task 4 already updated their signatures)" claim from Step 6.1 and Step 6.10 commit, and explicitly prescribe the decomposition pattern: `let (from, to) = (window.start, window.end);  // storage still takes DateTime` (or `let (from, to) = window.to_sql_pair();  // storage still takes &str`), OR (b) extend Task 4 to migrate `get_frames`, `get_events`, `get_metrics`, `get_process_snapshots`, `get_idle_periods`, `list_work_sessions`, `list_interruptions`, `list_hourly_metrics_since` to `&TimeWindow` (raises Task 4 estimate substantially).
- Recommend (a) for surgical scope (matches the spirit of Step 6.8's "non-deprecated helpers stay" decision).
- Add a dedicated Step 6.0.5 "**Storage signature audit**: this migration changes ONLY window construction (parse + validation), not storage signatures. The 1 method `count_events_in_range` was migrated by Task 4; the remaining 8+ storage methods stay un-migrated. Each service decomposes `&window` back to `(window.start, window.end)` (DateTime) or `(window.to_sql_pair().0, window.to_sql_pair().1)` (String) as needed for the existing storage signature."

---

## NEW IMPORTANT ISSUES (introduced by v9)

### NEW-I1 (Important) — Step 6.2 missing "continuation refactor on top of Task 4D.3" wording

**Severity**: Important (clarity issue — implementer can recover but it's confusing)

Step 7.4 (line 2191) sets a clear precedent: "Since Task 4D.3 already migrated `data_web_service.rs:36+51` to build TimeWindow inline via `TimeWindow::from_rfc3339_pair(&request.from, &request.to)?`, this step **simplifies the call sites** to use the new `period()` accessor (defined in Task 7.1)".

Step 6.2 (events_service migration) is a structurally similar continuation — Task 4D.3 line 1657–1660 wrote:
```rust
let window = TimeWindow::new(from, to)
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
let total = self.ctx.storage.count_events_in_range(&window)
```
…where `from = params.from_datetime()` and `to = params.to_datetime()` (still using helpers).

Step 6.2 then says "Same pattern. Default lookback `Duration::days(7)`. Note `count_events_in_range` on storage (Task 4) now takes `&TimeWindow` — caller passes `&window` directly." This is technically correct but lacks the "Task 4D.3 already wrote X; this step simplifies to Y" framing that Step 7.4 has. The implementer might:
- Re-do Task 4D.3's migration (creating a duplicate `let window = TimeWindow::new(...)` line), or
- Be confused about what state the file is in by Step 6 time.

**Recommended fix in iter-11**:
Make Step 6.2 explicit:
> "**Note**: Task 4D.3 already migrated this file's storage call to `count_events_in_range(&window)` where `window = TimeWindow::new(from, to).map_err(BadRequest)?`. This Step 6.2 *simplifies the window construction* by replacing the manual `TimeWindow::new(params.from_datetime(), params.to_datetime())` chain with `params.to_time_window(Duration::days(7)).map_err(BadRequest)?`. The storage call itself stays at `count_events_in_range(&window)`."

### NEW-I2 (Important) — Step 11.3 PHASE-HISTORY entry inherits incomplete behavior-change disclosure

**Severity**: Important (documentation completeness — derives from NEW-C1)

Step 11.3 PHASE-HISTORY entry at lines 2611–2621 lists wire codes, test counts, and external API contract preservation, but does NOT mention either:
- The parse-error → HTTP 400 behavior change (mentioned in Step 6.10 commit but not bubbled up to PHASE-HISTORY)
- The default-window-size widening from 24h to 7d/30d (NEW-C1)

These are user-facing changes that should appear in PHASE-HISTORY.md so downstream consumers (frontend authors, integrators, ops) see the change.

**Recommended fix in iter-11**: Add two bullets to the PHASE-HISTORY entry:
- "Behavior change: invalid `?from`/`?to` timestamps now return HTTP 400 BadRequest (previously silently fell through to a hardcoded 24h fallback and returned 200 OK with default-window data)."
- "Behavior change: default REST query window (when neither `?from` nor `?to` provided) widened from `Duration::hours(24)` to domain-specific (frames/events/metrics/idle/processes/timeline = `Duration::days(7)`, focus = `Duration::days(30)`)."

---

## CROSS-LAYER AUDIT

```
$ grep -rln "TimeRangeQuery\|count_events_in_range\|count_frames_in_range\|delete_data_in_range\|get_daily_active_secs\|list_frame_file_paths_in_range\|flag_noise_range\|\.from_datetime()\|\.to_datetime()" \
    src-tauri/src/commands/ crates/oneshim-web/src/grpc/ crates/oneshim-network/src/
```

**Result**: zero matches.

**Verdict**: ✓ Tauri (`src-tauri/src/commands/`), gRPC (`crates/oneshim-web/src/grpc/`), and network adapter (`crates/oneshim-network/src/`) are all clean — no additional layers consume the 8 SQL helpers, the `TimeRangeQuery` type, or the `from_datetime/to_datetime` accessors. Plan v9's claim in `phase3-readiness-state.md` that scope is bounded to {service+handler+storage+scheduler+api-contracts} is confirmed.

---

## VERDICT

**[NEEDS PHASE 2 iter-11]**

Plan v9 successfully:
- Migrated Task 6 to the correct architectural layer (service, not handler) with accurate file enumeration ✓
- Migrated Task 7 to the correct architectural layer (service, not handler) ✓
- Documented the half-open boundary (Step 4C.5) and the containment semantic (Step 4C.1) ✓
- Confirmed no additional Tauri/gRPC/network layers need migration ✓

But plan v9 introduced **2 NEW Critical** + **2 NEW Important** issues that need iter-11 to address:

| ID | Severity | Summary | Resolution direction |
|----|---------|---------|----------------------|
| NEW-C1 | Critical | Silent default-window-size regression (24h → 7d/30d) for default-no-bounds REST requests | Document in plan + commit + PHASE-HISTORY OR realign defaults to `Duration::hours(24)` |
| NEW-C2 | Critical | "Task 4 already updated their sigs" claim is wrong for 9 of 10 storage call sites — only `count_events_in_range` is migrated; remaining 9 storage methods stay on `DateTime`/`&str` | Add Step 6.0.5 storage-signature audit; correct Step 6.1 example + Step 6.10 commit msg to prescribe `let (from, to) = (window.start, window.end);` decomposition pattern |
| NEW-I1 | Important | Step 6.2 missing the "Task 4D.3 already wrote X; this step simplifies to Y" framing that Step 7.4 has | Add explicit "continuation refactor on top of Task 4D.3" wording to Step 6.2 |
| NEW-I2 | Important | Step 11.3 PHASE-HISTORY entry incomplete — missing parse-error + default-window-size behavior changes | Add 2 bullets to PHASE-HISTORY entry |

The user mandate ("Critical+Important 이슈가 없을 때까지") is not met. Recommend iter-11 to address all 4 issues before Phase 2 EXIT.

---

**End of Phase 2 iter-10 verification.**
