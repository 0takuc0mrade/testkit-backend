//! Regression test for the documented no-network test requirement.
//!
//! `BUILD_SPEC.md` §3 puts anything requiring network access at test
//! time out of scope, and `CONTRIBUTING.md` repeats it — but a
//! documented rule nothing enforces is a rule that erodes silently.
//! CI must run every test invocation through
//! `.github/scripts/test-no-network.sh`, which drops the network with
//! `unshare --net` before the suite starts.

mod common;

use common::read_repo_file;

#[test]
fn ci_runs_the_test_suite_with_the_network_removed() {
    let ci = read_repo_file(".github/workflows/ci.yml");
    assert!(
        !ci.contains("cargo test"),
        ".github/workflows/ci.yml must not invoke the test runner directly. \
         Route every test run through `.github/scripts/test-no-network.sh` \
         (`all-targets`, `doc`, `full`, or `coverage`) so the no-network \
         requirement is enforced everywhere the suite runs; a direct \
         `cargo test` step skips the isolation entirely."
    );
    assert!(
        ci.contains(".github/scripts/test-no-network.sh"),
        ".github/workflows/ci.yml never calls `.github/scripts/test-no-network.sh`, \
         so the suite would run with full network access and a network-dependent \
         test would pass CI green."
    );

    let script = read_repo_file(".github/scripts/test-no-network.sh");
    assert!(
        script.contains("set -euo pipefail"),
        "test-no-network.sh must use `set -euo pipefail`: a failed fetch or a \
         failed test must fail the step, not fall through to a green run."
    );
    assert!(
        script.contains("cargo fetch --locked"),
        "test-no-network.sh must fetch locked dependencies *outside* the \
         namespace — after `unshare` there is no network left to download them."
    );
    assert!(
        script.contains("unshare --net"),
        "test-no-network.sh must run the suite inside `unshare --net`. Without \
         the network namespace the no-network requirement in BUILD_SPEC.md §3 \
         is documentation, not enforcement: a test that opens a socket to the \
         internet would still succeed."
    );
    assert!(
        script.contains("cargo test"),
        "test-no-network.sh no longer runs the test suite at all — the \
         isolation wrapper must wrap the actual suite, not an empty command."
    );
}

#[test]
fn test_no_network_script_avoids_masking_doctests_behind_all_targets() {
    let script = read_repo_file(".github/scripts/test-no-network.sh");

    // all-targets must execute --doc as well, because `cargo test --all-targets`
    // excludes doctests by cargo design.
    assert!(
        script.contains("--all-targets --locked") && script.contains("--doc --locked"),
        "test-no-network.sh must include --doc in its all-targets target so failed \
         doctests are not silently masked."
    );
}

#[test]
fn test_no_network_script_preserves_cargo_exit_status() {
    let script = read_repo_file(".github/scripts/test-no-network.sh");

    assert!(
        script.contains("exit \"$status\""),
        "test-no-network.sh must exit with the original status code of the cargo command."
    );
}

