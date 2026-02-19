# Crate 구현 문서

ONESHIM Rust 클라이언트의 10개 크레이트 상세 구현 문서입니다.

## Crate 의존성 그래프

```
┌─────────────────────────────────────────────────────────────────┐
│                       oneshim-app (바이너리)                      │
│                    DI 와이어링, 스케줄러, 라이프사이클               │
└─────────────────────────────────────────────────────────────────┘
        │
        ├───────────┬───────────┬───────────┬───────────┬─────────┐
        ▼           ▼           ▼           ▼           ▼         ▼
┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌────────────┐
│  network  │ │suggestion │ │  storage  │ │  monitor  │ │   vision  │ │ automation │
│ HTTP/SSE  │ │ 제안 처리  │ │  SQLite   │ │ 시스템 모니터│ │ Edge 이미지│ │ 자동화 제어 │
│ gRPC/WS   │ │ 우선순위큐 │ │  WAL모드  │ │ 활동 추적  │ │ PII 필터  │ │ 샌드박스   │
└───────────┘ └───────────┘ └───────────┘ └───────────┘ └───────────┘ └────────────┘
        │           │                                         │           │
        └─────┬─────┘                                         │           │
              ▼                                               ▼           │
       ┌───────────┐       ┌───────────┐                ┌───────────┐    │
       │    web    │       │    ui     │                │    ui     │    │
       │ REST API  │       │ 데스크톱 UI│◀───────────────│           │    │
       │ React 프론트│       │ 트레이 메뉴│                └───────────┘    │
       └───────────┘       └───────────┘                                  │
              │                   │                                       │
              └───────┬──────────┘                                        │
                      ▼                                                   │
┌─────────────────────────────────────────────────────────────────────────┘
│                       oneshim-core (기반)                                 │
│           도메인 모델, 포트 인터페이스(11개), 에러(23개), 설정              │
└──────────────────────────────────────────────────────────────────────────┘
```

## 크레이트 목록

| 크레이트 | 역할 | 주요 구현 | 문서 |
|----------|------|----------|------|
| **oneshim-core** | 기반 레이어 | 모델, 포트(11개), 에러(23개), 설정 | [상세](./oneshim-core.md) |
| **oneshim-network** | 네트워크 어댑터 | HTTP, SSE, WebSocket, 압축, 인증, gRPC, AI OCR/LLM 클라이언트 | [상세](./oneshim-network.md) |
| **oneshim-vision** | Edge 이미지 처리 | 캡처, 델타, WebP, OCR, 개인정보 필터, Privacy Gateway | [상세](./oneshim-vision.md) |
| **oneshim-monitor** | 시스템 모니터링 | CPU/메모리/디스크, 활성 창, 유휴 감지, 입력 활동 | [상세](./oneshim-monitor.md) |
| **oneshim-storage** | 로컬 저장소 | SQLite, 마이그레이션(V7), 보존 정책, Edge Intelligence | [상세](./oneshim-storage.md) |
| **oneshim-suggestion** | 제안 파이프라인 | 수신, 우선순위 큐, 피드백, 이력 | [상세](./oneshim-suggestion.md) |
| **oneshim-ui** | 데스크톱 UI | 시스템 트레이, 알림, 메인 윈도우, 테마, 자동화 토글 | [상세](./oneshim-ui.md) |
| **oneshim-web** | 로컬 웹 대시보드 | Axum REST API(60개+), React 프론트엔드(9페이지), SSE | [상세](./oneshim-web.md) |
| **oneshim-automation** | 자동화 제어 | 정책 기반 실행, 감사 로깅, OS 샌드박스, 의도 해석, 프리셋(10개) | [상세](./oneshim-automation.md) |
| **oneshim-app** | 바이너리 진입점 | DI, 9-루프 스케줄러, FocusAnalyzer, 자동 업데이트 | [상세](./oneshim-app.md) |

## 아키텍처 원칙

### Hexagonal Architecture (Ports & Adapters)

