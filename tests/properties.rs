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
//!
//! # The formatting half
//!
//! Everything from [`SWEEP_LIMITS`] onwards is about what happens *after* a
//! specifier parses. Section 3.6's differential already compares the port's output
//! against d3's byte for byte, so nothing below re-checks a digit; what it checks is
//! the structure the differential cannot see, because the differential only ever
//! looks at one string at a time. That the owned and the reusing entry point produce
//! the same string. That a padded result is the unpadded one with a run of the fill
//! inserted where the alignment says. That a trimmed fraction never ends in a zero.
//! That numeral substitution reaches every ASCII digit in the assembled string. That
//! no accepted input can produce an output larger than the limits admit, which is
//! the allocation half of the `construction-time-width-limits` divergence.
//!
//! Where a property has an independent implementation to be compared against, it
//! gets one, exactly as the parser does above: grouping is checked against a
//! transcription of `src/formatGroup.js` rather than against the layout's own
//! reading of it.

use d3_format::{
    as_precision, precision_fixed, precision_prefix, precision_round, Align, FormatError,
    FormatLimits, FormatSpecifier, FormatType, InputKind, Locale, ParseErrorKind, Sign, Symbol,
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

/// Values chosen so that every branch of the layout is reached by something.
///
/// The three sign cases including both zeros, the two infinities, both NaN sign
/// bits, the subnormal and normal extremes, the exponent boundaries where notation
/// switches, and the SI table's ends and one step past each of them.
const AWKWARD_VALUES: [f64; 34] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NAN,
    -f64::NAN,
    f64::MIN,
    f64::MAX,
    f64::MIN_POSITIVE,
    -f64::MIN_POSITIVE,
    5e-324,
    -5e-324,
    f64::EPSILON,
    0.1,
    0.5,
    -0.5,
    1.5,
    2.5,
    9.995,
    0.000_001,
    0.000_000_1,
    999_999.5,
    1e6,
    1e21,
    1e-7,
    1.797_693_134_862_315_7e308,
    1e24,
    1e25,
    -1e25,
    1e-24,
    1e-25,
    4_294_967_296.0,
];

#[test]
fn no_corpus_input_panics_anywhere_in_the_public_surface() {
    // Section 5.4: no accepted input may trigger a panic or an integer wrap, for
    // any accepted specifier, locale or `f64` bit pattern. The corpus supplies the
    // specifiers, `AWKWARD_VALUES` the bit patterns, and every accepted pair is
    // formatted for real -- parse, render, compile, the input-kind gate, and the
    // layout behind it.
    //
    // The width limit is lowered from the default million so that the corpus's
    // deliberately huge widths are refused at compile time instead of allocating a
    // megabyte apiece; `the_width_limit_is_enforced_before_allocating` in
    // `tests/public_api.rs` covers the wide path, and
    // `a_pathological_width_still_produces_the_right_string` below covers it here.
    let limits = FormatLimits {
        max_width_utf16: 4096,
        max_output_bytes: 1 << 20,
    };
    let locale = Locale::en_us();
    let mut out = String::new();
    let mut specifiers = 0usize;
    let mut formatted = 0usize;
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let _ = specifier.to_string();
        let _ = specifier.to_builder().build();
        let Ok(formatter) = locale.formatter_for_with_limits(&specifier, limits) else {
            continue;
        };
        specifiers += 1;

        // The wrong-kind path returns rather than panicking.
        let wrong = match formatter.input_kind() {
            InputKind::Number => formatter.format_text("x"),
            InputKind::Text => formatter.format_number(0.0),
        };
        assert!(matches!(wrong, Err(FormatError::InputTypeMismatch { .. })));

        match formatter.input_kind() {
            InputKind::Text => {
                // A grapheme cluster, an astral pair and a lone combining mark: the
                // three shapes that make UTF-16 width differ from bytes and chars.
                for text in ["", "x", "e\u{301}", "\u{1f600}", "\u{0301}"] {
                    formatter
                        .format_text_into(&mut out, text)
                        .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                    formatted += 1;
                }
            }
            InputKind::Number => {
                for value in AWKWARD_VALUES {
                    formatter
                        .format_number_into(&mut out, value)
                        .unwrap_or_else(|error| {
                            panic!("{specifier} on {:016X}: {error}", value.to_bits())
                        });
                    formatted += 1;
                }
            }
        }
    }
    eprintln!(
        "d3-format: {formatted} values laid out through {specifiers} accepted specifiers, \
no panic and no error"
    );
}

/// The references the `formatPrefix` sweep uses.
///
/// A ninth of `AWKWARD_VALUES` because that sweep is quadratic -- every reference is
/// crossed with every value, for every accepted specifier in the corpus -- and these
/// are the ones that select a different code path rather than a different digit: all
/// four degenerate references, both signs of a real bucket, and the two ends of the
/// SI table.
const PREFIX_REFERENCES: [f64; 9] = [
    0.0,
    -0.0,
    f64::NAN,
    f64::INFINITY,
    1e6,
    -1e6,
    1.0,
    1e24,
    1e-24,
];

#[test]
fn no_corpus_input_panics_through_a_prefix_formatter() {
    // `formatPrefix` is the other entry point, and section 4.1's degenerate
    // references -- the ones whose scale is NaN -- are the reason it needs its own
    // sweep: nothing there may index the SI table with a NaN exponent.
    let limits = FormatLimits {
        max_width_utf16: 4096,
        max_output_bytes: 1 << 20,
    };
    let locale = Locale::en_us();
    let mut out = String::new();
    let mut formatted = 0usize;
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let rendered = specifier.to_string();
        for reference in PREFIX_REFERENCES {
            let compiled = locale.prefix_formatter_with_limits(&rendered, reference, limits);
            let Ok(formatter) = compiled else { continue };
            // `formatPrefix` forces type `f`, so the formatter is always numeric.
            assert_eq!(formatter.input_kind(), InputKind::Number);
            for value in PREFIX_REFERENCES {
                formatter
                    .format_number_into(&mut out, value)
                    .unwrap_or_else(|error| {
                        panic!(
                            "{specifier} @ {:016X} on {:016X}: {error}",
                            reference.to_bits(),
                            value.to_bits()
                        )
                    });
                formatted += 1;
            }
        }
    }
    eprintln!(
        "d3-format: {formatted} values laid out through prefix formatters, \
no panic and no error"
    );
}

