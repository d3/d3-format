//! The numeric contract, exercised from outside the crate.
//!
//! Every expectation here was taken from Node v24.18.0, the pinned oracle, and
//! none of it is derived from the Rust implementation. The differential corpora
//! cover the same ground at scale; this file is the part a reader can check by
//! eye, and the part that fails loudly rather than statistically when one of the
//! layout ordering rules is dropped.

use d3_format::decimal::{
    exponent, format_decimal, format_decimal_parts, format_prefix_auto, format_rounded,
    js_round_nonnegative, number_to_string, percent_scaled, round_to_string_radix,
    si_prefix_exponent, si_prefix_scale, si_prefix_scaled, to_exponential, to_fixed,
    to_locale_string_en, to_precision,
};
use d3_format::{FormatError, FormatLimits};

const LIMITS: FormatLimits = FormatLimits::DEFAULT;

/// The `f64` one unit in the last place above `x`, for `x` finite and positive.
fn next_up(x: f64) -> f64 {
    f64::from_bits(x.to_bits() + 1)
}

/// The `f64` one unit in the last place below `x`, for `x` finite and positive.
fn next_down(x: f64) -> f64 {
    f64::from_bits(x.to_bits() - 1)
}

// ---------------------------------------------------------------------------
// Rounding: half away from zero, against the exact binary64 value
// ---------------------------------------------------------------------------

/// Section 0.3 records that a half-even simulation passes all 1,078 committed
/// assertions, so the ported suite cannot tell the two rules apart. Each value here
/// is an exact binary tie at the requested precision -- a sum of powers of two whose
/// scaled remainder is exactly one half -- so the two rules give different answers
/// and the `half_even` column is what this port must *not* produce.
#[test]
fn ties_round_away_from_zero_where_half_even_would_round_down() {
    // value, precision, ECMAScript, what half-to-even would have said
    let fixed: [(f64, u32, &str, &str); 9] = [
        (0.5, 0, "1", "0"),
        (2.5, 0, "3", "2"),
        (4.5, 0, "5", "4"),
        (0.125, 2, "0.13", "0.12"),
        (0.375, 2, "0.38", "0.38"),
        (1.0625, 3, "1.063", "1.062"),
        (0.015625, 5, "0.01563", "0.01562"),
        (8.5, 0, "9", "8"),
        (16.5, 0, "17", "16"),
    ];
    for (value, precision, ecmascript, half_even) in fixed {
        let actual = to_fixed(value, precision, &LIMITS).unwrap();
        assert_eq!(actual, ecmascript, "toFixed({value}, {precision})");
        if ecmascript != half_even {
            assert_ne!(
                actual, half_even,
                "toFixed({value}, {precision}) rounded to even"
            );
        }
    }

    // The same rule through the two other roundings, and through Rust's own
    // formatter for contrast: `{:.0}` is half-even and answers "2" for 2.5, which is
    // exactly why section 4.2 forbids delegating to it.
    assert_eq!(to_precision(2.5, 1, &LIMITS).unwrap(), "3");
    assert_eq!(to_exponential(2.5, Some(0), &LIMITS).unwrap(), "3e+0");
    assert_eq!(format!("{:.0}", 2.5_f64), "2");
    assert_eq!(format!("{:.2}", 0.125_f64), "0.12");
}

/// The rounding decision is made against the exact value of the bits, not against
/// the shortest decimal that identifies them. `1.005` looks like a tie and is not
/// one: the stored value is below it, so it rounds down.
#[test]
fn near_ties_are_decided_by_the_exact_binary_value() {
    assert_eq!(to_fixed(1.005, 2, &LIMITS).unwrap(), "1.00");
    assert_eq!(to_fixed(1.015, 2, &LIMITS).unwrap(), "1.01");
    assert_eq!(to_fixed(1.045, 2, &LIMITS).unwrap(), "1.04");
    assert_eq!(to_fixed(2.675, 2, &LIMITS).unwrap(), "2.67");
    assert_eq!(to_fixed(9.995, 2, &LIMITS).unwrap(), "9.99");
    // ... while these land above their apparent tie and round up. Nothing about the
    // decimal spelling predicts which group a value falls into.
    assert_eq!(to_fixed(8.535, 2, &LIMITS).unwrap(), "8.54");
    assert_eq!(to_fixed(2.345, 2, &LIMITS).unwrap(), "2.35");
    assert_eq!(to_fixed(4.355, 2, &LIMITS).unwrap(), "4.36");
}

