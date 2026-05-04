[English](./2026-05-04-phase-6-migration-plan.md) | [한국어](./2026-05-04-phase-6-migration-plan.ko.md)

---
status: Draft
target_phase: 6
companion_strategy: docs/plan/2026-04-30-maekon-client-public-oss-strategy.ko.md
---

# Phase 6 — 모노레포 마이그레이션 계획

> **상태: HISTORICAL (회고).**
> 작성 시점(2026-05-04) 기준 Phase 6는 이미 부모 모노레포에서 커밋
> `f6a91a52a`(2026-04-30)로 **완료된 상태**였습니다 — `clients/maekon-client/`와
> `tools/public-export/maekon-client/`가 이미 존재. 본 문서는 마이그레이션을
> *가정했을 때* 적용되었을 계획 근거(planning rationale)로 보존되며,
> 아래의 게이트별 "현황 (2026-05-04)" 항목은 머지 시점의 실제가 아닌
> 가정상의 사전 상태를 기술합니다. 활성 SSOT는 부모 `clients/maekon-client/`,
> 본 레거시 레포는 archive 예정입니다.

## 1. 목적

`client-rust`를 현재 상태(별도 `pseudotop/oneshim-client` 저장소이며 parent
`pseudotop/oneshim` 모노레포에 Git submodule로 포함됨)에서 parent 내부
single-source-of-truth `clients/maekon-client/`로 옮기고, public export는
전략 문서대로 `tools/public-export/maekon-client/`에서 구동되도록 만드는
구체적이고 순서가 정해진 마이그레이션 단계를 정리한다.

Companion: [`2026-04-30-maekon-client-public-oss-strategy.ko.md`](./2026-04-30-maekon-client-public-oss-strategy.ko.md).
이 문서는 전략 결정을 단계별 액션, 롤백 지점, 검증 게이트로 좁힌다.

## 2. 범위

포함:
- parent의 submodule 제거.
- `client-rust`를 `clients/maekon-client/`로 스냅샷/import.
- `tools/public-export/maekon-client/` 구축 (2026-05-04 결정에 따른 export
  도구 Option B).
- `client-rust`를 언급하는 90개 parent 파일에 대한 경로 참조 sweep.
- 새 export 도구로의 cut-over 및 병행 운영 검증 기간.

미포함 (Phase 7 이후로 연기):
- `maekon-client`의 공개 릴리스 태깅 및 notarization.
- 최종 `oneshim-client` archive 공지.
- `maekon.dev` / `docs.maekon.dev` 배포 단위.
- 공개 저장소의 Maekon hard-reset.

## 3. 사전 조건 (진입 게이트)

