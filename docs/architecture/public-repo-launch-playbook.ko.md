[English](./public-repo-launch-playbook.md) | [한국어](./public-repo-launch-playbook.ko.md)

# 퍼블릭 리포 런치 플레이북

이 문서는 ONESHIM을 오픈소스로 공개할 때, 내부 이력을 강제로 재작성하지 않고 안전하게 공개하는 절차를 정의합니다.

## 전략

검증된 스냅샷에서 **별도 퍼블릭 히스토리**를 생성합니다.

- 내부/비공개 리포의 기존 히스토리는 유지합니다.
- 공개 가능한 소스 ref에서 트리 스냅샷을 추출합니다.
- 별도 디렉터리/리포에서 1-커밋 히스토리로 시작합니다.
- 해당 결과를 퍼블릭 원격으로 푸시합니다.

## 추천 후킹 카피

README와 리포 설명에 동일한 포지셔닝 문구를 사용합니다.

- **대표 문구**: `흩어진 업무 흔적을, 매일 성과로 이어지는 집중 인사이트로.`
- **리포 설명 후보**: `로컬 업무 신호를 실시간 집중 타임라인과 실행 가능한 제안으로 바꾸는 오픈소스 데스크톱 인텔리전스 클라이언트.`

## 사전 게이트 (Go/No-Go)

1. CI green (Rust + 프런트 빌드 + E2E)
2. 대상 플랫폼 릴리즈 아티팩트 검증 완료
3. 알려진 P0 이슈 0건
4. `docs/STATUS.md` 및 최신 QA 증적 최신화

## Export 절차

```bash
# 내부/비공개 리포 루트에서 실행
./scripts/export-public-repo.sh /tmp/oneshim-client-public <source-ref>

# 예시
./scripts/export-public-repo.sh /tmp/oneshim-client-public codex/release-web-gates-qa-connected-hardening
```

스크립트 동작:

1. `<source-ref>` 스냅샷 아카이브
2. `scripts/public-repo-exclude.txt` 경로 제거
3. 단일 초기 커밋으로 새 Git 히스토리 생성

## Publish 절차

```bash
cd /tmp/oneshim-client-public
git remote add origin <public-repo-url>
git push -u origin main
```

## 반복 업데이트 절차

퍼블릭 업데이트 시:

1. 내부 검증 완료된 source ref 준비
2. 새 임시 경로로 export 재실행
3. 퍼블릭 리포에서 diff/CI 검증
4. 릴리즈 노트와 함께 푸시