/// `Number::toString` breaks its own ties the other way. Both spellings below are
/// shortest and both read back to the same bits; ECMAScript keeps the even one,
/// while `toFixed` at the same digit count keeps the larger one. Section 4.2's
/// half-away rule governs the rounding functions, not the shortest conversion, and
/// conflating them is a divergence the tie corpora would catch only here.
#[test]
fn the_shortest_conversion_breaks_exact_ties_toward_even() {
    // 2^50 + 1/4, written as a sum because the ulp at 2^50 is exactly 1/4 and the
    // literal spelling is what the test is about.
    let value = 1_125_899_906_842_624.0_f64 + 0.25;
    assert_eq!(
        number_to_string(value, &LIMITS).unwrap(),
        "1125899906842624.2"
    );
    assert_eq!(to_fixed(value, 1, &LIMITS).unwrap(), "1125899906842624.3");
    assert_eq!(
        to_precision(value, 17, &LIMITS).unwrap(),
        "1125899906842624.3"
    );
}

// ---------------------------------------------------------------------------
// The 1e21 threshold: two rules that share a number
// ---------------------------------------------------------------------------

/// Section 4.1 gives `toFixed` and type `d` the same threshold and different
/// behaviour above it, and different behaviour *below* it too: `toFixed` expands the
/// exact integer the bits denote, while type `d` expands the shortest digits. The
/// predecessor of `1e21` is where that difference is visible without leaving fixed
/// notation at all.
#[test]
fn the_1e21_threshold_is_two_independent_rules() {
    // value, toFixed(0), toFixed(2), type `d`
    let cases: [(f64, &str, &str, &str); 4] = [
        (
            next_down(1e21),
            "999999999999999868928",
            "999999999999999868928.00",
            "999999999999999900000",
        ),
        (1e21, "1e+21", "1e+21", "1000000000000000000000"),
        (
            next_up(1e21),
            "1.0000000000000001e+21",
            "1.0000000000000001e+21",
            "1000000000000000100000",
        ),
        (
            next_up(1e20),
            "100000000000000016384",
            "100000000000000016384.00",
            "100000000000000020000",
        ),
    ];
    for (value, fixed0, fixed2, decimal) in cases {
        let bits = value.to_bits();
        assert_eq!(
            to_fixed(value, 0, &LIMITS).unwrap(),
            fixed0,
            "toFixed({bits:x}, 0)"
        );
        assert_eq!(
            to_fixed(value, 2, &LIMITS).unwrap(),
            fixed2,
            "toFixed({bits:x}, 2)"
        );
        assert_eq!(
            format_decimal(value, &LIMITS).unwrap(),
            decimal,
            "type d {bits:x}"
        );
    }

    // Below the threshold the two rules can still coincide, and at `1e20` they do.
    assert_eq!(
        to_fixed(1e20, 2, &LIMITS).unwrap(),
        "100000000000000000000.00"
    );
    assert_eq!(
        format_decimal(1e20, &LIMITS).unwrap(),
        "100000000000000000000"
    );

    // Above it, type `d` never reaches exponential notation, however large.
    assert_eq!(
        format_decimal(9.999999999999998e21, &LIMITS).unwrap(),
        "9999999999999998000000"
    );
    assert_eq!(
        format_decimal(f64::MAX, &LIMITS).unwrap(),
        "17976931348623157".to_string() + &"0".repeat(292)
    );
    assert_eq!(
        to_fixed(f64::MAX, 2, &LIMITS).unwrap(),
        "1.7976931348623157e+308"
    );
}

