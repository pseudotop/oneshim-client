use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

#[test]
fn gitignore_covers_tauri_generated_sidecar_binaries() {
    let root = repo_root();
    let gitignore = fs::read_to_string(root.join(".gitignore")).expect(".gitignore is readable");

    assert!(
        gitignore
            .lines()
            .any(|line| line.trim() == "src-tauri/binaries/oneshim-sandbox-worker*"),
        ".gitignore should ignore Tauri-generated sandbox worker sidecars under src-tauri/binaries/"
    );
}

#[test]
fn release_reliability_smoke_can_require_signature_verification() {
    let root = repo_root();
    let script = fs::read_to_string(root.join("scripts/release-reliability-smoke.sh"))
        .expect("release reliability smoke script is readable");

    assert!(
        script.contains("ONESHIM_SMOKE_REQUIRE_SIGNATURE"),
        "release smoke should expose an env override for requiring signatures"
    );
    assert!(
        script.contains("--require-signature"),
        "release smoke should document and pass through --require-signature"
    );
    assert!(
        script.contains("SIGNATURE_PATH=\"$ARTIFACT_PATH.sig\""),
        "release smoke should resolve the expected signature sidecar path"
    );
    assert!(
        script.contains("[[ -f \"$SIGNATURE_PATH\" ]] || fatal"),
        "release smoke should fail early when signature verification is required but the sidecar is missing"
    );
    assert!(
        script.contains("INSTALL_ARGS+=(--require-signature)"),
        "release smoke should invoke the installer in fail-closed signature mode"
    );
}

#[test]
fn release_workflow_runs_signed_installer_smoke_before_publishing() {
    let root = repo_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("release workflow is readable");

    let sign_step = workflow
        .find("Sign release artifacts (Ed25519)")
        .expect("release workflow should sign artifacts");
    let smoke_step = workflow
        .find("Run signed release reliability smoke")
        .expect("release workflow should smoke signed installer verification");

    assert!(
        sign_step < smoke_step,
        "signed release smoke should run after Ed25519 signatures are generated"
    );
    assert!(
        workflow.contains(
            "./scripts/release-reliability-smoke.sh --assets-dir dist --asset-name oneshim-linux-x64.tar.gz --skip-updater-tests --require-signature"
        ),
        "release workflow should run installer smoke in fail-closed signature mode before publishing"
    );
}
