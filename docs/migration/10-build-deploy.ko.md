[English](./10-build-deploy.md) | [한국어](./10-build-deploy.ko.md)

# 10. 빌드/배포 + 리스크

[← 테스트 전략](./09-testing.ko.md) | [README →](./README.ko.md)

---

## 빌드 및 배포

### 바이너리 크기 최적화

```toml
# .cargo/config.toml
[profile.release]
opt-level = "z"          # 크기 최적화
lto = true               # Link-Time Optimization
codegen-units = 1        # 단일 코드젠 유닛
strip = true             # 디버그 심볼 제거
panic = "abort"          # unwind 제거
```

**예상 바이너리 크기**: ~15-25MB (UI 포함)

### 크로스 컴파일

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

### 인스톨러

| 플랫폼 | 형식 | 도구 |
|--------|------|------|
| macOS | .dmg | create-dmg |
| Windows | .msi | cargo-wix |
| Linux | .deb, .AppImage | cargo-deb, appimage-builder |

---

## 기존 Python Client와의 공존

### 병행 운영 전략

```
Phase 1-2: Rust CLI 모드 (터미널에서 SSE 수신 확인)
           Python Client 계속 사용 (UI 담당)

Phase 3:   Rust UI 완성 → Python Client 대체 시작
           Python Client 유지보수 모드 진입

Phase 4:   Rust Client 전면 전환
           Python Client 아카이브 (client-legacy/)
```

### 데이터 마이그레이션

```
Python SQLite DB → Rust SQLite DB
  - 스키마 호환 유지 (동일 테이블 구조)
  - 기존 DB 파일 그대로 사용 가능하도록 설계
  - rusqlite로 Python sqlite3 DB 직접 읽기
```

---

## 리스크 및 대응

| 리스크 | 심각도 | 대응 |
|--------|--------|------|
| macOS 접근성 권한 (CGWindowListCreate) | High | `CoreGraphics` FFI + Info.plist 설정 |
| Windows 서명 없는 바이너리 경고 | High | 코드 서명 인증서 취득 |
| UI 프레임워크 성숙도 | Medium | iced/egui 모두 활발한 개발 중, Phase 3에서 최종 선택 |
| Tesseract OCR 시스템 의존성 | Medium | `ocr` feature flag optional, 미설치 시 OCR 없이 동작 |
| SSE 재연결 안정성 | Medium | eventsource-client 자동 재연결 + 수동 fallback |
| 이미지 처리 메모리 사용 | Low | 프레임별 처리 후 즉시 해제, 동시 1프레임만 메모리에 |
| 크로스 컴파일 CI 복잡도 | Low | GitHub Actions matrix 빌드 |
