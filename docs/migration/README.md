# ONESHIM Client — Rust Native 마이그레이션 계획

> **⚠️ 이 문서는 `client/docs/rust-migration/`에서 이동되었습니다.** (2026-01-28)
>
> **작성일**: 2026-01-28
> **완료일**: 2026-01-28
> **상태**: ✅ **마이그레이션 완료** (Phase 0-6, 163 tests, GA Ready)
> **결정 사항**: Python Client → Pure Rust Native (Tauri/Sidecar 아님)
> **UI**: 순수 Rust (iced)
> **테스트**: 전체 Rust (#[test], #[tokio::test])

---

## 문서 구조

작업 시 해당 파일만 열어서 수정합니다.

| # | 문서 | 내용 | 상태 |
|---|------|------|------|
| 1 | [전환 근거](./01-rationale.md) | 왜 Rust, 왜 Full Native (Sidecar 아님) | ✅ 완료 |
| 2 | [프로젝트 구조 + 의존성](./02-project-structure.md) | 8개 크레이트 구조, Cargo.toml, 플랫폼별 의존성 | ✅ 완료 |
| 3 | [Python → Rust 매핑](./03-module-mapping.md) | 180+ Python 파일 → Rust 모듈 매핑표 | ✅ 완료 |
| 4 | [Server API 연동](./04-server-api.md) | 29개 엔드포인트, SSE 이벤트 타입 | ✅ 완료 |
| 5 | [마이그레이션 단계 + 성공 기준](./05-migration-phases.md) | Phase 0-6, 체크리스트, 완료 기준 | ✅ 완료 |
| 6 | [UI 프레임워크](./06-ui-framework.md) | iced 선택 (egui 대비 높은 접근성, 데스크톱 최적화) | ✅ 완료 |
| 7 | [코드 스케치](./07-code-sketches.md) | 핵심 Rust 구현 스케치 (모델, SSE, 제안) | 📚 참조용 |
| 8 | [Edge Vision 파이프라인](./08-edge-vision.md) | 이미지 전처리, 델타 인코딩, OCR, 타임라인 | ✅ 완료 |
| 9 | [테스트 전략](./09-testing.md) | 크레이트별 테스트, 예시 코드 | ✅ 완료 |
| 10 | [빌드/배포 + 리스크](./10-build-deploy.md) | 크로스 컴파일, 인스톨러, CI/CD | ✅ 완료 |

---

## 요약

```
Python Client (현재)          →    Rust Client (목표)
─────────────────────────────────────────────────────
~100MB+ 배포               →    ~15-25MB 단일 바이너리
Python + venv 설치         →    더블클릭 설치
psutil (래퍼)              →    sysinfo (네이티브)
aiohttp (GIL 제약)         →    reqwest + tokio (진정한 async)
SSE 없음 ❌               →    eventsource-client ✅
제안 수신 없음 ❌          →    SSE → 큐 → 알림 → 피드백 ✅
원본 JPEG 전송 (150-300KB) →    Edge 전처리: 델타/썸네일/OCR (~10-100KB) ✅
이미지 리와인드 없음 ❌    →    타임라인 + 텍스트 검색 + 썸네일 스크롤 ✅
mss + Pillow (래퍼)        →    xcap + image + webp (네이티브, SIMD)
OCR 없음 ❌               →    Tesseract FFI (optional feature)
CustomTkinter              →    iced/egui (순수 Rust)
pytest                     →    #[test] + #[tokio::test]
GPL 의존성 위험            →    MIT/Apache-2.0 전체 (오픈소스 안전)
```

**핵심**: Phase 1에서 SSE 연결을 확보하면, 서버의 완성된 Proactive Suggestion 파이프라인이 즉시 활성화된다. Phase 2에서 Edge Vision 파이프라인(델타 인코딩 + 로컬 OCR + 스마트 트리거)이 추가되면, 클라이언트는 동영상 대비 1/30~1/100 대역폭으로 서버에 시각 컨텍스트를 전달할 수 있다. **메타데이터 + 전처리된 이미지의 혼합 전송**이 ONESHIM Edge 처리의 핵심이다.

---

## ✅ 마이그레이션 완료 (2026-01-28)

모든 Phase가 완료되었습니다:

| Phase | 내용 | 상태 |
|-------|------|------|
| Phase 0 | Workspace 설정, CI/CD | ✅ |
| Phase 1 | Core 도메인 모델 + Ports | ✅ |
| Phase 2 | Network 어댑터 (HTTP/SSE/WS) | ✅ |
| Phase 3 | Storage + Monitor | ✅ |
| Phase 4 | Vision (Edge 이미지 처리) | ✅ |
| Phase 4.5 | Auto-start + OCR + 테스트 보강 | ✅ |
| Phase 5 | 자동 업데이트 | ✅ |
| Phase 6 | GA 준비 (CI/CD, 인스톨러, 문서) | ✅ |

**결과**:
- 8 crates, 68 source files, ~8,103 lines
- 163 tests, 0 failures, 0 clippy warnings
- GA Ready

**Python 클라이언트**: `client/` 폴더는 **DEPRECATED** 처리됨 (2026-01-28)

> **📌 참고**: 마이그레이션 이후 기능은 계속 확장되었습니다.
> 현재 품질 지표(테스트 수, 실패 수, lint/build 상태)의 단일 소스는 [STATUS.md](../STATUS.md)입니다.
> 최신 개발 가이드는 [CLAUDE.md](../../CLAUDE.md)를 참조하세요.
