[English](./09-testing.md) | [한국어](./09-testing.ko.md)

# 9. Testing Strategy

[← Edge Vision](./08-edge-vision.md) | [Build/Deploy →](./10-build-deploy.md)

---

## Full Rust Testing (#[test], #[tokio::test])

```
oneshim-core/        → Unit tests (model serialization/deserialization, config validation)
oneshim-monitor/     → Unit + integration (real sysinfo calls, platform-specific #[cfg(test)])
oneshim-vision/      → Unit + integration (delta encoding, encoder, trigger, timeline)
oneshim-network/     → Unit + integration (HTTP mock with mockito, SSE mock)
oneshim-storage/     → Unit + integration (in-memory SQLite)
oneshim-suggestion/  → Unit (queue, parsing, presenter)
oneshim-ui/          → Limited (state logic only)
oneshim-app/         → Integration (full pipeline)
```

## Test Crates

```toml
[workspace.dev-dependencies]
mockito = "1"           # HTTP server mock
tokio-test = "0.4"      # Async test utilities
tempfile = "3"           # Temporary files/DB
wiremock = "0.6"         # Advanced HTTP mock
assert_matches = "1"
```

## Test Examples

### SSE + Suggestion Test

```rust
#[tokio::test]
async fn test_sse_suggestion_parsing() {
    let raw = r#"{"suggestion_id":"sug_001","suggestion_type":"WORK_GUIDANCE","content":"Please commit","priority":"HIGH","confidence_score":0.95,"relevance_score":0.88,"is_actionable":true,"created_at":"2026-01-28T10:00:00Z","expires_at":null}"#;

    let suggestion: Suggestion = serde_json::from_str(raw).unwrap();
    assert_eq!(suggestion.suggestion_id, "sug_001");
    assert!(matches!(suggestion.suggestion_type, SuggestionType::WorkGuidance));
    assert!(suggestion.confidence_score > 0.9);
}

#[tokio::test]
async fn test_suggestion_queue_priority_ordering() {
    let queue = SuggestionQueue::new(50);

    queue.push(make_suggestion(Priority::Low)).await;
    queue.push(make_suggestion(Priority::Critical)).await;
    queue.push(make_suggestion(Priority::High)).await;

    let top = queue.pop().await.unwrap();
    assert!(matches!(top.priority, Priority::Critical));
}
```

### Vision Tests

```rust
#[test]
fn test_delta_encoding_detects_changes() {
    // Two images: only top-left 100×100 region changed
    let prev = DynamicImage::new_rgba8(1920, 1080);
    let mut curr = prev.clone();
    // Change (0,0)-(100,100) region to red
    for y in 0..100 {
        for x in 0..100 {
            curr.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }

    let delta = compute_delta(&prev, &curr);
    assert!(delta.changed_ratio < 0.1);  // ~1% change of total
    assert!(delta.bounds.w <= 112);       // Tile-aligned (16px multiple)
    assert!(delta.bounds.h <= 112);
}

#[test]
fn test_webp_encoding_smaller_than_jpeg() {
    let img = DynamicImage::new_rgba8(480, 270);
    let webp = encode_webp(&img, WebPQuality::Medium);
    let jpeg = encode_jpeg(&img, 75);
    assert!(webp.len() <= jpeg.len());  // WebP is smaller at same quality
}

#[test]
fn test_capture_trigger_throttle() {
    let mut trigger = CaptureTrigger::new(Duration::from_secs(5));
    let event = make_context_event(TriggerType::WindowChange);

    assert!(trigger.should_capture(&event).is_some());  // First: capture
    assert!(trigger.should_capture(&event).is_none());   // Immediate re-call: throttled
}

#[test]
fn test_importance_scoring() {
    let error_event = ContextEvent {
        window_title: "Error: connection refused".into(),
        ..Default::default()
    };
    let normal_event = ContextEvent {
        window_title: "main.rs — VS Code".into(),
        ..Default::default()
    };

    assert!(score_importance(&TriggerType::ErrorDetected, &error_event) > 0.9);
    assert!(score_importance(&TriggerType::WindowChange, &normal_event) < 0.5);
}

#[test]
fn test_adaptive_encoding_respects_max_size() {
    let large_img = DynamicImage::new_rgba8(3840, 2160);  // 4K
    let encoded = encode_adaptive(&large_img, 100_000);   // 100KB limit
    assert!(encoded.data.len() <= 100_000);
}

#[test]
fn test_timeline_retention() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    let mut timeline = Timeline::new(db, tempdir().unwrap().path(), Duration::from_secs(0));

    timeline.insert(make_frame_index(Utc::now() - chrono::Duration::hours(2)));
    timeline.insert(make_frame_index(Utc::now()));

    let deleted = timeline.enforce_retention();
    assert_eq!(deleted, 1);  // 2-hour-old frame deleted
}
```
