//! Integration tests for `soroban-testkit limits`.
//!
//! Exercises the CLI binary for resource limit discovery, boundary refinement audits,
//! output sanitization, measurement limitations reporting, baseline comparisons,
//! and ramp parameter handling.

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

fn ensure_vault_wasm() -> PathBuf {
    let wasm = workspace_root().join("target/wasm32v1-none/release/vault.wasm");
    if !wasm.exists() {
        let build = Command::new("cargo")
            .args([
                "build",
                "-p",
                "vault",
                "--target",
                "wasm32v1-none",
                "--release",
            ])
            .current_dir(workspace_root())
            .status()
            .expect("failed to build vault.wasm");
        assert!(
            build.success(),
            "failed to compile examples/vault for documented wasm target `wasm32v1-none`. \
             Ensure the target is installed: `rustup target add wasm32v1-none`."
        );
    }
    wasm
}

#[test]
fn limits_help_documents_options_and_limitations() {
    let output = run(&["limits", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("--contract"), "{stdout}");
    assert!(stdout.contains("--fn"), "{stdout}");
    assert!(stdout.contains("--ramp"), "{stdout}");
    assert!(stdout.contains("--arg"), "{stdout}");
    assert!(stdout.contains("--probe-timeout"), "{stdout}");
    assert!(stdout.contains("--baseline"), "{stdout}");
    assert!(stdout.contains("--baseline-tolerance-pct"), "{stdout}");
    assert!(stdout.contains("--save-baseline"), "{stdout}");
    assert!(stdout.contains("--export"), "{stdout}");
}

#[test]
fn limits_rejects_invalid_baseline_tolerance() {
    let wasm = ensure_vault_wasm();
    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--baseline-tolerance-pct",
        "150",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--baseline-tolerance-pct must be between 0 and 100"),
        "{stderr}"
    );
}

#[test]
fn limits_rejects_arg_overriding_ramp_parameter() {
    let wasm = ensure_vault_wasm();
    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--arg",
        "recipients=10",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--arg cannot override ramp parameter \"recipients\""),
        "{stderr}"
    );
}

#[test]
fn limits_rejects_unknown_function_with_available_list() {
    let wasm = ensure_vault_wasm();
    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "nonexistent_function",
        "--ramp",
        "recipients",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no function named \"nonexistent_function\""),
        "{stderr}"
    );
    assert!(stderr.contains("available functions:"), "{stderr}");
    assert!(stderr.contains("batch_payout"), "{stderr}");
}

#[test]
fn limits_reports_refinement_audit_and_limitations_in_stdout() {
    let wasm = ensure_vault_wasm();
    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("last successful recipients"), "{stdout}");
    assert!(stdout.contains("instructions:"), "{stdout}");
    assert!(stdout.contains("memory bytes:"), "{stdout}");
    assert!(
        stdout.contains("boundary refinement: probed"),
        "stdout missing refinement audit: {stdout}"
    );
    assert!(
        stdout.contains(
            "ledger reads/writes and transaction size are not measured by this command; see --help"
        ),
        "stdout missing measurement limitations: {stdout}"
    );
    // #492: failures are reported with a standardized category.
    assert!(
        stdout.contains("first failed recipients value:"),
        "stdout missing first failure: {stdout}"
    );
    assert!(
        stdout.contains("probe failures:"),
        "stdout missing probe failure categories: {stdout}"
    );
}

