//! Regression coverage for `AuthMatrix` when callers and entry points are
//! duplicated.
//!
//! A repeated address in an `allowed` list must still be counted and checked
//! once per entry point, and reusing an entry-point name must produce two
//! independently-checked sections rather than silently merging.

use soroban_testkit::auth::AuthMatrix;
use soroban_testkit::core::TestEnv;

fn panic_message(err: Box<dyn std::any::Any + Send>) -> String {
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn a_duplicated_allowed_address_is_checked_once_not_twice() {
    let env = TestEnv::new();
    let alice = env.address();

    let matrix = AuthMatrix::new(&env).entry_point("dup", &[alice.clone(), alice.clone()], |_| {
        Err(soroban_sdk::Error::from_contract_error(1))
    });

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| matrix.assert_enforced()))
        .expect_err("alice is expected to pass but is rejected");
    let message = panic_message(err);

    assert!(
        message.contains("of 1 cells"),
        "a duplicated actor must be counted once: {message}"
    );
}

#[test]
fn duplicates_are_harmless_when_every_cell_is_correct() {
    let env = TestEnv::new();
    let alice = env.address();
    let bob = env.address();

    // `alice` is listed twice on each entry point; every cell is allowed and
    // every closure agrees, so duplicates must not produce a spurious failure.
    AuthMatrix::new(&env)
        .entry_point(
            "deposit",
            &[alice.clone(), alice.clone(), bob.clone()],
            |_| Ok(()),
        )
        .entry_point(
            "withdraw",
            &[bob.clone(), alice.clone(), alice.clone()],
            |_| Ok(()),
        )
        .assert_enforced();
}

#[test]
fn duplicate_entry_point_names_produce_independent_sections() {
    let env = TestEnv::new();
    let alice = env.address();

    let matrix = AuthMatrix::new(&env)
        .entry_point("deposit", std::slice::from_ref(&alice), |_| {
            Err(soroban_sdk::Error::from_contract_error(1))
        })
        .entry_point("deposit", std::slice::from_ref(&alice), |_| Ok(()));

    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| matrix.assert_enforced()))
        .expect_err("the first `deposit` rejects alice and must be flagged");
    let message = panic_message(err);

    assert_eq!(
        message.matches("[deposit]").count(),
        2,
        "two entry points sharing a name must both be reported: {message}"
    );
    assert!(
        message.contains("of 2 cells"),
        "each same-named entry point still gets its own cells: {message}"
    );
}

#[test]
fn assert_requires_auth_still_catches_an_unmocked_call() {
    let env = TestEnv::new();
    env.assert_requires_auth(|| panic!("simulated auth failure"));
}
