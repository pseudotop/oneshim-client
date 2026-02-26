[English](./2026-02-25-adr-002-gui-v2-implementation-plan.md) | [한국어](./2026-02-25-adr-002-gui-v2-implementation-plan.ko.md)

# ADR-002 GUI V2 구현 계획

**날짜**: 2026-02-25
**상태**: Active
**원본 ADR**: `docs/architecture/ADR-002-os-gui-interaction-boundary.md`
**상세 계획**: [`2026-02-25-adr-002-phase3-delivery-plan.ko.md`](./2026-02-25-adr-002-phase3-delivery-plan.ko.md)

## 1. 목표

ADR-002 상호작용 모델을 E2E로 출시합니다.

1. `propose -> highlight -> confirm -> execute`
2. 고정 환경설정 기반 서명 티켓 (`ONESHIM_GUI_TICKET_HMAC_SECRET`)
3. 전용 세션 SSE (`/api/automation/gui/sessions/:id/events`)
4. 제어 평면(web)과 실행 평면(OS runtime) 분리

## 2. 크레이트별 범위

- `oneshim-core`: GUI 계약(모델 + 포트)
- `oneshim-automation`: 세션 상태머신, 티켓 서명/검증, 오케스트레이션
- `oneshim-web`: V2 API 핸들러 + 토큰 검증 + 전용 SSE
- `oneshim-app`: 조립 루트 연결(FocusProbe 어댑터, Overlay driver)
- `oneshim-ui` / OS 어댑터: 네이티브 overlay 및 접근성 하드닝(Phase 3)

## 3. 마일스톤

## M0: 계약 + 베이스 런타임 (완료)

- `FocusProbe`, `OverlayDriver` 포트 추가
- GUI 모델(세션/후보/포커스 스냅샷/티켓/이벤트) 추가
- In-memory `GuiInteractionService` 추가
  - 세션 capability token 검증
  - HMAC 서명/검증
  - TTL cleanup task
  - 상태 전이 및 이벤트 발행
- 컨트롤러 오케스트레이션 메서드 추가
- Web V2 엔드포인트 + 전용 세션 SSE 추가

## M1: 호환성 및 핸들러 하드닝 (진행 중)

- 핸들러 통합 테스트 추가
  - `X-Gui-Session-Token` 누락/오류
  - `409/422/503` 매핑
  - 세션 스코프 SSE 필터링
- 전이/거부 경로 전체에 대한 audit 이벤트 보강
- `docs/contracts`에 요청/응답 예시 명세 추가

## M2: 실행 신뢰성 (다음)

- execute 경로의 atomic focus 재검증 진단 강화
- nonce replay 및 경계값 테스트(세션/티켓 만료, race window)
- 복구 가능한 실행 실패에 대한 retry 시맨틱 추가

## M3: 네이티브 Overlay + 접근성 (Phase 3 핵심)

### macOS
- AXUIElement 기반 접근성 어댑터
- always-on-top, click-through NSWindow overlay 어댑터
- AX 권한 미부여 시 권한/실패 UX 처리

### Windows
- UIA COM 기반 접근성 어댑터
- `WS_EX_LAYERED` 기반 투명/click-through overlay 어댑터
- DPI 보정 좌표 정규화

### Linux
- D-Bus AT-SPI 기반 접근성 어댑터
- X11/Wayland fallback overlay 전략
- compositor 호환성 처리

## M4: 프로덕션 하드닝

- 후보 랭킹/overlay 갱신 지연 성능 프로파일링
- 보안 점검(session hijack/replay/spoofing)
- OS별 및 headless fallback E2E 스모크 시나리오 정리

## 4. API 계약 (V2)

- `POST /api/automation/gui/sessions`
- `GET /api/automation/gui/sessions/:id`
- `POST /api/automation/gui/sessions/:id/highlight`
- `POST /api/automation/gui/sessions/:id/confirm`
- `POST /api/automation/gui/sessions/:id/execute`
- `DELETE /api/automation/gui/sessions/:id`
- `GET /api/automation/gui/sessions/:id/events`

모든 `:id` 라우트는 `X-Gui-Session-Token`이 필수입니다.

## 5. 보안 게이트

- 티켓 무결성: 티켓 페이로드 HMAC 서명
- 모든 세션 API/SSE에 capability token 필수
- focus drift(`409`), 티켓 무효/만료(`422`) 시 실행 차단
- HMAC secret 미설정 시 fail-closed(`503`)

## 6. 완료 기준 (Definition of Done)

- M0-M2가 CI 포함하여 안정화
- M3에서 OS별 최소 1개 네이티브 overlay + 접근성 어댑터를 feature flag로 출하
- 프런트엔드가 GUI 세션 라이프사이클의 단일 진실원천으로 전용 SSE 사용
- 롤아웃 동안 기존 scene API는 계속 동작

## 7. 현재 구현 스냅샷 (2026-02-25)

이번 작업으로 반영된 항목:
- Core GUI 계약 + 포트
- Automation GUI 서비스 + 컨트롤러 오케스트레이션
- Web V2 라우트 + 전용 세션 SSE
- `ProcessMonitor` 기반 `FocusProbe` 연결
- HMAC secret 고정(`ONESHIM_GUI_TICKET_HMAC_SECRET`)
- Phase 3 시작 어댑터를 대상 OS 전체에 추가:
  - macOS: System Events 기반 접근성 프로브 + Python 오버레이 테두리 렌더러
  - Windows: PowerShell UIA 프로브 + WinForms 오버레이 테두리 렌더러
  - Linux: `xdotool` 기반 활성 창 접근성 fallback + Python 오버레이 테두리 렌더러

우선순위 남은 작업:
1. 핸들러/서비스 통합 테스트 + 계약 문서화(M1)
2. 실행 race/replay 경계 하드닝(M2)
3. OS별 실제 네이티브 overlay + 접근성 어댑터(M3)
