# ADR-019: 에러 코드 인프라 + AWS Bedrock 의도적 미지원

- **상태**: Accepted (2026-04-19)
- **관련**: ADR-001 (에러 전략), ADR-003 (디렉토리 모듈 패턴)
- **구현**: `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`, `docs/superpowers/plans/2026-04-19-error-code-infrastructure.md`

## 배경

ONESHIM client-rust 워크스페이스(14 crate, ~1,150 `CoreError` 생성 사이트)에 에러 코드 컨벤션이 없었다. 텔레메트리(Grafana), i18n(ko/en), 감사 로그 모두 안정적인 머신 리더블 식별자가 필요했다. 별개로 AWS Bedrock이 지원 provider surface로 등록돼 있었으나 구현 미완(Signature V4 인증 없음) — OCR의 `ProviderAuthScheme::AwsSignatureV4` arm에서 조용한 no-auth fallthrough 보안 버그 발생.

두 요구가 만남: 에러 코드 인프라 도입 + Bedrock을 첫 "의도적 미지원" 일급 시민으로 출시.

## 결정

### 1. 에러 코드 인프라

- 19개 code enum(`ConfigCode`, `NetworkCode`, ..., `GuiCode`)을 단일 `define_code_enum!` 매크로로 정의. enum 본체, `as_str` 매치, `Display`, `all()` 열거자를 한 variant 리스트에서 생성.
- 모든 struct-variant `CoreError`, `GuiInteractionError`는 타입 `code` 필드 보유; `#[from]`-래핑 외부 에러 타입(`Serialization`, `Io`)은 §7에 따라 `impl code()`에서 code 도출.
- 통합 접근자 `err.code() -> &'static str` — 텔레메트리/로그/i18n 진입점.
- Wire-format 코드는 `{domain}.{category}[.{qualifier}]` 컨벤션.
- 릴리스된 코드 문자열은 불변(wire contract). 신규 추가, 이름 변경 시 RFC PR 필요.

### 2. 네이밍 컨벤션

```
{domain}.{category}[.{qualifier}[.{sub_qualifier}]]
소문자, snake_case, dot 구분자
```

예: `config.invalid`, `network.timeout`, `provider.bedrock.unsupported`.

### 3. AWS Bedrock: 의도적 미지원

- Bedrock vendor + `provider_surface.bedrock.direct_api` surface를 `specs/providers/provider-surface-catalog.json`에서 **삭제**.
- `oneshim-network` 전반 7개 match arm이 `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, .. }` 반환:
  - `ai_ocr_client/mod.rs` (2 arm: auth + BedrockConverse request shape)
  - `ai_ocr_client/strategy.rs` (1 arm: strategy 선택)
  - `ai_llm_client/request.rs` (3 arm: request 빌드 + auth + 응답 파싱)
  - `http_api_session/mod.rs` (1 arm: auth)
- `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`, `ProviderRequestShape::BedrockConverse` enum variant는 **유지**(catalog 삭제 후 런타임 도달 불가) — 미래 재도입 경로의 surgical diff 확보.
- OCR `apply_auth_headers` 시그니처를 infallible → `Result<_, CoreError>`로 변경해 조용한 no-auth 보안 버그 닫음.
- 위 7개 match arm을 우회하는 sibling 클라이언트 경로에 defense-in-depth 가드 추가 — 모두 병합 후 drift 감사에서 추가, 동일한 `CoreError::Config { code: UnsupportedProviderBedrock, .. }` 반환:
  - `crates/oneshim-network/src/analysis_client.rs::analyze` — 조용히 Bedrock 엔드포인트로 OpenAI 포맷 요청 + Bearer 인증을 전송하는 경로를 차단하는 early-return 가드.
  - `crates/oneshim-web/src/services/ai_model_catalog_web_service.rs::list_models` — `resolve_model_discovery_api_key()` 호출 **전**에 early-return, AWS 자격 증명 없는 사용자가 generic "no API key" 에러 대신 친절한 unsupported 알림을 받도록.

### 4. Soft migration 전략

4 phase / 16 PR / 2-3주 계획이 단일 브랜치로 실현:
1. Phase 1: V2 variant 도입, V1 deprecated.
2. Phase 2: crate별 13 retrofit (12 crate + sandbox-worker 검증-only).
3. Phase 3: C5 Bedrock skip + 본 ADR.
4. Phase 4: V1 삭제 + V2 → canonical rename (rust-analyzer LSP, sed 금지).

CI deprecation 게이트: Phase 3까지 warn-only (`-A deprecated` escape hatch in lefthook clippy). Phase 4에서 escape hatch 제거, `-D warnings` 복원 (Rust의 `deprecated` 린트는 기본적으로 warn이므로 `-D warnings`가 잔여 V1 사용 시 CI 실패).

