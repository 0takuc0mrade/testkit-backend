//! One-line Stellar Asset Contract test doubles.
//!
//! # Units
//!
//! Amounts in this module come in two units, and every method names the one
//! it takes so a caller never has to guess:
//!
//! - **Whole units** — what a human means by "100 XLM". Used only by
//!   [`TestToken::mint`], which converts to base units internally.
//! - **Base units** — the on-chain integer the protocol actually stores,
//!   equal to `whole * 10^7` (the SAC's fixed 7 decimals). Used by
//!   [`TestToken::mint_raw`], [`TestToken::balance`],
//!   [`TestToken::assert_balance`] and [`TestToken::total_tracked`], and it
//!   is what every Stellar Asset Contract reports. Base units are also called
//!   *stroops*; this module uses the two terms interchangeably.
//!
//! Mixing them up is the classic off-by-`10^7` bug, so each method states its
//! unit in its own docs and in its failure messages.
//!
//! # Administrative authorization
//!
//! [`TestToken`] deploys a real Stellar Asset Contract with a fresh test
//! address as its admin. It mocks that **admin's** authorization for
//! [`TestToken::mint`] / [`TestToken::mint_raw`] only, so minting works with
//! no global auth mocking enabled — but every other token operation
//! (transfers, burns, approvals) still needs the test to authorize the caller
//! with `mock_all_auths` or a per-call `mock_auths`.
//!
//! The admin is fixed at deployment; this double deliberately offers no way to
//! change it.

mod sac;

pub use sac::{base_to_whole_units, whole_to_base_units, TestToken};
