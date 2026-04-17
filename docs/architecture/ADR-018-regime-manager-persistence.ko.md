[English](./ADR-018-regime-manager-persistence.md) | [한국어](./ADR-018-regime-manager-persistence.ko.md)

# ADR-018: RegimeManager 영속화

**상태 (Status)**: 승인 (Approved)
**날짜 (Date)**: 2026-04-18
**범위 (Scope)**: `oneshim-core::ports::regime_storage`, `oneshim-storage::regime_manager_state_store`, `oneshim-analysis::RegimeManager::hydrate_from`, `src-tauri::main::RunEvent::Exit`

---

## 배경 (Context)

`RegimeManager` 는 완전히 인메모리(in-memory) 상태였다 — 재시작마다 사용자가 큐레이션한 regime 이름, 병합(merge), 삭제(delete) 내역이 모두 소실됐다. 기존 SQL `regimes` 테이블은 크로스 디바이스 동기화 경로(`sync_merger.rs`) 에서만 사용되며, RegimeManager 의 전체 상태(centroid, RegimeStatus enum, 사용자 지정 이름 오버라이드) 를 담고 있지 않다.

2026-04-16 Feature Gap Analysis (X6) 참조.

## 결정 (Decision)

`oneshim-core` 에 새 `RegimeStoragePort` 를, `oneshim-storage` 에 `SqliteRegimeManagerStateStore` 를 추가한다. 상태는 **전용(dedicated)** `regime_manager_state` 싱글톤(singleton) 테이블 (v31 마이그레이션) 의 JSON 블롭(blob) 으로 저장한다 — 기존 `regimes` 테이블은 사용하지 않는다.

시작 시 구성 루트(composition root) 가 `store.load_all()` → `RegimeManager::hydrate_from(regimes)` 를 호출한다. 정상 종료 시 `main.rs::RunEvent::Exit` 핸들러가 `store.save_all(&regime_manager.all_regimes())` 를 4 초 워치독(watchdog) 과 함께 호출한다.

파싱 실패 시 `load_all` 은 손상된 페이로드를 `payload_backup` 컬럼으로 격리(quarantine) 하며, `payload_backup_at` 타임스탬프를 기록하고 `error!` 로그를 남긴 뒤 `Ok(vec![])` 를 반환하여 앱은 새 상태로 시작한다. 사용자 큐레이션 상태는 이후 복구를 위해 보존된다.

## 결과 (Consequences)

### 긍정 (Positive)

- Regime 이 재시작에도 살아남는다 — 같은 클러스터에 대해 매 콜드 부트마다 "새 regime 발견" 알림이 재발생하지 않는다.
- Vector `regime_id` 필터 (C3a) 가 세션 경계를 넘어 의미를 갖는다 — regime ID 가 이제 안정적(stable) 이다.
- sync_merger 가 사용하는 기존 `regimes` 테이블은 건드리지 않는다.

### 부정 / 제약 (Negative / Constraints)

- JSON 블롭은 `Regime` 구조체의 진화에 따라 바뀐다. serde 의 `#[serde(default)]` 는 필드 추가(additive fields) 를 처리한다. 필드 제거/이름 변경은 격리 경로(quarantine path) 를 트리거한다. 스키마 불일치는 절대로 조용히 wipe 되지 않는다.
- `load_all` 은 격리 엣지 케이스에서 read-only 가 아니다. 문서가 호출자에게 경고하며, 모든 호출 지점은 시작 시 단발(single-shot) 이다.
- 종료 시 저장은 4 초 워치독 하의 best-effort 이다 — 텔레메트리(telemetry) 종료와 일치한다. 데드라인 초과 시 `warn!` 로그 후 진행한다 — 종료는 절대로 블록되어서는 안 된다.

### 중립 (Neutral)

- 런타임 주기적 저장(mid-life periodic save) 은 본 페이즈 범위 밖이다. 루틴 재시작 생존을 위해서는 종료 시점 저장만으로 충분하며, 콜드 킬(cold-kill) 데이터 손실이 문제가 되면 `run_maintenance` 틱 이후 저장을 추가하는 후속 페이즈가 가능하다.

## 대안 검토 (Alternatives considered)

- **기존 `regimes` 테이블 재사용** — 기각. 스키마가 부분적(centroid 없음, RegimeStatus enum 없음, 사용자 이름 오버라이드 없음) 이며 sync_merger 소유다. 확장하려면 마이그레이션과 쓰기 경로 업데이트가 필요해 동기화 일관성을 유지해야 한다. 신규 전용 테이블이 블라스트 반경(blast radius) 을 피한다.
- **JSON 블롭 대신 regime 별 행(row-per-regime)** — 기각. RegimeManager 의 regime 수는 `max_active + archive_days` 로 제한되므로 단일 블롭이 단순하며 비용은 무시 가능하다. 필요해지면 diff API 를 백워드 호환으로 후속 추가할 수 있다.
- **"파싱 실패 시 fresh start"** — 스펙 리뷰 중 명시적으로 기각. 수개월의 사용자 큐레이션 이름을 조용히 wipe 하는 것은 회귀(regression) 다. 격리(quarantine) 가 복구 경로를 보존한다.

## 참조 (References)

- 스펙: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`
- Gap 분석: `docs/reviews/2026-04-16-feature-gaps-analysis.md` C3 + X6
- ADR-016 ConfigChangeBus (종료 워치독 패턴)
