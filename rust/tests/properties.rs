//! Parser and display properties, checked over a generated corpus.
//!
//! Section 5.2 makes two claims that a handful of examples cannot support: that
//! `FromStr` accepts exactly d3's grammar, and that
//! `Locale::formatter_for(&specifier)` is *exactly* equivalent to
//! `Locale::formatter(&specifier.to_string())` without bypassing normalization.
//! Both are checked here against a corpus, and the first is checked against an
//! independent implementation of the grammar rather than against the parser's own
//! reading of it.
//!
//! # The reference matcher
//!
//! `src/formatSpecifier.js` is one regular expression:
//!
//! ```text
//! /^(?:(.)?([<>=^]))?([+\-( ])?([$#])?(0)?(\d+)?(,)?(\.\d+)?(~)?([a-z%])?$/i
//! ```
//!
//! [`reference_match`] is a brute-force backtracking matcher for exactly that
//! pattern, written from the pattern rather than from `src/specifier.rs`. It tries
//! each group's alternatives in the order a backtracking engine would -- longest
//! and present first -- and it runs on UTF-16 code units, because that is what a
//! JavaScript regular expression without the `u` flag matches and it is the reason
//! `.` cannot capture a supplementary-plane character as fill.
//!
//! The crate's parser is hand-written and single-pass. Comparing it against a
//! search that considers every parse is the point: a single-pass parser that
//! commits to the wrong alternative is exactly the bug this catches.
//!
//! The matcher was itself checked against the real thing: run under Node 24.18.0
//! over all 62,823 inputs [`corpus`] produces, the pattern above captures exactly
//! what this matcher does, group for group, with no disagreement. That comparison
//! is not committed, because it would make the test suite depend on Node; what is
//! committed is [`the_reference_matcher_agrees_with_node_on_the_documented_examples`],
//! which pins the cases the oracle suite states outright.

use d3_format::{
    Align, FormatError, FormatSpecifier, FormatType, Locale, ParseErrorKind, Sign, Symbol,
};

// ---------------------------------------------------------------------------
// An independent implementation of d3's grammar
// ---------------------------------------------------------------------------

/// What the regular expression captures, as raw code units and digit runs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Captures {
    fill: Option<u16>,
    align: Option<u16>,
    sign: Option<u16>,
    symbol: Option<u16>,
    zero: bool,
    width: Option<String>,
    comma: bool,
    precision: Option<String>,
    trim: bool,
    format_type: Option<u16>,
}

impl Captures {
    fn empty() -> Captures {
        Captures {
            fill: None,
            align: None,
            sign: None,
            symbol: None,
            zero: false,
            width: None,
            comma: false,
            precision: None,
            trim: false,
            format_type: None,
        }
    }
}

/// The ASCII character a code unit spells, or `None` if it spells none.
///
/// Every literal in the pattern is ASCII, so this is the only conversion the
/// matcher needs; a code unit above 0x7F is simply not one of them.
fn ascii(unit: u16) -> Option<char> {
    (unit < 0x80).then_some(unit as u8 as char)
}

fn is(unit: u16, expected: char) -> bool {
    ascii(unit) == Some(expected)
}

fn is_align(unit: u16) -> bool {
    matches!(ascii(unit), Some('<' | '>' | '=' | '^'))
}

fn is_sign(unit: u16) -> bool {
    matches!(ascii(unit), Some('+' | '-' | '(' | ' '))
}

fn is_symbol(unit: u16) -> bool {
    matches!(ascii(unit), Some('$' | '#'))
}

fn is_digit(unit: u16) -> bool {
    ascii(unit).is_some_and(|character| character.is_ascii_digit())
}

/// `[a-z%]` under the `i` flag.
fn is_type(unit: u16) -> bool {
    ascii(unit).is_some_and(|character| character == '%' || character.is_ascii_alphabetic())
}

/// What JavaScript's `.` refuses: the four line terminators.
fn is_line_terminator(unit: u16) -> bool {
    matches!(unit, 0x000A | 0x000D | 0x2028 | 0x2029)
}

/// The length of the digit run at `at`.
fn digit_run(units: &[u16], at: usize) -> usize {
    units[at..]
        .iter()
        .take_while(|unit| is_digit(**unit))
        .count()
}

