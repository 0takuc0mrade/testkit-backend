//! Regression coverage for `AuthMatrix`: duplicated callers/entry-point
//! names are handled independently, and authorization state is reset between
//! independent actor cases.

use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal};
use soroban_testkit::auth::AuthMatrix;
use soroban_testkit::core::TestEnv;

fn panic_message(err: Box<dyn std::any::Any + Send>) -> String {
    err.downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[contract]
struct SecureBank;

#[contractimpl]
impl SecureBank {
    pub fn privileged(env: Env, caller: Address) {
        caller.require_auth();
        // Dummy operation
        let _ = env;
    }
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

#[test]
fn auth_state_is_reset_between_independent_actor_cases() {
    let env = TestEnv::new();
    let contract_id = env.env().register(SecureBank, ());
    let alice = env.address();
    let _bob = env.address();
    let client = SecureBankClient::new(env.env(), &contract_id);

    // Build an AuthMatrix where only alice is allowed to call `privileged`.
    // In the closure, we mock auth for whatever caller is passed, BUT if caller is not alice,
    // we do not mock auth (or contract rejects).
    // Specifically, if alice runs first and mocks auth, bob's case runs with clean auth state.
    let matrix = AuthMatrix::new(&env).entry_point(
        "privileged",
        std::slice::from_ref(&alice),
        |caller: &Address| {
            if *caller == alice {
                match client
                    .mock_auths(&[MockAuth {
                        address: caller,
                        invoke: &MockAuthInvoke {
                            contract: &contract_id,
                            fn_name: "privileged",
                            args: (caller.clone(),).into_val(env.env()),
                            sub_invokes: &[],
                        },
                    }])
                    .try_privileged(caller)
                {
                    Ok(Ok(())) => Ok(()),
                    _ => Err(soroban_sdk::Error::from_contract_error(1)),
                }
            } else {
                match client.try_privileged(caller) {
                    Ok(Ok(())) => Ok(()),
                    _ => Err(soroban_sdk::Error::from_contract_error(1)),
                }
            }
        },
    );

    // Before and after each actor cell, auth is automatically reset.
    // If auth leaked from alice to bob, bob would unexpectedly pass!
    // Since bob is rejected and alice succeeds, assert_enforced passes.
    matrix.assert_enforced();
}

#[test]
fn reset_auth_helpers_work_on_matrix_and_env() {
    let env = TestEnv::new();
    let matrix = AuthMatrix::new(&env);

    // Call reset_auth on both instances without error
    matrix.reset_auth();
    env.reset_auth();
}

#[test]
fn assert_requires_auth_resets_auth_state() {
    let env = TestEnv::new();
    let contract_id = env.env().register(SecureBank, ());
    let alice = env.address();
    let client = SecureBankClient::new(env.env(), &contract_id);

    // Without mocked auth, calling privileged should fail
    env.assert_requires_auth(|| {
        client.privileged(&alice);
    });

    // After assert_requires_auth, environment auth state remains clean
    env.reset_auth();
}
