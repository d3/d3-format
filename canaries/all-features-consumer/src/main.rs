//! Downstream canary: the packaged crate with every feature on, plus the
//! locale-heavy workload from section 7's Phase 6 canary list.
//!
//! Two things are being checked at once. That the `locales` and `serde` features
//! work from outside the crate -- the generated table is only reachable through the
//! feature, and the derives only exist under it -- and that the three ways to obtain
//! a locale agree byte for byte:
//!
//! 1. `Locale::named`, straight from the generated table;
//! 2. `Locale::from_definition`, from a `LocaleDefinition` assembled by hand out of
//!    that table's fields;
//! 3. the same definition after a JSON round trip.
//!
//! Anything that is not `Locale::named` is what a downstream application actually
//! does when it ships its own locale data, so (2) and (3) are the interesting ones.
//! `serde` claims `LocaleDefinition` is the stable wire shape; (3) is that claim.

use std::time::Instant;

use d3_format::locales::{self, GeneratedLocale};
use d3_format::{Locale, LocaleDefinition};

/// Every specifier the locale sweep drives, chosen so that each piece of a locale
/// definition is observable in at least one of them: the decimal point, the group
/// separator and grouping cycle, both currency affixes, the percent sign, the minus
/// sign, the NaN text, and the numerals.
const SWEEP_SPECIFIERS: [&str; 14] = [
    ",d", "$,.2f", "$,.0f", ",.2f", ".3f", ".1%", ",.1%", ".2p", ".3s", ",.4g", "+.3e", "(,.2f",
    "#x", "012,.2f",
];

/// Values for the sweep. The tail is deliberately awkward: both zeros, NaN, both
/// infinities, and the magnitude at which type `d` changes engine code path.
const SWEEP_VALUES: [f64; 14] = [
    0.0,
    1.0,
    -1.0,
    1234.5,
    -1234.5,
    1234567890.0,
    0.1234,
    1.3e-6,
    1.3e12,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    1e21,
    -0.0,
];

/// Oracle answers for a spread of locales, so that the sweep's agreement between
/// three construction paths is anchored to d3 rather than only to itself.
const ANCHORS: [(&str, &str, f64, &str); 13] = [
    // Grouping cycles rather than repeating, which is the whole of the Indian case.
    ("en-IN", ",d", 1234567890.0, "1,23,45,67,890"),
    // Numeral substitution runs last, over the assembled string: the `0` of the `0x`
    // prefix is substituted and the hexadecimal digits are not.
    ("ar-001", "#x", 48879.0, "\u{660}xbeef"),
    (
        "ar-EG",
        ",d",
        1234567.0,
        "\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66c}\u{665}\u{666}\u{667}",
    ),
    ("de-DE", "$,.2f", 1234.5, "1.234,50\u{a0}\u{20ac}"),
    ("fr-FR", ",.2f", -1234.5, "\u{2212}1\u{a0}234,50"),
    ("ru-RU", "$,.2f", 1234.5, "1\u{a0}234,50\u{a0}\u{20bd}"),
    ("ja-JP", "$,.0f", 1234567.0, "1,234,567\u{5186}"),
    ("he-IL", "$,.2f", 1234.5, "\u{20aa}1,234.50"),
    ("nl-NL", "$,.2f", -1234.5, "\u{2212}\u{20ac}\u{a0}1.234,50"),
    ("ca-ES", ",.3s", 1.3e6, "1,30M"),
    ("zh-CN", "$,.2f", 1234.5, "\u{a5}1,234.50"),
    ("cs-CZ", ",.1%", 0.1234, "12,3%"),
    (
        "uk-UA",
        "$,.2f",
        -1234.5,
        "\u{2212}1\u{a0}234,50\u{a0}\u{20b4}.",
    ),
];

/// d3 ships 59 locale files and 23 of them define numerals.
const EXPECTED_LOCALES: usize = 59;
const EXPECTED_NUMERAL_LOCALES: usize = 23;

fn main() {
    let started = Instant::now();

    let anchored = anchored_outputs();
    let swept = locale_sweep();
    let wire = wire_shape();

    println!(
        "canary all-features-consumer: OK -- {anchored} anchored, {swept} swept, \
         {wire} wire-shape, {} value(s) total in {:.2}s",
        anchored + swept + wire,
        started.elapsed().as_secs_f64()
    );
}

/// The table itself, and the anchors that tie it to the oracle.
fn anchored_outputs() -> usize {
    assert_eq!(locales::LOCALE_COUNT, EXPECTED_LOCALES);
    assert_eq!(locales::LOCALES.len(), EXPECTED_LOCALES);
    assert_eq!(locales::names().count(), EXPECTED_LOCALES);

    // `names()` and `by_name` are two views of one table and must not disagree.
    for entry in &locales::LOCALES {
        let looked_up = locales::by_name(entry.name).expect("a name from the table resolves");
        assert_eq!(looked_up, entry, "by_name disagreed for {}", entry.name);
        assert!(
            Locale::named(entry.name).is_some(),
            "{} has no locale",
            entry.name
        );
    }
    assert!(locales::by_name("xx-XX").is_none());
    assert!(Locale::named("xx-XX").is_none());

    for (name, specifier, value, expected) in ANCHORS {
        let locale = Locale::named(name).expect("a bundled locale");
        let formatter = locale.formatter(specifier).expect("a valid specifier");
        assert_eq!(
            formatter.format_number(value).expect("in range"),
            expected,
            "{name} `{specifier}` of {value}"
        );
    }

    ANCHORS.len()
}

