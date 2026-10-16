use std::process::Command;

use clap::{Args, ValueEnum};

use super::CliError;

/// Output format for `soroban-testkit coverage`.
#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    /// Per-file coverage stats printed to the terminal.
    Text,
    /// An `lcov.info` file, for IDE integrations (e.g. Coverage Gutters).
    Lcov,
    /// A browsable HTML report.
    Html,
    /// Machine-readable coverage data from cargo-llvm-cov.
    Json,
}

/// Arguments for `soroban-testkit coverage`.
#[derive(Args)]
pub struct CoverageArgs {
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// Fail (non-zero exit) if line coverage is below this percentage.
    #[arg(long, value_name = "PCT")]
    fail_under: Option<f64>,
    /// Open the HTML report after generating it in the default browser (implies --format html).
    ///
    /// Requires platform browser opener support (e.g. macOS `open`, Linux `xdg-open`,
    /// Windows shell). On headless or CI environments without a display server or browser,
    /// omit `--open` and view the generated HTML report directly.
    ///
    /// `--open` is HTML-only; specifying it with `--format lcov` or `--format json`
    /// is rejected.
    #[arg(long)]
    open: bool,
    /// Only collect coverage for this workspace package. Repeatable.
    /// Mutually exclusive with `--exclude`.
    ///
    /// If no source files in the selected package are instrumented, coverage
    /// collection will report that no source files were covered.
    #[arg(long, value_name = "PACKAGE")]
    include: Vec<String>,
    /// Collect coverage for the whole workspace except this package.
    /// Repeatable. Mutually exclusive with `--include`.
    ///
    /// If all packages containing instrumented source files are excluded,
    /// coverage collection will report that no source files were covered.
    #[arg(long, value_name = "PACKAGE")]
    exclude: Vec<String>,
    /// Directory where coverage output files should be written.
    /// Defaults to the current directory.
    #[arg(long, value_name = "DIR")]
    output_dir: Option<std::path::PathBuf>,
    /// Write JSON to this path (defaults to coverage.json).
    #[arg(long, value_name = "PATH")]
    output: Option<std::path::PathBuf>,
}

/// Wraps `cargo llvm-cov test`, which handles Soroban's coverage needs
/// correctly out of the box — soroban_sdk's own test harness runs contract
/// logic natively (not through the WASM VM), so ordinary LLVM
/// source-based coverage instrumentation applies without any
/// Soroban-specific flags. This command's job is knowing *that*, and
/// knowing the right output-format flags, not reimplementing coverage
/// collection.
///
/// Requires `cargo-llvm-cov` to be installed
/// (`cargo install cargo-llvm-cov`).
///
/// # Package filters
///
/// `--include <PACKAGE>` (repeatable) narrows collection to the named
/// workspace package(s), via cargo's own `-p`/`--package`. `--exclude
/// <PACKAGE>` (repeatable) collects for the whole workspace except the
/// named package(s), via `--workspace --exclude`. Specifying both is
/// rejected up front with an actionable error — cargo doesn't have a
/// sensible combined meaning for "these packages, but not these other
/// packages" as a single invocation, and guessing one would be worse
/// than asking the user to run the command twice.
///
/// # Behavior when no source files are instrumented
///
/// When `soroban-testkit coverage` runs in a workspace or package where no source
/// files are instrumented — such as when `--include` specifies a package without
/// tests or source files, `--exclude` filters out all instrumentable code, or the
/// target contracts have no native test targets — the underlying `cargo-llvm-cov`
/// tool may report zero coverage or exit with an error indicating no coverage
/// data was gathered.
///
/// In this scenario:
/// - If `cargo-llvm-cov` exits non-zero, the CLI fails with actionable diagnostic
///   guidance explaining that no instrumented source files were found or covered,
///   prompting the user to check package filters (`--include`/`--exclude`) and
///   verify that test targets exist.
/// - If the coverage run completes but generates an empty or missing output file,
///   the post-run output validator explicitly flags that no code was covered or
///   no tests were executed rather than failing silently.
///
/// # Platform support for `--open`
///
/// `--open` is HTML-only and launches the generated `index.html` report in the
/// system's default browser using platform openers (macOS `open`, Linux `xdg-open`,
/// Windows shell). In headless environments or CI containers without a display server
/// (e.g. no `$DISPLAY` or `$WAYLAND_DISPLAY`), `--open` should be omitted; users
/// should generate the report with `--format html` and inspect the output directory.
///
/// # Output file creation and validation
///
/// Output directories and parent paths are validated and created before the tool
/// executes. Output artifacts are verified after completion — empty or missing
/// files are reported with actionable diagnostic errors.
pub fn run(args: CoverageArgs) -> Result<(), CliError> {
    prepare_output_destinations(&args)?;
    let mut cmd = build_command(&args)?;

    let status = cmd.status().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => CliError(
            "cargo-llvm-cov is not installed or not found in PATH; \
                     install it with: cargo install cargo-llvm-cov"
                .to_string(),
        ),
        _ => CliError(format!(
            "failed to run `cargo llvm-cov` ({err}); is cargo-llvm-cov installed? \
                 try `cargo install cargo-llvm-cov`"
        )),
    })?;

    if !status.success() {
        return Err(CliError(coverage_failure_message(status, args.open)));
    }

    validate_coverage_outputs(&args)?;

    Ok(())
}

