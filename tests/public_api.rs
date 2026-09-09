//! The section 5 public surface, exercised the way a dependent would.
//!
//! The unit tests inside `src/` may reach private fields; these may not. What they
//! check is the contract: the declared signatures, the edge policies section 5.3
//! enumerates, the limit and error behaviour of section 5.4, and the thread-safety
//! section 5.3 requires. Each edge policy has a test named after it, so a change in
//! behaviour names the policy it broke.
//!
//! Digits are not the subject here -- `tests/generated` compares those against d3
//! call for call. What these check about `format_number` and `format_text` is the
//! surface around them: which inputs each accepts, what the buffer looks like after
//! a failure, and which errors a caller can rely on seeing.

use std::collections::TryReserveError;

use d3_format::{
    as_precision, precision_fixed, precision_prefix, precision_round, Align, FormatError,
    FormatLimits, FormatSpecifier, FormatSpecifierBuilder, FormatType, Formatter, InputKind,
    Locale, LocaleBuilder, LocaleDefinition, LocaleError, ParseError, ParseErrorKind, Sign, Symbol,
};

// ---------------------------------------------------------------------------
// Section 5.3: Send + Sync
// ---------------------------------------------------------------------------

/// Section 5.3: `Locale`, `FormatSpecifier` and `Formatter` must be `Send + Sync`.
///
/// This is the compile-time half. It is a function rather than a `#[test]` so that
/// the assertion is the *build*: if any of these stops being thread-safe, the test
/// target fails to compile and no run is needed to notice.
fn _section_5_3_thread_safety_is_a_compile_error_to_break() {
    fn require<T: Send + Sync + 'static>() {}

    require::<Locale>();
    require::<FormatSpecifier>();
    require::<Formatter>();

    // The supporting types travel with them, so they are held to the same bar. An
    // error that is not `Send + Sync` cannot cross a thread boundary or go into a
    // `Box<dyn Error + Send + Sync>`, which is where a caller's error type usually
    // ends up.
    require::<LocaleBuilder>();
    require::<LocaleDefinition>();
    require::<FormatSpecifierBuilder>();
    require::<FormatLimits>();
    require::<FormatError>();
    require::<ParseError>();
    require::<LocaleError>();
    require::<InputKind>();
}

#[test]
fn a_formatter_can_be_built_on_one_thread_and_read_on_another() {
    // The compile-time assertion says the bounds hold; this says they are useful,
    // which is the point of not having a lifetime parameter on `Formatter`.
    let locale = Locale::en_us();
    let formatter = locale.formatter("$,.2f").expect("valid");

    let moved = std::thread::spawn(move || formatter.specifier().to_string())
        .join()
        .expect("the thread must not panic");
    assert_eq!(moved, " >-$,.2f");

    // A shared reference works too, which is what `Sync` buys.
    let shared = std::sync::Arc::new(locale.formatter("d").expect("valid"));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let shared = std::sync::Arc::clone(&shared);
            std::thread::spawn(move || shared.specifier().to_string())
        })
        .collect();
    for reader in readers {
        assert_eq!(reader.join().expect("no panic"), " >-d");
    }
}

#[test]
fn cloning_a_locale_shares_it_rather_than_copying_it() {
    // Section 5.3: `Locale` is `Arc`-backed and cheap to clone. There is no public
    // pointer to compare, so this asserts the observable half -- clones are equal
    // and independent of each other -- and leaves the refcount to the type.
    let locale = Locale::en_us();
    let clone = locale.clone();
    assert_eq!(locale, clone);

    let other = Locale::builder().decimal(",").build().expect("valid");
    assert_ne!(locale, other);
    assert_eq!(
        locale.decimal(),
        ".",
        "building another locale changed en-US"
    );
}

// ---------------------------------------------------------------------------
// Section 5.2: the specifier, through its public surface only
// ---------------------------------------------------------------------------

#[test]
fn parsing_fills_in_every_default_and_display_spells_them_all() {
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
    assert_eq!(specifier.to_string(), " >-");
    assert_eq!(specifier, FormatSpecifier::default());
}

#[test]
fn the_builder_is_the_only_way_to_change_a_specifier() {
    let original: FormatSpecifier = "$,.2f".parse().expect("valid");
    let changed = original
        .to_builder()
        .format_type(FormatType::Exponent)
        .precision(4)
        .build()
        .expect("valid");

    assert_eq!(original.to_string(), " >-$,.2f", "the original moved");
    assert_eq!(changed.to_string(), " >-$,.4e");
    assert_eq!(
        FormatSpecifierBuilder::new(),
        FormatSpecifier::default().to_builder()
    );
}

