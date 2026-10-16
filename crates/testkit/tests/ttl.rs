//! Regression coverage for the per-storage-kind TTL rules documented in
//! `soroban_testkit::ttl` (temporary entries are deleted on expiry and cannot
//! be recovered; persistent and instance entries are archived and restored;
//! instance storage has a single entry per contract; `expire` ignores the
//! kind and advances past `max_entry_ttl`).

use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{contract, contractimpl, symbol_short, Env, Symbol};
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