#[test]
fn a_pathological_width_still_produces_the_right_string() {
    // The wide path the sweep above declines to walk 40,000 times, walked once.
    let formatter = Locale::en_us().formatter("*^999999.2f").expect("valid");
    let formatted = formatter.format_number(-1.5).expect("valid");
    assert_eq!(formatted.chars().count(), 999_999);
    assert_eq!(formatted.matches('*').count(), 999_994);
    assert!(formatted.contains("\u{2212}1.50"));

    // And one code unit further is refused rather than truncated or aborted.
    let over = Locale::en_us().formatter_with_limits(
        "999999f",
        FormatLimits {
            max_width_utf16: 999_998,
            max_output_bytes: 1 << 20,
        },
    );
    assert!(matches!(over, Err(FormatError::WidthLimit { .. })));
}

// ---------------------------------------------------------------------------
// Formatting properties
// ---------------------------------------------------------------------------

/// The limits the layout sweeps below run under.
///
/// The same pair the two sweeps above use, and for the same reason: the default
/// million-code-unit width would let the corpus's twelve-digit widths allocate a
/// megabyte of padding apiece. [`a_pathological_width_still_produces_the_right_string`]
/// walks the wide path once, on purpose.
const SWEEP_LIMITS: FormatLimits = FormatLimits {
    max_width_utf16: 4096,
    max_output_bytes: 1 << 20,
};

/// Text inputs for the type `c` half of the sweeps.
///
/// A grapheme cluster, an astral pair and a lone combining mark are the three shapes
/// that make UTF-16 length differ from bytes and from `char`s. The last two carry
/// ASCII digits, which numeral substitution has to reach even though they are text
/// the port otherwise promises to preserve byte for byte.
const TEXT_SAMPLES: [&str; 7] = [
    "",
    "x",
    "e\u{301}",
    "\u{1f600}",
    "\u{301}",
    "2024",
    "-1.5e+3",
];

/// Whether value `index` is exercised for the specifier at `position`.
///
/// The full cross product of the corpus and [`AWKWARD_VALUES`] is half a million
/// pairs, and several properties below measure each pair twice. Offsetting the
/// residue by the specifier's position keeps every value in the sweep -- each is
/// reached by a `stride`th of the specifiers, and each specifier sees a different
/// subset from its neighbour -- while dividing the work by `stride`. Every test that
/// samples asserts how many pairs it actually reached, so a corpus that silently
/// shrank would fail rather than quietly check less.
fn sampled(position: usize, index: usize, stride: usize) -> bool {
    (position + index) % stride == 0
}

/// The UTF-16 code units of `text`, which is the unit d3 measures a width in.
fn code_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// What an output buffer carries into a `*_into` call.
///
/// Section 5.3 has the call clear the buffer before doing any work, so every one of
/// these must be gone from every result. Handing in an empty buffer instead would
/// check nothing.
const JUNK: &str = "left over from the last caller";

/// Whether every code unit in `units` is the fill character.
///
/// The parser refuses a fill outside the Basic Multilingual Plane, so one fill is
/// one code unit and a padding run is a run of equal units; the assertion inside
/// says so rather than assuming it.
fn all_fill(units: &[u16], fill: char) -> bool {
    let mut buffer = [0u16; 2];
    let encoded = fill.encode_utf16(&mut buffer);
    assert_eq!(encoded.len(), 1, "fill {fill:?} is not one code unit");
    units.iter().all(|unit| *unit == encoded[0])
}

#[test]
fn owned_and_reused_output_are_the_same_string() {
    // Section 5.3 gives `format_number` and `format_number_into` as two spellings of
    // one operation, and the Phase 5 benchmark reports them as two measured paths.
    // Nothing in the differential can tell them apart: it compares one of them
    // against d3 and never the two against each other. A reusing path that forgot to
    // clear, or that took a shortcut for a buffer that already had the capacity it
    // needed, would leave the benchmark comparing two different operations and the
    // differential none the wiser.
    //
    // The buffer carries junk into every call and is never emptied by the test, so a
    // call that appended instead of clearing is caught on the first value and on
    // every value after it.
    let locale = Locale::en_us();
    let mut out = String::new();
    let mut capacity = out.capacity();
    let mut numeric = 0usize;
    let mut textual = 0usize;

    for (position, input) in corpus().into_iter().enumerate() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let Ok(formatter) = locale.formatter_for_with_limits(&specifier, SWEEP_LIMITS) else {
            continue;
        };
        match formatter.input_kind() {
            InputKind::Number => {
                for (index, value) in AWKWARD_VALUES.iter().enumerate() {
                    if !sampled(position, index, 2) {
                        continue;
                    }
                    let owned = formatter
                        .format_number(*value)
                        .unwrap_or_else(|error| panic!("{specifier} on {value}: {error}"));
                    out.push_str(JUNK);
                    formatter
                        .format_number_into(&mut out, *value)
                        .unwrap_or_else(|error| panic!("{specifier} on {value}: {error}"));
                    assert_eq!(
                        out,
                        owned,
                        "{specifier} on {:016X} differs between the owned and reused paths",
                        value.to_bits()
                    );
                    numeric += 1;
                }
            }
            InputKind::Text => {
                for (index, text) in TEXT_SAMPLES.iter().enumerate() {
                    if !sampled(position, index, 2) {
                        continue;
                    }
                    let owned = formatter
                        .format_text(text)
                        .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                    out.push_str(JUNK);
                    formatter
                        .format_text_into(&mut out, text)
                        .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                    assert_eq!(
                        out, owned,
                        "{specifier} on {text:?} differs between the owned and reused paths"
                    );
                    textual += 1;
                }
            }
        }

        // Capacity is the other half of the section 5.3 claim. It can only grow:
        // `clear` keeps it and `try_reserve` never gives it back, so a call that
        // rebuilt the buffer instead of reusing it would show up as a drop here.
        assert!(
            out.capacity() >= capacity,
            "{specifier} shrank the caller's buffer from {capacity} to {}",
            out.capacity()
        );
        capacity = out.capacity();
    }

    assert!(
        numeric > 200_000,
        "only {numeric} numeric pairs were compared"
    );
    assert!(textual > 500, "only {textual} text pairs were compared");
    eprintln!(
        "d3-format: {numeric} numeric and {textual} text results compared owned against reused"
    );
}

