[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# 프로젝트 상태 스냅샷

이 문서는 변경 가능한 품질 신호와 워크플로우 레퍼런스를 요약한 스냅샷 문서입니다.

## 범위

다른 문서에는 아래 값을 하드코딩하지 말고, 이 문서를 링크합니다.

- 최신 전체 CI 상태와 링크
- 최신 릴리스 워크플로우 상태와 링크
- 현재 브랜치 기준 로컬 검증 baseline
- 알려진 flaky 또는 격리된 테스트

## 업데이트 원칙

워크플로우 상태, 검증 baseline, flaky 테스트 상태가 바뀌면 이 문서를 갱신합니다.
실시간 워크플로우 상태의 기준은 GitHub Actions run 페이지입니다.

권장 검증 명령:

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm lint && pnpm build-storybook
```

## 현재 스냅샷 (2026-04-20)

### 버전

v0.4.39-rc.1 (Phase 5-D8 완료; Phase 4 Updater Hardening 출시)

### 워크스페이스

- **패키지**: 15개 (`cargo metadata --no-deps` 기준 — `crates/` 하위 14개(`oneshim-sandbox-worker` 포함) + `src-tauri`; 원래 계획된 `oneshim-ui` 및 과거 `crates/oneshim-app/` 은 ADR-004 Tauri 마이그레이션으로 제거됨)
- **SQLite 스키마**: V31 (V31은 Phase 3 `regime_manager_state`에서 추가)

### 워크플로우 상태 (2026-04-20 기준 spot-check)

- **최신 `main` CI** (`fix(updater): per-PID boot_count markers ...`): 실패 — [Run 24623392589](https://github.com/pseudotop/oneshim-client/actions/runs/24623392589) (2026-04-19). 원인: 크로스 플랫폼 `Build` job 이 `ARTIFACT-DL` 로 `frontend-dist-bundle` artifact 를 다운로드하려 하지만, main push 에서는 `Frontend (build + e2e)` job 이 `skipped` (frontend-touched PR 에서만 실행). 알려진 CI 구성 drift 이며, Rust 테스트 surface 는 green. 본 브랜치 외부에서 트래킹 중.
- **최신 PR 컨텍스트 CI 성공**: 성공 — [Run 24622597161](https://github.com/pseudotop/oneshim-client/actions/runs/24622597161) (2026-04-19, 브랜치 `fix/updater-boot-count-per-pid-markers`). PR CI 는 안정적으로 통과 — artifact gap 은 main-push-only 현상.
- **최신 Release RC**: 성공 (`Release`, `v0.4.38-rc.4`) — [Run 24570428239](https://github.com/pseudotop/oneshim-client/actions/runs/24570428239) (2026-04-17). 직전 v0.4.38-rc.3 실패는 다른 원인.
- **최근 stable 태그**: v0.4.37 (2026-04-12). 현재 브랜치는 ADR-019 PR 랜딩 이후 `promote-stable.sh` 로 v0.4.39-rc.1 → stable 승격 대상.

### 로컬 검증 Baseline

- `cargo check --workspace`: 통과
- `cargo clippy --workspace --all-targets -- -D warnings`: 통과
- `cargo test --workspace`: 통과 — **3,651 통과, 0 실패, 21 무시** (ADR-019 + drift-audit + Follow-up 반영 후 baseline. 이전 2,995에서 누적 증가: Phase 2 +11 default + 11 telemetry-only, Phase 3 regime, Phase 4 Updater Hardening +27, Phase 5-D8 PR1/PR2/PR3 +27, ADR-019 + post-merge drift audit iter 87~214 +196 (HTTP status 매핑 회귀 테스트, Internal→specific re-route, subprocess_kind 분기, LLM envelope 추출 + iter-196 `IpcError` 계약 테스트 +10 등). Phase 2 telemetry 테스트는 `--features telemetry -- --test-threads=1` 로 별도 실행. **3,651에 미포함**: 6 `map_challenge_status_to_error` 테스트 (iter-195 Follow-up #5) 는 `lan-sync` feature gate 뒤에 있어 기본 워크스페이스 실행에 포함되지 않음 — `cargo test -p oneshim-network --features lan-sync --lib sync::lan_transport::auth` 로 별도 실행).
- `cargo fmt --check`: 통과
- `pnpm lint` (`crates/oneshim-web/frontend`): 통과
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): 통과

### 알려진 Flaky / 격리 테스트

- 기본 non-ignored Rust 테스트 스위트에는 알려진 flaky 테스트가 없습니다.
- `oneshim-embedding` fastembed 생성자/embed smoke 테스트는 모델 자산 다운로드가 필요하므로 기본값으로 `ignored` 유지합니다. 네트워크가 보장된 명시적 검증에서만 실행합니다.

### 무시된 테스트 (Ignored Tests)

21개 테스트가 외부 의존성 또는 긴 실행 시간으로 인해 `#[ignore]` 표시됨:

| 크레이트 | 수량 | 사유 |
|---------|------|------|
| oneshim-vision | 7 | macOS 접근성 API (라이브 OS 권한 필요) — Phase 4 Updater Hardening 에서 +1 추가 |
| oneshim-embedding | 3 | Hugging Face 모델 다운로드 |
| oneshim-storage | 3 | 키체인 연동 (macOS 키체인 접근 필요); 뮤텍스 독립 경로는 Phase 5-D8 PR1 전용 테스트로 커버됨 |
| oneshim-network | 2 | 런타임 컨텍스트 필요 doc-test 예제 |
| src-tauri | 5 | GitHub API e2e (2) + 장시간 메모리 프로파일 (3) |
| oneshim-storage (doc) | 1 | 런타임 컨텍스트 필요 doc-test 예제 |

무시된 테스트 명시적 실행: `cargo test --workspace -- --ignored`

### 릴리스 위생 Baseline

- `CHANGELOG.md`에는 `[Unreleased]` 헤더가 정확히 1개만 있어야 합니다.
- RC 준비와 stable 승격은 `Cargo.lock`의 workspace package 버전을 동기화해야 합니다.
- Release workflow는 build fan-out 전에 `Cargo.toml`, `Cargo.lock`, frontend 버전, changelog hygiene, Tauri 메타데이터를 검증합니다.

## 메모

- 과거 릴리스 노트는 [`CHANGELOG.md`](../CHANGELOG.md)에서 관리합니다.
- GUI V2 마일스톤 이력과 구현 상세는 변동 상태 문서가 아니라 ADR 및 크레이트 문서에서 관리합니다.
