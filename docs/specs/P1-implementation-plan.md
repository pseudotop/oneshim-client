# P1 Hexagonal Compliance — Implementation Plan (v2)

Based on spec v2. Codex plan review feedback incorporated.

## Phase 1: P1-1A — IntentPlanner core 승격

### Step 1.1: Create port file
- Create `crates/oneshim-core/src/ports/intent_planner.rs`
- Move `IntentPlanner` trait definition (uses `AutomationIntent` already in core)
- Register in `crates/oneshim-core/src/ports/mod.rs`

### Step 1.2: Update oneshim-automation
- Remove trait definition from `crates/oneshim-automation/src/intent_planner.rs`
- Add explicit re-export: `pub use oneshim_core::ports::intent_planner::IntentPlanner;`
- Keep `LlmIntentPlanner` impl in automation crate (adapter stays)

### Step 1.3: Update src-tauri consumer
- `src-tauri/src/automation_runtime.rs`: change import from `oneshim_automation` to `oneshim_core`

### Step 1.4: Verify
- `cargo check --workspace`

## Phase 2: P1-1B — ClusteringStrategy encapsulation

**Review feedback**: 단순 래퍼 대신 상위 레벨 facade. `Vec<Regime>` 반환.
기존 `RegimeDetector` 이름 충돌 → `RegimeAnalysisFacade` 사용.

### Step 2.1: Create facade
- In `crates/oneshim-analysis/src/regime_analysis_facade.rs`:
  ```rust
  pub struct RegimeAnalysisFacade { /* Box<dyn ClusteringStrategy> 내부 */ }
  impl RegimeAnalysisFacade {
      pub fn new(algorithm: ClusteringAlgorithm) -> Self; // enum, not &str
      pub fn detect_regimes(&self, features: &[RegimeFeatures]) -> Result<Vec<Regime>, CoreError>;
      pub fn recluster_with_constraints(&self, features: &[RegimeFeatures], constraints: &[ClusterConstraint]) -> Result<Vec<Regime>, CoreError>;
  }
  ```
- `classify()`는 노출하지 않음 (이미 `RegimeClassifier`가 담당)
- 반환 타입을 `ClusteringResult` → `Vec<Regime>`으로 올려서 domain 추상화

### Step 2.2: Update src-tauri consumer
- `src-tauri/src/scheduler/mod.rs`: `Box<dyn ClusteringStrategy>` → `RegimeAnalysisFacade`
- `src-tauri/src/scheduler/analysis_pipeline/regime.rs`: facade 메서드 사용
- Remove `ClusteringStrategy` import from src-tauri

### Step 2.3: Verify
- `cargo check --workspace`
- `cargo test -p oneshim-analysis`

## Phase 3: P1-1B — Integration transport assembly factory

**Review feedback**: domain method 위임 X. 조립 전용 factory만 제공.
기존 coordinator 패턴 유지. src-tauri는 trait을 직접 import하지 않음.

### Step 3.1: Create assembly factory
- In `crates/oneshim-network/src/integration/transport_factory.rs`:
  ```rust
  pub struct IntegrationTransportAssembly {
      // 기존 coordinator에서 바로 사용 가능한 타입 조합
      pub session_coordinator: SessionCoordinator,
      pub egress_coordinator: EgressCoordinator,
      pub inbox_coordinator: InboxCoordinator,
  }
  impl IntegrationTransportAssembly {
      pub async fn https(config: &IntegrationConfig, auth: IntegrationAuthContext) -> Result<Self, CoreError>;
  }
  ```
- Factory가 transport + proof_factory를 내부에서 조립하여 coordinator 생성까지 완료
- src-tauri는 coordinator만 받고, transport trait은 알 필요 없음

### Step 3.2: Update src-tauri consumer
- `src-tauri/src/integration_runtime.rs`: transport trait 직접 조립 코드 → `IntegrationTransportAssembly::https()` 호출로 대체
- Remove 4개 transport trait import

### Step 3.3: Verify
- `cargo check --workspace`
- `cargo test -p oneshim-network`

## Phase 4: P1-2 — Service extraction (behavior-preserving)

**Review feedback**: 파일 이동이 아닌 테스트/행동 보존 설계.

### Step 4.1: dashboard_service.rs
- Extract logic to service: `build_daily_digest()`, `compute_statistics()`, parsing helpers
- **daily_digest.rs 의존 수정**: dashboard handler의 `get_dashboard_day()`를 직접 호출하는 대신 `DashboardService`를 공유
- Handler: thin wrapper → service 호출
- **테스트**: module-private helper 테스트를 service module의 `#[cfg(test)]`로 이동
- Add: `test_compute_statistics_basic()` service unit test

### Step 4.2: semantic_search_service.rs
- Extract logic to service: 모드 분기, 키워드/벡터 검색, 점수 계산
- **행동 보존**: 기존 FTS boost 로직을 그대로 이전 (RRF 변경 아님, pure refactor)
- **crate 경계**: `oneshim-analysis` 직접 의존 금지 유지. core port 경유만 허용
- Handler: thin wrapper
- **테스트**: sanitize/mode resolution 테스트를 service로 이동
- Add: `test_keyword_search_basic()` service unit test

### Step 4.3: pomodoro_service.rs
- Extract: duration validation, conflict detection, state transitions, response formatting
- Handler: thin wrapper
- **테스트**: handler 내부 helper 테스트를 service `#[cfg(test)]`로 이동
- Add: `test_start_validates_duration()` service unit test

### Step 4.4: coaching_service.rs
- Extract: coaching engine coordination (goals CRUD + history)
- Handler: thin wrapper
- **테스트**: route-level 테스트는 handler에 유지 (이미 정상). Service unit test 추가.
- Add: `test_get_goals_empty()` service unit test

### Step 4.5: Register services
- Update `services/mod.rs` with 4 new modules

### Step 4.6: Verify
- `cargo check --workspace`
- `cargo test --workspace` (전체 기존 테스트 통과 확인)
- `pnpm build` (frontend 빌드 정상 확인)

## Review Checkpoints

| After Phase | Review |
|-------------|--------|
| Phase 1 | `cargo check` + verify IntentPlanner import path |
| Phase 2 | `cargo check` + verify no ClusteringStrategy in src-tauri |
| Phase 3 | `cargo check` + verify no transport trait in src-tauri |
| Phase 4 | Full `cargo test --workspace` + Codex final review |

## Key Constraints
- **Pure refactoring** — 기존 HTTP API 동작 변경 없음
- **semantic_search**: FTS boost 로직 그대로 이전 (RRF 알고리즘 변경 아님)
- **crate 경계**: `oneshim-web → oneshim-analysis` 직접 의존 금지
- **daily_digest.rs**: dashboard service를 공유하도록 수정 (handler 직접 호출 제거)