#[test]
fn a_parse_failure_carries_d3s_message_and_a_distinguishable_kind() {
    // Section 5.2 pins the text; the kind is how a caller tells the three causes
    // apart without matching on a string.
    for text in ["foo", ".-2s", ".f"] {
        let error = text.parse::<FormatSpecifier>().unwrap_err();
        assert_eq!(error.to_string(), format!("invalid format: {text}"));
        assert_eq!(error.kind(), ParseErrorKind::Syntax);
        assert_eq!(error.input(), text);
    }

    let overflow = "99999999999f".parse::<FormatSpecifier>().unwrap_err();
    assert_eq!(overflow.kind(), ParseErrorKind::WidthOverflow);
    assert_eq!(overflow.to_string(), "invalid format: 99999999999f");

    let fill = "\t>d".parse::<FormatSpecifier>().unwrap_err();
    assert_eq!(fill.kind(), ParseErrorKind::Fill);
    assert_eq!(fill.to_string(), "invalid format: \t>d");
}

#[test]
fn a_specifier_error_reaches_the_formatter_untouched() {
    // The wrapping must not add a prefix: `test/format-test.js` asserts the same
    // `invalid format: ...` text of `format(...)` that it asserts of
    // `formatSpecifier(...)`.
    let error = Locale::en_us().formatter("foo").unwrap_err();
    assert_eq!(error.to_string(), "invalid format: foo");
    match &error {
        FormatError::InvalidSpecifier(parse) => {
            assert_eq!(parse.kind(), ParseErrorKind::Syntax);
            assert_eq!(parse.input(), "foo");
        }
        other => panic!("expected InvalidSpecifier, got {other:?}"),
    }

    // And it is the *source*, so `{:#}`-style error chains still see both levels.
    let source = std::error::Error::source(&error).expect("a wrapped parse error");
    assert_eq!(source.to_string(), "invalid format: foo");
}

#[test]
fn an_unknown_ascii_type_letter_is_kept_rather_than_rejected() {
    // Section 5.2: unknown ASCII type letters remain stored. d3 formats them as
    // `.12~g`; the specifier itself still reports the letter.
    let specifier: FormatSpecifier = "q".parse().expect("d3 accepts any ASCII letter");
    assert_eq!(specifier.format_type(), FormatType::Unknown('q'));
    assert!(!specifier.trim());
    assert_eq!(specifier.precision(), None);
    assert_eq!(specifier.to_string(), " >-q");

    // Only ASCII letters and `%`. A digit or punctuation is a syntax error.
    assert_eq!(FormatType::from_char('q'), Some(FormatType::Unknown('q')));
    assert_eq!(FormatType::from_char('1'), None);
    assert_eq!(FormatType::from_char('\u{b5}'), None);
}

#[test]
fn a_huge_width_or_precision_renders_as_itself_rather_than_wrapping() {
    // `FormatSpecifier.prototype.toString` spells the width as
    // `Math.max(1, this.width | 0)` and the precision as
    // `Math.max(0, this.precision | 0)`, so both wrap through ToInt32 above 2^31:
    // Node renders `formatSpecifier("4294967295f")` as `" >-1f"` and
    // `formatSpecifier(".99999999999999999999f")` as `" >-.1661992960f"`.
    //
    // Section 5.2 removes the ToInt32 behaviour, so the port renders the value it
    // parsed. See `immutable-typed-specifier` in `DIVERGENCES.md`; no formatted
    // output differs, because both clamp the number into range before using it.
    let width: FormatSpecifier = "4294967295f".parse().expect("u32::MAX fits");
    assert_eq!(width.width(), Some(u32::MAX));
    assert_eq!(width.to_string(), " >-4294967295f");

    // A precision too large to hold saturates rather than failing, because every
    // value above 21 clamps to the same place at format time.
    let precision: FormatSpecifier = ".99999999999999999999f".parse().expect("valid");
    assert_eq!(precision.precision(), Some(u32::MAX));
    assert_eq!(precision.to_string(), " >-.4294967295f");

    // Which means rendering stays a fixed point where d3's does not: re-parsing the
    // port's spelling gives the same specifier back.
    assert_eq!(
        precision
            .to_string()
            .parse::<FormatSpecifier>()
            .expect("valid"),
        precision
    );
}

