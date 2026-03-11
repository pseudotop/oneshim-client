#!/usr/bin/env bash
# release.sh — ONESHIM 클라이언트 RC 릴리스 자동화 스크립트
#
# 사용법: ./scripts/release.sh <버전>
#   예시: ./scripts/release.sh 0.3.7-rc.1
#
# 전제 조건:
#   - main 브랜치에 있을 것
#   - 작업 디렉터리가 클린 상태일 것 (커밋되지 않은 변경 없음)
#   - CHANGELOG.md의 [Unreleased] 섹션에 실제 내용이 있을 것
#   - git-cliff가 PATH에 있거나 ~/.cargo/bin에 있을 것
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
source "${SCRIPT_DIR}/release-common.sh"

# ── 색상 출력 헬퍼 ─────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info()    { echo -e "${BLUE}[info]${NC}  $*"; }
success() { echo -e "${GREEN}[ok]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[warn]${NC}  $*"; }
error()   { echo -e "${RED}[error]${NC} $*" >&2; }
die()     { error "$*"; exit 1; }

# ── 인수 검증 ──────────────────────────────────────────────────────────────────
if [[ $# -lt 1 ]]; then
    echo "사용법: $0 <버전>" >&2
    echo "  예시: $0 0.3.7-rc.1" >&2
    exit 1
fi

VERSION="$1"

# RC-only 릴리스 강제
if ! is_rc_version "$VERSION"; then
    die "release.sh는 RC 버전만 허용합니다: '$VERSION' (올바른 형식: x.y.z-rc.N)"
fi

TAG="v${VERSION}"
BASE_VERSION="$(base_version "${VERSION}")"

info "RC 릴리스 준비 시작: ${TAG}"

# ── git-cliff 경로 탐색 ────────────────────────────────────────────────────────
# CI 환경: /usr/local/bin/git-cliff (taiki-e/install-action 설치)
# 로컬 환경: PATH 또는 ~/.cargo/bin
if command -v git-cliff &>/dev/null; then
    GIT_CLIFF="git-cliff"
elif [[ -x "/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.cargo/bin/git-cliff" ]]; then
    GIT_CLIFF="/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.cargo/bin/git-cliff"
elif [[ -x "${HOME}/.cargo/bin/git-cliff" ]]; then
    GIT_CLIFF="${HOME}/.cargo/bin/git-cliff"
else
    die "git-cliff를 찾을 수 없습니다. 설치 방법: cargo install git-cliff"
fi
info "git-cliff 경로: ${GIT_CLIFF}"

# ── 작업 디렉터리: 스크립트가 있는 레포 루트로 이동 ──────────────────────────
cd "${REPO_ROOT}"
info "레포 루트: ${REPO_ROOT}"

# ── [Unreleased] 섹션 내용 검증 (모든 검사 중 가장 먼저) ─────────────────────
# CI가 실행되기 전에 조기 종료해야 함
CHANGELOG="CHANGELOG.md"
if [[ ! -f "${CHANGELOG}" ]]; then
    die "${CHANGELOG} 파일이 없습니다"
fi

# [Unreleased] 헤딩과 다음 ## 헤딩 사이의 내용을 추출
UNRELEASED_CONTENT=$(awk '/^## \[Unreleased\]/{found=1; next} found && /^## \[/{exit} found{print}' "${CHANGELOG}" | grep -v '^[[:space:]]*$' || true)

if [[ -z "${UNRELEASED_CONTENT}" ]]; then
    die "[Unreleased] 섹션이 비어 있습니다. CHANGELOG.md에 변경 내용을 먼저 추가하세요.
힌트: git-cliff --unreleased --prepend CHANGELOG.md 를 실행하면 자동으로 채워집니다."
fi
success "[Unreleased] 섹션에 내용이 있습니다"

# ── 현재 브랜치 검증 ──────────────────────────────────────────────────────────
if ! require_main_branch; then
    CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    die "main 브랜치에서만 릴리스할 수 있습니다 (현재: ${CURRENT_BRANCH})"
fi
success "브랜치 확인: main"

# ── 작업 디렉터리 클린 상태 검증 ──────────────────────────────────────────────
if ! require_clean_worktree; then
    die "커밋되지 않았거나 스테이징된 변경사항이 있습니다. 먼저 정리하세요."
fi
success "작업 디렉터리 클린 상태 확인"

# ── 태그 중복 검증 ────────────────────────────────────────────────────────────
if git rev-parse "${TAG}" &>/dev/null; then
    die "태그 '${TAG}'가 이미 존재합니다"
fi
if git rev-parse "v${BASE_VERSION}" &>/dev/null; then
    die "stable 태그 'v${BASE_VERSION}'가 이미 존재합니다. stable 이후에는 새 RC를 만들 수 없습니다"
fi

# ── CHANGELOG.md 생성 (git-cliff로 전체 재생성) ───────────────────────────────
info "CHANGELOG.md를 git-cliff로 갱신합니다 (태그: ${TAG})..."
"${GIT_CLIFF}" --tag "${TAG}" -o "${CHANGELOG}"
success "CHANGELOG.md 갱신 완료"

# ── [Unreleased] 헤더가 상단에 유지되는지 확인 ───────────────────────────────
# git-cliff는 전체 파일을 재생성하므로 [Unreleased] 섹션을 앞에 추가
# cliff.toml 설정에 따라 이미 포함되어 있어야 하지만 안전을 위해 확인
if ! grep -q "## \[Unreleased\]" "${CHANGELOG}"; then
    warn "[Unreleased] 헤더가 없습니다. 수동으로 추가합니다..."
    # 첫 번째 버전 헤딩 앞에 [Unreleased] 삽입
    sed -i.bak "s/^## \[${VERSION}\]/## [Unreleased]\n\n## [${VERSION}]/" "${CHANGELOG}"
    rm -f "${CHANGELOG}.bak"
    success "[Unreleased] 헤더 추가 완료"
fi

# ── 버전 파일 동기화 ──────────────────────────────────────────────────────────
info "버전 파일을 ${VERSION}으로 동기화합니다..."
set_workspace_version "${VERSION}"
set_frontend_version "${VERSION}"

UPDATED_CARGO_VERSION="$(workspace_version)"
UPDATED_FRONTEND_VERSION="$(frontend_version)"
if [[ "${UPDATED_CARGO_VERSION}" != "${VERSION}" ]]; then
    die "Cargo.toml 버전 업데이트 실패 (현재: ${UPDATED_CARGO_VERSION}, 기대: ${VERSION})"
fi
if [[ "${UPDATED_FRONTEND_VERSION}" != "${VERSION}" ]]; then
    die "frontend/package.json 버전 업데이트 실패 (현재: ${UPDATED_FRONTEND_VERSION}, 기대: ${VERSION})"
fi
if ! changelog_has_entry "${VERSION}"; then
    die "CHANGELOG.md에 [${VERSION}] 섹션이 없습니다"
fi
success "버전 파일 동기화 완료: ${VERSION}"

# ── 변경사항 커밋 ─────────────────────────────────────────────────────────────
info "변경사항을 커밋합니다..."
git add "${CHANGELOG}" Cargo.toml crates/oneshim-web/frontend/package.json
git commit -m "chore(release): ${TAG}"
success "커밋 완료"

# ── 태그 생성 ─────────────────────────────────────────────────────────────────
info "태그를 생성합니다: ${TAG}"
git tag -a "${TAG}" -m "Release ${TAG}"
success "태그 생성 완료: ${TAG}"

# ── 원격 저장소에 푸시 ────────────────────────────────────────────────────────
info "main 브랜치를 origin에 푸시합니다..."
git push origin main
success "main 브랜치 푸시 완료"

info "태그 ${TAG}를 origin에 푸시합니다..."
git push origin "${TAG}"
success "태그 푸시 완료"

# ── 완료 메시지 ───────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  RC 릴리스 완료: ${TAG}${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  GitHub Releases (CI가 완료되면 자동 생성됩니다):"
echo "  https://github.com/pseudotop/oneshim-client/releases/tag/${TAG}"
echo ""
echo "  릴리스 빌드 CI 상태:"
echo "  https://github.com/pseudotop/oneshim-client/actions"
echo ""
echo "  RC 검증이 끝나면 stable 승격:"
echo "  ./scripts/promote-stable.sh ${VERSION}"
echo ""
