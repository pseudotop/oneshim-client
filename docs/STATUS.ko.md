[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# 프로젝트 상태 (단일 소스)

최신/정확한 기준 문서는 영문 기본 문서인 [STATUS.md](./STATUS.md)입니다.

## 범위

아래의 변동 지표는 `STATUS.md`에서만 관리합니다.

- Rust 테스트 수 및 통과/실패
- E2E 테스트 수 및 통과/실패
- Lint/Build 상태
- 알려진 flaky 테스트 상태

## 운영 원칙

- 다른 문서에는 변동 수치를 하드코딩하지 않습니다.
- 현재 상태를 언급할 때는 `STATUS.md` 링크를 사용합니다.

## 최근 업데이트 (2026-03-08)

- Rust 테스트: **842개** (0 실패)
- v0.2.0 릴리스 완료 — CI 전체 통과
- GUI V2 M3 완료: SSE 이벤트 스트림 통합 (10개 테스트)
- Linux smoke `tauri::generate_context!()` 오류 수정 (v0.1.6부터 누적된 사전 버그)
- 상세 내역은 [STATUS.md](./STATUS.md) 참조
