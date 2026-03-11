#!/usr/bin/env bash
# Shared helpers for rc-first release automation.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORKSPACE_CARGO_TOML="${REPO_ROOT}/Cargo.toml"
FRONTEND_PACKAGE_JSON="${REPO_ROOT}/crates/oneshim-web/frontend/package.json"
CHANGELOG_PATH="${REPO_ROOT}/CHANGELOG.md"

is_rc_version() {
  [[ "${1}" =~ ^[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]
}

is_stable_version() {
  [[ "${1}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

base_version() {
  printf '%s\n' "${1%%-rc.*}"
}

workspace_version() {
  grep -m1 '^version' "${WORKSPACE_CARGO_TOML}" | sed 's/.*"\(.*\)"/\1/'
}

frontend_version() {
  python3 - "${FRONTEND_PACKAGE_JSON}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
print(json.loads(path.read_text(encoding="utf-8"))["version"])
PY
}

set_workspace_version() {
  python3 - "${WORKSPACE_CARGO_TOML}" "${1}" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
content = path.read_text(encoding="utf-8")

lines = content.splitlines()
out = []
in_workspace_package = False
replaced = False

for line in lines:
    if line.strip() == "[workspace.package]":
        in_workspace_package = True
        out.append(line)
        continue
    if in_workspace_package and line.startswith("[") and line.strip() != "[workspace.package]":
        in_workspace_package = False
    if in_workspace_package and re.match(r'^version\s*=\s*"[^"]+"$', line):
        out.append(f'version = "{version}"')
        replaced = True
    else:
        out.append(line)

if not replaced:
    raise SystemExit("Could not find [workspace.package] version field in Cargo.toml")

path.write_text("\n".join(out) + "\n", encoding="utf-8")
PY
}

set_frontend_version() {
  python3 - "${FRONTEND_PACKAGE_JSON}" "${1}" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
version = sys.argv[2]
data = json.loads(path.read_text(encoding="utf-8"))
data["version"] = version
path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY
}

changelog_has_entry() {
  grep -q "^## \[${1}\]" "${CHANGELOG_PATH}"
}

copy_changelog_section() {
  python3 - "${CHANGELOG_PATH}" "${1}" "${2}" "$(date +%Y-%m-%d)" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
source_version = sys.argv[2]
target_version = sys.argv[3]
target_date = sys.argv[4]
lines = path.read_text(encoding="utf-8").splitlines(keepends=True)


def find_section(version: str):
    header = re.compile(rf"^## \[{re.escape(version)}\](?: - .*)?\n?$")
    start = None
    for idx, line in enumerate(lines):
        if header.match(line):
            start = idx
            break
    if start is None:
        return None
    end = len(lines)
    for idx in range(start + 1, len(lines)):
        if lines[idx].startswith("## ["):
            end = idx
            break
    return start, end


source = find_section(source_version)
if source is None:
    raise SystemExit(f"CHANGELOG.md missing source section [{source_version}]")

target = find_section(target_version)
source_body = "".join(lines[source[0] + 1:source[1]]).strip()

if target is not None:
    target_body = "".join(lines[target[0] + 1:target[1]]).strip()
    if target_body != source_body:
        raise SystemExit(
            f"CHANGELOG.md already has [{target_version}], but its body differs from [{source_version}]"
        )
    print(f"CHANGELOG.md already contains [{target_version}] with matching content")
    raise SystemExit(0)

insert_block = [f"## [{target_version}] - {target_date}\n"]
insert_block.extend(lines[source[0] + 1:source[1]])
if insert_block and insert_block[-1].strip():
    insert_block.append("\n")

lines[source[0]:source[0]] = insert_block
path.write_text("".join(lines), encoding="utf-8")
PY
}

changelog_section_body_matches() {
  python3 - "${CHANGELOG_PATH}" "${1}" "${2}" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
left_version = sys.argv[2]
right_version = sys.argv[3]
lines = path.read_text(encoding="utf-8").splitlines()


def body(version: str):
    header = re.compile(rf"^## \[{re.escape(version)}\](?: - .*)?$")
    start = None
    for idx, line in enumerate(lines):
        if header.match(line):
            start = idx
            break
    if start is None:
        return None
    end = len(lines)
    for idx in range(start + 1, len(lines)):
        if lines[idx].startswith("## ["):
            end = idx
            break
    return "\n".join(lines[start + 1:end]).strip()


left = body(left_version)
right = body(right_version)
if left is None or right is None:
    raise SystemExit(1)
raise SystemExit(0 if left == right else 1)
PY
}

require_main_branch() {
  local current_branch
  current_branch="$(git rev-parse --abbrev-ref HEAD)"
  [[ "${current_branch}" == "main" ]]
}

require_clean_worktree() {
  git diff --quiet && git diff --cached --quiet
}

ensure_head_matches_tag() {
  local tag="$1"
  local tag_commit
  tag_commit="$(git rev-parse "${tag}^{commit}")"
  [[ "$(git rev-parse HEAD)" == "${tag_commit}" ]]
}

latest_rc_tag_for_base() {
  git tag -l "v${1}-rc.*" --sort=-version:refname | head -n1
}

allowed_promotion_file() {
  case "${1}" in
    Cargo.toml|Cargo.lock|CHANGELOG.md|crates/oneshim-web/frontend/package.json)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}
