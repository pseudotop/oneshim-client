[English](./http-api-standardization.md) | [한국어](./http-api-standardization.ko.md)

# HTTP API 표준화 (OpenAPI 트랙)

이 문서는 ONESHIM 로컬 웹 인터페이스를 OpenAPI 호환 거버넌스로 표준화하는 기준을 정의합니다.

## 현재 SSOT

- 전송 DTO SSOT: `crates/oneshim-api-contracts`
- 런타임 라우트 SSOT: `crates/oneshim-web/src/routes.rs`
- 기계 검증용 인터페이스 스냅샷: `docs/contracts/http-interface-manifest.v1.json`

## 결정 사항

1. 전송 DTO 소유권은 `oneshim-api-contracts`에 유지하고, 핸들러는 공개 DTO를 직접 정의하지 않습니다.
2. 라우트 소유권은 `oneshim-web/src/routes.rs`에 유지합니다.
3. OpenAPI 연결 산출물로 버전드 인터페이스 매니페스트(`http.interface.manifest.v1`)를 유지합니다.
4. CI에서 라우트와 매니페스트 동기화를 강제합니다 (`scripts/verify-http-interface-manifest.sh`).
5. HTTP 계약의 breaking 변경은 명시적 버전 업으로 관리합니다.

## 이 구조를 선택한 이유

- 도메인/헥사고날 경계를 보존하면서 웹 전용 메타데이터 유입을 방지합니다.
- 전면 OpenAPI 생성 이전에도 기계 검증 가능한 인터페이스 거버넌스를 즉시 확보합니다.
- AI/자동화/업데이트/진단 등 엔드포인트 확장 시 소유 모듈을 명확히 유지합니다.

## 다음 단계 (계획)

1. 아래 입력으로 build-time OpenAPI 생성 단계를 추가합니다.
   - 라우트 맵 (`routes.rs`)
   - 계약 모듈 (`oneshim-api-contracts`)
   - 매니페스트 메타데이터 (`http-interface-manifest.v1.json`)
2. 생성된 `oneshim-web.v1.openapi.yaml`을 CI 아티팩트 및 릴리스 산출물로 게시합니다.
3. PR에서 라우트/계약 변경 시 OpenAPI diff gate를 추가합니다.

## 현재 단계의 비목표

- OpenAPI 생성기의 런타임 의존성 도입 없음
- 어댑터/도메인 결합 구조 변경 없음
- 핸들러 단위 수동 스키마 중복 작성 없음