/// Type `d` rounds the magnitude first; `Math.round` on the signed value would
/// answer differently at every negative tie. Section 4.4 fixes the order.
#[test]
fn integer_types_take_the_magnitude_before_rounding() {
    assert_eq!(js_round_nonnegative(-2.5), -2.0);
    assert_eq!(js_round_nonnegative(2.5), 3.0);
    // Type `d` takes the magnitude before rounding, so it prints 3; rounding the
    // signed value first would print -2.
    assert_eq!(format_decimal((-2.5_f64).abs(), &LIMITS).unwrap(), "3");
    assert_eq!(format_decimal(-2.5_f64, &LIMITS).unwrap(), "-2");
    assert_eq!(round_to_string_radix(2.5_f64, 2, &LIMITS).unwrap(), "11");
    assert_eq!(round_to_string_radix(0.5_f64, 2, &LIMITS).unwrap(), "1");
}

// ---------------------------------------------------------------------------
// toPrecision's notation switch
// ---------------------------------------------------------------------------

/// The switch is decided on the exponent *after* rounding, so a coefficient that
/// carries into a new decade changes the notation as well as the digits.
#[test]
fn to_precision_selects_notation_after_rounding() {
    // 999.9 at three significant digits rounds to 1000, whose exponent is 3, which
    // is not less than the precision -- so the answer is exponential.
    assert_eq!(to_precision(999.9, 3, &LIMITS).unwrap(), "1.00e+3");
    assert_eq!(to_precision(999.4, 3, &LIMITS).unwrap(), "999");
    // The same carry at the low end: 0.0000009999 rounds to 1e-6, whose exponent is
    // -6, which is not below -6 -- so the answer stays positional.
    assert_eq!(to_precision(9.999e-7, 3, &LIMITS).unwrap(), "0.00000100");
    assert_eq!(to_precision(9.99e-7, 2, &LIMITS).unwrap(), "0.0000010");
    assert_eq!(to_precision(9.99e-8, 2, &LIMITS).unwrap(), "1.0e-7");
    assert_eq!(to_precision(1e-6, 3, &LIMITS).unwrap(), "0.00000100");
    assert_eq!(to_precision(1e-7, 3, &LIMITS).unwrap(), "1.00e-7");
    assert_eq!(to_precision(1e21, 3, &LIMITS).unwrap(), "1.00e+21");
}

// ---------------------------------------------------------------------------
// Boundary values (section 4.1)
// ---------------------------------------------------------------------------

#[test]
fn the_non_finite_and_signed_zero_spellings_match_ecmascript() {
    assert_eq!(number_to_string(f64::NAN, &LIMITS).unwrap(), "NaN");
    // A NaN with the sign bit set is still "NaN": ECMAScript never prints its sign.
    let negative_nan = f64::from_bits(f64::NAN.to_bits() | 0x8000_0000_0000_0000);
    assert!(negative_nan.is_sign_negative() && negative_nan.is_nan());
    assert_eq!(number_to_string(negative_nan, &LIMITS).unwrap(), "NaN");
    assert_eq!(to_fixed(negative_nan, 2, &LIMITS).unwrap(), "NaN");
    assert_eq!(format_decimal(negative_nan, &LIMITS).unwrap(), "NaN");
    assert_eq!(
        round_to_string_radix(negative_nan, 16, &LIMITS).unwrap(),
        "NaN"
    );

    assert_eq!(
        number_to_string(f64::INFINITY, &LIMITS).unwrap(),
        "Infinity"
    );
    assert_eq!(
        number_to_string(f64::NEG_INFINITY, &LIMITS).unwrap(),
        "-Infinity"
    );
    assert_eq!(
        to_fixed(f64::NEG_INFINITY, 4, &LIMITS).unwrap(),
        "-Infinity"
    );
    // `toLocaleString("en")` spells infinity with the mathematical symbol, which is
    // what type `d` inherits above the threshold.
    assert_eq!(
        to_locale_string_en(f64::INFINITY, &LIMITS).unwrap(),
        "\u{221e}"
    );

    // Negative zero loses its sign in `Number::toString` and keeps it in `toFixed`
    // only when the rounded value is a negative non-zero. ECMAScript prints "0.00"
    // for -0 and "-0.00" for a small negative.
    assert_eq!(number_to_string(-0.0, &LIMITS).unwrap(), "0");
    assert_eq!(to_fixed(-0.0, 2, &LIMITS).unwrap(), "0.00");
    assert_eq!(to_fixed(-1e-9, 2, &LIMITS).unwrap(), "-0.00");
    assert_eq!(to_exponential(-0.0, Some(3), &LIMITS).unwrap(), "0.000e+0");
    assert_eq!(format_decimal(-0.0, &LIMITS).unwrap(), "0");
    assert_eq!(round_to_string_radix(-0.0, 2, &LIMITS).unwrap(), "0");
}

