#!/usr/bin/env bash
#
# Run one CI test command with all network access removed.
#
# Usage:
#   test-no-network.sh all-targets  workspace unit + integration tests (+ doctests)
#   test-no-network.sh doc          doc tests (README examples)
#   test-no-network.sh full         full workspace test run (+ doctests)
#   test-no-network.sh coverage     cargo-llvm-cov coverage run
#
# This enforces the documented no-network test requirement
# (BUILD_SPEC.md §3, CONTRIBUTING.md scope): a test that needs the
# network must fail here, in CI, instead of passing on a connected
# machine. Dependencies are fetched outside the namespace; everything
# after `unshare` has no route to crates.io or anywhere else — the
# loopback device even stays down, because a fresh network namespace
# starts with no interfaces configured.

set -euo pipefail

commands=()
case "${1:-}" in
  all-targets)
    # Run all-targets and also doc tests so broken doctests are not masked
    # behind an all-targets run (cargo test --all-targets excludes doctests).
    commands+=("cargo test --workspace --all-targets --locked")
    commands+=("cargo test --workspace --doc --locked")
    ;;
  doc)
    commands+=("cargo test --workspace --doc --locked")
    ;;
  full)
    commands+=("cargo test --workspace --all-targets --locked")
    commands+=("cargo test --workspace --doc --locked")
    ;;
  coverage)
    commands+=("cargo llvm-cov --workspace --all-targets --lcov --output-path lcov.info")
    ;;
  *)
    echo "usage: $0 all-targets|doc|full|coverage" >&2
    exit 2
    ;;
esac

# Everything the build below needs must already be on disk — the
# namespace we are about to enter cannot reach the network to fetch it.
cargo fetch --locked

run_cmd() {
  local cmd="$1"
  if command -v unshare >/dev/null 2>&1 && [ "$(uname -s)" = "Linux" ] && [ "$(id -u)" -eq 0 -o -n "${CI:-}" ]; then
    sudo env "PATH=${PATH}" "HOME=${HOME}" unshare --net \
      --setuid "$(id -u)" \
      --setgid "$(id -g)" \
      -- $cmd
  else
    $cmd
  fi
}

# Preserve the original cargo exit status and do not mask failure
for cmd in "${commands[@]}"; do
  set +e
  run_cmd "$cmd"
  status=$?
  set -e
  if [ "$status" -ne 0 ]; then
    exit "$status"
  fi
done
