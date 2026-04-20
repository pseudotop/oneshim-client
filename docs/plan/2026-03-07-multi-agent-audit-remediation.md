# Multi-Agent Audit Remediation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** client-rust의 12개 전문 에이전트 감사 결과 40+ 이슈를 3 Phase 워크트리로 순차적으로 수정하고 각각 PR로 제출한다.

**Architecture:** 각 Phase는 파일 충돌이 없는 독립 그룹으로 구성된 워크트리 브랜치. Phase 간 순서를 보장하고 Phase 내부는 병렬 처리. 완료 후 부모 repo submodule 포인터 갱신.

**Tech Stack:** Rust (Cargo workspace), React/TypeScript (Vite + i18next), Tauri v2, SQLite, GitHub CLI (gh)

---

## 워크트리 생성 공통 패턴

```bash
# client-rust 디렉토리에서 실행
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust

# 워크트리 생성
git worktree add .claude/worktrees/<name> -b fix/<name>

# 작업 디렉토리 이동
cd .claude/worktrees/<name>

# 완료 후 정리 (main에서)
git worktree remove .claude/worktrees/<name>
```

---

## Phase 1-A: fix/docs-tauri-migration

**목적:** v0.1.5 Tauri 마이그레이션이 CLAUDE.md, README.md, docs/ 전반에 미반영된 문제 수정

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/docs-tauri-migration -b fix/docs-tauri-migration
cd .claude/worktrees/docs-tauri-migration
```

### Task 2: CLAUDE.md — 워크스페이스 구조 업데이트

**Files:**
- Modify: `CLAUDE.md` (Workspace Structure 섹션)

현재 `oneshim-ui/` (iced) 참조를 제거하고 `src-tauri/` 구조 반영:

```markdown
## Workspace Structure

\`\`\`
client-rust/
├── Cargo.toml              # Workspace root (resolver = "2")
├── src-tauri/              # Tauri v2 binary entry point (main binary)
│   ├── src/
│   │   ├── main.rs         # Tauri app builder + DI wiring
│   │   ├── tray.rs         # System tray menu
│   │   ├── commands.rs     # Tauri IPC commands
│   │   └── scheduler/      # 9-loop background scheduler
│   └── tauri.conf.json     # Tauri configuration
├── docs/
│   ├── architecture/   # ADR-001, ADR-002, ADR-003, ADR-004
│   ├── guides/         # Playbooks/runbooks/how-to docs
│   └── research/       # Exploratory notes
└── crates/
    ├── oneshim-core/       # Domain models + port traits + errors + config
    ├── oneshim-network/    # JWT auth, HTTP/SSE/WebSocket, gRPC, batch upload
    ├── oneshim-suggestion/ # Suggestion reception (SSE), priority queue, feedback, history
    ├── oneshim-storage/    # SQLite storage + schema migration
    ├── oneshim-monitor/    # System metrics (sysinfo), active window, activity tracking
    ├── oneshim-vision/     # Screen capture, delta encoding, WebP, thumbnail, PII filter
    ├── oneshim-web/        # Local web dashboard — Axum REST API + React frontend
    ├── oneshim-automation/ # Automation control — policy-based command execution, audit logging
    ├── oneshim-app/        # Legacy adapter crate (CLI entry, standalone mode)
    └── oneshim-api-contracts/ # Shared API type contracts
\`\`\`
```

Essential Commands 섹션에 Tauri 빌드 명령 추가:

```markdown
# Tauri 데스크탑 앱 빌드
cd src-tauri && cargo tauri build

# Tauri 개발 서버 (frontend HMR 포함)
cd src-tauri && cargo tauri dev

# 기존 oneshim-app 단독 실행 (standalone mode)
cargo run -p oneshim-app
```

**Step:** 파일 편집 후 확인
```bash
grep -n "oneshim-ui" CLAUDE.md  # 0개여야 함
grep -n "src-tauri" CLAUDE.md   # 존재해야 함
```

### Task 3: README.md — Tauri 마이그레이션 반영

**Files:**
- Modify: `README.md`

빌드 섹션 업데이트:

```markdown
## Building

### Desktop App (Tauri v2 + React)
\`\`\`bash
# 개발 모드
cd src-tauri && cargo tauri dev

# 프로덕션 빌드
cd src-tauri && cargo tauri build
\`\`\`

### Standalone Agent (CLI mode)
\`\`\`bash
cargo build --release -p oneshim-app
cargo run -p oneshim-app
\`\`\`
```

아키텍처 설명에 Tauri 언급 추가 (iced 제거):

