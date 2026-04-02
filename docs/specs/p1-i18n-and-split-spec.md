# P1 Spec — Tracking Panel i18n + Chat.tsx Split

**Date**: 2026-04-02
**Scope**: Two real P1 tasks (two others were false alarms)

---

## Corrected P1 Status

| Original P1 | Finding | Action |
|-------------|---------|--------|
| tokio::spawn tracking (50 sites) | Scheduler has full shutdown management (sync.rs:365-410). Non-scheduler spawns are bounded fire-and-forget. | **No action needed** |
| Tracking panel i18n | 28 hardcoded strings confirmed | **Implement** |
| http_api_session split (2,381L) | Already ADR-003 directory module (6 files) | **No action needed** |
| Chat.tsx split (1,775L) | Real — needs decomposition | **Implement** |

---

## Task 1: Tracking Panel i18n

### Current State
- `tracking-panel/App.tsx` (371 lines) has 28 hardcoded English strings
- i18n infrastructure exists (i18next, 5 locales) but panel doesn't import it
- `main.tsx` doesn't initialize i18n

### Changes Required

1. **Import i18n in `main.tsx`** — add `import '../i18n'` before render
2. **Use `useTranslation` in `App.tsx`** — wrap all 28 strings in `t()`
3. **Add `trackingPanel` namespace** to all 5 locale files

### String Inventory (28 strings)

| Key | en | ko |
|-----|----|----|
| statusUnavailable | Status unavailable | 상태를 확인할 수 없습니다 |
| connectionUnavailable | Connection status unavailable | 연결 상태를 확인할 수 없습니다 |
| captured | Captured | 캡처 완료 |
| captureFailed | Capture failed | 캡처 실패 |
| analyzing | Analyzing... | 분석 중... |
| elements | elements | 요소 |
| analysisFailed | Analysis failed | 분석 실패 |
| focusOff | Focus off | 집중 모드 해제 |
| focus25m | Focus 25m | 집중 모드 25분 |
| focusToggleFailed | Focus toggle failed | 집중 모드 전환 실패 |
| suggestionsOpened | Suggestions panel opened | 제안 패널 열림 |
| suggestionsUnavailable | Suggestions unavailable | 제안을 사용할 수 없습니다 |
| paused | Paused | 일시정지 |
| capturing | Capturing | 캡처 중 |
| resume | Resume | 재개 |
| pause | Pause | 일시정지 |
| collapse | Collapse | 접기 |
| expand | Expand | 펼치기 |
| hide | Hide | 숨기기 |
| openDashboard | Open Dashboard | 대시보드 열기 |
| manualCapture | Manual Capture | 수동 캡처 |
| sceneAnalysis | Scene Analysis | 화면 분석 |
| aiSuggestions | AI Suggestions | AI 제안 |
| focusMode | Focus Mode | 집중 모드 |
| offlineMessage | Offline — local capture + analysis available | 오프라인 — 로컬 캡처 + 분석 가능 |
| server | Server | 서버 |
| openSettings | Open Settings | 설정 열기 |
| comingSoon | Coming soon | 출시 예정 |

## Task 2: Chat.tsx Decomposition

Deferred to a separate PR — requires careful state wiring across 7+ extracted hooks and 4+ components. Out of scope for this session's P1 focus on i18n.

---

## Implementation Plan

1. Add `trackingPanel` keys to `en.json` and `ko.json`
2. Add `trackingPanel` keys to `ja.json`, `zh-CN.json`, `es.json`
3. Import i18n in `tracking-panel/main.tsx`
4. Wrap strings in `App.tsx` with `t()`
5. Verify with `pnpm lint` + `pnpm build`
