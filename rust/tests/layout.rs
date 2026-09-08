//! The section 4.4 risks that value layout owns, one test each.
//!
//! Section 4.4 lists thirteen behaviours that must survive the port. Six of them are
//! properties of the section 4.2 decimal engine and are pinned in `tests/decimal.rs`;
//! the rest are visible only once a value is laid out, and are pinned here.
//!
//! The generated suite in `tests/generated` already compares 793 recorded d3 calls
//! byte for byte, so this file is not where agreement is established. It is where a
//! *specific* regression is named: each test states the risk, and its failure says
//! which of the thirteen was dropped rather than that some string changed. Every
//! expectation was read from Node v24.18.0 running `src/index.js`, never from the
//! Rust implementation.

use d3_format::{FormatType, Locale};

/// `format(spec)(value)` in the default locale, for a test that needs no more.
fn fmt(spec: &str, value: f64) -> String {
    Locale::en_us()
        .formatter(spec)
        .unwrap_or_else(|error| panic!("{spec:?}: {error}"))
        .format_number(value)
        .unwrap_or_else(|error| panic!("{spec:?}: {error}"))
}

/// `formatPrefix(spec, reference)(value)` in the default locale.
fn prefixed(spec: &str, reference: f64, value: f64) -> String {
    Locale::en_us()
        .prefix_formatter(spec, reference)
        .unwrap_or_else(|error| panic!("{spec:?} @ {reference}: {error}"))
        .format_number(value)
        .unwrap_or_else(|error| panic!("{spec:?} @ {reference}: {error}"))
}

/// The `ar-001` numerals, which make numeral substitution visible.
fn arabic() -> Locale {
    Locale::builder()
        .decimal("\u{66b}")
        .thousands("\u{66c}")
        .grouping([3])
        .currency("", "")
        .numerals([
            "\u{660}".to_owned(),
            "\u{661}".to_owned(),
            "\u{662}".to_owned(),
            "\u{663}".to_owned(),
            "\u{664}".to_owned(),
            "\u{665}".to_owned(),
            "\u{666}".to_owned(),
            "\u{667}".to_owned(),
            "\u{668}".to_owned(),
            "\u{669}".to_owned(),
        ])
        .build()
        .expect("valid")
}

/// Section 4.4: post-rounding notation and SI exponent selection.
///
/// The SI prefix is chosen from the exponent of the *rounded* digits, not of the
/// input. `999999` at three significant digits rounds to `1.00`, which is a
/// megabyte's worth of exponent even though the input's own exponent is `10^5`; a
/// port that picked the bucket first would print `1000k`.
#[test]
fn the_si_bucket_follows_the_rounded_digits_not_the_input() {
    assert_eq!(fmt(".3s", 999_999.0), "1.00M");
    assert_eq!(fmt(".3s", 999_500.0), "1.00M");
    // One unit below the tie stays in the smaller bucket, which is the pair that
    // makes this a rounding question rather than a threshold.
    assert_eq!(fmt(".3s", 999_499.0), "999k");
    assert_eq!(fmt(".2s", 999.0), "1.0k");
    assert_eq!(fmt(".2s", 995.0), "1.0k");
    assert_eq!(fmt(".4s", 999_999.0), "1.000M");
    assert_eq!(fmt(".3s", 0.000_999_5), "999\u{b5}");

    // The same question for the non-SI types, which choose notation rather than a
    // bucket but choose it from the same rounded digits.
    assert_eq!(fmt(".2r", 999.0), "1000");
    assert_eq!(fmt(".3g", 999_999.0), "1.00e+6");
    assert_eq!(fmt(".2e", 999.0), "9.99e+2");

    // Past the ends of the table the bucket stops moving and the digits absorb it.
    assert_eq!(fmt("s", 1e24), "1.00000Y");
    assert_eq!(fmt("s", 1.5e24), "1.50000Y");
    assert_eq!(fmt(".3s", 1e-25), "0.10y");
}

/// Section 4.4: floor division via `div_euclid(3)`.
///
/// The SI table is indexed by `8 + e / 3`, and in JavaScript that division is
/// followed by an implicit floor for every negative exponent. Rust's `/` truncates
/// towards zero instead, so `-1 / 3` is `0` where `(-1).div_euclid(3)` is `-1`: the
/// whole small half of the table lands one bucket too high without it.
#[test]
fn the_si_table_index_floors_rather_than_truncates() {
    // Each decade from milli down to yocto, so every negative index is visited and
    // every one of them is a case truncation would move.
    assert_eq!(fmt(".3s", 0.1), "100m");
    assert_eq!(fmt(".3s", 0.01), "10.0m");
    assert_eq!(fmt(".3s", 0.001), "1.00m");
    assert_eq!(fmt(".3s", 0.000_1), "100\u{b5}");
    assert_eq!(fmt(".3s", 0.000_01), "10.0\u{b5}");
    assert_eq!(fmt(".3s", 0.000_001), "1.00\u{b5}");
    assert_eq!(fmt(".3s", 1e-7), "100n");
    assert_eq!(fmt(".3s", 1e-8), "10.0n");
    assert_eq!(fmt(".3s", 1e-9), "1.00n");
    assert_eq!(fmt(".3s", 1e-22), "100y");
    assert_eq!(fmt(".3s", 1e-23), "10.0y");
    assert_eq!(fmt(".3s", 1e-24), "1.00y");
}

