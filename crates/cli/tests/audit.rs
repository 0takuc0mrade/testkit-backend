//! Integration tests for `soroban-testkit audit`.
//!
//! Exercises the CLI binary's documented scan roots (directory, single file,
//! and stdin) and its directory exclusions (`target/`, `node_modules/`, and
//! hidden directories), plus the actionable error for a missing path.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_soroban-testkit"))
        .args(args)
        .current_dir(workspace_root())
        .output()
        .expect("failed to run soroban-testkit")
}

fn run_with_stdin(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_soroban-testkit"))
        .args(args)
        .current_dir(workspace_root())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run soroban-testkit");
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), stdin).unwrap();
    child.wait_with_output().unwrap()
}

/// A source snippet that trips `missing-require-auth`.
const UNAUTHORIZED_ENTRY_POINT: &str = r#"
#[contractimpl]
impl Contract {
    pub fn withdraw(env: Env, from: Address) {
        env.storage().instance().set(&from, &1i128);
    }
}
"#;

#[test]
fn audit_help_documents_supported_scan_roots_and_exclusions() {
    let output = run(&["audit", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Scan roots.
    assert!(stdout.contains("stdin"), "{stdout}");
    assert!(stdout.contains("directory"), "{stdout}");
    // Documented exclusions.
    assert!(stdout.contains("target/"), "{stdout}");
    assert!(stdout.contains("node_modules/"), "{stdout}");
    assert!(stdout.contains("hidden"), "{stdout}");
    // Standard options remain documented.
    assert!(stdout.contains("--format"), "{stdout}");
    assert!(stdout.contains("--strict"), "{stdout}");
}

#[test]
fn audit_scans_a_directory_recursively() {
    let root = std::env::temp_dir().join(format!("stk-audit-dir-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src/nested")).unwrap();
    std::fs::write(root.join("src/lib.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();
    std::fs::write(root.join("src/nested/more.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();

    let output = run(&["audit", root.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("src/lib.rs"), "{stdout}");
    assert!(stdout.contains("src/nested/more.rs"), "{stdout}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_skips_excluded_directories() {
    let root = std::env::temp_dir().join(format!("stk-audit-exclude-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(root.join(".git/objects")).unwrap();
    std::fs::write(root.join("src/vault.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();
    std::fs::write(
        root.join("target/debug/generated.rs"),
        UNAUTHORIZED_ENTRY_POINT,
    )
    .unwrap();
    std::fs::write(
        root.join("node_modules/pkg/dep.rs"),
        UNAUTHORIZED_ENTRY_POINT,
    )
    .unwrap();
    std::fs::write(root.join(".git/objects/blob.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();

    let output = run(&["audit", root.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("src/vault.rs"), "{stdout}");
    // Excluded trees must never be scanned.
    assert!(!stdout.contains("generated.rs"), "{stdout}");
    assert!(!stdout.contains("dep.rs"), "{stdout}");
    assert!(!stdout.contains("blob.rs"), "{stdout}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_scans_a_single_file_root() {
    let root = std::env::temp_dir().join(format!("stk-audit-single-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("only.rs");
    std::fs::write(&file, UNAUTHORIZED_ENTRY_POINT).unwrap();

    let output = run(&["audit", file.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("only.rs"), "{stdout}");
    assert!(stdout.contains("missing-require-auth"), "{stdout}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_scans_stdin_with_dash() {
    let output = run_with_stdin(&["audit", "-"], UNAUTHORIZED_ENTRY_POINT.as_bytes());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("<stdin>"), "{stdout}");
    assert!(stdout.contains("missing-require-auth"), "{stdout}");
}

#[test]
fn audit_reports_a_missing_path_actionably() {
    let missing = std::env::temp_dir()
        .join("stk-audit-path-that-does-not-exist")
        .join("nope");
    let output = run(&["audit", missing.to_str().unwrap()]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(stderr.contains("stdin"), "{stderr}");
}

#[test]
fn audit_json_output_reports_findings_in_a_temp_crate() {
    let root = std::env::temp_dir().join(format!("stk-audit-json-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/vault.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();

    let output = run(&["audit", root.to_str().unwrap(), "--format", "json"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["tool"], "soroban-testkit");
    assert!(report["summary"]["finding_count"].as_u64().unwrap() >= 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_sarif_output_reports_findings_and_rules() {
    let root = std::env::temp_dir().join(format!("stk-audit-sarif-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/vault.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();

    let output = run(&["audit", root.to_str().unwrap(), "--format", "sarif"]);
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["version"], "2.1.0");
    assert_eq!(
        report["runs"][0]["tool"]["driver"]["name"],
        "soroban-testkit"
    );
    let results = report["runs"][0]["results"].as_array().unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0]["ruleId"], "missing-require-auth");
    assert!(
        results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .unwrap()
            .contains("vault.rs")
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_diagnostic_formats_agree_on_finding_counts() {
    let root = std::env::temp_dir().join(format!("stk-audit-diag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/vault.rs"), UNAUTHORIZED_ENTRY_POINT).unwrap();

    let text_out = run(&["audit", root.to_str().unwrap(), "--format", "text"]);
    let json_out = run(&["audit", root.to_str().unwrap(), "--format", "json"]);
    let sarif_out = run(&["audit", root.to_str().unwrap(), "--format", "sarif"]);

    assert!(text_out.status.success());
    assert!(json_out.status.success());
    assert!(sarif_out.status.success());

    let json_report: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    let json_count = json_report["summary"]["finding_count"].as_u64().unwrap();

    let sarif_report: serde_json::Value = serde_json::from_slice(&sarif_out.stdout).unwrap();
    let sarif_count = sarif_report["runs"][0]["results"].as_array().unwrap().len() as u64;

    assert_eq!(json_count, sarif_count);

    let text_str = String::from_utf8_lossy(&text_out.stdout);
    assert!(text_str.contains(&format!("{json_count} finding(s)")));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn audit_formats_handle_clean_crate_consistently() {
    let root = std::env::temp_dir().join(format!("stk-audit-clean-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/clean.rs"),
        "// clean code with no findings\n",
    )
    .unwrap();

    let text_out = run(&["audit", root.to_str().unwrap(), "--format", "text"]);
    let json_out = run(&["audit", root.to_str().unwrap(), "--format", "json"]);
    let sarif_out = run(&["audit", root.to_str().unwrap(), "--format", "sarif"]);

    assert!(text_out.status.success());
    assert!(json_out.status.success());
    assert!(sarif_out.status.success());

    let text_str = String::from_utf8_lossy(&text_out.stdout);
    assert!(text_str.contains("audit: no findings"));

    let json_report: serde_json::Value = serde_json::from_slice(&json_out.stdout).unwrap();
    assert_eq!(json_report["summary"]["finding_count"], 0);

    let sarif_report: serde_json::Value = serde_json::from_slice(&sarif_out.stdout).unwrap();
    assert_eq!(
        sarif_report["runs"][0]["results"].as_array().unwrap().len(),
        0
    );

    let _ = std::fs::remove_dir_all(&root);
}