#[test]
fn limits_text_summary_and_json_export_agree_on_the_boundary() {
    // #490: the terminal summary and the machine-readable report are both
    // rendered from one summary, so the discovered boundary and the
    // standardized failure category must match exactly.
    let wasm = ensure_vault_wasm();
    let temp_dir = std::env::temp_dir().join(format!("stk-limits-agree-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let export_path = temp_dir.join("agreement.json");

    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--export",
        export_path.to_str().unwrap(),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");

    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&export_path).unwrap()).unwrap();
    let best = json["best"].as_u64().unwrap();

    assert!(
        stdout.contains(&format!(
            "last successful recipients for \"batch_payout\": {best}"
        )),
        "stdout boundary disagrees with JSON: {stdout}"
    );
    assert_eq!(json["audit"]["final_success"], best);

    // The standardized failure category must appear identically in both.
    let category = match json["first_failure_category"].as_str() {
        Some(category) => category,
        None => panic!("JSON is missing first_failure_category: {json}"),
    };
    assert!(
        stdout.contains(&format!("[{category}]")),
        "stdout is missing standardized category {category}: {stdout}"
    );
    assert!(
        json["probe_failure_counts"][category].as_u64().unwrap() >= 1,
        "JSON failure counts are missing {category}: {json}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn limits_export_json_includes_limitations_audit_and_sanitizes_temp_paths() {
    let wasm = ensure_vault_wasm();
    let temp_dir =
        std::env::temp_dir().join(format!("stk-limits-test-json-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Copy wasm into temp directory to verify temp path sanitization
    let temp_wasm = temp_dir.join("vault.wasm");
    std::fs::copy(&wasm, &temp_wasm).unwrap();

    let export_path = temp_dir.join("results.json");
    let output = run(&[
        "limits",
        "--contract",
        temp_wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--export",
        export_path.to_str().unwrap(),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");

    let export_content = std::fs::read_to_string(&export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&export_content).unwrap();

    assert_eq!(json["function"], "batch_payout");
    assert_eq!(json["ramp"], "recipients");
    assert!(json["best"].as_u64().unwrap() > 0);
    assert!(json["instructions"].as_u64().unwrap() > 0);
    assert!(json["memory_bytes"].as_u64().unwrap() > 0);

    // Verify measurement limitations
    assert_eq!(
        json["measurement_limitations"],
        "ledger reads/writes and transaction size are not measured by this command; see --help"
    );

    // Verify temp directory path sanitization: must not leak temp_dir prefix
    assert_eq!(json["contract"], "vault.wasm");
    assert!(!export_content.contains("stk-limits-test-json"));

    // Verify audit structure is present
    assert!(json["audit"].is_object());
    assert!(json["audit"]["steps"].as_array().unwrap().len() > 0);
    assert_eq!(json["audit"]["final_success"], json["best"]);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn limits_save_baseline_includes_limitations_and_sanitizes_temp_paths() {
    let wasm = ensure_vault_wasm();
    let temp_dir =
        std::env::temp_dir().join(format!("stk-limits-test-base-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let temp_wasm = temp_dir.join("vault.wasm");
    std::fs::copy(&wasm, &temp_wasm).unwrap();

    let baseline_path = temp_dir.join("vault.baseline");
    let output = run(&[
        "limits",
        "--contract",
        temp_wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--save-baseline",
        baseline_path.to_str().unwrap(),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");

    let baseline_content = std::fs::read_to_string(&baseline_path).unwrap();
    assert!(baseline_content.contains("function=batch_payout"));
    assert!(baseline_content.contains("ramp=recipients"));
    assert!(baseline_content.contains("contract=vault.wasm"));
    assert!(!baseline_content.contains("stk-limits-test-base"));
    assert!(baseline_content.contains("measurement_limitations=ledger reads/writes and transaction size are not measured by this command; see --help"));

    // Verify subsequent --baseline comparison passes
    let cmp_output = run(&[
        "limits",
        "--contract",
        temp_wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--baseline",
        baseline_path.to_str().unwrap(),
    ]);
    let cmp_stdout = String::from_utf8_lossy(&cmp_output.stdout);
    assert!(cmp_output.status.success(), "stdout: {cmp_stdout}");
    assert!(cmp_stdout.contains("no regression"), "{cmp_stdout}");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn limits_explains_how_to_reproduce_reported_maximum_locally() {
    let wasm = ensure_vault_wasm();
    let temp_dir =
        std::env::temp_dir().join(format!("stk-limits-repro-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let export_path = temp_dir.join("repro.json");

    let output = run(&[
        "limits",
        "--contract",
        wasm.to_str().unwrap(),
        "--fn",
        "batch_payout",
        "--ramp",
        "recipients",
        "--export",
        export_path.to_str().unwrap(),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("reproduce locally:"),
        "stdout missing reproduction guidance: {stdout}"
    );
    assert!(
        stdout.contains("batch_payout"),
        "reproduction guidance missing function name: {stdout}"
    );
    assert!(
        stdout.contains("recipients"),
        "reproduction guidance missing ramp parameter: {stdout}"
    );

    let export_content = std::fs::read_to_string(&export_path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&export_content).unwrap();
    assert!(
        json["reproduction"].is_string(),
        "JSON export missing reproduction guidance: {json}"
    );
    let repro_str = json["reproduction"].as_str().unwrap();
    assert!(
        repro_str.contains("batch_payout"),
        "repro string: {repro_str}"
    );
    assert!(
        repro_str.contains("recipients"),
        "repro string: {repro_str}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
