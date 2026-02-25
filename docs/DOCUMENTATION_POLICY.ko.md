[English](./DOCUMENTATION_POLICY.md) | [한국어](./DOCUMENTATION_POLICY.ko.md)

# 문서 정책

기준 문서는 영문 기본 문서인 [DOCUMENTATION_POLICY.md](./DOCUMENTATION_POLICY.md)입니다.

## 언어 정책

- 공개 문서는 **영문 기본(English-primary)** 으로 운영합니다.
- 핵심 공개 문서에는 한국어 companion 문서를 함께 유지합니다.
- 한국어 companion은 영문 기본 문서와 의미를 동기화합니다.

## 지표 일관성 정책

- 테스트 수, lint/build 상태 같은 변동 지표는 [STATUS.md](./STATUS.md)에서만 관리합니다.
- 다른 문서에서는 변동 수치를 중복 기재하지 않고 `STATUS.md`를 참조합니다.

## 디렉터리 구조 정책

- `docs/architecture/`는 ADR 전용입니다.
  - 파일 이름 규칙: `ADR-XXX-*.md`, `ADR-XXX-*.ko.md`
  - 리서치/플레이북/런북 같은 비-ADR 문서는 이 디렉터리에 두지 않습니다.
- `docs/research/`는 탐색/조사 문서를 관리합니다.
- `docs/guides/`는 운영/개발 플레이북과 how-to/런북 문서를 관리합니다.
- `docs/plan/`은 날짜 기반 구현 계획과 실행 추적 문서를 관리합니다.
  - 파일 이름 규칙: `YYYY-MM-DD-*.md` (핵심 계획은 `YYYY-MM-DD-*.ko.md` companion 포함)
- `docs/contracts/`는 버전드 payload/API 계약 문서를 관리합니다.
- `docs/crates/`는 crate별 구현 레퍼런스를 관리합니다.
- `docs/migration/`는 마이그레이션 이력/단계 아카이브 문서를 관리합니다. `README`는 최신 상태로 유지하고, 하위 문서는 migration 인덱스에서 active/legacy로 구분합니다.
- `docs/security/`는 보안 기준선 및 무결성 운영 문서를 관리합니다.
- `docs/qa/`는 QA 템플릿과 실행 기록 메타 문서를 관리합니다.

현재 문서 맵은 [docs/README.ko.md](./README.ko.md)를 참조하세요.
