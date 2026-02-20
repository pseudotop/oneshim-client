# ONESHIM Rust Client (한국어 안내)

영문 README가 기본 문서이며, 이 문서는 한국어 요약/온보딩용 companion 문서입니다.

영문 기본 문서: [README.md](./README.md)

## 빠른 시작 (Standalone 권장)

```bash
# 보안 민감 환경 권장 모드
cargo run -p oneshim-app -- --offline

# 로컬 대시보드
# http://localhost:9090
```

## 현재 제품 상태

- Standalone 모드는 현재 사용 가능
- Connected(서버 연동) 기능은 준비 중이며, production-ready 시 공지

## 핵심 포인트

- Edge 처리 기반 컨텍스트 수집 (캡처/델타/썸네일/OCR)
- 로컬 SQLite 저장 + 보존 정책
- 시스템 트레이, 알림, 자동 업데이트, 로컬 웹 대시보드

## 환경 변수 (연결 모드 기준)

| 변수 | 설명 |
|------|------|
| `ONESHIM_EMAIL` | 로그인 이메일 (connected mode에서 사용) |
| `ONESHIM_PASSWORD` | 로그인 비밀번호 (connected mode에서 사용) |
| `ONESHIM_TESSDATA` | Tesseract 데이터 경로 (선택) |
| `RUST_LOG` | 로그 레벨 |

## 문서 정책 및 상태

- 문서 정책: [docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md)
- 최신 품질 지표: [docs/STATUS.md](./docs/STATUS.md)

## 개발/기여 문서

- 개발 가이드: [CLAUDE.md](./CLAUDE.md)
- 기여 가이드: [CONTRIBUTING.md](./CONTRIBUTING.md)
- 보안 정책: [SECURITY.md](./SECURITY.md)
