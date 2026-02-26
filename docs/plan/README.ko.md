[English](./README.md) | [한국어](./README.ko.md)

# 구현 계획 인덱스

이 디렉터리는 날짜 기반 구현 계획과 실행 추적 문서를 관리합니다.

## 파일 이름 규칙

- `YYYY-MM-DD-<topic>.md`
- 핵심 계획은 `YYYY-MM-DD-<topic>.ko.md` companion 문서를 함께 유지

## 상태 규칙

- `Draft`: 검토 중 제안
- `Active`: 현재 구현 기준 문서
- `Done`: 후속 계획 없이 완료
- `Superseded`: 더 최신 날짜 계획으로 대체됨

## Active 계획

| 날짜 | 상태 | 계획 |
| --- | --- | --- |
| 2026-02-25 | Active | [ADR-002 GUI V2 구현 계획](./2026-02-25-adr-002-gui-v2-implementation-plan.ko.md) |
| 2026-02-25 | Active | [ADR-002 Phase3 상세 실행 계획](./2026-02-25-adr-002-phase3-delivery-plan.ko.md) |

## 운영 규칙

1. 범위나 실행 전략이 크게 바뀌면 새 날짜 문서를 추가합니다.
2. 계획 문서를 변경할 때 인덱스도 같은 커밋에서 갱신합니다.
3. 과거 문서는 바로 삭제하지 않고 `Superseded`로 표기합니다(archive 이동은 migration 정책에 따름).
