# Recording Pipeline Fix Handoff

> 생성: 2026-03-18, 메인 세션 분석 결과 기반

## 현재 상태

로컬 레코딩 파이프라인은 **완전 동작** (9-loop 스케줄러, 스크린 캡처, SQLite 저장, 웹 대시보드 전부 OK).
**서버 전송 + AI 연동**만 차단됨.

## P0: 즉시 수정 필요 (2개)

### 1. `upload_enabled` 하드코딩 해제

**파일**: `src-tauri/src/agent_runtime_support.rs` ~line 112
```rust
// 현재 (차단됨):
upload_enabled: false,

// 수정: AppConfig에서 읽도록 변경
```

**영향 범위**:
- `src-tauri/src/scheduler/config.rs` — `SchedulerConfig.upload_enabled` 필드 (이미 존재)
- `src-tauri/src/scheduler/config.rs:56` — `PlatformEgressPolicy::new()` 에서 `config.upload_enabled` 사용
- `src-tauri/src/scheduler/loops.rs` ~line 375-387 — sync loop에서 `egress.is_enabled()` 체크

**작업 내용**:
1. `AppConfig`에 `upload_enabled: bool` 설정 추가 (또는 기존 `monitor` 섹션에 추가)
2. `agent_runtime_support.rs`에서 `self.config`에서 값을 읽도록 변경
3. 기본값은 `false` 유지 (안전)
4. Settings UI에서 토글 가능하게 (선택)

**주의**: `scheduler/mod.rs`에 테스트 `platform_sync_is_disabled_in_current_ai_runtime`이 있음 — 기본값 false를 assert하므로 기본값은 false로 유지해야 함.

### 2. `mark_as_sent` 호출 추가

**파일**: `crates/oneshim-network/src/batch_uploader.rs` — `flush()` 메서드 (~line 193)

**현재**: upload 성공 후 이벤트의 `is_sent` 플래그를 업데이트하지 않음
**문제**: retention cleanup이 `is_sent=1`인 이벤트만 삭제 → 이벤트 무한 증가

**작업 내용**:
1. `BatchUploader`에 storage 참조 추가 (또는 flush 호출자에서 처리)
2. upload 성공 시 `storage.mark_as_sent(event_ids)` 호출
3. `crates/oneshim-storage/src/sqlite/events.rs` — `mark_as_sent()` 메서드 이미 구현됨 (line 239-272)

## P1: 리텐션 정상화

### 3. Frame 리텐션 스케줄러 연결

**파일**: `src-tauri/src/scheduler/loops.rs` — aggregation loop (~line 461-505)

**현재**: `FrameFileStorage::enforce_retention()` 과 `enforce_storage_limit()` 메서드 존재하지만 **스케줄러에서 호출하지 않음**
**영향**: 프레임 파일이 무한 축적 (30일/500MB 정책 미적용)

**작업**: aggregation loop (1시간 주기)에 아래 호출 추가:
```rust
frame_storage.enforce_retention().await;
frame_storage.enforce_storage_limit().await;
```

## P2: AI LLM 연동 설계

### 현재 연결 상태
- `RemoteOcrProvider` — EdgeFrameProcessor에서 importance ≥ 0.8일 때 호출됨 ✅
- `RemoteLlmProvider` — 구현 완료 (Anthropic/OpenAI/Gemini/Ollama), 레코딩 데이터와 미연결 ❌
- `SuggestionReceiver` — SSE 수신 준비 완료, 서버 제안 대기 중 ✅

### 의도된 E2E 흐름
```
캡처 → EdgeFrameProcessor(OCR) → SQLite 저장
  → BatchUploader → 서버 POST /user_context/batches
  → 서버 AI Intelligence → Suggestion 생성
  → SSE /user_context/suggestions/stream → SuggestionReceiver → 알림
```

### 필요 작업
- 서버측 AI Intelligence 도메인에서 배치 데이터 수신 → 분석 → 제안 생성 구현
- 또는 클라이언트 로컬에서 LLM으로 직접 분석 (RemoteLlmProvider 활용)

## 참고: 코드 위치 맵

| 컴포넌트 | 경로 |
|---------|------|
| 스케줄러 config | `src-tauri/src/scheduler/config.rs` |
| 스케줄러 loops | `src-tauri/src/scheduler/loops.rs` |
| DI 와이어링 | `src-tauri/src/agent_runtime_support.rs` |
| BatchUploader | `crates/oneshim-network/src/batch_uploader.rs` |
| SQLite events | `crates/oneshim-storage/src/sqlite/events.rs` |
| Frame storage | `crates/oneshim-storage/src/frame_storage.rs` |
| AppConfig | `crates/oneshim-core/src/config/mod.rs` |
| EdgeFrameProcessor | `crates/oneshim-vision/src/processor.rs` |
| RemoteLlmProvider | `crates/oneshim-network/src/ai_llm_client.rs` |
| RemoteOcrProvider | `crates/oneshim-network/src/ai_ocr_client.rs` |

## 테스트 주의사항

- `cargo test -p oneshim-app -- scheduler::tests` — 스케줄러 관련 6개 테스트
- `platform_sync_is_disabled_in_current_ai_runtime` — 기본값 false assert
- `strict_policy_redacts_window_title` / `allow_filtered_policy_uses_pii_filter` — `upload_enabled: true` 설정 필요
