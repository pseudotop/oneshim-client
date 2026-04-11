# Review Polish: 5-Domain Expert Review Fixes

**Date:** 2026-04-11
**Branch:** fix/review-polish (worktree)
**Scope:** 22 confirmed issues across 5 domains, organized in 5 phases

## Background

A 5-domain expert review (Vision/CV, Security, Architecture, Network, Testing) identified 76 issues total. After code-level verification of 17 Critical claims, 9 were false positives and 2 were by-design. This spec covers the 22 confirmed issues requiring fixes.

## Phase 1: Critical Fixes (2 items)

### 1-1. Credit Card Masking Bypass

**File:** `crates/oneshim-vision/src/privacy.rs:273-301`
**Severity:** CRITICAL
**Verified:** Yes — regex patterns defined but unused (`_pattern`), digit masking logic inverted

**Current behavior:**
- `mask_credit_cards()` defines two regex patterns but iterates with `_pattern` (unused)
- The masking logic counts consecutive digits but keeps digits AFTER position 16 instead of masking the first 16
- Result: credit cards are NOT masked; function returns them partially obfuscated

**Fix:**
- Replace the broken loop with a proper state machine that scans for contiguous digit sequences (possibly separated by spaces/hyphens)
- Match two patterns: `\d{4}[- ]\d{4}[- ]\d{4}[- ]\d{4}` and `\d{16}`
- Replace matched spans with `[CARD]`
- No regex crate needed — a character-by-character state machine is sufficient and avoids adding a dependency

**Implementation approach:**
```
fn mask_credit_cards(text: &str) -> String:
  1. Scan for sequences of digits optionally separated by single space or hyphen
  2. If a sequence contains exactly 13-19 digits (valid card lengths), replace entire span with [CARD]
  3. Non-matching sequences pass through unchanged
```

**Tests required:**
- Spaced format: `"Card: 4111 1111 1111 1111"` → `"Card: [CARD]"`
- Hyphenated: `"4111-1111-1111-1111"` → `"[CARD]"`
- Contiguous: `"4111111111111111"` → `"[CARD]"`
- Non-card (15 digits): `"123456789012345"` → unchanged
- Mixed text: `"Call 1234567890 or card 4111111111111111 today"` → only card masked
- Multiple cards in one string
- Short digit sequences (phone numbers) not masked

### 1-2. Circuit Breaker Lock Poisoning

**File:** `crates/oneshim-network/src/circuit_breaker.rs`
**Severity:** HIGH
**Verified:** Yes — uses `std::sync::Mutex` with `.expect()`, panic causes permanent poisoning

**Current behavior:**
- `use std::sync::Mutex;` at line 1
- 5 methods use `.lock().expect("circuit breaker lock poisoned")`: `check()`, `record_success()`, `record_failure()`, `state()`, `reset()`
- If any panic occurs while holding the lock, all subsequent calls panic permanently

**Fix:**
- Replace `std::sync::Mutex` with `parking_lot::Mutex` (already in workspace deps)
- Remove all `.expect()` calls — `parking_lot::Mutex::lock()` returns `MutexGuard` directly (no `Result`)
- No API changes needed

**Tests required:**
- Existing tests continue to pass
- No new tests needed (parking_lot is well-tested; poisoning is eliminated by design)

---

## Phase 2: oneshim-vision Fixes (5 items)

### 2-1. Bounding Box Merge Overflow

**File:** `crates/oneshim-vision/src/gui_detector/correlation.rs:76-85`
**Severity:** IMPORTANT
**Verified:** Yes — `bbox.x + bbox.width` can overflow u32

**Fix:**
- Use `u32::checked_add()` for `bbox.x + bbox.width` and `bbox.y + bbox.height`
- Return `None` (skip merge) if overflow detected
- Alternative: cast to u64 for intermediate calculation, clamp result

**Tests:** Merge with near-u32::MAX coordinates, normal merge unchanged

### 2-2. crop_region_rgba Offset Overflow

**File:** `crates/oneshim-vision/src/gui_detector/mod.rs:172-197`
**Severity:** IMPORTANT
**Verified:** Yes — `(bbox.y + row) * stride` can overflow usize on 32-bit or with extreme values

**Fix:**
- Validate `frame_rgba.len() >= (frame_width as usize) * (frame_height as usize) * 4` upfront
- Use `usize::checked_mul` and `checked_add` for `src_offset` calculation
- Return `None` on overflow

**Tests:** Normal crop, crop at frame edge, oversized bbox values

### 2-3. UTF-8 Byte vs Char Length

**File:** `crates/oneshim-vision/src/gui_detector/correlation.rs:64-66`
**Severity:** IMPORTANT
**Verified:** Yes — `text.len()` returns bytes, not characters

**Fix:**
- Replace `prev.text.len()` with `prev.text.chars().count()`

**Tests:** ASCII text (unchanged behavior), CJK text (wider chars), emoji text

### 2-4. Element Type Inference Improvements

