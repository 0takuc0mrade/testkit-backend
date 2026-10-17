//! Regression test: release tag matches both workspace crate versions.
//!
//! `BUILD_SPEC.md` and `RELEASING.md` require both published crates
//! (`soroban-testkit` and `soroban-testkit-cli`) to remain version-locked
//! and released together from one git tag. The release workflow uses
//! `.github/scripts/verify-release.sh` to enforce that any release tag
//! agrees with both crate manifests before publishing.

mod common;

use common::read_repo_file;

#[test]
fn release_workflow_runs_verify_release_script() {
    let release_workflow = read_repo_file(".github/workflows/release.yml");
    assert!(
        release_workflow.contains(".github/scripts/verify-release.sh"),
        ".github/workflows/release.yml must run `.github/scripts/verify-release.sh` \
         to verify that a release tag matches both crate versions."
    );
}

#[test]
fn releasing_docs_document_both_crate_versions_must_match() {
    let releasing = read_repo_file("RELEASING.md");
    assert!(
        releasing.contains(".github/scripts/verify-release.sh"),
        "RELEASING.md must document that `.github/scripts/verify-release.sh` enforces \
         that git tags match both published crate versions."
    );
    assert!(
        releasing.contains("crates/testkit") && releasing.contains("crates/cli"),
        "RELEASING.md must explicitly name both published crates."
    );
}

#[test]
fn verify_release_script_checks_both_crates() {
    let script = read_repo_file(".github/scripts/verify-release.sh");
    assert!(
        script.contains("set -euo pipefail"),
        "verify-release.sh must use `set -euo pipefail`."
    );
    assert!(
        script.contains("crates/testkit") && script.contains("crates/cli"),
        "verify-release.sh must check both `crates/testkit` and `crates/cli`."
    );
    assert!(
        script.contains("workspace_version"),
        "verify-release.sh must check workspace_version."
    );
    assert!(
        script.contains("version.workspace"),
        "verify-release.sh must handle inherited workspace versioning."
    );
}

#[test]
fn workspace_and_crates_currently_agree_on_version() {
    let root_cargo = read_repo_file("Cargo.toml");
    let workspace_ver = root_cargo
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("version = ") {
                Some(trimmed.trim_start_matches("version = ").trim_matches('"'))
            } else {
                None
            }
        })
        .expect("Cargo.toml must have a workspace version");

    for crate_manifest in ["crates/testkit/Cargo.toml", "crates/cli/Cargo.toml"] {
        let content = read_repo_file(crate_manifest);
        let inherits = content.contains("version.workspace = true");
        let explicit = content.lines().find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("version = ") {
                Some(trimmed.trim_start_matches("version = ").trim_matches('"'))
            } else {
                None
            }
        });

        let crate_ver = if inherits {
            workspace_ver
        } else {
            explicit.expect("crate must define or inherit version")
        };

        assert_eq!(
            crate_ver, workspace_ver,
            "{crate_manifest} version must match workspace version {workspace_ver}"
        );
    }
}