/// Section 4.4: abs-before-`Math.round` ordering for the integer types.
///
/// `Math.round` breaks ties towards positive infinity, so its answer for a negative
/// tie depends on whether the magnitude was taken first. d3 takes it first, which
/// makes every integer type round halves away from zero.
#[test]
fn integer_types_round_the_magnitude_not_the_signed_value() {
    // Rounding the signed value would give −2, −1 and −0 for these three.
    assert_eq!(fmt("d", -2.5), "\u{2212}3");
    assert_eq!(fmt("d", -1.5), "\u{2212}2");
    assert_eq!(fmt("d", -0.5), "\u{2212}1");
    assert_eq!(fmt("d", 2.5), "3");
    assert_eq!(fmt("d", 1.5), "2");
    assert_eq!(fmt("d", 0.5), "1");

    // The radix types round through the same helper, so they inherit the ordering.
    assert_eq!(fmt("b", -2.5), "\u{2212}11");
    assert_eq!(fmt("o", -2.5), "\u{2212}3");
    assert_eq!(fmt("x", -2.5), "\u{2212}3");
    assert_eq!(fmt("X", -2.5), "\u{2212}3");
}

/// Section 4.4: the negative-zero and negative-NaN sign policy.
///
/// `valueNegative = value < 0 || 1 / value < 0`. The second test is the only thing
/// that sees negative zero, and *neither* test sees a NaN's sign bit, because every
/// comparison against a NaN is false. So `-0` is negative and `-NaN` is not.
#[test]
fn negative_zero_is_negative_and_a_negative_nan_is_not() {
    // Negative zero formats as zero, so the sign is then suppressed -- except under
    // sign `+`, which the suppression rule exempts.
    assert_eq!(fmt("f", -0.0), "0.000000");
    assert_eq!(fmt("(f", -0.0), "0.000000");
    assert_eq!(fmt("+f", -0.0), "\u{2212}0.000000");
    assert_eq!(fmt("+.2s", -0.0), "\u{2212}0.0");
    assert_eq!(fmt("d", -0.0), "0");
    assert_eq!(fmt("x", -0.0), "0");
    assert_eq!(fmt("$,.2f", -0.0), "$0.00");

    // Both NaN sign bits, through all three sign modes. A `+` prepends its plus
    // because the value is not negative, and `(` never wraps a NaN in parentheses.
    for nan in [f64::NAN, -f64::NAN] {
        let bits = nan.to_bits();
        assert_eq!(fmt("f", nan), "NaN", "{bits:016X}");
        assert_eq!(fmt("+f", nan), "+NaN", "{bits:016X}");
        assert_eq!(fmt("(f", nan), "NaN", "{bits:016X}");
    }
    // The sign bit really did differ, so the loop above compared two values.
    assert!(!f64::NAN.is_sign_negative());
    assert!((-f64::NAN).is_sign_negative());
}

/// Section 4.4: formatted-string zero suppression.
///
/// `if (valueNegative && +value === 0 && sign !== "+") valueNegative = false`. The
/// subject is the string the conversion produced, not the value, so the same input
/// keeps or loses its sign depending on the precision it was rendered at.
#[test]
fn a_value_that_rendered_as_zero_loses_its_sign() {
    // The same input at two precisions: rounded away at 2, still there at 6.
    assert_eq!(fmt(".2f", -0.0001), "0.00");
    assert_eq!(fmt(".6f", -0.0001), "\u{2212}0.000100");

    // Sign `+` is exempt; sign `(` is not, so it also drops its parentheses.
    assert_eq!(fmt("+.2f", -0.0001), "\u{2212}0.00");
    assert_eq!(fmt("(.2f", -0.0001), "0.00");

    // The test parses the string, so an exponent or a suffix that keeps it non-zero
    // keeps the sign, and the affixes -- added after -- do not enter into it.
    assert_eq!(fmt(".2e", -0.0001), "\u{2212}1.00e-4");
    assert_eq!(fmt(".1s", -1e-7), "\u{2212}100n");
    assert_eq!(fmt("d", -0.4), "0");
    assert_eq!(fmt("$.2f", -0.001), "$0.00");
    assert_eq!(fmt(".2%", -0.000_01), "0.00%");
}

