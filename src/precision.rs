//! The three precision suggestions from `src/precisionFixed.js`,
//! `src/precisionRound.js` and `src/precisionPrefix.js`.
//!
//! MIGRATION_TO_RUST.md section 5.5 keeps these returning `f64` rather than
//! `Option<u32>`, because d3 returns NaN for a zero or non-finite step and
//! `test/precision*-test.js` asserts exactly that. Handing back a float preserves
//! the contract those assertions describe; [`as_precision`] is the ergonomic step
//! from there to the `u32` a specifier wants, and it is a conversion, not a
//! replacement.
//!
//! # Why these do not use `f64::max`
//!
//! d3 writes `Math.max(0, ...)`, and `Math.max` is NaN-propagating: any NaN
//! argument makes the result NaN. Rust's `f64::max` is the opposite -- it returns
//! the non-NaN operand, so `0.0_f64.max(f64::NAN)` is `0.0`. Reaching for it here
//! would turn every one of d3's NaN results into zero and would pass a suite that
//! only checked the finite cases. [`js_max`] is the operation `Math.max` actually
//! is, down to the bits of the NaN it answers with -- see [`MATH_NAN`].

use crate::decimal;

/// The NaN `Math.max` and `Math.min` answer with.
///
/// Not `f64::NAN`. Rust's is the positive quiet NaN, `0x7FF8000000000000`; the
/// oracle's two builtins return the *negative* one whatever their operands are,
/// including a NaN with a payload and a NaN that arrived positive:
///
/// ```text
/// Math.max(0, NaN)                 FFF8000000000000
/// Math.max(0, fromBits(7FF8..ABCDEF))  FFF8000000000000
/// Math.min(0, NaN)                 FFF8000000000000
/// NaN - 0                          7FF8000000000000   // ordinary arithmetic does not
/// ```
///
/// It matters because every NaN these helpers can return passes through one of the
/// two builtins on its way out, `precisionRound`'s `1 + …` included, and section
/// 3.6 compares numeric results by bit pattern. Answering `f64::NAN` disagrees with
/// the oracle on all twelve of the recorded degenerate calls.
const MATH_NAN: f64 = -f64::NAN;

/// `Math.max(a, b)`.
///
/// NaN-propagating, and `Math.max(+0, -0)` is `+0`.
fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return MATH_NAN;
    }
    if a > b {
        a
    } else if b > a {
        b
    } else if a == 0.0 && a.is_sign_negative() {
        // Equal, and both are zero with `a` the negative one.
        b
    } else {
        a
    }
}

/// `src/exponent.js`, with d3's NaN in place of the port's `None`.
fn js_exponent(x: f64) -> f64 {
    match decimal::exponent(x) {
        Some(exponent) => f64::from(exponent),
        None => f64::NAN,
    }
}

/// The number of fixed-point decimal digits needed to show `step`.
///
/// `Math.max(0, -exponent(Math.abs(step)))`. Returns NaN for a zero or non-finite
/// step, because d3 does and the oracle asserts it.
///
/// ```
/// use d3_format::precision_fixed;
///
/// assert_eq!(precision_fixed(8.9), 0.0);
/// assert_eq!(precision_fixed(0.089), 2.0);
/// assert!(precision_fixed(0.0).is_nan());
/// assert!(precision_fixed(f64::INFINITY).is_nan());
/// ```
pub fn precision_fixed(step: f64) -> f64 {
    js_max(0.0, -js_exponent(step.abs()))
}

/// The number of significant digits needed to tell `max` from `max - step`.
///
/// `1 + Math.max(0, exponent(Math.abs(max) - Math.abs(step)) -
/// exponent(Math.abs(step)))`. Section 4.4 requires the subtraction be an ordinary
/// `f64` one: the rounding it introduces is part of the answer, not an error in it.
///
/// ```
/// use d3_format::precision_round;
///
/// assert_eq!(precision_round(0.01, 0.99), 2.0);
/// assert_eq!(precision_round(0.01, 1.01), 3.0);
/// assert!(precision_round(0.0, 1.0).is_nan());
/// ```
pub fn precision_round(step: f64, max: f64) -> f64 {
    let step = step.abs();
    let max = max.abs() - step;
    let digits = js_max(0.0, js_exponent(max) - js_exponent(step));
    // `1 + NaN` keeps the NaN's sign bit on every target this crate builds for, but
    // "on every target" is a claim about hardware rather than about the language,
    // and the sign is compared. Returning the NaN says so instead of assuming it.
    if digits.is_nan() {
        digits
    } else {
        digits + 1.0
    }
}

