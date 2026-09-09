//! The d3 format mini-language, as an immutable validated value.
//!
//! d3's grammar is one regular expression in `src/formatSpecifier.js`:
//!
//! ```text
//! /^(?:(.)?([<>=^]))?([+\-( ])?([$#])?(0)?(\d+)?(,)?(\.\d+)?(~)?([a-z%])?$/i
//! ```
//!
//! [`FormatSpecifier`] is what that produces, with MIGRATION_TO_RUST.md section
//! 5.2's differences made real: the fields are typed and private, there is no
//! coercion on assignment because there is no assignment, and two inputs the
//! JavaScript object can hold but not usefully render are rejected up front.
//!
//! The parser below is hand-written rather than regex-driven, because the crate has
//! no default dependencies. It is not a general regex engine; it reproduces this
//! one pattern, including the two places the pattern really does backtrack. Those
//! are called out where they happen, and `tests/properties.rs` checks the result
//! against an independent brute-force matcher over a large generated corpus rather
//! than trusting the reading.

use core::fmt;
use core::str::FromStr;

use crate::types::{Align, FormatType, Sign, Symbol};

/// Why a specifier could not be parsed or built.
///
/// Section 5.2 fixes the `Display` text at exactly `invalid format: {input}`,
/// because `test/format-test.js` and `test/formatSpecifier-test.js` assert that
/// message by regular expression. All three causes share it, so the message cannot
/// drift apart from d3's for the syntactic case; [`ParseError::kind`] is how a
/// caller tells them apart programmatically.
///
/// Limit, locale-validation and input-kind failures are *not* reported here. They
/// are separate [`FormatError`](crate::FormatError) and
/// [`LocaleError`](crate::LocaleError) variants, so nothing that is not a
/// specifier-shape problem is disguised as one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    input: String,
    kind: ParseErrorKind,
}

impl ParseError {
    fn new(input: impl Into<String>, kind: ParseErrorKind) -> ParseError {
        ParseError {
            input: input.into(),
            kind,
        }
    }

    /// The text that was rejected.
    ///
    /// For a parse this is the input verbatim. For a failed build it is the
    /// canonical spelling the builder would have produced, which is the same text
    /// [`FromStr`] would reject.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Which part of the grammar the input broke.
    pub fn kind(&self) -> ParseErrorKind {
        self.kind
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid format: {}", self.input)
    }
}

impl std::error::Error for ParseError {}

/// What went wrong in a [`ParseError`].
///
/// Every variant renders as the same `invalid format: {input}` message, so adding
/// one cannot change what any existing error prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParseErrorKind {
    /// The text is not in the grammar at all.
    Syntax,
    /// The fill character is one the port refuses to pad with.
    ///
    /// See the `no-non-bmp-or-newline-fill` entry in `DIVERGENCES.md`. d3's
    /// `.` already rejects the line terminators and, in effect, the supplementary
    /// plane; the port additionally rejects the other control characters.
    Fill,
    /// The width has more magnitude than the syntax can carry.
    ///
    /// Section 5.4: parsing rejects a syntactic width above `u32::MAX`. d3 accepts
    /// the digits and fails unpredictably later, if at all.
    WidthOverflow,
}

/// A validated, immutable format specifier.
///
/// Produced by parsing (`"$,.2f".parse()`) or by [`FormatSpecifier::builder`], both
/// of which validate up front, and never modified afterwards. To change one, take
/// [`FormatSpecifier::to_builder`] and build another.
///
/// # Differences from d3
///
/// Section 5.2 lists these as intentional:
///
/// * the fields are typed and private, so there is nothing to mutate and no
///   `String(value)`, `+value` or `ToInt32` coercion path;
/// * a fill character outside the Basic Multilingual Plane, or one that is a
///   control character or line terminator, is rejected;
/// * a width the syntax spells but `u32` cannot hold is rejected;
/// * an unknown ASCII type letter is kept, and formats as d3's `.12~g`.
///
/// # Examples
///
/// ```
/// use d3_format::{Align, FormatSpecifier, FormatType, Sign, Symbol};
///
/// let specifier: FormatSpecifier = "$,.2f".parse()?;
/// assert_eq!(specifier.symbol(), Symbol::Currency);
/// assert_eq!(specifier.comma(), true);
/// assert_eq!(specifier.precision(), Some(2));
/// assert_eq!(specifier.format_type(), FormatType::Fixed);
///
/// // Defaults are filled in, so the canonical spelling is longer than the input.
/// assert_eq!(specifier.fill(), ' ');
/// assert_eq!(specifier.align(), Align::Right);
/// assert_eq!(specifier.sign(), Sign::Minus);
/// assert_eq!(specifier.to_string(), " >-$,.2f");
/// # Ok::<(), d3_format::ParseError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatSpecifier {
    fill: char,
    align: Align,
    sign: Sign,
    symbol: Symbol,
    zero: bool,
    width: Option<u32>,
    comma: bool,
    precision: Option<u32>,
    trim: bool,
    format_type: FormatType,
}

