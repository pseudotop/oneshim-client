# ADR-019: 에러 코드 인프라 + AWS Bedrock 의도적 미지원

- **상태**: Accepted (2026-04-19)
- **관련**: ADR-001 (에러 전략), ADR-003 (디렉토리 모듈 패턴)
- **구현**: `docs/superpowers/specs/2026-04-19-error-code-infrastructure-design.md`, `docs/superpowers/plans/2026-04-19-error-code-infrastructure.md`

## 배경

ONESHIM client-rust 워크스페이스(14 crate, ~1,150 `CoreError` 생성 사이트)에 에러 코드 컨벤션이 없었다. 텔레메트리(Grafana), i18n(ko/en), 감사 로그 모두 안정적인 머신 리더블 식별자가 필요했다. 별개로 AWS Bedrock이 지원 provider surface로 등록돼 있었으나 구현 미완(Signature V4 인증 없음) — OCR의 `ProviderAuthScheme::AwsSignatureV4` arm에서 조용한 no-auth fallthrough 보안 버그 발생.

두 요구가 만남: 에러 코드 인프라 도입 + Bedrock을 첫 "의도적 미지원" 일급 시민으로 출시.

## 결정

### 1. 에러 코드 인프라

- 18개 code enum(`ConfigCode`, `NetworkCode`, ..., `GuiCode`)을 단일 `define_code_enum!` 매크로로 정의. enum 본체, `as_str` 매치, `Display`, `all()` 열거자를 한 variant 리스트에서 생성.
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

와이어 코드의 `{domain}` 접두사는 보통 Rust enum 카테고리와 일치(예: `ConfigCode::Invalid` → `config.invalid`). `UnsupportedProviderBedrock` variant는 의도적 예외 — `ConfigCode`에 소속되지만(Bedrock-unsupported가 config 로드/요청 빌드 시점에 표면화) 와이어 코드는 `provider.*`로 유지해 관측성 대시보드가 provider-unsupported 신호 전체를 단일 네임스페이스로 그룹핑하고 `config.*` vs `provider.*` 경계를 넘지 않도록 함. 이 예외는 `crates/oneshim-core/src/error_codes/config.rs` 파일 docstring에 기록됨.

### 3. AWS Bedrock: 의도적 미지원

- Bedrock vendor + `provider_surface.bedrock.direct_api` surface를 `specs/providers/provider-surface-catalog.json`에서 **삭제**.
- `oneshim-network` 전반 8개 match arm이 `CoreError::Config { code: ConfigCode::UnsupportedProviderBedrock, .. }` 반환:
  - `ai_ocr_client/mod.rs` (2 arm: auth + BedrockConverse request shape)
  - `ai_ocr_client/strategy.rs` (1 arm: strategy 선택)
  - `ai_llm_client/request.rs` (3 arm: request 빌드 + auth + 응답 파싱)
  - `http_api_session/mod.rs` (2 arm: auth + BedrockConverse request shape — 2번째 arm은 post-merge drift 감사 중 추가; 이전에는 wildcard `_` arm이 BedrockConverse를 `InternalCode::Generic`으로 잘못 라벨링)
- `AiProviderType::Bedrock`, `ProviderAuthScheme::AwsSignatureV4`, `ProviderRequestShape::BedrockConverse` enum variant는 **유지**(catalog 삭제 후 런타임 도달 불가) — 미래 재도입 경로의 surgical diff 확보.
- OCR `apply_auth_headers` 시그니처를 infallible → `Result<_, CoreError>`로 변경해 조용한 no-auth 보안 버그 닫음.
- 위 8개 match arm을 우회하는 sibling 클라이언트 경로에 defense-in-depth 가드 추가 — 모두 병합 후 drift 감사에서 추가, 동일한 `CoreError::Config { code: UnsupportedProviderBedrock, .. }` 반환:
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

아래 모든 follow-up 은 `docs/reviews/2026-04-20-adr019-followup-*.md` 에 design doc 이 존재하며, 종합 로드맵은 `docs/reviews/2026-04-20-adr019-followups-roadmap.md`. 각각 독립 PR 시리즈로 실행 가능하며 ADR-019 머지를 막지 않음.

