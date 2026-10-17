//! Systematically prove that privileged entry points reject unauthorized
//! callers — the most common Soroban vulnerability class.
//!
//! [`AuthMatrix`] registers each privileged entry point with the addresses
//! allowed to call it, then cross-checks every **known actor** — any address
//! named in at least one entry point's `allowed` list — against every entry
//! point. The definition of a known actor is documented on [`AuthMatrix`] and
//! restated in the failure report so the coverage it implies is never
//! ambiguous.

mod matrix;

pub use matrix::AuthMatrix;
