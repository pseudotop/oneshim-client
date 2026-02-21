[English](./05-migration-phases.md) | [한국어](./05-migration-phases.ko.md)

# 5. 마이그레이션 단계 + 성공 기준

[← Server API](./04-server-api.ko.md) | [UI 프레임워크 →](./06-ui-framework.ko.md)

---

## Phase 0: 프로젝트 기반 구축

**목표**: Rust workspace 생성, 빌드 파이프라인 설정

```
[ ] Cargo workspace 초기화 (8개 크레이트)
[ ] CI/CD 설정 (cargo test, cargo clippy, cargo fmt)
[ ] 크로스 컴파일 설정 (macOS universal + Windows x64)
[ ] .cargo/config.toml 빌드 최적화
```

## Phase 1: 핵심 모델 + 네트워크 (P0 — SSE 연결)

**목표**: 서버와 SSE로 연결하여 제안을 수신하는 최소 파이프라인

```
[ ] oneshim-core: 모델 구조체 (serde), trait 정의, 에러 타입
[ ] oneshim-network/auth.rs: 로그인 → JWT 토큰 저장 → 자동 갱신
[ ] oneshim-network/http_client.rs: reqwest 기반 API 클라이언트
[ ] oneshim-network/sse_client.rs: SSE 스트림 연결 + 이벤트 파싱
[ ] oneshim-suggestion/receiver.rs: SSE 이벤트 → Suggestion 구조체 변환
[ ] oneshim-suggestion/feedback.rs: 수락/거절 HTTP POST
[ ] oneshim-app/main.rs: 최소 실행 — 로그인 → SSE 연결 → stdout 출력
```

**검증**: `cargo run` → 서버에서 SSE 제안 수신 → 터미널에 출력

## Phase 2: 로컬 저장소 + 모니터링 + Edge Vision

**목표**: 컨텍스트 수집 + 이미지 Edge 처리 → 로컬 저장 → 배치 업로드

```
[ ] oneshim-storage/sqlite.rs: 이벤트 로그 + 프레임 인덱스 테이블, CRUD, 보존 정책
[ ] oneshim-storage/migration.rs: 스키마 버전 관리
[ ] oneshim-monitor/system.rs: sysinfo 기반 CPU/메모리/디스크/네트워크
[ ] oneshim-monitor/process.rs: 활성 창 정보 (플랫폼 분기)
[ ] oneshim-monitor/macos.rs: CoreGraphics 프론트 앱 감지
[ ] oneshim-monitor/windows.rs: Win32 GetForegroundWindow
[ ] oneshim-vision/capture.rs: xcap 기반 스크린 캡처 (멀티모니터)
[ ] oneshim-vision/trigger.rs: 스마트 캡처 트리거 (이벤트 기반, 5초 쓰로틀)
[ ] oneshim-vision/processor.rs: Edge 전처리 오케스트레이터 (중요도별 분기)
[ ] oneshim-vision/delta.rs: 델타 인코딩 (이전 프레임 대비 변경 영역만 추출)
[ ] oneshim-vision/encoder.rs: WebP/JPEG 인코딩 + 품질 자동 선택
[ ] oneshim-vision/thumbnail.rs: 480×270 썸네일 생성
[ ] oneshim-vision/ocr.rs: Tesseract FFI 로컬 OCR (텍스트 메타 추출)
[ ] oneshim-vision/timeline.rs: 프레임 인덱스 관리 (SQLite 연동, 리와인드 지원)
[ ] oneshim-vision/privacy.rs: PII 필터링 (창 제목 새니타이징)
[ ] oneshim-network/batch_uploader.rs: 배치 큐 + 재시도 + 압축 (메타+이미지 혼합)
[ ] oneshim-network/compression.rs: gzip/zstd/lz4 선택적 압축
[ ] oneshim-app/scheduler.rs: 모니터링 루프 (1초), 동기화 루프 (10초), 하트비트
```

**검증**: 컨텍스트 수집 + 스크린샷 Edge 처리 → SQLite 저장 → 메타+전처리 이미지 배치 업로드 → SSE 수신

## Phase 3: UI 기반

**목표**: 시스템 트레이 + 제안 알림 + 메인 창 + 리와인드 타임라인

```
[ ] oneshim-ui/tray.rs: 시스템 트레이 아이콘 + 메뉴 (Show/Hide, Settings, Quit)
[ ] oneshim-ui/views/suggestion_popup.rs: 제안 토스트/팝업 (수락/거절 버튼)
[ ] oneshim-ui/views/main_window.rs: 현재 컨텍스트 + 상태 표시
[ ] oneshim-ui/views/status_bar.rs: 연결 상태, 메트릭
[ ] oneshim-ui/views/context_panel.rs: 활성 앱, 시스템 리소스
[ ] oneshim-ui/views/timeline_view.rs: 스크린샷 리와인드 타임라인 (썸네일 스크롤)
[ ] oneshim-ui/theme.rs: 다크/라이트 테마
[ ] oneshim-suggestion/presenter.rs: 제안 → UI 데이터 변환 (파이프라인 프리뷰 포함)
[ ] oneshim-suggestion/queue.rs: 로컬 제안 큐 (최대 50개, 우선순위)
```

**검증**: 트레이 아이콘 → SSE 제안 수신 → 데스크톱 알림/팝업 → 수락/거절 → 타임라인 리와인드

## Phase 4: 완성도

**목표**: 기능 완전성 + 배포 준비

```
[ ] oneshim-ui/views/settings.rs: 설정 화면
[ ] oneshim-network/ws_client.rs: WebSocket (대화 모드)
[ ] oneshim-suggestion/history.rs: 제안 이력 로컬 캐시
[ ] oneshim-app/lifecycle.rs: 시작/종료, 리소스 정리
[ ] oneshim-app/event_bus.rs: 내부 이벤트 (tokio::broadcast)
[ ] 자동 시작 설정 (launchd / 레지스트리)
[ ] 자동 업데이트 메커니즘
[ ] 인스톨러 빌드 (.dmg, .exe/.msi)
[ ] README, 사용자 가이드
```

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

### Phase 1 완료 기준 (MVP)
- [ ] `cargo run` → 서버 로그인 → SSE 연결 → 제안 수신 → 터미널 출력
- [ ] 제안 수락/거절 피드백 전송 → 서버에서 확인

### Phase 2 완료 기준
- [ ] 컨텍스트 수집 (활성 창, CPU, 메모리) → SQLite 저장 → 배치 업로드
- [ ] 스크린 캡처 → Edge 전처리 (델타/썸네일/OCR) → 메타+이미지 배치 전송
- [ ] 프레임 인덱스 SQLite 저장 + 보존 정책 동작
- [ ] 서버에서 컨텍스트 기반 제안 생성 → SSE로 수신

### Phase 3 완료 기준
- [ ] 시스템 트레이 아이콘 + 메뉴
- [ ] 제안 수신 → 데스크톱 알림 → 수락/거절 UI
- [ ] 메인 창: 현재 컨텍스트 + 상태 표시
- [ ] 타임라인 리와인드: 썸네일 스크롤 + 텍스트 검색

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
