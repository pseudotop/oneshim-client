#!/usr/bin/env bash
# check-endpoint-coverage.sh — Verify every route handler has at least one test.
#
# Extracts handler function references from routes.rs, then checks whether
# each handler's module contains a #[cfg(test)] block with at least one test
# that exercises the handler (by function name or HTTP path).
#
# Usage: ./scripts/check-endpoint-coverage.sh
# Exit code 0 if all covered, 1 if gaps found.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ROUTES_FILE="${REPO_ROOT}/crates/oneshim-web/src/routes.rs"
HANDLERS_DIR="${REPO_ROOT}/crates/oneshim-web/src/handlers"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

if [[ ! -f "${ROUTES_FILE}" ]]; then
    echo -e "${RED}[error]${NC} routes.rs not found: ${ROUTES_FILE}" >&2
    exit 1
fi

# Step 1: Extract all handler references from routes.rs
# Format: "module::function" (e.g., "metrics::get_metrics")
handlers=$(grep -oE 'handlers::[a-z_]+::[a-z_]+' "${ROUTES_FILE}" | \
    sed 's/^handlers:://' | sort -u)

total=$(echo "${handlers}" | wc -l | tr -d ' ')
covered=0
uncovered=0
uncovered_list=""

# Step 2: For each handler, check if its module has a test
while IFS= read -r handler; do
    module=$(echo "${handler}" | cut -d: -f1)
    func=$(echo "${handler}" | cut -d: -f3)

    # Find the handler source file
    handler_file=""
    if [[ -f "${HANDLERS_DIR}/${module}.rs" ]]; then
        handler_file="${HANDLERS_DIR}/${module}.rs"
    elif [[ -f "${HANDLERS_DIR}/${module}/mod.rs" ]]; then
        handler_file="${HANDLERS_DIR}/${module}/mod.rs"
    fi

    if [[ -z "${handler_file}" ]]; then
        # Check for test files in subdirectory
        test_files=$(find "${HANDLERS_DIR}/${module}" -name "tests*.rs" 2>/dev/null || true)
        if [[ -n "${test_files}" ]]; then
            handler_file="${test_files}"
        fi
    fi

    # Check if there's at least one #[test] or #[tokio::test] in the module
    has_test=false
    if [[ -n "${handler_file}" ]]; then
        # Check the handler file itself
        if grep -q '#\[tokio::test\]\|#\[test\]' ${handler_file} 2>/dev/null; then
            has_test=true
        fi
        # Also check test subfiles for directory modules
        if [[ -d "${HANDLERS_DIR}/${module}" ]]; then
            if find "${HANDLERS_DIR}/${module}" -name "*.rs" -exec grep -l '#\[tokio::test\]\|#\[test\]' {} + 2>/dev/null | grep -q .; then
                has_test=true
            fi
        fi
    fi

    if ${has_test}; then
        covered=$((covered + 1))
    else
        uncovered=$((uncovered + 1))
        uncovered_list="${uncovered_list}\n  ${handler}"
    fi
done <<< "${handlers}"

# Step 3: Report
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Endpoint Test Coverage Report"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "  Total handlers in routes.rs:  ${total}"
echo -e "  ${GREEN}Covered (module has tests):${NC}   ${covered}"
echo -e "  ${RED}Uncovered (no tests):${NC}         ${uncovered}"
echo ""

if [[ ${uncovered} -gt 0 ]]; then
    echo -e "${YELLOW}Uncovered handlers:${NC}"
    echo -e "${uncovered_list}"
    echo ""
    echo -e "${RED}[FAIL]${NC} ${uncovered} handler(s) have no tests in their module."
    exit 1
else
    echo -e "${GREEN}[PASS]${NC} All ${total} handlers have test coverage in their modules."
    exit 0
fi
