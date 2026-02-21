[English](./01-rationale.md) | [한국어](./01-rationale.ko.md)

# 1. 전환 근거

[← README](./README.ko.md) | [프로젝트 구조 →](./02-project-structure.ko.md)

---

## 왜 Rust Native인가

| 항목 | Python (현재) | Rust (목표) |
|------|--------------|-------------|
| **배포 크기** | ~100MB+ (Python 런타임 포함) | ~15-20MB (단일 바이너리) |
| **설치** | Python 설치 → venv → pip install → 실행 | .dmg / .exe 더블클릭 |
| **시작 시간** | 2-5초 (인터프리터 로딩) | <100ms |
| **메모리 사용** | ~80-150MB (GC 오버헤드) | ~20-40MB |
| **시스템 접근** | psutil (래퍼) + pyobjc/pywin32 | 직접 시스템 콜 |
| **동시성** | asyncio + threading (GIL) | tokio (진정한 멀티스레드) |
| **안정성** | 런타임 타입 에러 가능 | 컴파일 타임 보장 |
| **보안** | 소스 노출, 메모리 취약 | 바이너리, 메모리 안전 |

## 왜 Sidecar가 아닌 Full Rust인가

- Sidecar: Python UI + Rust 모니터링 → 두 런타임 유지 관리 부담
- Full Rust: 단일 바이너리, 단일 언어, 단일 빌드 파이프라인
- AI가 코드 생성하므로 Rust UI 개발 난이도는 문제되지 않음
