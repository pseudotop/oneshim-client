[English](./2026-04-30-maekon-client-public-oss-strategy.md) | [한국어](./2026-04-30-maekon-client-public-oss-strategy.ko.md)

# Maekon Client 공개 OSS 전략

**Date**: 2026-04-30
**Status**: Draft
**Scope**: Maekon Client 공개 전략, parent repo 편입 전략, public export 운영
**Related**: `docs/guides/public-repo-launch-playbook.ko.md`, `scripts/export-public-repo.sh`, `scripts/public-repo-exclude.txt`

## 목적

Maekon Client는 신뢰성을 위해 오픈소스로 공개한다. 사용자는 공개 릴리스 클라이언트를 설치하고, 필요할 때 선택적으로 ONESHIM Platform과 연동한다.

이 문서는 Maekon Client를 공개 저장소처럼 운영하면서도, parent 프로젝트의 내부 서버, SaaS 운영, private test, roadmap, 인프라, 조직 운영 문맥을 공개하지 않기 위한 기준을 정리한다.

## 배경

현재 `client-rust`는 별도 repository/submodule로 관리되고 있다. 장기적으로는 parent 프로젝트의 일부로 편입하되, 전체 parent를 완전한 root-level monorepo 구조로 재편하는 것은 현재 범위가 아니다.

사용하려는 운영 모델은 순수 OSS가 아니라 부분 공개 모델이다.

- Maekon Client는 공개 가능한 로컬 클라이언트다.
- ONESHIM Platform은 선택적 managed platform이다.
- 공개 저장소는 신뢰와 투명성을 제공한다.
- 유료/비공개 가치는 팀 운영, 동기화, 정책, 감사, 관리형 인프라, 엔터프라이즈 지원에 둔다.

## 결정 사항

### 1. Canonical source는 parent 내부에 둔다

parent 편입 후 Maekon Client의 canonical source of truth는 다음 경로를 목표로 한다.

```text
clients/
  maekon-client/
```

`apps/maekon-client`는 현 단계에서 사용하지 않는다. root-level `apps/`를 만들면 server, backoffice, docs-site까지 모두 같은 IA로 재편할지 결정해야 하며, 현재 의도한 범위를 넘어선다.

`clients/maekon-client`는 client-centric SSOT를 표현하면서도 parent 전체 구조 재편을 강제하지 않는다.

### 2. Public repo는 두 번째 원본이 아니다

`pseudotop/maekon-client`는 공개 저장소이지만 개발 원본은 아니다. public repo는 vetted internal source에서 생성되는 curated export target으로 취급한다.

```text
private parent repo
  clients/maekon-client/          # SSOT

public GitHub repo
  pseudotop/maekon-client         # generated/exported public surface
```

public repo에서 발견된 이슈나 외부 PR은 parent SSOT에 반영한 뒤 다시 export한다. public repo에 직접 기능 개발을 누적하지 않는다.

예외적으로 public-only metadata, GitHub repository setting, issue template 등은 public repo에서 직접 조정할 수 있다. 단, 반복 가능한 항목은 가능한 한 export overlay로 되돌려 관리한다.

### 3. Open-source 운영물은 source tree가 아니라 export policy로 관리한다

tracked `opensource/maekon-client` 복제본은 만들지 않는다. 같은 소스가 parent 안에 두 번 존재하면 어떤 쪽이 원본인지 흐려지고, stale copy 위험이 커진다.

대신 공개 운영물은 다음처럼 분리한다.

```text
clients/
  maekon-client/                  # SSOT

tools/
  public-export/
    maekon-client/
      export.sh
      include.txt
      exclude.txt
      required-paths.txt
      forbidden-patterns.txt
      overlays/
        README.md
        SECURITY.md
        CONTRIBUTING.md
        .github/

.public-worktrees/
  maekon-client/                  # gitignored generated checkout
```

`tools/public-export/maekon-client`는 무엇을 공개하고 무엇을 제외할지 정의하는 내부 운영 계층이다.

`.public-worktrees/maekon-client`는 실제 public repo checkout 또는 export 결과물을 검증하는 로컬 작업 디렉터리다. 이 경로는 parent git에 포함하지 않는다.

### 4. 브랜드 관계를 명시한다

Maekon과 ONESHIM의 관계는 다음 문장으로 고정한다.

> Maekon Client is the transparent, open-source local client. ONESHIM Platform is the optional managed platform for sync, teams, automation governance, and enterprise operations.

한국어 설명은 다음과 같이 둔다.