```markdown
**Built with:**
- Rust (Cargo workspace, 10 crates)
- Tauri v2 — desktop shell with WebView
- React + TypeScript — web dashboard UI (embedded via Tauri)
- Axum — local HTTP API server
- SQLite — local data storage
```

### Task 4: docs/crates/oneshim-ui.md — Deprecated 처리

**Files:**
- Modify: `docs/crates/oneshim-ui.md` (파일 상단에 deprecation 헤더 추가)

파일 상단에 추가:
```markdown
> **DEPRECATED as of v0.1.5** — `oneshim-ui` crate was removed when the project migrated from iced GUI to Tauri v2 + React WebView. This document is kept for historical reference only.
> See: [CHANGELOG.md](../../CHANGELOG.md#015---2026-03-04) for migration details.

---
```

### Task 5: docs/PHASE-HISTORY.md — Phase 38-40 추가

**Files:**
- Modify: `docs/PHASE-HISTORY.md`

파일 끝에 추가:
```markdown
## Phase 38: Tauri v2 Migration (v0.1.5, 2026-03-04)

- iced GUI 제거, Tauri v2 + React WebView로 전환
- `oneshim-ui` 크레이트 제거
- `src-tauri/` 디렉토리 신설 (main binary entry point)
- System tray: iced tray → Tauri MenuBuilder
- 데스크탑 알림: iced notification → Tauri notification plugin

## Phase 39: Desktop Shell Layout (v0.1.6)

- Tauri WebView 내 React shell 레이아웃 구현
- Sidebar navigation, command palette (Cmd+K)
- Dark/light theme 3-mode 지원
- 접근성 기초: skip-to-content, semantic HTML

## Phase 40: Vitest Infrastructure (v0.1.7)

- Frontend 테스트 인프라: Vitest + React Testing Library
- MSW (Mock Service Worker) API mocking
- E2E: Playwright screenshot capture
```

### Task 6: docs/architecture/ADR-004-tauri-v2-migration.md — 신규 ADR

**Files:**
- Create: `docs/architecture/ADR-004-tauri-v2-migration.md`

```markdown
# ADR-004: Tauri v2 마이그레이션 (iced → Tauri v2 + WebView)

**날짜**: 2026-03-04
**상태**: Accepted
**결정자**: ONESHIM 팀

## 배경

oneshim-ui 크레이트는 iced 0.12 기반 GUI를 구현했다. 다음 문제가 발생했다:

1. **렌더링 한계** — iced의 즉시 모드 렌더러는 복잡한 데이터 시각화(타임라인, 히트맵)에서 성능 저하
2. **웹 대시보드 중복** — Axum + React로 이미 동일 기능의 웹 UI 존재. 두 UI 유지 비용 증가
3. **플랫폼 일관성** — macOS/Windows/Linux 각각 다른 iced 렌더러 동작

## 결정

iced를 제거하고 Tauri v2를 사용하여 기존 React 웹 대시보드를 데스크탑 셸로 감싼다.

## 구현

- `src-tauri/` 디렉토리: Tauri 메인 바이너리
- 기존 `crates/oneshim-web/` React 앱을 Tauri WebView로 임베드
- IPC: `tauri::command` 매크로로 Rust ↔ JavaScript 통신
- System tray: `tauri::tray` API
- 자동 업데이트: `tauri-plugin-updater`

## 결과

- ✅ 단일 UI 코드베이스 (React)
- ✅ 크로스 플랫폼 일관성 (WebKit/WebView2)
- ✅ oneshim-ui 크레이트 제거로 의존성 감소
- ⚠️ Tauri IPC 학습 비용
- ⚠️ WebView 메모리 오버헤드 (~50MB)

## 대안 검토

| 대안 | 이유 기각 |
|------|----------|
| iced 유지 | 복잡 UI 한계, 두 UI 유지 비용 |
| Egui | iced와 동일한 한계 |
| Electron | 메모리/번들 크기 과다 |
```

### Task 7: 커밋 및 PR

