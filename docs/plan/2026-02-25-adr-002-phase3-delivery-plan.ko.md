[English](./2026-02-25-adr-002-phase3-delivery-plan.md) | [한국어](./2026-02-25-adr-002-phase3-delivery-plan.ko.md)

# ADR-002 Phase3 상세 실행 계획

**날짜**: 2026-02-25  
**상태**: Active  
**원본 ADR**: `docs/architecture/ADR-002-os-gui-interaction-boundary.md`  
**기준 계획 문서**: [`2026-02-25-adr-002-gui-v2-implementation-plan.ko.md`](./2026-02-25-adr-002-gui-v2-implementation-plan.ko.md)

## 1. 목표

ADR-002의 OS GUI 상호작용을 프로덕션 등급으로 완성합니다.

1. 세션/티켓 보안 게이트 신뢰성 확보
2. 전용 세션 SSE 라이프사이클 정확성 확보
3. macOS/Windows/Linux 네이티브 접근성 + 오버레이 어댑터 하드닝

## 2. 현재 베이스라인 (2026-02-25 기준 구현 완료)

- GUI V2 계약, 세션 흐름, 전용 SSE 엔드포인트
- `ONESHIM_GUI_TICKET_HMAC_SECRET` 기반 HMAC 티켓 서명/검증
- 대상 OS 전체의 접근성/오버레이 시작 어댑터
- `oneshim-app` 조립 루트 연동

## 3. 워크스트림

## WS-1: Control Plane 및 계약 하드닝

대상 크레이트:
- `oneshim-automation`
- `oneshim-web`
- `docs/contracts`

구현 항목:
1. GUI V2 엔드포인트의 `401/409/422/503` 경계 경로 핸들러 테스트를 추가합니다.
2. 7개 V2 엔드포인트와 전용 SSE 이벤트 페이로드 예시를 계약 문서에 추가합니다.
3. 세션 토큰/티켓 만료 경계 테스트와 nonce replay 테스트를 추가합니다.
4. 상태 전이 및 실행 거부 경로별 audit 검증을 추가합니다.

완료 기준:
1. 계약 예시는 `docs/contracts/`에 버전 관리됩니다.
2. GUI 세션 API 네거티브 경로 테스트가 CI에서 안정적으로 통과합니다.
3. 실행 실패 경로마다 추적 가능한 audit 이벤트가 남습니다.

## WS-2: Execution Plane 하드닝 (OS별)

대상 크레이트:
- `oneshim-app`

구현 항목:
1. macOS: 스크립트 fallback 경로를 AXUIElement + NSWindow overlay(기능 플래그)로 대체합니다.
2. Windows: 스크립트 fallback 경로를 UIA COM + layered transparent overlay로 대체합니다.
3. Linux: AT-SPI 탐지 경로를 추가하고 X11/Wayland fallback 전략을 명시적으로 유지합니다.
4. DPI/compositor 영향을 고려한 좌표 정규화와 focus 검증을 통합합니다.
5. 미지원/headless 환경의 no-op fallback을 유지합니다.

완료 기준:
1. 각 OS마다 최소 1개의 네이티브 어댑터 경로를 설정/플래그로 선택할 수 있습니다.
2. 세션 cancel/expire/execute 시 overlay 정리가 결정적으로 수행됩니다.
3. execute 시점 focus drift가 일관되게 `409`로 차단됩니다.

## WS-3: 신뢰성 및 운영 준비

대상 크레이트/문서:
- `oneshim-app`
- `oneshim-automation`
- `docs/guides`
- `docs/qa`

구현 항목:
1. `propose -> highlight -> confirm -> execute` E2E 스모크 시나리오를 추가합니다.
2. 권한 거부, focus drift, 티켓 만료, overlay 렌더 실패 시나리오를 추가합니다.
3. 운영자 트러블슈팅 런북(권한, OS feature toggle, fallback 동작)을 보강합니다.
4. 크로스 OS 검증용 QA 실행 메타 템플릿을 추가합니다.

완료 기준:
1. macOS/Windows/Linux 반복 가능한 스모크 매트릭스가 존재합니다.
2. 알려진 실패 시그니처마다 운영 대응 가이드가 명시됩니다.
3. CI에 GUI V2 계약 + 서비스 레벨 회귀 검증이 포함됩니다.

## 4. 실행 일정 (계획)

1. 2026-02-26 ~ 2026-03-03: WS-1 완료(계약/테스트/audit 경로).
2. 2026-03-04 ~ 2026-03-14: WS-2 OS별 네이티브 어댑터 하드닝.
3. 2026-03-15 ~ 2026-03-21: WS-3 런북/QA 매트릭스/릴리즈 게이트 점검.

## 5. 릴리즈 게이트

1. 보안: HMAC secret 필수, nonce replay 차단, 모든 `:id` 라우트/SSE에서 세션 토큰 강제.
2. 아키텍처: 승인된 조립 경로 외 어댑터 간 직접 결합 금지.
3. 품질: 변경 크레이트 대상 `cargo check` 및 관련 `cargo test` 통과.
4. 문서: `docs/plan/README.ko.md`와 `docs/README.ko.md` 인덱스 동기화 유지.