/// One bounded call, checked against the budget it was given.
///
/// Returns whether the result fitted. `owned` re-runs the same call through the
/// allocating entry point, and is only consulted on the refusal path.
fn assert_bounded(
    what: &str,
    outcome: Result<(), FormatError>,
    out: &str,
    limits: FormatLimits,
    width: u32,
    owned: impl FnOnce() -> Result<String, FormatError>,
) -> bool {
    match outcome {
        Ok(()) => {
            assert!(
                out.len() <= limits.max_output_bytes,
                "{what} produced {} bytes, over the {} it was given",
                out.len(),
                limits.max_output_bytes
            );
            // A width the formatter accepted is a width the result reaches: the
            // padding cannot have been quietly dropped to stay inside the budget.
            assert!(
                code_units(out).len() >= width as usize,
                "{what} produced {out:?}, narrower than its width"
            );
            true
        }
        Err(FormatError::OutputLimit { requested, maximum }) => {
            assert_eq!(maximum, limits.max_output_bytes, "{what}");
            assert!(
                requested > maximum,
                "{what} refused {requested} bytes, which fits"
            );
            // Section 5.3: nothing partial is left behind, so a caller that ignores
            // the error reads an empty buffer rather than a truncated number.
            assert!(out.is_empty(), "{what} left {out:?} behind");
            // Both entry points refuse, and refuse identically. The agreement on the
            // *success* path is what `owned_and_reused_output_are_the_same_string`
            // sweeps; what that test cannot reach is a failure, because it runs under
            // limits nothing in the corpus exceeds.
            assert_eq!(
                owned(),
                Err(FormatError::OutputLimit { requested, maximum }),
                "{what} disagrees between its two entry points"
            );
            false
        }
        Err(error) => panic!("{what} failed as {error:?}, which is not a limit"),
    }
}

#[test]
fn no_accepted_input_produces_an_unbounded_allocation() {
    // Section 5.4, and the allocation half of the `construction-time-width-limits`
    // entry in DIVERGENCES.md. The width half is settled at construction and is
    // checked by the parser sweep above; this is the other half, and it is the one
    // that needed the layout path to exist before it could be written: that for
    // every accepted specifier and every `f64` bit pattern, the output either fits
    // the declared budget or is refused by a typed error naming that budget. Never a
    // panic, never a silent truncation, and never a success larger than the caller
    // agreed to.
    //
    // The budget is deliberately far too small. At 48 bytes a `.20f` of `1e308`, a
    // binary expansion of `f64::MAX` and a padded currency amount all overflow it,
    // so the refusal path is the common case rather than a corner reached once.
    const TIGHT: FormatLimits = FormatLimits {
        max_width_utf16: 32,
        max_output_bytes: 48,
    };

    let locale = Locale::en_us();
    let mut out = String::new();
    let mut within = 0usize;
    let mut refused = 0usize;
    let mut too_wide = 0usize;

    for (position, input) in corpus().into_iter().enumerate() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let formatter = match locale.formatter_for_with_limits(&specifier, TIGHT) {
            Ok(formatter) => formatter,
            Err(FormatError::WidthLimit { requested, maximum }) => {
                too_wide += 1;
                assert_eq!(maximum, TIGHT.max_width_utf16, "{specifier}");
                assert!(
                    requested > maximum,
                    "{specifier} was refused a width it fits"
                );
                continue;
            }
            Err(error) => panic!("{specifier} failed to compile: {error:?}"),
        };
        let width = specifier.width().unwrap_or(0);

        match formatter.input_kind() {
            InputKind::Number => {
                for (index, value) in AWKWARD_VALUES.iter().enumerate() {
                    if !sampled(position, index, 2) {
                        continue;
                    }
                    out.push_str(JUNK);
                    let outcome = formatter.format_number_into(&mut out, *value);
                    let label = format!("{specifier} on {:016X}", value.to_bits());
                    let fitted = assert_bounded(&label, outcome, &out, TIGHT, width, || {
                        formatter.format_number(*value)
                    });
                    if fitted {
                        within += 1;
                    } else {
                        refused += 1;
                    }
                }
            }
            InputKind::Text => {
                // Type `c` counts its text against the same budget, and a text longer
                // than the whole budget is the only way to reach the refusal through
                // that entry point, since the samples are all short.
                let long = "x".repeat(200);
                for text in TEXT_SAMPLES.iter().copied().chain([long.as_str()]) {
                    out.push_str(JUNK);
                    let outcome = formatter.format_text_into(&mut out, text);
                    let label = format!("{specifier} on {} bytes of text", text.len());
                    let fitted = assert_bounded(&label, outcome, &out, TIGHT, width, || {
                        formatter.format_text(text)
                    });
                    if fitted {
                        within += 1;
                    } else {
                        refused += 1;
                    }
                }
            }
        }
    }

    assert!(
        within > 100_000,
        "only {within} results stayed inside the budget"
    );
    assert!(refused > 10_000, "only {refused} results were refused");
    assert!(
        too_wide > 1_000,
        "only {too_wide} specifiers were too wide to compile"
    );
    eprintln!(
        "d3-format: {within} results within a 48-byte budget, {refused} refused by it, \
{too_wide} specifiers refused a width over 32"
    );
}

/// What one padded result turned out to be worth checking.
#[derive(Debug, Clone, Copy)]
enum Padded {
    /// A padding run was pinned in the position this alignment puts it.
    Run(Align),
    /// The natural result already reached the width, so the two had to be equal.
    Nothing,
    /// The zero-grouping escape: only the width floor was checked.
    Regrouped,
}

/// How many results of each kind a sweep produced.
///
/// Tallied and asserted because "the padding is where the alignment says" is only
/// worth anything if every alignment was reached: a corpus that stopped producing
/// centred specifiers would leave one quarter of the rule unchecked and still pass.
#[derive(Debug, Default)]
struct Tally {
    /// Padding runs pinned, indexed by `<`, `>`, `^`, `=` in that order.
    runs: [usize; 4],
    nothing: usize,
    regrouped: usize,
}

impl Tally {
    fn record(&mut self, padded: Padded) {
        match padded {
            Padded::Run(Align::Left) => self.runs[0] += 1,
            Padded::Run(Align::Right) => self.runs[1] += 1,
            Padded::Run(Align::Center) => self.runs[2] += 1,
            Padded::Run(Align::AfterSign) => self.runs[3] += 1,
            Padded::Nothing => self.nothing += 1,
            Padded::Regrouped => self.regrouped += 1,
        }
    }
}