#[test]
fn the_representable_extremes_round_trip() {
    let min_subnormal = f64::from_bits(1);
    assert_eq!(number_to_string(min_subnormal, &LIMITS).unwrap(), "5e-324");
    assert_eq!(
        number_to_string(next_up(min_subnormal), &LIMITS).unwrap(),
        "1e-323"
    );
    assert_eq!(
        number_to_string(f64::MIN_POSITIVE, &LIMITS).unwrap(),
        "2.2250738585072014e-308"
    );
    assert_eq!(
        number_to_string(next_down(f64::MIN_POSITIVE), &LIMITS).unwrap(),
        "2.225073858507201e-308"
    );
    assert_eq!(
        number_to_string(f64::MAX, &LIMITS).unwrap(),
        "1.7976931348623157e+308"
    );
    assert_eq!(number_to_string(1e-6, &LIMITS).unwrap(), "0.000001");
    assert_eq!(number_to_string(1e-7, &LIMITS).unwrap(), "1e-7");
    assert_eq!(
        number_to_string(next_down(1e-6), &LIMITS).unwrap(),
        "9.999999999999997e-7"
    );
    assert_eq!(
        number_to_string(next_up(1e-7), &LIMITS).unwrap(),
        "1.0000000000000001e-7"
    );

    // The smallest subnormal has 751 zeros after the point at full expansion, so it
    // is also the widest thing `toFixed` will ever be asked for -- and `toFixed`
    // caps at 100 fraction digits, which is why it answers zero here.
    assert_eq!(
        to_fixed(min_subnormal, 100, &LIMITS).unwrap(),
        format!("0.{}", "0".repeat(100))
    );
    assert_eq!(
        to_exponential(min_subnormal, Some(20), &LIMITS).unwrap(),
        "4.94065645841246544177e-324"
    );
}

#[test]
fn radix_output_reaches_the_full_1024_bit_expansion() {
    let binary = round_to_string_radix(f64::MAX, 2, &LIMITS).unwrap();
    assert_eq!(binary.len(), 1024);
    assert_eq!(&binary[..53], "1".repeat(53));
    assert_eq!(&binary[53..], "0".repeat(971));

    assert_eq!(
        round_to_string_radix(f64::MAX, 16, &LIMITS).unwrap().len(),
        256
    );
    assert_eq!(round_to_string_radix(255.0, 16, &LIMITS).unwrap(), "ff");
    assert_eq!(round_to_string_radix(255.0, 8, &LIMITS).unwrap(), "377");
    assert_eq!(round_to_string_radix(-255.6, 16, &LIMITS).unwrap(), "-100");
    // The two code paths -- bit slicing for a power-of-two radix, repeated division
    // otherwise -- have to agree where both apply.
    assert_eq!(
        round_to_string_radix(1e21, 4, &LIMITS).unwrap(),
        "31203113021223130113132220000000000"
    );
    assert_eq!(
        round_to_string_radix(1e21, 6, &LIMITS).unwrap(),
        "551013104230421441113341344"
    );

    // Outside the radices d3 reaches, the port is exact and Node is not: V8 stops
    // emitting digits below half an ulp and pads with zeros, so it prints
    // "5v1j4f4ds7c000" for this. `Number.prototype.toString` is only specified as
    // "a generalization of" `Number::toString` for a radix other than ten, no d3
    // output depends on it, and `dump_cases` declines the case rather than claiming
    // an agreement it does not have.
    assert_eq!(
        round_to_string_radix(1e21, 36, &LIMITS).unwrap(),
        "5v1j4f4ds79m9s"
    );
}

// ---------------------------------------------------------------------------
// Scaling (section 4.4)
// ---------------------------------------------------------------------------

