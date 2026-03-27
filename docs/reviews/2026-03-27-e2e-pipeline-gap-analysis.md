# E2E Pipeline Gap Analysis

**Date**: 2026-03-27
**Status**: Initial review — ongoing improvement
**Scope**: client-rust full codebase (14 pipelines)

---

## Pipeline Map

### 1. Vision Pipeline (Screen Capture → OCR → Upload)

**Flow**: `ScreenCapture → SmartCaptureTrigger → FrameProcessor → RingBuffer → DeltaEncoder → PrivacyGateway → FrameStorage → BatchSink`

| Gap | Severity | Description |
|-----|----------|-------------|
| Ring buffer backpressure 없음 | **High** | 인코더가 느리면 프레임 무손실 drop, 로그 없음 |
| JPEG 인코딩 동기 블로킹 | Medium | 메인 스레드 stall 가능 |
| 캡처 실패 시 재시도 없음 | Low | xcap 에러 후 다음 tick까지 대기 |

**Key files**: `crates/oneshim-vision/src/capture.rs`, `trigger.rs`, `processor.rs`, `ring_buffer.rs`, `delta.rs`, `encoder.rs`, `privacy.rs`

---

### 2. Monitor Loop (Context Collection → Event Emission)

**Flow**: `ActivityMonitor → InputCollector → WindowLayoutTracker → AccessibilityExtractor → IdleTracker → EventAnalysis → Storage`

| Gap | Severity | Description |
|-----|----------|-------------|
| Accessibility API 실패 시 재시도 없음 | Medium | AX/UIA 연결 끊기면 해당 tick skip |
| Focus highlight thrashing | Low | 빠른 요소 전환 시 오버레이 깜빡임 |

**Key files**: `src-tauri/src/scheduler/loops/monitor.rs`, `crates/oneshim-monitor/src/activity.rs`, `system.rs`, `process.rs`, `input_activity.rs`

---

### 3. System Metrics Pipeline

**Flow**: `SystemMonitor(5s) → SQLite → RealtimeEvent broadcast → NotificationManager → ProcessSnapshot`

| Gap | Severity | Description |
|-----|----------|-------------|
| (양호) | — | 현재 특이 갭 없음 |

**Key files**: `src-tauri/src/scheduler/loops/system.rs`, `crates/oneshim-monitor/src/system.rs`

---

### 4. Network/Upload Pipeline (Batch Upload → Server)

**Flow**: `EventStorage(unsent) → BatchSink.flush() → HttpApiClient.upload_batch() → mark_as_sent → retention enforcement`

| Gap | Severity | Description |
|-----|----------|-------------|
| Circuit breaker 없음 | **High** | 서버 다운 시 계속 업로드 시도 → 리소스 낭비 |
| Upload queue 무한 성장 | **High** | flush 실패 시 unsent 이벤트 누적 → 메모리/디스크 |
| Retention policy 동기 실행 | Medium | 종료 시 블로킹 |

**Key files**: `crates/oneshim-network/src/batch_uploader.rs`, `http_client.rs`, `src-tauri/src/scheduler/loops/network.rs`

---

### 5. Suggestion Pipeline (SSE → Queue → UI)

**Flow**: `SSE connect → SuggestionReceiver → SuggestionQueue(max 50) → DesktopNotifier → Presenter`

| Gap | Severity | Description |
|-----|----------|-------------|
| 큐 오버플로 시 무손실 drop | Medium | 50개 초과 시 oldest 삭제, 사용자 알림 없음 |
| 알림 표시 실패 재시도 없음 | Low | platform notifier 에러 무시 |

**Key files**: `crates/oneshim-suggestion/src/receiver.rs`, `queue.rs`, `presenter.rs`, `feedback.rs`

---

### 6. Analysis & Intelligence Pipeline

**Flow**: `AdaptiveTrigger → EmaStatsTracker → DriftDetector → RegimeClassifier → RegimeManager → ContentTracker → CoachingEngine`

| Gap | Severity | Description |
|-----|----------|-------------|
| Regime re-clustering 메인 루프 stall | Medium | heavy computation이 monitor loop 블로킹 가능 |
| EMA stats 초기화/리셋 미정의 | Low | 앱 재시작 시 baseline 소실 |
| Clustering 전략 선택 미문서화 | Low | HDBSCAN vs k-means 선택 기준 불명확 |

**Sub-pipelines**:
- Adaptive Trigger: `AdaptiveTrigger → EmaStatsTracker → DriftDetector → TriggerDecision`
- Regime Detection: `RegimeClassifier → RegimeDetector → RegimeManager → SharedRegimeState`
- Content Analysis: `SegmentBuffer → SegmentSummarizer → WorkTypeClassifier → ContentTracker`
- Embedding & Search: `EmbeddingPipeline → VectorStore → VectorRetriever → AdaptiveSearchCoordinator`
- Coaching: `CoachingEngine.evaluate() → TemplateRegistry → LLM personalization → MagicOverlay`

**Key files**: `crates/oneshim-analysis/src/adaptive_trigger.rs`, `regime_classifier.rs`, `regime_manager.rs`, `coaching_engine/`, `embedding_pipeline.rs`, `vector_retriever.rs`

---

### 7. Storage Pipeline

**Flow**: `Event/Metrics/Frame writes → SQLite(WAL) → Migration → FrameFileStorage → Retention`