/// The width invariant, for one specifier under one locale.
fn check_width(
    locale: &Locale,
    position: usize,
    specifier: &FormatSpecifier,
    stride: usize,
    tally: &mut Tally,
) {
    let Some(width) = specifier.width().filter(|width| *width > 0) else {
        return;
    };
    let Ok(formatter) = locale.formatter_for_with_limits(specifier, SWEEP_LIMITS) else {
        return;
    };
    let unpadded = specifier
        .to_builder()
        .width(None)
        .build()
        .expect("dropping a width cannot invalidate a specifier");
    let unpadded = locale
        .formatter_for_with_limits(&unpadded, SWEEP_LIMITS)
        .expect("a specifier with no width is inside every width limit");

    // `newFormat`'s first two resolutions, restated from outside the crate: fill `0`
    // with align `=` *is* the zero flag, and type `n` *is* a comma. Both decide what
    // the padding looks like, so a test that read the specifier's own fields would
    // check the wrong shape for `0=12d` and for `012n`.
    let zero =
        specifier.zero() || (specifier.fill() == '0' && specifier.align() == Align::AfterSign);
    let (fill, align) = if zero {
        ('0', Align::AfterSign)
    } else {
        (specifier.fill(), specifier.align())
    };
    let comma = specifier.comma() || specifier.format_type() == FormatType::Grouped;
    let regrouped = comma && zero && locale.grouping().is_some();

    match formatter.input_kind() {
        InputKind::Number => {
            for (index, value) in AWKWARD_VALUES.iter().enumerate() {
                if !sampled(position, index, stride) {
                    continue;
                }
                let padded = formatter
                    .format_number(*value)
                    .unwrap_or_else(|error| panic!("{specifier} on {value}: {error}"));
                let natural = unpadded
                    .format_number(*value)
                    .unwrap_or_else(|error| panic!("{specifier} on {value}: {error}"));
                let label = format!("{specifier} on {:016X}", value.to_bits());
                tally.record(assert_padding(
                    &label, &padded, &natural, width, fill, align, regrouped,
                ));
            }
        }
        InputKind::Text => {
            let empty = unpadded.format_text("").expect("type c takes any text");
            for (index, text) in TEXT_SAMPLES.iter().enumerate() {
                if !sampled(position, index, stride) {
                    continue;
                }
                let padded = formatter
                    .format_text(text)
                    .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                let natural = unpadded
                    .format_text(text)
                    .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                // Section 5.3: type `c` preserves its text byte for byte, and only
                // the affixes, the width and the alignment touch the result. Removing
                // the text once must therefore leave exactly what the empty string
                // produced -- which is a stronger statement than `contains`, because
                // it also says nothing else was added.
                if !text.is_empty() {
                    assert_eq!(
                        natural.replacen(text, "", 1),
                        empty,
                        "{specifier} did not preserve {text:?}"
                    );
                }
                let label = format!("{specifier} on {text:?}");
                tally.record(assert_padding(
                    &label, &padded, &natural, width, fill, align, regrouped,
                ));
            }
        }
    }
}

/// A successful result is `max(width, natural)` code units long, and the difference
/// is a run of the fill where the alignment puts it.
fn assert_padding(
    what: &str,
    padded: &str,
    natural: &str,
    width: u32,
    fill: char,
    align: Align,
    regrouped: bool,
) -> Padded {
    let padded_units = code_units(padded);
    let natural_units = code_units(natural);
    let padding = (width as usize).saturating_sub(natural_units.len());

    if regrouped {
        // `if (comma && zero) value = group(padding + value, padding.length ? width -
        // valueSuffix.length : Infinity)`. Two things about that statement put it
        // outside the rule below, and both are d3's.
        //
        // The padding is decided *before* the grouping rather than after, so it is
        // measured against the ungrouped digits and the separators are added on top
        // of a run already sized to the width. And the group budget then drops
        // leading fill once it runs out, counting every separator as one unit
        // whatever it spells. The result is a width that overshoots by an amount no
        // relation between the padded and unpadded strings predicts -- the private
        // test `layout::tests::grouping_is_d3s_substring_walk` records eight zero-fill
        // digits reaching nine code units against a budget of eight, and
        // `tests/layout.rs::numerals_are_substituted_over_the_assembled_string`
        // records the same path landing exactly on twenty.
        //
        // What survives is the floor, which is the half a caller relies on: a width
        // is a minimum, and this path never pads to less than one.
        assert!(
            padded_units.len() >= width as usize,
            "{what}: {padded:?} is narrower than the {width} it asked for"
        );
        return Padded::Regrouped;
    }

    assert_eq!(
        padded_units.len(),
        natural_units.len().max(width as usize),
        "{what}: {padded:?} is not max(width, natural) code units long"
    );
    if padding == 0 {
        assert_eq!(padded, natural, "{what}: padded when nothing was needed");
        return Padded::Nothing;
    }

    match align {
        Align::Right => {
            assert!(
                all_fill(&padded_units[..padding], fill),
                "{what}: {padded:?} is not padded with {fill:?} on the left"
            );
            assert_eq!(
                &padded_units[padding..],
                &natural_units[..],
                "{what}: {padded:?} is not {natural:?} behind its padding"
            );
        }
        Align::Left => {
            let body = padded_units.len() - padding;
            assert_eq!(
                &padded_units[..body],
                &natural_units[..],
                "{what}: {padded:?} is not {natural:?} in front of its padding"
            );
            assert!(
                all_fill(&padded_units[body..], fill),
                "{what}: {padded:?} is not padded with {fill:?} on the right"
            );
        }
        Align::Center => {
            // `padding.slice(0, length = padding.length >> 1)` and the rest: an odd
            // run puts the extra character on the right.
            let head = padding >> 1;
            let body = padded_units.len() - (padding - head);
            assert!(
                all_fill(&padded_units[..head], fill),
                "{what}: {padded:?} is not centred in {fill:?}"
            );
            assert_eq!(
                &padded_units[head..body],
                &natural_units[..],
                "{what}: {padded:?} is not {natural:?} between its padding"
            );
            assert!(
                all_fill(&padded_units[body..], fill),
                "{what}: {padded:?} is not centred in {fill:?}"
            );
        }
        Align::AfterSign => {
            // The run goes between the sign and the digits, and the sign is the
            // locale's text rather than a character this test can name, so the
            // position is pinned by construction instead: the padded result must be
            // the natural one with a single run of the fill inserted into it. Taking
            // the longest common prefix finds that run wherever it is, and the two
            // assertions together admit no other reading.
            let shared = padded_units
                .iter()
                .zip(&natural_units)
                .take_while(|(padded, natural)| padded == natural)
                .count();
            assert!(
                all_fill(&padded_units[shared..shared + padding], fill),
                "{what}: {padded:?} is not {natural:?} with a run of {fill:?} inserted"
            );
            assert_eq!(
                &padded_units[shared + padding..],
                &natural_units[shared..],
                "{what}: {padded:?} is not {natural:?} with a run of {fill:?} inserted"
            );
        }
    }
    Padded::Run(align)
}

