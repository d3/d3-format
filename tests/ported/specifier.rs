//! Specifier assertions that read parsed state rather than formatting a value.
//!
//! Sources: `test/format-test.js` and `test/formatSpecifier-test.js`.
//!
//! Every test here is implemented and passes. They stay `#[ignore]`d for the reason
//! the target root explains: this target's inventory is derived from the oracle's
//! assertion map, which does not move as the port advances. The non-ignored runner
//! in `main.rs` is what actually executes them, so nothing here is dead and nothing
//! claims coverage it has not demonstrated.

use d3_format::{Align, FormatSpecifier, FormatType, Locale, ParseErrorKind, Sign, Symbol};

/// `format("d") + "" == " >-d"`, from `test/format-test.js`.
///
/// The oracle reads the compiled formatter's normalized specifier. In Rust that is
/// `Formatter::specifier()` rendered through `Display for FormatSpecifier`, which
/// section 5.2 requires to emit the canonical d3 spelling.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn formatter_reports_its_normalized_specifier() {
    let formatter = Locale::en_us().formatter("d").expect("\"d\" is valid");
    assert_eq!(formatter.specifier().to_string(), " >-d");
}

/// The six `assert.throws(..., /invalid format: .../)` sites in `test/format-test.js`
/// and `test/formatSpecifier-test.js`.
///
/// Section 5.2 pins `ParseError`'s `Display` text to exactly `invalid format: {input}`,
/// so the inputs `"foo"`, `".-2s"` and `".f"` must be rejected with that message from
/// both `FormatSpecifier::from_str` and `Locale::formatter`.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn invalid_specifiers_are_rejected_with_the_d3_message() {
    let locale = Locale::en_us();
    for input in ["foo", ".-2s", ".f"] {
        let expected = format!("invalid format: {input}");

        // `formatSpecifier(input)` throws.
        let parse = input.parse::<FormatSpecifier>().unwrap_err();
        assert_eq!(parse.to_string(), expected);
        assert_eq!(parse.kind(), ParseErrorKind::Syntax);

        // `format(input)` throws the same thing, because it is the same error
        // reported through `FormatError::InvalidSpecifier`, whose `Display` is
        // transparent.
        let compile = locale.formatter(input).unwrap_err();
        assert_eq!(compile.to_string(), expected);
    }
}

/// `s instanceof formatSpecifier`, from `test/formatSpecifier-test.js`.
///
/// A divergence: JavaScript checks prototype identity between the factory function
/// and its instances. Rust parsing simply yields a `FormatSpecifier` value, so the
/// only thing left to assert is that the parse succeeds and round-trips.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn parsing_yields_a_concrete_format_specifier_type() {
    let specifier: FormatSpecifier = "".parse().expect("the empty specifier is valid");
    assert_eq!(specifier, FormatSpecifier::default());
    assert_eq!(specifier.to_string(), " >-");

    let round_tripped: FormatSpecifier = specifier
        .to_string()
        .parse()
        .expect("the canonical spelling is valid");
    assert_eq!(round_tripped, specifier);
}

/// `formatSpecifier("")` and `formatSpecifier(specifier) preserves shorthand`,
/// from `test/formatSpecifier-test.js`.
///
/// Defaults: fill `" "`, align `">"`, sign `"-"`, empty symbol, `zero` false, no
/// width, `comma` false, no precision, `trim` false, empty type. JavaScript's
/// `undefined` width and precision become `None`.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn empty_specifier_has_the_documented_defaults() {
    let specifier: FormatSpecifier = "".parse().expect("the empty specifier is valid");
    assert_eq!(specifier.fill(), ' ');
    assert_eq!(specifier.align(), Align::Right);
    assert_eq!(specifier.sign(), Sign::Minus);
    assert_eq!(specifier.symbol(), Symbol::None);
    assert!(!specifier.zero());
    assert_eq!(specifier.width(), None);
    assert!(!specifier.comma());
    assert_eq!(specifier.precision(), None);
    assert!(!specifier.trim());
    assert_eq!(specifier.format_type(), FormatType::None);
}

/// `formatSpecifier(specifier) preserves unknown types`, from
/// `test/formatSpecifier-test.js`.
///
/// Section 5.2 keeps unknown ASCII type letters stored, and section 4 has them
/// format through d3's `.12~g` fallback, so `"q"` must survive parsing as type `q`
/// with `trim` still false.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn unknown_type_letters_are_preserved() {
    let specifier: FormatSpecifier = "q".parse().expect("an unknown letter is still valid");
    assert!(!specifier.trim());
    assert_eq!(specifier.format_type(), FormatType::Unknown('q'));

    // The `.12~g` rewrite is the formatter's, not the specifier's: `format("q")`
    // still reports itself as `q`.
    let formatter = Locale::en_us().formatter("q").expect("valid");
    assert_eq!(
        formatter.specifier().format_type(),
        FormatType::Unknown('q')
    );
    assert_eq!(formatter.specifier().to_string(), " >-q");
}

/// `new FormatSpecifier({})`, from `test/formatSpecifier-test.js`.
///
/// The Rust builder starting from no fields must produce exactly the same
/// specifier as parsing the empty string.
#[test]
#[ignore = "runs in the target root; see the module documentation"]
pub(crate) fn builder_defaults_match_the_empty_specifier() {
    let built = FormatSpecifier::builder()
        .build()
        .expect("the defaults are valid");
    let parsed: FormatSpecifier = "".parse().expect("the empty specifier is valid");
    assert_eq!(built, parsed);

    // Field by field, so a change that broke both the same way would still show.
    assert_eq!(built.fill(), ' ');
    assert_eq!(built.align(), Align::Right);
    assert_eq!(built.sign(), Sign::Minus);
    assert_eq!(built.symbol(), Symbol::None);
    assert!(!built.zero());
    assert_eq!(built.width(), None);
    assert!(!built.comma());
    assert_eq!(built.precision(), None);
    assert!(!built.trim());
    assert_eq!(built.format_type(), FormatType::None);
}

/// Every assertion in this module, for the runner in the target root.
pub(crate) fn all() {
    formatter_reports_its_normalized_specifier();
    invalid_specifiers_are_rejected_with_the_d3_message();
    parsing_yields_a_concrete_format_specifier_type();
    empty_specifier_has_the_documented_defaults();
    unknown_type_letters_are_preserved();
    builder_defaults_match_the_empty_specifier();
}
