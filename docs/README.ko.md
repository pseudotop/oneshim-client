[English](./README.md) | [한국어](./README.ko.md)

# 문서 인덱스

이 디렉터리는 문서 목적에 따라 구성됩니다.

## 루트 문서

- [DOCUMENTATION_POLICY.md](./DOCUMENTATION_POLICY.md): 문서 컨벤션 및 유지보수 규칙
- [STATUS.md](./STATUS.md): 변동 품질 신호와 워크플로우 링크를 모아 둔 스냅샷
- [install.ko.md](./install.ko.md): 설치 가이드

## 하위 디렉터리

- `architecture/`: ADR 전용 아키텍처 결정 문서
- `research/`: 조사/탐색 문서
- `guides/`: 운영/개발 플레이북, 런북, how-to 가이드
- `plan/`: 날짜 기반 구현 계획 및 실행 추적 문서
- `contracts/`: 버전드 API/payload 계약 문서와 생성 OpenAPI 스냅샷
- `crates/`: crate 단위 구현 레퍼런스
- `migration/`: 마이그레이션 이력 및 단계별 계획(`migration/README.ko.md`의 active/legacy 구분 기준 적용)
- `security/`: 보안 기준선 및 무결성 운영 문서
- `qa/`: QA 템플릿, 실행 기록, 아티팩트 메타 문서
- `reviews/`: 스프린트형 phase 의 설계+리뷰 문서 세트(`YYYY-MM-DD-<phase>-{design,spec,plan}.md`). `plan/` 과는 다름 — `reviews/` 는 한 phase 의 "설계 → 구현 계획" 짝을 같이 캡쳐; `plan/` 은 단일 실행 계획 파일
- `roadmap/`: 장기 horizon 다중 phase roadmap 문서 (`<phase-name>-spec.md`)
- `specs/`: 개별 기능 상세 functional spec (ADR 보다 앞서거나 보완)
- `testing/`: 테스트 전략 문서
- `superpowers/`: (대부분 gitignore) `superpowers` 플러그인 워크플로우의 세션 단위 spec/plan/review/brainstorm 아티팩트. 영속성 있는 결정은 `architecture/`/`plan/`/`reviews/` 로 승격

## 빠른 배치 규칙

1. `docs/architecture/`에는 `ADR-XXX-*` 형식만 둡니다.
2. 구속력이 없는 탐색 문서는 `docs/research/`에 둡니다.
3. 절차형 플레이북/런북은 `docs/guides/`에 두고, 보안 전용이면 `docs/security/`에 둡니다.
4. 구현 계획 문서는 `docs/plan/`에 두고 `YYYY-MM-DD-*.md` 형식을 사용합니다(핵심 계획은 `.ko.md` companion 유지).
5. 스프린트 phase 의 설계+계획 짝은 `docs/reviews/` 에 `YYYY-MM-DD-phaseN-<topic>-{design,spec,plan}.md` 형식으로 둡니다.
6. 공개 핵심 문서는 영문 기본 + 한국어 companion을 함께 유지합니다.