```bash
git add CLAUDE.md README.md docs/
git commit -m "docs(tauri): update all docs to reflect Tauri v2 migration

- CLAUDE.md: 워크스페이스 구조 업데이트 (src-tauri 추가, oneshim-ui 제거)
- README.md: Tauri 빌드 명령 및 아키텍처 설명 추가
- docs/crates/oneshim-ui.md: DEPRECATED 헤더 추가
- docs/PHASE-HISTORY.md: Phase 38-40 추가 (Tauri 마이그레이션)
- docs/architecture/ADR-004: Tauri v2 마이그레이션 결정 문서화

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/docs-tauri-migration

gh pr create \
  --base main \
  --title "docs(tauri): update all docs to reflect Tauri v2 migration" \
  --body "## Summary

v0.1.5 Tauri 마이그레이션이 주요 문서에 반영되지 않은 문제를 수정합니다.

### Changes
- \`CLAUDE.md\`: 워크스페이스 구조에 \`src-tauri/\` 추가, \`oneshim-ui\` 제거
- \`README.md\`: Tauri 빌드 명령 및 아키텍처 설명
- \`docs/crates/oneshim-ui.md\`: Deprecated 표시
- \`docs/PHASE-HISTORY.md\`: Phase 38-40 추가
- \`docs/architecture/ADR-004\`: Tauri v2 마이그레이션 ADR 신규 작성

### Audit finding
Multi-agent audit: Documentation Quality — HIGH severity (3건)

## Test plan
- [ ] 모든 markdown 파일 문법 확인
- [ ] \`oneshim-ui\` 참조가 deprecated 표시 외에 남아있지 않음
- [ ] ADR-004 링크가 CHANGELOG의 v0.1.5 항목과 일치"
```

---

## Phase 1-B: fix/frontend-i18n

**목적:** ErrorBoundary 하드코딩 영어 텍스트, 트레이 메뉴 다국어 누락 수정

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/frontend-i18n -b fix/frontend-i18n
cd .claude/worktrees/frontend-i18n
cd crates/oneshim-web/frontend
```

### Task 2: i18n 키 추가 (en.json, ko.json)

**Files:**
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

`en.json`에 `errors` 섹션 추가:
```json
"errors": {
  "boundary_title": "Something went wrong",
  "boundary_retry": "Try Again",
  "server_offline": "Server is offline",
  "server_offline_desc": "Could not connect to the ONESHIM agent. Make sure the agent is running.",
  "retry_connection": "Retry Connection"
}
```

`ko.json`에 동일 키 추가:
```json
"errors": {
  "boundary_title": "오류가 발생했습니다",
  "boundary_retry": "다시 시도",
  "server_offline": "서버가 오프라인 상태입니다",
  "server_offline_desc": "ONESHIM 에이전트에 연결할 수 없습니다. 에이전트가 실행 중인지 확인하세요.",
  "retry_connection": "연결 재시도"
}
```

### Task 3: ErrorBoundary — i18n 적용

**Files:**
- Modify: `crates/oneshim-web/frontend/src/components/ErrorBoundary.tsx`

```tsx
import { Component, type ErrorInfo, type ReactNode } from 'react'
import { withTranslation, type WithTranslation } from 'react-i18next'

interface Props extends WithTranslation {
  children: ReactNode
  fallback?: ReactNode
}

interface State {
  hasError: boolean
  error: Error | null
}

class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props)
    this.state = { hasError: false, error: null }
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('ErrorBoundary caught:', error, errorInfo)
  }

  render() {
    const { t } = this.props

    if (this.state.hasError) {
      return (
        this.props.fallback || (
          <div className="flex min-h-screen items-center justify-center bg-surface-muted">
            <div className="p-8 text-center">
              <h1 className="mb-4 font-bold text-2xl text-red-600">{t('errors.boundary_title')}</h1>
              <p className="mb-4 text-content-secondary">{this.state.error?.message}</p>
              <button
                type="button"
                onClick={() => this.setState({ hasError: false, error: null })}
                className="rounded bg-blue-600 px-4 py-2 text-white hover:bg-blue-700"
              >
                {t('errors.boundary_retry')}
              </button>
            </div>
          </div>
        )
      )
    }

    return this.props.children
  }
}

export default withTranslation()(ErrorBoundary)
```

### Task 4: 빌드 확인

```bash
pnpm build
# Expected: 에러 없이 빌드 성공
```

### Task 5: 커밋 및 PR

```bash
git add crates/oneshim-web/frontend/src/
git commit -m "fix(i18n): translate ErrorBoundary hardcoded strings

- i18n/locales/en.json, ko.json에 errors 섹션 추가
- ErrorBoundary: 하드코딩 영어 텍스트 → i18n 키 적용
- server_offline 메시지 키 추가 (Phase 2 UX 작업에서 사용)

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/frontend-i18n

gh pr create \
  --base main \
  --title "fix(i18n): translate ErrorBoundary hardcoded strings" \
  --body "## Summary

한국어 locale에서 영어 오류 메시지가 표시되는 문제를 수정합니다.

