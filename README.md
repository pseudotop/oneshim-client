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

A desktop client for AI-assisted office productivity — local context capture, real-time suggestions, and a built-in dashboard. Built with Rust for native performance across macOS, Windows, and Linux.

## Why ONESHIM

- **Turn activity into actionable insight**: Track context, timeline, focus trends, and interruptions in one place.
- **Stay lightweight on-device**: Edge processing (delta encoding, thumbnailing, OCR) reduces transfer volume and keeps response fast.
- **Use a production-ready desktop stack**: Cross-platform binary, auto-update, system tray integration, and local web dashboard.

## Who It's For

- Individual contributors who want visibility into focus patterns and work context
- Teams building AI-assisted workflow tooling on top of rich desktop signals
- Developers who want a modular, high-performance client with clear architecture boundaries

## 2-Minute Quickstart

```bash
# 1) Run in standalone mode (recommended for security-sensitive environments)
cargo run -p oneshim-app -- --offline

# 2) Open local dashboard
# http://localhost:9090
```

Standalone mode is available now.

Online features are currently in progress and will be announced when production-ready.

## Safety and Privacy at a Glance

- PII filtering levels (Off/Basic/Standard/Strict) are applied in the vision pipeline
- Local data is stored in SQLite and managed with retention controls
- Security reporting and response policy: [SECURITY.md](./SECURITY.md)
- Standalone integrity baseline: [docs/security/standalone-integrity-baseline.md](./docs/security/standalone-integrity-baseline.md)
- Integrity operation runbook: [docs/security/integrity-runbook.md](./docs/security/integrity-runbook.md)
- Current quality and release metrics: [docs/STATUS.md](./docs/STATUS.md)

## Features

### Core Features
- **Real-time Context Monitoring**: Tracks active windows, system resources, and user activity
- **Edge Image Processing**: Screenshot capture, delta encoding, thumbnails, and OCR
- **Connected Server Features (In Progress)**: Real-time suggestions and feedback sync are currently being prepared
- **System Tray**: Runs in the background with quick access
- **Auto-Update**: Automatic updates based on GitHub Releases
- **Cross-Platform**: Supports macOS, Windows, and Linux

### Local Web Dashboard (http://localhost:9090)
- **Dashboard**: Real-time system metrics, CPU/memory charts, app usage time
- **Timeline**: Screenshot timeline, tag filtering, lightbox viewer
- **Reports**: Weekly/monthly activity reports, productivity analysis
- **Session Replay**: Session replay with app segment visualization
- **Focus Analytics**: Focus analysis, interruption tracking, local suggestions
- **Settings**: Configuration management, data export/backup

### Desktop Notifications
- **Idle Notification**: Triggered after 30+ minutes of inactivity
- **Long Session Notification**: Triggered after 60+ minutes of continuous work
- **High Usage Notification**: Triggered when CPU/memory exceeds 90%
- **Focus Suggestions**: Break reminders, focus time scheduling, context restoration

## Requirements

- Rust 1.75 or later
- macOS 10.15+ / Windows 10+ / Linux (X11/Wayland)

## Developer Quick Start (Build from Source)

### Build

```bash
# Development build
cargo build -p oneshim-app

# Release build
cargo build --release -p oneshim-app
```

### Build Cache (Recommended for Local Development)

```bash
# Optional: install sccache
brew install sccache

# Use cached Rust builds via helper wrapper
./scripts/cargo-cache.sh check --workspace
./scripts/cargo-cache.sh test -p oneshim-web
./scripts/cargo-cache.sh build -p oneshim-app
```

If `sccache` is not installed, the wrapper falls back to normal `cargo`.

### Run

```bash
# Standalone mode (recommended)
cargo run -p oneshim-app -- --offline
```

Connected mode is in progress and not yet the recommended default path.

### Test

```bash
# Rust tests (current metrics: docs/STATUS.md)
cargo test --workspace

# E2E tests (current metrics: docs/STATUS.md) — web dashboard
cd crates/oneshim-web/frontend && pnpm test:e2e

# Lint (policy: zero warnings in CI)
cargo clippy --workspace

# Format check
cargo fmt --check
```

## Installation

### Package Managers (Recommended)

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

### Download Binaries

