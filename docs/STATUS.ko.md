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

## 현재 스냅샷 (2026-04-27)

### 버전

v0.4.41-rc.1 (Phase 9 PR-B1 크로스 플랫폼 자동 시작 기반 + PR-B2 Linux 심층 견고성 출시; v0.4.41-rc.1 후보)

### 워크스페이스

- **패키지**: 15개 (`cargo metadata --no-deps` 기준 — `crates/` 하위 14개(`oneshim-sandbox-worker` 포함) + `src-tauri`; 원래 계획된 `oneshim-ui` 및 과거 `crates/oneshim-app/` 은 ADR-004 Tauri 마이그레이션으로 제거됨)
- **SQLite 스키마**: V31 (V31은 Phase 3 `regime_manager_state`에서 추가)

### 워크플로우 상태 (2026-04-27 기준 spot-check)

- **최신 `main` CI**: PR-B2 머지 후 대기 중. PR-B2 이전 main CI는 [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104) (`rustls-webpki 0.103.10` CRL 파싱 panic)로 혼합 상태였으나 PR #526 lockfile-only 0.103.13 bump으로 수정 완료.
- **최신 Release RC**: PR-B2 머지 후 대기 중 (v0.4.41-rc.1 후보). 직전: `v0.4.40-rc.2` 성공 — [Run 24950722387](https://github.com/pseudotop/oneshim-client/actions/runs/24950722387) (2026-04-26 07:02 UTC).
- **최근 stable 태그**: v0.4.40 (2026-04-27). v0.4.40-rc.3에서 `promote-stable.sh`로 stable 승격 완료; v0.4.40-rc.2 승격 대기 절차 종료.

### 로컬 검증 Baseline

- `cargo check --workspace`: 통과
- `cargo clippy --workspace --all-targets -- -D warnings`: 통과
- `cargo test --workspace`: 통과 — **3,906 통과, 0 실패, 24 무시** (마지막 전체 측정은 v0.4.41-rc.1 후보). PR-B2 추가 항목: `lifecycle::migration_hashes` 유닛 테스트 10개 + `lifecycle::sd_notify` 스모크 테스트 2개 = PR-B1 baseline 대비 +12. Linux 전용 인라인 테스트 (`autostart::tests::linux_capability_tests` +3, `linux_autostart_unit` 스모크 +1)는 컴파일 정상이나 Linux CI에서만 실행. 이전 3,798 baseline(v0.4.39-rc.1)에서의 누적 증가: Phase 9 PR-B1 크로스 플랫폼 자동 시작 기반, TimeWindow primitive 통합 (+37), D5 SanitizedDisplay coaching + gui_pipeline 마이그레이션, PR #494/#497/#514 의존성 bump 포함. Phase 2 telemetry 테스트는 `--features telemetry -- --test-threads=1` 로 별도 실행. **3,906에 미포함**: 6 `map_challenge_status_to_error` 테스트 (iter-195 Follow-up #5) 는 `lan-sync` feature gate 뒤에 있어 기본 워크스페이스 실행에 포함되지 않음 — `cargo test -p oneshim-network --features lan-sync --lib sync::lan_transport::auth` 로 별도 실행.
- `cargo fmt --check`: 통과
- `pnpm lint` (`crates/oneshim-web/frontend`): 통과
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): 통과
- 프런트엔드 Vitest (`pnpm test --run` in `crates/oneshim-web/frontend`): 통과 — PR-B2 baseline 측정 시점 **279 통과 / 43 테스트 파일** (PR-B2는 신규 테스트 케이스 없음 — `translateError.test.ts` wire-code count assertion을 49 → 53으로 bump해 신규 `autostart.*` wire 코드 4개 반영; PR-B1 baseline 272/42 파일에서 이후 머지로 증가).

### Phase 2 Telemetry Feature (2026-04-17 추가)

- `cargo test -p oneshim-app --features telemetry -- --test-threads=1`: 통과 — **10 통과** (T-X2-1은 default-build-only이며 위의 워크스페이스 스위트에서 실행됨).
- `cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings ...`: 통과.
- `cargo build --release -p oneshim-app` 의 바이너리 크기 차이 (macOS arm64, 기본 strip 적용): **default 46.4 MB, `--features telemetry` 47.6 MB → +1.2 MB**. 스펙 §7 의 ≤5 MB 목표 대비 충분히 여유.

### 알려진 Flaky / 격리 테스트

- 기본 non-ignored Rust 테스트 스위트에는 알려진 flaky 테스트가 없습니다.
- `oneshim-embedding` fastembed 생성자/embed smoke 테스트는 모델 자산 다운로드가 필요하므로 기본값으로 `ignored` 유지합니다. 네트워크가 보장된 명시적 검증에서만 실행합니다.

### 무시된 테스트 (Ignored Tests)

24개 테스트가 외부 의존성 또는 긴 실행 시간으로 인해 `#[ignore]` 표시됨:

| 크레이트 | 수량 | 사유 |
|---------|------|------|
| oneshim-vision | 7 | macOS 접근성 API (라이브 OS 권한 필요) — Phase 4 Updater Hardening 에서 +1 추가 |
| oneshim-embedding | 3 | Hugging Face 모델 다운로드 |
| oneshim-storage | 3 | 키체인 연동 (macOS 키체인 접근 필요); 뮤텍스 독립 경로는 Phase 5-D8 PR1 전용 테스트로 커버됨 |
| oneshim-network | 2 | 런타임 컨텍스트 필요 doc-test 예제 |
| src-tauri | 8 | GitHub API e2e (2) + 장시간 메모리 프로파일 (3) + Linux systemd 라이브 통합 T8/T9/T10 (3, 사용자 systemd 필요 — workflow_dispatch `linux-systemd-integration.yml`로 실행) |
| oneshim-storage (doc) | 1 | 런타임 컨텍스트 필요 doc-test 예제 |

무시된 테스트 명시적 실행: `cargo test --workspace -- --ignored`

### 릴리스 위생 Baseline

- `CHANGELOG.md`에는 `[Unreleased]` 헤더가 정확히 1개만 있어야 합니다.
- RC 준비와 stable 승격은 `Cargo.lock`의 workspace package 버전을 동기화해야 합니다.
- Release workflow는 build fan-out 전에 `Cargo.toml`, `Cargo.lock`, frontend 버전, changelog hygiene, Tauri 메타데이터를 검증합니다.

## 메모

- 과거 릴리스 노트는 [`CHANGELOG.md`](../CHANGELOG.md)에서 관리합니다.
- GUI V2 마일스톤 이력과 구현 상세는 변동 상태 문서가 아니라 ADR 및 크레이트 문서에서 관리합니다.