| Gap | Severity | Description |
|-----|----------|-------------|
| 디스크 공간 검사 없음 | **High** | 디스크 풀 시 SQLite 쓰기 실패 → 데이터 손실 |
| Migration 실패 무시 | Medium | 스키마 마이그레이션 에러 시 silent continue |
| Partial write rollback 없음 | Medium | 트랜잭션 중 에러 시 부분 데이터 잔존 |

**Key files**: `crates/oneshim-storage/src/sqlite.rs`, `migration.rs`, `frame_storage.rs`, `encryption.rs`

---

### 8. Automation/GUI Interaction Pipeline

**Flow**: `Intent → AutomationController → GuiInteractionService → AccessibilityTree → FocusValidation → TicketSigning → ActionExecution → AuditLog`

| Gap | Severity | Description |
|-----|----------|-------------|
| Ticket expiry 고부하 시 초과 | Low | 정상 운영에서는 문제 없음 |
| Overlay cleanup race condition | Low | ADR-002 M2-P2 추적 중 |

**Performance targets**: create_session <50ms, highlight_session <16ms, confirm_candidate <10ms

**Key files**: `crates/oneshim-automation/src/controller/`, `gui_interaction/service.rs`, `audit.rs`

---

### 9. Cross-Device Sync Pipeline

**Flow**: `SyncEngine.run_cycle() → pull() → merge(LWW/dedup) → push() → SyncCrypto encryption`

| Gap | Severity | Description |
|-----|----------|-------------|
| LAN transport 테스트 부족 | Medium | 통합 테스트 미작성 |
| Conflict resolution 에지 케이스 | Medium | LWW 충돌 시 데이터 손실 가능 |

**Key files**: `crates/oneshim-network/src/sync/`, `crates/oneshim-storage/src/sync_extractor.rs`, `sync_merger.rs`

---

### 10. Web Dashboard / REST API Pipeline

**Flow**: `HTTP Request → Axum routes → Handler → Assembler → Service → JSON Response`

| Gap | Severity | Description |
|-----|----------|-------------|
| (양호) | — | 26+ 엔드포인트 구현 완료, WebSocket 실시간 업데이트 |

**Major endpoints**: `/api/dashboard`, `/api/metrics`, `/api/events`, `/api/frames`, `/api/sessions`, `/api/search`, `/api/tags`, `/api/coaching`, `/api/automation`, etc.

**Key files**: `crates/oneshim-web/src/routes.rs`, `handlers/`, `services/`

---

### 11. Health Monitoring Pipeline

**Flow**: `HealthLoop → server/LLM/CLI probes → Atomic bool flags → Tray icon sync → Notification`

| Gap | Severity | Description |
|-----|----------|-------------|
| Health → UI 반영 지연 | Low | Atomic bool 기반, UI 폴링 간격 의존 |

**Key files**: `src-tauri/src/scheduler/loops/health.rs`

---

### 12. OAuth/Authentication Pipeline

**Flow**: `ProviderRegistry → OIDC DeviceFlow → CallbackServer → TokenExchange → RefreshCoordinator → ReauthEvent`

| Gap | Severity | Description |
|-----|----------|-------------|
| (양호) | — | PKCE, 자동 갱신, reauth 알림 구현 완료 |

**Key files**: `crates/oneshim-network/src/oauth/`

---

### 13. Integration Sync Pipeline

**Flow**: `IntegrationRuntime → SessionCoordinator → ProducerCoordinator → EgressCoordinator → HttpTransport → CloudEvents`

| Gap | Severity | Description |
|-----|----------|-------------|
| (양호) | — | Resilience(retry+backoff), connectivity 감지, privacy-aware egress |

**Key files**: `crates/oneshim-network/src/integration/`

---

### 14. Scheduler Architecture

**10 concurrent async loops**: monitor(1s), system(5s), network, sync, suggestions, health, events, intelligence, coaching_helper, helpers

| Gap | Severity | Description |
|-----|----------|-------------|
| Loop panic 격리 없음 | Medium | 한 루프 panic 시 전체 scheduler 영향 가능 |

**Key files**: `src-tauri/src/scheduler/mod.rs`, `loops/`

---

## Priority Summary

### High (즉시 대응)

1. **Network circuit breaker 부재** — 서버 장애 시 무한 재시도로 리소스 소진
2. **Upload queue unbounded growth** — 지속적 업로드 실패 시 메모리/디스크 누적
3. **디스크 공간 사전 검사 없음** — 디스크 풀 시 SQLite 쓰기 실패로 데이터 손실
4. **Ring buffer backpressure 없음** — 인코더 지연 시 프레임 무경고 삭제

### Medium (개선 권장)

5. Regime clustering 메인 루프 블로킹 가능
6. Migration 실패 silent 처리
7. LAN sync 테스트 커버리지 부족
8. SSE 큐 오버플로 사용자 알림 없음
9. Accessibility API 실패 재시도 없음
10. Scheduler loop panic 격리 없음
11. Sync conflict resolution 에지 케이스
12. Retention policy 종료 시 동기 블로킹

### Low (향후 개선)

13. JPEG 인코딩 동기 블로킹
14. Focus highlight thrashing
15. EMA stats 앱 재시작 시 소실
16. 알림 표시 실패 재시도 없음
17. Health → UI 반영 지연
18. Ticket expiry 고부하 초과
19. Overlay cleanup race condition
20. Clustering 전략 선택 미문서화

---

## Revision History

| Date | Changes |
|------|---------|
| 2026-03-27 | Initial review — 14 pipelines mapped, 20 gaps identified |