Download the binary for your platform from the [Releases](https://github.com/pseudotop/oneshim-client/releases) page.

| Platform | File |
|--------|------|
| macOS Universal | `oneshim-macos-universal.tar.gz` |
| macOS Apple Silicon | `oneshim-macos-arm64.tar.gz` |
| macOS Intel | `oneshim-macos-x64.tar.gz` |
| Windows x64 (zip) | `oneshim-windows-x64.zip` |
| Windows x64 (MSI) | `oneshim-app-*.msi` |
| Linux x64 | `oneshim-linux-x64.tar.gz` |

### macOS

```bash
# Extract archive
tar -xzf oneshim-macos-*.tar.gz

# Make executable
chmod +x oneshim

# Run
./oneshim
```

Configure auto-start:
```bash
# Use install script
./scripts/install-macos.sh

# Uninstall
./scripts/uninstall-macos.sh
```

### Windows

```powershell
# Extract archive
Expand-Archive oneshim-windows-x64.zip

# Run
.\oneshim.exe
```

Configure auto-start:
```powershell
# Use install script
.\scripts\install-windows.ps1

# Uninstall
.\scripts\uninstall-windows.ps1
```

### Linux

```bash
# Extract archive
tar -xzf oneshim-linux-x64.tar.gz

# Make executable
chmod +x oneshim

# Run
./oneshim
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|------|------|--------|
| `ONESHIM_EMAIL` | Login email (connected mode only) | (optional in standalone) |
| `ONESHIM_PASSWORD` | Login password (connected mode only) | (optional in standalone) |
| `ONESHIM_TESSDATA` | Tesseract data path | (optional) |
| `RUST_LOG` | Log level | `info` |

### Config File

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

## Architecture

A Cargo workspace with 10 crates following Hexagonal Architecture (Ports & Adapters).

```
oneshim-client/
├── crates/
│   ├── oneshim-core/       # Domain models + port traits + errors
│   ├── oneshim-network/    # HTTP/SSE/WebSocket adapters
│   ├── oneshim-suggestion/ # Suggestion reception and processing
│   ├── oneshim-storage/    # SQLite local storage
│   ├── oneshim-monitor/    # System monitoring
│   ├── oneshim-vision/     # Image processing (Edge)
│   ├── oneshim-ui/         # Desktop UI (iced)
│   ├── oneshim-web/        # Local web dashboard (Axum + React)
│   └── oneshim-app/        # Binary entry point
└── docs/
    ├── crates/             # Per-crate detailed documentation
    ├── architecture/       # ADR documents
    └── migration/          # Migration documents
```

### Crate Documentation

| Crate | Role | Docs |
|----------|------|------|
| oneshim-core | Domain models, port interfaces | [Details](./docs/crates/oneshim-core.md) |
| oneshim-network | HTTP/SSE/WebSocket, compression, auth | [Details](./docs/crates/oneshim-network.md) |
| oneshim-vision | Capture, delta encoding, OCR | [Details](./docs/crates/oneshim-vision.md) |
| oneshim-monitor | System metrics, active windows | [Details](./docs/crates/oneshim-monitor.md) |
| oneshim-storage | SQLite, offline storage | [Details](./docs/crates/oneshim-storage.md) |
| oneshim-suggestion | Suggestion queue, feedback | [Details](./docs/crates/oneshim-suggestion.md) |
| oneshim-ui | System tray, notifications, windows | [Details](./docs/crates/oneshim-ui.md) |
| oneshim-web | Local web dashboard, REST API | [Details](./docs/crates/oneshim-web.md) |
| oneshim-app | DI, scheduler, auto-update | [Details](./docs/crates/oneshim-app.md) |

Full documentation index: [docs/crates/README.md](./docs/crates/README.md)

For a detailed development guide, see [CLAUDE.md](./CLAUDE.md).

Current quality and release metrics are tracked in [docs/STATUS.md](./docs/STATUS.md).
Documentation language and consistency rules are defined in [docs/DOCUMENTATION_POLICY.md](./docs/DOCUMENTATION_POLICY.md).
Korean translation: [README.ko.md](./README.ko.md).
Korean companion policy/status docs: [docs/DOCUMENTATION_POLICY.ko.md](./docs/DOCUMENTATION_POLICY.ko.md), [docs/STATUS.ko.md](./docs/STATUS.ko.md).

## Development

### Code Style

- **Language**: English-first documentation with Korean companion docs for key public guides
- **Format**: `cargo fmt` default settings
- **Lint**: `cargo clippy` with 0 warnings

### Adding New Features

1. Define port traits in `oneshim-core`
2. Implement adapters in the relevant crate
3. Wire up DI in `oneshim-app`
4. Add tests

### Building Installers

macOS .app bundle:
```bash
cargo install cargo-bundle
cargo bundle --release -p oneshim-app
```

Windows .msi:
```bash
cargo install cargo-wix
cargo wix -p oneshim-app
```

## License

Apache License 2.0 — see [LICENSE](./LICENSE)

- [Contributing Guide](./CONTRIBUTING.md)
- [Code of Conduct](./CODE_OF_CONDUCT.md)
- [Security Policy](./SECURITY.md)

## Contributing

1. Fork
2. Create a feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push the branch (`git push origin feature/amazing`)
5. Open a Pull Request