> Maekon Client는 투명하게 공개되는 오픈소스 로컬 클라이언트입니다. ONESHIM Platform은 동기화, 팀 운영, 자동화 거버넌스, 엔터프라이즈 운영을 위한 선택적 관리형 플랫폼입니다.

앱, README, 설치 문서, OAuth 화면, 로그인 문구, 업데이트 문서, 보안 문서는 이 관계를 일관되게 설명해야 한다.

### 5. 공개 범위와 비공개 범위를 분리한다

| 영역 | 공개 여부 | 기준 |
| --- | --- | --- |
| Maekon Client source | 공개 | 로컬 클라이언트 신뢰성의 핵심 |
| 설치/릴리스/보안 문서 | 공개 | 사용자가 바이너리와 업데이트 경로를 검증할 수 있어야 함 |
| 로컬 API contract | 공개 | 클라이언트 통합 경계 설명에 필요 |
| parent server | 비공개 | ONESHIM Platform 내부 구현 |
| SaaS 운영/infra | 비공개 | 배포, 운영, 보안 경계 |
| private tests | 비공개 | 내부 시나리오와 비공개 환경 포함 가능 |
| roadmap/spec draft | 비공개 | 공개 약속으로 오해될 수 있는 계획 문서 |
| docs/plan, docs/specs, docs/reviews | 비공개 | 내부 의사결정과 검토 기록 |

현재 `scripts/public-repo-exclude.txt`는 `docs/plan`, `docs/specs`, `docs/reviews`, `docs/research`, `docs/roadmap`, `docs/migration`, `tests/private` 등을 제외한다. 이 문서도 `docs/plan`에 위치하므로 public export 대상이 아니다.

### 6. 수익화 경계는 개인 기능 제한보다 운영 가치에 둔다

Maekon Client 공개의 목적은 신뢰다. 따라서 공개 클라이언트가 설치 후 실질적으로 유용해야 한다.

공개/무료 영역:

- 로컬 캡처와 로컬 분석
- 로컬 대시보드
- 기본 설정과 privacy controls
- 로컬 자동화의 안전한 기본 흐름
- 공개 install/build/security 문서

유료/비공개 또는 managed platform 영역:

- ONESHIM 계정 기반 동기화
- 팀/조직 정책
- 중앙 감사 로그와 보존 정책
- SSO/SCIM, RBAC, compliance export
- managed LLM/OCR routing
- enterprise support, SLA, managed updates

초기 전략은 OSI-approved open-source license를 기본 전제로 삼는다. source-available 또는 anti-free-riding license는 지금 단계의 기본값으로 사용하지 않는다. 라이선스 최종 선택은 Apache-2.0/MIT/AGPL 후보를 대상으로 별도 결정한다.

## Landing/docs 운영 전략

Maekon 공개 후 landing/docs는 parent ONESHIM 문서와 섞지 않는다.

권장 public surface:

```text
maekon.dev
  - Maekon Client 소개
  - 다운로드
  - GitHub 링크
  - ONESHIM Platform 연동 설명

docs.maekon.dev
  - 설치
  - 로컬 실행
  - privacy/security
  - ONESHIM 연동
  - 개발/빌드
  - release integrity
```

초기에는 public repo README와 GitHub Releases가 우선이다. landing/docs 배포 단위는 parent 작업이 안정된 뒤 별도 단계에서 결정한다.

## Public contribution 처리

public repo가 export target이면 외부 PR 처리 방식도 명확해야 한다.

1. public repo issue/PR에서 제안을 받는다.
2. maintainer가 변경 의도를 parent SSOT에 반영한다.
3. parent에서 테스트와 review를 진행한다.
4. public export를 재생성한다.
5. public repo에는 export 결과를 반영하고 원 public issue/PR을 참조한다.

이 흐름은 public contributor에게 다소 느리게 보일 수 있다. 대신 공개 저장소 README/CONTRIBUTING에서 "public repo는 curated export이며, accepted changes are applied through the internal source tree before export"라고 설명한다.

## Release trust 기준

공개 릴리스는 사용자가 다음 질문에 답할 수 있어야 한다.

- 이 바이너리는 어디서 받는가?
- 어떤 버전의 소스에 대응하는가?
- 서명과 notarization 상태는 무엇인가?
- checksum은 어디서 확인하는가?
- 취약점은 어디로 제보하는가?
- 자동 업데이트 서버와 GitHub Releases의 관계는 무엇인가?

최소 기준:

- GitHub Releases에 플랫폼별 artifact 제공
- checksum 제공
- macOS signed/notarized/stapled release 유지
- `SECURITY.md`에 `security@maekon.dev` 명시
- support 문서에 `support@maekon.dev` 명시
- release notes에서 Maekon Client와 ONESHIM Platform의 관계를 반복 설명

## `oneshim-client` archive 경계

`pseudotop/oneshim-client`는 Maekon public repo와 parent SSOT 경로가 준비된 뒤에도 계속 넓은 제품 작업을 받는 저장소가 되어서는 안 된다. 장기 공개 채널이 아니라, 깨끗한 archive 경계가 필요한 전환 저장소로 취급한다.

`oneshim-client`의 마지막 업데이트 범위는 다음으로 제한한다.

- 보존해야 하는 최신 merged client 상태
- `pseudotop/maekon-client`를 안전하게 bootstrap하기 위한 public export gate와 launch playbook 수정
- Maekon Client / ONESHIM Platform 관계를 설명하는 문서
- 기존 `oneshim-client` 링크로 들어오는 사용자를 위한 release/install/security 안내
- replacement public repo가 준비된 뒤 추가하는 archive/deprecation notice

다음 작업은 `oneshim-client`에서 진행하지 않는다.

- parent SSOT migration이 시작된 뒤의 신규 기능 개발
- 장기 Maekon 전용 landing/docs 작업
- `pseudotop/maekon-client`가 생긴 뒤의 중복 public source 유지보수
- private parent 내부에서만 의미 있는 parent migration script

권장 archive 순서는 다음이다.

1. `oneshim-client`에 strategy/export cleanup을 먼저 반영한다.
2. vetted export로 `pseudotop/maekon-client`를 bootstrap 또는 update한다.
3. Maekon public repo의 install, release, security 링크를 검증한다.
4. `oneshim-client` README와 repository description에 `pseudotop/maekon-client`로 안내하는 final archive notice를 추가한다.
5. `oneshim-client`를 security/redirect-only maintenance 상태로 freeze한다.
6. active install/update flow가 더 이상 `oneshim-client`에 의존하지 않을 때만 GitHub repository archive를 실행한다.

GitHub repository archive를 너무 빨리 실행하면 install URL, Releases, issue reporting에 대한 사용자 기대를 깨뜨릴 수 있다. archive action은 첫 신호가 아니라 마지막 단계여야 한다.

## Parent 편입 전 재검토 게이트

다음 조건을 만족하기 전에는 `client-rust`를 parent 내부 SSOT로 승격하지 않는다.

1. parent 작업이 안정되고 최신 `origin/main` 기준으로 구조를 다시 확인한다.
2. parent의 기존 `client-rust` submodule workflow 문서를 식별한다.
3. `clients/maekon-client` 이동이 parent의 server/backoffice/docs 구조를 불필요하게 흔들지 않는지 확인한다.
4. export script가 parent source path를 받을 수 있도록 설계한다.
5. public export gate가 parent-only path 유출을 차단하는지 검증한다.
6. Maekon/ONESHIM 브랜드 관계 문구를 README, install docs, login/OAuth, update docs에 반영할 수 있는지 확인한다.
7. public repo에서 외부 contribution을 처리할 운영 문구를 준비한다.

## 향후 작업

1. parent repo 안정화 후 현재 submodule 사용처를 다시 조사한다.
2. `clients/maekon-client` 이동 계획을 별도 migration plan으로 작성한다.
3. `scripts/export-public-repo.sh`를 parent-aware export 도구로 승격할지, 새 `tools/public-export/maekon-client` 도구로 옮길지 결정한다.
4. `pseudotop/maekon-client` public repo에 필요한 template, SECURITY, CONTRIBUTING, release notes skeleton을 정리한다.
5. Maekon landing/docs를 public repo 내부에 둘지 별도 repo/deploy unit으로 둘지 결정한다.
6. Maekon public repo 검증 후 최종 `oneshim-client` archive notice를 준비한다.

## 현재 결론

현 단계의 권장안은 다음이다.

```text
clients/maekon-client              # parent 내부 SSOT
tools/public-export/maekon-client  # 공개 범위와 export 운영 규칙
.public-worktrees/maekon-client    # gitignored public checkout
```

이 구조는 parent 전체를 즉시 완전한 모노레포 IA로 재편하지 않으면서도, Maekon Client를 신뢰 기반 공개 클라이언트로 운영할 수 있게 한다.