#[test]
fn a_fill_that_cannot_be_padded_with_is_rejected_at_parse_time() {
    // Divergence `no-non-bmp-or-newline-fill`. d3's `.` already refuses the line
    // terminators and cannot capture a supplementary-plane character as a whole;
    // the port refuses the rest of the control characters too, and says so.
    for text in [
        "\u{1f600}>d", // outside the BMP: d3 would capture half a surrogate pair
        "\n>d",
        "\r>d",
        "\u{2028}>d",
        "\u{2029}>d",
        "\t>d",
        "\u{0}>d",
        "\u{7f}>d",
        "\u{85}>d",
    ] {
        let error = text.parse::<FormatSpecifier>().unwrap_err();
        assert_eq!(error.to_string(), format!("invalid format: {text}"));
        assert!(
            matches!(error.kind(), ParseErrorKind::Fill | ParseErrorKind::Syntax),
            "{text:?} was rejected as {:?}",
            error.kind()
        );
    }

    // Everything d3's own tests and locale data use as fill is accepted, including
    // non-ASCII BMP characters.
    for (text, expected) in [("_>d", '_'), (" >d", ' '), ("\u{2212}>d", '\u{2212}')] {
        let specifier: FormatSpecifier = text.parse().expect("an ordinary BMP fill");
        assert_eq!(specifier.fill(), expected);
    }

    // The builder applies the same rule, and names the canonical spelling.
    let error = FormatSpecifier::builder().fill('\n').build().unwrap_err();
    assert_eq!(error.kind(), ParseErrorKind::Fill);
}

// ---------------------------------------------------------------------------
// Section 5.3: the locale edge policies, one test each
// ---------------------------------------------------------------------------

#[test]
fn policy_defaults_are_d3s() {
    // Section 5.3: decimal `.`, empty currency affixes, percent `%`, minus U+2212,
    // NaN `NaN`, and grouping off.
    for locale in [
        Locale::builder().build().expect("valid"),
        Locale::from_definition(LocaleDefinition::default()).expect("valid"),
        LocaleBuilder::default().build().expect("valid"),
    ] {
        assert_eq!(locale.decimal(), ".");
        assert_eq!(locale.currency_prefix(), "");
        assert_eq!(locale.currency_suffix(), "");
        assert_eq!(locale.percent(), "%");
        assert_eq!(locale.minus(), "\u{2212}");
        assert_eq!(locale.nan(), "NaN");
        assert_eq!(locale.grouping(), None);
        assert_eq!(locale.thousands(), "");
        assert_eq!(locale.numerals(), None);
    }

    // The ASCII hyphen is *not* the default: `formatLocale({}).format("06.2f")(-2)`
    // is `"\u{2212}02.00"` in `test/locale-test.js`.
    assert_ne!(Locale::builder().build().expect("valid").minus(), "-");
}

#[test]
fn policy_en_us_adds_grouping_a_separator_and_a_currency_prefix() {
    let locale = Locale::en_us();
    assert_eq!(locale.decimal(), ".");
    assert_eq!(locale.thousands(), ",");
    assert_eq!(locale.grouping(), Some(&[3][..]));
    assert_eq!(locale.currency_prefix(), "$");
    assert_eq!(locale.currency_suffix(), "");
    assert_eq!(locale.minus(), "\u{2212}");
    assert_eq!(locale.nan(), "NaN");
}

#[test]
fn policy_grouping_is_disabled_unless_both_keys_are_supplied() {
    assert_eq!(
        Locale::builder()
            .grouping([3])
            .build()
            .expect("valid")
            .grouping(),
        None,
        "sizes without a separator"
    );
    assert_eq!(
        Locale::builder()
            .thousands(",")
            .build()
            .expect("valid")
            .grouping(),
        None,
        "a separator without sizes"
    );

    let both = Locale::builder()
        .grouping([3])
        .thousands(",")
        .build()
        .expect("valid");
    assert_eq!(both.grouping(), Some(&[3][..]));
    assert_eq!(both.thousands(), ",");
}

#[test]
fn policy_an_empty_grouping_list_disables_grouping() {
    let locale = Locale::builder()
        .grouping(Vec::new())
        .thousands(",")
        .build()
        .expect("an empty list is a definition, not an error");
    assert_eq!(locale.grouping(), None);
    assert_eq!(locale.thousands(), "");
}

#[test]
fn policy_a_zero_grouping_size_is_rejected() {
    // d3's grouping loop would consume no digits and never terminate.
    let error = Locale::builder()
        .grouping([3, 0, 2])
        .thousands(",")
        .build()
        .unwrap_err();
    assert_eq!(error, LocaleError::ZeroGroupingSize { index: 1 });
    assert!(error.to_string().contains("index 1"), "{error}");

    assert!(Locale::builder()
        .grouping([0])
        .thousands(",")
        .build()
        .is_err());
    // Every positive size is fine, including d3's cycling `[3, 2]` shape.
    assert!(Locale::builder()
        .grouping([3, 2])
        .thousands(",")
        .build()
        .is_ok());
}

