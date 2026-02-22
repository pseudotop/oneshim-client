# Standalone 무결성 베이스라인

이 문서는 ONESHIM Client의 standalone 모드에서 반드시 지켜야 하는 무결성 기준을 정의합니다.

목표는 현재 standalone 신뢰 모델을 엄격하게 유지하면서, 추후 서버/서드파티 연동 시 아키텍처를 재설계하지 않고 확장 가능하게 만드는 것입니다.

## 보안 목표

- 무결성 검증 실패 시 fail-closed 동작
- 업데이트/설치 신뢰 체인을 암호학 검증으로 고정
- 릴리즈 단위 공급망 증거를 기계적으로 검증 가능하게 유지
- 미래 원격 연동을 위한 계약 경계의 안정성 확보

## 필수 통제 항목

### 1) 업데이트 무결성

- 업데이트가 활성화된 경우 `update.require_signature_verification`은 반드시 true여야 합니다.
- `update.signature_public_key`는 유효한 Ed25519 공개키(디코딩 후 32바이트)여야 합니다.
- 아티팩트는 설치 전에 SHA-256 + Ed25519 검증을 모두 통과해야 합니다.
- 모든 릴리즈 아티팩트에 `.sig`, `.sha256`를 함께 생성/배포해야 합니다.

### 2) 공급망 무결성

- RustSec: `cargo audit`
- 의존성 정책: `cargo deny check licenses advisories sources bans`
- vet 정책: `cargo vet check`
- SBOM: `cargo cyclonedx --workspace`
- Provenance: 릴리즈 아티팩트 Attestation 생성

### 3) 런타임 경계 규칙

- Web 핸들러에서 SQLite 내부 직접 접근(`conn_ref`)을 금지합니다.
- Web 핸들러 데이터 접근은 storage adapter API를 통해서만 수행합니다.
- 무결성 민감 동작은 warn-and-continue가 아니라 fail-closed로 처리합니다.

### 4) 문서화 및 감사 추적성

- 무결성 정책 변경 시 본 문서와 `docs/security/integrity-runbook.md`를 함께 갱신해야 합니다.
- 취약점 신고/공개 정책은 `SECURITY.md`를 따릅니다.

## 로컬 검증 명령

```bash
./scripts/verify-integrity.sh
```

이 명령은 무결성 정책 테스트, 서명 검증 테스트, 공급망 검사, SBOM 생성을 한 번에 검증합니다.

## 미래 연동 준비 (Server / Third-Party)

standalone 단계에서는 즉시 필수는 아니지만, 이 단계부터 설계 제약으로 유지해야 합니다.

- 요청 계약에 `nonce`, `timestamp`, `key_id`, `sig` 필드 예약
- 재전송 방지(replay protection) 가능한 프로토콜 의미론 유지
- capability 기반 최소권한 서드파티 계약
- 루트키/온라인키 분리 및 키 롤오버 절차 문서화
