# Regression Tests

Every bug fix MUST include a reproducer test in this directory before merge.

## Naming Convention
- `gh{issue_number}_{short_description}.rs`
- Example: `gh142_deep_merge_null_array.rs`

## Rules
1. Test must FAIL on the buggy code and PASS on the fix
2. Tests are never deleted — only quarantined with `#[ignore]` + issue link
3. Each test documents the original bug report URL
4. Run with: `cargo test --test '*'` (integration test style)
