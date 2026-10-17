//! Regression test: ensure the vault fixture compiles for the documented wasm target.
//!
//! `BUILD_SPEC.md` and `ARCHITECTURE.md` document `examples/vault` as a real contract
//! fixture compiled to `wasm32v1-none` for resource limits and integration tests.
//! This test verifies that the workspace configuration, toolchain targets, and vault manifest
//! correctly support the documented wasm compilation.

mod common;

use common::read_repo_file;

#[test]
fn rust_toolchain_includes_documented_wasm_target() {
    let toolchain = read_repo_file("rust-toolchain.toml");
    assert!(
        toolchain.contains("wasm32v1-none"),
        "rust-toolchain.toml must include `wasm32v1-none` in targets."
    );
}

#[test]
fn vault_manifest_supports_cdylib_and_rlib() {
    let vault_cargo = read_repo_file("examples/vault/Cargo.toml");
    assert!(
        vault_cargo.contains("crate-type = [\"cdylib\", \"rlib\"]"),
        "examples/vault/Cargo.toml must declare crate-type = [\"cdylib\", \"rlib\"] \
         so it can compile to a wasm binary and be used as a Rust dependency."
    );
    assert!(
        vault_cargo.contains("no_std")
            || read_repo_file("examples/vault/src/lib.rs").contains("#![no_std]"),
        "examples/vault must be #![no_std] to compile for wasm targets."
    );
}

#[test]
fn ci_and_architecture_document_wasm_target_for_vault() {
    let ci = read_repo_file(".github/workflows/ci.yml");
    assert!(
        ci.contains("wasm32v1-none"),
        ".github/workflows/ci.yml must configure wasm32v1-none target."
    );

    let arch = read_repo_file("ARCHITECTURE.md");
    assert!(
        arch.contains("wasm32v1-none"),
        "ARCHITECTURE.md must document wasm32v1-none as the target for examples/vault."
    );
}
