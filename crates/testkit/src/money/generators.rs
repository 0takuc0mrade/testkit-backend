//! Adversarial value generators for **signed** money math.
//!
//! Stellar contracts model token amounts as a signed [`i128`]: a negative
//! amount is a meaningful value (a debit, a refund, or the result of a
//! subtraction), not an error to be filtered out. Every generator in this
//! module that produces an *amount* therefore works in signed [`i128`] and
//! deliberately includes the signed extremes, because a subtraction that
//! reaches the negative boundary is exactly the class of bug these values
//! exist to catch:
//!
//! - [`adversarial_amounts`] returns `-1` and both [`i128::MIN`] and
//!   `i128::MIN + 1`.
//! - [`amounts_in`] draws from a signed, inclusive `[min, max]` range, so a
//!   caller asking for `[-100, 100]` gets negative draws as well as positive
//!   ones.
//! - [`overflow_edge_triples`] carries a signed amount alongside its duration
//!   and rate.
//!
//! The one exception is [`bps_values`]: a basis-point *fraction* is a [`u32`]
//! and cannot be negative, so its adversarial coverage is out-of-range large
//! values (`10_001`, [`u32::MAX`]) rather than signed extremes.

use crate::core::TestEnv;

/// Values chosen to break money math: `0`, `1`, `-1`, [`i128::MAX`],
/// [`i128::MIN`], `i128::MAX - 1`, [`i128::MIN`] `+ 1`, stroop boundaries
/// (`10^7`, the standard Stellar asset decimal count), and values around the
/// midpoint of the `i128` range, which is where naive `amount * rate`
/// multiplications before a `/ bps_denominator` division tend to overflow
/// first.
///
/// The amounts are **signed** [`i128`] values: `-1`, [`i128::MIN`], and
/// [`i128::MIN`] `+ 1` are included on purpose, so a contract that mishandles
/// negative balances, refunds, or debits is exercised rather than skipped.
///
/// # Example
///
/// ```
/// use soroban_testkit::money::adversarial_amounts;
///
/// let amounts = adversarial_amounts();
/// assert!(amounts.contains(&0));
/// assert!(amounts.contains(&i128::MAX));
/// assert!(amounts.contains(&i128::MIN));
/// ```
pub fn adversarial_amounts() -> Vec<i128> {
    let stroop = 10_i128.pow(7);
    vec![
        0,
        1,
        -1,
        i128::MAX,
        i128::MIN,
        i128::MAX - 1,
        i128::MIN + 1,
        stroop,
        stroop - 1,
        stroop + 1,
        i128::MAX / 2,
        i128::MAX / 2 + 1,
        i128::MIN / 2,
    ]
}

/// Random **signed** amounts in the inclusive range `[min, max]`, seeded from
/// `env` so that a test using [`TestEnv::with_seed`] gets the same amounts on
/// every run.
///
/// Because the range is signed, `min` may be negative: `amounts_in(&env,
/// -100, -1, n)` yields only negative amounts, and `amounts_in(&env, -100,
/// 100, n)` draws from both sides of zero. This mirrors contract math where a
/// negative amount is a valid debit, refund, or subtraction result.
///
/// Panics if `min > max`.
///
/// # Example
///
/// ```
/// use soroban_testkit::core::TestEnv;
/// use soroban_testkit::money::amounts_in;
///
/// let env = TestEnv::with_seed(7);
/// let amounts = amounts_in(&env, 0, 100, 5);
/// assert_eq!(amounts.len(), 5);
/// assert!(amounts.iter().all(|a| (0..=100).contains(a)));
///
/// let again = amounts_in(&TestEnv::with_seed(7), 0, 100, 5);
/// assert_eq!(amounts, again);
///
/// // Signed ranges can produce negative amounts:
/// let debits = amounts_in(&env, -100, -1, 5);
/// assert!(debits.iter().all(|a| (-100..=-1).contains(a)));
/// ```
pub fn amounts_in(env: &TestEnv, min: i128, max: i128, n: usize) -> Vec<i128> {
    assert!(
        min <= max,
        "money::amounts_in: min ({min}) must be <= max ({max})"
    );

    let mut state = env.seed() ^ 0xD1B5_4A32_D192_ED03;
    let span_u128 = (max as u128).wrapping_sub(min as u128);

    (0..n)
        .map(|_| {
            let low = splitmix64(&mut state) as u128;
            let high = splitmix64(&mut state) as u128;
            let draw = low | (high << 64);
            let offset = if span_u128 == u128::MAX {
                draw
            } else {
                draw % (span_u128 + 1)
            };
            (min as u128).wrapping_add(offset) as i128
        })
        .collect()
}

/// Amount/duration/rate triples where `rate * duration` sits at or just
/// past the point where it would overflow `i128` — the pattern used by
/// interest and streaming-payout accrual math.
///
/// The amount component is a **signed** [`i128`] (the same representation a
/// contract uses for balances and deltas); the duration is an unsigned [`u64`]
/// and the rate is a signed [`i128`]. The triples pin the exact `i128`
/// boundaries so a contract's accrual path is checked where it is most likely
/// to overflow.
///
/// # Example
///
/// ```
/// use soroban_testkit::money::overflow_edge_triples;
///
/// let triples = overflow_edge_triples();
/// assert!(!triples.is_empty());
/// ```
pub fn overflow_edge_triples() -> Vec<(i128, u64, i128)> {
    let stroop = 10_i128.pow(7);
    let rate = 10_000_i128;
    let boundary_duration = (i128::MAX / rate) as u64;

    vec![
        (0, 0, 0),
        (1, 1, 1),
        (i128::MAX, 0, 0),
        (i128::MAX, 1, 1),
        (1, u64::MAX, i128::MAX),
        (stroop, boundary_duration, rate),
        (stroop, boundary_duration.saturating_add(1), rate),
    ]
}

