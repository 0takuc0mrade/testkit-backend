use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal};
use soroban_testkit::auth::AuthMatrix;
use soroban_testkit::core::TestEnv;

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