impl FormatSpecifier {
    /// A builder starting from d3's defaults, the same ones `""` parses to.
    pub fn builder() -> FormatSpecifierBuilder {
        FormatSpecifierBuilder::new()
    }

    /// A builder starting from this specifier's fields.
    ///
    /// This is how a specifier is "changed": the original is untouched and a new
    /// value comes out. d3 assigns to the object's fields instead, which is the
    /// `immutable-typed-specifier` entry in `DIVERGENCES.md`.
    ///
    /// ```
    /// use d3_format::{FormatSpecifier, FormatType};
    ///
    /// let fixed: FormatSpecifier = ".2f".parse()?;
    /// let exponential = fixed
    ///     .to_builder()
    ///     .format_type(FormatType::Exponent)
    ///     .build()?;
    ///
    /// assert_eq!(fixed.to_string(), " >-.2f");
    /// assert_eq!(exponential.to_string(), " >-.2e");
    /// # Ok::<(), d3_format::ParseError>(())
    /// ```
    pub fn to_builder(&self) -> FormatSpecifierBuilder {
        FormatSpecifierBuilder {
            fill: self.fill,
            align: self.align,
            sign: self.sign,
            symbol: self.symbol,
            zero: self.zero,
            width: self.width,
            comma: self.comma,
            precision: self.precision,
            trim: self.trim,
            format_type: self.format_type,
        }
    }

    /// The padding character. Defaults to a space.
    pub fn fill(&self) -> char {
        self.fill
    }

    /// Where the padding goes. Defaults to [`Align::Right`].
    pub fn align(&self) -> Align {
        self.align
    }

    /// How the sign is spelled. Defaults to [`Sign::Minus`].
    pub fn sign(&self) -> Sign {
        self.sign
    }

    /// The currency or base-prefix symbol. Defaults to [`Symbol::None`].
    pub fn symbol(&self) -> Symbol {
        self.symbol
    }

    /// Whether the specifier asked for zero fill.
    ///
    /// This is the `0` the grammar spells. A formatter also turns on zero fill for
    /// a specifier that spells fill `0` with align `=`; that resolution belongs to
    /// the formatter, not to the parsed value.
    pub fn zero(&self) -> bool {
        self.zero
    }

    /// The requested width in UTF-16 code units, if any.
    ///
    /// A width of zero is representable, because d3's grammar can produce one:
    /// `"00c"` sets the zero flag from the first digit and width zero from the
    /// second. `Display` renders it as `1`, exactly as d3's `Math.max(1, width | 0)`
    /// does, so it is the one field for which `to_string().parse()` is not the
    /// identity. See the [`Display`](std::fmt::Display#impl-Display-for-FormatSpecifier)
    /// implementation.
    pub fn width(&self) -> Option<u32> {
        self.width
    }

    /// Whether grouping was requested with `,`.
    pub fn comma(&self) -> bool {
        self.comma
    }

    /// The requested precision, if any.
    ///
    /// This is the value the grammar spells, before the formatter clamps it to the
    /// range its type supports -- `[1, 21]` for the significant-digit types and
    /// `[0, 20]` for the rest.
    pub fn precision(&self) -> Option<u32> {
        self.precision
    }

    /// Whether insignificant trailing zeros should be trimmed, from `~`.
    pub fn trim(&self) -> bool {
        self.trim
    }

    /// The conversion. Defaults to [`FormatType::None`], d3's `.12~g` shorthand.
    pub fn format_type(&self) -> FormatType {
        self.format_type
    }
}