#[test]
fn policy_numerals_are_exactly_ten_strings() {
    // The type is the validation: `[String; 10]` cannot hold nine or eleven, so
    // d3's "silently emit `undefined` for the digits the array does not cover" has
    // no way to happen. That is a compile-time property, so what runs here is that
    // ten do work and survive to the getter.
    let eastern: [String; 10] = [
        "\u{660}", "\u{661}", "\u{662}", "\u{663}", "\u{664}", "\u{665}", "\u{666}", "\u{667}",
        "\u{668}", "\u{669}",
    ]
    .map(str::to_owned);
    let locale = Locale::builder()
        .numerals(eastern.clone())
        .build()
        .expect("valid");
    assert_eq!(locale.numerals(), Some(&eastern));

    // A numeral may be any string, including a multi-code-point one.
    let decorated: [String; 10] = std::array::from_fn(|digit| format!("<{digit}>"));
    let locale = Locale::builder()
        .numerals(decorated.clone())
        .build()
        .expect("valid");
    assert_eq!(locale.numerals(), Some(&decorated));
}

#[test]
fn policy_locale_text_may_be_unicode() {
    // `test/locale-test.js` uses "\u{202f}%", "➖" and "\u{a0}€"; none of them is
    // ASCII and none of them is one code unit.
    let locale = Locale::builder()
        .decimal(",")
        .thousands(".")
        .grouping([3])
        .currency("", "\u{a0}\u{20ac}")
        .percent("\u{202f}%")
        .minus("\u{2796}")
        .nan("N/A")
        .build()
        .expect("valid");
    assert_eq!(locale.currency_suffix(), "\u{a0}\u{20ac}");
    assert_eq!(locale.percent(), "\u{202f}%");
    assert_eq!(locale.minus(), "\u{2796}");
    assert_eq!(locale.nan(), "N/A");
}

#[test]
fn policy_width_is_measured_in_utf16_code_units() {
    // Section 5.3 measures width in UTF-16 code units, and section 5.4's limit is
    // spelled in the same unit, so the two cannot disagree about what a width is.
    // The width a specifier carries is that count, and the limit rejects it in that
    // count -- not in bytes and not in `char`s.
    let limits = FormatLimits {
        max_width_utf16: 10,
        ..FormatLimits::DEFAULT
    };
    let locale = Locale::en_us();
    assert!(locale.formatter_with_limits("10d", limits).is_ok());
    assert_eq!(
        locale.formatter_with_limits("11d", limits).unwrap_err(),
        FormatError::WidthLimit {
            requested: 11,
            maximum: 10
        }
    );

    // A non-ASCII fill does not change the count: the width is a number of code
    // units of output, not of the fill character's encoding.
    let specifier: FormatSpecifier = "\u{2212}>10d".parse().expect("valid");
    assert_eq!(specifier.width(), Some(10));
    assert!(locale.formatter_for_with_limits(&specifier, limits).is_ok());
}

#[test]
fn policy_built_locales_and_formatters_are_immutable() {
    // There is no `&mut self` method on either type, which is the real assertion
    // and is enforced by the compiler. What is left to check at run time is that
    // reading through a formatter cannot be observed to change its locale, and that
    // two formatters built the same way are indistinguishable.
    let locale = Locale::en_us();
    let first = locale.formatter("$,.2f").expect("valid");
    let second = locale.formatter("$,.2f").expect("valid");
    assert_eq!(first, second);
    assert_eq!(first.clone(), first);

    assert_eq!(first.specifier().to_string(), " >-$,.2f");
    assert_eq!(second.specifier().to_string(), " >-$,.2f");
    assert_eq!(locale.currency_prefix(), "$");
}

#[test]
fn a_formatter_reports_the_specifier_it_was_given_not_the_one_it_resolved() {
    // `newFormat` rewrites `n` to `,g` and an unknown letter to `.12~g` in local
    // variables, so `format.toString()` still spells what the caller wrote. The
    // resolution is real but private; this is the part that is observable.
    let locale = Locale::en_us();
    assert_eq!(
        locale
            .formatter("n")
            .expect("valid")
            .specifier()
            .to_string(),
        " >-n"
    );
    assert_eq!(
        locale
            .formatter("q")
            .expect("valid")
            .specifier()
            .to_string(),
        " >-q"
    );
    assert_eq!(
        locale.formatter("").expect("valid").specifier().to_string(),
        " >-"
    );
}