**File:** `crates/oneshim-vision/src/gui_detector/inference.rs:22-111`
**Severity:** MEDIUM
**Verified:** Yes — scrollbar detection lacks edge position check

**Fix:**
- Add screen-edge proximity check for scrollbar candidates: `bbox.x + bbox.width >= self.screen_resolution.0 - margin || bbox.x <= margin`
- Use existing `self.screen_resolution` field (already available in the struct) — no signature change needed
- Add vertical edge check: `bbox.y + bbox.height >= self.screen_resolution.1 - margin || bbox.y <= margin`

**Tests:** Narrow element at screen edge (scrollbar), narrow element in center (not scrollbar)

### 2-5. Thumbnail Cache Weak Hash

**File:** `crates/oneshim-vision/src/thumbnail.rs:20-60`
**Severity:** MEDIUM
**Verified:** Yes — 8x8 sampling (64 pixels) is collision-prone

**Fix:**
- Increase sampling grid from 8x8 to 16x16 (256 pixels, still fast)
- No algorithm change needed, just constant update

**Tests:** Existing tests pass with new grid size

---

## Phase 3: oneshim-network Fixes (6 items)

### 3-1. Circuit Breaker Ok(0) Masking Failures

**File:** `crates/oneshim-network/src/batch_uploader.rs:207-210`
**Severity:** IMPORTANT

**Fix:**
- Add `CircuitOpen` variant to `NetworkError` (defined in `crates/oneshim-network/src/error.rs`)
- Add mapping in `From<NetworkError> for CoreError`: `CircuitOpen => CoreError::ServiceUnavailable("circuit breaker open".into())`
- Return `Err(NetworkError::CircuitOpen)` instead of `Ok(0)` in `flush()`
- Update any exhaustive `match` arms on `NetworkError` to handle the new variant
- Callers can distinguish "nothing to flush" (`Ok(0)`) from "circuit open" (`Err`)

**Tests:** Verify flush returns error when circuit is open

### 3-2. SSE Activity Timeout

**File:** `crates/oneshim-network/src/sse_client.rs:50`
**Severity:** IMPORTANT

**Fix:**
- Wrap the inner event stream loop with `tokio::time::timeout(Duration::from_secs(300), ...)`
- On timeout, log warning, close connection, trigger reconnect
- Make timeout configurable via `SseConfig` (default 5 minutes)

**Tests:** Verify timeout triggers reconnection (mock server that stops sending)

### 3-3. Auth Retry Respects Retry-After

**File:** `crates/oneshim-network/src/auth.rs:140-217`
**Severity:** IMPORTANT

**Fix:**
- On 429 response, parse `Retry-After` header (seconds or HTTP-date)
- Use parsed value as backoff duration instead of exponential default
- Cap at 60 seconds to prevent server-driven DoS

**Tests:** 429 with Retry-After header respected, missing header falls back to exponential

### 3-4. gRPC Lazy Init Race

**File:** `crates/oneshim-network/src/grpc/unified_client.rs:81-109`
**Severity:** MEDIUM

**Fix:**
- Replace `RwLock<Option<T>>` with `tokio::sync::Mutex<Option<T>>` for each gRPC client
- In `ensure_grpc_*()`, hold the Mutex across the entire check-and-init sequence to eliminate TOCTOU
- Pattern: `let mut guard = self.grpc_auth.lock().await; if guard.is_none() { *guard = Some(connect().await?); }`
- This allows future reconnection (setting to None and re-initializing) unlike OnceCell which is write-once
- Note: `tokio::sync::OnceCell` was considered but rejected because gRPC channels may need reset on connection failure
- Trade-off: Holding Mutex across `connect()` serializes concurrent initialization attempts. This is intentional — it eliminates TOCTOU at the cost of brief blocking during first connection. Subsequent calls hit the `is_some()` fast path.

**Tests:** Concurrent initialization produces single connection; reconnection after failure works

### 3-5. gRPC Spawned Task Lifetime

**File:** `crates/oneshim-network/src/grpc/sse_adapter.rs:52-80`
**Severity:** MEDIUM

**Fix:**
- Add `CancellationToken` parameter
- Use `tokio::select!` with cancellation token in the spawned loop
- Return `JoinHandle` for supervision

**Tests:** Verify task stops when token cancelled

### 3-6. SSE Gap Counter Metric

**File:** `crates/oneshim-network/src/sse_client.rs:182-197`
**Severity:** MEDIUM

**Fix:**
- Add `AtomicU64` counter for total gaps detected
- Expose via public method `gap_count() -> u64`
- Existing `warn!` log remains

**Tests:** Verify counter increments on gap detection

---

## Phase 4: Core/Automation/Storage Fixes (6 items)

### 4-1. Consent File TOCTOU

**File:** `crates/oneshim-core/src/consent.rs:170-189`
**Severity:** IMPORTANT

**Fix:**
- Write revocation to temp file, then `fs::rename()` (atomic on same filesystem)
- On revoke: save a full `ConsentRecord` with all fields populated + `pending_deletion: true` instead of deleting the file
- This maintains compatibility with `load_from_file()` which deserializes into `ConsentRecord`
- On load: check `pending_deletion` field — if true, treat as revoked consent and trigger deferred deletion

