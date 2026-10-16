use std::cell::Cell;

use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{Address, Env, IntoVal};

use crate::core::{TestEnv, TestkitError};

/// The decimal precision every Stellar Asset Contract uses. This is fixed
/// by the protocol (the classic Stellar asset precision), not something a
/// deployer can choose — verified against soroban-sdk 27.0.6:
/// `register_stellar_asset_contract_v2` takes no decimals parameter, and a
/// freshly deployed SAC's own `decimals()` always returns 7.
const SAC_DECIMALS: u32 = 7;

/// A deployed Stellar Asset Contract test double, with one-line mint/balance
/// helpers so tests don't hand-roll SAC setup boilerplate.
///
/// # Administrative authorization
///
/// The double deploys the SAC with a fresh test address as its admin and
/// mocks that admin's authorization for [`TestToken::mint`] /
/// [`TestToken::mint_raw`] only — minting therefore works with no global auth
/// mocking enabled. Every other operation (transfers, burns, approvals) still
/// needs the test to authorize the caller with `mock_all_auths` or a per-call
/// `mock_auths`; see [`crate::tokens`] for the same rule stated module-wide.
pub struct TestToken {
    env: Env,
    address: Address,
    admin: Address,
    total_tracked: Cell<i128>,
}

impl TestToken {
    fn new(env: &Env, admin: Address) -> Self {
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        Self {
            env: env.clone(),
            address: sac.address(),
            admin,
            total_tracked: Cell::new(0),
        }
    }

    /// The token contract's address.
    pub fn address(&self) -> Address {
        self.address.clone()
    }

    /// The token's decimal precision. Always `7`, fixed by the protocol.
    pub fn decimals(&self) -> u32 {
        SAC_DECIMALS
    }

    /// Mint `amount` **whole units** to `to` (converted internally to base
    /// units, i.e. `amount * 10^decimals`).
    ///
    /// The Stellar Asset Contract admin's authorization is mocked for this
    /// call, so it succeeds with no global auth mocking enabled.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::Misuse`] if the conversion to base
    /// units would overflow `i128`; use [`TestToken::mint_raw`] for amounts
    /// already in base units.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token();
    /// let alice = env.address();
    ///
    /// token.mint(&alice, 100);
    /// token.assert_balance(&alice, 1_000_000_000); // 100 * 10^7
    /// ```
    pub fn mint(&self, to: &Address, amount: i128) {
        let scale = 10i128.pow(self.decimals());
        let raw = amount.checked_mul(scale).unwrap_or_else(|| {
            panic!(
                "{}",
                TestkitError::Misuse(format!(
                    "mint({amount}) overflows i128 once converted to base units at {} \
                     decimals; use mint_raw for amounts already in base units",
                    self.decimals()
                ))
            )
        });
        self.mint_raw(to, raw);
    }

    /// Mint `amount` **base units** (stroops) to `to`, with no unit
    /// conversion.
    ///
    /// The Stellar Asset Contract admin's authorization is mocked for this
    /// call, so it succeeds with no global auth mocking enabled.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token();
    /// let alice = env.address();
    ///
    /// token.mint_raw(&alice, 42);
    /// token.assert_balance(&alice, 42);
    /// ```
    pub fn mint_raw(&self, to: &Address, amount: i128) {
        StellarAssetClient::new(&self.env, &self.address)
            .mock_auths(&[MockAuth {
                address: &self.admin,
                invoke: &MockAuthInvoke {
                    contract: &self.address,
                    fn_name: "mint",
                    args: (to.clone(), amount).into_val(&self.env),
                    sub_invokes: &[],
                },
            }])
            .mint(to, &amount);
        self.total_tracked.set(self.total_tracked.get() + amount);
    }

    /// The balance of `of`, in **base units**.
    pub fn balance(&self, of: &Address) -> i128 {
        TokenClient::new(&self.env, &self.address).balance(of)
    }

    /// Assert that `of`'s balance equals `expected`, in **base units**.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::AssertionFailed`] showing the expected
    /// and actual balances if they differ.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token();
    /// let alice = env.address();
    ///
    /// token.mint_raw(&alice, 10);
    /// token.assert_balance(&alice, 10);
    /// ```
    pub fn assert_balance(&self, of: &Address, expected: i128) {
        let actual = self.balance(of);
        if actual != expected {
            panic!(
                "{}",
                TestkitError::AssertionFailed(format!(
                    "expected balance of {of:?} to be {expected}, found {actual}"
                ))
            );
        }
    }

    /// Total **base units** minted through this `TestToken` (via
    /// [`TestToken::mint`] or [`TestToken::mint_raw`]) across every
    /// recipient — for supply-conservation assertions.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token();
    /// token.mint_raw(&env.address(), 10);
    /// token.mint_raw(&env.address(), 5);
    /// assert_eq!(token.total_tracked(), 15);
    /// ```
    pub fn total_tracked(&self) -> i128 {
        self.total_tracked.get()
    }
}

impl TestEnv {
    /// Deploy a Stellar Asset Contract with the standard 7 decimals, using a
    /// fresh test address as its admin.
    ///
    /// The admin's authorization is mocked for minting, so [`TestToken::mint`]
    /// / [`TestToken::mint_raw`] need no global auth mocking; other operations
    /// still do — see [`crate::tokens`].
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token();
    /// assert_eq!(token.decimals(), 7);
    /// ```
    pub fn token(&self) -> TestToken {
        let admin = self.address();
        TestToken::new(self.env(), admin)
    }

