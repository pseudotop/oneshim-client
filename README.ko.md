<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/brand/logo-full-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/brand/logo-full-light.svg">
    <img alt="ONESHIM Client" src="./assets/brand/logo-full-light.svg" width="400">
  </picture>
</p>

<p align="center">
  <a href="./README.md">English</a> | <a href="./README.ko.md">한국어</a>
</p>

# ONESHIM Client

AI 기반 업무 생산성 향상을 위한 데스크톱 클라이언트입니다. 로컬 컨텍스트 수집, 실시간 제안, 내장 대시보드를 제공합니다. Rust로 구축되어 macOS, Windows, Linux에서 네이티브 성능을 발휘합니다.

## ONESHIM을 선택하는 이유

- **활동을 실행 가능한 인사이트로 전환**: 컨텍스트, 타임라인, 집중 패턴, 방해 요소를 한 곳에서 추적합니다.
- **가벼운 온디바이스 처리**: Edge 처리(델타 인코딩, 썸네일, OCR)로 전송량을 줄이고 빠른 응답 속도를 유지합니다.
- **프로덕션 수준의 데스크톱 스택**: 크로스 플랫폼 바이너리, 자동 업데이트, 시스템 트레이 통합, 로컬 웹 대시보드를 지원합니다.

## 대상 사용자

- 집중 패턴과 업무 컨텍스트에 대한 가시성을 원하는 개인 기여자
- 풍부한 데스크톱 신호를 기반으로 AI 지원 워크플로우 도구를 개발하는 팀
- 모듈식 고성능 클라이언트와 명확한 아키텍처 경계를 원하는 개발자

## 2분 빠른 시작

```bash
# 1) Standalone 모드로 실행 (보안 민감 환경 권장)
cargo run -p oneshim-app -- --offline

# 2) 로컬 대시보드 열기
# http://localhost:9090
```

Standalone 모드는 현재 사용 가능합니다.

온라인 기능은 현재 개발 중이며, 프로덕션 준비가 완료되면 공지할 예정입니다.

## 보안 및 개인정보 보호 요약

- PII 필터링 수준(Off/Basic/Standard/Strict)이 비전 파이프라인에 적용됩니다
- 로컬 데이터는 SQLite에 저장되며, 보존 정책으로 관리됩니다
- 보안 보고 및 대응 정책: [SECURITY.md](./SECURITY.md)
- 현재 품질 및 릴리스 지표: [docs/STATUS.md](./docs/STATUS.md)

## 기능

### 핵심 기능
- **실시간 컨텍스트 모니터링**: 활성 창, 시스템 리소스, 사용자 활동을 추적합니다
- **Edge 이미지 처리**: 스크린샷 캡처, 델타 인코딩, 썸네일, OCR 지원
- **서버 연동 기능 (개발 중)**: 실시간 제안 및 피드백 동기화를 준비 중입니다
- **시스템 트레이**: 백그라운드에서 실행되며 빠른 접근이 가능합니다
- **자동 업데이트**: GitHub Releases 기반 자동 업데이트
- **크로스 플랫폼**: macOS, Windows, Linux를 지원합니다

### 로컬 웹 대시보드 (http://localhost:9090)
- **대시보드**: 실시간 시스템 지표, CPU/메모리 차트, 앱 사용 시간
- **타임라인**: 스크린샷 타임라인, 태그 필터링, 라이트박스 뷰어
- **리포트**: 주간/월간 활동 리포트, 생산성 분석
- **세션 재생**: 앱 세그먼트 시각화를 포함한 세션 재생
- **집중 분석**: 집중도 분석, 방해 요소 추적, 로컬 제안
- **설정**: 설정 관리, 데이터 내보내기/백업

### 데스크톱 알림
- **유휴 알림**: 30분 이상 비활성 시 트리거
- **장시간 세션 알림**: 60분 이상 연속 작업 시 트리거
- **높은 사용량 알림**: CPU/메모리가 90%를 초과하면 트리거
- **집중 제안**: 휴식 알림, 집중 시간 스케줄링, 컨텍스트 복원

## 요구 사항

- Rust 1.75 이상
- macOS 10.15+ / Windows 10+ / Linux (X11/Wayland)

## 개발자 빠른 시작 (소스에서 빌드)

### 빌드

```bash
# 개발 빌드
cargo build -p oneshim-app

# 릴리스 빌드
cargo build --release -p oneshim-app
```

