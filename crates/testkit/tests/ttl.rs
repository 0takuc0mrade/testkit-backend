//! Regression coverage for the per-storage-kind TTL rules documented in
//! `soroban_testkit::ttl` (temporary entries are deleted on expiry and cannot
//! be recovered; persistent and instance entries are archived and restored;
//! instance storage has a single entry per contract; `expire` ignores the
//! kind and advances past `max_entry_ttl`), plus what `ttl_of` reports for
//! missing keys and unsupported targets, and what `expire`/`simulate_expiry`
//! preserve.

use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};
use soroban_testkit::core::TestEnv;
use soroban_testkit::ttl::StorageKind;

#[contract]
struct Store;

#[contractimpl]
impl Store {
    pub fn set_persistent(env: Env, key: Symbol, value: i128) {
        env.storage().persistent().set(&key, &value);
    }

    pub fn read_persistent(env: Env, key: Symbol) -> Option<i128> {
        env.storage().persistent().get(&key)
    }

    pub fn set_temporary(env: Env, key: Symbol, value: i128) {
        env.storage().temporary().set(&key, &value);
    }

    pub fn read_temporary(env: Env, key: Symbol) -> Option<i128> {
        env.storage().temporary().get(&key)
    }

    pub fn set_instance(env: Env, value: i128) {
        env.storage().instance().set(&symbol_short!("v"), &value);
    }
}

#[contract]
struct SampleStorageContract;

#[contractimpl]
impl SampleStorageContract {
    pub fn write_persistent(env: Env, key: Symbol, value: i128) {
        env.storage().persistent().set(&key, &value);
    }

    pub fn write_temporary(env: Env, key: Symbol, value: i128) {
        env.storage().temporary().set(&key, &value);
    }
}

#[test]
fn temporary_and_persistent_entries_get_their_own_minimum_ttl() {
    let env = TestEnv::new();
    let id = env.env().register(Store, ());
    let client = StoreClient::new(env.env(), &id);
    client.set_persistent(&symbol_short!("p"), &1);
    client.set_temporary(&symbol_short!("t"), &1);

    let info = env.env().ledger().get();
    let persistent = env.ttl_of(&id, StorageKind::Persistent, symbol_short!("p"));
    let temporary = env.ttl_of(&id, StorageKind::Temporary, symbol_short!("t"));

    // Each kind is granted its own minimum on write; the reported remaining
    // TTL is one ledger lower because the writing call consumed a ledger.
    assert_eq!(
        persistent + 1,
        info.min_persistent_entry_ttl.min(info.max_entry_ttl),
        "persistent writes are granted min_persistent_entry_ttl"
    );
    assert_eq!(
        temporary + 1,
        info.min_temp_entry_ttl.min(info.max_entry_ttl),
        "temporary writes are granted min_temp_entry_ttl"
    );
}

#[test]
fn a_temporary_entry_is_absent_after_expiry() {
    let env = TestEnv::new();
    let id = env.env().register(Store, ());
    let client = StoreClient::new(env.env(), &id);
    client.set_temporary(&symbol_short!("t"), &2);

    // Temporary storage is deleted, not archived: after expiry the contract
    // must observe the entry as absent.
    env.assert_temporary_expires(
        &id,
        symbol_short!("t"),
        || client.read_temporary(&symbol_short!("t")),
        None,
    );
}

#[test]
fn a_persistent_entry_recovers_after_expiry() {
    let env = TestEnv::new();
    let id = env.env().register(Store, ());
    let client = StoreClient::new(env.env(), &id);
    client.set_persistent(&symbol_short!("p"), &7);

    // Persistent storage is archived, not deleted: reading it after expiry
    // restores it, so the contract still sees the value.
    env.assert_recovers_after_expiry(
        &id,
        symbol_short!("p"),
        || client.read_persistent(&symbol_short!("p")).unwrap_or(0),
        7,
    );
}

#[test]
fn ttl_of_ignores_the_key_for_instance_storage() {
    let env = TestEnv::new();
    let id = env.env().register(Store, ());
    let client = StoreClient::new(env.env(), &id);
    client.set_instance(&5);

    // There is one instance entry per contract, so every key resolves to the
    // same live TTL.
    let with_symbol = env.ttl_of(&id, StorageKind::Instance, symbol_short!("a"));
    let with_other_symbol = env.ttl_of(&id, StorageKind::Instance, symbol_short!("b"));
    let with_number = env.ttl_of(&id, StorageKind::Instance, 12_345u32);

    assert!(with_symbol > 0, "the instance entry should be live");
    assert_eq!(with_symbol, with_other_symbol);
    assert_eq!(with_symbol, with_number);
}

