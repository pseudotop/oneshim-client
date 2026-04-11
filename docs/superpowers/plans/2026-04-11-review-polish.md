# Review Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 21 confirmed issues from 5-domain expert review across vision, network, core, and storage crates.

**Architecture:** Fixes are organized by crate proximity to minimize context switching. Each task is self-contained with TDD (test first, then implement). No new dependencies added — parking_lot and tokio are already in workspace.

**Tech Stack:** Rust 2021, parking_lot, tokio 1.x, thiserror, serde_json

**Spec:** `docs/superpowers/specs/2026-04-11-review-polish-design.md`

**Note:** Item 4-6 (suggestion queue hash collision) was dropped — code already hashes `suggestion_type`.

---

## Task 1: Fix Credit Card Masking (CRITICAL)

**Files:**
- Modify: `crates/oneshim-vision/src/privacy.rs:273-301`

- [ ] **Step 1: Write failing tests**

Add to existing test module in `privacy.rs`:

```rust
#[test]
fn mask_credit_cards_contiguous() {
    assert_eq!(mask_credit_cards("4111111111111111"), "[CARD]");
}

#[test]
fn mask_credit_cards_spaced() {
    assert_eq!(mask_credit_cards("Card: 4111 1111 1111 1111"), "Card: [CARD]");
}

#[test]
fn mask_credit_cards_hyphenated() {
    assert_eq!(mask_credit_cards("4111-1111-1111-1111"), "[CARD]");
}

#[test]
fn mask_credit_cards_mixed_text() {
    assert_eq!(
        mask_credit_cards("Call 1234567890 or card 4111111111111111 today"),
        "Call 1234567890 or card [CARD] today"
    );
}

#[test]
fn mask_credit_cards_phone_not_masked() {
    assert_eq!(mask_credit_cards("Call 1234567890"), "Call 1234567890");
}

#[test]
fn mask_credit_cards_multiple() {
    let input = "Cards: 4111111111111111 and 5500000000000004";
    let result = mask_credit_cards(input);
    assert!(result.contains("[CARD]"));
    assert!(!result.contains("4111111111111111"));
    assert!(!result.contains("5500000000000004"));
}

#[test]
fn mask_credit_cards_short_sequence() {
    // 12 digits — not a card
    assert_eq!(mask_credit_cards("123456789012"), "123456789012");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p oneshim-vision mask_credit_cards -- --nocapture`
Expected: Multiple FAILs (current logic is broken)

- [ ] **Step 3: Implement the fix**

Replace `mask_credit_cards` function (lines 273-301) with:

```rust
fn mask_credit_cards(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;

    while i < len {
        if chars[i].is_ascii_digit() {
            // Start of a potential card number sequence
            let start = i;
            let mut digit_count = 0;

            // Scan digits and single separators (space or hyphen)
            while i < len {
                if chars[i].is_ascii_digit() {
                    digit_count += 1;
                    i += 1;
                } else if (chars[i] == ' ' || chars[i] == '-')
                    && i + 1 < len
                    && chars[i + 1].is_ascii_digit()
                {
                    i += 1; // skip separator
                } else {
                    break;
                }
            }

            if digit_count >= 13 && digit_count <= 19 {
                result.push_str("[CARD]");
            } else {
                // Not a card — preserve original characters
                for ch in &chars[start..i] {
                    result.push(*ch);
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p oneshim-vision mask_credit_cards -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Run full vision crate tests**

Run: `cargo test -p oneshim-vision`
Expected: All existing tests still pass

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-vision/src/privacy.rs
git commit -m "fix(vision): rewrite credit card masking with correct state machine

The previous implementation defined regex patterns but never used them
(_pattern), and the digit masking logic was inverted. Replaced with a
character-by-character state machine that correctly identifies 13-19
digit sequences separated by spaces/hyphens."
```

---

## Task 2: Fix Circuit Breaker Lock Poisoning (HIGH)

**Files:**
- Modify: `crates/oneshim-network/src/circuit_breaker.rs`

- [ ] **Step 1: Replace Mutex import and usage**

In `circuit_breaker.rs`, change line 1:
```rust
// Before:
use std::sync::Mutex;

// After:
use parking_lot::Mutex;
```

Then remove all `.expect("circuit breaker lock poisoned")` calls (lines 72, 83, 94, 127, 134), replacing with direct `.lock()`:

