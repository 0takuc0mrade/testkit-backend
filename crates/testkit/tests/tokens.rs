//! Regression coverage for `TestToken`'s two amount units, the Stellar Asset
//! Contract admin authorization the double mocks (and the ones it deliberately
//! does not), and the checked unit-conversion helpers.
//!
//! Unit terminology (see `soroban_testkit::tokens` module docs):
//! - **whole units** — only `mint`.
//! - **base units** (stroops, `10^7` per whole unit) — `mint_raw`, `balance`,
//!   `assert_balance`, `total_tracked`.

use soroban_sdk::token::TokenClient;
use soroban_testkit::core::TestEnv;
use soroban_testkit::tokens::{base_to_whole_units, whole_to_base_units};

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

#[test]
fn decimal_scaling_overflow_is_prevented_with_actionable_error() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    // 1. Extreme amount with standard 7 decimals overflows i128
    let err = whole_to_base_units(i128::MAX, 7).expect_err("overflow should return Err");
    assert!(err.is_misuse());
    assert!(err.message().contains("overflows i128"));

    // 2. Excessively large decimal precision (e.g. 40) where 10^40 overflows i128
    let err_prec = whole_to_base_units(1, 40).expect_err("large precision should return Err");
    assert!(err_prec.is_misuse());
    assert!(err_prec
        .message()
        .contains("decimal precision 40 is too large"));

    let err_base =
        base_to_whole_units(1_000, 40).expect_err("large precision base should return Err");
    assert!(err_base.is_misuse());
    assert!(err_base
        .message()
        .contains("decimal precision 40 is too large"));

    // 3. TestToken methods use checked conversion
    let err_token = token
        .to_base_units(i128::MAX)
        .expect_err("token to_base_units overflow");
    assert!(err_token.is_misuse());

    // 4. TestToken::mint panics with Misuse instead of integer overflow
    let mint_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        token.mint(&alice, i128::MAX);
    }));
    assert!(mint_res.is_err(), "mint with overflow amount should panic");

    // 5. TestToken::assert_balance_whole panics with Misuse on overflow
    let assert_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        token.assert_balance_whole(&alice, i128::MAX);
    }));
    assert!(
        assert_res.is_err(),
        "assert_balance_whole with overflow amount should panic"
    );
}

#[test]
fn whole_unit_balance_conversion_and_assertions_work() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    token.mint(&alice, 15);
    token.assert_balance_whole(&alice, 15);
    assert_eq!(token.balance_whole(&alice), 15);
    token.assert_balance(&alice, 150_000_000);
}

#[test]
fn tracked_supply_remains_consistent_when_mint_traps() {
    let env = TestEnv::new();
    let token = env.token();
    let alice = env.address();

    assert_eq!(token.total_tracked(), 0);

    // Negative mint via try_mint_raw should fail and keep total_tracked at 0
    let err_raw = token.try_mint_raw(&alice, -100);
    assert!(err_raw.is_err());
    assert_eq!(token.total_tracked(), 0);

    // Negative mint via try_mint should fail and keep total_tracked at 0
    let err_whole = token.try_mint(&alice, -5);
    assert!(err_whole.is_err());
    assert_eq!(token.total_tracked(), 0);

    // Negative mint via mint_raw trapped and caught with catch_unwind
    let trap_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        token.mint_raw(&alice, -500);
    }));
    assert!(trap_res.is_err());
    assert_eq!(token.total_tracked(), 0);

    // Successful mint increases total_tracked
    token.mint(&alice, 10);
    assert_eq!(token.total_tracked(), 100_000_000);

    // Subsequent trapped mint does not change total_tracked
    let _ = token.try_mint_raw(&alice, -1);
    assert_eq!(token.total_tracked(), 100_000_000);

    // Valid subsequent raw mint adds accurately
    token.mint_raw(&alice, 42);
    assert_eq!(token.total_tracked(), 100_000_042);
}
