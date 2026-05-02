[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# 퍼블릭 리포 런치 플레이북

이 문서는 Maekon을 오픈소스로 공개할 때, 내부 이력을 강제로 재작성하지 않고 안전하게 공개하는 절차를 정의합니다.

**내부 전용**: 이 런치 플레이북은 maintainer가 공개 저장소를 준비할 때
사용하는 문서입니다. public-minimal export 에서는 제외됩니다. 공개 사용자는
릴리스, 설치, 보안, contribution 문서를 보게 해야 합니다.

더 넓은 SSOT/export/managed-platform 전략은
`docs/plan/2026-04-30-maekon-client-public-oss-strategy.ko.md`를 기준으로
합니다.

## 전략

검증된 스냅샷에서 **별도 퍼블릭 히스토리**를 생성합니다.

- 내부/비공개 리포의 기존 히스토리는 유지합니다.
- 공개 가능한 소스 ref에서 트리 스냅샷을 추출합니다.
- 별도 디렉터리/리포에서 1-커밋 히스토리로 시작합니다.
- 해당 결과를 퍼블릭 원격으로 푸시합니다.
- 퍼블릭 리포는 두 번째 개발 원본이 아니라 export 대상물로 취급합니다.

export profile은 의도적으로 **public-minimal**입니다. 소스 코드, 빌드
메타데이터, 설치/릴리즈 문서, 보안 문서, 아키텍처 ADR, API contract,
crate reference, 공개 가이드만 내보냅니다. 세션 계획, sprint review 산출물,
탐색 research, roadmap/spec 초안, private test bundle, 환경 파일은 제외합니다.

런타임 데이터 예외가 하나 있습니다. `oneshim-core`가 compile time에
`include_str!`로 읽기 때문에 `specs/providers/provider-surface-catalog.json`은
퍼블릭 트리에 남겨야 합니다.

## 추천 후킹 카피

README와 리포 설명에 동일한 포지셔닝 문구를 사용합니다.

- **대표 문구**: `흩어진 업무 흔적을, 매일 성과로 이어지는 집중 인사이트로.`
- **리포 설명 후보**: `로컬 업무 신호를 실시간 집중 타임라인과 실행 가능한 제안으로 바꾸는 오픈소스 데스크톱 인텔리전스 클라이언트.`

## 사전 게이트 (Go/No-Go)

1. CI green (Rust + 프런트 빌드 + E2E)
2. 현재 공개 배포 범위의 릴리즈 아티팩트 검증 완료. 문서화된
   `glib 0.18.x` 런타임 advisory 예외가 활성화되어 있는 동안 Linux
   다운로드는 public release surface에서 제외합니다.
3. 알려진 P0 이슈 0건
4. 최신 QA 증적과 workflow 페이지 최신화

## Export 절차

```bash
# 내부/비공개 리포 루트에서 실행
./scripts/export-public-repo.sh /tmp/maekon-client-public <source-ref>

# 예시
./scripts/export-public-repo.sh /tmp/maekon-client-public codex/release-web-gates-qa-connected-hardening

# 커밋 전 현재 작업트리 smoke
./scripts/export-public-repo.sh --dry-run --worktree
```

스크립트 동작:

1. `<source-ref>` 스냅샷 아카이브
2. `scripts/public-repo-exclude.txt` 경로 제거
3. 필요한 public 경로와 금지된 internal 경로 검증
4. high-confidence internal reference scan 실행
5. 단일 초기 커밋으로 새 Git 히스토리 생성

다운스트림 도구가 Git 히스토리 없는 export tree만 필요로 할 때는
`--no-commit`을 사용합니다.

## Export Gate 범위

내장 gate는 private context 유출 또는 public build 실패 가능성이 큰 edge case를
막는 데 초점을 둡니다.

- 내부 planning, review, research, roadmap, migration, private validation 디렉터리
- `server/`, `backoffice/`, `terraform/` 같은 parent monorepo 디렉터리
- 로컬 환경 파일과 agent tooling 파일
- provider surface catalog 등 public/runtime 필수 파일 누락
- public Dependabot config 누락, 내부 Dependabot auto-merge automation 또는
  생성된 SBOM 아티팩트의 우발적 포함
- 로컬 절대 경로, 생성된 assistant review marker, private test bundle 참조 같은
  high-confidence internal text reference

이 gate가 release review를 대체하지는 않습니다. push 전에는 export diff를 직접
확인하고, export tree에서 테스트를 돌리고, 제외된 내부 계획 문서 때문에 생긴
public docs broken link를 검토해야 합니다.

## Dependency update 정책

public Dependabot은 켜둡니다. public dependency PR은 오픈소스 신뢰 표면의
일부입니다. dependency drift를 공개적으로 보여주고, contributor가 같은 신호를
검토할 수 있게 하며, maintainer에게 공개 audit trail을 남깁니다.

대신 PR은 path-aware 규칙으로 분류합니다.

- `Cargo.toml`, `Cargo.lock`, Rust source, exported workflow처럼 SSOT에서
  mirror되는 source/dependency path는 parent/client SSOT tree에 먼저 같은
  변경을 재현합니다. private/full CI와 export gate를 통과한 뒤 public tree를
  재생성하고, public PR은 upstream 변경 링크와 함께 close 또는 supersede
  처리합니다.
- public repository metadata, public issue template, 명시적인 public overlay
  파일처럼 public-only path는 public 쪽에서 직접 조정할 수 있습니다. 계속 유지할
  변경이면 export overlay/tooling에도 되돌려 반영합니다.
- security-critical fix는 속도를 위해 maintainer exception을 둘 수 있습니다.
  다만 바로 SSOT replay를 따라붙여 public repo가 영구적인 두 번째 source of
  truth가 되지 않게 합니다.

public repo 설정에서는 Dependabot security alerts와 version-update PR을 켜두되,
mirror된 dependency path에 대한 public auto-merge는 켜지 않습니다.
public-minimal export는 `.github/dependabot.yml`을 남겨 visibility를 유지하고,
내부 Dependabot auto-merge workflow만 제외합니다.

## Publish 절차

```bash
cd /tmp/maekon-client-public
git remote add origin <public-repo-url>
git push -u origin main
```

## 반복 업데이트 절차

퍼블릭 업데이트 시:

1. 내부 검증 완료된 source ref 준비
2. 새 임시 경로로 export 재실행
3. public Dependabot config는 포함되고 내부 auto-merge automation은 포함되지
   않았는지 확인
4. 퍼블릭 리포에서 diff/CI 검증
5. 릴리즈 노트와 함께 푸시