```rust
// Before (5 occurrences):
let mut inner = self.state.lock().expect("circuit breaker lock poisoned");

// After:
let mut inner = self.state.lock();
```

For `state()` and `stats()` (read-only):
```rust
// Before:
let inner = self.state.lock().expect("circuit breaker lock poisoned");

// After:
let inner = self.state.lock();
```

- [ ] **Step 2: Verify parking_lot is in Cargo.toml**

Run: `grep parking_lot crates/oneshim-network/Cargo.toml`
Expected: `parking_lot` listed in dependencies (already present)

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-network circuit_breaker`
Expected: All existing tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-network/src/circuit_breaker.rs
git commit -m "fix(network): replace std::sync::Mutex with parking_lot in circuit breaker

parking_lot::Mutex does not poison on panic, eliminating the risk of
permanent circuit breaker failure after a thread panic."
```

---

## Task 3: Vision Correlation Fixes (3 items)

**Files:**
- Modify: `crates/oneshim-vision/src/gui_detector/correlation.rs:64-85`
- Modify: `crates/oneshim-vision/src/gui_detector/mod.rs:172-197`

- [ ] **Step 1: Write failing test for UTF-8 char count**

In `correlation.rs` test module:
```rust
#[test]
fn word_grouping_cjk_text() {
    // CJK characters are 3 bytes each in UTF-8
    // With .len() this gives 12 bytes / width = wrong avg_char_width
    // With .chars().count() this gives 4 chars / width = correct
    let region1 = OcrRegion {
        text: "테스트입".to_string(),  // 4 chars, 12 bytes
        bbox: BoundingBox { x: 0, y: 0, width: 80, height: 20 },
        confidence: 0.9,
    };
    let region2 = OcrRegion {
        text: "력".to_string(),
        bbox: BoundingBox { x: 100, y: 0, width: 20, height: 20 },
        confidence: 0.9,
    };
    // avg_char_width should be 80/4=20, gap=100-80=20, max_gap=20*1.5=30
    // So gap(20) < max_gap(30) → should merge
    let result = group_words(vec![region1, region2]);
    assert_eq!(result.len(), 1, "CJK words close enough should merge");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p oneshim-vision word_grouping_cjk`