pub(crate) fn prepare_output_destinations(args: &CoverageArgs) -> Result<(), CliError> {
    if let Some(dir) = &args.output_dir {
        std::fs::create_dir_all(dir).map_err(|err| {
            CliError(format!(
                "failed to create coverage output directory '{}': {err}; \
                 verify directory permissions and path validity",
                dir.display()
            ))
        })?;
    }

    if let Some(output) = &args.output {
        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    CliError(format!(
                        "failed to create parent directory for coverage output '{}': {err}; \
                         verify directory permissions and path validity",
                        output.display()
                    ))
                })?;
            }
        }
    }

    Ok(())
}

pub(crate) fn validate_coverage_outputs(args: &CoverageArgs) -> Result<(), CliError> {
    match args.format {
        Format::Text => {
            if args.open {
                validate_html_output(args.output_dir.as_deref())?;
            }
            Ok(())
        }
        Format::Lcov => {
            let output_path = if let Some(dir) = &args.output_dir {
                dir.join("lcov.info")
            } else {
                std::path::PathBuf::from("lcov.info")
            };

            if !output_path.exists() {
                return Err(CliError(format!(
                    "coverage output file not found at '{}'; \
                     the coverage tool may have failed to generate the output or no source files were instrumented — \
                     check the coverage run output above for errors",
                    output_path.display()
                )));
            }

            let contents = std::fs::read_to_string(&output_path).map_err(|err| {
                CliError(format!(
                    "failed to read coverage output at '{}': {err}",
                    output_path.display()
                ))
            })?;

            if contents.trim().is_empty() {
                return Err(CliError(format!(
                    "coverage output file at '{}' is empty; \
                     this usually indicates that no tests ran or no source files were instrumented",
                    output_path.display()
                )));
            }

            Ok(())
        }
        Format::Html => validate_html_output(args.output_dir.as_deref()),
        Format::Json => {
            let output_path = args.output.clone().unwrap_or_else(|| {
                args.output_dir
                    .as_ref()
                    .map(|dir| dir.join("coverage.json"))
                    .unwrap_or_else(|| std::path::PathBuf::from("coverage.json"))
            });

            if !output_path.exists() {
                return Err(CliError(format!(
                    "coverage JSON output not found at '{}'; \
                     the coverage tool may have failed to generate the output or no source files were instrumented — \
                     check the coverage run output above for errors",
                    output_path.display()
                )));
            }

            let contents = std::fs::read_to_string(&output_path).map_err(|err| {
                CliError(format!(
                    "failed to read coverage JSON output at '{}': {err}",
                    output_path.display()
                ))
            })?;

            if contents.trim().is_empty() {
                return Err(CliError(format!(
                    "coverage JSON output file at '{}' is empty; \
                     this usually indicates that no tests ran or no source files were instrumented",
                    output_path.display()
                )));
            }

            Ok(())
        }
    }
}

fn validate_html_output(output_dir: Option<&std::path::Path>) -> Result<(), CliError> {
    let dir = output_dir.unwrap_or(std::path::Path::new("."));
    let index_path = dir.join("index.html");

    if !index_path.exists() {
        return Err(CliError(format!(
            "coverage HTML report not found at '{}'; \
             the coverage tool may have failed to generate the report or no source files were instrumented — \
             check the coverage run output above for errors",
            index_path.display()
        )));
    }

    Ok(())
}

