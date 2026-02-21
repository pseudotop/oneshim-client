[English](./10-build-deploy.md) | [한국어](./10-build-deploy.ko.md)

# 10. Build/Deploy + Risks

[← Testing Strategy](./09-testing.md) | [README →](./README.md)

---

## Build and Deployment

### Binary Size Optimization

```toml
# .cargo/config.toml
[profile.release]
opt-level = "z"          # Size optimization
lto = true               # Link-Time Optimization
codegen-units = 1        # Single codegen unit
strip = true             # Strip debug symbols
panic = "abort"          # Remove unwind
```

**Expected binary size**: ~15-25MB (including UI)

### Cross-Compilation

```bash
# macOS Universal (ARM + Intel)
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
lipo -create target/aarch64-.../oneshim target/x86_64-.../oneshim -output oneshim-universal

# Windows
cargo build --release --target x86_64-pc-windows-msvc

# Linux
cargo build --release --target x86_64-unknown-linux-gnu
```

### Installers

| Platform | Format | Tool |
|----------|--------|------|
| macOS | .dmg | create-dmg |
| Windows | .msi | cargo-wix |
| Linux | .deb, .AppImage | cargo-deb, appimage-builder |

---

## Coexistence with Legacy Python Client

### Parallel Operation Strategy

```
Phase 1-2: Rust CLI mode (verify SSE reception in terminal)
           Python Client continues in use (handles UI)

Phase 3:   Rust UI complete → Begin replacing Python Client
           Python Client enters maintenance mode

Phase 4:   Full switch to Rust Client
           Python Client archived (client-legacy/)
```

### Data Migration

```
Python SQLite DB → Rust SQLite DB
  - Maintain schema compatibility (same table structure)
  - Designed to use existing DB file directly
  - Read Python sqlite3 DB directly with rusqlite
```

---

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| macOS accessibility permissions (CGWindowListCreate) | High | `CoreGraphics` FFI + Info.plist configuration |
| Windows unsigned binary warning | High | Obtain code signing certificate |
| UI framework maturity | Medium | Both iced/egui actively developed, final selection in Phase 3 |
| Tesseract OCR system dependency | Medium | `ocr` feature flag optional, operates without OCR when not installed |
| SSE reconnection stability | Medium | eventsource-client auto-reconnect + manual fallback |
| Image processing memory usage | Low | Release immediately after per-frame processing, only 1 frame in memory at a time |
| Cross-compilation CI complexity | Low | GitHub Actions matrix builds |
