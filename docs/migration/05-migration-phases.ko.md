[English](./05-migration-phases.md) | [한국어](./05-migration-phases.ko.md)

# 5. 마이그레이션 단계 + 성공 기준

[← Server API](./04-server-api.ko.md) | [UI 프레임워크 →](./legacy/06-ui-framework.ko.md)

---

## Phase 0: 프로젝트 기반 구축 ✅ 완료

**목표**: Rust workspace 생성, 빌드 파이프라인 설정

- [x] Cargo workspace (현재 15개 패키지 — `cargo metadata --no-deps`)
- [x] CI/CD (cargo test, cargo clippy, cargo fmt)
- [x] 크로스 컴파일 (macOS universal + Windows x64 + Linux)
- [x] `.cargo/config.toml` 빌드 최적화

## Phase 1: 핵심 모델 + 네트워크 (P0 — SSE 연결) ✅ 완료

**목표**: 서버와 SSE로 연결하여 제안을 수신하는 최소 파이프라인

- [x] `oneshim-core` 모델 + trait + 에러 타입 (ADR-019 typed code로 발전)
- [x] `oneshim-network/auth.rs` JWT 로그인 + 자동 갱신
- [x] `oneshim-network/http_client.rs` reqwest 기반 API 클라이언트
- [x] `oneshim-network/sse_client.rs` SSE 스트림 + 자동 재연결
- [x] `oneshim-suggestion/receiver.rs` SSE → Suggestion 변환
- [x] `oneshim-suggestion/feedback.rs` 수락/거절 HTTP POST + FeedbackRetryQueue

**검증**: `cargo run -p oneshim-app` → SSE 제안 수신 → UI surface.

## Phase 2: 로컬 저장소 + 모니터링 + Edge Vision ✅ 완료

**목표**: 컨텍스트 수집 + 이미지 Edge 처리 → 로컬 저장 → 배치 업로드

- [x] `oneshim-storage/sqlite.rs` + V1–V22 마이그레이션 (events, frames, work_sessions, focus_metrics, IVF index, coaching 등)
- [x] `oneshim-monitor/{system,process,macos,windows,linux,activity,input_activity,window_layout}.rs` — 플랫폼 분기 active window + 메트릭 + idle + 레이아웃 추적
- [x] `oneshim-vision/{capture,trigger,processor,delta,encoder,thumbnail,ocr,timeline,privacy}.rs` — Edge 파이프라인 (xcap 멀티모니터 캡처 → WebP 인코더 → 썸네일 LRU → 중요도별 delta/OCR → Off/Basic/Standard/Strict 단계 PII 필터)
- [x] `oneshim-network/{batch_uploader,compression}.rs` — lock-free SegQueue + gzip/zstd/lz4 자동 선택
- [x] `src-tauri/src/scheduler/` — 16 루프 background scheduler (원래 계획된 단일 `scheduler.rs` 대체)

**검증**: 컨텍스트 수집 + 스크린샷 Edge 처리 → SQLite retention → 메타+이미지 배치 업로드 → SSE 수신.

## Phase 3: UI 기반 ✅ 완료 (Tauri 경유 — ADR-004 참조)

**목표**: 시스템 트레이 + 제안 알림 + 메인 창 + 리와인드 타임라인

> 원래 계획된 `oneshim-ui` 크레이트(iced)는 [ADR-004](../architecture/ADR-004-tauri-v2-migration.ko.md)에 따라 **Tauri v2 + React** 로 교체. 논리적 산출물은 새 surface 로 shipped:

- [x] 시스템 트레이 (`src-tauri/src/tray.rs`)
- [x] 제안 popup / toast — 데스크톱 알림 + MagicOverlay(ADR-002 M3) 로 전달
- [x] 메인 창 + status bar + context panel — `crates/oneshim-web/frontend/src/pages/` 하위 React 페이지
- [x] 타임라인 rewind — frame timeline (in-memory + SQLite 기반)
- [x] 다크/라이트 테마 — React `useTheme` hook
- [x] `oneshim-suggestion/{presenter,queue}.rs` — SuggestionView + BTreeSet 우선순위 큐 (최대 50)

**검증**: Tray → SSE 제안 → 데스크톱 알림/팝업 → 수락/거절 → 타임라인 리와인드.

## Phase 4: 완성도 ✅ 완료

**목표**: 기능 완전성 + 배포 준비

- [x] 설정 화면 — React 설정 탭 (GeneralTab, NotificationsTab, PermissionsTab, PrivacyTab 등)
- [x] `oneshim-suggestion/history.rs` FIFO 이력 캐시
- [x] Lifecycle (start/shutdown + 리소스 정리) — `src-tauri/src/lifecycle/`
- [x] 내부 이벤트 버스 — scheduler 전역 tokio::broadcast
- [x] 자동 시작 (launchd / 레지스트리)
- [x] 자동 업데이트 메커니즘 — `src-tauri/src/updater/` with D9 다중키 Ed25519 trust + D10 방어적 rollout + D11 self-healthy probe with 자동 rollback
- [x] 인스톨러 빌드 (.dmg, .exe/.msi, .deb/.AppImage) via `cargo tauri build`
- [x] README + 사용자 가이드 + 한국어 companion 문서