1. **Tauri IPC code 전파** — ⏳ **인프라 shipped iter-196**. `src-tauri/src/ipc_error.rs` 에 `IpcError { code, message }` + 11 `From` chain impl (`CoreError`, `GuiInteractionError`, 6개 무조건 어댑터 + 2개 feature-gated + 2개 stdlib) + 10 계약 테스트 제공. 남은 작업: 112 command 시그니처를 `Result<_, String>` → `Result<_, IpcError>` 로 단계 마이그레이션 (low-risk → state-mutating → streaming). [ipc-error-dto-design](../reviews/2026-04-20-adr019-followup-ipc-error-dto-design.md) 참조.
2. **Grafana 대시보드 재라벨링** — message-regex 패널을 `err_code` 인덱스 라벨 group-by 로 교체. 설계: `err.code = %e.code()` tracing 필드 + Loki 파이프라인에서 `[code]` 를 label 로 승격 + 패널/알림 마이그레이션. [grafana-relabeling-design](../reviews/2026-04-20-adr019-followup-grafana-relabeling-design.md) 참조. 경과 ~1일.
3. **프런트엔드 i18n 연결** — `err.code()` 문자열을 프런트엔드 i18n 계층에 translation key 로 공급. 설계: 41개 키 en/ko translation resource + `translateError` 헬퍼 + wire-contract snapshot 대비 build-time coverage check. Follow-up #1 에 hard dependency. [frontend-i18n-wiring-design](../reviews/2026-04-20-adr019-followup-frontend-i18n-wiring-design.md) 참조. ~1일.
4. **`Internal` 코드 granularity 세분화** — Phase 4 종료 시점에 `InternalCode` 는 `Generic`, `Io`, `Serialization` 보유. Post-merge drift audit iter 88~109 가 ~122개 Internal emission 을 더 구체적인 variant 로 재라우팅(Config.Missing/Invalid/OutOfRange, NotFound, ServiceUnavailable, InvalidArguments, Analysis, OcrError). 현재 Internal callsite 개수: ~294 (Phase 4 종료 시 ~416). 추가 세분화는 프로덕션 텔레메트리 기반 영구 개선 항목 — Follow-up #2 의 텔레메트리 시그널에 의존.
5. **Sandbox variant 통합** — `SandboxInit` + `SandboxExecution` + `SandboxUnsupported` + `ExecutionTimeout` 의미 중복; 단일 variant 로 통합 가능. 별도 리팩토링, 블로킹 아님.
6. **`sync/lan_transport::authenticate_with_peer` 회귀 테스트** — ✅ **SHIPPED iter-194**. 초기 설계 ([lan-transport-tests-design](../reviews/2026-04-20-adr019-followup-lan-transport-tests-design.md)) 는 `rcgen` + `tokio_rustls::TlsAcceptor` fixture 를 제안했으나, 구현 시 더 단순한 순수 함수 추출 접근을 채택: `crates/oneshim-network/src/sync/lan_transport/auth.rs` 의 인라인 `match status.as_u16()` 을 private `map_challenge_status_to_error(status_code, peer_id) -> CoreError` 헬퍼로 추출 + 6개 unit 테스트 (5개 canonical status 401/403/429/503/504 + 1개 500-fallback). 동일한 커버리지, TLS 테스트 인프라 불필요. `docs/guides/http-status-error-mapping.md` 레지스트리 행을 15/15 tested 로 갱신.

### Post-merge 테스트 커버리지 확장

초기 ADR과 현재 상태 사이에, 14개 (15개 중)의 디스패처에 걸쳐 시맨틱 HTTP status 매핑을 검증하는 85+ 회귀 테스트가 추가됨. 각 테스트는 특정 status code → CoreError variant 매핑을 검증하고, 대부분의 디스패처는 도메인 fallback assertion도 포함. 정식 패턴과 전체 디스패처 레지스트리는 [`docs/guides/http-status-error-mapping.ko.md`](../guides/http-status-error-mapping.ko.md) 참조. 15번째 디스패처(`auth::refresh`)는 iter-98에서 5개 회귀 테스트(401/429/503/504/500)와 함께 추가.

### Post-merge 고아 wire-code 정리 (pre-merge)

