[English](./09-testing.md) | [한국어](./09-testing.ko.md)

# 9. 테스트 전략

[← Edge Vision](./08-edge-vision.ko.md) | [빌드/배포 →](./10-build-deploy.ko.md)

---

## 전체 Rust 테스트 (#[test], #[tokio::test])

```
oneshim-core/        → 단위 테스트 (모델 직렬화/역직렬화, 설정 검증)
oneshim-monitor/     → 단위 + 통합 (sysinfo 실제 호출, 플랫폼별 #[cfg(test)])
oneshim-vision/      → 단위 + 통합 (델타 인코딩, 인코더, 트리거, 타임라인)
oneshim-network/     → 단위 + 통합 (mockito로 HTTP mock, SSE mock)
oneshim-storage/     → 단위 + 통합 (인메모리 SQLite)
oneshim-suggestion/  → 단위 (큐, 파싱, 프레젠터)
oneshim-ui/          → 제한적 (상태 로직만 테스트)
oneshim-app/         → 통합 (전체 파이프라인)
```

## 테스트 크레이트

```toml
[workspace.dev-dependencies]
mockito = "1"           # HTTP 서버 mock
tokio-test = "0.4"      # async 테스트 유틸
tempfile = "3"           # 임시 파일/DB
wiremock = "0.6"         # 고급 HTTP mock
assert_matches = "1"
```

## 테스트 예시

### SSE + Suggestion 테스트

```rust
#[tokio::test]
async fn test_sse_suggestion_parsing() {
    let raw = r#"{"suggestion_id":"sug_001","suggestion_type":"WORK_GUIDANCE","content":"커밋하세요","priority":"HIGH","confidence_score":0.95,"relevance_score":0.88,"is_actionable":true,"created_at":"2026-01-28T10:00:00Z","expires_at":null}"#;

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

### Vision 테스트

```rust
#[test]
fn test_delta_encoding_detects_changes() {
    // 두 이미지: 좌상단 100×100만 변경
    let prev = DynamicImage::new_rgba8(1920, 1080);
    let mut curr = prev.clone();
    // curr의 (0,0)-(100,100) 영역을 빨간색으로 변경
    for y in 0..100 {
        for x in 0..100 {
            curr.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }

    let delta = compute_delta(&prev, &curr);
    assert!(delta.changed_ratio < 0.1);  // 전체 대비 ~1% 변경
    assert!(delta.bounds.w <= 112);       // 타일 정렬 (16px 배수)
    assert!(delta.bounds.h <= 112);
}

#[test]
fn test_webp_encoding_smaller_than_jpeg() {
    let img = DynamicImage::new_rgba8(480, 270);
    let webp = encode_webp(&img, WebPQuality::Medium);
    let jpeg = encode_jpeg(&img, 75);
    assert!(webp.len() <= jpeg.len());  // WebP가 동일 품질에서 더 작음
}

#[test]
fn test_capture_trigger_throttle() {
    let mut trigger = CaptureTrigger::new(Duration::from_secs(5));
    let event = make_context_event(TriggerType::WindowChange);

    assert!(trigger.should_capture(&event).is_some());  // 첫 번째: 캡처
    assert!(trigger.should_capture(&event).is_none());   // 즉시 재호출: 쓰로틀
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
    let encoded = encode_adaptive(&large_img, 100_000);   // 100KB 제한
    assert!(encoded.data.len() <= 100_000);
}

#[test]
fn test_timeline_retention() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    let mut timeline = Timeline::new(db, tempdir().unwrap().path(), Duration::from_secs(0));

    timeline.insert(make_frame_index(Utc::now() - chrono::Duration::hours(2)));
    timeline.insert(make_frame_index(Utc::now()));

    let deleted = timeline.enforce_retention();
    assert_eq!(deleted, 1);  // 2시간 전 프레임 삭제
}
```