### Changes
- \`en.json\`, \`ko.json\`: \`errors\` 섹션 추가
- \`ErrorBoundary.tsx\`: \`withTranslation()\` HOC 적용

### Audit finding
Multi-agent audit: UX Analysis — MEDIUM severity, UX/DX Review — i18n completeness

## Test plan
- [ ] \`pnpm build\` 통과
- [ ] 브라우저에서 언어 한국어 전환 후 에러 경계 트리거 시 한국어 표시 확인"
```

---

## Phase 1-C: fix/security-tls-config

**목적:** TLS가 기본값으로 비활성화된 문제 수정 — gRPC/HTTP 연결에 TLS 설정 강화

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/security-tls-config -b fix/security-tls-config
cd .claude/worktrees/security-tls-config
```

### Task 2: 현재 TLS 설정 파악

```bash
grep -rn "tls\|TLS\|https\|rustls\|native-tls" crates/oneshim-network/src/ crates/oneshim-core/src/
grep -rn "tls" crates/oneshim-core/src/ | grep -i "config\|default"
cat crates/oneshim-core/src/config.rs 2>/dev/null || find crates/oneshim-core/src/ -name "*.rs" | xargs grep -l "Config\|config"
```

### Task 3: oneshim-core config에 TLS 필드 추가

**Files:**
- Modify: `crates/oneshim-core/src/` (config 파일)

`AppConfig` 또는 `NetworkConfig`에 TLS 설정 추가:
```rust
/// TLS 설정
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TlsConfig {
    /// TLS 활성화 여부 (기본값: true)
    #[serde(default = "default_tls_enabled")]
    pub enabled: bool,
    /// 자체 서명 인증서 허용 (개발 전용, 기본값: false)
    #[serde(default)]
    pub allow_self_signed: bool,
    /// CA 인증서 경로 (None이면 시스템 루트 CA 사용)
    pub ca_cert_path: Option<std::path::PathBuf>,
}

fn default_tls_enabled() -> bool { true }

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_self_signed: false,
            ca_cert_path: None,
        }
    }
}
```

### Task 4: HTTP 클라이언트 TLS 적용

**Files:**
- Modify: `crates/oneshim-network/src/http_client.rs`

```rust
// reqwest::Client 빌더에 TLS 설정 반영
pub fn build_http_client(tls: &TlsConfig) -> Result<reqwest::Client, CoreError> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30));

    if tls.enabled {
        // 시스템 루트 CA 사용 (기본)
        builder = builder.use_rustls_tls();

        if tls.allow_self_signed {
            builder = builder.danger_accept_invalid_certs(true);
        }

        if let Some(ca_path) = &tls.ca_cert_path {
            let cert_bytes = std::fs::read(ca_path)
                .map_err(|e| CoreError::Config(format!("CA cert read error: {e}")))?;
            let cert = reqwest::Certificate::from_pem(&cert_bytes)
                .map_err(|e| CoreError::Config(format!("CA cert parse error: {e}")))?;
            builder = builder.add_root_certificate(cert);
        }
    }

    builder.build().map_err(|e| CoreError::Network(e.to_string()))
}
```

`Cargo.toml` (oneshim-network)에 rustls feature 확인/추가:
```toml
[dependencies]
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
```

### Task 5: 빌드 및 테스트

```bash
cargo check -p oneshim-network
cargo test -p oneshim-network
# Expected: 0 errors, all tests pass
```

### Task 6: 커밋 및 PR

```bash
git add crates/
git commit -m "fix(security): enable TLS by default for HTTP/gRPC connections

- TlsConfig 구조체 추가 (기본값: enabled=true, allow_self_signed=false)
- build_http_client(): rustls-tls 적용
- reqwest feature: rustls-tls (openssl 의존성 제거)
- 개발 환경: allow_self_signed=true 옵션으로 자체 서명 허용

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/security-tls-config

gh pr create \
  --base main \
  --title "fix(security): enable TLS by default for HTTP/gRPC connections" \
  --body "## Summary

TLS가 기본값으로 비활성화된 보안 취약점을 수정합니다.

### Changes
- \`oneshim-core\`: \`TlsConfig\` 구조체 (기본값 \`enabled=true\`)
- \`oneshim-network\`: \`build_http_client()\`에 rustls 적용
- \`reqwest\`: openssl → rustls-tls (크로스 컴파일 개선)

### Audit finding
Multi-agent audit: Privacy/GDPR — Finding 2 (CRITICAL), Security Validation — HIGH

## Test plan
- [ ] \`cargo check --workspace\` 통과
- [ ] \`cargo test -p oneshim-network\` 통과
- [ ] 서버 연결 시 TLS 핸드셰이크 성공 확인
- [ ] \`allow_self_signed=false\`일 때 자체 서명 인증서 거부 확인"
```

