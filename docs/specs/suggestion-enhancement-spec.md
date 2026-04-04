# Suggestion Enhancement Spec

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `oneshim-suggestion`, `oneshim-core`, `src-tauri`

## 1. Problem Statement

The current suggestion system receives, queues, displays, and collects feedback — but stops there. Three critical gaps prevent it from being a mature pipeline:

| Gap | Current State | Impact |
|-----|--------------|--------|
| **No deduplication** | Same content from server + local LLM appear as separate items | User sees duplicate suggestions, erodes trust |
| **No expiry enforcement** | `remove_expired()` exists but is never called in runtime | Stale suggestions sit in queue indefinitely |
| **No feedback learning** | Feedback sent to server but ignored locally | Repeatedly rejected suggestion patterns keep appearing |

## 2. Goals

1. **Dedup**: Prevent duplicate suggestions from appearing in the queue (cross-source: server vs local LLM)
2. **Expiry**: Automatically purge expired suggestions from the queue on a periodic basis
3. **Feedback Learning**: Use local feedback history to adjust future suggestion relevance

### Non-Goals

- Server-side feedback processing (out of scope — server owns its own model)
- ML-based suggestion ranking (future work — task #5 Coaching ML)
- New suggestion types or sources
- Web API endpoint expansion (separate task)
- Schema migration changes (reuse existing columns)

## 3. Design

### 3.1 Deduplication

**Strategy**: Content fingerprint on ingestion. When a new suggestion arrives (SSE or local LLM), compute a fingerprint and check against the queue.

**Fingerprint formula**:
```
fingerprint = DefaultHasher::hash("{suggestion_type}:{normalized_content}")
```

Where `normalized_content` = lowercase, trimmed, collapsed whitespace, first 200 chars.

**Why `std::hash::DefaultHasher`**: Zero new dependencies. SipHash-based (Rust's default hasher) is fast and collision-resistant at queue sizes <= 50. blake3 is not in the workspace dependency tree and would be overkill for this use case.

**Dedup scope**: In-memory queue only. This is intentional:
- The `suggestions` table uses `INSERT OR REPLACE` on `suggestion_id` — different IDs with same content are valid historical records
- Local LLM suggestions (from intelligence loop) save directly to storage, bypassing the queue. They are NOT dedup-checked against the queue because they serve a different purpose (persistent history vs real-time display)
- The web dashboard queries storage independently and may show content duplicates from different sources. This is acceptable — the dashboard is a historical view, not a real-time queue
- If cross-source dedup at the storage level becomes needed, it belongs in a separate task (storage-layer fingerprint column)

**Implementation**:
- Add `fingerprints: HashSet<u64>` field to `SuggestionQueue`
- On `push()`: compute fingerprint → if exists, reject (return `false`) with `debug!` log
- On `remove_by_id()` / `pop()` / `clear()` / `remove_expired()`: remove corresponding fingerprint
- Fingerprint is NOT persisted — rebuilds naturally as queue populates

**Edge cases**:
- Same content, different priority → first-in wins (dedup rejects the duplicate)
- Same content, different source (server vs local) → first-in wins. This is correct because the user shouldn't see both.
- After eviction (queue full), fingerprint is removed → same content can re-enter later. This is acceptable because the queue state has changed.
- Deferred suggestion stays in queue → its fingerprint remains → same content arriving again is rejected (correct: user already has this suggestion pending)

### 3.2 Expiry Enforcement

**Strategy**: Opportunistic expiry on each incoming suggestion. No separate periodic tick needed.

**Implementation**:
- In `SuggestionReceiver::handle_suggestion()`: call `queue.remove_expired()` before `push()` within the same lock acquisition
- This is cheap: O(n) scan where n <= 50 (queue max size)
- Log count of removed items at `debug!` level when count > 0

**Why NOT a periodic tick**: The scheduler loop uses `tokio::select!` with `receiver.run()` (blocking SSE) and `shutdown_rx`. Adding a third `interval.tick()` branch would add lock contention without meaningful benefit — since expiry is already called on every incoming suggestion, the queue stays clean as long as SSE events arrive. If no suggestions arrive for hours, there's no user impact from stale items since nobody is looking at them.

**Fallback**: If the SSE stream is idle for extended periods, expired suggestions remain in queue but are invisible to the user (no IPC queries trigger). On the next suggestion arrival, they get cleaned up. This is acceptable.

### 3.3 Feedback Learning Loop

**Strategy**: Local relevance penalty/boost based on feedback patterns. No ML — pure rule-based scoring.

**Concept**: Track per-`(suggestion_type, source)` pair feedback ratios. When the rejection ratio for a type+source exceeds a threshold, apply a relevance penalty to future suggestions of that type+source.

**Prerequisite**: Add `Hash` derive to `SuggestionType` and `SuggestionSource` in `oneshim-core/src/models/suggestion.rs`. Both enums already derive `Eq` — adding `Hash` is safe and required for `HashMap` key usage.

**Data structure** — `FeedbackScorer` (new struct in `oneshim-suggestion`):
```rust
pub struct FeedbackScorer {
    // key: (SuggestionType, SuggestionSource) → FeedbackTally
    tallies: HashMap<(SuggestionType, SuggestionSource), FeedbackTally>,
}

struct FeedbackTally {
    accepted: u32,
    rejected: u32,
    deferred: u32,
    last_updated: DateTime<Utc>,
}
```

**Thread safety**: `FeedbackScorer` is wrapped in `Arc<Mutex<FeedbackScorer>>` at the DI level. The entire scorer is locked for the duration of `score()` and `record()` calls. This is acceptable because both operations are O(1) HashMap lookups with no I/O.

**Scoring logic**:
```
total = accepted + rejected + deferred
if total < 5 { return 0.0 }  // insufficient data
rejection_ratio = rejected / total
boost = if rejection_ratio > 0.7 { -0.3 }       // heavily rejected → big penalty
        else if rejection_ratio > 0.5 { -0.15 }  // often rejected → small penalty
        else if acceptance_ratio > 0.7 { +0.1 }   // heavily accepted → small boost
        else { 0.0 }
```

**Application point**: In `SuggestionReceiver::handle_suggestion()`, before `queue.push()`:
1. Compute `boost = scorer.score(suggestion.suggestion_type, suggestion.source)`
2. `suggestion.relevance_score = (suggestion.relevance_score + boost).clamp(0.0, 1.0)`
3. If adjusted `relevance_score < 0.2`, skip queueing entirely (too irrelevant based on history)

**Recording feedback**: In `submit_suggestion_feedback()` IPC command, after sending to server:
1. `scorer.record(suggestion.suggestion_type, suggestion.source, feedback_type)`

**Persistence**: `FeedbackScorer` tallies are volatile (in-memory only). Rationale:
- History resets on app restart → fresh start, no stale bias
- Tallies accumulate within a session (typically 8+ hours of work)
- Server-side has the authoritative long-term learning model
- Avoids schema migration and storage complexity

**Decay**: Optional. If the session runs very long (24h+), old tallies could bias. Simple approach: reset tallies if `last_updated` is >12h old. Check on each `record()` call.

### 3.4 Integration Points

```
SuggestionReceiver::handle_suggestion(suggestion)
  │
  ├─ 1. queue.remove_expired()           [Expiry: opportunistic]
  ├─ 2. queue.is_duplicate(&suggestion)   [Dedup: fingerprint check]
  │     └─ if duplicate → return (skip)
  ├─ 3. scorer.adjust(&mut suggestion)    [Feedback: relevance adjust]
  │     └─ if relevance < 0.2 → return (suppressed)
  └─ 4. queue.push(suggestion)            [Existing: priority insert]
        └─ if accepted → notify

submit_suggestion_feedback(id, action)
  │
  ├─ 1. feedback_sender.{accept|reject|defer}()  [Existing: server send]
  ├─ 2. queue.remove_by_id(id) + history.add()   [Existing: local state]
  └─ 3. scorer.record(type, source, feedback)     [NEW: tally update]
```

## 4. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `crates/oneshim-core/src/models/suggestion.rs` | Add `Hash` derive to `SuggestionType`, `SuggestionSource` | +2 |
| `crates/oneshim-suggestion/src/queue.rs` | Add fingerprint dedup to `SuggestionQueue` | +40 |
| `crates/oneshim-suggestion/src/scorer.rs` | **NEW** — `FeedbackScorer` struct | +120 |
| `crates/oneshim-suggestion/src/receiver.rs` | Inject `FeedbackScorer`, call dedup+adjust+expiry | +25 |
| `crates/oneshim-suggestion/src/lib.rs` | Export `scorer` module | +1 |
| `src-tauri/src/commands/suggestions.rs` | Call `scorer.record()` on feedback | +10 |
| `src-tauri/src/suggestion_manager.rs` | Hold `Arc<Mutex<FeedbackScorer>>` | +8 |
| `src-tauri/src/main.rs` | Wire `FeedbackScorer` into DI | +5 |

**Estimated total**: ~210 lines of new/modified code + ~150 lines of tests

## 5. Test Strategy

| Test | Location | Type |
|------|----------|------|
| Fingerprint dedup: same content rejected | `queue.rs` | unit |
| Fingerprint dedup: different content accepted | `queue.rs` | unit |
| Fingerprint cleanup on remove/pop/clear | `queue.rs` | unit |
| Expiry removes stale items | `queue.rs` | unit (existing, verify) |
| Scorer: rejection ratio penalty applied | `scorer.rs` | unit |
| Scorer: acceptance ratio boost applied | `scorer.rs` | unit |
| Scorer: insufficient data returns 0 | `scorer.rs` | unit |
| Scorer: decay resets old tallies | `scorer.rs` | unit |
| Receiver: duplicate suppressed | `receiver.rs` | unit |
| Receiver: low-relevance suppressed | `receiver.rs` | unit |
| IPC: feedback records to scorer | `commands/suggestions.rs` | integration |

## 6. Risks & Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Fingerprint collision (different content, same hash) | Very Low | u64 SipHash space = 2^64. Birthday paradox at 50 items: ~6.8e-16 probability — negligible |
| Feedback bias (early rejections suppress permanently) | Low | 12h decay + minimum 5 samples before applying penalty |
| Lock contention (scorer + queue both locked) | Low | Scorer locked separately, not nested. Short critical sections. |
| Expired items during SSE idle | Very Low | No user impact during idle; cleaned on next suggestion arrival |

## 7. Config

No new config fields required. Uses existing:
- `analysis.max_suggestions` — queue max size (dedup fingerprint set bounded by this)
- Expiry interval hardcoded at 300s (5 min) — not worth exposing as config

Constants (in `scorer.rs`):
- `MIN_SAMPLES: u32 = 5`
- `HIGH_REJECTION_THRESHOLD: f64 = 0.7`
- `MEDIUM_REJECTION_THRESHOLD: f64 = 0.5`
- `HIGH_ACCEPTANCE_THRESHOLD: f64 = 0.7`
- `HEAVY_PENALTY: f64 = -0.3`
- `LIGHT_PENALTY: f64 = -0.15`
- `ACCEPTANCE_BOOST: f64 = 0.1`
- `SUPPRESSION_THRESHOLD: f64 = 0.2`
- `TALLY_DECAY_HOURS: i64 = 12`