- **Core**: `oneshim-core`가 모든 포트(trait, 11개)와 모델 정의
- **Adapters**: 나머지 9개 크레이트가 포트 구현
- **의존성 규칙**: 어댑터 → Core (역방향 금지)

### 크로스-크레이트 통신 규칙

1. **직접 의존 금지**: 어댑터 간 직접 import 불가
2. **Core를 통한 통신**: 모든 인터페이스는 Core의 trait으로
3. **예외**: `suggestion → network` (SSE), `ui → suggestion` (표시)

### DI 패턴

- 생성자 주입 + `Arc<dyn T>`
- DI 프레임워크 미사용, 수동 와이어링
- 와이어링은 `oneshim-app/main.rs`에서 수행

### 2-레이어 자동화 액션 모델

- **AutomationIntent** (서버→클라이언트): 고수준 의도 (ClickElement, TypeIntoElement, ExecuteHotkey 등)
- **AutomationAction** (클라이언트 내부): 저수준 액션 (MouseMove, MouseClick, KeyType 등)
- **IntentResolver**: Intent → Action 시퀀스 변환 (OCR + LLM 활용)

## 주요 흐름

### 모니터링 흐름 (1초 간격)

```
SystemMonitor → ProcessMonitor → ActivityMonitor
       │              │               │
       └──────────────┴───────────────┘
                      │
                      ▼
               ContextEvent
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
    CaptureTrigger            Storage
          │                       │
          ▼                       │
    FrameProcessor                │
          │                       │
          └───────────┬───────────┘
                      ▼
               BatchUploader
                      │
                      ▼
                   Server
```

### 제안 수신 흐름

```
Server (SSE) → SseClient → SuggestionReceiver → PriorityQueue
                                    │                 │
                                    ▼                 ▼
                            DesktopNotifier    MainWindow (UI)
                                                      │
                                                      ▼
                                              FeedbackSender
                                                      │
                                                      ▼
                                              Server (REST)
```

### 자동화 실행 흐름

```
Server (AutomationIntent)
          │
          ▼
  AutomationController
          │
    ┌─────┴──────┐
    ▼            ▼
PolicyClient  AuditLogger
(검증)        (기록)
    │
    ▼
IntentResolver
    │
    ├── ElementFinder (OCR)
    ├── LlmProvider
    └── PrivacyGateway
          │
          ▼
  AutomationAction[]
          │
          ▼
    ┌─────┴──────┐
    ▼            ▼
InputDriver   Sandbox
(실행)        (격리)
```

## 테스트 현황

| 크레이트 | 테스트 수 | 커버리지 |
|----------|----------|----------|
| oneshim-core | 48 | 모델/에러/동의/자동화 모델/AiProviderType |
| oneshim-network | 86 | HTTP/압축/재시도/gRPC/AI 클라이언트 |
| oneshim-vision | 78 | 델타/인코더/썸네일 캐싱/PII필터/Privacy Gateway |
| oneshim-monitor | 39 | 메트릭/입력활동/창레이아웃 |
| oneshim-storage | 41 | CRUD/Edge Intelligence/N+1 최적화 |
| oneshim-suggestion | 17 | 큐/프레젠터 |
| oneshim-ui | 37 | 테마/차트/설정UI/자동화 토글 |
| oneshim-web | 68 | API/핸들러/태그/리포트/자동화 DTO |
| oneshim-automation | 104 | 정책/감사/샌드박스/리졸버/프리셋/의도해석 |
| oneshim-app | 102 | 통합/FocusAnalyzer/스케줄러 |
| **Rust Total** | **620** | - |
| **E2E (Playwright)** | **72** | 웹 대시보드 |
| **Total** | **692** | - |

## 참조

- [ADR-001: Rust Client Architecture Patterns](../architecture/ADR-001-rust-client-architecture-patterns.md)
- [Migration Overview](../migration/README.md)
- [CLAUDE.md](../../CLAUDE.md) - 개발 가이드
- [기여 가이드](../../CONTRIBUTING.md)
- [행동 강령](../../CODE_OF_CONDUCT.md)
- [보안 정책](../../SECURITY.md)