/// The canonical d3 spelling, matching `FormatSpecifier.prototype.toString`.
///
/// Every field is spelled, including the ones that were defaulted, so the output is
/// longer than most inputs: `"d"` renders as `" >-d"`. The rendering is idempotent
/// -- parsing it and rendering again gives the same text -- and re-parsing it
/// recovers the same specifier, with one exception d3 shares: a width of zero
/// renders as `1`, because d3 renders `Math.max(1, this.width | 0)`.
impl fmt::Display for FormatSpecifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}{}", self.fill, self.align, self.sign, self.symbol)?;
        if self.zero {
            f.write_str("0")?;
        }
        if let Some(width) = self.width {
            write!(f, "{}", width.max(1))?;
        }
        if self.comma {
            f.write_str(",")?;
        }
        if let Some(precision) = self.precision {
            write!(f, ".{precision}")?;
        }
        if self.trim {
            f.write_str("~")?;
        }
        write!(f, "{}", self.format_type)
    }
}

impl Default for FormatSpecifier {
    /// The specifier `""` parses to.
    fn default() -> FormatSpecifier {
        FormatSpecifierBuilder::new()
            .build()
            .expect("the default fields are valid")
    }
}

impl FromStr for FormatSpecifier {
    type Err = ParseError;

    fn from_str(text: &str) -> Result<FormatSpecifier, ParseError> {
        parse(text)
    }
}

/// Builds a [`FormatSpecifier`] from typed fields.
///
/// Every setter takes a value of the field's own type, so the ill-typed assignments
/// `test/formatSpecifier-test.js` makes -- `fill: 1`, `type: 10`, a negative width
/// -- cannot be written. Validation happens in [`FormatSpecifierBuilder::build`],
/// and it is the same validation the parser applies to the canonical spelling of
/// the same fields, so the two entry points cannot accept different sets.
///
/// # Examples
///
/// ```
/// use d3_format::{Align, FormatSpecifier, FormatType, Sign, Symbol};
///
/// let specifier = FormatSpecifier::builder()
///     .fill('_')
///     .align(Align::Center)
///     .sign(Sign::Plus)
///     .symbol(Symbol::Currency)
///     .zero(true)
///     .width(12)
///     .comma(true)
///     .precision(2)
///     .trim(true)
///     .format_type(FormatType::Fixed)
///     .build()?;
///
/// assert_eq!(specifier.to_string(), "_^+$012,.2~f");
/// # Ok::<(), d3_format::ParseError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FormatSpecifierBuilder {
    fill: char,
    align: Align,
    sign: Sign,
    symbol: Symbol,
    zero: bool,
    width: Option<u32>,
    comma: bool,
    precision: Option<u32>,
    trim: bool,
    format_type: FormatType,
}

impl FormatSpecifierBuilder {
    /// A builder holding d3's defaults.
    pub fn new() -> FormatSpecifierBuilder {
        FormatSpecifierBuilder {
            fill: ' ',
            align: Align::Right,
            sign: Sign::Minus,
            symbol: Symbol::None,
            zero: false,
            width: None,
            comma: false,
            precision: None,
            trim: false,
            format_type: FormatType::None,
        }
    }

    /// Sets the padding character.
    ///
    /// Rejected at [`build`](FormatSpecifierBuilder::build) time if it is outside
    /// the Basic Multilingual Plane, or is a control character or line terminator.
    pub fn fill(mut self, value: char) -> Self {
        self.fill = value;
        self
    }

    /// Sets where the padding goes.
    pub fn align(mut self, value: Align) -> Self {
        self.align = value;
        self
    }

    /// Sets how the sign is spelled.
    pub fn sign(mut self, value: Sign) -> Self {
        self.sign = value;
        self
    }

    /// Sets the currency or base-prefix symbol.
    pub fn symbol(mut self, value: Symbol) -> Self {
        self.symbol = value;
        self
    }

    /// Sets the zero-fill flag.
    pub fn zero(mut self, value: bool) -> Self {
        self.zero = value;
        self
    }

    /// Sets or clears the width.
    ///
    /// Takes either a `u32` or an `Option<u32>`, so `width(12)` and `width(None)`
    /// both work. A negative width is unrepresentable rather than clamped; d3
    /// clamps because it accepts one in the first place.
    pub fn width(mut self, value: impl Into<Option<u32>>) -> Self {
        self.width = value.into();
        self
    }