최종 drift 감사 중 YAGNI 위배로 declared-but-never-constructed 상태였던 3개 wire code + 2개 `CoreError` variant를 merge 전에 제거:

- `CoreError::BinaryHashMismatch` + `IntegrityCode::HashMismatch` (+ `IntegrityCode` enum 자체) — 바이너리 무결성은 updater 내부 `UpdateError::Integrity`로 처리; 이 `CoreError` variant는 v0.1.0부터 construction site 전무 + `From<UpdateError> for CoreError` 미존재. enum 파일 전체 삭제.
- `CoreError::ProcessNotAllowed` + `PolicyCode::ProcessDenied` — `PolicyDenied`와 redundant (필드 시그니처 동일, display text만 상이). automation 전 경로가 `PolicyDenied` emit; `ProcessNotAllowed` construction site 0건.
- `NetworkCode::Failed` — 연결 레벨 실패용 reserved (docstring 명시) 였으나 wire-up 없음; 모든 non-timeout 네트워크 에러는 `NetworkCode::Generic` 사용. `NetworkCode::Generic`을 canonical fallback으로 유지.

Wire snapshot: 57 → 54 codes (iter-87). Code enum 개수: 19 → 18 (iter-87). 외부 소비자에게 아직 공개되지 않은 wire contract 이므로 merge 전 전량 삭제. 추후 필요 시 일반적인 wire-immutability 절차(append, don't replace) 적용.

이후 iteration 추가 orphan 정리:
- **iter-148**: `GuiCode::Generic` / `gui.generic` — emission site 0건; `GuiInteractionError::Internal`은 항상 `GuiCode::InternalError` 사용. Snapshot 54 → 53.
- **iter-161**: 11개 추가 `*Code::Generic` placeholder variants (audio/config/consent/oauth/permission/policy/provider/secret/service/storage/validation) — 모두 Phase 2 boilerplate이며 Phase 4 완료 이후 emission site 0건. Snapshot 53 → 42.
- **iter-163**: `AuthCode::Generic` / `auth.generic` — iter-161 재감사 시 "1 site" 카운트가 test-only (`TestSessionPort` mock) 임이 확인됨; test는 `AuthCode::Failed`로 전환 ("not authenticated" 의미에 정확히 부합). Snapshot 42 → 41. 유지 Generic variant (2개, 실제 emission 있음): `internal.generic` (workspace-wide Internal fallback, 수백 개 site), `network.generic` (HTTP status fallback, ~70 site).

YAGNI 를 wire code 에서 adapter error type 으로 확장:
- **iter-164**: `NetworkError`에 construction site 0건인 dead variant 5개 (`Serialization`, `OAuth`, `OAuthRefresh`, `Ocr`, `SecretStore`) 존재 — `impl From<NetworkError> for CoreError`의 match arm 들이 실제로 unreachable. variant 5개 + arm 제거로 활성 `NetworkError` 12개 (이전 17개). `StorageError::Core` / `StorageError::Io` 는 명시적 construction 0건이지만 `#[from]` 래핑 + 198개 함수에서 `?`-propagation 으로 사용되므로 정당하게 유지 확인.
- **iter-165**: adapter error 감사를 4개 enum 으로 확장 — 총 10개 dead variant 추가 제거. `AutomationError` × 5 (`Config`, `Io` (automation 내 `?`-propagation 사용처 없음), `PrivacyDenied`, `SandboxUnsupported`, `ServiceUnavailable`; `UserDenied` / `PolicyBlocked` unit variant 은 regression-guard 테스트 `all_policy_denial_variants_share_single_wire_code`가 canonical `policy.denied` 매핑을 문서화하고 dead `ProcessDenied` wire code 재도입 dispatcher drift 를 예방하므로 유지). `VisionError` × 2 (`PermissionDenied`, `ElementNotFound`). `AnalysisError` × 2 (`Internal`, `LlmService`). `SuggestionError` × 1 (`Internal`). 설계 패턴 확립: 모든 adapter error type 에 `#[from] Core` variant 를 `CoreError` composition 의 미래 escape hatch 로 유지 (현재 `?`-propagation callsite 가 없더라도).

Current wire snapshot: **41 codes**.