/// Percent scaling is one ordinary multiplication, so it inherits its rounding
/// error. Computing `x * 100` any other way -- `x / 0.01`, or a decimal shift --
/// gives a different `f64` for these inputs.
#[test]
fn percent_scaling_is_a_plain_multiplication() {
    assert_eq!(
        percent_scaled(0.07).to_bits(),
        7.000000000000001_f64.to_bits()
    );
    assert_ne!(percent_scaled(0.07).to_bits(), 7.0_f64.to_bits());
    assert_eq!(
        percent_scaled(0.29).to_bits(),
        28.999999999999996_f64.to_bits()
    );
    assert_eq!(
        number_to_string(percent_scaled(0.07), &LIMITS).unwrap(),
        "7.000000000000001"
    );
}

/// Section 0.3 records that the multiplication and the division disagree in 4 of 11
/// sampled pairs. `1e-9 * 12345678900` is one of them, and it is what `formatPrefix`
/// must produce.
#[test]
fn prefix_scaling_multiplies_by_a_pinned_constant() {
    assert_eq!(si_prefix_scale(1e9).to_bits(), 1e-9_f64.to_bits());
    let scaled = si_prefix_scaled(1e9, 12_345_678_900.0);
    assert_eq!(scaled.to_bits(), 12.345678900000001_f64.to_bits());
    assert_ne!(scaled.to_bits(), (12_345_678_900.0_f64 / 1e9).to_bits());

    // Every reachable bucket, and the clamp at both ends.
    let exponents: [(f64, i32); 7] = [
        (1e-30, -24),
        (1e-24, -24),
        (1e-3, -3),
        (1.0, 0),
        (1e3, 3),
        (1e24, 24),
        (1e30, 24),
    ];
    for (reference, expected) in exponents {
        assert_eq!(si_prefix_exponent(reference), Some(expected), "{reference}");
    }
    // Floor division, not truncation: `floor(-1 / 3)` is -1, so the bucket is -3.
    assert_eq!(si_prefix_exponent(1e-1), Some(-3));
    assert_eq!(si_prefix_exponent(1e-2), Some(-3));
    assert_eq!(si_prefix_exponent(1e-4), Some(-6));
}

/// A zero, NaN or infinite reference gives d3 a NaN scale, and a negative reference
/// selects from the magnitude. Neither may reach an `as i32` cast or a table index.
#[test]
fn a_degenerate_prefix_reference_yields_a_nan_scale() {
    for reference in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(si_prefix_exponent(reference), None, "{reference}");
        assert!(si_prefix_scale(reference).is_nan(), "{reference}");
        assert!(si_prefix_scaled(reference, 1.0).is_nan(), "{reference}");
        assert_eq!(exponent(reference), None, "{reference}");
    }
    assert_eq!(si_prefix_exponent(-1.5e-7), Some(-9));
    assert_eq!(si_prefix_exponent(-1e6), Some(6));
    assert_eq!(si_prefix_scale(-1e6).to_bits(), 1e-6_f64.to_bits());
    // The NaN scale is what the formatter will eventually turn into the locale's NaN
    // string with no SI suffix; the numeric layer's job is only to not crash.
    assert!(number_to_string(si_prefix_scaled(f64::NAN, 42.0), &LIMITS)
        .unwrap()
        .eq("NaN"));
}

// ---------------------------------------------------------------------------
// The d3-shaped helpers
// ---------------------------------------------------------------------------

/// `exponent` comes from the shortest-decimal computation. `log10().floor()` gets
/// these wrong: `1e23_f64.log10()` is 23.000000000000004 and `1e-7_f64.log10()` is
/// -7.000000000000001, and one of the two rounds the wrong way.
#[test]
fn the_decimal_exponent_is_not_a_logarithm() {
    let cases: [(f64, i32); 8] = [
        (1e23, 23),
        (1e-7, -7),
        (1e22, 22),
        (999.9, 2),
        (1.0, 0),
        (0.1, -1),
        (f64::MAX, 308),
        (f64::from_bits(1), -324),
    ];
    for (value, expected) in cases {
        assert_eq!(exponent(value), Some(expected), "{value}");
        assert_eq!(exponent(-value), Some(expected), "-{value}");
    }
    assert_eq!(exponent(0.0), None);
}