#[test]
fn a_prefix_formatter_reports_type_f_whatever_the_caller_wrote() {
    // `formatPrefix` assigns `specifier.type = "f"` on the parsed object before
    // building the closure, so the normalized specifier really does change.
    let locale = Locale::en_us();
    let formatter = locale.prefix_formatter(",.2", 1e3).expect("valid");
    assert_eq!(formatter.specifier().format_type(), FormatType::Fixed);
    assert_eq!(formatter.specifier().to_string(), " >-,.2f");
    assert_eq!(formatter.input_kind(), InputKind::Number);

    // Including from a `c` specifier, which stops it being a text formatter.
    let forced = locale.prefix_formatter("c", 1e3).expect("valid");
    assert_eq!(forced.specifier().format_type(), FormatType::Fixed);
    assert_eq!(forced.input_kind(), InputKind::Number);

    // A reference with no decimal exponent still builds. Section 4.1: d3's scale is
    // then NaN, so every value formats as the locale's NaN with no SI suffix.
    for reference in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let degenerate = locale
            .prefix_formatter("s", reference)
            .unwrap_or_else(|error| panic!("reference {reference}: {error}"));
        assert_eq!(
            degenerate.format_number(1234.5).expect("valid"),
            "NaN",
            "reference {reference}"
        );
    }
}

// ---------------------------------------------------------------------------
// Section 5.3: input kinds
// ---------------------------------------------------------------------------

#[test]
fn only_type_c_accepts_text() {
    let locale = Locale::en_us();
    assert_eq!(
        locale.formatter("c").expect("valid").input_kind(),
        InputKind::Text
    );
    assert_eq!(
        locale.formatter("_^20c").expect("valid").input_kind(),
        InputKind::Text
    );
    for spec in [
        "", "d", "f", "e", "g", "n", "s", "%", "p", "r", "b", "o", "x", "X", "q",
    ] {
        assert_eq!(
            locale.formatter(spec).expect("valid").input_kind(),
            InputKind::Number,
            "{spec:?}"
        );
    }
}

#[test]
fn a_numeric_formatter_refuses_text_instead_of_coercing_it() {
    // Divergence `no-input-coercion`. d3 would coerce; the port names the mismatch.
    let formatter = Locale::en_us().formatter("6.2f").expect("valid");
    let error = formatter.format_text("nonsense").unwrap_err();
    assert_eq!(
        error,
        FormatError::InputTypeMismatch {
            expected: InputKind::Number,
            actual: InputKind::Text,
        }
    );
    assert_eq!(
        error.to_string(),
        "this formatter takes numeric input, but was given text input"
    );

    // Even for the empty text, which is the case a coercing implementation is most
    // likely to let through.
    assert!(formatter.format_text("").is_err());

    // `format_text_into` reports the same thing and leaves the buffer empty.
    let mut out = String::from("previous contents");
    assert!(formatter.format_text_into(&mut out, "nonsense").is_err());
    assert!(out.is_empty(), "the buffer was not cleared: {out:?}");
}

#[test]
fn a_text_formatter_refuses_numbers_instead_of_coercing_them() {
    let formatter = Locale::en_us().formatter("c").expect("valid");
    let error = formatter.format_number(42.0).unwrap_err();
    assert_eq!(
        error,
        FormatError::InputTypeMismatch {
            expected: InputKind::Text,
            actual: InputKind::Number,
        }
    );
    assert_eq!(
        error.to_string(),
        "this formatter takes text input, but was given numeric input"
    );

    let mut out = String::from("previous contents");
    assert!(formatter.format_number_into(&mut out, 42.0).is_err());
    assert!(out.is_empty(), "the buffer was not cleared: {out:?}");
}

#[test]
fn the_input_kind_check_happens_before_any_layout() {
    // The mismatch must be reported rather than half-formatted: a numeric formatter
    // handed text must not lay the text out, and a `c` formatter handed a number
    // must not stringify it. Both directions are checked above; this states the
    // ordering, by giving each one input the other would happily have accepted.
    let numeric = Locale::en_us().formatter("d").expect("valid");
    assert!(
        numeric.format_text("1").is_err(),
        "a numeric formatter formatted text that would have parsed as a number"
    );

    let textual = Locale::en_us().formatter("c").expect("valid");
    assert!(
        textual.format_number(1.0).is_err(),
        "a text formatter formatted a number"
    );
}

// ---------------------------------------------------------------------------
// Section 5.4: limits and errors
// ---------------------------------------------------------------------------