### 5. Bedrock 재도입 체크리스트

미래 Bedrock 지원이 필요해지면:

1. AWS Signature V4 서명 구현 (`aws-sigv4` crate 등).
2. AWS 자격 증명 로더 (`access_key` / `secret_key` / 옵션 `session_token`).
3. Settings UI에 AWS 자격 증명 필드.
4. Bedrock vendor + surface를 `provider-surface-catalog.json`에 재등록.
5. 7개 Bedrock match arm(`ConfigCode::UnsupportedProviderBedrock` 반환 중)을 작동하는 Bedrock 핸들러로 대체.
6. Bedrock 경로 live smoke 테스트(`--ignored`).
7. 새 코드 추가 시 wire-format 스냅샷 fixture 업데이트.
8. `ConfigCode::UnsupportedProviderBedrock`을 `ConfigCode`에서 제거 (wire-immutability 삭제 절차 — RFC PR 필요).

### 6. Public-API Exhaustiveness

`CoreError`, `GuiInteractionError`는 `#[non_exhaustive]` **미부착**.

근거:
1. 둘 다 이 워크스페이스(14 member) 내부용; 모든 소비자 1st-party.
2. Exhaustive match가 리팩터 중 누락 variant 잡아줌 — 기능이지 버그 아님.
3. `err.code()`가 패턴 매칭 불필요한 forward-compat 채널 제공.
4. 이 라이브러리가 외부로 추출/발행되면, 이 결정은 한 줄 변경 + 하류 `match` 리뷰로 가역.

Code enum(`ConfigCode` 등)은 `#[non_exhaustive]` **부착**:
- 내부 사용이지만 follow-up으로 확장 가능.
- 워크스페이스 내부 소비자를 variant 추가 파손에서 방어적으로 보호.

### 7. 신규 `#[from]` variant 추가

신규 `#[from]` 래핑 외부 에러 타입을 `CoreError`에 추가 시:

1. 해당 타입용 `InternalCode::*` variant 할당 (예: `tokio::io::Error` 추가 시 `InternalCode::TokioIo`).
2. 같은 PR에서 variant + `#[from]` 속성 추가.
3. `impl CoreError::code()`에 해당 arm 추가 (새 `InternalCode` 반환).
4. `wire_contract_snapshot.expected.txt` fixture 업데이트.

## 결과

### 긍정

- 머신 리더블 에러 식별자로 Grafana 레이블 그룹핑 가능.
- `err.code()`로 i18n 해금 (프런트에서 code를 번역 키로 소비).
- Bedrock UX 결정론적: 조용한 fallthrough 없음, catalog가 provider 광고 안 함.
- 타입-세이프 code 레지스트리; single-source 매크로 + 스냅샷 테스트로 wire format 드리프트 불가능.

### 부정

- 2-3주 migration 노력; V1/V2 공존이 일시적으로 enum variant 수 증가 (Phase 1-3).
- migration 창 동안 ~133 `#[deprecated]` 경고 (예상 신호, 회귀 아님).

### 중립

- Phase 4 rename 시 `CoreError` / `GuiInteractionError` 작업 중인 PR 간 잠깐 동결 필요.
- Phase 4 이후 Grafana 대시보드 재라벨링은 follow-up (비-블로킹).

### 알려진 follow-up (비-블로킹)

1. **Tauri IPC code 전파** — `src-tauri/src/commands/*.rs`의 ~58 callsite가 `.map_err(|e| e.to_string())` 사용, `CoreError`를 plain `String`으로 프런트엔드에 전달. 타입화된 `err.code()`가 이 경계에서 소실. Follow-up: `IpcError { code: String, message: String }` DTO 도입 + callsite 업데이트, 프런트엔드가 display message substring 매칭 대신 `code`로 프로그래밍적 분기 가능. 규모: ~0.5일. 본 ADR과 독립.
2. **Grafana 대시보드 재라벨링** — 로그 파이프라인이 `err.code()` 노출 이후 message-regex 패널을 `code`-label group-by로 교체. ~0.5일.
3. **프런트엔드 i18n 연결** — `err.code()` 문자열을 프런트엔드 i18n 계층에 translation key로 공급. key 누락 시 fallback message 필요. ~1일.
4. **`Internal` 코드 granularity 세분화** — Phase 4 종료 시점에 `InternalCode`는 `Generic`, `Io`, `Serialization` 보유. callsite 1위 variant (`Internal` = ~416 사이트)는 프로덕션 텔레메트리 신호에 따라 추가 세분 가능. 영구 개선 항목.
5. **Sandbox variant 통합** — `SandboxInit` + `SandboxExecution` + `SandboxUnsupported` + `ExecutionTimeout` 의미 중복; 단일 variant로 통합 가능. 별도 리팩토링, 블로킹 아님.