/// Section 7's locale-heavy workload.
///
/// Every bundled locale, every sweep specifier, every sweep value, through all three
/// construction paths, twice. Formatting is a pure function of the formatter and the
/// value, so the second pass must reproduce the first exactly; and a definition that
/// survived a JSON round trip must format identically to the table entry it came
/// from.
fn locale_sweep() -> usize {
    let mut formatted = 0;
    let mut with_numerals = 0;

    for entry in &locales::LOCALES {
        let definition = definition_of(entry);

        let named = Locale::named(entry.name).expect("a bundled locale");
        let built = Locale::from_definition(definition.clone()).expect("a valid definition");

        let json = serde_json::to_string(&definition).expect("a definition serializes");
        let parsed: LocaleDefinition =
            serde_json::from_str(&json).expect("a definition deserializes");
        assert_eq!(
            parsed, definition,
            "{} did not survive the JSON round trip",
            entry.name
        );
        let restored = Locale::from_definition(parsed).expect("a valid definition");

        // The accessors are part of the public surface too, and they are the only way
        // a caller can see that the three locales really are the same locale.
        for other in [&built, &restored] {
            assert_eq!(other.decimal(), named.decimal(), "{}", entry.name);
            assert_eq!(other.thousands(), named.thousands(), "{}", entry.name);
            assert_eq!(other.grouping(), named.grouping(), "{}", entry.name);
            assert_eq!(
                other.currency_prefix(),
                named.currency_prefix(),
                "{}",
                entry.name
            );
            assert_eq!(
                other.currency_suffix(),
                named.currency_suffix(),
                "{}",
                entry.name
            );
            assert_eq!(other.numerals(), named.numerals(), "{}", entry.name);
            assert_eq!(other.percent(), named.percent(), "{}", entry.name);
            assert_eq!(other.minus(), named.minus(), "{}", entry.name);
            assert_eq!(other.nan(), named.nan(), "{}", entry.name);
        }

        for specifier in SWEEP_SPECIFIERS {
            let reference = named.formatter(specifier).expect("a valid specifier");
            let from_built = built.formatter(specifier).expect("a valid specifier");
            let from_restored = restored.formatter(specifier).expect("a valid specifier");

            for value in SWEEP_VALUES {
                let expected = reference.format_number(value).expect("in range");

                assert_eq!(
                    reference.format_number(value).expect("in range"),
                    expected,
                    "{} `{specifier}` of {value} is not deterministic",
                    entry.name
                );
                assert_eq!(
                    from_built.format_number(value).expect("in range"),
                    expected,
                    "{} `{specifier}` of {value} differs when built from a definition",
                    entry.name
                );
                assert_eq!(
                    from_restored.format_number(value).expect("in range"),
                    expected,
                    "{} `{specifier}` of {value} differs after a JSON round trip",
                    entry.name
                );

                formatted += 4;
            }
        }

        // Numerals replace the ASCII digits over the whole assembled string, so a
        // locale that defines them must not leave an ASCII digit in a plain integer.
        // `,d` has no base prefix, which is the one place a digit legitimately
        // survives -- see the ar-001 `#x` anchor above.
        if entry.numerals.is_some() {
            with_numerals += 1;
            let integer = named.formatter(",d").expect("a valid specifier");
            let rendered = integer.format_number(1234567.0).expect("in range");
            assert!(
                !rendered.contains(|c: char| c.is_ascii_digit()),
                "{} defines numerals but `,d` produced {rendered:?}",
                entry.name
            );
        }
    }

    assert_eq!(with_numerals, EXPECTED_NUMERAL_LOCALES);

    formatted
}

/// The `serde` feature's own claim: absent keys stay absent.
///
/// A definition that omitted `numerals` must not come back carrying `"numerals":
/// null`, because feeding that to d3 would coerce to the four-character string
/// "null" rather than falling back to d3's default.
fn wire_shape() -> usize {
    let sparse = LocaleDefinition {
        decimal: Some(",".to_owned()),
        ..LocaleDefinition::default()
    };
    let json = serde_json::to_string(&sparse).expect("a definition serializes");
    assert_eq!(json, r#"{"decimal":","}"#);
    assert_eq!(
        serde_json::from_str::<LocaleDefinition>(&json).expect("a definition deserializes"),
        sparse
    );

    // And an empty document is the all-defaults definition rather than an error, so a
    // consumer's partial locale JSON loads.
    let empty: LocaleDefinition = serde_json::from_str("{}").expect("an empty definition loads");
    assert_eq!(empty, LocaleDefinition::default());
    let defaults = Locale::from_definition(empty).expect("a valid definition");
    assert_eq!(defaults.decimal(), ".");
    assert_eq!(defaults.minus(), "\u{2212}");
    assert_eq!(defaults.nan(), "NaN");
    assert_eq!(defaults.grouping(), None);

    // The one definition shape that is refused, and that d3 accepts into an infinite
    // loop. See `validated-locale-shape` in DIVERGENCES.md.
    let zero_group = LocaleDefinition {
        thousands: Some(",".to_owned()),
        grouping: Some(vec![3, 0]),
        ..LocaleDefinition::default()
    };
    assert!(Locale::from_definition(zero_group).is_err());

    3
}

/// The generated table's borrowed shape, widened into the owned `LocaleDefinition`
/// a downstream consumer would deserialize.
fn definition_of(entry: &GeneratedLocale) -> LocaleDefinition {
    LocaleDefinition {
        decimal: entry.decimal.map(str::to_owned),
        thousands: entry.thousands.map(str::to_owned),
        grouping: entry.grouping.map(|sizes| sizes.to_vec()),
        currency: entry.currency.map(|affixes| affixes.map(str::to_owned)),
        numerals: entry.numerals.map(|numerals| numerals.map(str::to_owned)),
        percent: entry.percent.map(str::to_owned),
        minus: entry.minus.map(str::to_owned),
        nan: entry.nan.map(str::to_owned),
    }
}
