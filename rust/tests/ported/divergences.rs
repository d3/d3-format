//! Oracle assertions whose JavaScript semantics the port deliberately does not
//! reproduce.
//!
//! Each test here is named by an entry in `rust/DIVERGENCES.md` and by the
//! `divergence` field of `rust/fixtures/d3-format-3.1.2/assertion-map.json`. None of
//! them may be declared "passed" by some other Rust call: the point is to assert the
//! replacement contract, not to pretend the JavaScript behaviour survived.

const PHASE_3: &str =
    "Phase 3 placeholder: the Locale, Formatter and FormatSpecifier APIs do not exist yet";

/// `format === locale.format` and `formatPrefix === locale.formatPrefix`, from
/// `test/defaultLocale-test.js`.
///
/// Divergence `no-mutable-default-locale`. The oracle asserts that calling
/// `formatDefaultLocale` rebinds two module-level exports. Rust has no mutable
/// process-global default locale, so there is no binding whose identity could
/// change. The replacement contract is that `Locale::en_us()` is a value, that
/// building another locale leaves it untouched, and that both are `Send + Sync`.
#[test]
#[ignore = "Phase 3 placeholder: Locale does not exist yet"]
fn default_locale_is_not_a_mutable_process_global() {
    crate::placeholder(
        PHASE_3,
        "Locale::en_us() to be unaffected by building any other locale, \
         with no process-global to rebind",
    );
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
#[ignore = "Phase 3 placeholder: FormatSpecifier and its builder do not exist yet"]
fn format_specifier_is_immutable_after_construction() {
    crate::placeholder(
        PHASE_3,
        "the builder to render \"_>-\" through \"_^+$012,.2~f\" without \
         mutating a constructed specifier, and negative width or precision to be unrepresentable",
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
#[ignore = "Phase 3 placeholder: the FormatSpecifier builder does not exist yet"]
fn specifier_construction_does_not_coerce_arbitrary_javascript_values() {
    crate::placeholder(
        PHASE_3,
        "the builder to accept only typed field values, with no \
         String()/ToInt32/truthiness coercion path",
    );
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
#[test]
#[ignore = "Phase 3 placeholder: Formatter and FormatError do not exist yet"]
fn numeric_formatters_reject_non_numeric_input() {
    crate::placeholder(
        PHASE_3,
        "f64::NAN to render as the locale's nan text and format_text to \
         fail with FormatError::InputTypeMismatch on a numeric specifier",
    );
}
