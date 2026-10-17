//! Focused coverage for the signed-amount semantics documented in
//! `soroban_testkit::money`.
//!
//! Every amount generator models a signed `i128`: negative values are
//! meaningful (debits, refunds, subtraction results), so the generators
//! include the signed extremes on purpose. Basis-point fractions are the one
//! unsigned case. These tests pin that contract at the public API boundary.

use soroban_testkit::core::TestEnv;
use soroban_testkit::money::{adversarial_amounts, amounts_in, bps_values, Conservation};

#[test]
fn adversarial_amounts_covers_the_signed_extremes() {
    let amounts = adversarial_amounts();
    for boundary in [-1, i128::MIN, i128::MIN + 1] {
        assert!(
            amounts.contains(&boundary),
            "adversarial_amounts() must include the signed boundary {boundary}"
        );
    }
}

#[test]
fn amounts_in_draws_negative_amounts_from_a_signed_range() {
    let env = TestEnv::with_seed(42);
    let debits = amounts_in(&env, -500, -1, 32);
    assert_eq!(debits.len(), 32);
    assert!(debits.iter().all(|a| (-500..=-1).contains(a)));
    assert!(debits.iter().any(|&a| a < 0));
}

#[test]
fn amounts_in_is_reproducible_for_the_same_seed() {
    let a = amounts_in(&TestEnv::with_seed(1), -1_000, 1_000, 16);
    let b = amounts_in(&TestEnv::with_seed(1), -1_000, 1_000, 16);
    assert_eq!(a, b);
}

#[test]
#[should_panic(expected = "min (10) must be <= max (0)")]
fn amounts_in_rejects_an_inverted_range() {
    let _ = amounts_in(&TestEnv::new(), 10, 0, 1);
}

#[test]
fn bps_values_is_unsigned_but_still_adversarial() {
    let values = bps_values();
    assert!(values.contains(&10_000));
    assert!(
        values.iter().any(|&v| v > 10_000),
        "unsigned fractions are pushed out of range rather than negative"
    );
}

#[test]
fn conservation_accounts_for_signed_refunds() {
    // A refund is a negative flow; conservation still holds.
    Conservation {
        deposited: 100,
        withdrawn: 130,
        refunded: -30,
        remaining: 0,
    }
    .assert_holds();
}
