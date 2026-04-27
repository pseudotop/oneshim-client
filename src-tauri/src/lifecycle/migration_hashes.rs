//! Known SHA-256 hashes of prior-version systemd service file templates.
//!
//! Used by autostart_migration to determine whether an existing
//! `~/.config/systemd/user/oneshim.service` file matches a known template
//! (safe to overwrite) vs has been customized by the user (skip).

// Functions and constants are consumed by autostart_migration (wired in Task 9).
#![allow(dead_code)]

use sha2::{Digest, Sha256};

/// (hash, label) pairs for every released template content prior to PR-B2.
///
/// Hashes are computed by `compute_hash(canonicalize(template, binary_path))`
/// where binary_path is the resolved current_exe() path.
pub const KNOWN_PRIOR_HASHES: &[(&str, &str)] = &[
    // PR-B1 Type=simple (Maekon brand, v0.4.40-rc.3+ and v0.4.40 stable)
    (
        "9b1f5d384dc9246228a5601ac67de4127c7f5fdf9a24b2f447c78d1bb671047f",
        "PR-B1 Type=simple",
    ),
    // PR-B1 Type=simple (pre-rebrand ONESHIM brand, v0.4.40-rc.1 and v0.4.40-rc.2)
    (
        "beebd724df50096241ffe040d3fc044dc93493ba3ae265302bc2c3a3215c59ca",
        "PR-B1 Type=simple (pre-rebrand)",
    ),
];

/// Canonicalize the service file before hashing.
///
/// Handles both line-ending normalization AND word-boundary-aware
/// ExecStart line replacement to avoid:
/// - Binary path substring collision: `/home/user/oneshim` would match
///   `/home/user/oneshim-old` with a naive string replace.
/// - Line-ending variation: `\r\n` (Windows-edited files) producing a
///   different hash than `\n`.
///
/// Symlink edge case: if the user wrote the service file using a symlink path
/// but `current_exe()` returns the canonical path (or vice versa), the ExecStart
/// line won't match and the file will be treated as customized — log warn + skip.
/// This is acceptable behavior.
pub fn canonicalize(content: &str, binary_path: &str) -> String {
    // Step 1: normalize line endings
    let normalized = content.replace("\r\n", "\n");
    // Step 2: replace ExecStart line specifically (not arbitrary substring)
    let exec_line = format!("ExecStart={}\n", binary_path);
    normalized.replace(&exec_line, "ExecStart={BINARY_PATH}\n")
}

/// Compute a lowercase hex SHA-256 digest of the given string.
pub fn compute_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in digest.as_slice() {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

/// Returns `Some(label)` if the canonicalized content matches a known prior hash.
/// Returns `None` if the user has customized the file (or it is the new template already).
pub fn matches_known_template(content: &str, binary_path: &str) -> Option<&'static str> {
    let canonical = canonicalize(content, binary_path);
    let hash = compute_hash(&canonical);
    KNOWN_PRIOR_HASHES
        .iter()
        .find(|(known_hash, _)| *known_hash == hash)
        .map(|(_, label)| *label)
}
