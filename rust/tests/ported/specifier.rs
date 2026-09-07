//! Specifier assertions that read parsed state rather than formatting a value.
//!
//! Sources: `test/format-test.js` and `test/formatSpecifier-test.js`.

const PHASE_3: &str = "Phase 3 placeholder: FormatSpecifier and its typed getters do not exist yet";

/// `format("d") + "" == " >-d"`, from `test/format-test.js`.
///
/// The oracle reads the compiled formatter's normalized specifier. In Rust that is
/// `Formatter::specifier()` rendered through `Display for FormatSpecifier`, which
/// section 5.2 requires to emit the canonical d3 spelling.
#[test]
#[ignore = "Phase 3 placeholder: Formatter and FormatSpecifier do not exist yet"]
fn formatter_reports_its_normalized_specifier() {
    crate::placeholder(PHASE_3, "en-US formatter for \"d\" to render as \" >-d\"");
}

/// The six `assert.throws(..., /invalid format: .../)` sites in `test/format-test.js`
/// and `test/formatSpecifier-test.js`.
///
/// Section 5.2 pins `ParseError`'s `Display` text to exactly `invalid format: {input}`,
/// so the inputs `"foo"`, `".-2s"` and `".f"` must be rejected with that message from
/// both `FormatSpecifier::from_str` and `Locale::formatter`.
#[test]
#[ignore = "Phase 3 placeholder: ParseError does not exist yet"]
fn invalid_specifiers_are_rejected_with_the_d3_message() {
    crate::placeholder(
        PHASE_3,
        "\"foo\", \".-2s\" and \".f\" to be rejected as `invalid format: ...`",
    );
}

/// `s instanceof formatSpecifier`, from `test/formatSpecifier-test.js`.
///
/// A divergence: JavaScript checks prototype identity between the factory function
/// and its instances. Rust parsing simply yields a `FormatSpecifier` value, so the
/// only thing left to assert is that the parse succeeds and round-trips.
#[test]
#[ignore = "Phase 3 placeholder: FormatSpecifier does not exist yet"]
fn parsing_yields_a_concrete_format_specifier_type() {
    crate::placeholder(
        PHASE_3,
        "\"\".parse::<FormatSpecifier>() to succeed and round-trip",
    );
}

/// `formatSpecifier("")` and `formatSpecifier(specifier) preserves shorthand`,
/// from `test/formatSpecifier-test.js`.
///
/// Defaults: fill `" "`, align `">"`, sign `"-"`, empty symbol, `zero` false, no
/// width, `comma` false, no precision, `trim` false, empty type. JavaScript's
/// `undefined` width and precision become `None`.
#[test]
#[ignore = "Phase 3 placeholder: FormatSpecifier's typed getters do not exist yet"]
fn empty_specifier_has_the_documented_defaults() {
    crate::placeholder(PHASE_3, "the empty specifier's ten field defaults");
}

/// `formatSpecifier(specifier) preserves unknown types`, from
/// `test/formatSpecifier-test.js`.
///
/// Section 5.2 keeps unknown ASCII type letters stored, and section 4 has them
/// format through d3's `.12~g` fallback, so `"q"` must survive parsing as type `q`
/// with `trim` still false.
#[test]
#[ignore = "Phase 3 placeholder: FormatSpecifier's typed getters do not exist yet"]
fn unknown_type_letters_are_preserved() {
    crate::placeholder(PHASE_3, "\"q\" to parse with type 'q' and trim false");
}

/// `new FormatSpecifier({})`, from `test/formatSpecifier-test.js`.
///
/// The Rust builder starting from no fields must produce exactly the same
/// specifier as parsing the empty string.
#[test]
#[ignore = "Phase 3 placeholder: the FormatSpecifier builder does not exist yet"]
fn builder_defaults_match_the_empty_specifier() {
    crate::placeholder(
        PHASE_3,
        "FormatSpecifier::builder().build() to equal \"\".parse()",
    );
}