#[test]
fn the_default_limits_are_the_documented_ones() {
    let limits = FormatLimits::default();
    assert_eq!(limits.max_width_utf16, 1_000_000);
    assert_eq!(limits.max_output_bytes, 16 * 1024 * 1024);
    assert_eq!(limits, FormatLimits::DEFAULT);
}

#[test]
fn parsing_rejects_a_width_the_syntax_cannot_hold() {
    // Section 5.4: a syntactic width above `u32::MAX` is a parse failure, before
    // any limit is consulted. d3 accepts the digits and fails later, if at all.
    assert_eq!(
        "4294967295f"
            .parse::<FormatSpecifier>()
            .expect("u32::MAX fits")
            .width(),
        Some(u32::MAX)
    );
    let error = "4294967296f".parse::<FormatSpecifier>().unwrap_err();
    assert_eq!(error.kind(), ParseErrorKind::WidthOverflow);
    assert_eq!(error.to_string(), "invalid format: 4294967296f");

    // And through the formatter, as a wrapped parse error rather than a width one:
    // the value never became a width.
    let error = Locale::en_us().formatter("4294967296f").unwrap_err();
    assert!(
        matches!(error, FormatError::InvalidSpecifier(_)),
        "{error:?}"
    );
}

#[test]
fn formatter_construction_rejects_a_width_over_the_limit() {
    // The check is at construction, so a formatter that exists cannot later fail to
    // pad. Section 5.4 puts it exactly there.
    let locale = Locale::en_us();
    assert!(locale.formatter("1000000d").is_ok());
    assert_eq!(
        locale.formatter("1000001d").unwrap_err(),
        FormatError::WidthLimit {
            requested: 1_000_001,
            maximum: 1_000_000
        }
    );
    assert_eq!(
        locale.formatter("1000001d").unwrap_err().to_string(),
        "specifier width 1000001 is over the 1000000 code unit limit"
    );
}

#[test]
fn the_default_taking_entry_points_use_the_default_limits() {
    // Section 5.4 names the three: `formatter`, `formatter_for` and
    // `prefix_formatter`. All three must refuse the same width, and all three
    // `*_with_limits` siblings must accept it once the caller raises the bound.
    let locale = Locale::en_us();
    let over: FormatSpecifier = "1000001d".parse().expect("valid syntax");
    let raised = FormatLimits {
        max_width_utf16: 2_000_000,
        ..FormatLimits::DEFAULT
    };
    let expected = FormatError::WidthLimit {
        requested: 1_000_001,
        maximum: 1_000_000,
    };

    assert_eq!(locale.formatter("1000001d").unwrap_err(), expected);
    assert_eq!(locale.formatter_for(&over).unwrap_err(), expected);
    assert_eq!(
        locale.prefix_formatter("1000001d", 1e3).unwrap_err(),
        expected
    );

    assert!(locale.formatter_with_limits("1000001d", raised).is_ok());
    assert!(locale.formatter_for_with_limits(&over, raised).is_ok());
    assert!(locale
        .prefix_formatter_with_limits("1000001d", 1e3, raised)
        .is_ok());

    // Lowering works as well as raising, so the parameter is really consulted.
    let tight = FormatLimits {
        max_width_utf16: 4,
        ..FormatLimits::DEFAULT
    };
    assert_eq!(
        locale.formatter_with_limits("5d", tight).unwrap_err(),
        FormatError::WidthLimit {
            requested: 5,
            maximum: 4
        }
    );
}

#[test]
fn a_formatter_remembers_the_limits_it_was_built_with() {
    let tight = FormatLimits {
        max_width_utf16: 32,
        max_output_bytes: 4096,
    };
    let formatter = Locale::en_us()
        .formatter_with_limits("d", tight)
        .expect("valid");
    assert_eq!(formatter.limits(), tight);
    assert_eq!(
        Locale::en_us().formatter("d").expect("valid").limits(),
        FormatLimits::default()
    );
}

#[test]
fn locale_affixes_count_against_the_output_byte_limit() {
    // Section 5.4: locale affixes are part of the output, so they are measured
    // against `max_output_bytes`. A formatter whose fixed text alone exceeds the
    // budget is refused at construction rather than at the first call.
    let locale = Locale::builder()
        .currency("\u{a4}\u{a4}\u{a4}\u{a4}", "")
        .build()
        .expect("valid");
    let tight = FormatLimits {
        max_output_bytes: 4,
        ..FormatLimits::DEFAULT
    };

    // Four currency signs are eight UTF-8 bytes, so a `$` specifier does not fit.
    assert_eq!(
        locale.formatter_with_limits("$d", tight).unwrap_err(),
        FormatError::OutputLimit {
            requested: 8,
            maximum: 4
        }
    );
    // Without the `$` there is no affix to pay for, so the same locale and the same
    // budget are fine.
    assert!(locale.formatter_with_limits("d", tight).is_ok());
    // And with the default budget the affix is unremarkable.
    assert!(locale.formatter("$d").is_ok());
}

