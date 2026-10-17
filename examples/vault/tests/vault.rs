#![cfg(feature = "testutils")]

use soroban_sdk::{testutils::Address as _, Address, Env};
use vault::{Vault, VaultClient, VaultError};

#[test]
fn vault_initialization_and_deposit() {
    let env = Env::default();
    let contract_id = env.register(Vault, ());
    let client = VaultClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);

    assert_eq!(client.owner(), owner);
    assert_eq!(client.balance(), 0);

    client.deposit(&1_000);
    assert_eq!(client.balance(), 1_000);
}

#[test]
fn vault_withdraw_and_checked_withdraw() {
    let env = Env::default();
    let contract_id = env.register(Vault, ());
    let client = VaultClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.initialize(&owner);
    client.deposit(&500);

    // withdraw without auth works (demonstrating deliberately buggy behavior)
    client.withdraw(&200);
    assert_eq!(client.balance(), 300);

    // withdraw_checked with mock_all_auths works
    env.mock_all_auths();
    client.withdraw_checked(&100);
    assert_eq!(client.balance(), 200);

    // withdraw more than balance returns InsufficientBalance
    let res = client.try_withdraw(&500);
    assert_eq!(res, Err(Ok(VaultError::InsufficientBalance)));
}

#[test]
fn vault_record_and_temp_storage() {
    let env = Env::default();
    let contract_id = env.register(Vault, ());
    let client = VaultClient::new(&env, &contract_id);

    client.set_record(&42);
    assert_eq!(client.touch_record(), 42);
    assert_eq!(client.touch_record_checked(), 42);

    client.set_temp(&99);
    assert_eq!(client.read_temp_unchecked(), 99);
    assert_eq!(client.read_temp_checked(), 99);
}

#[test]
fn vault_batch_payout() {
    let env = Env::default();
    let contract_id = env.register(Vault, ());
    let client = VaultClient::new(&env, &contract_id);

    let addr1 = Address::generate(&env);
    let addr2 = Address::generate(&env);
    let recipients = soroban_sdk::vec![&env, addr1, addr2];

    let count = client.batch_payout(&recipients, &100);
    assert_eq!(count, 2);
}