    /// Sets or clears grouping.
    pub fn comma(mut self, value: bool) -> Self {
        self.comma = value;
        self
    }

    /// Sets or clears the precision.
    ///
    /// Takes either a `u32` or an `Option<u32>`. A negative precision is
    /// unrepresentable rather than clamped to zero.
    pub fn precision(mut self, value: impl Into<Option<u32>>) -> Self {
        self.precision = value.into();
        self
    }

    /// Sets or clears trimming of insignificant zeros.
    pub fn trim(mut self, value: bool) -> Self {
        self.trim = value;
        self
    }

    /// Sets the conversion.
    ///
    /// A [`FormatType::Unknown`] carrying a letter that names a known conversion is
    /// canonicalized to that conversion; one carrying anything that is not an ASCII
    /// letter is rejected at [`build`](FormatSpecifierBuilder::build) time.
    pub fn format_type(mut self, value: FormatType) -> Self {
        self.format_type = value;
        self
    }

    /// Validates the fields and freezes them into a [`FormatSpecifier`].
    ///
    /// # Errors
    ///
    /// Returns the same [`ParseError`] that parsing this builder's canonical
    /// spelling would return, so the builder and the parser accept exactly the same
    /// specifiers.
    pub fn build(self) -> Result<FormatSpecifier, ParseError> {
        let format_type = match self.format_type {
            FormatType::None => FormatType::None,
            other => {
                let letter = other.as_char().expect("only None has no character");
                // Canonicalizes `Unknown('f')` into `Fixed`, and rejects an
                // `Unknown` carrying something the grammar could never have
                // produced.
                FormatType::from_char(letter)
                    .ok_or_else(|| ParseError::new(self.render(), ParseErrorKind::Syntax))?
            }
        };
        if let Err(kind) = check_fill(self.fill) {
            return Err(ParseError::new(self.render(), kind));
        }
        Ok(FormatSpecifier {
            fill: self.fill,
            align: self.align,
            sign: self.sign,
            symbol: self.symbol,
            zero: self.zero,
            width: self.width,
            comma: self.comma,
            precision: self.precision,
            trim: self.trim,
            format_type,
        })
    }

    /// The canonical spelling these fields would render to.
    ///
    /// Used only to name the offending text in a build failure, which is why it
    /// does not need a validated specifier to exist first.
    fn render(&self) -> String {
        let mut text = String::new();
        text.push(self.fill);
        text.push(self.align.as_char());
        text.push(self.sign.as_char());
        if let Some(symbol) = self.symbol.as_char() {
            text.push(symbol);
        }
        if self.zero {
            text.push('0');
        }
        if let Some(width) = self.width {
            text.push_str(&width.max(1).to_string());
        }
        if self.comma {
            text.push(',');
        }
        if let Some(precision) = self.precision {
            text.push('.');
            text.push_str(&precision.to_string());
        }
        if self.trim {
            text.push('~');
        }
        if let Some(letter) = self.format_type.as_char() {
            text.push(letter);
        }
        text
    }
}

impl Default for FormatSpecifierBuilder {
    fn default() -> FormatSpecifierBuilder {
        FormatSpecifierBuilder::new()
    }
}

/// Whether `value` may be used as fill.
///
/// d3's `(.)` capture is one UTF-16 code unit and `.` does not match a line
/// terminator, so d3 already rejects `\n`, `\r`, U+2028 and U+2029 in that position,
/// and a supplementary-plane character never reaches it either: `.` would take only
/// the high surrogate and the alignment character would then have to be the low
/// surrogate. The port makes both refusals explicit, and extends them to the
/// remaining control characters, which d3 accepts and pads with. See
/// `no-non-bmp-or-newline-fill` in `DIVERGENCES.md`.
fn check_fill(value: char) -> Result<(), ParseErrorKind> {
    let outside_bmp = value as u32 > 0xFFFF;
    let line_terminator = matches!(value, '\u{2028}' | '\u{2029}');
    if outside_bmp || line_terminator || value.is_control() {
        return Err(ParseErrorKind::Fill);
    }
    Ok(())
}

