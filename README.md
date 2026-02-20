# ONESHIM Rust Client

The Rust native desktop client for the AI-powered autonomous office work assistant agent.

## Features

### Core Features
- **Real-time Context Monitoring**: Tracks active windows, system resources, and user activity
- **Edge Image Processing**: Screenshot capture, delta encoding, thumbnails, and OCR
- **Server SSE Connection**: Receives real-time suggestions and sends feedback
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

## Quick Start

### Build

```bash
# Development build
cargo build -p oneshim-app

# Release build
cargo build --release -p oneshim-app
```

### Run

```bash
# Set environment variables
export ONESHIM_EMAIL="your@email.com"
export ONESHIM_PASSWORD="your-password"

# Run
cargo run -p oneshim-app
```

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

### Download Binaries

Download the binary for your platform from the [Releases](https://github.com/pseudotop/oneshim-client/releases) page.

| Platform | File |
|--------|------|
| macOS Universal | `oneshim-macos-universal.tar.gz` |
| macOS Apple Silicon | `oneshim-macos-arm64.tar.gz` |
| macOS Intel | `oneshim-macos-x64.tar.gz` |
| Windows x64 | `oneshim-windows-x64.zip` |
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
| `ONESHIM_EMAIL` | Login email | (required) |
| `ONESHIM_PASSWORD` | Login password | (required) |
| `ONESHIM_TESSDATA` | Tesseract data path | (optional) |
| `RUST_LOG` | Log level | `info` |

### Config File

`~/.config/oneshim/config.toml` (Linux/macOS) or `%APPDATA%\oneshim\config.toml` (Windows):

```toml
[server]
base_url = "https://api.oneshim.com"
request_timeout_ms = 30000
sse_max_retry_secs = 30

[monitor]
poll_interval_ms = 1000
sync_interval_ms = 10000
heartbeat_interval_ms = 30000

[storage]
retention_days = 30
max_storage_mb = 500

[vision]
capture_throttle_ms = 5000
thumbnail_width = 480
thumbnail_height = 270
ocr_enabled = false

[update]
enabled = true
repo_owner = "pseudotop"
repo_name = "oneshim-client"
check_interval_hours = 24
include_prerelease = false

[web]
enabled = true
port = 9090
allow_external = false

[notification]
enabled = true
idle_threshold_mins = 30
long_session_threshold_mins = 60
high_usage_threshold_percent = 90
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

## Development

### Code Style

- **Language**: Comments and documentation in English
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