**Tests:** Concurrent revoke + read doesn't produce inconsistent state

### 4-2. Policy Cache TTL Replay

**File:** `crates/oneshim-automation/src/policy/mod.rs:85-88`
**Severity:** IMPORTANT

**Fix:**
- When `ttl_seconds == 0`, treat ALL tokens as expired (reject replay)
- Add explicit check: `if ttl_seconds == 0 { return Ok(false); }`

**Tests:** TTL=0 rejects previously valid token

### 4-3. Windows ACL Validation

**File:** `crates/oneshim-storage/src/encryption.rs:148-237`
**Severity:** MEDIUM

**Fix:**
- Add assertion: `assert!(needed > 0 && needed <= 4096, "unexpected token info size")`
- Validate `acl_size` doesn't underflow (check `sid_len >= sizeof::<u32>()`)
- Add comment explaining the ACE size calculation

**Tests:** Existing Windows CI tests (manual verification on Windows)

### 4-4. cached_size_bytes Reconciliation

**File:** `crates/oneshim-storage/src/frame_storage.rs:142-144`
**Severity:** IMPORTANT

**Fix:**
- Fix existing `fetch_sub` TOCTOU: replace `load()→min()→fetch_sub()` with atomic `fetch_update()` using `saturating_sub`
- `saturating_sub` alone prevents underflow — the `.min()` guard becomes redundant and is removed
- Pattern: `self.cached_size_bytes.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| Some(current.saturating_sub(deleted_bytes)))`
- Add periodic reconciliation: every 10 cleanup cycles, recalculate from filesystem via directory walk + `fs::metadata().len()`
- Add `reconcile_cache_size()` public method

**Tests:** Add/delete frames, verify cached_size tracks actual disk usage; concurrent add+delete doesn't underflow

### 4-5. Lock Ordering Documentation

**File:** `src-tauri/src/scheduler/mod.rs` (top of file)
**Severity:** IMPORTANT

**Fix:**
- Add lock ordering table as module-level doc comment:
```
// Lock Ordering (acquire in this order to prevent deadlocks):
// 1. deferred_suggestions
// 2. suggestion_queue
// 3. retry_queue
// 4. shared_regime_state (parking_lot::RwLock — fast, sync)
// 5. capture_context (AppState sub-struct)
```

**Tests:** N/A (documentation only)

### 4-6. Suggestion Queue Hash Collision

**File:** `crates/oneshim-suggestion/src/queue.rs:40-53`
**Severity:** MEDIUM

**Fix:**
- Include `suggestion.suggestion_type` in fingerprint input
- Change: `format!("{}{}", suggestion.suggestion_type, normalized_content)`

**Tests:** Two suggestions with same text but different types are not deduplicated

---

## Phase 5: Testing Gaps (3 items)

### 5-1. API Contracts Serde Round-Trip Tests

**File:** `crates/oneshim-api-contracts/src/` (10 priority modules)
**Severity:** MEDIUM

**Scope:** Add serde round-trip tests for:
- `sessions.rs`, `frames.rs`, `events.rs`, `settings.rs`, `suggestions.rs`
- `metrics.rs`, `ai_providers.rs`, `automation.rs`, `coaching.rs`, `focus.rs`

**Pattern per module:**
```rust
#[test]
fn round_trip_session_response() {
    let original = SessionResponse { /* fields */ };
    let json = serde_json::to_string(&original).unwrap();
    let decoded: SessionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(original, decoded);
}
```

**Test count:** ~20-30 tests (2-3 per module)

### 5-2. Action Dispatcher Tests

**File:** `crates/oneshim-automation/src/action_dispatcher.rs`
**Severity:** MEDIUM

**Scope:** Add test module with mock sandbox:
- Successful dispatch returns CommandResult::Success
- Sandbox failure returns CommandResult::Failed
- Each action type variant dispatches correctly

**Test count:** ~5-8 tests

### 5-3. Circuit Breaker Concurrency Tests

**File:** `crates/oneshim-network/src/circuit_breaker.rs`
**Severity:** MEDIUM

**Scope:**
- 10 concurrent threads calling record_failure → verify state transitions to Open
- Concurrent record_success during HalfOpen → verify correct state transition
- Stress test: 100 concurrent check() calls don't panic

**Test count:** ~3-5 tests

---

## Success Criteria

1. All 22 fixes implemented with corresponding tests
2. `cargo check --workspace` passes
3. `cargo clippy --workspace --all-targets -- -D warnings` passes
4. `cargo test --workspace` passes with 0 failures
5. `cargo fmt --check` passes
6. No new `unwrap()` calls introduced
7. No new cross-adapter dependencies introduced

## Out of Scope

- P3 deferrals: Linux Landlock/seccomp, Windows ACL parent dir, ML ONNX model training
- Performance optimization beyond what's needed for correctness
- Refactoring not directly related to identified issues