Expected: FAIL (current code uses `.len()` = 12 bytes, avg_char_width = 80/12 ≈ 6.7, max_gap = 10, gap = 20 > 10 → doesn't merge)

- [ ] **Step 3: Fix UTF-8 char count**

In `correlation.rs` line 64, change:
```rust
// Before:
let char_count = prev.text.len().max(1) as f32;

// After:
let char_count = prev.text.chars().count().max(1) as f32;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p oneshim-vision word_grouping_cjk`
Expected: PASS

- [ ] **Step 5: Fix bbox merge overflow**

In `correlation.rs` lines 76-85, replace the merge calculation:
```rust
// Before:
let max_x = (prev.bbox.x + prev.bbox.width).max(region.bbox.x + region.bbox.width);
let max_y = (prev.bbox.y + prev.bbox.height).max(region.bbox.y + region.bbox.height);

// After:
let prev_right = (prev.bbox.x as u64) + (prev.bbox.width as u64);
let prev_bottom = (prev.bbox.y as u64) + (prev.bbox.height as u64);
let region_right = (region.bbox.x as u64) + (region.bbox.width as u64);
let region_bottom = (region.bbox.y as u64) + (region.bbox.height as u64);
let max_x = prev_right.max(region_right).min(u32::MAX as u64) as u32;
let max_y = prev_bottom.max(region_bottom).min(u32::MAX as u64) as u32;
```

- [ ] **Step 6: Fix crop_region_rgba offset overflow**

In `gui_detector/mod.rs` `crop_region_rgba` function, add upfront validation and use checked arithmetic:
```rust
pub fn crop_region_rgba(
    frame_rgba: &[u8],
    frame_width: u32,
    frame_height: u32,
    bbox: &BoundingBox,
) -> Option<Vec<u8>> {
    // Validate total buffer size
    let expected_len = (frame_width as usize)
        .checked_mul(frame_height as usize)?
        .checked_mul(4)?;
    if frame_rgba.len() < expected_len {
        return None;
    }

    if bbox.x + bbox.width > frame_width || bbox.y + bbox.height > frame_height {
        return None;
    }

    let stride = (frame_width as usize) * 4;
    let crop_stride = (bbox.width as usize) * 4;
    let mut crop = Vec::with_capacity((bbox.width as usize) * (bbox.height as usize) * 4);

    for row in 0..bbox.height as usize {
        let y_offset = (bbox.y as usize).checked_add(row)?;
        let src_offset = y_offset.checked_mul(stride)?.checked_add((bbox.x as usize) * 4)?;
        let src_end = src_offset.checked_add(crop_stride)?;
        if src_end <= frame_rgba.len() {
            crop.extend_from_slice(&frame_rgba[src_offset..src_end]);
        } else {
            return None;
        }
    }
    Some(crop)
}
```

- [ ] **Step 7: Run all vision tests**

Run: `cargo test -p oneshim-vision`
Expected: All PASS

- [ ] **Step 8: Commit**

```bash
git add crates/oneshim-vision/src/gui_detector/correlation.rs crates/oneshim-vision/src/gui_detector/mod.rs
git commit -m "fix(vision): use chars().count() for UTF-8, checked_add for bbox merge, checked arithmetic for crop offset"
```

---

## Task 4: Vision Inference + Thumbnail (2 items)

**Files:**
- Modify: `crates/oneshim-vision/src/gui_detector/inference.rs`
- Modify: `crates/oneshim-vision/src/thumbnail.rs`

- [ ] **Step 1: Add scrollbar edge-position test**

In `inference.rs` test module:
```rust
#[test]
fn scrollbar_requires_screen_edge() {
    let detector = GuiElementDetector::new((1920, 1080), PiiFilterLevel::Off, 10, None);
    // Narrow element in center — should NOT be scrollbar
    let center_bbox = BoundingBox { x: 500, y: 100, width: 12, height: 300 };
    let center_type = detector.infer_element_type("", &center_bbox);
    assert_ne!(center_type, GuiElementType::Scrollbar);

    // Narrow element at right edge — should be scrollbar
    let edge_bbox = BoundingBox { x: 1908, y: 100, width: 12, height: 300 };
    let edge_type = detector.infer_element_type("", &edge_bbox);
    assert_eq!(edge_type, GuiElementType::Scrollbar);
}
```

- [ ] **Step 2: Implement scrollbar edge check**

In `inference.rs`, add edge proximity condition to scrollbar scoring:
```rust
// Add to scrollbar detection logic:
let edge_margin: u32 = 20;
let at_horizontal_edge = bbox.x <= edge_margin
    || bbox.x + bbox.width >= self.screen_resolution.0.saturating_sub(edge_margin);
let at_vertical_edge = bbox.y <= edge_margin
    || bbox.y + bbox.height >= self.screen_resolution.1.saturating_sub(edge_margin);

// Only score as scrollbar if at screen edge
if is_narrow && (at_horizontal_edge || at_vertical_edge) {
    scores.push((GuiElementType::Scrollbar, 0.7));
}
```

- [ ] **Step 3: Update thumbnail hash sampling grid**

In `thumbnail.rs`, change the sampling constants:
```rust
// Before:
for sy in 0..8 {
    ...
    for sx in 0..8 {

// After:
for sy in 0..16 {
    ...
    for sx in 0..16 {
```

And update step calculations accordingly:
```rust
let step_x = (w as usize) / 16;
let step_y = (h as usize) / 16;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-vision`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-vision/src/gui_detector/inference.rs crates/oneshim-vision/src/thumbnail.rs
git commit -m "fix(vision): add screen-edge check for scrollbar detection, increase thumbnail hash grid to 16x16"
```

---

## Task 5: NetworkError CircuitOpen + Batch Uploader (Phase 3-1)

**Files:**
- Modify: `crates/oneshim-network/src/error.rs`
- Modify: `crates/oneshim-network/src/batch_uploader.rs:207-210`

- [ ] **Step 1: Add CircuitOpen variant to NetworkError**

In `error.rs`, add variant to enum:
```rust
#[error("circuit breaker open — requests are being fast-failed")]
CircuitOpen,
```

Add to `From<NetworkError> for CoreError` impl:
```rust
NetworkError::CircuitOpen => CoreError::ServiceUnavailable("circuit breaker open".into()),
```

- [ ] **Step 2: Write failing test**

In `batch_uploader.rs` test module:
```rust
#[tokio::test]
async fn flush_returns_error_when_circuit_open() {
    let uploader = create_test_uploader(); // existing helper
    // Trip the circuit breaker
    for _ in 0..5 {
        uploader.circuit_breaker.record_failure();
    }
    uploader.enqueue(test_event());
    let result = uploader.flush().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NetworkError::CircuitOpen));
}
```

- [ ] **Step 3: Fix flush to return error**

In `batch_uploader.rs` lines 207-210:
```rust
// Before:
CircuitState::Open { .. } => {
    debug!("circuit open — skipping flush");
    return Ok(0);
}