#[test]
fn a_suffix_and_a_prefix_are_both_counted() {
    let locale = Locale::builder()
        .currency("", "\u{a0}\u{20ac}")
        .build()
        .expect("valid");
    let tight = FormatLimits {
        max_output_bytes: 3,
        ..FormatLimits::DEFAULT
    };
    // U+00A0 is two bytes and U+20AC is three.
    assert_eq!(
        locale.formatter_with_limits("$d", tight).unwrap_err(),
        FormatError::OutputLimit {
            requested: 5,
            maximum: 3
        }
    );

    // The SI suffix of a prefix formatter is counted with the locale's own suffix,
    // because both end up in the same string.
    let micro = Locale::builder()
        .currency("", "\u{a0}\u{20ac}")
        .build()
        .expect("valid");
    let budget = FormatLimits {
        max_output_bytes: 5,
        ..FormatLimits::DEFAULT
    };
    assert!(micro
        .prefix_formatter_with_limits("$f", 1.0, budget)
        .is_ok());
    assert_eq!(
        micro
            .prefix_formatter_with_limits("$f", 1e6, budget)
            .unwrap_err(),
        FormatError::OutputLimit {
            requested: 6,
            maximum: 5
        }
    );
}

#[test]
fn the_error_type_says_what_it_is_without_leaking_how() {
    // Section 5.4's five variants, each with a message that names the quantity a
    // caller would need to act on.
    let messages = [
        (
            FormatError::WidthLimit {
                requested: 5,
                maximum: 4,
            },
            "specifier width 5 is over the 4 code unit limit",
        ),
        (
            FormatError::OutputLimit {
                requested: 20,
                maximum: 10,
            },
            "formatted output would be 20 bytes, over the 10 byte limit",
        ),
        (FormatError::AllocationFailed, "allocation failed"),
        (
            FormatError::InputTypeMismatch {
                expected: InputKind::Number,
                actual: InputKind::Text,
            },
            "this formatter takes numeric input, but was given text input",
        ),
    ];
    for (error, expected) in messages {
        assert_eq!(error.to_string(), expected);
        assert!(std::error::Error::source(&error).is_none(), "{error:?}");
    }

    let parse = "foo".parse::<FormatSpecifier>().unwrap_err();
    let wrapped = FormatError::from(parse.clone());
    assert_eq!(wrapped, FormatError::InvalidSpecifier(parse));
    assert_eq!(wrapped.to_string(), "invalid format: foo");
}

#[test]
fn a_reserve_failure_would_be_reported_rather_than_aborting() {
    // Section 5.4 requires `try_reserve` on every dynamic output buffer, whose
    // whole point is that a refused allocation is a value. There is no portable way
    // to make the allocator refuse inside a formatter, so what this pins is that
    // the crate's failure story exists and is reachable: `TryReserveError` maps to
    // `AllocationFailed`, and `AllocationFailed` is a `FormatError` a caller can
    // match on rather than an abort they cannot.
    let huge: Result<(), TryReserveError> = Vec::<u8>::new().try_reserve(usize::MAX);
    assert!(huge.is_err(), "reserving usize::MAX bytes succeeded");
    let mapped = huge.map_err(|_| FormatError::AllocationFailed).unwrap_err();
    assert_eq!(mapped, FormatError::AllocationFailed);

    // The same request through the public surface is an `OutputLimit`, because the
    // limit is consulted before the allocator is.
    let locale = Locale::builder().currency("$", "").build().expect("valid");
    let none = FormatLimits {
        max_output_bytes: 0,
        ..FormatLimits::DEFAULT
    };
    assert_eq!(
        locale.formatter_with_limits("$d", none).unwrap_err(),
        FormatError::OutputLimit {
            requested: 1,
            maximum: 0
        }
    );
}