/// The number of digits needed to show `step` under the SI prefix `value` selects.
///
/// `Math.max(0, Math.max(-8, Math.min(8, Math.floor(exponent(value) / 3))) * 3 -
/// exponent(Math.abs(step)))`. The inner clamp is the same SI bucket selection
/// `formatPrefix` uses, so the two cannot disagree about which prefix a value gets.
///
/// ```
/// use d3_format::precision_prefix;
///
/// assert_eq!(precision_prefix(1e-6, 1e-6), 0.0);  // 1µ
/// assert_eq!(precision_prefix(1e-9, 1e-6), 3.0);  // 0.001µ
/// assert!(precision_prefix(0.0, 1.0).is_nan());
/// ```
pub fn precision_prefix(step: f64, value: f64) -> f64 {
    let bucket = match decimal::si_prefix_exponent(value) {
        Some(exponent) => f64::from(exponent),
        None => f64::NAN,
    };
    js_max(0.0, bucket - js_exponent(step.abs()))
}

/// The `u32` precision a suggestion denotes, or `None` if it denotes none.
///
/// The three helpers above answer in `f64` because d3 does. This is the step from
/// that answer to the value
/// [`FormatSpecifierBuilder::precision`](crate::FormatSpecifierBuilder::precision)
/// takes. It is deliberately narrow: a NaN, a negative, a fraction or a value past
/// `u32::MAX` is `None`, so a caller has to decide what to do about it rather than
/// receiving a silently substituted zero.
///
/// ```
/// use d3_format::{as_precision, precision_fixed};
///
/// assert_eq!(as_precision(precision_fixed(0.089)), Some(2));
/// assert_eq!(as_precision(precision_fixed(0.0)), None);
/// assert_eq!(as_precision(-1.0), None);
/// assert_eq!(as_precision(2.5), None);
/// ```
pub fn as_precision(value: f64) -> Option<u32> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > f64::from(u32::MAX) {
        return None;
    }
    Some(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every NaN the three helpers can answer with, by bit pattern.
    ///
    /// `assert!(x.is_nan())` passes for both signs and every payload, so it cannot
    /// see the difference the differential compares on.
    fn assert_math_nan(value: f64, what: &str) {
        assert_eq!(
            format!("{:016X}", value.to_bits()),
            "FFF8000000000000",
            "{what} must be the NaN Math.max answers with"
        );
    }

    #[test]
    fn math_nan_is_the_negative_quiet_nan_the_oracle_returns() {
        assert_math_nan(MATH_NAN, "MATH_NAN");
        // Rust's own NaN is the other one, which is the whole reason for the
        // constant.
        assert_eq!(format!("{:016X}", f64::NAN.to_bits()), "7FF8000000000000");
    }

    #[test]
    fn js_max_propagates_nan_where_f64_max_would_swallow_it() {
        assert_math_nan(js_max(0.0, f64::NAN), "js_max(0, NaN)");
        assert_math_nan(js_max(f64::NAN, 0.0), "js_max(NaN, 0)");
        // A payload, and a sign, are both discarded: Math.max answers with one
        // fixed pattern.
        let payload = f64::from_bits(0x7FF8_0000_00AB_CDEF);
        assert_math_nan(js_max(0.0, payload), "js_max(0, NaN with a payload)");
        assert_math_nan(js_max(0.0, -f64::NAN), "js_max(0, -NaN)");
        // The trap this function exists to avoid.
        assert_eq!(0.0_f64.max(f64::NAN), 0.0);

        assert_eq!(js_max(0.0, 3.0), 3.0);
        assert_eq!(js_max(3.0, 0.0), 3.0);
        assert_eq!(js_max(-2.0, 0.0), 0.0);
        // Math.max(+0, -0) === +0, in either argument order.
        assert!(js_max(0.0, -0.0).is_sign_positive());
        assert!(js_max(-0.0, 0.0).is_sign_positive());
        assert!(js_max(-0.0, -0.0).is_sign_negative());
    }

    #[test]
    fn precision_fixed_matches_the_oracle_assertions() {
        assert_eq!(precision_fixed(8.9), 0.0);
        assert_eq!(precision_fixed(1.1), 0.0);
        assert_eq!(precision_fixed(0.89), 1.0);
        assert_eq!(precision_fixed(0.11), 1.0);
        assert_eq!(precision_fixed(0.089), 2.0);
        assert_eq!(precision_fixed(0.011), 2.0);
    }

    #[test]
    fn precision_fixed_is_nan_where_there_is_no_exponent() {
        for step in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_math_nan(precision_fixed(step), &format!("precisionFixed({step})"));
        }
    }

    #[test]
    fn precision_round_matches_the_oracle_assertions() {
        assert_eq!(precision_round(0.1, 1.1), 2.0);
        assert_eq!(precision_round(0.01, 0.99), 2.0);
        assert_eq!(precision_round(0.01, 1.00), 2.0);
        assert_eq!(precision_round(0.01, 1.01), 3.0);
    }

    #[test]
    fn precision_round_is_nan_where_there_is_no_exponent() {
        for step in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_math_nan(
                precision_round(step, 1.0),
                &format!("precisionRound({step}, 1)"),
            );
        }
    }

    #[test]
    fn precision_round_uses_ordinary_f64_subtraction() {
        // Section 4.4 lists this subtraction as one not to "correct". Here is a
        // case where correcting it would change the answer: exactly,
        // 1.0000000000000002 - 2.5e-16 is 0.99999999999999997..., which rounds to
        // the double below one and has decimal exponent -1. Rounded once, as f64
        // subtraction does it, the difference is 1.0, exponent 0.
        assert_eq!(1.0000000000000002_f64 - 2.5e-16, 1.0);
        // Node agrees: `precisionRound(2.5e-16, 1.0000000000000002)` is 17. An
        // exact difference would make it 16.
        assert_eq!(precision_round(2.5e-16, 1.0000000000000002), 17.0);
    }

    #[test]
    fn precision_prefix_returns_zero_when_step_shares_the_values_units() {
        // `test/precisionPrefix-test.js`, generalized over all 17 prefixes.
        let mut bucket = -24;
        while bucket <= 24 {
            for exponent in bucket..bucket + 3 {
                let step = format!("1e{bucket}").parse::<f64>().expect("finite");
                let value = format!("1e{exponent}").parse::<f64>().expect("finite");
                assert_eq!(
                    precision_prefix(step, value),
                    0.0,
                    "1e{bucket}, 1e{exponent}"
                );
            }
            bucket += 3;
        }
    }

    #[test]
    fn precision_prefix_counts_the_fractional_digits_a_step_needs() {
        let mut bucket = -24;
        while bucket <= 24 {
            for exponent in bucket - 4..bucket {
                let step = format!("1e{exponent}").parse::<f64>().expect("finite");
                let value = format!("1e{bucket}").parse::<f64>().expect("finite");
                assert_eq!(
                    precision_prefix(step, value),
                    f64::from(bucket - exponent),
                    "1e{exponent}, 1e{bucket}"
                );
            }
            bucket += 3;
        }
    }

    #[test]
    fn precision_prefix_saturates_at_the_yocto_and_yotta_buckets() {
        assert_eq!(precision_prefix(1e-24, 1e-24), 0.0);
        assert_eq!(precision_prefix(1e-25, 1e-25), 1.0);
        assert_eq!(precision_prefix(1e-26, 1e-26), 2.0);
        assert_eq!(precision_prefix(1e-27, 1e-27), 3.0);
        assert_eq!(precision_prefix(1e-28, 1e-28), 4.0);
        assert_eq!(precision_prefix(1e24, 1e24), 0.0);
        assert_eq!(precision_prefix(1e24, 1e25), 0.0);
        assert_eq!(precision_prefix(1e24, 1e26), 0.0);
        assert_eq!(precision_prefix(1e24, 1e27), 0.0);
        assert_eq!(precision_prefix(1e23, 1e27), 1.0);
    }

    #[test]
    fn precision_prefix_is_nan_where_there_is_no_exponent() {
        for step in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_math_nan(
                precision_prefix(step, 1.0),
                &format!("precisionPrefix({step}, 1)"),
            );
        }
        // A degenerate *value* has no bucket either.
        for value in [0.0, f64::NAN, f64::INFINITY] {
            assert_math_nan(
                precision_prefix(1.0, value),
                &format!("precisionPrefix(1, {value})"),
            );
        }
    }

    #[test]
    fn as_precision_accepts_only_whole_non_negative_values_in_range() {
        assert_eq!(as_precision(0.0), Some(0));
        assert_eq!(as_precision(-0.0), Some(0));
        assert_eq!(as_precision(21.0), Some(21));
        assert_eq!(as_precision(f64::from(u32::MAX)), Some(u32::MAX));
        assert_eq!(as_precision(f64::NAN), None);
        assert_eq!(as_precision(f64::INFINITY), None);
        assert_eq!(as_precision(-1.0), None);
        assert_eq!(as_precision(2.5), None);
        assert_eq!(as_precision(f64::from(u32::MAX) + 1.0), None);
    }
}
