[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# 퍼블릭 리포 런치 플레이북

이 문서는 Maekon을 오픈소스로 공개할 때, 내부 이력을 강제로 재작성하지 않고 안전하게 공개하는 절차를 정의합니다.

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
2. 대상 플랫폼 릴리즈 아티팩트 검증 완료
3. 알려진 P0 이슈 0건
4. 최신 QA 증적과 workflow 페이지 최신화

## 남은 운영 TODO

아래 항목은 Rust 클라이언트 코드베이스와 독립적으로 처리할 수 있으며, 별도
source change 없이 진행 가능합니다. 현재 client 정리를 막지 않도록 이 문서에
따로 추적하며, 이 playbook은 public export 대상에서 제외됩니다.

- **Landing 배포 연결, 보류**: 아직 landing 구현체가 없으므로
  `maekon.dev`를 임시 placeholder 웹 호스트에 연결하지 않습니다. DNS 준비만
  유지하고, landing 표면을 확정할 때까지 apex 웹 타깃은 비워둡니다. 준비되면
  `maekon.dev`를 공개 landing host에 연결하고, `www.maekon.dev`는 apex host로
  redirect하며, origin 인증서가 정상 제공된 뒤 Cloudflare SSL/TLS 모드를 Full
  (strict)로 올립니다.
- **사람이 보는 연락처, 공개 준비에는 충분**: `support@maekon.dev`와
  `security@maekon.dev`는 Cloudflare Email Routing에 유지합니다. catch-all은
  비활성 상태를 유지합니다. 이 상태만으로도 README/SECURITY/GitHub security
  contact 준비에는 충분하며, transactional product email이 없어도 공개 준비를
  막지 않습니다.
- **Transactional email, 보류**: 지금은 Resend outbound를 설정하지 않습니다.
  이유는 이 결정 시점의 Resend 공개 pricing이 Free plan은 1 domain, Pro plan은
  10 domains/$20/mo로 표시하고 있고, 현재 Resend Free team의 custom domain 1개
  한도를 `thengd.com`이 이미 사용 중이며, dashboard에서 새 team 생성은 paid
  feature이고, 도메인 한도 우회를 목적으로 새 계정을 만드는 방식은 Resend
  acceptable-use의 quota circumvention 금지 취지와 충돌하기 때문입니다. 또한
  Maekon은 public repository bootstrap 단계에서 아직 자동 발신 메일을 필요로 하지
  않습니다. 지금 설정하면 실제 product workflow 없이 provider credential, DNS
  상태, deliverability 관리 책임만 먼저 생기므로 보류합니다. 실제 자동 발신이
  필요해질 때 기존 Resend team을 Pro로 올려 `mail.maekon.dev`를 추가하거나,
  `thengd.com`이 Resend를 더 이상 쓰지 않을 때 도메인을 교체하거나, 다른 outbound
  provider를 비교해 선택합니다. Resend를 선택하면 `noreply@mail.maekon.dev`,
  `releases@mail.maekon.dev`를 예약하고, SPF/DKIM/DMARC 레코드를 Cloudflare에
  추가하며, `mail.maekon.dev`로 제한된 Sending-access API key를 발급합니다.
  참고: [Resend pricing](https://resend.com/pricing),
  [Resend acceptable use](https://resend.com/legal/acceptable-use),
  [Resend API key permissions](https://resend.com/docs/dashboard/api-keys/introduction).
- **Inbound automation, 추후**: 이메일 답장을 앱 이벤트로 처리해야 할 때만
  `reply.maekon.dev`를 Resend inbound webhook용으로 추가합니다.

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
- 로컬 절대 경로, 생성된 assistant review marker, private test bundle 참조 같은
  high-confidence internal text reference

이 gate가 release review를 대체하지는 않습니다. push 전에는 export diff를 직접
확인하고, export tree에서 테스트를 돌리고, 제외된 내부 계획 문서 때문에 생긴
public docs broken link를 검토해야 합니다.

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
3. 퍼블릭 리포에서 diff/CI 검증
4. 릴리즈 노트와 함께 푸시
