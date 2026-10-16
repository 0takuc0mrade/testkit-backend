//! Integration tests for `soroban-testkit coverage`.
//!
//! Exercises the CLI binary for coverage collection options, platform open behavior,
//! output file creation, package filtering, and validation diagnostics.

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

#[test]
fn coverage_help_documents_options_and_platform_notes() {
    let output = run(&["coverage", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("--format"), "{stdout}");
    assert!(stdout.contains("--fail-under"), "{stdout}");
    assert!(stdout.contains("--open"), "{stdout}");
    assert!(stdout.contains("--include"), "{stdout}");
    assert!(stdout.contains("--exclude"), "{stdout}");
    assert!(stdout.contains("--output-dir"), "{stdout}");
    assert!(stdout.contains("--output"), "{stdout}");
}

#[test]
fn combining_include_and_exclude_is_rejected_with_actionable_error() {
    let output = run(&["coverage", "--include", "core", "--exclude", "cli"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--include and --exclude cannot be combined"),
        "{stderr}"
    );
}

#[test]
fn output_option_rejected_for_non_json_formats() {
    let output = run(&["coverage", "--format", "text", "--output", "coverage.json"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--output is only valid with --format json"),
        "{stderr}"
    );

    let output_lcov = run(&["coverage", "--format", "lcov", "--output", "coverage.json"]);
    assert!(!output_lcov.status.success());
    let stderr_lcov = String::from_utf8_lossy(&output_lcov.stderr);
    assert!(
        stderr_lcov.contains("--output is only valid with --format json"),
        "{stderr_lcov}"
    );
}

#[test]
fn open_option_rejected_for_incompatible_formats() {
    // Rejected for JSON
    let output_json = run(&["coverage", "--format", "json", "--open"]);
    assert!(!output_json.status.success());
    let stderr_json = String::from_utf8_lossy(&output_json.stderr);
    assert!(
        stderr_json.contains("--open is only valid with --format html"),
        "{stderr_json}"
    );

    // Rejected for LCOV
    let output_lcov = run(&["coverage", "--format", "lcov", "--open"]);
    assert!(!output_lcov.status.success());
    let stderr_lcov = String::from_utf8_lossy(&output_lcov.stderr);
    assert!(
        stderr_lcov.contains("--open is only valid with --format html"),
        "{stderr_lcov}"
    );
}

#[test]
fn fail_under_percentage_bounds_are_enforced() {
    let output_negative = run(&["coverage", "--fail-under=-5"]);
    assert!(!output_negative.status.success());
    let stderr_neg = String::from_utf8_lossy(&output_negative.stderr);
    assert!(
        stderr_neg.contains("must be between 0 and 100"),
        "{stderr_neg}"
    );

    let output_over = run(&["coverage", "--fail-under=105"]);
    assert!(!output_over.status.success());
    let stderr_over = String::from_utf8_lossy(&output_over.stderr);
    assert!(
        stderr_over.contains("must be between 0 and 100"),
        "{stderr_over}"
    );
}

#[test]
fn format_names_are_consistent_in_errors_and_help() {
    let output = run(&["coverage", "--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("text"), "{stdout}");
    assert!(stdout.contains("lcov"), "{stdout}");
    assert!(stdout.contains("html"), "{stdout}");
    assert!(stdout.contains("json"), "{stdout}");

    // Format names in error messages:
    let output_text_out = run(&["coverage", "--format", "text", "--output", "coverage.json"]);
    assert!(!output_text_out.status.success());
    let stderr_text = String::from_utf8_lossy(&output_text_out.stderr);
    assert!(
        stderr_text.contains("--output is only valid with --format json (got --format text)"),
        "{stderr_text}"
    );

    let output_lcov_open = run(&["coverage", "--format", "lcov", "--open"]);
    assert!(!output_lcov_open.status.success());
    let stderr_lcov = String::from_utf8_lossy(&output_lcov_open.stderr);
    assert!(
        stderr_lcov.contains("--open is only valid with --format html (got --format lcov)"),
        "{stderr_lcov}"
    );
}
