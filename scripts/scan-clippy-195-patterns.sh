#!/usr/bin/env bash
# scan-clippy-195-patterns.sh — Pre-flight grep scan for Rust 1.95 clippy
# lints that surface in CI but are easy to miss when the local toolchain lags.
#
# Background: CI uses `dtolnay/rust-toolchain@stable` which auto-picks the
# current stable. When stable ticks over, fresh lint errors appear. The
# 1.94 → 1.95 bump caused PRs #425/#426 to take 3 push rounds to clear
# because local clippy did not catch the new patterns.
#
# This is a fast (sub-second) grep-based pre-flight check that only flags
# patterns with **precise** signatures — i.e. signatures whose grep regex
# tracks clippy's actual rule closely enough to keep false-positive rate
# at or near zero on the current tree.
#
# Patterns covered:
#   1. unnecessary_sort_by — REVERSE sort with `b.field.cmp(&a.field)`.
#                            Detected via perl backreference so ascending
#                            sorts (`a.field.cmp(&b.field)`) are skipped.
#
# Patterns NOT covered (regex too noisy without context awareness — rely on
# `cargo clippy` for these):
#   - manual_while_let_some  (false positives in `tokio::select!` loops and
#                             loops with prelude statements before the let-else)
#   - collapsible_match
#   - field_reassign_with_default
#   - items_after_test_module
#   - ptr_arg                (clippy applies usage-site heuristics this script
#                             cannot replicate)
#
# When you do bump the local toolchain, follow up with the broader
# `cargo clippy --workspace --all-targets -- -D warnings` to catch the
# patterns this script intentionally skips.

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

declare -i exit_code=0

# 1. unnecessary_sort_by (Rust 1.95)
#    `v.sort_by(|a, b| b.field.cmp(&a.field))` → use `sort_by_key + Reverse`.
#    Perl regex with backreference: capture the two closure params, then
#    require body to be `<param2>.field.cmp(&<param1>.field)` — this matches
#    only REVERSE sorts and skips ascending ones.
UNNECESSARY_SORT_HITS=$(
    find crates src-tauri \
        -path '*/target' -prune -o \
        -name '*.rs' -type f -print 2>/dev/null \
        | xargs perl -nle '
            BEGIN { $found = 0 }
            if (/\.sort_by\(\|([a-z_][a-z_0-9]*),\s*([a-z_][a-z_0-9]*)\|\s*\2\.[a-z_0-9]+\.cmp\(&\1\./) {
                print "$ARGV:$.:$_";
                $found = 1;
            }
            END { exit($found ? 1 : 0) }
        ' 2>/dev/null
)
if [ -n "$UNNECESSARY_SORT_HITS" ]; then
    echo "[FAIL] unnecessary_sort_by — REVERSE-sort with .cmp() (Rust 1.95):"
    printf '%s\n' "$UNNECESSARY_SORT_HITS" | sed 's/^/   /'
    echo ""
    exit_code=1
fi

if [ "$exit_code" -ne 0 ]; then
    cat <<'EOF'
============================================================
Detected likely Rust 1.95 clippy violation(s).

Quick fix:
  unnecessary_sort_by:
    v.sort_by(|a, b| b.x.cmp(&a.x));
                ↓
    v.sort_by_key(|x| std::cmp::Reverse(x.x));

If a flagged line is a known false positive, run
  cargo clippy --workspace --all-targets -- -D warnings
to confirm. cargo clippy is authoritative; this scan exists only to
shortcut CI iteration cost when the local toolchain trails stable.
============================================================
EOF
fi

exit "$exit_code"