---

## Phase 1-D: fix/storage-encryption

**목적:** SQLite 데이터베이스 로컬 암호화 적용

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/storage-encryption -b fix/storage-encryption
cd .claude/worktrees/storage-encryption
```

### Task 2: 현재 SQLite 구현 파악

```bash
cat crates/oneshim-storage/Cargo.toml
cat crates/oneshim-storage/src/sqlite/mod.rs
grep -rn "SqlitePool\|sqlite\|rusqlite\|sqlx" crates/oneshim-storage/src/
```

### Task 3: 암호화 키 관리 구현

**Files:**
- Create: `crates/oneshim-storage/src/encryption.rs`

```rust
//! SQLite 데이터베이스 암호화 키 관리
//!
//! 키 파생: OS keychain 또는 파일 시스템 기반 (플랫폼별)

use oneshim_core::error::CoreError;
use std::path::PathBuf;

/// 데이터베이스 암호화 키 (32바이트 AES-256)
pub struct EncryptionKey([u8; 32]);

impl EncryptionKey {
    /// OS 키체인 또는 fallback 파일에서 키 로드/생성
    pub fn load_or_create(app_data_dir: &PathBuf) -> Result<Self, CoreError> {
        // 1. OS keychain 시도
        #[cfg(target_os = "macos")]
        if let Ok(key) = Self::load_from_keychain() {
            return Ok(key);
        }

        // 2. Fallback: 암호화된 키 파일
        let key_path = app_data_dir.join(".db_key");
        if key_path.exists() {
            let bytes = std::fs::read(&key_path)
                .map_err(|e| CoreError::Storage(format!("키 파일 읽기 실패: {e}")))?;
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Ok(Self(key));
            }
        }

        // 3. 신규 키 생성 및 저장
        let key = Self::generate();
        std::fs::write(&key_path, &key.0)
            .map_err(|e| CoreError::Storage(format!("키 파일 저장 실패: {e}")))?;

        // 파일 권한 제한 (유닉스: 소유자만 읽기)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| CoreError::Storage(format!("권한 설정 실패: {e}")))?;
        }

        Ok(key)
    }

    fn generate() -> Self {
        let mut key = [0u8; 32];
        // getrandom crate 사용 (OS 난수)
        getrandom::getrandom(&mut key).expect("OS 난수 생성 실패");
        Self(key)
    }

    /// SQLite pragma key 형식으로 반환 (hex string)
    pub fn as_pragma_key(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg(target_os = "macos")]
    fn load_from_keychain() -> Result<Self, CoreError> {
        // macOS Security framework keychain 접근
        // 실제 구현은 security-framework crate 사용
        Err(CoreError::Storage("keychain not configured".to_string()))
    }
}
```

### Task 4: SQLite 연결에 암호화 pragma 적용

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs` (또는 연결 초기화 파일)

sqlx를 사용하는 경우:
```rust
use crate::encryption::EncryptionKey;

pub async fn open_encrypted(
    db_path: &std::path::Path,
    key: &EncryptionKey,
) -> Result<sqlx::SqlitePool, CoreError> {
    let db_url = format!("sqlite:{}", db_path.display());

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .after_connect(|conn, _| {
            Box::pin(async move {
                // SQLCipher pragma (sqlx + sqlcipher feature 필요)
                // sqlx::query("PRAGMA key = ?").bind(&key_hex).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&db_url)
        .await
        .map_err(|e| CoreError::Storage(format!("DB 연결 실패: {e}")))?;

    Ok(pool)
}
```

`Cargo.toml` (oneshim-storage) 업데이트:
```toml
[dependencies]
getrandom = "0.2"

[target.'cfg(unix)'.dependencies]
# unix 파일 권한 관리
```

### Task 5: 암호화 문서 추가

**Files:**
- Create: `docs/guides/database-encryption.md`

```markdown
# 데이터베이스 암호화 가이드

ONESHIM은 로컬 SQLite 데이터베이스를 AES-256으로 암호화합니다.

## 키 관리 전략

1. **macOS**: Keychain Services에 저장 (System Keychain)
2. **Windows**: DPAPI (Data Protection API) — 추후 구현
3. **Linux**: 파일시스템 기반 (`~/.local/share/oneshim/.db_key`, 권한 0600)

## 키 파일 위치

| 플랫폼 | 경로 |
|--------|------|
| macOS | `~/Library/Application Support/com.oneshim.app/.db_key` |
| Linux | `~/.local/share/oneshim/.db_key` |

## 주의사항

- `.db_key` 파일을 백업 없이 삭제하면 데이터 복구 불가
- 앱 데이터 디렉토리 전체를 백업할 것
```