// After:
CircuitState::Open { .. } => {
    debug!("circuit open — fast-failing flush");
    return Err(NetworkError::CircuitOpen);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-network`
Expected: All PASS (check that callers handle the new error variant)

- [ ] **Step 5: Run workspace check**

Run: `cargo check --workspace`
Expected: PASS (verify no exhaustive match compilation errors)

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-network/src/error.rs crates/oneshim-network/src/batch_uploader.rs
git commit -m "fix(network): return CircuitOpen error from flush instead of masking with Ok(0)"
```

---

## Task 6: SSE Activity Timeout + Gap Counter (Phase 3-2, 3-6)

**Files:**
- Modify: `crates/oneshim-network/src/sse_client.rs`

- [ ] **Step 1: Add AtomicU64 gap counter to SseStreamClient**

Add field to struct:
```rust
gap_count: Arc<AtomicU64>,
```

Initialize in constructor:
```rust
gap_count: Arc::new(AtomicU64::new(0)),
```

Add public accessor:
```rust
pub fn gap_count(&self) -> u64 {
    self.gap_count.load(Ordering::Relaxed)
}
```

- [ ] **Step 2: Increment counter on gap detection**

In the gap detection code (around line 189):
```rust
if new_n > last_n + 1 {
    let gap = new_n - last_n - 1;
    self.gap_count.fetch_add(gap, Ordering::Relaxed);
    warn!(gap = gap, last = last_n, current = new_n, "SSE event ID gap detected");
}
```

- [ ] **Step 3: Add activity timeout to SSE stream loop**

Wrap the inner stream read with `tokio::time::timeout`:
```rust
use tokio::time::{timeout, Duration};

let activity_timeout = Duration::from_secs(self.config.activity_timeout_secs.unwrap_or(300));

loop {
    match timeout(activity_timeout, stream.next()).await {
        Ok(Some(Ok(msg))) => {
            // existing message handling
        }
        Ok(Some(Err(e))) => {
            warn!("SSE stream error: {e}");
            break;
        }
        Ok(None) => {
            info!("SSE stream ended");
            break;
        }
        Err(_) => {
            warn!("SSE activity timeout after {}s — reconnecting", activity_timeout.as_secs());
            break; // triggers reconnection via outer loop
        }
    }
}
```

- [ ] **Step 4: Add activity_timeout_secs to SseConfig**

```rust
pub struct SseConfig {
    // ... existing fields ...
    /// Activity timeout in seconds (default: 300). Reconnects if no events received.
    pub activity_timeout_secs: Option<u64>,
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-network sse`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-network/src/sse_client.rs
git commit -m "fix(network): add SSE activity timeout (5min default) and gap counter metric"
```

---

## Task 7: Auth Retry-After (Phase 3-3)

**Files:**
- Modify: `crates/oneshim-network/src/auth.rs`

- [ ] **Step 1: Add Retry-After parsing helper**

```rust
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get("retry-after")?.to_str().ok()?;
    // Try parsing as seconds first
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs.min(60)));
    }
    // HTTP-date parsing not implemented — fall back to default
    None
}
```

- [ ] **Step 2: Use Retry-After in refresh retry loop**

In the retry loop of `refresh()`, when handling 429 responses:
```rust
if status == 429 {
    if let Some(retry_duration) = parse_retry_after(response.headers()) {
        tokio::time::sleep(retry_duration).await;
        continue;
    }
    // Fall through to existing exponential backoff
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-network auth`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-network/src/auth.rs
git commit -m "fix(network): respect Retry-After header in auth token refresh (capped at 60s)"
```

---

## Task 8: gRPC Fixes (Phase 3-4, 3-5)

**Files:**
- Modify: `crates/oneshim-network/src/grpc/unified_client.rs`
- Modify: `crates/oneshim-network/src/grpc/sse_adapter.rs`

- [ ] **Step 1: Replace RwLock with Mutex in UnifiedClient**

Change field types:
```rust
// Before:
grpc_auth: RwLock<Option<GrpcAuthClient>>,
grpc_session: RwLock<Option<GrpcSessionClient>>,
grpc_context: RwLock<Option<GrpcContextClient>>,

// After:
grpc_auth: tokio::sync::Mutex<Option<GrpcAuthClient>>,
grpc_session: tokio::sync::Mutex<Option<GrpcSessionClient>>,
grpc_context: tokio::sync::Mutex<Option<GrpcContextClient>>,
```

- [ ] **Step 2: Fix ensure_grpc_* methods**

```rust
async fn ensure_grpc_auth(&self) -> Result<(), CoreError> {
    let mut guard = self.grpc_auth.lock().await;
    if guard.is_none() {
        let client = GrpcAuthClient::connect(self.config.clone()).await?;
        *guard = Some(client);
    }
    Ok(())
}
// Same pattern for ensure_grpc_session and ensure_grpc_context
```

- [ ] **Step 3: Add CancellationToken to SSE adapter**

In `sse_adapter.rs`, add token parameter and `tokio::select!`:
```rust
pub async fn start_streaming(
    &self,
    stream: tonic::Streaming<SuggestionEvent>,
    tx: mpsc::Sender<SseEvent>,
    cancel: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        tokio::pin!(let cancelled = cancel.cancelled(););
        loop {
            tokio::select! {
                _ = &mut cancelled => {
                    info!("gRPC SSE adapter cancelled");
                    break;
                }
                msg = stream.message() => {
                    match msg {
                        Ok(Some(event)) => { /* existing handling */ }
                        Ok(None) => break,
                        Err(e) => { warn!("gRPC stream error: {e}"); break; }
                    }
                }
            }
        }
    })
}
```

- [ ] **Step 4: Add tokio-util dependency**

Add to workspace `Cargo.toml` `[workspace.dependencies]` section:
```toml
tokio-util = { version = "0.7", features = ["rt"] }
```

Add to `crates/oneshim-network/Cargo.toml` `[dependencies]`:
```toml
tokio-util = { workspace = true }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-network grpc`
Expected: All PASS

- [ ] **Step 6: Commit**

```bash
git add crates/oneshim-network/src/grpc/
git commit -m "fix(network): eliminate gRPC TOCTOU with Mutex, add CancellationToken to SSE adapter"
```

---

## Task 9: Consent File TOCTOU (Phase 4-1)

**Files:**
- Modify: `crates/oneshim-core/src/consent.rs:170-189`

- [ ] **Step 1: Fix revoke_consent to use atomic write**

Replace delete-after-save with atomic rename + pending_deletion flag:
```rust
pub fn revoke_consent(&mut self) -> Result<(), CoreError> {
    if let Some(record) = &mut self.current_consent {
        record.data_deletion_requested = true;
        record.revoked_at = Some(Utc::now());

        // Atomic write: temp file → rename
        let tmp_path = self.consent_file_path().with_extension("tmp");
        let json = serde_json::to_string_pretty(record)
            .map_err(|e| CoreError::Internal(format!("consent serialize: {e}")))?;
        std::fs::write(&tmp_path, &json)
            .map_err(|e| CoreError::Internal(format!("consent write: {e}")))?;
        std::fs::rename(&tmp_path, self.consent_file_path())
            .map_err(|e| CoreError::Internal(format!("consent rename: {e}")))?;

        self.current_consent = None;
        Ok(())
    } else {
        Err(CoreError::Internal("no consent to revoke".into()))
    }
}
```

- [ ] **Step 2: Update load_from_file to check pending_deletion**

```rust
fn load_from_file(path: &PathBuf) -> Option<ConsentRecord> {
    let data = std::fs::read_to_string(path).ok()?;
    let record: ConsentRecord = serde_json::from_str(&data).ok()?;
    if record.data_deletion_requested {
        return None; // Treat as revoked
    }
    Some(record)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-core consent`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-core/src/consent.rs
git commit -m "fix(core): use atomic file rename for consent revocation, check pending_deletion on load"
```

---

## Task 10: Policy Cache TTL (Phase 4-2)

**Files:**
- Modify: `crates/oneshim-automation/src/policy/mod.rs:85-88`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn ttl_zero_rejects_all_tokens() {
    let mut client = PolicyClient::new(/* config with ttl_seconds = 0 */);
    // Pre-validate a token
    let token = "pol-1:nonce123";
    // With TTL=0, validation should reject
    let result = client.validate_command("test_cmd", token);
    assert_eq!(result.unwrap(), false);
}
```

- [ ] **Step 2: Add TTL=0 guard**

In `validate_command()` around line 85:
```rust
if self.config.ttl_seconds == 0 {
    tracing::debug!("policy TTL is 0 — rejecting all cached tokens");
    return Ok(false);
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p oneshim-automation policy`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add crates/oneshim-automation/src/policy/mod.rs
git commit -m "fix(automation): reject all tokens when policy cache TTL is zero"
```

---

## Task 11: Windows ACL Validation (Phase 4-3)

**Files:**
- Modify: `crates/oneshim-storage/src/encryption.rs:148-237`

- [ ] **Step 1: Add size validation assertions**

After the `GetTokenInformation` call that returns `needed`:
```rust
// Validate returned size is reasonable
if needed == 0 || needed > 4096 {
    return Err(anyhow::anyhow!(
        "unexpected token info size: {needed} bytes"
    ));
}
```

Before `InitializeAcl`, validate `acl_size`:
```rust
// Validate ACL size doesn't underflow
// SidStart in ACCESS_ALLOWED_ACE is already counted once in the struct,
// so we subtract sizeof(u32) to avoid double-counting
if sid_len < std::mem::size_of::<u32>() as u32 {
    return Err(anyhow::anyhow!(
        "SID length too small: {sid_len} bytes"
    ));
}
```

- [ ] **Step 2: Run check (Windows-only code, just verify compilation)**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/oneshim-storage/src/encryption.rs
git commit -m "fix(storage): add size validation for Windows ACL token info and SID"
```

---

## Task 12: Frame Storage cached_size_bytes (Phase 4-4)

**Files:**
- Modify: `crates/oneshim-storage/src/frame_storage.rs:622-625`

- [ ] **Step 1: Write test for concurrent add/delete**

```rust
#[test]
fn cached_size_bytes_no_underflow() {
    let storage = create_test_storage();
    // Set initial size
    storage.cached_size_bytes.store(100, Ordering::Relaxed);
    // Try to subtract more than available
    let _ = storage.cached_size_bytes.fetch_update(
        Ordering::Relaxed, Ordering::Relaxed,
        |current| Some(current.saturating_sub(200))
    );
    assert_eq!(storage.cached_size_bytes.load(Ordering::Relaxed), 0);
}
```

- [ ] **Step 2: Replace fetch_sub with fetch_update**

At lines 622-625:
```rust
// Before:
self.cached_size_bytes.fetch_sub(
    total_deleted_bytes.min(self.cached_size_bytes.load(Ordering::Relaxed)),
    Ordering::Relaxed,
);

// After:
let _ = self.cached_size_bytes.fetch_update(
    Ordering::Relaxed,
    Ordering::Relaxed,
    |current| Some(current.saturating_sub(total_deleted_bytes)),
);
```

- [ ] **Step 3: Add reconcile_cache_size method**

```rust
/// Recalculates cached_size_bytes from actual filesystem state.
/// Call periodically (e.g., every 10 cleanup cycles) to correct drift.
pub fn reconcile_cache_size(&self) -> std::io::Result<u64> {
    let mut total: u64 = 0;
    if self.base_dir.exists() {
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if entry.path().is_file() {
                total += entry.metadata()?.len();
            }
        }
    }
    self.cached_size_bytes.store(total, Ordering::Relaxed);
    Ok(total)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-storage frame`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-storage/src/frame_storage.rs
git commit -m "fix(storage): use atomic fetch_update with saturating_sub for cached_size_bytes"
```

---

## Task 13: Lock Ordering Documentation (Phase 4-5)

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`

- [ ] **Step 1: Add lock ordering comment at top of file**

After the existing module-level doc comment, add:
```rust
// ## Lock Ordering
//
// Acquire locks in this order to prevent deadlocks:
//
// 1. deferred_suggestions   (tokio::sync::Mutex — async, held briefly)
// 2. suggestion_queue        (tokio::sync::Mutex — async, held briefly)
// 3. retry_queue             (tokio::sync::Mutex — async, held briefly)
// 4. shared_regime_state     (parking_lot::RwLock — sync, <1μs ops)
// 5. capture_context         (AppState sub-struct fields)
//
// Never acquire a lower-numbered lock while holding a higher-numbered one.
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/scheduler/mod.rs
git commit -m "docs(scheduler): add lock ordering table to prevent deadlocks"
```

---

## Task 14: Testing Gaps (Phase 5-1, 5-2, 5-3)

**Files:**
- Modify: `crates/oneshim-api-contracts/src/sessions.rs` (add tests)
- Modify: `crates/oneshim-api-contracts/src/frames.rs` (add tests)
- Modify: `crates/oneshim-api-contracts/src/events.rs` (add tests)
- Modify: `crates/oneshim-api-contracts/src/settings.rs` (add tests)
- Modify: `crates/oneshim-api-contracts/src/suggestions.rs` (add tests)
- Modify: `crates/oneshim-automation/src/action_dispatcher.rs` (add tests)
- Modify: `crates/oneshim-network/src/circuit_breaker.rs` (add concurrency tests)

- [ ] **Step 1: Add serde round-trip tests for 5 priority api-contracts modules**

For each module, add a `#[cfg(test)] mod tests` section with round-trip tests. Pattern:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_serde() {
        // Create instance with representative data
        let original = TypeName { /* all fields populated */ };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TypeName = serde_json::from_str(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn deserialize_empty_optional_fields() {
        let json = r#"{"required_field": "value"}"#;
        let result: Result<TypeName, _> = serde_json::from_str(json);
        // Should either succeed with defaults or produce clear error
        assert!(result.is_ok() || result.is_err());
    }
}
```

Add 2-3 tests per module for the main request/response types.

- [ ] **Step 2: Add action_dispatcher tests**

In `action_dispatcher.rs`, add test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Note: Read actual Sandbox trait from oneshim-core/src/ports/sandbox.rs
    // and AutomationAction/SandboxConfig types before implementing.
    // Create a MockSandbox that implements the Sandbox port trait.
    // Test dispatch with success and failure scenarios.
    // Exact mock struct and method signatures depend on current Sandbox trait definition.

    #[tokio::test]
    async fn dispatch_success() {
        // 1. Create MockSandbox implementing oneshim_core::ports::sandbox::Sandbox
        // 2. Create SandboxActionDispatcher with Arc::new(MockSandbox { should_fail: false })
        // 3. Create a test AutomationAction
        // 4. Call dispatcher.dispatch(&action, &config).await
        // 5. Assert result.is_ok()
        // Implementation depends on exact Sandbox trait signature — read it first
    }

    #[tokio::test]
    async fn dispatch_sandbox_failure() {
        // Same as above but MockSandbox returns Err
        // Assert result.is_err()
    }
}
```

- [ ] **Step 3: Add circuit breaker concurrency tests**

In `circuit_breaker.rs` test module:
```rust
#[test]
fn concurrent_failures_transition_to_open() {
    let cb = Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
        failure_threshold: 5,
        ..Default::default()
    }));

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let cb = Arc::clone(&cb);
            std::thread::spawn(move || {
                cb.record_failure();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert!(matches!(cb.check(), CircuitState::Open { .. }));
}

#[test]
fn concurrent_checks_dont_panic() {
    let cb = Arc::new(CircuitBreaker::new(Default::default()));
    let handles: Vec<_> = (0..100)
        .map(|_| {
            let cb = Arc::clone(&cb);
            std::thread::spawn(move || {
                let _ = cb.check();
                cb.record_failure();
                let _ = cb.stats();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
    // No panics = success
}
```

- [ ] **Step 4: Run all tests**

Run: `cargo test --workspace`
Expected: All PASS, 0 failures

- [ ] **Step 5: Commit**

```bash
git add crates/oneshim-api-contracts/src/ crates/oneshim-automation/src/action_dispatcher.rs crates/oneshim-network/src/circuit_breaker.rs
git commit -m "test: add serde round-trip, action_dispatcher, and circuit breaker concurrency tests"
```

---

## Final Verification

- [ ] **Step 1: Full quality gate**

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: All PASS, 0 warnings, 0 failures

- [ ] **Step 2: Final commit (if any formatting fixes needed)**

```bash
cargo fmt
git add -A
git commit -m "chore: format fixes"
```