/// A cursor over the specifier text, in `char`s.
///
/// The grammar is entirely ASCII apart from the fill character, and fill is
/// constrained to the Basic Multilingual Plane, so a `char` cursor and d3's UTF-16
/// code-unit cursor agree everywhere it matters. [`check_fill`] is what keeps that
/// true.
struct Cursor<'a> {
    chars: &'a [char],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.at + offset).copied()
    }

    /// Consumes the next character if `accept` maps it to a value.
    fn take<T>(&mut self, accept: impl FnOnce(char) -> Option<T>) -> Option<T> {
        let value = accept(self.peek()?)?;
        self.at += 1;
        Some(value)
    }

    /// Consumes `expected` if it is next.
    fn take_char(&mut self, expected: char) -> bool {
        self.take(|found| (found == expected).then_some(()))
            .is_some()
    }

    /// Consumes a maximal run of ASCII digits.
    fn take_digits(&mut self) -> Option<&'a [char]> {
        let start = self.at;
        while self.peek().is_some_and(|found| found.is_ascii_digit()) {
            self.at += 1;
        }
        (self.at > start).then(|| &self.chars[start..self.at])
    }

    fn at_end(&self) -> bool {
        self.at >= self.chars.len()
    }
}

/// Parses `text` against d3's grammar.
fn parse(text: &str) -> Result<FormatSpecifier, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut cursor = Cursor {
        chars: &chars,
        at: 0,
    };

    // `(?:(.)?([<>=^]))?`. The engine is greedy, so it first tries to spend a
    // character on the fill and match the alignment against the *second* character;
    // only if that fails does it try the alignment at the first character, and only
    // then does it skip the group. Trying them in that order is what makes `"<<d"`
    // fill `<` align `<`, and `"<d"` the default fill with align `<`.
    let mut fill = ' ';
    let mut align = Align::Right;
    match (cursor.peek(), cursor.peek_at(1).and_then(Align::from_char)) {
        (Some(candidate), Some(aligned)) => {
            // The fill slot is committed here, so a fill the port refuses is
            // reported as such rather than as a generic syntax error. d3 rejects
            // most of the same characters, by failing this branch and then failing
            // to match anything else either.
            if let Err(kind) = check_fill(candidate) {
                return Err(ParseError::new(text, kind));
            }
            fill = candidate;
            align = aligned;
            cursor.at += 2;
        }
        _ => {
            if let Some(aligned) = cursor.take(Align::from_char) {
                align = aligned;
            }
        }
    }

    let sign = cursor.take(Sign::from_char).unwrap_or(Sign::Minus);
    let symbol = cursor.take(Symbol::from_char).unwrap_or(Symbol::None);

    // `(0)?(\d+)?`. These two overlap, and the engine is greedy: the leading zero
    // goes to the flag, and any digits after it are the width. `"00c"` therefore
    // means zero fill with width zero, not width `00`.
    let zero = cursor.take_char('0');
    let width = match cursor.take_digits() {
        None => None,
        Some(digits) => {
            let value = parse_u32(digits)
                .ok_or_else(|| ParseError::new(text, ParseErrorKind::WidthOverflow))?;
            Some(value)
        }
    };

    let comma = cursor.take_char(',');

    // `(\.\d+)?`. A `.` with no digits after it is not a precision, and since
    // nothing later in the grammar can match a `.` either, the whole parse fails --
    // which is why d3 rejects `".f"`.
    let precision = if cursor.peek() == Some('.')
        && cursor
            .peek_at(1)
            .is_some_and(|found| found.is_ascii_digit())
    {
        cursor.at += 1;
        let digits = cursor.take_digits().expect("a digit was just peeked");
        // d3 clamps precision into its type's supported range at format time, so a
        // precision too large to hold saturates rather than failing: `u32::MAX`
        // clamps to the same 20 or 21 that d3's `Math.min` reaches from `1e20`.
        // Only the width has a syntactic limit, per section 5.4.
        Some(parse_u32(digits).unwrap_or(u32::MAX))
    } else {
        None
    };

    let trim = cursor.take_char('~');
    let format_type = cursor
        .take(FormatType::from_char)
        .unwrap_or(FormatType::None);

    if !cursor.at_end() {
        return Err(ParseError::new(text, ParseErrorKind::Syntax));
    }

    Ok(FormatSpecifier {
        fill,
        align,
        sign,
        symbol,
        zero,
        width,
        comma,
        precision,
        trim,
        format_type,
    })
}