/// Section 4.4: width is measured in UTF-16 code units.
///
/// Neither bytes nor `char`s. An astral character is one `char`, four bytes and two
/// code units, so the three measures disagree by two on a single emoji -- and the
/// locale's own text is measured the same way, since d3 counts the assembled
/// prefixes and suffixes with `String.prototype.length` too.
#[test]
fn width_counts_utf16_code_units() {
    let locale = Locale::en_us();
    let pad = |spec: &str, text: &str| {
        locale
            .formatter(spec)
            .expect("valid")
            .format_text(text)
            .expect("valid")
    };

    // U+1F600 is two code units, so eight fill characters, not nine.
    assert_eq!(pad(">10c", "\u{1f600}"), "        \u{1f600}");
    assert_eq!(pad("<10c", "\u{1f600}"), "\u{1f600}        ");
    assert_eq!(pad("^10c", "\u{1f600}"), "    \u{1f600}    ");
    // A combining mark is a code unit of its own, so "e" plus U+0301 is two.
    assert_eq!(pad(">10c", "e\u{301}"), "        e\u{301}");

    // The locale's affixes are measured too, and an astral one costs two.
    let astral = |field: Locale, spec: &str, value: f64| {
        field.formatter(spec).expect("valid").format_number(value)
    };
    let minus = Locale::builder()
        .decimal(".")
        .minus("\u{1f600}")
        .build()
        .expect("valid");
    assert_eq!(
        astral(minus, "8.2f", -1.0).expect("valid"),
        "  \u{1f600}1.00"
    );
    let nan = Locale::builder()
        .decimal(".")
        .nan("\u{1f600}")
        .build()
        .expect("valid");
    assert_eq!(
        astral(nan, "8.2f", f64::NAN).expect("valid"),
        "      \u{1f600}"
    );
    let currency = Locale::builder()
        .decimal(".")
        .currency("\u{1f600}", "")
        .build()
        .expect("valid");
    assert_eq!(
        astral(currency, "$8.2f", 1.0).expect("valid"),
        "  \u{1f600}1.00"
    );
}

/// Section 4.4: trim before grouping.
///
/// `formatTrim` runs on the converted value before `group` sees it, so the separators
/// are placed in the shortened digits. Grouping first would leave a separator in a
/// position the trimmed string no longer has.
#[test]
fn trimming_happens_before_grouping() {
    assert_eq!(fmt("$,~f", 1234.5), "$1,234.5");
    assert_eq!(fmt(",~g", 1_234_567.0), "1.23457e+6");
    // The trim reaches only the insignificant tail; a fraction that survives it is
    // grouped around, never through.
    assert_eq!(fmt(",.10~f", 1_234_567.1), "1,234,567.1000000001");
}

/// Section 4.4: numeral substitution is last.
///
/// `formatNumerals` runs over the whole assembled string -- fill, sign, separators,
/// affixes and digits alike -- so it reaches an ASCII digit anywhere in the result,
/// including one that is not part of the number.
#[test]
fn numerals_are_substituted_over_the_assembled_string() {
    let arabic = arabic();
    let fmt = |spec: &str, value: f64| {
        arabic
            .formatter(spec)
            .expect("valid")
            .format_number(value)
            .expect("valid")
    };

    // The `0` fill is substituted, which only happens because the pass runs after
    // padding: the fill character itself is an ASCII digit here.
    assert_eq!(
        fmt("0>12,.2f", -1234.56),
        "\u{660}\u{660}\u{660}\u{2212}\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66b}\u{665}\u{666}"
    );
    // Zero-fill grouping, where the padding is grouped in with the digits and then
    // every one of them is substituted.
    assert_eq!(
        fmt("020,.2f", 1234.56),
        "\u{660}\u{66c}\u{660}\u{660}\u{660}\u{66c}\u{660}\u{660}\u{660}\u{66c}\
         \u{660}\u{660}\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66b}\u{665}\u{666}"
    );
    // The SI suffix is not a digit and survives; the digits before it do not.
    assert_eq!(fmt(".3s", 1.3e6), "\u{661}\u{66b}\u{663}\u{660}M");
    // And the sharpest case: the `0` of the `0x` base prefix is substituted while
    // the hexadecimal digits, which are not ASCII `0`-`9`, are left alone.
    assert_eq!(fmt("#x", 48879.0), "\u{660}xbeef");
}