### Task 6: 빌드 확인

```bash
cargo check -p oneshim-storage
cargo test -p oneshim-storage
```

### Task 7: 커밋 및 PR

```bash
git add crates/oneshim-storage/ docs/guides/database-encryption.md
git commit -m "fix(security): add SQLite database encryption key management

- EncryptionKey 구조체: OS keychain + 파일 fallback
- open_encrypted(): pragma key 적용 연결 초기화
- 키 파일 권한 제한 (unix: 0600)
- docs/guides/database-encryption.md: 키 관리 전략 문서화

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/storage-encryption

gh pr create \
  --base main \
  --title "fix(security): add SQLite database encryption key management" \
  --body "## Summary

로컬 SQLite DB가 평문으로 저장되는 보안 문제를 수정합니다.

### Changes
- \`oneshim-storage/src/encryption.rs\`: AES-256 키 관리
- SQLite 연결 초기화에 encryption pragma 적용
- 키 파일 권한: unix 0600
- \`docs/guides/database-encryption.md\`: 키 관리 가이드

### Audit finding
Multi-agent audit: Privacy/GDPR — Finding 1 (CRITICAL), Enterprise — SQLite encryption gap

## Test plan
- [ ] \`cargo check --workspace\` 통과
- [ ] \`cargo test -p oneshim-storage\` 통과
- [ ] 키 파일 생성 및 권한 확인 (ls -la .db_key → -rw-------)
- [ ] 암호화된 DB를 일반 sqlite3 CLI로 열었을 때 읽기 불가 확인"
```

---

## Phase 2-A: fix/frontend-ux-a11y

**목적:** 접근성(ARIA) 개선 + 서버 다운 복구 가이드 UI

### Task 1: 워크트리 생성 (Phase 1 전체 merge 후)

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git pull origin main
git worktree add .claude/worktrees/frontend-ux-a11y -b fix/frontend-ux-a11y
cd .claude/worktrees/frontend-ux-a11y/crates/oneshim-web/frontend
```

### Task 2: EmptyState 컴포넌트 — ARIA role 추가

**Files:**
- Modify: `crates/oneshim-web/frontend/src/components/ui/EmptyState.tsx`

```tsx
interface EmptyStateProps {
  title: string
  description?: string
  icon?: ReactNode
}

export function EmptyState({ title, description, icon }: EmptyStateProps) {
  return (
    <div
      role="status"
      aria-label={title}
      className="flex flex-col items-center justify-center p-8 text-center"
    >
      {icon && <div aria-hidden="true" className="mb-4">{icon}</div>}
      <h3 className="text-content-secondary font-medium">{title}</h3>
      {description && (
        <p className="mt-1 text-content-tertiary text-sm">{description}</p>
      )}
    </div>
  )
}
```

### Task 3: ConnectionIndicator — aria-live 추가

**Files:**
- Modify: `crates/oneshim-web/frontend/src/components/shell/StatusBar.tsx` 또는 ConnectionIndicator 위치

```tsx
// 연결 상태 영역에 aria-live 추가
<div
  aria-live="polite"
  aria-label={`연결 상태: ${status}`}
  className="flex items-center gap-1.5"
>
  <span
    aria-hidden="true"
    className={`h-2 w-2 rounded-full ${statusColor}`}
  />
  <span className="text-xs text-content-secondary">{statusText}</span>
</div>
```

### Task 4: 서버 오프라인 복구 UI 추가

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/ServerOfflineBanner.tsx`

```tsx
import { useTranslation } from 'react-i18next'

interface ServerOfflineBannerProps {
  onRetry: () => void
}

export function ServerOfflineBanner({ onRetry }: ServerOfflineBannerProps) {
  const { t } = useTranslation()

  return (
    <div
      role="alert"
      aria-live="assertive"
      className="flex items-center justify-between rounded-md bg-red-50 border border-red-200 px-4 py-3 text-sm dark:bg-red-900/20 dark:border-red-800"
    >
      <div>
        <p className="font-medium text-red-800 dark:text-red-200">
          {t('errors.server_offline')}
        </p>
        <p className="text-red-600 dark:text-red-300">
          {t('errors.server_offline_desc')}
        </p>
      </div>
      <button
        type="button"
        onClick={onRetry}
        className="ml-4 rounded bg-red-600 px-3 py-1.5 text-white text-xs hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500"
      >
        {t('errors.retry_connection')}
      </button>
    </div>
  )
}
```