/// Reads a run of ASCII digits, or `None` if the value does not fit a `u32`.
///
/// Hand-rolled rather than `str::parse` so that the digits do not have to be
/// reassembled into a `String` first, and so overflow is a value rather than a
/// parse error carrying a message the caller would discard.
fn parse_u32(digits: &[char]) -> Option<u32> {
    let mut value: u32 = 0;
    for digit in digits {
        let digit = digit.to_digit(10).expect("the caller matched ASCII digits");
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str) -> FormatSpecifier {
        text.parse().unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn the_empty_specifier_has_d3s_defaults() {
        let parsed = spec("");
        assert_eq!(parsed.fill(), ' ');
        assert_eq!(parsed.align(), Align::Right);
        assert_eq!(parsed.sign(), Sign::Minus);
        assert_eq!(parsed.symbol(), Symbol::None);
        assert!(!parsed.zero());
        assert_eq!(parsed.width(), None);
        assert!(!parsed.comma());
        assert_eq!(parsed.precision(), None);
        assert!(!parsed.trim());
        assert_eq!(parsed.format_type(), FormatType::None);
        assert_eq!(parsed.to_string(), " >-");
        assert_eq!(parsed, FormatSpecifier::default());
    }

    #[test]
    fn the_alignment_group_prefers_spending_a_character_on_fill() {
        assert_eq!(spec("<d").fill(), ' ');
        assert_eq!(spec("<d").align(), Align::Left);
        assert_eq!(spec("<<d").fill(), '<');
        assert_eq!(spec("<<d").align(), Align::Left);
        assert_eq!(spec("_>8d").fill(), '_');
        assert_eq!(spec("_>8d").width(), Some(8));
        assert_eq!(spec("0=12").fill(), '0');
        assert_eq!(spec("0=12").align(), Align::AfterSign);
        assert_eq!(spec("0=12").width(), Some(12));
    }

    #[test]
    fn the_zero_flag_wins_the_first_digit() {
        // `(0)?(\d+)?` is greedy, so "012" is the zero flag plus width 12, and
        // "00c" is the zero flag plus width zero.
        assert!(spec("012").zero());
        assert_eq!(spec("012").width(), Some(12));
        assert!(spec("00c").zero());
        assert_eq!(spec("00c").width(), Some(0));
        assert!(!spec("12").zero());
        assert_eq!(spec("12").width(), Some(12));
    }

    #[test]
    fn a_zero_width_renders_as_one_exactly_as_d3_does() {
        // `Math.max(1, this.width | 0)`. The one field for which rendering is
        // lossy, and lossy in d3 too.
        let parsed = spec("00c");
        assert_eq!(parsed.to_string(), " >-01c");
        assert_eq!(spec(&parsed.to_string()).width(), Some(1));
    }

    #[test]
    fn every_field_survives_a_full_specifier() {
        let parsed = spec("_^+$012,.2~f");
        assert_eq!(parsed.fill(), '_');
        assert_eq!(parsed.align(), Align::Center);
        assert_eq!(parsed.sign(), Sign::Plus);
        assert_eq!(parsed.symbol(), Symbol::Currency);
        assert!(parsed.zero());
        assert_eq!(parsed.width(), Some(12));
        assert!(parsed.comma());
        assert_eq!(parsed.precision(), Some(2));
        assert!(parsed.trim());
        assert_eq!(parsed.format_type(), FormatType::Fixed);
        assert_eq!(parsed.to_string(), "_^+$012,.2~f");
    }

    #[test]
    fn unknown_type_letters_survive_parsing() {
        let parsed = spec("q");
        assert_eq!(parsed.format_type(), FormatType::Unknown('q'));
        assert!(
            !parsed.trim(),
            "the `.12~g` rewrite belongs to the formatter"
        );
        assert_eq!(parsed.precision(), None);
        assert_eq!(parsed.to_string(), " >-q");
    }

    #[test]
    fn the_type_letter_is_case_sensitive_only_where_d3_is() {
        assert_eq!(spec("x").format_type(), FormatType::HexLower);
        assert_eq!(spec("X").format_type(), FormatType::HexUpper);
        // The `i` flag admits an uppercase letter; only `X` has a conversion.
        assert_eq!(spec("F").format_type(), FormatType::Unknown('F'));
        assert_eq!(spec("F").to_string(), " >-F");
    }

    #[test]
    fn the_three_oracle_rejections_carry_d3s_message() {
        for text in ["foo", ".-2s", ".f"] {
            let error = text.parse::<FormatSpecifier>().unwrap_err();
            assert_eq!(error.to_string(), format!("invalid format: {text}"));
            assert_eq!(error.kind(), ParseErrorKind::Syntax);
            assert_eq!(error.input(), text);
        }
    }

    #[test]
    fn a_width_above_u32_is_a_syntactic_rejection() {
        let error = "999999999999f".parse::<FormatSpecifier>().unwrap_err();
        assert_eq!(error.kind(), ParseErrorKind::WidthOverflow);
        assert_eq!(error.to_string(), "invalid format: 999999999999f");
        assert_eq!(spec("4294967295f").width(), Some(u32::MAX));
    }

    #[test]
    fn an_unholdable_precision_saturates_rather_than_failing() {
        // d3 accepts the digits and clamps to 20 or 21 at format time, so failing
        // here would reject a specifier d3 renders.
        assert_eq!(spec(".99999999999999999999f").precision(), Some(u32::MAX));
    }

    #[test]
    fn refused_fill_characters_are_reported_as_fill_problems() {
        for text in ["\u{1f600}>d", "\n>d", "\r>d", "\t>d", "\u{2028}>d"] {
            let error = text.parse::<FormatSpecifier>().unwrap_err();
            assert_eq!(error.kind(), ParseErrorKind::Fill, "{text:?}");
            assert_eq!(error.to_string(), format!("invalid format: {text}"));
        }
        // A BMP non-control character is fine, including a non-ASCII one.
        assert_eq!(spec("\u{2212}>d").fill(), '\u{2212}');
    }

    #[test]
    fn the_builder_and_the_parser_accept_the_same_specifiers() {
        let built = FormatSpecifier::builder()
            .fill('_')
            .align(Align::Center)
            .sign(Sign::Plus)
            .symbol(Symbol::Currency)
            .zero(true)
            .width(12)
            .comma(true)
            .precision(2)
            .trim(true)
            .format_type(FormatType::Fixed)
            .build()
            .expect("valid");
        assert_eq!(built, spec("_^+$012,.2~f"));
        assert_eq!(built.to_string(), "_^+$012,.2~f");
    }

    #[test]
    fn the_builder_canonicalizes_a_known_letter_spelled_as_unknown() {
        let built = FormatSpecifier::builder()
            .format_type(FormatType::Unknown('f'))
            .build()
            .expect("valid");
        assert_eq!(built.format_type(), FormatType::Fixed);
        assert_eq!(built, spec("f"));
    }

    #[test]
    fn the_builder_rejects_what_the_parser_rejects_and_names_the_same_text() {
        let error = FormatSpecifier::builder()
            .fill('\u{1f600}')
            .build()
            .unwrap_err();
        assert_eq!(error.kind(), ParseErrorKind::Fill);
        assert_eq!(error.input(), "\u{1f600}>-");
        assert!(error.input().parse::<FormatSpecifier>().is_err());

        let error = FormatSpecifier::builder()
            .format_type(FormatType::Unknown('1'))
            .build()
            .unwrap_err();
        assert_eq!(error.kind(), ParseErrorKind::Syntax);
    }

    #[test]
    fn width_and_precision_take_a_value_or_its_absence() {
        let cleared = FormatSpecifier::builder()
            .width(12)
            .precision(2)
            .width(None)
            .precision(None)
            .build()
            .expect("valid");
        assert_eq!(cleared, FormatSpecifier::default());
    }

    #[test]
    fn to_builder_leaves_the_original_untouched() {
        let original = spec(".2f");
        let changed = original
            .to_builder()
            .format_type(FormatType::Exponent)
            .build()
            .expect("valid");
        assert_eq!(original.to_string(), " >-.2f");
        assert_eq!(changed.to_string(), " >-.2e");
        assert_eq!(original.to_builder().build().expect("valid"), original);
    }
}