#[test]
fn a_width_is_reached_exactly_and_padded_with_the_resolved_fill() {
    // Sections 5.3 and 4.4: a width is a number of UTF-16 code units, and it is a
    // minimum rather than a field size. The generated suite pins the padded strings
    // d3 was recorded producing; what it cannot say is that the rule holds for a
    // specifier nobody recorded. Stated as a relation between the padded result and
    // the unpadded one, it can be checked for every accepted specifier in the corpus
    // without a single hand-written expectation -- and it catches the two mistakes a
    // rewrite of the padding actually makes: counting bytes or `char`s instead of
    // code units, and putting the run on the wrong side of the sign.
    let locale = Locale::en_us();
    let mut tally = Tally::default();
    for (position, input) in corpus().into_iter().enumerate() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        check_width(&locale, position, &specifier, 2, &mut tally);
    }

    // Affix text counts toward the width like any other output, which is only
    // visible in a locale whose affixes are not ASCII: `\u{2796}` is one code unit,
    // `\u{a0}\u{20ac}` is two, and the narrow no-break space in front of the percent
    // sign is one more. A port measuring bytes would pad these three too little.
    let unicode = Locale::builder()
        .decimal(",")
        .thousands("\u{202f}")
        .grouping([3])
        .currency("", "\u{a0}\u{20ac}")
        .percent("\u{202f}%")
        .minus("\u{2796}")
        .nan("N/A")
        .build()
        .expect("valid");
    for (position, spec) in [
        "$12,.2f",
        "<$12.2f",
        "^$13.2f",
        "=$12.2f",
        "\u{2212}>12.1%",
        "$012,.2f",
        "12,.3s",
        "020c",
        "\u{2796}^11c",
    ]
    .into_iter()
    .enumerate()
    {
        let specifier: FormatSpecifier = spec.parse().expect("valid");
        check_width(&unicode, position, &specifier, 1, &mut tally);
    }

    let runs: usize = tally.runs.iter().sum();
    for (alignment, count) in ['<', '>', '^', '='].iter().zip(&tally.runs) {
        assert!(
            *count > 100,
            "only {count} padding runs were checked for alignment {alignment:?}"
        );
    }
    assert!(runs > 30_000, "only {runs} padding runs were pinned");
    assert!(
        tally.regrouped > 100,
        "only {} took the zero-grouping path",
        tally.regrouped
    );
    eprintln!(
        "d3-format: {runs} padding runs pinned as {:?} for < > ^ =, {} results needed none, \
{} held to the width floor through zero-fill grouping",
        tally.runs, tally.nothing, tally.regrouped
    );
}

/// The digits between the first decimal point and whatever ends the fraction.
fn fraction_of<'a>(text: &'a str, decimal: &str) -> Option<&'a str> {
    let at = text.find(decimal)? + decimal.len();
    let rest = &text[at..];
    let end = rest
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Values chosen to end in an insignificant zero under some precision.
const TRIM_VALUES: [f64; 12] = [
    1.0,
    1.05,
    1.5,
    100.0,
    0.25,
    0.000_010_5,
    1.000_000_1,
    123_456.7,
    1.2e21,
    9.999_5,
    1.048_576e6,
    0.1 + 0.2,
];

#[test]
fn a_trimmed_result_never_ends_in_an_insignificant_zero() {
    // `~` promises that the fraction carries no digit d3 considers insignificant, and
    // `src/formatTrim.js` keeps that promise with a two-position scan that is easy to
    // get subtly wrong: one position too far and a significant digit leaves with the
    // zeros, one too few and a zero survives. The generated suite pins the trims that
    // were recorded; this pins the invariant, over the specifiers nobody recorded and
    // over the three tails that arrive *after* the trim and must not be mistaken for
    // part of it -- the percent sign, the SI prefix and the currency suffix.
    //
    // Every specifier loses its width first. Padding would otherwise decide the
    // question and decide it wrongly: `format("0<10~f")(1.5)` is `"1.50000000"`,
    // whose trailing zeros are fill rather than digits.
    //
    // The locale is en-US, so the decimal point is `.` and the group separator is
    // `,`: the first `.` in the result is always the decimal point, never a
    // separator, and never a fill character.
    let locale = Locale::en_us();
    let mut checked = 0usize;
    let mut fractions = 0usize;
    for (position, input) in corpus().into_iter().enumerate() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        // Trimming is on for `~`, and also for the empty type and any unknown letter,
        // which `newFormat` resolves to `.12~g`. `n` is rewritten a line earlier and
        // never reaches the fallback.
        let format_type = specifier.format_type();
        let trims =
            specifier.trim() || (format_type != FormatType::Grouped && !format_type.is_known());
        if !trims {
            continue;
        }
        let unpadded = specifier
            .to_builder()
            .width(None)
            .build()
            .expect("dropping a width cannot invalidate a specifier");
        let formatter = locale
            .formatter_for_with_limits(&unpadded, SWEEP_LIMITS)
            .expect("a specifier with no width is inside every width limit");
        if formatter.input_kind() == InputKind::Text {
            continue;
        }
        for (index, value) in AWKWARD_VALUES.iter().chain(&TRIM_VALUES).enumerate() {
            if !sampled(position, index, 3) {
                continue;
            }
            let formatted = formatter
                .format_number(*value)
                .unwrap_or_else(|error| panic!("{unpadded} on {value}: {error}"));
            checked += 1;
            let Some(fraction) = fraction_of(&formatted, locale.decimal()) else {
                continue;
            };
            fractions += 1;
            assert!(
                !fraction.ends_with('0'),
                "{unpadded} on {:016X} produced {formatted:?}, whose fraction still ends in a zero",
                value.to_bits()
            );
            // A decimal point with nothing behind it is the other way the scan can
            // go wrong: `formatTrim` removes the point along with the run it opened.
            assert!(
                !fraction.is_empty(),
                "{unpadded} on {:016X} produced {formatted:?}, a decimal point with no fraction",
                value.to_bits()
            );
        }
    }
    assert!(
        checked > 20_000,
        "only {checked} trimmed results were checked"
    );
    assert!(
        fractions > 2_000,
        "only {fractions} of them kept a fraction"
    );
    eprintln!("d3-format: {checked} trimmed results checked, {fractions} of them with a fraction");
}