| # | 게이트 (전략 §260) | 상태 (2026-05-04) | 필요 검증 |
|---|----------------------|---------------------|------------|
| 1 | parent main 안정, 최신 `origin/main` 구조 재검토 | 🟡 부분 — open PR 0건, ADR-070 R-FU-5 + ADR-067 #6 시리즈 PR #1025에서 정착; cadence는 활발 (DPoP + UX 작업 진행 중) | 사용자 측 판단 |
| 2 | parent의 기존 `client-rust` submodule 워크플로 docs 식별 완료 | ✅ parent 90개 파일 참조 매핑 완료; `.github/workflows/`에는 0건 | — |
| 3 | `clients/maekon-client/`가 parent의 `server`/`backoffice`/`docs` 구조를 교란하지 않음 | 🟡 사전 점검 — `clients/`, `tools/` 디렉토리 아직 존재하지 않음 | Step 2 dry-run |
| 4 | export 도구가 parent source path 수용 | ✅ Option B 확정 (`tools/public-export/maekon-client/`에 신규 도구 구축); 기존 `client-rust/scripts/export-public-repo.sh`는 cut-over 시점까지 병행 작동 | Step 5 cut-over |
| 5 | public export 게이트가 parent-only 경로 누설 차단 | ✅ 기존 `forbidden_paths`가 이미 `server`, `backoffice`, `terraform`, `tests/private`을 포함; `tools/public-export/maekon-client/exclude.txt`로 포팅 예정 | Step 2 smoke 비교 |
| 6 | Maekon/ONESHIM 관계 카피를 README, 설치 docs, 로그인/OAuth, 업데이트 docs에 적용 가능 | ⚠️ 감사 보류 | Step 6 |
| 7 | 공개 컨트리뷰션 처리 카피 준비 | ⚠️ `SECURITY.md`는 maekon-client에 이미 존재; `CONTRIBUTING.md` 갭은 감사 예정 (Phase 6 prep follow-up #4) | Step 7 |
| (운영) | parent local debug + release mode 검증 | ❌ 사용자 측 검증 보류 | 사용자 신호 |

## 4. 마이그레이션 단계

단계는 순서대로 진행한다. Step N의 검증 게이트가 통과되기 전에는 Step N+1을
시작하지 않는다.

### Step 1 — `client-rust` SSOT 사전 작업

목표: Step 3을 재현하거나 롤백할 수 있도록 known-good 베이스라인을 확보.

액션:
- 마지막 sync round 1회 실행 (`oneshim-client` → `maekon-client`)으로 public
  mirror를 동일 commit baseline에 핀. (Phase 6 진입 시점에 직전 sync 이후
  상위 PR이 추가되지 않았다면 이미 충족된 상태일 수 있다.)
- `client-rust`에 마이그레이션 commit으로 `phase-6-baseline-YYYYMMDD` 태그를
  부착. parent submodule pointer는 이 commit을 참조해야 한다.
- 베이스라인 캡처:
  - `git -C client-rust rev-parse HEAD`
  - `git -C oneshim ls-tree HEAD client-rust | awk '{print $3}'`
  - `cargo deny check`, `cargo clippy`, `cargo test` 결과 기록.
- 베이스라인 기준 dry-run export 1회 실행, 출력 트리(~30 MB, ~3,600 파일) 저장
  → Step 2 비교용.

게이트: 베이스라인 태그 push, 검증 결과 기록, dry-run output 저장 완료.

### Step 2 — `tools/public-export/maekon-client/` 구축 (parent)

목표: 동일 입력에 대해 검증된 상태로 Option B를 기존 client-side 도구와
병행해 세운다.

parent에 만들 디렉토리 구조:

```
tools/public-export/maekon-client/
├── README.md           # 운영 가이드 (sync round 절차)
├── export.sh           # entry point; client-rust/scripts/export-public-repo.sh 로직을 미러링
├── exclude.txt         # client-rust/scripts/public-repo-exclude.txt에서 포팅
└── overlays/
    └── .github/
        └── ISSUE_TEMPLATE/
            ├── bug_report.yml
            ├── config.yml
            ├── feature_request.yml
            └── install_release_issue.yml
```

기존 도구 대비 주요 조정:
- `REPO_ROOT`가 parent 모노레포 root를 가리킨다.
- 소스 archival은 `git archive HEAD:clients/maekon-client/ | tar -x …`로 서브트리
  스코프 (Step 3 완료 전까지는 부분 충족 — 그 사이 `export.sh`는 기존 submodule
  경로에서 작동하며 `--source-path` override 플래그로 Step 2 smoke 테스트를
  수행한다).
- `validate_public_export()`는 거의 그대로 포팅 — required/forbidden path
  목록, 내부 참조 스캔, stale public repo 참조 스캔 모두 동일.
- ISSUE_TEMPLATE overlay 자동화: rsync 후
  `tools/public-export/maekon-client/overlays/.github/`를 destination에 복사.
  `project_maekon_sync_workflow` 메모리에 기록된 수동 우회를 제거.

검증:
- Step 1 baseline에 대해 Step 2 export 실행.
- 출력을 Step 1 저장 트리와 `diff -ru` 비교. 기대값: overlays 자동화(이번 단계
  추가)를 제외하고 동일.
- 비자명한 차이가 있으면 Step 3 진입 전 디버그.

게이트: Step 1 dry-run 대비 동일 또는 strictly-additive 출력.

### Step 3 — `client-rust`를 `clients/maekon-client/`로 import

목표: submodule을 parent 내부 디렉토리로 전환.

선택 가능한 import 전략 3종 (결정 필요, §7 참조):

#### 3a — Subtree merge (히스토리 보존, 권장 기본)

```bash
# parent oneshim/에서
git remote add client-rust-import https://github.com/pseudotop/oneshim-client.git
git fetch client-rust-import main
git merge --allow-unrelated-histories -s ours --no-commit client-rust-import/main
git read-tree --prefix=clients/maekon-client/ -u client-rust-import/main
git submodule deinit -- client-rust
git rm -f client-rust
# .gitmodules에서 client-rust 항목 제거 (또는 빈 파일 자체 삭제)
git commit -m "feat(monorepo): import client-rust as clients/maekon-client (subtree)"
```

장점: `git log clients/maekon-client/`로 전체 히스토리 도달 가능.
단점: 초기 commit이 큼; merge base에서 히스토리가 섞인다.

#### 3b — Snapshot 복사 (히스토리 없음)

```bash
# parent oneshim/에서
git submodule deinit -- client-rust
rsync -a --exclude='.git/' client-rust/ clients/maekon-client/
git rm -f client-rust
# .gitmodules에서 client-rust 항목 제거
git add clients/maekon-client/ .gitmodules
git commit -m "feat(monorepo): import client-rust as clients/maekon-client (snapshot)"
```

장점: 깔끔한 diff, 리뷰 용이.
단점: `oneshim-client` 출신 코드의 `git log`/`git blame` 연속성 없음.

#### 3c — `git filter-branch` / `git-filter-repo` (전체 히스토리 + prefix 재작성)

가장 무거운 선택지. blame/log 연속성이 필수일 때만 사용.

각 옵션 검증:
- parent root에서 `cargo check --workspace` 성공.
- `clients/maekon-client/Cargo.toml`이 유효.
- submodule pointer 제거 (`.gitmodules`에 `[submodule "client-rust"]` 부재).
- submodule 디렉토리 제거 (트리에서 `client-rust/`가 더는 존재하지 않음).

게이트: 선택한 import 전략 적용 완료, parent가 `clients/maekon-client/`로
컴파일.

### Step 4 — parent 전반 경로 참조 sweep

목표: 2026-05-04 submodule 스캔으로 식별된 90개 parent 파일 참조를 갱신.

분류와 접근:
- ADR plan/spec docs (`server/docs/plans/*.md`,
  `server/docs/domains/.../README.md` 등): `client-rust/` →
  `clients/maekon-client/` 일괄 `sed` 재작성.
- `.claude/agents/{rust-core-owner,rust-runtime-owner,qa-gatekeeper}.md`:
  agent 텍스트가 단순 경로가 아닌 워크플로를 언급하므로 수동 편집.
- 최상위 `CLAUDE.md`, `README.md`, `SECURITY.md`, `tests/CLAUDE.md`,
  `server/CLAUDE.md`: 사용자 노출 면이라 수동 편집.
- `tests/private/client-rust/`:
  - 결정 사항(§7): `clients/maekon-client/tests/private/`로 이동 또는
    `tests/private/client-rust/` 그대로 두고 `tests/private/maekon-client/`
    로 이름 변경.
  - run script (`run.sh`, `run-frontend.sh`, `run-e2e-tauri.sh`,
    `run-e2e-live.sh`)는 submodule 경로를 직접 참조 — 반드시 갱신.
- `.gitmodules`: `[submodule "client-rust"]` 블록 제거 (Step 3에서 3a/3b를
  따랐다면 이미 제거됨).

검증:
- parent 전반에서
  `grep -rn "client-rust" --include="*.md" --include="*.sh" --include="*.yml"
  --include="*.yaml" --include="*.json" --include="*.toml"` 결과가 의도된
  히스토리 언급(CHANGELOG, archived notes)만 남는다.
- `tests/private` run script가 새 경로로 smoke 실행 성공.

게이트: 의도되지 않은 `client-rust` 참조 0건; tests/private script smoke
통과.

### Step 5 — 공개 export cut-over

목표: sync round 절차를 `client-rust/scripts/export-public-repo.sh`에서
`tools/public-export/maekon-client/export.sh`로 전환.

절차:
- 두 도구로 sync round를 병행 1~2회 수행, `maekon-client` PR diff가 동일한지
  확인. (Step 2 smoke 비교가 깨끗했다면 형식적 절차 — 다만 병행 실행이
  cut-over의 강제 함수다.)
- `project_maekon_sync_workflow` 메모리를 새 도구 경로로 갱신, ISSUE_TEMPLATE
  수동 overlay 단계 제거.
- `docs/guides/public-repo-launch-playbook.md` (parent + 모든 client-rust 사본)
  를 새 도구 위치로 갱신.
- (이제 이전된) `clients/maekon-client/scripts/export-public-repo.sh`에
  새 도구 경로를 echo하고 `exit 1`하는 deprecation banner 추가. 스크립트
  완전 제거는 Phase 7로 연기.

검증:
- Cut-over 이후 sync round N+1이 병행 실행과 동등한 diff signature 생성.
- 메모리 + playbook 갱신 머지.

게이트: cut-over commit 머지; deprecation 공지 활성; client-side 스크립트
잔존 사용자 0.

### Step 6 — Maekon 카피 sweep

목표: 전략 §90의 Maekon/ONESHIM 관계 카피를 사용자 노출 면에 적용.

감사 면:
- `clients/maekon-client/README.md`
- `clients/maekon-client/docs/guides/install*.md`
- 로그인/OAuth UI 문자열 (frontend i18n)
- 업데이트 플로우 docs + 프롬프트
- 공개 릴리스 노트 템플릿

검증:
- 사용자 spot-check; 카피를 전략 §90 대조.

게이트: 사용자 카피 sign-off.

### Step 7 — 공개 컨트리뷰션 템플릿

목표: Follow-up #4 (템플릿) 갭 해소.

액션:
- 현재 `maekon-client` 내용 감사: `SECURITY.md` ✅, ISSUE_TEMPLATE ✅
  (Step 2 overlays 경유).
- 공개 측에 `CONTRIBUTING.md`가 부재하면 골격 작성.
- 다음 sync 이후 공개 미러에 `dependabot.yml`, `CODE_OF_CONDUCT.md`, license
  파일이 모두 최신 상태로 존재하는지 확인.

검증:
- Step 5 cut-over 이후 공개 repo diff에 새 템플릿이 깔끔히 반영.

게이트: maekon-client가 컨트리뷰션-ready (SECURITY, CONTRIBUTING,
ISSUE_TEMPLATE, CODE_OF_CONDUCT, LICENSE 모두 일관).

## 5. 롤백 시나리오

### Step 2 (export 도구 구축) 단계 롤백
- `tools/public-export/maekon-client/` WIP 브랜치 폐기.
- 기존 `client-rust/scripts/export-public-repo.sh`가 sync round를 계속 처리.
  사용자 노출 영향 없음.

### Step 3 (import 중간) 단계 롤백
- parent에 `git reset --hard <pre-import-commit>`.
- submodule 복원: `git submodule add` + `client-rust`를 baseline 태그에 동기화.
- submodule 활성 상태에서 parent의 `cargo check --workspace` 성공 확인.

### Step 4 (경로 sweep) 단계 롤백
- 경로 갱신 commit 되돌리기.
- submodule은 여전히 제거 상태; clients/maekon-client/도 그대로; 참조만 롤백.
  실제 코드 경로가 정확하므로 parent CI는 정상 동작 — 롤백은 sweep으로
  유발된 다운스트림 docs/참조 회귀를 정리하는 용도.

### Step 5 (cut-over) 단계 롤백
- `client-rust/scripts/export-public-repo.sh` (이제
  `clients/maekon-client/scripts/`로 이전) 재활성화.
- 메모리 + playbook을 옛 도구 경로로 되돌림.
- 병행 실행 회귀 진단 전까지 sync round는 레거시 도구로 재개.

### Hard 롤백 (Step 3 이후, full)
- 예외 처리: baseline 태그에 submodule pointer 재생성, `clients/maekon-client/`
  에서 import 이후 commit이 있다면 `oneshim-client`로 cherry-pick, 그 다음
  `clients/maekon-client/` + `tools/public-export/maekon-client/` 삭제.
- 비용이 크므로 Step 2/5 병행 실행 검증을 통해 이 경로를 회피한다.

## 6. 리스크 레지스터

| 리스크 | 심각도 | 가능성 | 완화 |
|--------|--------|--------|------|
| 3b 선택 시 히스토리 손실 | 중 | 3b 선택 시 높음 | 사용자가 깔끔한 diff를 명시 선호하지 않으면 3a 기본값 |
| parent docs에 stale `client-rust/` 참조 잔존 | 저 | 중 | grep sweep + CI 가드 (CHANGELOG 외에서 `client-rust/`가 발견되면 fail하는 parent CI job) |
| Step 3 후 parent test 회귀 | 중 | 저 | import 직후 `cargo check --workspace` + 전체 테스트 스위트 실행 |
| `tests/private/client-rust/` script 손상 | 저 | 중 | Step 4와 동일 PR에서 script 갱신 |
| 외부 도구 참조 (Doppler, GitHub Actions) 손상 | 저 | 매우 낮음 | `.github/workflows/`에 client-rust 참조 0건 — 검증 완료 |
| Cut-over 시 공개 sync round 회귀 | 중 | 저 | 옛 도구 deprecate 전 병행 실행 1~2회 |
| 사용자 흐름 교란 (developer git clone) | 저 | 저 | submodule 전환은 내부 사항; 다운스트림 사용자는 새 경로만 본다 |
| Step 3 중 롤백으로 작업 손실 | 중 | 저 | Step 1 baseline 태그 + dry-run export 저장이 명시적 복구 지점 |
| auto-update 사용자의 GitHub Releases 연속성 | 저 | 매우 낮음 | maekon-client 릴리스는 별도 트래킹; oneshim-client archive는 Phase 7 연기 |

## 7. 미해결 결정 사항 (사용자 입력 필요)

다음 결정은 특정 단계를 차단한다. 결정 시점은 해당 단계 시작 전이어야 한다.

1. **Step 3 import 전략** — 3a (subtree, 히스토리 보존), 3b (snapshot,
   클린), 3c (filter-repo, full prefix 재작성). 기본 추천: **3a**.
2. **`tests/private/client-rust/` 이동 위치** — `clients/maekon-client/tests/private/`
   (코드와 co-locate) vs. `tests/private/maekon-client/` (parent가 private QA
   소유). 기본 추천: **co-locate** — `clients/maekon-client/` 하위.
3. **Maekon landing/docs 위치** (전략 Follow-up #5) — maekon-client repo
   내부, 별도 `pseudotop/maekon-landing` repo, 또는 다른 배포 단위. Step 6
   대상 면에 영향.
4. **CONTRIBUTING.md 깊이** — 최소 (보안 정책 링크 + PR 매너) vs. 상세
   (빌드 절차, 테스트, 코드 스타일).
5. **submodule `.gitmodules` 정리** — `client-rust` 항목 완전 제거 vs.
   `# Removed in Phase 6 (YYYY-MM-DD)` 히스토릭 코멘트 보존.

## 8. 의존성 및 병렬 실행

```
Step 1 (사전 작업)
  └─ Step 2 (tools/public-export/maekon-client 구축) ───┐
  └─ Step 3 (clients/maekon-client로 import) ───────────┤
       └─ Step 4 (경로 sweep) ─── Step 6과 병행 가능   │
            └─ Step 5 (export 도구 cut-over) ←─────────┘
                 └─ Step 6 (Maekon 카피 sweep)
                      └─ Step 7 (CONTRIBUTING.md + 템플릿)
```

Step 2는 Step 1 baseline 캡처 직후 시작 가능; Step 3을 기다리지 않는다.
Step 6과 Step 7은 다른 면을 다루므로 Step 4와 병행 시작 가능.

## 9. 추정 작업량

| Step | 추정 | 비고 |
|------|------|------|
| 1 — 사전 작업 | 30분 | 태그, 베이스라인 캡처, dry-run export |
| 2 — Export 도구 포팅 | 4–6시간 | 베이스라인 대비 smoke 비교 포함 |
| 3 — Import | 1–2시간 | 선택 전략에 따라 다름; 3a가 가장 김 |
| 4 — 경로 sweep | 2–4시간 | 90개 파일, 대부분 기계적 작업 |
| 5 — Cut-over | 1시간 | 병행 실행 검증 후 절차적 작업 |
| 6 — Maekon 카피 | 4–8시간 | 콘텐츠 감사 + 작성 |
| 7 — 템플릿 | 2–3시간 | CONTRIBUTING.md + 점검 |
| **합계** | **~14–24시간** | 여러 세션에 분산 |

## 10. 성공 기준

- parent `oneshim` main이 `clients/maekon-client/` 디렉토리를 ship하면서 CI
  또는 로컬 빌드 회귀 없음.
- `tools/public-export/maekon-client/export.sh`로 구동된 sync round가 round
  6 베이스라인과 동일한 형태의 `maekon-client` PR을 생성 (의도된 overlay
  자동화 제외).
- 90개 parent `client-rust` 참조 모두 `clients/maekon-client/`로 갱신
  되었거나 CHANGELOG / 히스토릭 노트로 의도적으로 아카이브됨.
- `pseudotop/maekon-client` 미러가 ISSUE_TEMPLATE 수동 overlay 단계 없이
  계속 sync round를 받음.
- `cargo check --workspace`, `cargo clippy --workspace`, parent main 전체
  테스트 스위트 green.
- 사용자가 `git clone pseudotop/oneshim` 후 `clients/maekon-client/`에서
  `cargo run -p oneshim-app` 깔끔히 실행.
- 메모리, playbook, CLAUDE.md docs가 새 구조를 반영.

## 11. 참고

- [`docs/plan/2026-04-30-maekon-client-public-oss-strategy.ko.md`](./2026-04-30-maekon-client-public-oss-strategy.ko.md) — 이 계획이 구현하는 전략 문서.
- [`docs/guides/public-repo-launch-playbook.ko.md`](../guides/public-repo-launch-playbook.ko.md) — 현재 sync round 절차 (Step 5에서 갱신 예정).
- `scripts/export-public-repo.sh` + `scripts/public-repo-exclude.txt` —
  Option B가 복제할 현재 client-rust-side 도구.
- 2026-05-04 sync round 5/6 PR (`pseudotop/maekon-client#29`, `#30`) —
  레거시 도구로 실행한 마지막 라운드; Step 2 smoke 비교의 베이스라인.
