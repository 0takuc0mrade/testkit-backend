use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, Symbol};
use soroban_testkit::core::TestEnv;
use soroban_testkit::ttl::StorageKind;

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
fn distinguish_missing_key_from_unsupported_storage_inspection() {
    let env = TestEnv::new();
    let contract_id = env.env().register(SampleStorageContract, ());
    let client = SampleStorageContractClient::new(env.env(), &contract_id);
    let present_key = symbol_short!("present");
    let missing_key = symbol_short!("missing");

    client.write_persistent(&present_key, &100);
    client.write_temporary(&present_key, &200);

    // 1. Present keys report positive TTL
    assert!(env.ttl_of(&contract_id, StorageKind::Persistent, present_key) > 0);
    assert!(env.ttl_of(&contract_id, StorageKind::Temporary, present_key) > 0);

    // 2. Missing key on a valid contract returns AssertionFailed distinguishing missing key
    let err_missing = env
        .try_ttl_of(&contract_id, StorageKind::Persistent, missing_key)
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
        env.ttl_of(&contract_id, StorageKind::Persistent, missing_key);
    }));
    assert!(panic_missing.is_err());

    // 3. Unsupported storage inspection (e.g. unregistered address) returns Misuse
    let unregistered: Address = env.address();
    let err_unsupported = env
        .try_ttl_of(&unregistered, StorageKind::Persistent, present_key)
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
        env.ttl_of(&unregistered, StorageKind::Persistent, present_key);
    }));
    assert!(panic_unsupported.is_err());
}

#[test]
fn expiry_simulation_preserves_unrelated_ledger_settings() {
    let env = TestEnv::new();

    // Configure distinct ledger settings
    env.env().ledger().set_protocol_version(21);
    env.env().ledger().set_base_reserve(500);
    env.env().ledger().set_min_persistent_entry_ttl(1_234);
    env.env().ledger().set_max_entry_ttl(456_789);

    let initial = env.env().ledger().get();
    assert_eq!(initial.protocol_version, 21);
    assert_eq!(initial.base_reserve, 500);
    assert_eq!(initial.min_persistent_entry_ttl, 1_234);
    assert_eq!(initial.max_entry_ttl, 456_789);

    // Call expire() which advances sequence and timestamp
    env.expire(StorageKind::Persistent);

    let after_expire = env.env().ledger().get();
    assert_eq!(
        after_expire.protocol_version, 21,
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
        after_expire.max_entry_ttl, 456_789,
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
    assert_eq!(final_ledger.protocol_version, 21);
    assert_eq!(final_ledger.base_reserve, 500);
}
