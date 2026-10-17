//! Integration coverage for the scoped ledger clock (`TestEnv::at`).
//!
//! `at` runs a closure at a chosen ledger time and then restores the
//! environment. These tests pin the boundary that a scoped evaluation must
//! not be built from — or leave behind — ledger state newer than the moment
//! it claims to represent: inside the scope the sequence number belongs to
//! the scoped timestamp, and the newer pre-scope clock comes back intact on
//! exit.

use std::panic::{self, AssertUnwindSafe};
use std::time::Duration;

use soroban_sdk::testutils::Ledger as _;
use soroban_testkit::core::{LedgerDefaults, TestEnv};
use soroban_testkit::ledger::LEDGER_CLOSE_TIME_SECS;

fn clock(env: &TestEnv) -> (u64, u32) {
    (env.now(), env.sequence())
}

#[test]
fn scoped_at_reports_the_sequence_that_belongs_to_its_timestamp() {
    let env = TestEnv::new();
    env.advance(Duration::from_secs(100)); // (100, 20)

    let inside = env.at(40, || clock(&env));

    assert_eq!(
        inside,
        (40, 8),
        "a scope at timestamp 40 must not report the newer pre-scope sequence"
    );
    assert_eq!(clock(&env), (100, 20));
}

#[test]
fn advance_inside_a_scoped_at_moves_from_the_scoped_position() {
    let env = TestEnv::new();
    env.advance(Duration::from_secs(100)); // (100, 20)

    let inside = env.at(40, || {
        env.advance(Duration::from_secs(10));
        clock(&env)
    });

    assert_eq!(inside, (50, 10));
    assert_eq!(
        clock(&env),
        (100, 20),
        "the newer pre-scope clock must come back intact"
    );
}

#[test]
fn scoped_at_restores_the_newer_pre_scope_state_when_the_closure_panics() {
    let env = TestEnv::new();
    env.advance(Duration::from_secs(100));
    let before = clock(&env);

    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        env.at(0, || {
            assert_eq!(clock(&env), (0, 0));
            panic!("boom");
        });
    }));

    assert!(result.is_err());
    assert_eq!(clock(&env), before);
}

#[test]
fn scoped_at_still_rolls_back_ledger_fields_changed_inside() {
    let env = TestEnv::new();
    let before = env.env().ledger().get();

    env.at(env.now() + 25, || {
        env.env().ledger().set(soroban_sdk::testutils::LedgerInfo {
            base_reserve: before.base_reserve + 1,
            ..env.env().ledger().get()
        });
    });

    let after = env.env().ledger().get();
    assert_eq!(after.base_reserve, before.base_reserve);
    assert_eq!(after.protocol_version, before.protocol_version);
    assert_eq!(after.network_id, before.network_id);
}

#[test]
fn scoped_at_preserves_a_seeded_timestamp_sequence_offset() {
    let env = TestEnv::with_ledger_defaults(
        LedgerDefaults::new()
            .timestamp(1_700_000_000)
            .sequence_number(1_000),
    );

    let inside = env.at(1_700_000_000 - 100, || clock(&env));

    assert_eq!(inside, (1_700_000_000 - 100, 980));
    assert_eq!(clock(&env), (1_700_000_000, 1_000));
}

#[test]
fn scoped_at_clamps_the_sequence_at_the_ends_of_its_range() {
    let past = TestEnv::with_ledger_defaults(
        LedgerDefaults::new()
            .timestamp(1_000_000)
            .sequence_number(1),
    );
    assert_eq!(past.at(0, || past.sequence()), 0);
    assert_eq!(clock(&past), (1_000_000, 1));

    let future = TestEnv::new();
    assert_eq!(future.at(u64::MAX, || future.sequence()), u32::MAX);
    assert_eq!(clock(&future), (0, 0));
}

#[test]
fn scoped_at_uses_the_environments_close_interval() {
    let env = TestEnv::new().with_ledger_close_interval(2);
    env.advance(Duration::from_secs(20)); // (20, 10)

    // Two seconds per ledger: timestamp 6 is three ledgers.
    assert_eq!(env.at(6, || env.sequence()), 3);

    let default = TestEnv::new();
    assert_eq!(
        default.ledger_close_interval(),
        LEDGER_CLOSE_TIME_SECS,
        "the default interval stays documented"
    );
}
