[English](./06-ui-framework.md) | [한국어](./06-ui-framework.ko.md)

# 6. UI 프레임워크 선택

[← 마이그레이션 단계](./05-migration-phases.ko.md) | [코드 스케치 →](./07-code-sketches.ko.md)

---

## 후보 비교

| 프레임워크 | 장점 | 단점 | 적합도 |
|-----------|------|------|--------|
| **iced** | Elm 아키텍처, 크로스플랫폼, 네이티브 렌더링 | GPU 필요, 트레이 별도 처리 | ★★★★☆ |
| **egui** (+ eframe) | 즉시모드, 가볍고 빠름, WebAssembly 지원 | 네이티브 UX 부족 | ★★★★☆ |
| **gtk4-rs** | 네이티브 GTK, 성숙도 높음 | Windows 배포 복잡, macOS 비네이티브 | ★★★☆☆ |
| **slint** | 선언형 UI, 디자이너 도구 | 라이선스 이슈 (GPL/상용) | ★★★☆☆ |

## 권장: iced 또는 egui

**iced 선택 시**:
- Elm-like 아키텍처 → 상태 관리 명확
- 커스텀 위젯 용이
- tokio 통합 네이티브

**egui 선택 시**:
- 즉시모드 → 프로토타이핑 빠름
- AI 코드 생성에 더 적합 (단순한 API)
- 가볍고 빠른 렌더링

> 결정은 Phase 3 진입 시 확정. Phase 1-2는 UI 없이 진행하므로 블로킹 없음.