#[test]
fn expire_ignores_the_kind_and_advances_past_max_entry_ttl() {
    let env = TestEnv::new();
    let max_entry_ttl = env.env().ledger().get().max_entry_ttl;
    let start = env.env().ledger().sequence();

    env.expire(StorageKind::Temporary);

    // The `kind` argument documents intent only; the advance is the same for
    // every kind and guaranteed to clear the longest possible TTL.
    assert_eq!(env.env().ledger().sequence() - start, max_entry_ttl + 1);
}

#[test]
fn distinguish_missing_key_from_unsupported_storage_inspection() {
    let env = TestEnv::new();
    let contract_id = env.env().register(SampleStorageContract, ());
    let client = SampleStorageContractClient::new(env.env(), &contract_id);
    let present_key = symbol_short!("present");
    let missing_key = symbol_short!("missing");

    client.write_persistent(&present_key, &100);
    client.write_temporary(&present_key, &200);

    // 1. Present keys report positive TTL
    assert!(env.ttl_of(&contract_id, StorageKind::Persistent, &present_key) > 0);
    assert!(env.ttl_of(&contract_id, StorageKind::Temporary, &present_key) > 0);

    // 2. Missing key on a valid contract returns AssertionFailed distinguishing missing key
    let err_missing = env
        .try_ttl_of(&contract_id, StorageKind::Persistent, &missing_key)
        .expect_err("missing key must return error");
    assert!(
        err_missing.is_assertion_failed(),
        "missing key should be an AssertionFailed, got: {err_missing}"
    );
    assert!(
        err_missing.message().contains("missing key"),
        "error message should indicate missing key: {err_missing}"
    );

    // Panicking variant for missing key
    let panic_missing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.ttl_of(&contract_id, StorageKind::Persistent, &missing_key);
    }));
    assert!(panic_missing.is_err());

    // 3. Unsupported storage inspection (e.g. unregistered address) returns Misuse
    let unregistered: Address = env.address();
    let err_unsupported = env
        .try_ttl_of(&unregistered, StorageKind::Persistent, &present_key)
        .expect_err("unsupported target must return error");
    assert!(
        err_unsupported.is_misuse(),
        "unsupported storage inspection should be Misuse, got: {err_unsupported}"
    );
    assert!(
        err_unsupported
            .message()
            .contains("unsupported storage inspection"),
        "error message should indicate unsupported storage inspection: {err_unsupported}"
    );

    // Panicking variant for unsupported storage inspection
    let panic_unsupported = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        env.ttl_of(&unregistered, StorageKind::Persistent, &present_key);
    }));
    assert!(panic_unsupported.is_err());
}

#[test]
fn expiry_simulation_preserves_unrelated_ledger_settings() {
    let env = TestEnv::new();

    // Configure distinct ledger settings
    let test_protocol = env.env().ledger().get().protocol_version;
    env.env().ledger().set_protocol_version(test_protocol);
    env.env().ledger().set_base_reserve(500);
    env.env().ledger().set_min_persistent_entry_ttl(1_234);
    env.env().ledger().set_max_entry_ttl(456_790);

    let initial = env.env().ledger().get();
    assert_eq!(initial.protocol_version, test_protocol);
    assert_eq!(initial.base_reserve, 500);
    assert_eq!(initial.min_persistent_entry_ttl, 1_234);

    // Call expire() which advances sequence and timestamp
    env.expire(StorageKind::Persistent);

    let after_expire = env.env().ledger().get();
    assert_eq!(
        after_expire.protocol_version, test_protocol,
        "expire() must preserve protocol_version"
    );
    assert_eq!(
        after_expire.base_reserve, 500,
        "expire() must preserve base_reserve"
    );
    assert_eq!(
        after_expire.min_persistent_entry_ttl, 1_234,
        "expire() must preserve min_persistent_entry_ttl"
    );
    assert_eq!(
        after_expire.max_entry_ttl, initial.max_entry_ttl,
        "expire() must preserve max_entry_ttl"
    );

    // Call simulate_expiry() which restores clock and parameters
    let seq_before = env.sequence();
    let time_before = env.now();

    env.simulate_expiry(StorageKind::Persistent, || {
        assert!(env.sequence() > seq_before);
    });

    assert_eq!(env.sequence(), seq_before);
    assert_eq!(env.now(), time_before);

    let final_ledger = env.env().ledger().get();
    assert_eq!(final_ledger.protocol_version, test_protocol);
    assert_eq!(final_ledger.base_reserve, 500);
}