    /// Deploy a Stellar Asset Contract, asserting it uses `decimals`.
    ///
    /// # Panics
    ///
    /// Panics with a [`TestkitError::Misuse`] if `decimals != 7`. Stellar
    /// Asset Contracts fix their decimal precision at 7 (the classic
    /// Stellar asset precision) — verified against soroban-sdk 27.0.6,
    /// where `register_stellar_asset_contract_v2` takes no decimals
    /// parameter. This method exists so a test that expects a specific
    /// precision fails loudly if that ever changes, rather than silently
    /// assuming it.
    ///
    /// # Example
    ///
    /// ```
    /// use soroban_testkit::core::TestEnv;
    ///
    /// let env = TestEnv::new();
    /// let token = env.token_with_decimals(7);
    /// assert_eq!(token.decimals(), 7);
    /// ```
    pub fn token_with_decimals(&self, decimals: u32) -> TestToken {
        if decimals != SAC_DECIMALS {
            panic!(
                "{}",
                TestkitError::Misuse(format!(
                    "token_with_decimals({decimals}): Stellar Asset Contracts fix decimals at \
                     {SAC_DECIMALS} and cannot be deployed with a different precision; use \
                     token() for the standard {SAC_DECIMALS}-decimal token"
                ))
            );
        }
        self.token()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl};

    #[test]
    fn token_is_immediately_usable_for_transfers() {
        let env = TestEnv::new();
        let token = env.token();
        let alice = env.address();
        let bob = env.address();
        token.mint_raw(&alice, 100);

        TokenClient::new(env.env(), &token.address())
            .mock_auths(&[MockAuth {
                address: &alice,
                invoke: &MockAuthInvoke {
                    contract: &token.address(),
                    fn_name: "transfer",
                    args: (alice.clone(), bob.clone(), 30_i128).into_val(env.env()),
                    sub_invokes: &[],
                },
            }])
            .transfer(&alice, soroban_sdk::MuxedAddress::from(bob.clone()), &30);

        token.assert_balance(&alice, 70);
        token.assert_balance(&bob, 30);
    }

    #[test]
    fn mint_works_with_no_global_auth_mocking() {
        let env = TestEnv::new();
        let token = env.token();
        let alice = env.address();

        // No mock_all_auths/mock_auths anywhere: the double mocks the SAC
        // admin's authorization for mint itself.
        token.mint(&alice, 2);
        token.mint_raw(&alice, 42);

        token.assert_balance(&alice, 2 * 10i128.pow(7) + 42);
    }

    #[test]
    fn transfers_still_require_authorization() {
        let env = TestEnv::new();
        let token = env.token();
        let alice = env.address();
        let bob = env.address();
        token.mint_raw(&alice, 100);

        // Only minting is mocked; an un-authorized transfer must still fail.
        env.assert_requires_auth(|| {
            TokenClient::new(env.env(), &token.address()).transfer(
                &alice,
                soroban_sdk::MuxedAddress::from(bob.clone()),
                &10,
            );
        });
    }

    #[test]
    fn mint_converts_whole_units_at_seven_decimals() {
        let env = TestEnv::new();
        let token = env.token();
        let alice = env.address();
        token.mint(&alice, 100);
        token.assert_balance(&alice, 1_000_000_000);
    }

    #[test]
    fn mint_and_mint_raw_disagree_by_exactly_ten_pow_decimals() {
        let env = TestEnv::new();
        let token = env.token();
        let a = env.address();
        let b = env.address();

        token.mint(&a, 1);
        token.mint_raw(&b, 1);

        assert_eq!(token.balance(&a), token.balance(&b) * 10i128.pow(7));
    }

    #[test]
    #[should_panic(expected = "token_with_decimals(6)")]
    fn token_with_decimals_rejects_non_seven() {
        let env = TestEnv::new();
        let _ = env.token_with_decimals(6);
    }

    #[test]
    fn total_tracked_sums_every_mint() {
        let env = TestEnv::new();
        let token = env.token();
        token.mint_raw(&env.address(), 10);
        token.mint(&env.address(), 1);
        assert_eq!(token.total_tracked(), 10 + 10i128.pow(7));
    }

    #[contract]
    struct Wallet;

    #[contractimpl]
    impl Wallet {
        pub fn send(env: Env, token: Address, from: Address, to: Address, amount: i128) {
            TokenClient::new(&env, &token).transfer(
                &from,
                soroban_sdk::MuxedAddress::from(to),
                &amount,
            );
        }

        pub fn spend(env: Env, token: Address, from: Address, amount: i128) {
            TokenClient::new(&env, &token).burn(&from, &amount);
        }

        pub fn allow(
            env: Env,
            token: Address,
            from: Address,
            spender: Address,
            amount: i128,
            live_until: u32,
        ) {
            TokenClient::new(&env, &token).approve(&from, &spender, &amount, &live_until);
        }
    }

    #[test]
    fn works_with_a_contract_that_transfers_burns_and_approves() {
        let env = TestEnv::new();
        let token = env.token();
        let alice = env.address();
        let bob = env.address();
        let spender = env.address();
        token.mint_raw(&alice, 1_000);

        env.env().mock_all_auths_allowing_non_root_auth();
        let wallet_id = env.env().register(Wallet, ());
        let wallet = WalletClient::new(env.env(), &wallet_id);

        wallet.send(&token.address(), &alice, &bob, &300);
        token.assert_balance(&alice, 700);
        token.assert_balance(&bob, 300);

        wallet.spend(&token.address(), &alice, &200);
        token.assert_balance(&alice, 500);

        wallet.allow(&token.address(), &alice, &spender, &50, &1_000);
        let allowance = TokenClient::new(env.env(), &token.address()).allowance(&alice, &spender);
        assert_eq!(allowance, 50);
    }
}
