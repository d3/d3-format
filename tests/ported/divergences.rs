//! Oracle assertions whose JavaScript semantics the port deliberately does not
//! reproduce.
//!
//! Each test here is named by an entry in `DIVERGENCES.md` and by the
//! `divergence` field of the oracle's assertion map. None of them may be declared
//! "passed" by some other Rust call: the point is to assert the replacement
//! contract, not to pretend the JavaScript behaviour survived.

use d3_format::{
    Align, FormatError, FormatSpecifier, FormatType, InputKind, Locale, ParseErrorKind, Sign,
    Symbol,
};

/// `format === locale.format` and `formatPrefix === locale.formatPrefix`, from
/// `test/defaultLocale-test.js`.
///
/// Divergence `no-mutable-default-locale`. The oracle asserts that calling
/// `formatDefaultLocale` rebinds two module-level exports. Rust has no mutable
/// process-global default locale, so there is no binding whose identity could
/// change. The replacement contract is that `Locale::en_us()` is a value, that
/// building another locale leaves it untouched, and that both are `Send + Sync`.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn default_locale_is_not_a_mutable_process_global() {
    // `frFr` from `test/defaultLocale-test.js`, which the oracle installs globally.
    let french = Locale::builder()
        .decimal(",")
        .thousands(".")
        .grouping([3])
        .currency("", "\u{a0}\u{20ac}")
        .percent("\u{202f}%")
        .build()
        .expect("valid");

    // Installing it is not a thing that can be done: the only way to use it is to
    // hold it, and holding it changes nothing else.
    let before = Locale::en_us();
    let after = Locale::en_us();
    assert_eq!(before, after);
    assert_eq!(after.decimal(), ".");
    assert_eq!(after.thousands(), ",");
    assert_eq!(after.currency_prefix(), "$");
    assert_eq!(after.currency_suffix(), "");
    assert_ne!(after, french);

    // A formatter is obtained from the locale it belongs to, so two formatters for
    // the same specifier and different locales are simply different values. In
    // JavaScript these would be the same `format` binding at different times.
    let american = before.formatter("$,.2f").expect("valid");
    let parisian = french.formatter("$,.2f").expect("valid");
    assert_ne!(american, parisian);
    assert_eq!(american.specifier(), parisian.specifier());

    // And they are thread-safe values rather than process state. The compile-time
    // bound lives in `tests/public_api.rs`; this is the observable half.
    let moved = std::thread::spawn(move || parisian.specifier().to_string())
        .join()
        .expect("no panic");
    assert_eq!(moved, " >-$,.2f");
}

/// `formatSpecifier(specifier).toString() reflects current field values` and the two
/// clamping blocks, from `test/formatSpecifier-test.js`.
///
/// Divergence `immutable-typed-specifier`. The oracle assigns to public fields
/// between assertions and re-reads `toString()`, and relies on `Math.max(0, p | 0)`
/// and `Math.max(1, w | 0)` to clamp negatives. A `FormatSpecifier` is immutable
/// after validated construction and its width and precision are unsigned, so a
/// negative value is unrepresentable rather than clamped. The replacement contract
/// is that the builder reaches each of the same normalized spellings
/// (`"_>-"` through `"_^+$012,.2~f"`) directly.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn format_specifier_is_immutable_after_construction() {
    // The oracle's ten assignments, in order, each one a new value built from the
    // previous. Every intermediate stays alive and unchanged, which is the whole
    // difference from the JavaScript sequence.
    let start: FormatSpecifier = "".parse().expect("valid");
    let steps: [(FormatSpecifier, &str); 10] = {
        let fill = start.to_builder().fill('_').build().expect("valid");
        let align = fill
            .to_builder()
            .align(Align::Center)
            .build()
            .expect("valid");
        let sign = align.to_builder().sign(Sign::Plus).build().expect("valid");
        let symbol = sign
            .to_builder()
            .symbol(Symbol::Currency)
            .build()
            .expect("valid");
        let zero = symbol.to_builder().zero(true).build().expect("valid");
        let width = zero.to_builder().width(12).build().expect("valid");
        let comma = width.to_builder().comma(true).build().expect("valid");
        let precision = comma.to_builder().precision(2).build().expect("valid");
        let typed = precision
            .to_builder()
            .format_type(FormatType::Fixed)
            .build()
            .expect("valid");
        let trimmed = typed.to_builder().trim(true).build().expect("valid");
        [
            (fill, "_>-"),
            (align, "_^-"),
            (sign, "_^+"),
            (symbol, "_^+$"),
            (zero, "_^+$0"),
            (width, "_^+$012"),
            (comma, "_^+$012,"),
            (precision, "_^+$012,.2"),
            (typed, "_^+$012,.2f"),
            (trimmed, "_^+$012,.2~f"),
        ]
    };
    for (specifier, expected) in &steps {
        assert_eq!(&specifier.to_string(), expected);
    }

    // Nothing was mutated: the value each step was built from still renders as it
    // did. In JavaScript there is only ever one object, so this cannot be asked.
    assert_eq!(start.to_string(), " >-");
    for (index, (specifier, expected)) in steps.iter().enumerate() {
        assert_eq!(
            &specifier.to_string(),
            expected,
            "step {index} changed after later steps were built"
        );
    }

    // `toString() clamps precision to zero` and `clamps width to one`. d3 needs the
    // clamps because `specifier.precision = -1` and `specifier.width = -1` are
    // writable; here the fields are `Option<u32>`, so a negative one cannot be
    // written and the clamp target is simply the smallest representable value.
    assert_eq!(
        FormatSpecifier::builder()
            .precision(0)
            .build()
            .expect("valid")
            .to_string(),
        " >-.0"
    );
    assert_eq!(
        FormatSpecifier::builder()
            .width(0)
            .build()
            .expect("valid")
            .to_string(),
        " >-1"
    );
}