/// Section 4.4: the `0x` prefix stays lowercase for type `X`.
///
/// d3 uppercases the digits with `toUpperCase()` and adds the prefix afterwards, so
/// `#X` really does produce a lowercase `x` in front of uppercase digits.
#[test]
fn the_base_prefix_is_lowercase_even_for_uppercase_hex() {
    assert_eq!(fmt("#X", 48879.0), "0xBEEF");
    assert_eq!(fmt("#x", 48879.0), "0xbeef");
    assert_eq!(fmt("#X", 0.0), "0x0");
    assert_eq!(fmt("#08X", 48879.0), "0x00BEEF");
    // The prefix is not grouped, which is the other half of it being a prefix.
    assert_eq!(fmt("#,X", 1e9), "0x3B,9AC,A00");
    // The other two bases, for contrast: their letters were never uppercase.
    assert_eq!(fmt("#b", 5.0), "0b101");
    assert_eq!(fmt("#o", 8.0), "0o10");
}

/// Section 4.4: type `d` renders an infinity as `∞`.
///
/// `formatDecimal` returns `undefined` for a non-finite value and d3's `format`
/// answers `"∞"`, where every other type reports the ECMAScript spelling
/// `"Infinity"`. The magnitude is taken first, so the sign is the locale's minus.
#[test]
fn type_d_spells_infinity_with_the_symbol() {
    assert_eq!(fmt("d", f64::INFINITY), "\u{221e}");
    assert_eq!(fmt("d", f64::NEG_INFINITY), "\u{2212}\u{221e}");
    assert_eq!(fmt("+d", f64::INFINITY), "+\u{221e}");
    // One code unit, so it pads to nine and not to three.
    assert_eq!(fmt("10d", f64::INFINITY), "         \u{221e}");

    // Every other type keeps `Infinity`, including the radix types, whose conversion
    // also goes through `Number.prototype.toString`.
    for spec in ["f", "e", "s", "x", "b", "o", "g", "r"] {
        assert_eq!(fmt(spec, f64::INFINITY), "Infinity", "{spec:?}");
    }
    assert_eq!(fmt("%", f64::INFINITY), "Infinity%");

    // A finite value type `d` can still print in full, which is the boundary the
    // `1e21` switch in section 4.2 sits on.
    assert_eq!(fmt(",d", 1e21), "1,000,000,000,000,000,000,000");
}

/// Section 4.1: a `formatPrefix` reference with no decimal exponent.
///
/// `si_prefix_exponent` is `undefined` for zero, NaN and the infinities, so d3's
/// scale is `Math.pow(10, -undefined)`, which is NaN, and its suffix is
/// `prefixes[undefined]`, which is absent. Every value then formats as the locale's
/// NaN with no SI suffix. Nothing here may index the prefix table with a NaN.
#[test]
fn a_degenerate_prefix_reference_formats_everything_as_nan() {
    for reference in [
        0.0,
        -0.0,
        f64::NAN,
        -f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let bits = reference.to_bits();
        for value in [1e6, -1e6, 0.0, f64::NAN, f64::INFINITY] {
            assert_eq!(prefixed(",.0", reference, value), "NaN", "{bits:016X}");
        }
    }

    // The type is forced to `f` even for a reference that cannot be bucketed, so the
    // formatter is a well-formed numeric one that simply always answers NaN. A port
    // that fell back to type `s` here would start choosing a bucket per value.
    let degenerate = Locale::en_us().prefix_formatter("s", 0.0).expect("valid");
    assert_eq!(degenerate.specifier().format_type(), FormatType::Fixed);
}

/// Section 4.1: a negative `formatPrefix` reference buckets by magnitude.
///
/// `si_prefix_exponent` takes `Math.abs(reference)` first, so `-1e6` and `1e6` pick
/// the same prefix. The scale is a bit-pinned `Math.pow(10, -e)` and is applied by
/// multiplication, never by dividing by `10^e`.
#[test]
fn a_negative_prefix_reference_selects_its_bucket_from_the_magnitude() {
    for reference in [1e6, -1e6] {
        assert_eq!(prefixed(",.2", reference, 1e6), "1.00M", "{reference}");
        assert_eq!(
            prefixed(",.2", reference, -1e6),
            "\u{2212}1.00M",
            "{reference}"
        );
        // The suffix is fixed by the reference, so even zero carries it.
        assert_eq!(prefixed(",.2", reference, 0.0), "0.00M", "{reference}");
    }
    for reference in [1e-6, -1e-6] {
        assert_eq!(
            prefixed(",.2", reference, 1e6),
            "1,000,000,000,000.00\u{b5}",
            "{reference}"
        );
        assert_eq!(prefixed(",.2", reference, 0.0), "0.00\u{b5}", "{reference}");
    }
    // Exponent zero: a real bucket with an empty suffix, not an absent one.
    assert_eq!(prefixed(",.2", -1.0, 1e6), "1,000,000.00");
    assert_eq!(prefixed(".3", 1e-6, 0.000_042), "42.000\u{b5}");
    assert_eq!(prefixed("$,.2", 1e6, 1.3e6), "$1.30M");
}