### 빌드 캐시 (로컬 개발 권장)

```bash
# 선택: sccache 설치
brew install sccache

# 캐시를 사용하는 Rust 빌드 래퍼
./scripts/cargo-cache.sh check --workspace
./scripts/cargo-cache.sh test -p oneshim-web
./scripts/cargo-cache.sh build -p oneshim-app
```

`sccache`가 없으면 래퍼는 일반 `cargo`로 자동 폴백합니다.

### 실행

```bash
# Standalone 모드 (권장)
cargo run -p oneshim-app -- --offline
```

Connected 모드는 개발 중이며, 아직 기본 권장 경로가 아닙니다.

### 테스트

```bash
# Rust 테스트 (현재 지표: docs/STATUS.md)
cargo test --workspace

# E2E 테스트 (현재 지표: docs/STATUS.md) — 웹 대시보드
cd crates/oneshim-web/frontend && pnpm test:e2e

# 린트 (정책: CI에서 경고 0건)
cargo clippy --workspace

# 포맷 검사
cargo fmt --check
```

## 설치

### 패키지 매니저 (권장)

**Homebrew (macOS/Linux):**
```bash
brew tap pseudotop/tap
brew install oneshim
```

**Scoop (Windows):**
```powershell
scoop bucket add oneshim https://github.com/pseudotop/scoop-bucket
scoop install oneshim
```

### 바이너리 다운로드

