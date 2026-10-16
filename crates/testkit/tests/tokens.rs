//! Regression coverage for `TestToken`'s two amount units and for the
//! Stellar Asset Contract admin authorization the double mocks (and the ones
//! it deliberately does not).
//!
//! Unit terminology (see `soroban_testkit::tokens` module docs):
//! - **whole units** — only `mint`.
//! - **base units** (stroops, `10^7` per whole unit) — `mint_raw`, `balance`,
//!   `assert_balance`, `total_tracked`.

use soroban_sdk::token::TokenClient;
use soroban_testkit::core::TestEnv;

#[test]
fn mint_takes_whole_units_while_balance_reports_base_units() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    token.mint(&alice, 100);

    // `mint` converted whole units to base units at the protocol's 7 decimals.
    assert_eq!(token.balance(&alice), 100 * 10i128.pow(7));
    token.assert_balance(&alice, 1_000_000_000);
}

#[test]
fn mint_and_mint_raw_differ_by_exactly_one_ten_pow_decimals() {
    let env = TestEnv::new();
    let token = env.token();
    let whole = env.address();
    let raw = env.address();

    token.mint(&whole, 1);
    token.mint_raw(&raw, 1);

    assert_eq!(token.balance(&whole), token.balance(&raw) * 10i128.pow(7));
}

#[test]
fn total_tracked_accumulates_in_base_units() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    token.mint(&alice, 1);
    token.mint_raw(&alice, 10);

    assert_eq!(token.total_tracked(), 10i128.pow(7) + 10);
}

#[test]
fn mint_authorizes_the_sac_admin_with_no_global_auth_mocking() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    // No mock_all_auths / mock_auths anywhere: the double mocks the SAC
    // admin's authorization for minting on its own.
    token.mint_raw(&alice, 7);
    token.assert_balance(&alice, 7);
}

#[test]
fn token_operations_outside_mint_still_require_authorization() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();
    let bob = env.address();
    token.mint_raw(&alice, 100);

    // Only the admin's authorization for mint is mocked; an un-authorized
    // transfer must still be rejected by the host.
    env.assert_requires_auth(|| {
        TokenClient::new(env.env(), &token.address()).transfer(
            &alice,
            soroban_sdk::MuxedAddress::from(bob.clone()),
            &10,
        );
    });
}

#[test]
#[should_panic(expected = "token_with_decimals(9)")]
fn token_with_decimals_rejects_a_non_protocol_precision() {
    let env = TestEnv::new();
    let _ = env.token_with_decimals(9);
}
