[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# 프로젝트 상태 (단일 소스)

이 문서는 변경 가능한 품질 신호와 워크플로우 레퍼런스의 기준 문서입니다.

## 범위

다른 문서에는 아래 값을 하드코딩하지 말고, 이 문서를 링크합니다.

- 최신 전체 CI 상태와 링크
- 최신 릴리스 워크플로우 상태와 링크
- 현재 브랜치 기준 로컬 검증 baseline
- 알려진 flaky 또는 격리된 테스트

## 업데이트 원칙

워크플로우 상태, 검증 baseline, flaky 테스트 상태가 바뀌면 이 문서를 갱신합니다.

권장 검증 명령:

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm lint && pnpm build-storybook
```

## 현재 스냅샷 (2026-03-30)

### 워크플로우 상태

- 최신 `main` CI: 실패 (`CI`) — [Run 23740715704](https://github.com/pseudotop/oneshim-client/actions/runs/23740715704) (2026-03-30). 원인: `oneshim-embedding` 생성자 커버리지가 live Hugging Face 다운로드에 의존했고 HTTP 504로 실패했습니다.
- 최신 성공 전체 CI: 성공 (`CI`, PR #263) — [Run 23740036667](https://github.com/pseudotop/oneshim-client/actions/runs/23740036667) (2026-03-30).
- 최신 RC 릴리스: 성공 (`Release`, 태그 `v0.4.11-rc.2`) — [Run 23740840957](https://github.com/pseudotop/oneshim-client/actions/runs/23740840957) (2026-03-30).
- 최신 stable 복구 릴리스: 성공 (`Release`, `v0.4.10` workflow_dispatch) — [Run 23732221718](https://github.com/pseudotop/oneshim-client/actions/runs/23732221718) (2026-03-30).

### 로컬 검증 Baseline

- `cargo check --workspace`: 통과
- `cargo clippy --workspace --all-targets -- -D warnings`: 통과
- `cargo test --workspace`: 통과
- `pnpm lint` (`crates/oneshim-web/frontend`): 통과
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): 통과

### 알려진 Flaky / 격리 테스트

- 기본 non-ignored Rust 테스트 스위트에는 알려진 flaky 테스트가 없습니다.
- `oneshim-embedding` fastembed 생성자/embed smoke 테스트는 모델 자산 다운로드가 필요하므로 기본값으로 `ignored` 유지합니다. 네트워크가 보장된 명시적 검증에서만 실행합니다.

### 릴리스 위생 Baseline

- `CHANGELOG.md`에는 `[Unreleased]` 헤더가 정확히 1개만 있어야 합니다.
- RC 준비와 stable 승격은 `Cargo.lock`의 workspace package 버전을 동기화해야 합니다.
- Release workflow는 build fan-out 전에 `Cargo.toml`, `Cargo.lock`, frontend 버전, changelog hygiene, Tauri 메타데이터를 검증합니다.

## 메모

- 과거 릴리스 노트는 [`CHANGELOG.md`](../CHANGELOG.md)에서 관리합니다.
- GUI V2 마일스톤 이력과 구현 상세는 변동 상태 문서가 아니라 ADR 및 크레이트 문서에서 관리합니다.