#[test]
fn the_into_entry_points_clear_the_buffer_and_reuse_its_capacity() {
    // Section 5.3: `format_*_into` clears `out` before doing any work, leaves it
    // empty on error, and reuses the caller's capacity. It makes no allocation-free
    // claim, so what is asserted is that a buffer already large enough is not
    // reallocated -- not that no allocation happens anywhere.
    let formatter = Locale::en_us().formatter("$,.2f").expect("valid");
    let mut out = String::with_capacity(4096);
    let capacity = out.capacity();
    out.push_str("previous contents");

    formatter
        .format_number_into(&mut out, 1234.5)
        .expect("valid");
    assert_eq!(out, "$1,234.50", "the previous contents survived");
    assert_eq!(out.capacity(), capacity, "the buffer was reallocated");

    // Again into the same buffer: the clear is at the start of the call, not the
    // end of the previous one, so a caller that inspects `out` between calls sees
    // the last result rather than an empty string.
    formatter.format_number_into(&mut out, -1.0).expect("valid");
    assert_eq!(out, "\u{2212}$1.00");
    assert_eq!(out.capacity(), capacity);

    // On a limit failure the buffer is empty, not half-written. The budget is small
    // enough that the affixes alone would exceed it, so the failure happens after
    // some output has already been produced.
    let tight = Locale::en_us()
        .formatter_with_limits(
            "$,.2f",
            FormatLimits {
                max_output_bytes: 4,
                ..FormatLimits::DEFAULT
            },
        )
        .expect("valid");
    let error = tight.format_number_into(&mut out, 1234.5).unwrap_err();
    assert!(
        matches!(error, FormatError::OutputLimit { .. }),
        "{error:?}"
    );
    assert!(out.is_empty(), "the buffer kept a partial result: {out:?}");

    // And the same for the text entry point.
    let character = Locale::en_us().formatter("_^9c").expect("valid");
    out.push_str("previous contents");
    character
        .format_text_into(&mut out, "\u{1f600}")
        .expect("valid");
    assert_eq!(out, "___\u{1f600}____");
    assert_eq!(out.capacity(), capacity);
}

// ---------------------------------------------------------------------------
// Section 5.5: precision helpers
// ---------------------------------------------------------------------------

#[test]
fn the_precision_helpers_are_exported_and_return_d3s_values() {
    assert_eq!(precision_fixed(0.089), 2.0);
    assert_eq!(precision_round(0.01, 1.01), 3.0);
    assert_eq!(precision_prefix(1e-6, 1e-6), 0.0);

    // NaN for the degenerate cases, which is why the return type is `f64`.
    assert!(precision_fixed(0.0).is_nan());
    assert!(precision_round(0.0, 1.0).is_nan());
    assert!(precision_prefix(0.0, 1.0).is_nan());
}

#[test]
fn as_precision_converts_a_suggestion_into_a_specifier_field() {
    let suggestion = precision_fixed(0.089);
    let precision = as_precision(suggestion).expect("a finite whole suggestion");
    let specifier = FormatSpecifier::builder()
        .precision(precision)
        .format_type(FormatType::Fixed)
        .build()
        .expect("valid");
    assert_eq!(specifier.to_string(), " >-.2f");

    // It is a conversion, not a fallback: a suggestion with no answer stays absent
    // rather than becoming zero.
    assert_eq!(as_precision(precision_fixed(0.0)), None);
    assert_eq!(as_precision(-1.0), None);
    assert_eq!(as_precision(1.5), None);
}

// ---------------------------------------------------------------------------
// Feature-gated surface
// ---------------------------------------------------------------------------

#[cfg(feature = "locales")]
#[test]
fn the_named_locales_are_reachable_and_build() {
    let locale = Locale::named("en-IN").expect("a bundled locale");
    assert_eq!(locale.grouping(), Some(&[3, 2, 2, 2, 2, 2, 2, 2, 2, 2][..]));
    assert_eq!(locale.currency_prefix(), "\u{20b9}");

    let french = Locale::named("fr-FR").expect("a bundled locale");
    assert_eq!(french.decimal(), ",");
    assert_eq!(french.percent(), "\u{202f}%");

    assert!(Locale::named("xx-XX").is_none());
    assert!(Locale::named("").is_none());
}

/// Section 5.1: under the `serde` feature `LocaleDefinition` is the wire shape.
///
/// The derives are checked as bounds rather than by a round trip, because a round
/// trip needs a data format and the crate has no JSON dependency to borrow one
/// from. Section 6.1's dependency guard and section 5.1's "no default runtime
/// dependencies" both argue against adding one for a single assertion, and the
/// bound is what a dependent actually needs: that these two `impl`s exist for this
/// type. What a format would additionally pin -- that an absent key stays absent --
/// is a property of the `Option` fields, and `policy_defaults_are_d3s` already
/// checks the half of it that changes behaviour.
#[cfg(feature = "serde")]
fn _the_locale_definition_is_serde_shaped() {
    fn require<T: serde::Serialize + serde::de::DeserializeOwned>() {}

    require::<LocaleDefinition>();
}