[Releases](https://github.com/pseudotop/oneshim-client/releases) 페이지에서 해당 플랫폼용 바이너리를 다운로드하세요.

| 플랫폼 | 파일 |
|--------|------|
| macOS Universal | `oneshim-macos-universal.tar.gz` |
| macOS Apple Silicon | `oneshim-macos-arm64.tar.gz` |
| macOS Intel | `oneshim-macos-x64.tar.gz` |
| Windows x64 (zip) | `oneshim-windows-x64.zip` |
| Windows x64 (MSI) | `oneshim-app-*.msi` |
| Linux x64 | `oneshim-linux-x64.tar.gz` |

### macOS

```bash
# 압축 해제
tar -xzf oneshim-macos-*.tar.gz

# 실행 권한 부여
chmod +x oneshim

# 실행
./oneshim
```

자동 시작 설정:
```bash
# 설치 스크립트 사용
./scripts/install-macos.sh

# 제거
./scripts/uninstall-macos.sh
```

### Windows

```powershell
# 압축 해제
Expand-Archive oneshim-windows-x64.zip

# 실행
.\oneshim.exe
```

자동 시작 설정:
```powershell
# 설치 스크립트 사용
.\scripts\install-windows.ps1

# 제거
.\scripts\uninstall-windows.ps1
```

### Linux

```bash
# 압축 해제
tar -xzf oneshim-linux-x64.tar.gz

# 실행 권한 부여
chmod +x oneshim

# 실행
./oneshim
```

## 설정

### 환경 변수

| 변수 | 설명 | 기본값 |
|------|------|--------|
| `ONESHIM_EMAIL` | 로그인 이메일 (Connected 모드 전용) | (Standalone에서는 선택사항) |
| `ONESHIM_PASSWORD` | 로그인 비밀번호 (Connected 모드 전용) | (Standalone에서는 선택사항) |
| `ONESHIM_TESSDATA` | Tesseract 데이터 경로 | (선택사항) |
| `RUST_LOG` | 로그 레벨 | `info` |

### 설정 파일

`~/.config/oneshim/config.json` (Linux) / `~/Library/Application Support/com.oneshim.agent/config.json` (macOS) / `%APPDATA%\oneshim\agent\config.json` (Windows):

```json
{
  "server": {
    "base_url": "https://api.oneshim.com",
    "request_timeout_ms": 30000,
    "sse_max_retry_secs": 30
  },
  "monitor": {
    "poll_interval_ms": 1000,
    "sync_interval_ms": 10000,
    "heartbeat_interval_ms": 30000
  },
  "storage": {
    "retention_days": 30,
    "max_storage_mb": 500
  },
  "vision": {
    "capture_throttle_ms": 5000,
    "thumbnail_width": 480,
    "thumbnail_height": 270,
    "ocr_enabled": false
  },
  "update": {
    "enabled": true,
    "repo_owner": "pseudotop",
    "repo_name": "oneshim-client",
    "check_interval_hours": 24,
    "include_prerelease": false
  },
  "web": {
    "enabled": true,
    "port": 9090,
    "allow_external": false
  },
  "notification": {
    "enabled": true,
    "idle_threshold_mins": 30,
    "long_session_threshold_mins": 60,
    "high_usage_threshold_percent": 90
  }
}
```

## 아키텍처

Hexagonal Architecture (Ports & Adapters) 패턴을 따르는 10개 크레이트로 구성된 Cargo 워크스페이스입니다.

```
oneshim-client/
├── crates/
│   ├── oneshim-core/       # 도메인 모델 + 포트 트레이트 + 에러
│   ├── oneshim-network/    # HTTP/SSE/WebSocket 어댑터
│   ├── oneshim-suggestion/ # 제안 수신 및 처리
│   ├── oneshim-storage/    # SQLite 로컬 저장소
│   ├── oneshim-monitor/    # 시스템 모니터링
│   ├── oneshim-vision/     # 이미지 처리 (Edge)
│   ├── oneshim-ui/         # 데스크톱 UI (iced)
│   ├── oneshim-web/        # 로컬 웹 대시보드 (Axum + React)
│   └── oneshim-app/        # 바이너리 진입점
└── docs/
    ├── crates/             # 크레이트별 상세 문서
    ├── architecture/       # ADR 문서
    └── migration/          # 마이그레이션 문서
```

### 크레이트 문서

| 크레이트 | 역할 | 문서 |
|----------|------|------|
| oneshim-core | 도메인 모델, 포트 인터페이스 | [상세](./docs/crates/oneshim-core.md) |
| oneshim-network | HTTP/SSE/WebSocket, 압축, 인증 | [상세](./docs/crates/oneshim-network.md) |
| oneshim-vision | 캡처, 델타 인코딩, OCR | [상세](./docs/crates/oneshim-vision.md) |
| oneshim-monitor | 시스템 지표, 활성 창 | [상세](./docs/crates/oneshim-monitor.md) |
| oneshim-storage | SQLite, 오프라인 저장소 | [상세](./docs/crates/oneshim-storage.md) |
| oneshim-suggestion | 제안 큐, 피드백 | [상세](./docs/crates/oneshim-suggestion.md) |
| oneshim-ui | 시스템 트레이, 알림, 창 관리 | [상세](./docs/crates/oneshim-ui.md) |
| oneshim-web | 로컬 웹 대시보드, REST API | [상세](./docs/crates/oneshim-web.md) |
| oneshim-app | DI, 스케줄러, 자동 업데이트 | [상세](./docs/crates/oneshim-app.md) |

전체 문서 색인: [docs/crates/README.md](./docs/crates/README.md)

상세 개발 가이드: [CLAUDE.md](./CLAUDE.md)

현재 품질 및 릴리스 지표: [docs/STATUS.md](./docs/STATUS.md)
문서 언어 및 일관성 규칙: [docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md)
한국어 정책/상태 문서: [docs/DOCUMENTATION_POLICY.ko.md](./docs/DOCUMENTATION_POLICY.ko.md), [docs/STATUS.ko.md](./docs/STATUS.ko.md)

## 개발

### 코드 스타일

- **언어**: 영문 기본 문서 + 주요 공개 가이드에 대한 한국어 번역 문서 제공
- **포맷**: `cargo fmt` 기본 설정
- **린트**: `cargo clippy` 경고 0건

### 새 기능 추가

1. `oneshim-core`에서 포트 트레이트를 정의합니다
2. 해당 크레이트에서 어댑터를 구현합니다
3. `oneshim-app`에서 DI를 연결합니다
4. 테스트를 추가합니다

### 인스톨러 빌드

macOS .app 번들:
```bash
cargo install cargo-bundle
cargo bundle --release -p oneshim-app
```

Windows .msi:
```bash
cargo install cargo-wix
cargo wix -p oneshim-app
```

## 라이선스

Apache License 2.0 -- [LICENSE](./LICENSE) 참조

- [기여 가이드](./CONTRIBUTING.md)
- [행동 강령](./CODE_OF_CONDUCT.md)
- [보안 정책](./SECURITY.md)

## 기여하기

1. Fork
2. 기능 브랜치를 생성합니다 (`git checkout -b feature/amazing`)
3. 변경 사항을 커밋합니다 (`git commit -m 'Add amazing feature'`)
4. 브랜치를 푸시합니다 (`git push origin feature/amazing`)
5. Pull Request를 생성합니다