/// d3 passes `p` straight into `toExponential(p - 1)`, and JavaScript's falsy test
/// turns a zero into "no argument". A Rust port that subtracts first underflows.
#[test]
fn a_zero_precision_means_shortest_not_minus_one() {
    let (digits, exp) = format_decimal_parts(1.2345, Some(0), &LIMITS)
        .unwrap()
        .unwrap();
    assert_eq!((digits.as_str(), exp), ("12345", 0));
    let (digits, exp) = format_decimal_parts(1.2345, None, &LIMITS)
        .unwrap()
        .unwrap();
    assert_eq!((digits.as_str(), exp), ("12345", 0));
    let (digits, exp) = format_decimal_parts(1.2345, Some(3), &LIMITS)
        .unwrap()
        .unwrap();
    assert_eq!((digits.as_str(), exp), ("123", 0));

    assert_eq!(format_decimal_parts(0.0, Some(3), &LIMITS).unwrap(), None);
    assert_eq!(
        format_decimal_parts(f64::NAN, Some(3), &LIMITS).unwrap(),
        None
    );
    assert_eq!(
        format_decimal_parts(f64::INFINITY, None, &LIMITS).unwrap(),
        None
    );

    // The same zero arrives inside `formatPrefixAuto` when the value is below one
    // yocto and `p + i - 1` clamps to zero; the coefficient there is the shortest
    // one, and the call must not panic.
    let (text, prefix) = format_prefix_auto(1e-30, 1, &LIMITS).unwrap();
    assert_eq!((text.as_str(), prefix), ("0.000001", Some(-24)));
}

#[test]
fn the_type_r_and_type_s_layouts_match_d3() {
    assert_eq!(format_rounded(123.456, 4, &LIMITS).unwrap(), "123.5");
    assert_eq!(
        format_rounded(0.000123456, 4, &LIMITS).unwrap(),
        "0.0001235"
    );
    assert_eq!(format_rounded(1234567.0, 3, &LIMITS).unwrap(), "1230000");
    assert_eq!(format_rounded(0.0, 3, &LIMITS).unwrap(), "0");

    let (text, prefix) = format_prefix_auto(1500.0, 3, &LIMITS).unwrap();
    assert_eq!((text.as_str(), prefix), ("1.50", Some(3)));
    let (text, prefix) = format_prefix_auto(999_999.0, 3, &LIMITS).unwrap();
    // Rounds into the next bucket, and the bucket is chosen from the rounded
    // exponent, so this is "1.00" with the mega prefix rather than "1000" with kilo.
    assert_eq!((text.as_str(), prefix), ("1.00", Some(6)));
    let (text, prefix) = format_prefix_auto(0.0, 3, &LIMITS).unwrap();
    assert_eq!((text.as_str(), prefix), ("0.00", None));
    let (text, prefix) = format_prefix_auto(f64::NAN, 3, &LIMITS).unwrap();
    assert_eq!((text.as_str(), prefix), ("NaN", None));
}

// ---------------------------------------------------------------------------
// Allocation policy (section 5.4)
// ---------------------------------------------------------------------------

/// The engine reports the limit rather than allocating past it, and the check
/// happens before the work: a hostile precision must not buy an allocation first.
#[test]
fn output_limits_are_reported_not_allocated() {
    let tight = FormatLimits {
        max_width_utf16: 16,
        max_output_bytes: 8,
    };
    assert!(matches!(
        to_fixed(1.0, 100, &tight),
        Err(FormatError::OutputLimit { .. })
    ));
    assert!(matches!(
        round_to_string_radix(f64::MAX, 2, &tight),
        Err(FormatError::OutputLimit { .. })
    ));
    assert!(matches!(
        format_decimal(f64::MAX, &tight),
        Err(FormatError::OutputLimit { .. })
    ));
    assert_eq!(to_fixed(1.0, 2, &tight).unwrap(), "1.00");

    // The error is a value, not a panic, and it says what it wanted.
    match to_fixed(1.0, 100, &tight) {
        Err(FormatError::OutputLimit { requested, maximum }) => {
            assert!(requested > maximum);
            assert_eq!(maximum, 8);
        }
        other => panic!("expected an output limit, got {other:?}"),
    }
}