fn digits_to_string(units: &[u16]) -> String {
    units.iter().map(|unit| *unit as u8 as char).collect()
}

/// Matches `units` against d3's specifier pattern, anchored at both ends.
///
/// Returns the captures a backtracking engine would produce, or `None` if the
/// pattern does not match. Alternatives are tried greedily, so the answer is the
/// one JavaScript's engine reaches, not merely *an* accepting parse.
fn reference_match(units: &[u16]) -> Option<Captures> {
    // `(?:(.)?([<>=^]))?`, greedy: spend a code unit on the fill first, then try the
    // alignment alone, then skip the group.
    let mut alignments: Vec<(usize, Option<u16>, Option<u16>)> = Vec::new();
    if units.len() >= 2 && is_align(units[1]) && !is_line_terminator(units[0]) {
        alignments.push((2, Some(units[0]), Some(units[1])));
    }
    if !units.is_empty() && is_align(units[0]) {
        alignments.push((1, None, Some(units[0])));
    }
    alignments.push((0, None, None));

    for (consumed, fill, align) in alignments {
        let mut captures = Captures::empty();
        captures.fill = fill;
        captures.align = align;
        if let Some(matched) = match_sign(units, consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_sign(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, sign) in optional(units, at, is_sign) {
        let mut captures = captures.clone();
        captures.sign = sign;
        if let Some(matched) = match_symbol(units, at + consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_symbol(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, symbol) in optional(units, at, is_symbol) {
        let mut captures = captures.clone();
        captures.symbol = symbol;
        if let Some(matched) = match_zero(units, at + consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_zero(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, zero) in optional(units, at, |unit| is(unit, '0')) {
        let mut captures = captures.clone();
        captures.zero = zero.is_some();
        if let Some(matched) = match_width(units, at + consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_width(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    // `(\d+)?` is greedy, so the longest run is tried first, then every shorter
    // non-empty one, then the absent alternative.
    let run = digit_run(units, at);
    for length in (1..=run).rev() {
        let mut captures = captures.clone();
        captures.width = Some(digits_to_string(&units[at..at + length]));
        if let Some(matched) = match_comma(units, at + length, captures) {
            return Some(matched);
        }
    }
    match_comma(units, at, captures)
}

fn match_comma(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, comma) in optional(units, at, |unit| is(unit, ',')) {
        let mut captures = captures.clone();
        captures.comma = comma.is_some();
        if let Some(matched) = match_precision(units, at + consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_precision(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    // `(\.\d+)?`: a dot on its own is not a precision, and nothing later in the
    // pattern matches a dot, which is why d3 rejects `".f"`.
    if units.get(at) == Some(&(b'.' as u16)) {
        let run = digit_run(units, at + 1);
        for length in (1..=run).rev() {
            let mut captures = captures.clone();
            captures.precision = Some(digits_to_string(&units[at + 1..at + 1 + length]));
            if let Some(matched) = match_trim(units, at + 1 + length, captures) {
                return Some(matched);
            }
        }
    }
    match_trim(units, at, captures)
}

fn match_trim(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, trim) in optional(units, at, |unit| is(unit, '~')) {
        let mut captures = captures.clone();
        captures.trim = trim.is_some();
        if let Some(matched) = match_type(units, at + consumed, captures) {
            return Some(matched);
        }
    }
    None
}

fn match_type(units: &[u16], at: usize, captures: Captures) -> Option<Captures> {
    for (consumed, format_type) in optional(units, at, is_type) {
        let mut captures = captures.clone();
        captures.format_type = format_type;
        if at + consumed == units.len() {
            return Some(captures);
        }
    }
    None
}

/// The two alternatives of a `(x)?` group, present first.
fn optional(units: &[u16], at: usize, accept: impl Fn(u16) -> bool) -> Vec<(usize, Option<u16>)> {
    match units.get(at) {
        Some(unit) if accept(*unit) => vec![(1, Some(*unit)), (0, None)],
        _ => vec![(0, None)],
    }
}

// ---------------------------------------------------------------------------
// The corpus
// ---------------------------------------------------------------------------

/// Characters chosen to reach every branch of the grammar and both sides of each
/// port-specific rejection.
const ALPHABET: [char; 28] = [
    ' ', '_', '<', '>', '^', '=', '+', '-', '(', ')', '$', '#', '0', '1', '2', '9', ',', '.', '~',
    'd', 'f', 'q', 's', '%', 'x', 'X', '\t', '\u{2212}',
];

/// Specifiers taken from the oracle test suite and the bundled locale data, so the
/// corpus is not only synthetic.
const REALISTIC: [&str; 40] = [
    "",
    "d",
    "f",
    ".2f",
    ".0f",
    ".30f",
    "e",
    ".3e",
    "g",
    ".0g",
    "n",
    ",g",
    "s",
    ".3s",
    "~s",
    "r",
    ".2r",
    "p",
    ".1p",
    "%",
    ".0%",
    "06.2%",
    "b",
    "#b",
    "o",
    "#o",
    "x",
    "#x",
    "X",
    "#X",
    "c",
    "020c",
    "$,.2f",
    "0=12",
    "012",
    "_^+$012,.2~f",
    "(,.2f",
    " 6.2f",
    "<6.2g",
    "\u{2212}>10.4f",
];

/// Inputs whose interesting property is that they are *not* in the grammar, or are
/// in it but outside what the port will accept.
const AWKWARD: [&str; 18] = [
    "foo",
    ".-2s",
    ".f",
    ".",
    "..2f",
    ",,d",
    "~~d",
    "0000",
    "00c",
    "4294967295f",
    "4294967296f",
    "99999999999999999999d",
    ".99999999999999999999f",
    "\u{1f600}>d",
    "\u{1f600}",
    "\n>d",
    "\u{2028}>d",
    "\u{85}>d",
];

/// A deterministic xorshift64 generator: the corpus must be the same on every run
/// and every machine, or a failure cannot be reproduced from the seed alone.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Every string of length 0 to 3 over [`ALPHABET`], plus longer random ones, plus
/// the two curated lists.
fn corpus() -> Vec<String> {
    let mut inputs: Vec<String> = Vec::new();
    inputs.push(String::new());
    for first in ALPHABET {
        inputs.push(first.to_string());
        for second in ALPHABET {
            inputs.push([first, second].iter().collect());
            for third in ALPHABET {
                inputs.push([first, second, third].iter().collect());
            }
        }
    }

    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..20_000 {
        let length = 4 + rng.below(9);
        let text: String = (0..length)
            .map(|_| ALPHABET[rng.below(ALPHABET.len())])
            .collect();
        inputs.push(text);
    }

    // Free-form noise of length four and up almost never lands in the grammar, so
    // the long half of the corpus would otherwise only exercise rejection. These
    // are assembled group by group instead, which reaches the long *accepted*
    // specifiers -- multi-digit widths, saturating precisions, every fill.
    for _ in 0..20_000 {
        inputs.push(structured(&mut rng));
    }

    inputs.extend(REALISTIC.iter().map(|text| (*text).to_owned()));
    inputs.extend(AWKWARD.iter().map(|text| (*text).to_owned()));
    inputs
}

/// One specifier built by choosing each group of the grammar independently.
///
/// Deliberately not always valid: the fill is drawn from a set that includes the
/// characters the port refuses, and the width and precision runs are long enough to
/// overflow, so the rejection paths are reached with well-formed surroundings
/// rather than only inside noise.
fn structured(rng: &mut Rng) -> String {
    const FILLS: [char; 8] = ['_', ' ', '0', '<', '$', '\t', '\u{2212}', '\u{1f600}'];
    const ALIGNS: [char; 4] = ['<', '>', '=', '^'];
    const SIGNS: [char; 4] = ['+', '-', '(', ' '];
    const SYMBOLS: [char; 2] = ['$', '#'];
    const TYPES: [char; 17] = [
        'b', 'c', 'd', 'e', 'f', 'g', 'n', 'o', 'p', 'r', 's', 'X', 'x', '%', 'q', 'Z', 'i',
    ];

    let mut text = String::new();
    if rng.below(3) > 0 {
        if rng.below(2) == 0 {
            text.push(FILLS[rng.below(FILLS.len())]);
        }
        text.push(ALIGNS[rng.below(ALIGNS.len())]);
    }
    if rng.below(2) == 0 {
        text.push(SIGNS[rng.below(SIGNS.len())]);
    }
    if rng.below(3) == 0 {
        text.push(SYMBOLS[rng.below(SYMBOLS.len())]);
    }
    if rng.below(3) == 0 {
        text.push('0');
    }
    if rng.below(2) == 0 {
        let digits = 1 + rng.below(12);
        for _ in 0..digits {
            text.push(char::from(b'0' + rng.below(10) as u8));
        }
    }
    if rng.below(3) == 0 {
        text.push(',');
    }
    if rng.below(2) == 0 {
        text.push('.');
        let digits = 1 + rng.below(12);
        for _ in 0..digits {
            text.push(char::from(b'0' + rng.below(10) as u8));
        }
    }
    if rng.below(3) == 0 {
        text.push('~');
    }
    if rng.below(4) > 0 {
        text.push(TYPES[rng.below(TYPES.len())]);
    }
    text
}

/// Whether the port refuses `fill`, per the `no-non-bmp-or-newline-fill` divergence.
fn fill_is_refused(fill: char) -> bool {
    fill as u32 > 0xFFFF || matches!(fill, '\u{2028}' | '\u{2029}') || fill.is_control()
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

#[test]
fn the_parser_accepts_exactly_the_grammar_the_reference_matcher_does() {
    let mut accepted = 0_usize;
    let mut rejected = 0_usize;
    let mut fill_refusals = 0_usize;
    let mut width_refusals = 0_usize;

    for input in corpus() {
        let units: Vec<u16> = input.encode_utf16().collect();
        let reference = reference_match(&units);
        let parsed = input.parse::<FormatSpecifier>();

        match (&reference, &parsed) {
            (Some(captures), Ok(specifier)) => {
                accepted += 1;
                assert_captures_agree(&input, captures, specifier);
            }
            (Some(captures), Err(error)) => match error.kind() {
                // The port refuses a fill d3 would pad with.
                ParseErrorKind::Fill => {
                    fill_refusals += 1;
                    let fill = char::from_u32(u32::from(captures.fill.expect("a captured fill")))
                        .expect("a captured BMP fill is always a scalar value");
                    assert!(fill_is_refused(fill), "{input:?} refused fill {fill:?}");
                }
                // The port refuses a width the syntax spells but `u32` cannot hold.
                ParseErrorKind::WidthOverflow => {
                    width_refusals += 1;
                    let digits = captures.width.as_ref().expect("a captured width");
                    assert!(
                        digits.parse::<u32>().is_err(),
                        "{input:?} refused width {digits:?}, which fits"
                    );
                }
                other => panic!("{input:?} matches the grammar but was rejected as {other:?}"),
            },
            (None, Err(error)) => {
                rejected += 1;
                // The one way the port may reject earlier than the grammar does:
                // a fill it refuses, committed before the alternatives run out.
                assert!(
                    matches!(error.kind(), ParseErrorKind::Syntax | ParseErrorKind::Fill),
                    "{input:?} was rejected as {:?}",
                    error.kind()
                );
            }
            (None, Ok(specifier)) => panic!(
                "{input:?} is not in the grammar but parsed as {}",
                specifier
            ),
        }
    }

    // A corpus that exercised only one side of the comparison would pass this test
    // while proving nothing, so the shape of the sample is asserted too.
    assert!(accepted > 5_000, "only {accepted} inputs were accepted");
    assert!(rejected > 5_000, "only {rejected} inputs were rejected");
    assert!(fill_refusals > 0, "no input exercised the fill refusal");
    assert!(width_refusals > 0, "no input exercised the width refusal");
}

/// Every field the reference captured must be the field the parser produced.
fn assert_captures_agree(input: &str, captures: &Captures, specifier: &FormatSpecifier) {
    let fill = captures
        .fill
        .map(|unit| char::from_u32(u32::from(unit)).expect("a BMP scalar value"))
        .unwrap_or(' ');
    assert_eq!(specifier.fill(), fill, "fill of {input:?}");

    let align = captures
        .align
        .map(|unit| Align::from_char(unit as u8 as char).expect("a captured alignment"))
        .unwrap_or(Align::Right);
    assert_eq!(specifier.align(), align, "align of {input:?}");

    let sign = captures
        .sign
        .map(|unit| Sign::from_char(unit as u8 as char).expect("a captured sign"))
        .unwrap_or(Sign::Minus);
    assert_eq!(specifier.sign(), sign, "sign of {input:?}");

    let symbol = captures
        .symbol
        .map(|unit| Symbol::from_char(unit as u8 as char).expect("a captured symbol"))
        .unwrap_or(Symbol::None);
    assert_eq!(specifier.symbol(), symbol, "symbol of {input:?}");

    assert_eq!(specifier.zero(), captures.zero, "zero of {input:?}");
    assert_eq!(specifier.comma(), captures.comma, "comma of {input:?}");
    assert_eq!(specifier.trim(), captures.trim, "trim of {input:?}");

    let width = captures
        .width
        .as_ref()
        .map(|digits| digits.parse::<u32>().expect("checked by the caller"));
    assert_eq!(specifier.width(), width, "width of {input:?}");

    // A precision too large to hold saturates rather than failing, because d3
    // clamps it into `[0, 21]` at format time either way.
    let precision = captures
        .precision
        .as_ref()
        .map(|digits| digits.parse::<u32>().unwrap_or(u32::MAX));
    assert_eq!(specifier.precision(), precision, "precision of {input:?}");

    let format_type = captures
        .format_type
        .map(|unit| FormatType::from_char(unit as u8 as char).expect("a captured type"))
        .unwrap_or(FormatType::None);
    assert_eq!(specifier.format_type(), format_type, "type of {input:?}");
}

#[test]
fn display_is_idempotent_and_round_trips_through_the_parser() {
    let mut checked = 0_usize;
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        checked += 1;

        let rendered = specifier.to_string();
        let reparsed: FormatSpecifier = rendered.parse().unwrap_or_else(|error| {
            panic!("{input:?} rendered {rendered:?}, which fails: {error}")
        });

        // Rendering twice is rendering once: the canonical form is a fixed point.
        assert_eq!(
            reparsed.to_string(),
            rendered,
            "{input:?} did not render to a fixed point"
        );

        if specifier.width() == Some(0) {
            // The one lossy field, and d3 is lossy in the same place:
            // `Math.max(1, this.width | 0)` cannot spell a width of zero.
            assert_eq!(reparsed.width(), Some(1), "{input:?}");
            assert_eq!(
                reparsed.to_builder().width(0).build().expect("valid"),
                specifier,
                "{input:?} differs from its rendering in more than the width"
            );
        } else {
            assert_eq!(reparsed, specifier, "{input:?} did not survive rendering");
        }
    }
    assert!(checked > 5_000, "only {checked} specifiers were rendered");
}

#[test]
fn a_rendered_specifier_is_always_in_the_grammar() {
    // The reference matcher is the judge here too: `Display` must not be able to
    // emit something `FromStr` would have to be lenient about.
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let rendered = specifier.to_string();
        let units: Vec<u16> = rendered.encode_utf16().collect();
        assert!(
            reference_match(&units).is_some(),
            "{input:?} rendered {rendered:?}, which is not in d3's grammar"
        );
    }
}

#[test]
fn formatter_for_is_exactly_formatter_of_the_rendered_specifier() {
    // Section 5.2 requires the equivalence and forbids bypassing normalization.
    // Comparing the compiled formatters rather than their inputs is what makes this
    // a check of the *result*: `Formatter`'s `PartialEq` covers the locale, the
    // stored specifier, the input kind, the limits, and the whole resolved layout,
    // so a `formatter_for` that skipped a normalization step would differ here even
    // if it happened to agree on the specifier it reports.
    let locales = [
        Locale::en_us(),
        Locale::builder().build().expect("valid"),
        Locale::builder()
            .decimal(",")
            .thousands(".")
            .grouping([3])
            .currency("", "\u{a0}\u{20ac}")
            .percent("\u{202f}%")
            .build()
            .expect("valid"),
    ];

    let mut compiled = 0_usize;
    let mut refused = 0_usize;
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        for locale in &locales {
            let direct = locale.formatter_for(&specifier);
            let rendered = locale.formatter(&specifier.to_string());
            assert_eq!(
                direct, rendered,
                "{input:?} compiled differently through formatter_for"
            );

            match direct {
                Ok(formatter) => {
                    compiled += 1;
                    // Normalization was not bypassed: the formatter reports the
                    // specifier the rendering re-parsed to, which is the original
                    // except where `Display` is lossy.
                    let expected = if specifier.width() == Some(0) {
                        specifier.to_builder().width(1).build().expect("valid")
                    } else {
                        specifier.clone()
                    };
                    assert_eq!(formatter.specifier(), &expected, "{input:?}");
                }
                Err(error) => {
                    refused += 1;
                    // The only construction failure a validated specifier can reach.
                    assert!(
                        matches!(error, FormatError::WidthLimit { .. }),
                        "{input:?} failed to compile: {error:?}"
                    );
                }
            }
        }
    }
    assert!(compiled > 5_000, "only {compiled} formatters were compiled");
    assert!(refused > 0, "no specifier exercised the width limit");
}

#[test]
fn the_reference_matcher_agrees_with_node_on_the_documented_examples() {
    // The reference matcher is only worth comparing against if it is itself right,
    // so it is pinned to the cases the oracle suite states outright.
    let expect_match = |text: &str| {
        let units: Vec<u16> = text.encode_utf16().collect();
        reference_match(&units).unwrap_or_else(|| panic!("{text:?} should match d3's pattern"))
    };
    let expect_no_match = |text: &str| {
        let units: Vec<u16> = text.encode_utf16().collect();
        assert!(
            reference_match(&units).is_none(),
            "{text:?} should not match d3's pattern"
        );
    };

    // `test/format-test.js` and `test/formatSpecifier-test.js` assert these throw.
    expect_no_match("foo");
    expect_no_match(".-2s");
    expect_no_match(".f");

    // `format("012")` is `format("0=12")`: the zero flag takes the first digit.
    let twelve = expect_match("012");
    assert!(twelve.zero);
    assert_eq!(twelve.width.as_deref(), Some("12"));

    // A supplementary-plane fill cannot be captured: `.` would take only the high
    // surrogate, and the alignment would have to be the low surrogate.
    expect_no_match("\u{1f600}>d");
    // A line terminator cannot be captured either, because `.` excludes it.
    expect_no_match("\u{2028}>d");
    // A tab can, which is the divergence the port introduces on purpose.
    let tab = expect_match("\t>d");
    assert_eq!(tab.fill, Some(u16::from(b'\t')));

    // The full specifier, with every group present.
    let full = expect_match("_^+$012,.2~f");
    assert_eq!(full.fill, Some(u16::from(b'_')));
    assert_eq!(full.align, Some(u16::from(b'^')));
    assert_eq!(full.sign, Some(u16::from(b'+')));
    assert_eq!(full.symbol, Some(u16::from(b'$')));
    assert!(full.zero);
    assert_eq!(full.width.as_deref(), Some("12"));
    assert!(full.comma);
    assert_eq!(full.precision.as_deref(), Some("2"));
    assert!(full.trim);
    assert_eq!(full.format_type, Some(u16::from(b'f')));
}

#[test]
fn no_corpus_input_panics_anywhere_in_the_phase_3_surface() {
    // Section 5.4: no accepted input may trigger a panic or an integer wrap. Value
    // layout is Phase 4 and diverges on purpose, so this covers everything up to
    // it: parsing, rendering, compiling, and the input-kind gate.
    let locale = Locale::en_us();
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let _ = specifier.to_string();
        let _ = specifier.to_builder().build();
        match locale.formatter_for(&specifier) {
            Err(_) => continue,
            Ok(formatter) => {
                // The wrong-kind path returns rather than panicking; the right-kind
                // path is the Phase 4 seam and is not called.
                let wrong = match formatter.input_kind() {
                    d3_format::InputKind::Number => formatter.format_text("x"),
                    d3_format::InputKind::Text => formatter.format_number(0.0),
                };
                assert!(matches!(wrong, Err(FormatError::InputTypeMismatch { .. })));
            }
        }
    }
}