/// A locale whose ten numerals are the Arabic-Indic digits.
///
/// Built here rather than taken from the generated table because the property has
/// nothing to do with the `locales` feature and has to hold in all four feature
/// combinations. Every string in it is free of ASCII digits, which is what makes the
/// assertion below a statement about the substitution rather than about the locale.
fn arabic_numerals() -> Locale {
    Locale::builder()
        .decimal("\u{66b}")
        .thousands("\u{66c}")
        .grouping([3])
        .currency("", " \u{62f}.\u{625}.")
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

#[test]
fn numeral_substitution_reaches_every_ascii_digit_in_the_result() {
    // Section 4.4: `formatNumerals` runs last, over the whole assembled string. The
    // consequence is not that the digits are translated -- any implementation does
    // that -- but that the translation reaches text no digit-emitting routine ever
    // touches: the `0` of a `0x` base prefix, a `0` used as a fill character, the
    // digits inside type `c`'s text, and the exponent of an `e`-notation result.
    // `tests/layout.rs::numerals_are_substituted_over_the_assembled_string` pins the
    // four examples section 4.4 names; this says the same thing as a closed
    // property, over every accepted specifier in the corpus, and it is closed in a
    // useful way -- an implementation that substituted before padding, or that
    // substituted only the value part, leaves an ASCII digit behind *somewhere*, and
    // the somewhere is what varies from bug to bug.
    let locale = arabic_numerals();
    let mut numeric = 0usize;
    let mut textual = 0usize;
    for (position, input) in corpus().into_iter().enumerate() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let Ok(formatter) = locale.formatter_for_with_limits(&specifier, SWEEP_LIMITS) else {
            continue;
        };
        match formatter.input_kind() {
            InputKind::Number => {
                for (index, value) in AWKWARD_VALUES.iter().enumerate() {
                    if !sampled(position, index, 4) {
                        continue;
                    }
                    let formatted = formatter
                        .format_number(*value)
                        .unwrap_or_else(|error| panic!("{specifier} on {value}: {error}"));
                    assert!(
                        !formatted.chars().any(|c| c.is_ascii_digit()),
                        "{specifier} on {:016X} left an ASCII digit in {formatted:?}",
                        value.to_bits()
                    );
                    numeric += 1;
                }
            }
            InputKind::Text => {
                for (index, text) in TEXT_SAMPLES.iter().enumerate() {
                    if !sampled(position, index, 4) {
                        continue;
                    }
                    let formatted = formatter
                        .format_text(text)
                        .unwrap_or_else(|error| panic!("{specifier} on {text:?}: {error}"));
                    assert!(
                        !formatted.chars().any(|c| c.is_ascii_digit()),
                        "{specifier} on {text:?} left an ASCII digit in {formatted:?}"
                    );
                    textual += 1;
                }
            }
        }
    }

    // The sharpest case section 4.4 states outright, kept here as well so that the
    // property above is anchored to a value and not only to an absence.
    let hexadecimal = locale.formatter("#x").expect("valid");
    assert_eq!(
        hexadecimal.format_number(48879.0).expect("valid"),
        "\u{660}xbeef"
    );

    assert!(
        numeric > 100_000,
        "only {numeric} numeric results were checked"
    );
    assert!(textual > 200, "only {textual} text results were checked");
    eprintln!("d3-format: {numeric} numeric and {textual} text results carried no ASCII digit");
}

/// `src/formatGroup.js` with an unbounded width, transcribed from the JavaScript.
///
/// Written from the source and not from `src/layout.rs`, for the same reason
/// [`reference_match`] is: the rule this checks -- that the sizes *cycle* -- is one
/// line of the loop, and a port that repeated the last size instead would agree with
/// itself on every input.
fn reference_group(digits: &str, sizes: &[u32], thousands: &str) -> String {
    let units: Vec<char> = digits.chars().collect();
    let mut groups: Vec<String> = Vec::new();
    let mut index = units.len() as i64;
    let mut cursor = 0usize;
    let mut size = i64::from(sizes[0]);
    while index > 0 && size > 0 {
        let end = index as usize;
        index -= size;
        // `value.substring(i -= g, i + g)`, whose start is clamped at zero.
        let start = index.max(0) as usize;
        groups.push(units[start..end].iter().collect());
        cursor = (cursor + 1) % sizes.len();
        size = i64::from(sizes[cursor]);
    }
    groups.reverse();
    groups.join(thousands)
}