fn coverage_failure_message(status: std::process::ExitStatus, open_requested: bool) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = status.signal() {
            return format!("coverage run terminated by signal {signal}");
        }
    }

    if open_requested {
        format!(
            "coverage run failed (cargo llvm-cov exited with {status}); \
             if opening the HTML report in a browser failed, ensure your platform has a supported \
             desktop browser opener (e.g. xdg-open on Linux, open on macOS, start on Windows), or \
             omit `--open` when running in headless/CI environments"
        )
    } else {
        format!(
            "coverage run failed (cargo llvm-cov exited with {status}); \
             if no source files were instrumented, verify that the target package has \
             test targets and that `--include`/`--exclude` filters do not exclude all source files"
        )
    }
}

/// Builds the `cargo llvm-cov` invocation for `args`, without running it —
/// split out from [`run`] so the flag assembly (including the
/// include/exclude package filters) is unit-testable via
/// [`Command::get_args`] instead of only through a full subprocess.
fn build_command(args: &CoverageArgs) -> Result<Command, CliError> {
    if !args.include.is_empty() && !args.exclude.is_empty() {
        return Err(CliError(
            "--include and --exclude cannot be combined; run coverage twice if you need both \
             a narrowed and a widened view"
                .to_string(),
        ));
    }
    if args.output.is_some() && !matches!(args.format, Format::Json) {
        return Err(CliError(
            "--output is only valid with --format json".to_string(),
        ));
    }
    if args.open && !matches!(args.format, Format::Html | Format::Text) {
        return Err(CliError(
            "--open is only valid with --format html".to_string(),
        ));
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("llvm-cov").arg("test");

    for package in &args.include {
        cmd.arg("--package").arg(package);
    }
    if !args.exclude.is_empty() {
        cmd.arg("--workspace");
        for package in &args.exclude {
            cmd.arg("--exclude").arg(package);
        }
    }

    match args.format {
        Format::Text => {
            if args.open {
                cmd.arg("--html");
                if let Some(dir) = &args.output_dir {
                    cmd.arg("--output-dir").arg(dir);
                }
            }
        }
        Format::Lcov => {
            cmd.arg("--lcov");
            let output_path = if let Some(dir) = &args.output_dir {
                dir.join("lcov.info")
            } else {
                std::path::PathBuf::from("lcov.info")
            };
            cmd.arg("--output-path").arg(output_path);
        }
        Format::Html => {
            cmd.arg("--html");
            if let Some(dir) = &args.output_dir {
                cmd.arg("--output-dir").arg(dir);
            }
        }
        Format::Json => {
            cmd.arg("--json");
            let output_path = args.output.clone().unwrap_or_else(|| {
                args.output_dir
                    .as_ref()
                    .map(|dir| dir.join("coverage.json"))
                    .unwrap_or_else(|| std::path::PathBuf::from("coverage.json"))
            });
            cmd.arg("--output-path").arg(output_path);
        }
    }

    if args.open {
        cmd.arg("--open");
    }

    if let Some(pct) = args.fail_under {
        if !pct.is_finite() || !(0.0..=100.0).contains(&pct) {
            return Err(CliError(format!(
                "--fail-under must be between 0 and 100 (got {pct})"
            )));
        }
        cmd.arg("--fail-under-lines").arg(pct.to_string());
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(include: &[&str], exclude: &[&str]) -> CoverageArgs {
        CoverageArgs {
            format: Format::Text,
            fail_under: None,
            open: false,
            include: include.iter().map(|s| s.to_string()).collect(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
            output_dir: None,
            output: None,
        }
    }

    fn rendered_args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect()
    }

    #[test]
    fn no_filters_by_default() {
        let cmd = build_command(&args(&[], &[])).unwrap();
        let rendered = rendered_args(&cmd);
        assert!(!rendered.contains(&"--package".to_string()));
        assert!(!rendered.contains(&"--exclude".to_string()));
        assert!(!rendered.contains(&"--workspace".to_string()));
    }

    #[test]
    fn include_adds_a_package_flag_per_entry() {
        let cmd = build_command(&args(&["core", "money"], &[])).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--package",
                "core",
                "--package",
                "money",
            ]
        );
    }

    #[test]
    fn exclude_adds_workspace_and_an_exclude_flag_per_entry() {
        let cmd = build_command(&args(&[], &["cli"])).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec!["llvm-cov", "test", "--workspace", "--exclude", "cli"]
        );
    }

    #[test]
    fn exclude_with_multiple_packages() {
        let cmd = build_command(&args(&[], &["cli", "audit"])).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--workspace",
                "--exclude",
                "cli",
                "--exclude",
                "audit",
            ]
        );
    }

    #[test]
    fn include_and_exclude_together_is_rejected() {
        let err = build_command(&args(&["core"], &["cli"])).unwrap_err();
        assert!(err.0.contains("cannot be combined"), "{}", err.0);
    }

    #[test]
    fn filters_compose_with_format_and_fail_under() {
        let mut a = args(&["core"], &[]);
        a.format = Format::Lcov;
        a.fail_under = Some(90.0);
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--package",
                "core",
                "--lcov",
                "--output-path",
                "lcov.info",
                "--fail-under-lines",
                "90",
            ]
        );
    }

    #[test]
    fn fail_under_must_be_between_zero_and_one_hundred() {
        for pct in [-0.1, 100.1, f64::NAN, f64::INFINITY] {
            let mut a = args(&[], &[]);
            a.fail_under = Some(pct);
            let err = build_command(&a).unwrap_err();
            assert!(err.0.contains("between 0 and 100"), "{}", err.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn signal_termination_is_reported_distinctly() {
        use std::os::unix::process::ExitStatusExt;

        let message = coverage_failure_message(std::process::ExitStatus::from_raw(9), false);
        assert_eq!(message, "coverage run terminated by signal 9");
    }

    #[test]
    fn output_dir_with_lcov_format() {
        let mut a = args(&[], &[]);
        a.format = Format::Lcov;
        a.output_dir = Some(std::path::PathBuf::from("./coverage"));
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--lcov",
                "--output-path",
                "./coverage/lcov.info",
            ]
        );
    }

    #[test]
    fn output_dir_with_html_format() {
        let mut a = args(&[], &[]);
        a.format = Format::Html;
        a.output_dir = Some(std::path::PathBuf::from("./coverage"));
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec!["llvm-cov", "test", "--html", "--output-dir", "./coverage",]
        );
    }

    #[test]
    fn output_dir_with_spaces_in_path_lcov() {
        let mut a = args(&[], &[]);
        a.format = Format::Lcov;
        a.output_dir = Some(std::path::PathBuf::from("./my coverage dir"));
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--lcov",
                "--output-path",
                "./my coverage dir/lcov.info",
            ]
        );
    }

    #[test]
    fn output_dir_with_spaces_in_path_html() {
        let mut a = args(&[], &[]);
        a.format = Format::Html;
        a.output_dir = Some(std::path::PathBuf::from("./my coverage results"));
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert_eq!(
            rendered,
            vec![
                "llvm-cov",
                "test",
                "--html",
                "--output-dir",
                "./my coverage results",
            ]
        );
    }

    #[test]
    fn json_format_writes_default_file() {
        let mut a = args(&[], &[]);
        a.format = Format::Json;
        assert_eq!(
            rendered_args(&build_command(&a).unwrap()),
            vec![
                "llvm-cov",
                "test",
                "--json",
                "--output-path",
                "coverage.json"
            ]
        );
    }

    #[test]
    fn json_format_respects_output_path_and_directory() {
        let mut a = args(&[], &[]);
        a.format = Format::Json;
        a.output_dir = Some(std::path::PathBuf::from("reports"));
        assert_eq!(
            rendered_args(&build_command(&a).unwrap()),
            vec![
                "llvm-cov",
                "test",
                "--json",
                "--output-path",
                "reports/coverage.json"
            ]
        );
        a.output = Some(std::path::PathBuf::from("result.json"));
        assert_eq!(
            rendered_args(&build_command(&a).unwrap()).last().unwrap(),
            "result.json"
        );
    }

    #[test]
    fn json_output_option_is_rejected_for_other_formats() {
        let mut a = args(&[], &[]);
        a.output = Some(std::path::PathBuf::from("result.json"));
        assert!(build_command(&a)
            .unwrap_err()
            .0
            .contains("only valid with --format json"));
    }

    #[test]
    fn json_cannot_be_opened_as_html() {
        let mut a = args(&[], &[]);
        a.format = Format::Json;
        a.open = true;
        assert!(build_command(&a)
            .unwrap_err()
            .0
            .contains("only valid with --format html"));
    }

    #[test]
    fn lcov_cannot_be_opened_as_html() {
        let mut a = args(&[], &[]);
        a.format = Format::Lcov;
        a.open = true;
        assert!(build_command(&a)
            .unwrap_err()
            .0
            .contains("only valid with --format html"));
    }

    #[test]
    fn open_flag_with_default_format_implies_html() {
        let mut a = args(&[], &[]);
        a.open = true;
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert!(rendered.contains(&"--html".to_string()));
        assert!(rendered.contains(&"--open".to_string()));
    }

    #[test]
    fn open_flag_with_explicit_html_format() {
        let mut a = args(&[], &[]);
        a.format = Format::Html;
        a.open = true;
        let cmd = build_command(&a).unwrap();
        let rendered = rendered_args(&cmd);
        assert!(rendered.contains(&"--html".to_string()));
        assert!(rendered.contains(&"--open".to_string()));
    }

    #[test]
    fn prepare_output_destinations_creates_dirs() {
        let dir = std::env::temp_dir().join(format!("stk-cov-prep-{}", std::process::id()));
        let mut a = args(&[], &[]);
        a.output_dir = Some(dir.join("reports"));
        a.format = Format::Json;
        a.output = Some(dir.join("nested/dir/cov.json"));

        assert!(prepare_output_destinations(&a).is_ok());
        assert!(dir.join("reports").exists());
        assert!(dir.join("nested/dir").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_coverage_outputs_passes_for_text_format() {
        let a = args(&[], &[]);
        assert!(validate_coverage_outputs(&a).is_ok());
    }

    #[test]
    fn validate_coverage_outputs_fails_for_missing_and_empty_lcov() {
        let dir = std::env::temp_dir().join(format!("stk-cov-val-lcov-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut a = args(&[], &[]);
        a.format = Format::Lcov;
        a.output_dir = Some(dir.clone());

        // Missing file
        let err = validate_coverage_outputs(&a).unwrap_err();
        assert!(err.0.contains("not found"), "{}", err.0);
        assert!(
            err.0.contains("no source files were instrumented"),
            "{}",
            err.0
        );

        // Empty file
        std::fs::write(dir.join("lcov.info"), "").unwrap();
        let err = validate_coverage_outputs(&a).unwrap_err();
        assert!(err.0.contains("empty"), "{}", err.0);
        assert!(
            err.0.contains("no source files were instrumented"),
            "{}",
            err.0
        );

        // Valid content
        std::fs::write(dir.join("lcov.info"), "TN:\nSF:src/lib.rs\nend_of_record\n").unwrap();
        assert!(validate_coverage_outputs(&a).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_coverage_outputs_fails_for_missing_html_report() {
        let dir = std::env::temp_dir().join(format!("stk-cov-val-html-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let mut a = args(&[], &[]);
        a.format = Format::Html;
        a.output_dir = Some(dir.clone());

        let err = validate_coverage_outputs(&a).unwrap_err();
        assert!(err.0.contains("HTML report not found"), "{}", err.0);

        std::fs::write(dir.join("index.html"), "<html></html>").unwrap();
        assert!(validate_coverage_outputs(&a).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_coverage_outputs_fails_for_missing_and_empty_json() {
        let dir = std::env::temp_dir().join(format!("stk-cov-val-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("custom.json");

        let mut a = args(&[], &[]);
        a.format = Format::Json;
        a.output = Some(json_path.clone());

        // Missing
        let err = validate_coverage_outputs(&a).unwrap_err();
        assert!(err.0.contains("JSON output not found"), "{}", err.0);

        // Empty
        std::fs::write(&json_path, "   ").unwrap();
        let err = validate_coverage_outputs(&a).unwrap_err();
        assert!(err.0.contains("empty"), "{}", err.0);
        assert!(
            err.0.contains("no source files were instrumented"),
            "{}",
            err.0
        );

        // Valid
        std::fs::write(&json_path, r#"{"data":[]}"#).unwrap();
        assert!(validate_coverage_outputs(&a).is_ok());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn coverage_failure_message_includes_actionable_guidance() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(256); // exit code 1
        let msg_normal = coverage_failure_message(status, false);
        assert!(
            msg_normal.contains("if no source files were instrumented"),
            "{msg_normal}"
        );

        let msg_open = coverage_failure_message(status, true);
        assert!(msg_open.contains("desktop browser opener"), "{msg_open}");
        assert!(msg_open.contains("headless/CI"), "{msg_open}");
    }
}