## Phase 5: 자동 업데이트

**목표**: GitHub Releases 기반 자동 업데이트

```
[x] oneshim-app/updater.rs: 버전 확인 + 다운로드 + 바이너리 교체
[x] UpdateConfig: repo, 주기, prerelease 옵션
[x] 플랫폼별 에셋 자동 감지 (macOS arm64/x64, Windows, Linux)
[x] tar.gz, zip 압축 해제
```

## Phase 6: GA 준비

**목표**: CI/CD + 인스톨러 + 문서화

```
[x] GitHub Actions (rust-ci.yml, rust-release.yml)
[x] 4개 플랫폼 빌드 (macOS arm64/x64, Windows x64, Linux x64)
[x] macOS Universal Binary 자동 생성
[x] 태그 푸시 시 자동 릴리즈
[x] cargo-bundle, cargo-wix, cargo-deb 인스톨러
```

## Phase 8-35: 기능 강화

**상세**: CLAUDE.md "Phase N 추가사항" 섹션 참조

- **Phase 8**: 시스템 메트릭 저장, 유휴 감지, 세션 통계
- **Phase 9-14**: 로컬 웹 대시보드 (Axum + React)
- **Phase 15-19**: 알림, 내보내기, Dark/Light 테마, 키보드 단축키
- **Phase 20-24**: i18n, 디자인 시스템, 태그, 리포트
- **Phase 25-27**: 백업/복원, E2E 테스트, 세션 리플레이
- **Phase 28-30**: Edge Intelligence, SQLite 성능 최적화
- **Phase 31-33**: 썸네일 캐싱, Lock-free 큐, 버퍼 풀
- **Phase 34-35**: 서버 통합 강화, 이벤트 페이로드 확장

## Phase 36: gRPC 클라이언트 ★ NEW

**목표**: gRPC API 통합 (SSE 대체)

```
[x] oneshim-network/grpc/mod.rs: GrpcConfig + 모듈 export
[x] oneshim-network/grpc/auth_client.rs: Login, Logout, RefreshToken, ValidateToken
[x] oneshim-network/grpc/session_client.rs: CreateSession, EndSession, Heartbeat
[x] oneshim-network/grpc/context_client.rs: UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
[x] oneshim-network/grpc/unified_client.rs: gRPC + REST 통합 클라이언트
[x] Feature Flag: --features grpc
[x] REST Fallback: gRPC 실패 시 자동 전환
[x] 산업 현장 ASCII 출력: NO_EMOJI=1 환경변수
```

**검증**: Mock 서버 통신 테스트 — 8개 RPC 모두 성공

---

## 성공 기준

### Phase 1 완료 기준 (MVP) ✅
- [x] `cargo run` → 서버 로그인 → SSE 연결 → 제안 수신
- [x] 제안 수락/거절 피드백 전송 → 서버에서 확인

### Phase 2 완료 기준 ✅
- [x] 컨텍스트 수집 (활성 창, CPU, 메모리) → SQLite 저장 → 배치 업로드
- [x] 스크린 캡처 → Edge 전처리 (델타/썸네일/OCR) → 메타+이미지 배치 전송
- [x] 프레임 인덱스 SQLite 저장 + 보존 정책 (30일 / 500MB)
- [x] 서버에서 컨텍스트 기반 제안 생성 → SSE로 수신

### Phase 3 완료 기준 ✅ (Tauri 경유 — ADR-004)
- [x] 시스템 트레이 아이콘 + 메뉴 (`src-tauri/src/tray.rs`)
- [x] 제안 수신 → 데스크톱 알림 → 수락/거절 UI
- [x] 메인 창: React 기반 현재 컨텍스트 + 상태 표시
- [x] 타임라인 리와인드: 썸네일 스크롤 + 텍스트 검색 (FTS5)

### Phase 4 완료 기준 (GA)
- [x] .dmg / .exe 단일 바이너리 배포
- [x] 자동 시작 + 자동 업데이트
- [x] Python Client 전면 대체
- [x] 전체 테스트 통과 (cargo test --workspace)

### Phase 36 완료 기준 (gRPC)
- [x] gRPC 인증 RPC: Login, Logout, RefreshToken, ValidateToken
- [x] gRPC 세션 RPC: CreateSession, EndSession, Heartbeat
- [x] gRPC 컨텍스트 RPC: UploadBatch, SubscribeSuggestions, SendFeedback, ListSuggestions
- [x] Server Streaming: 실시간 제안 수신 (SSE 대체)
- [x] REST Fallback: 산업 현장 지원
- [x] Mock 서버 통신 검증 완료
