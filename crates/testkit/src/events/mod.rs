//! Assert on contract events without hand-decoding `Val` topics.
//!
//! Events are captured as raw [`soroban_sdk::Val`]s and rendered in decoded
//! form in failure diagnostics, so what a failed assertion prints is always
//! aligned with the values the event actually carried.
//!
//! # Scope: nested invocation boundaries
//!
//! A captured [`EventLog`] covers exactly one **top-level** contract
//! invocation. Events emitted by nested sub-contract calls are included at any
//! depth — each attributed to the contract that published it — and all nested
//! calls of one top-level invocation accumulate into the same log. Starting
//! the next top-level call resets the log. See [`EventLog`] for the full note.

mod assertions;
mod capture;

pub use capture::{decode_topic_diagnostic, decode_val_diagnostic, CapturedEvent, EventLog};
