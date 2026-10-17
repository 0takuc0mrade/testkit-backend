//! Regression test: avoid stale compatibility claims across workspace documentation and manifests.
//!
//! When `soroban-sdk` is updated, documentation (COMPATIBILITY.md, README.md, ARCHITECTURE.md)
//! and companion manifests (`crates/cli/Cargo.toml`'s `soroban-spec`) must remain synchronized
//! to prevent stale claims from confusing users or breaking CI.

mod common;

use common::read_repo_file;

#[test]
fn compatibility_matrix_reflects_pinned_sdk_and_workspace_version() {
    let workspace_cargo = read_repo_file("Cargo.toml");
    let sdk_version = workspace_cargo
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("soroban-sdk = ") {
                Some(
                    trimmed
                        .trim_start_matches("soroban-sdk = ")
                        .trim_matches('"'),
                )
            } else {
                None
            }
        })
        .expect("Cargo.toml must pin soroban-sdk");

    let workspace_version = workspace_cargo
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("version = ") {
                Some(trimmed.trim_start_matches("version = ").trim_matches('"'))
            } else {
                None
            }
        })
        .expect("Cargo.toml must specify workspace version");

    let compat_md = read_repo_file("COMPATIBILITY.md");
    assert!(
        compat_md.contains(sdk_version),
        "COMPATIBILITY.md must list the active pinned soroban-sdk version: {sdk_version}"
    );
    assert!(
        compat_md.contains(workspace_version),
        "COMPATIBILITY.md must list the current workspace version: {workspace_version}"
    );
}

#[test]
fn cli_soroban_spec_matches_workspace_soroban_sdk() {
    let workspace_cargo = read_repo_file("Cargo.toml");
    let sdk_version = workspace_cargo
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("soroban-sdk = ") {
                Some(
                    trimmed
                        .trim_start_matches("soroban-sdk = ")
                        .trim_matches('"'),
                )
            } else {
                None
            }
        })
        .expect("Cargo.toml must pin soroban-sdk");

    let cli_cargo = read_repo_file("crates/cli/Cargo.toml");
    let spec_version = cli_cargo
        .lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("soroban-spec = ") {
                Some(
                    trimmed
                        .trim_start_matches("soroban-spec = ")
                        .trim_matches('"'),
                )
            } else {
                None
            }
        })
        .expect("crates/cli/Cargo.toml must pin soroban-spec");

    assert_eq!(
        spec_version, sdk_version,
        "crates/cli/Cargo.toml soroban-spec must match Cargo.toml soroban-sdk to prevent version drift"
    );
}

#[test]
fn docs_workflow_verifies_compatibility_without_stale_claims() {
    let docs_yml = read_repo_file(".github/workflows/docs.yml");
    assert!(
        docs_yml.contains("COMPATIBILITY.md"),
        ".github/workflows/docs.yml must verify COMPATIBILITY.md"
    );
    assert!(
        docs_yml.contains("soroban-spec"),
        ".github/workflows/docs.yml must ensure crates/cli soroban-spec matches soroban-sdk"
    );
}

#[test]
fn readme_and_architecture_link_compatibility_matrix() {
    let readme = read_repo_file("README.md");
    assert!(
        readme.contains("COMPATIBILITY.md"),
        "README.md must link to COMPATIBILITY.md for authoritative compatibility information"
    );

    let architecture = read_repo_file("ARCHITECTURE.md");
    assert!(
        architecture.contains("scheduled-sdk-check"),
        "ARCHITECTURE.md must document scheduled-sdk-check guarding upstream SDK updates"
    );
}