/// Basis-point values worth testing: `0`, `1`, `9999`, `10000` (100%), and
/// values past the valid range that a contract should reject.
///
/// Unlike the amount generators, this one is **unsigned**: a basis-point
/// *fraction* is a [`u32`] by definition (`10_000` bps = 100%), so it has no
/// negative members. Its adversarial coverage is therefore the out-of-range
/// large values (`10_001`, [`u32::MAX`]) a contract must reject, not signed
/// extremes.
///
/// # Example
///
/// ```
/// use soroban_testkit::money::bps_values;
///
/// let values = bps_values();
/// assert!(values.contains(&0));
/// assert!(values.contains(&10_000));
/// assert!(values.iter().any(|&v| v > 10_000));
/// ```
pub fn bps_values() -> Vec<u32> {
    vec![0, 1, 9_999, 10_000, 10_001, u32::MAX]
}

/// A small, fast, deterministic PRNG (SplitMix64) — good enough for
/// generating adversarial test inputs, not for anything cryptographic.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adversarial_amounts_includes_every_documented_boundary() {
        let amounts = adversarial_amounts();
        let stroop = 10_i128.pow(7);
        for expected in [
            0,
            1,
            -1,
            i128::MAX,
            i128::MIN,
            i128::MAX - 1,
            stroop,
            stroop - 1,
            stroop + 1,
        ] {
            assert!(
                amounts.contains(&expected),
                "adversarial_amounts() is missing {expected}"
            );
        }
        assert_eq!(amounts.len(), 13);
    }

    #[test]
    fn adversarial_amounts_includes_every_signed_extreme() {
        let amounts = adversarial_amounts();
        // The signed minimum boundaries are present so negative-overflow
        // paths are exercised, not merely documented.
        for expected in [-1, i128::MIN, i128::MIN + 1] {
            assert!(
                amounts.contains(&expected),
                "adversarial_amounts() must include the signed boundary {expected}"
            );
        }
    }

    #[test]
    fn amounts_in_stays_within_bounds() {
        let env = TestEnv::with_seed(1);
        let amounts = amounts_in(&env, -50, 50, 200);
        assert_eq!(amounts.len(), 200);
        assert!(amounts.iter().all(|a| (-50..=50).contains(a)));
    }

    #[test]
    fn amounts_in_can_draw_negative_amounts_from_a_signed_range() {
        let env = TestEnv::with_seed(5);
        let amounts = amounts_in(&env, -100, -1, 64);
        assert_eq!(amounts.len(), 64);
        assert!(
            amounts.iter().all(|a| (-100..=-1).contains(a)),
            "every draw from the range [-100, -1] must be negative: {amounts:?}"
        );
        assert!(amounts.iter().any(|&a| a < 0));
    }

    #[test]
    fn amounts_in_spans_both_sides_of_zero() {
        let env = TestEnv::with_seed(11);
        let amounts = amounts_in(&env, -1_000, 1_000, 500);
        assert!(amounts.iter().any(|&a| a < 0), "expected negative draws");
        assert!(amounts.iter().any(|&a| a > 0), "expected positive draws");
    }

    #[test]
    fn amounts_in_is_reproducible_for_same_seed() {
        let a = amounts_in(&TestEnv::with_seed(99), 0, 1_000_000, 20);
        let b = amounts_in(&TestEnv::with_seed(99), 0, 1_000_000, 20);
        assert_eq!(a, b);
    }

    #[test]
    fn amounts_in_handles_full_i128_range() {
        let env = TestEnv::with_seed(3);
        let amounts = amounts_in(&env, i128::MIN, i128::MAX, 10);
        assert_eq!(amounts.len(), 10);
    }

    #[test]
    fn overflow_edge_triples_brackets_the_overflow_boundary() {
        let triples = overflow_edge_triples();
        let overflowing = triples
            .iter()
            .any(|&(_, duration, rate)| (duration as i128).checked_mul(rate).is_none());
        assert!(
            overflowing,
            "overflow_edge_triples() should include at least one triple whose rate * duration overflows i128"
        );
    }

    #[test]
    fn bps_values_includes_every_documented_boundary() {
        let values = bps_values();
        for expected in [0, 1, 9_999, 10_000] {
            assert!(values.contains(&expected));
        }
        assert!(values.iter().any(|&v| v > 10_000));
    }

    #[test]
    fn bps_values_are_unsigned_and_cover_the_upper_range() {
        // bps_values returns `u32`, so negatives are unrepresentable by type;
        // adversarial coverage is the out-of-range large values instead.
        let values: Vec<u32> = bps_values();
        assert!(values.contains(&10_001));
        assert!(values.contains(&u32::MAX));
    }
}