/// `new FormatSpecifier({...}) coerces all inputs to the expected types`, from
/// `test/formatSpecifier-test.js`.
///
/// Divergence `no-specifier-field-coercion`. The oracle passes numbers where d3
/// expects strings and booleans and relies on `String(v)`, `!!v` and ToInt32. The
/// Rust builder takes typed values, so `fill: 1` and `type: 10` have no equivalent
/// at all. The replacement contract is that construction is typed and that invalid
/// field values are rejected rather than coerced.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn specifier_construction_does_not_coerce_arbitrary_javascript_values() {
    // The oracle's object is `{fill: 1, align: 2, sign: 3, symbol: 4, zero: 5,
    // width: 6, comma: 7, precision: 8, trim: 9, type: 10}`, and d3 turns it into
    // fill `"1"`, align `"2"`, sign `"3"`, symbol `"4"`, zero `true`, width `6`,
    // comma `true`, precision `8`, trim `true`, type `"10"`.
    //
    // Four of those coerced values name nothing in the grammar. The typed
    // constructors refuse them, which is what "rejected rather than coerced" means.
    assert_eq!(Align::from_char('2'), None);
    assert_eq!(Sign::from_char('3'), None);
    assert_eq!(Symbol::from_char('4'), None);
    assert_eq!(FormatType::from_char('1'), None);
    assert_eq!(FormatType::from_char('0'), None);

    // A type of `"10"` is two characters; `FormatType` holds one, so the coerced
    // value is not merely rejected, it is unrepresentable. The nearest thing that
    // can be written is refused by the builder.
    let error = FormatSpecifier::builder()
        .format_type(FormatType::Unknown('1'))
        .build()
        .unwrap_err();
    assert_eq!(error.kind(), ParseErrorKind::Syntax);

    // The fields d3 coerces through truthiness take `bool`, and the two it coerces
    // through ToInt32 take `u32`, so `zero: 5` and `width: -1` have no spelling.
    // What can be written is the typed equivalent, and it renders as d3's result
    // for the parts of the object d3 does not mangle.
    let typed = FormatSpecifier::builder()
        .fill('1')
        .zero(true)
        .width(6)
        .comma(true)
        .precision(8)
        .trim(true)
        .build()
        .expect("valid");
    assert_eq!(typed.fill(), '1');
    assert!(typed.zero());
    assert_eq!(typed.width(), Some(6));
    assert!(typed.comma());
    assert_eq!(typed.precision(), Some(8));
    assert!(typed.trim());

    // And the whole coerced specifier is not a specifier: d3 would render
    // `"1234061,.8~10"`, which is not in the grammar it just came out of.
    assert!("1234061,.8~10".parse::<FormatSpecifier>().is_err());
}

/// `formatLocale({nan: nan}) observes the specified not-a-number representation`,
/// from `test/locale-test.js`.
///
/// Divergence `no-input-coercion`. The oracle passes `undefined` to a numeric
/// formatter and relies on `+undefined` being NaN. Rust's `format_number` takes an
/// `f64`, and `format_text` on a numeric specifier fails with
/// `FormatError::InputTypeMismatch`. The replacement contract has two halves: the
/// NaN outputs `"   N/A"`, `"-     "` and `"   NaN"` must be produced for
/// `f64::NAN`, and non-numeric input must be rejected rather than coerced.
///
/// This is also where the three assertions of that oracle block are checked, since
/// the generated block is `#[ignore]`d for taking `undefined`: what d3 computes from
/// `+undefined` is what the port computes from `f64::NAN`, and the expected strings
/// are the oracle's own.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn numeric_formatters_reject_non_numeric_input() {
    // The three formatters the oracle builds, each handed something that is not a
    // number. In JavaScript each call succeeds by coercion; here each is a typed
    // refusal naming both kinds.
    let cases = [
        (
            Locale::builder().nan("N/A").build().expect("valid"),
            "6.2f",
            "   N/A",
        ),
        (
            Locale::builder().nan("-").build().expect("valid"),
            "<6.2g",
            "-     ",
        ),
        (Locale::builder().build().expect("valid"), " 6.2f", "   NaN"),
    ];
    for (locale, spec, expected) in &cases {
        let formatter = locale.formatter(spec).expect("valid");
        assert_eq!(formatter.input_kind(), InputKind::Number);
        assert_eq!(
            formatter.format_text("undefined").unwrap_err(),
            FormatError::InputTypeMismatch {
                expected: InputKind::Number,
                actual: InputKind::Text,
            },
            "{spec:?}"
        );

        // The replacement for the coercion: the value `+undefined` produces, passed
        // as itself. The oracle's expected strings are unchanged.
        assert_eq!(formatter.format_number(f64::NAN).expect("valid"), *expected);
    }

    // The converse, which d3 also coerces: a `c` formatter given a number.
    let character = Locale::en_us().formatter("c").expect("valid");
    assert_eq!(
        character.format_number(f64::NAN).unwrap_err(),
        FormatError::InputTypeMismatch {
            expected: InputKind::Text,
            actual: InputKind::Number,
        }
    );
}

/// Every assertion in this module, for the runner in the target root.
pub(crate) fn all() {
    default_locale_is_not_a_mutable_process_global();
    format_specifier_is_immutable_after_construction();
    specifier_construction_does_not_coerce_arbitrary_javascript_values();
    numeric_formatters_reject_non_numeric_input();
}