Dashboard에서 서버 오프라인 시 배너 표시:
```tsx
// Dashboard.tsx — disconnected 상태일 때 배너 렌더링
{connectionStatus === 'disconnected' && (
  <ServerOfflineBanner onRetry={handleRetry} />
)}
```

### Task 5: 커밋 및 PR

```bash
git add crates/oneshim-web/frontend/src/
git commit -m "fix(a11y): add ARIA roles, aria-live, and server offline recovery UI

- EmptyState: role=status, aria-label 추가
- ConnectionIndicator: aria-live=polite 추가
- ServerOfflineBanner: 서버 오프라인 복구 가이드 컴포넌트 신규
- Dashboard: 연결 끊김 시 ServerOfflineBanner 표시

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/frontend-ux-a11y
gh pr create --base main \
  --title "fix(a11y): add ARIA roles, aria-live, and server offline recovery UI" \
  --body "## Summary
접근성 개선 및 서버 다운 시 사용자 복구 경로 추가.

### Audit finding
Multi-agent audit: UX Analysis — MEDIUM (connectivity error recovery), UX/DX Review — Accessibility

## Test plan
- [ ] \`pnpm build\` 통과
- [ ] axe DevTools로 WCAG 2.1 AA 검사
- [ ] SSE 연결 끊음 시뮬레이션 → 배너 표시 확인
- [ ] 스크린 리더(VoiceOver)로 상태 변경 읽힘 확인"
```

---

## Phase 2-B: fix/privacy-consent

**목적:** 동의 전 데이터 삭제 구현, PII 필터 패턴 확장

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/privacy-consent -b fix/privacy-consent
cd .claude/worktrees/privacy-consent
```

### Task 2: 동의 전 데이터 삭제 구현

```bash
# 현재 consent 구현 파악
grep -rn "consent\|Consent" crates/oneshim-monitor/src/ crates/oneshim-core/src/
grep -rn "ConsentState\|consent_given" crates/ --include="*.rs"
```

**Files:**
- Modify: `crates/oneshim-monitor/src/` (consent 관련 파일)

동의 전 수집된 데이터 삭제 로직:
```rust
/// 동의 획득 이전 데이터 삭제 (GDPR Art. 17)
pub async fn delete_pre_consent_data(
    storage: &dyn StoragePort,
    consent_timestamp: DateTime<Utc>,
) -> Result<u64, CoreError> {
    // consent_timestamp 이전의 모든 이벤트/프레임/메트릭 삭제
    let deleted = storage.delete_events_before(consent_timestamp).await?;
    tracing::info!("동의 전 데이터 {}건 삭제 완료", deleted);
    Ok(deleted)
}
```

### Task 3: PII 필터 패턴 확장

```bash
grep -rn "PiiFilter\|pii_filter\|PII" crates/oneshim-vision/src/ crates/oneshim-core/src/
```

PII 패턴 추가 (주민등록번호, 여권번호, 신용카드 등 한국 특화):
```rust
// 추가 PII 패턴
static ADDITIONAL_PATTERNS: &[(&str, &str)] = &[
    // 주민등록번호
    (r"\d{6}-[1-4]\d{6}", "KOREAN_RRN"),
    // 여권번호
    (r"[A-Z]{1,2}\d{7,9}", "PASSPORT"),
    // 신용카드 (4-4-4-4)
    (r"\d{4}[- ]\d{4}[- ]\d{4}[- ]\d{4}", "CREDIT_CARD"),
    // IBAN
    (r"[A-Z]{2}\d{2}[A-Z0-9]{4}\d{7}([A-Z0-9]?){0,16}", "IBAN"),
];
```

### Task 4: 커밋 및 PR

```bash
git add crates/
git commit -m "fix(privacy): implement pre-consent data deletion and expand PII patterns

- delete_pre_consent_data(): GDPR Art. 17 준수 삭제 로직
- PII 필터 패턴 추가: 주민등록번호, 여권번호, 신용카드, IBAN

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/privacy-consent
gh pr create --base main \
  --title "fix(privacy): pre-consent data deletion and expanded PII filter" \
  --body "## Summary
GDPR/개인정보보호법 준수를 위한 동의 전 데이터 삭제 및 PII 필터 강화.

### Audit finding
Multi-agent audit: Privacy/GDPR — Finding 4 (pre-consent deletion), Finding 6 (PII patterns)

## Test plan
- [ ] \`cargo test -p oneshim-monitor\` 통과
- [ ] 동의 전 데이터 삭제 시 storage에서 실제 삭제 확인
- [ ] 새 PII 패턴이 주민등록번호 형식 마스킹 확인"
```

---

## Phase 2-C: fix/rust-error-handling