#[test]
fn grouping_cycles_through_its_sizes_rather_than_repeating_the_last() {
    // Section 4.4 states the rule and warns that a hand-written expectation is not
    // evidence for it, because the wrong answer is the plausible one: `[3, 2]` is the
    // Indian grouping in every description of the Indian grouping, and d3 does not
    // read it that way. So the corpus is generated, the expectation comes from a
    // transcription of `src/formatGroup.js`, and the two examples that make the
    // difference visible are stated as well.
    let separators = [",", ".", "\u{202f}", "\u{a0}", "'"];
    let grouped = |sizes: &[u32], thousands: &str, digits: &str| -> String {
        let locale = Locale::builder()
            .grouping(sizes.to_vec())
            .thousands(thousands)
            .build()
            .expect("a positive grouping size is valid");
        let value: f64 = digits.parse().expect("under 2^53, so exact");
        locale
            .formatter(",d")
            .expect("valid")
            .format_number(value)
            .expect("valid")
    };

    // Section 4.4's example, and the answer it is not.
    assert_eq!(grouped(&[3, 2], ",", "1234567890"), "12,345,67,890");
    assert_ne!(grouped(&[3, 2], ",", "1234567890"), "1,23,45,67,890");
    // The Indian grouping is spelled out entry by entry instead, which is exactly
    // what `locale/en-IN.json` does.
    assert_eq!(
        grouped(&[3, 2, 2, 2, 2, 2, 2, 2, 2, 2], ",", "1234567890"),
        "1,23,45,67,890"
    );

    // Fifteen digits is the most a `f64` holds exactly for every value in the range,
    // so `d` renders the digit string back unchanged and the only question left is
    // where the separators went.
    const DIGITS: &str = "123456789012345";
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let mut checked = 0usize;
    for round in 0..500 {
        // Sizes above the digit count reach `substring`'s clamp, and a list longer
        // than the number of groups the digits need reaches the cycle's wrap-around
        // from the other side; both are in range here.
        let length = 1 + rng.below(5);
        let sizes: Vec<u32> = (0..length).map(|_| 1 + rng.below(8) as u32).collect();
        let thousands = separators[round % separators.len()];
        for count in 1..=DIGITS.len() {
            let digits = &DIGITS[..count];
            assert_eq!(
                grouped(&sizes, thousands, digits),
                reference_group(digits, &sizes, thousands),
                "grouping {digits} by {sizes:?} with {thousands:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 7_000, "only {checked} groupings were checked");
}

/// Whether `x` is a value `src/exponent.js` has no decimal exponent for.
///
/// `formatDecimalParts` returns `null` for either zero, either infinity and NaN, and
/// `exponent.js` turns that into NaN. Every NaN the precision helpers can answer
/// with comes from one of these.
fn has_no_exponent(x: f64) -> bool {
    x == 0.0 || !x.is_finite()
}

/// The contract section 5.5 states for a precision suggestion.
fn assert_suggestion(what: &str, suggestion: f64, expected_nan: bool) {
    assert_eq!(
        suggestion.is_nan(),
        expected_nan,
        "{what} answered {suggestion}"
    );
    if expected_nan {
        // Not `f64::NAN`. `Math.max` answers with the negative quiet NaN whatever its
        // operands were, and section 3.6 compares numeric results by bit pattern, so
        // `is_nan()` alone would accept an answer the oracle disagrees with.
        assert_eq!(
            format!("{:016X}", suggestion.to_bits()),
            "FFF8000000000000",
            "{what} answered the wrong NaN"
        );
        assert_eq!(as_precision(suggestion), None, "{what}");
        return;
    }
    assert!(
        suggestion.is_finite() && suggestion >= 0.0 && suggestion.fract() == 0.0,
        "{what} answered {suggestion}, which is not a whole non-negative count"
    );
    // `Math.max(+0, -0)` is `+0`, and that is also compared by bit pattern.
    assert!(
        suggestion != 0.0 || suggestion.is_sign_positive(),
        "{what} answered a negative zero"
    );
    let precision = as_precision(suggestion).unwrap_or_else(|| {
        panic!("{what} answered {suggestion}, which as_precision refused to convert")
    });
    assert_eq!(f64::from(precision), suggestion, "{what}");
}

/// Steps and values for the precision sweep, beyond [`AWKWARD_VALUES`].
///
/// Raw bit patterns reach the NaN payloads and the subnormals; the powers of ten
/// reach the exponent extremes on both sides, including the two that overflow to
/// infinity, which is one of the four ways the helpers are required to answer NaN.
fn precision_operands(count: usize, seed: u64) -> Vec<f64> {
    let mut rng = Rng(seed);
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        let value = match index % 4 {
            0 => f64::from_bits(rng.next()),
            1 => {
                let exponent = rng.below(650) as i32 - 330;
                format!("1e{exponent}").parse::<f64>().expect("a literal")
            }
            2 => rng.next() as i64 as f64 / 1e9,
            _ => -(rng.below(1_000_000) as f64) / 1024.0,
        };
        values.push(value);
    }
    values
}

#[test]
fn the_precision_helpers_propagate_nan_where_a_float_max_would_lose_it() {
    // Section 5.5, and the trap it names: `Math.max` propagates NaN and `f64::max`
    // discards it, so `0.0_f64.max(f64::NAN)` is `0.0`. A port that reached for the
    // Rust builtin would turn every NaN answer the contract requires into a zero,
    // and would keep passing any suite that only exercised finite steps -- which is
    // most of the recorded ones, since a caller asking for a precision has a real
    // step in hand. The sweep is over the degenerate values on purpose, and it
    // asserts how many NaN answers it saw so that a corpus of only finite steps
    // fails here instead of passing.
    let mut steps: Vec<f64> = AWKWARD_VALUES.to_vec();
    steps.extend(precision_operands(200, 0x9e37_79b9_7f4a_7c15));
    let mut others: Vec<f64> = AWKWARD_VALUES.to_vec();
    others.extend(precision_operands(40, 0xd1b5_4a32_d192_ed03));

    let mut nan_answers = 0usize;
    let mut real_answers = 0usize;
    for step in &steps {
        let step = *step;
        assert_suggestion(
            &format!("precision_fixed({step})"),
            precision_fixed(step),
            has_no_exponent(step),
        );

        for other in &others {
            let other = *other;

            // `step = Math.abs(step), max = Math.abs(max) - step`, an ordinary `f64`
            // subtraction whose own degeneracies -- an infinity minus an infinity, a
            // difference that cancels to zero -- are the second way this answers NaN.
            let difference = other.abs() - step.abs();
            let round = precision_round(step, other);
            assert_suggestion(
                &format!("precision_round({step}, {other})"),
                round,
                has_no_exponent(step) || has_no_exponent(difference),
            );
            if !round.is_nan() {
                assert!(round >= 1.0, "precision_round({step}, {other}) is {round}");
            }

            // The SI bucket of a zero, NaN or infinite value is the third way, and it
            // is the one section 4.1 warns must not be cast to an integer or used to
            // index the prefix table.
            let prefix = precision_prefix(step, other);
            assert_suggestion(
                &format!("precision_prefix({step}, {other})"),
                prefix,
                has_no_exponent(step) || has_no_exponent(other),
            );

            for suggestion in [round, prefix] {
                if suggestion.is_nan() {
                    nan_answers += 1;
                } else {
                    real_answers += 1;
                }
            }
        }
    }
    assert!(nan_answers > 3_000, "only {nan_answers} answers were NaN");
    assert!(
        real_answers > 25_000,
        "only {real_answers} answers were real"
    );
    eprintln!("d3-format: {nan_answers} NaN and {real_answers} real precision suggestions checked");
}

#[test]
fn every_accepted_specifier_round_trips_through_its_own_builder() {
    // Section 5.2 makes `FormatSpecifier` immutable and the builder the only way to
    // derive one from another, which puts `to_builder` on the path of every change a
    // caller can make. A field it forgot to carry across would be silently reset to
    // the default -- and the default is a *valid* value for every field, so nothing
    // would fail except the caller's output. The corpus supplies the specifiers, and
    // the identity is the assertion.
    let mut checked = 0usize;
    for input in corpus() {
        let Ok(specifier) = input.parse::<FormatSpecifier>() else {
            continue;
        };
        let rebuilt = specifier
            .to_builder()
            .build()
            .unwrap_or_else(|error| panic!("{input:?} did not survive its own builder: {error}"));
        assert_eq!(rebuilt, specifier, "{input:?} changed through its builder");
        assert_eq!(
            rebuilt.to_string(),
            specifier.to_string(),
            "{input:?} renders differently after its builder"
        );
        checked += 1;
    }
    assert!(checked > 5_000, "only {checked} specifiers were rebuilt");
}

// ---------------------------------------------------------------------------
// The generated locale table
// ---------------------------------------------------------------------------

/// A generated locale as a [`LocaleDefinition`], through the public table only.
#[cfg(feature = "locales")]
fn definition_of(generated: &d3_format::locales::GeneratedLocale) -> d3_format::LocaleDefinition {
    d3_format::LocaleDefinition {
        decimal: generated.decimal.map(str::to_owned),
        thousands: generated.thousands.map(str::to_owned),
        grouping: generated.grouping.map(<[u32]>::to_vec),
        currency: generated
            .currency
            .map(|[prefix, suffix]| [prefix.to_owned(), suffix.to_owned()]),
        numerals: generated.numerals.map(|ten| ten.map(str::to_owned)),
        percent: generated.percent.map(str::to_owned),
        minus: generated.minus.map(str::to_owned),
        nan: generated.nan.map(str::to_owned),
    }
}

/// Whether every string this locale can emit is free of ASCII digits.
///
/// Only then does "no ASCII digit survives" say something about the substitution
/// rather than about the locale's own text.
#[cfg(feature = "locales")]
fn substitutes_every_digit(locale: &Locale) -> bool {
    let Some(numerals) = locale.numerals() else {
        return false;
    };
    let mut texts: Vec<&str> = vec![
        locale.decimal(),
        locale.thousands(),
        locale.currency_prefix(),
        locale.currency_suffix(),
        locale.percent(),
        locale.minus(),
        locale.nan(),
    ];
    texts.extend(numerals.iter().map(String::as_str));
    !texts
        .iter()
        .any(|text| text.chars().any(|character| character.is_ascii_digit()))
}

/// A specifier sample broad enough to reach every conversion and every affix.
#[cfg(feature = "locales")]
const LOCALE_SPECIFIERS: [&str; 26] = [
    "",
    "d",
    "f",
    ".2f",
    ".0f",
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
    "%",
    ".0%",
    "b",
    "#b",
    "#x",
    "$,.2f",
    "0>12,.2f",
    "_^+$012,.2~f",
    "020c",
];

#[cfg(feature = "locales")]
#[test]
fn every_named_locale_matches_its_definition_and_formats_everything() {
    // Section 5.4's "no accepted input may trigger a panic, for any accepted
    // specifier, locale or `f64` bit pattern" names the locale as one of the three
    // axes, and the sweeps above hold it fixed at en-US. This is that axis: all 59
    // bundled definitions, with their 23 distinct numeral sets, their multi-code-unit
    // separators and their right-to-left affixes, crossed with a specifier sample and
    // every awkward bit pattern.
    //
    // It also closes the door between the table and the constructor. `Locale::named`
    // is documented as `from_definition` of the table entry; asserting that from
    // outside means a future named locale that took a shortcut -- a cached
    // `LocaleInner`, a post-processing step, a different table -- is a failure rather
    // than a divergence nobody notices.
    let mut names = 0usize;
    let mut formatted = 0usize;
    let mut substituting = 0usize;
    for generated in &d3_format::locales::LOCALES {
        let name = generated.name;
        let locale = Locale::named(name).unwrap_or_else(|| panic!("{name} is in the table"));
        let rebuilt = Locale::from_definition(definition_of(generated))
            .unwrap_or_else(|error| panic!("{name} does not satisfy its own schema: {error}"));
        assert_eq!(locale, rebuilt, "{name} differs from its definition");
        names += 1;

        let digits_are_substituted = substitutes_every_digit(&locale);
        substituting += usize::from(digits_are_substituted);

        for spec in LOCALE_SPECIFIERS {
            let formatter = locale
                .formatter_with_limits(spec, SWEEP_LIMITS)
                .unwrap_or_else(|error| panic!("{name} refused {spec:?}: {error}"));
            match formatter.input_kind() {
                InputKind::Number => {
                    for value in AWKWARD_VALUES {
                        let out = formatter.format_number(value).unwrap_or_else(|error| {
                            panic!("{name} {spec:?} on {:016X}: {error}", value.to_bits())
                        });
                        if digits_are_substituted {
                            assert!(
                                !out.chars().any(|c| c.is_ascii_digit()),
                                "{name} {spec:?} on {:016X} left an ASCII digit in {out:?}",
                                value.to_bits()
                            );
                        }
                        formatted += 1;
                    }
                }
                InputKind::Text => {
                    for text in TEXT_SAMPLES {
                        let out = formatter
                            .format_text(text)
                            .unwrap_or_else(|error| panic!("{name} {spec:?} on {text:?}: {error}"));
                        if digits_are_substituted {
                            assert!(
                                !out.chars().any(|c| c.is_ascii_digit()),
                                "{name} {spec:?} on {text:?} left an ASCII digit in {out:?}"
                            );
                        }
                        formatted += 1;
                    }
                }
            }
        }
    }

    assert_eq!(names, d3_format::locales::LOCALE_COUNT);
    assert_eq!(names, 59, "the bundled locale count changed");
    assert_eq!(names, d3_format::locales::names().count());
    assert!(
        substituting >= 20,
        "only {substituting} bundled locales carry numerals"
    );
    eprintln!(
        "d3-format: {formatted} values laid out through {names} named locales, \
{substituting} of them substituting every digit"
    );
}
