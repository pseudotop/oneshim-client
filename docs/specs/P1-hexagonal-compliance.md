# P1 Hexagonal Architecture Compliance — Spec (v2)

## Problem Statement

Codex 전체 코드베이스 리뷰에서 2건의 P1 Hexagonal Architecture 위반이 발견됨.

### P1-1: Cross-crate contract traits outside oneshim-core

ADR-001 §7: cross-crate contract trait은 `oneshim-core/src/ports/`에 정의되어야 함.
현재 6개 trait이 adapter crate에 정의되어 있으면서 src-tauri에서 직접 소비됨.

| Trait | Source Crate | Consumer | Methods |
|-------|-------------|----------|---------|
| `IntentPlanner` | oneshim-automation | src-tauri/automation_runtime | 1 |
| `ClusteringStrategy` | oneshim-analysis | src-tauri/scheduler | 4 |
| `IntegrationTransportClient` | oneshim-network | src-tauri/integration_runtime | 3 |
| `IntegrationRequestProofFactory` | oneshim-network | src-tauri/integration_runtime | 1 |
| `IntegrationEgressTransportClient` | oneshim-network | src-tauri/integration_runtime | 1 |
| `IntegrationInboxTransportClient` | oneshim-network | src-tauri/integration_runtime | 2 |

### P1-2: Thick web handlers (ADR-009 thin handler 규칙 위반)

4개 핸들러에서 비즈니스/오케스트레이션 로직이 직접 구현됨:

| Handler | Code LOC | 위반 내용 |
|---------|----------|----------|
| `dashboard.rs` | ~240 | Digest 생성, 통계 계산, 캐시 |
| `semantic_search.rs` | ~190 | 임베딩, 하이브리드 랭킹 |
| `pomodoro.rs` | ~130 | 타이머 상태 머신, 검증 |
| `coaching.rs` | ~150 | 코칭 엔진 직접 조회 |

## Scope

### In Scope
- P1-1: trait leak 해소 (2가지 전략 혼합)
- P1-2: 4개 핸들러의 서비스 추출 + WebContext 연동

### Out of Scope
- P2 이슈 (error strategy, async_trait 누락, file split, updater ADR-004)
- P3 이슈 (contract test)

## Approach

### P1-1: Trait Leak Resolution (differentiated strategy)

Codex spec review 피드백 반영 — 6개 모두 core 승격이 아닌 차등 처리.

#### A. Core 승격 (1건): `IntentPlanner`
- `AutomationIntent`는 이미 `oneshim-core/src/models/intent.rs`에 존재
- `IntentPlanner` trait만 `oneshim-core/src/ports/intent_planner.rs`로 이전
- `oneshim-automation`에서 explicit re-export: `pub use oneshim_core::ports::intent_planner::IntentPlanner;`
- src-tauri에서 `oneshim_core::ports::IntentPlanner`로 import 변경

#### B. Composition-root encapsulation (5건): ClusteringStrategy + Integration transport
- **ClusteringStrategy**: src-tauri/scheduler가 `Box<dyn ClusteringStrategy>`를 직접 들고 있는 것을 `oneshim-analysis`가 제공하는 opaque `RegimeDetector` builder로 감쌈. src-tauri는 builder 결과물만 받고, trait 자체를 import하지 않음.
- **Integration transport 4개**: src-tauri/integration_runtime이 개별 trait을 직접 잡는 대신, `oneshim-network::integration`이 제공하는 `IntegrationTransportBundle` opaque struct로 묶음. src-tauri는 bundle만 받고 내부 trait을 알 필요 없음.

**효과**: adapter 내부 seam이 composition-root에 노출되지 않으므로 ADR-001 §7 위반 해소. core 비대화 방지.

### P1-2: Service Extraction (ADR-009 pattern)

**Pattern**: `State(WebContext) → Service → Assembler/Storage`

#### Context 전략
- dashboard/coaching/pomodoro: 기존 `StorageWebContext` 또는 `AppState` 내 port를 활용
- semantic_search: `EmbeddingWebContext` 신규 생성 (storage + embedding port)
- **주의**: `oneshim-web → oneshim-analysis` 직접 의존 금지 유지. semantic_search service는 oneshim-core port 경유로만 embedding 접근.

#### 추출할 서비스 4건
1. `dashboard_service.rs` — `build_daily_digest()` + `compute_statistics()` + 캐시 조율
2. `semantic_search_service.rs` — 모드 분기 + 키워드/벡터 검색 조합 + RRF 점수 계산
3. `pomodoro_service.rs` — 세션 상태 머신 (start/cancel/complete) + 검증
4. `coaching_service.rs` — 코칭 엔진 래퍼 (goals CRUD + history 조회)

#### 테스트 전략
- 각 서비스에 최소 1개 unit test (happy path)
- 기존 handler 테스트는 service 호출 검증으로 전환
- route-level integration test는 기존 것 유지 (API regression 방지)

## Non-goals
- `oneshim-web → oneshim-analysis` crate 의존 추가 (금지)
- `daily_digest.rs`, `recalibration.rs` 변경 (현 상태 유지)
- `ClusteringStrategy`/transport trait을 oneshim-core로 이전 (composition-root encapsulation으로 대체)

## Risks

| Risk | Mitigation |
|------|-----------|
| Core 비대화 | IntentPlanner만 이전, 나머지는 encapsulation |
| semantic_search crate 경계 침범 | core port 경유 강제, 직접 의존 금지 |
| 서비스 추출 시 API regression | 기존 route-level test 유지 + service unit test 추가 |
| Integration transport bundle이 유연성 제한 | bundle에 builder pattern 적용, 필요시 개별 접근 가능 |

## Success Criteria

- [ ] src-tauri에서 adapter crate의 trait을 직접 `use`하는 코드 0건
- [ ] 모든 핸들러 함수 ≤30 LOC (테스트 제외)
- [ ] `cargo check --workspace` 통과
- [ ] `cargo test --workspace` 기존 테스트 전부 통과
- [ ] 기존 HTTP API 동작 변경 없음 (pure refactoring)
- [ ] 각 신규 서비스에 최소 1개 unit test