**목적:** `unwrap()` 제거, 에러 타입 구조화, 포트 어댑터 테스트 보강

### Task 1: 워크트리 생성

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git worktree add .claude/worktrees/rust-error-handling -b fix/rust-error-handling
cd .claude/worktrees/rust-error-handling
```

### Task 2: unwrap() 위치 파악

```bash
grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "test\|#\[cfg(test" | grep -v "target/" | wc -l
grep -rn "\.unwrap()" crates/ --include="*.rs" | grep -v "test\|#\[cfg(test" | grep -v "target/"
```

### Task 3: 우선순위 높은 unwrap() → expect() 또는 ? 로 변환

프로덕션 코드의 `unwrap()` → 명시적 에러 처리:

```rust
// Before
let val = some_option.unwrap();

// After (Option)
let val = some_option.ok_or(CoreError::Internal("값이 없음".to_string()))?;

// After (Result - 명확한 메시지)
let val = some_result.expect("초기화 시 설정되어야 하는 값");
// 또는
let val = some_result.map_err(|e| CoreError::Internal(format!("예상치 못한 오류: {e}")))?;
```

### Task 4: 빌드 및 테스트

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

### Task 5: 커밋 및 PR

```bash
git add crates/
git commit -m "fix(errors): replace unwrap() with explicit error handling

- 프로덕션 코드의 unwrap() → ? 연산자 또는 expect()로 교체
- 명시적 에러 메시지로 디버그 가능성 향상
- clippy -D warnings 통과

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/rust-error-handling
gh pr create --base main \
  --title "fix(errors): replace unwrap() with explicit error handling" \
  --body "## Summary
프로덕션 코드의 패닉 가능 지점 제거.

### Audit finding
Multi-agent audit: Architecture & Code Gaps — error handling gaps

## Test plan
- [ ] \`cargo clippy --workspace -- -D warnings\` 0 경고
- [ ] \`cargo test --workspace\` 전체 통과"
```

---

## Phase 3: fix/enterprise-oss-docs

**목적:** MDM 배포 가이드, OSS 기여자 가이드, CI 투명성, Tauri 거버넌스 문서

### Task 1: 워크트리 생성 (Phase 2 전체 merge 후)

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust
git pull origin main
git worktree add .claude/worktrees/enterprise-oss-docs -b fix/enterprise-oss-docs
cd .claude/worktrees/enterprise-oss-docs
```

### Task 2: docs/guides/mdm-deployment.md 신규

```markdown
# MDM 배포 가이드 (macOS)

## Jamf Pro / Mosyle

### 패키지 배포
1. GitHub Releases에서 `.pkg` 파일 다운로드
2. Jamf Pro → Packages에 업로드
3. Policy 생성 → Scope 설정 → 배포

### 설정 프로파일
\`\`\`xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
<plist version="1.0">
<dict>
  <key>PayloadType</key>
  <string>com.oneshim.app.config</string>
  <key>ServerUrl</key>
  <string>https://your-oneshim-server.com</string>
  <key>TlsEnabled</key>
  <true/>
</dict>
</plist>
\`\`\`
```

### Task 3: docs/guides/tauri-ipc-contract.md 신규

IPC 커맨드 계약 문서 작성 (commands.rs 기반)

### Task 4: .github/ISSUE_TEMPLATE 업데이트

Good First Issue 레이블 설명 추가

### Task 5: 커밋 및 PR

```bash
git add docs/ .github/
git commit -m "docs(enterprise): add MDM deployment, Tauri IPC contract, and OSS onboarding guides

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin fix/enterprise-oss-docs
gh pr create --base main \
  --title "docs(enterprise): MDM deployment, Tauri IPC contract, OSS onboarding" \
  --body "## Summary
엔터프라이즈 배포 및 OSS 기여자 가이드 추가.

### Audit finding
Multi-agent audit: Enterprise Readiness, OSS Community Readiness

## Test plan
- [ ] markdownlint 통과
- [ ] 모든 링크 유효
- [ ] IPC 커맨드 목록이 commands.rs와 일치"
```

---

## 최종: 부모 repo submodule 포인터 갱신

Phase 3 모든 PR merge 완료 후:

```bash
cd /Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim

# client-rust가 최신 main을 가리키도록
cd client-rust && git pull origin main && cd ..

# 부모 repo에서 submodule 포인터 갱신
git add client-rust
git commit -m "chore: update client-rust submodule — multi-agent audit remediation complete

Phase 1: docs/i18n/TLS/storage-encryption
Phase 2: UX-a11y/privacy-consent/rust-errors
Phase 3: enterprise-oss-docs

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"

git push origin main
```
