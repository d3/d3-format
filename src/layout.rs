//! Value layout: everything the closure `newFormat` returns actually does.
//!
//! MIGRATION_TO_RUST.md section 7 splits `src/locale.js` in two. Resolving a
//! specifier against a locale -- rewriting `n` to `,g`, folding fill `0` with align
//! `=` into the zero flag, composing the affixes, clamping the precision -- is
//! `Formatter::compile` in `locale.rs`. Laying a *value* out with that resolution is
//! this module: type dispatch, trimming, sign policy, the SI suffix, the
//! integer/tail split, grouping, UTF-16 padding, alignment, and the locale's
//! numerals.
//!
//! The order of those steps is observable and section 4.4 lists several of the
//! orderings as things that must not be tidied up. Two in particular: the value is
//! trimmed *before* it is grouped, so `~,` sees the shortened digits, and numerals
//! are substituted *last*, over the assembled string, so they reach the padding and
//! the affixes as well as the digits. The code below therefore follows `format`'s
//! statement order line for line rather than a more natural decomposition, and the
//! places where that matters say so.
//!
//! # Widths are UTF-16 code units
//!
//! d3 measures every length with `String.prototype.length`, which counts UTF-16 code
//! units. That is neither bytes nor `char`s: `"\u{1f600}"` is four bytes, one `char`
//! and two code units. [`utf16_len`] is the measure used everywhere a length feeds
//! into padding or grouping, and grouping itself runs over a `u16` buffer so that
//! its slicing is d3's slicing.

use std::borrow::Cow;

use crate::decimal;
use crate::limits::{FormatError, FormatLimits};
use crate::locale::Locale;
use crate::types::{Align, FormatType, Sign, SI_PREFIXES};

/// The resolution `newFormat` performs before it returns its closure.
///
/// Immutable, and a pure function of the locale, the specifier and the limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    pub(crate) fill: char,
    pub(crate) align: Align,
    pub(crate) sign: Sign,
    pub(crate) zero: bool,
    /// The padded width, in UTF-16 code units. Zero means "no padding requested",
    /// which is also what a width of zero means, since a run of `width - length`
    /// characters is empty either way.
    pub(crate) width: u32,
    pub(crate) comma: bool,
    pub(crate) precision: u32,
    pub(crate) trim: bool,
    /// The type after `n` becomes `,g` and an unknown letter becomes `.12~g`.
    pub(crate) resolved_type: FormatType,
    /// `/[defgprs%]/.test(type)`: whether the result can carry a fractional or
    /// exponential tail that must not be grouped.
    pub(crate) maybe_suffix: bool,
    pub(crate) prefix: String,
    pub(crate) suffix: String,
    /// `Math.pow(10, -e)` for a `formatPrefix` formatter, as raw bits.
    ///
    /// Bits rather than an `f64` because the scale can be NaN -- a zero, NaN or
    /// infinite reference makes it so -- and two formatters built the same way must
    /// compare equal. Section 4.2 pins these values by bit pattern anyway.
    pub(crate) prefix_scale: Option<u64>,
}

/// What is being laid out.
///
/// One layout routine serves both entry points because `format` in `src/locale.js`
/// is one function: type `c` skips the numeric preamble and puts its text where a
/// fractional tail would go, and everything after that -- grouping, padding,
/// alignment, numerals -- is shared.
#[derive(Debug, Clone, Copy)]
enum Input<'a> {
    Number(f64),
    Text(&'a str),
}

impl Layout {
    /// Lays out a number, as `format(value)` does for every type but `c`.
    pub(crate) fn number(
        &self,
        locale: &Locale,
        limits: &FormatLimits,
        out: &mut String,
        value: f64,
    ) -> Result<(), FormatError> {
        self.dispatch(locale, limits, out, Input::Number(value))
    }

    /// Lays out text, as `format(value)` does for type `c`.
    pub(crate) fn text(
        &self,
        locale: &Locale,
        limits: &FormatLimits,
        out: &mut String,
        value: &str,
    ) -> Result<(), FormatError> {
        self.dispatch(locale, limits, out, Input::Text(value))
    }

    /// `return numerals(value)`.
    ///
    /// Numeral substitution is the last thing `format` does, and it runs over the
    /// whole assembled string -- padding, affixes and all -- not over the digits
    /// alone. Section 4.4 lists that ordering as one to preserve, so it is a
    /// separate pass here rather than something folded into digit emission.
    fn dispatch(
        &self,
        locale: &Locale,
        limits: &FormatLimits,
        out: &mut String,
        input: Input<'_>,
    ) -> Result<(), FormatError> {
        match locale.numerals() {
            None => self.assemble(locale, limits, out, input),
            Some(numerals) => {
                let mut assembled = String::new();
                self.assemble(locale, limits, &mut assembled, input)?;
                substitute_numerals(&assembled, numerals, limits, out)
            }
        }
    }

    /// The body of `format`, up to but not including numeral substitution.
    fn assemble(
        &self,
        locale: &Locale,
        limits: &FormatLimits,
        out: &mut String,
        input: Input<'_>,
    ) -> Result<(), FormatError> {
        // `var valueNegative = value < 0 || 1 / value < 0;`
        //
        // The second test is what catches negative zero, and both are false for a
        // NaN whatever its sign bit -- `NaN < 0` and `1 / NaN < 0` are both false --
        // so a negative NaN is not a negative value. Section 4.4 lists that policy;
        // reaching for `is_sign_negative` alone would break it.
        let mut negative = match input {
            Input::Text(_) => false,
            Input::Number(value) => {
                let value = self.scaled(value);
                value < 0.0 || (value == 0.0 && value.is_sign_negative())
            }
        };

        // `value = isNaN(value) ? nan : formatType(Math.abs(value), precision)`.
        // The magnitude is taken *before* the conversion, which is what makes
        // `Math.round` in the integer types round the magnitude rather than the
        // value: `format("d")(-2.5)` is `−3`, not `−2`.
        let mut si_exponent = None;
        let converted: Cow<'_, str> = match input {
            Input::Text(text) => Cow::Borrowed(text),
            Input::Number(value) => {
                let value = self.scaled(value);
                if value.is_nan() {
                    Cow::Borrowed(locale.nan())
                } else {
                    let (text, exponent) = self.convert(value.abs(), limits)?;
                    si_exponent = exponent;
                    Cow::Owned(text)
                }
            }
        };

        // `if (trim) value = formatTrim(value);` -- before grouping, and before the
        // integer/tail split, so a trimmed fraction cannot be grouped and a trimmed
        // exponent keeps its `e`.
        let trimmed = match input {
            Input::Number(_) if self.trim => trim_insignificant(&converted),
            _ => None,
        };
        let body: &str = trimmed.as_deref().unwrap_or(&converted);

        // `if (valueNegative && +value === 0 && sign !== "+") valueNegative = false;`
        //
        // The test is on the *formatted string*, not on the value, so `-0.0001` at
        // precision 2 loses its sign while `-0.0001` at precision 6 keeps it.
        if negative && self.sign != Sign::Plus && formats_as_zero(body) {
            negative = false;
        }

        // `valuePrefix = (...) + valuePrefix`, in that order: the sign comes before
        // the currency or base prefix, so `$` formats as `−$1.00`.
        //
        // Type `c` never reaches this statement at all -- it is inside the `else`
        // branch that the `if (type === "c")` arm skips -- so a `+` or a space sign
        // is silently dropped for text. `format("+c")("x")` really is `"x"`.
        let sign_prefix: &str = match input {
            Input::Text(_) => "",
            Input::Number(_) if negative => match self.sign {
                Sign::Parens => "(",
                _ => locale.minus(),
            },
            Input::Number(_) => match self.sign {
                Sign::Minus | Sign::Parens => "",
                Sign::Plus => "+",
                Sign::Space => " ",
            },
        };

        // `valueSuffix = (type === "s" && ... ? prefixes[8 + prefixExponent / 3] : "")
        //  + valueSuffix + (valueNegative && sign === "(" ? ")" : "")`.
        //
        // `prefixExponent` is a module-level variable in d3 that survives between
        // calls; here the auto-prefix routine returns it, so a NaN or infinite value
        // -- for which it is `undefined` -- cannot pick up the previous call's
        // suffix. See `no-global-si-prefix-leak` in DIVERGENCES.md.
        let si_suffix: &str = match (self.resolved_type, si_exponent) {
            (FormatType::SiPrefix, Some(exponent)) => si_prefix_of(exponent),
            _ => "",
        };
        let close_paren: &str = if negative && self.sign == Sign::Parens {
            ")"
        } else {
            ""
        };

        // The integer/tail split. `maybeSuffix` types stop at the first character
        // that is not an ASCII digit; a `.` becomes the locale's decimal point and
        // anything else -- `e`, `Infinity`, the NaN text, type `d`'s infinity sign --
        // moves wholesale into the suffix so that grouping cannot reach it.
        let (mut value_part, tail_decimal, tail) = match input {
            // Type `c`: `valueSuffix = formatType(value) + valueSuffix; value = ""`.
            // The text lands exactly where a fractional tail would, so it is never
            // grouped and never signed, and only padding, the affixes and the final
            // numeral substitution touch it.
            Input::Text(text) => ("", "", text),
            Input::Number(_) if self.maybe_suffix => split_at_first_non_digit(body, locale),
            Input::Number(_) => (body, "", ""),
        };

        let prefix_pieces: [&str; 2] = [sign_prefix, &self.prefix];
        let suffix_pieces: [&str; 5] = [tail_decimal, tail, si_suffix, &self.suffix, close_paren];

        // `if (comma && !zero) value = group(value, Infinity);`. A locale with no
        // grouping gets d3's `identity`, which is the `None` arm here.
        let grouped = match (self.comma && !self.zero, locale.grouping()) {
            (true, Some(sizes)) => {
                let units = utf16_units(value_part, limits)?;
                Some(group_units(
                    &units,
                    sizes,
                    locale.thousands(),
                    None,
                    limits,
                )?)
            }
            _ => None,
        };
        value_part = grouped.as_deref().unwrap_or(value_part);

        let prefix_len: usize = prefix_pieces.iter().copied().map(utf16_len).sum();
        let suffix_len: usize = suffix_pieces.iter().copied().map(utf16_len).sum();
        let length = prefix_len
            .checked_add(utf16_len(value_part))
            .and_then(|total| total.checked_add(suffix_len))
            .ok_or(FormatError::AllocationFailed)?;

        // `padding = length < width ? new Array(width - length + 1).join(fill) : ""`,
        // which is `width - length` copies of the fill character.
        let mut padding = (self.width as usize).saturating_sub(length);

        // `if (comma && zero) value = group(padding + value, padding.length ? width -
        //  valueSuffix.length : Infinity), padding = "";`
        //
        // The padding is grouped along with the digits, and the group budget
        // subtracts only the suffix -- not the prefix. That asymmetry is d3's, and it
        // is why `format("0$20,.2f")` and `format("$020,.2f")` differ in width.
        let zero_grouped = if self.comma && self.zero {
            let budget = if padding > 0 {
                Some(self.width as i64 - suffix_len as i64)
            } else {
                None
            };
            let units = padded_units(self.fill, padding, value_part, limits)?;
            let regrouped = match locale.grouping() {
                Some(sizes) => group_units(&units, sizes, locale.thousands(), budget, limits)?,
                // d3's `identity` when the locale supplies no grouping: the padding
                // is still folded into the value and still cleared.
                None => String::from_utf16_lossy(&units),
            };
            padding = 0;
            Some(regrouped)
        } else {
            None
        };
        value_part = zero_grouped.as_deref().unwrap_or(value_part);

        let mut sink = Sink {
            out,
            maximum: limits.max_output_bytes,
        };
        // `padding.slice(0, length = padding.length >> 1)` and `padding.slice(length)`.
        // The fill character is inside the Basic Multilingual Plane -- the parser
        // refuses anything else -- so one fill character is one code unit and the
        // halves are simply two runs.
        let head = padding >> 1;
        match self.align {
            Align::Left => {
                sink.pieces(&prefix_pieces)?;
                sink.text(value_part)?;
                sink.pieces(&suffix_pieces)?;
                sink.repeat(self.fill, padding)?;
            }
            Align::AfterSign => {
                sink.pieces(&prefix_pieces)?;
                sink.repeat(self.fill, padding)?;
                sink.text(value_part)?;
                sink.pieces(&suffix_pieces)?;
            }
            Align::Center => {
                sink.repeat(self.fill, head)?;
                sink.pieces(&prefix_pieces)?;
                sink.text(value_part)?;
                sink.pieces(&suffix_pieces)?;
                sink.repeat(self.fill, padding - head)?;
            }
            Align::Right => {
                sink.repeat(self.fill, padding)?;
                sink.pieces(&prefix_pieces)?;
                sink.text(value_part)?;
                sink.pieces(&suffix_pieces)?;
            }
        }
        Ok(())
    }

    /// `k * value` for a `formatPrefix` formatter, and `value` for every other one.
    ///
    /// Section 4.4 pins the multiplication: `k` is a bit-pinned `Math.pow(10, -e)`
    /// and the operation is a multiplication, never a division by `10^e`. A NaN
    /// scale -- which a zero, NaN or infinite reference produces -- makes every
    /// value NaN, which is how `formatPrefix` reports a reference it cannot bucket.
    fn scaled(&self, value: f64) -> f64 {
        match self.prefix_scale {
            Some(bits) => f64::from_bits(bits) * value,
            None => value,
        }
    }

    /// `formatTypes[type](Math.abs(value), precision)`.
    ///
    /// The second half of the answer is `formatPrefixAuto`'s chosen SI exponent,
    /// which d3 leaves in a module-level variable and this returns.
    fn convert(
        &self,
        magnitude: f64,
        limits: &FormatLimits,
    ) -> Result<(String, Option<i32>), FormatError> {
        let precision = self.precision;
        let text = match self.resolved_type {
            FormatType::Percent => {
                decimal::to_fixed(decimal::percent_scaled(magnitude), precision, limits)?
            }
            FormatType::Binary => decimal::round_to_string_radix(magnitude, 2, limits)?,
            FormatType::Decimal => decimal::format_decimal(magnitude, limits)?,
            FormatType::Exponent => decimal::to_exponential(magnitude, Some(precision), limits)?,
            FormatType::Fixed => decimal::to_fixed(magnitude, precision, limits)?,
            FormatType::Octal => decimal::round_to_string_radix(magnitude, 8, limits)?,
            FormatType::RoundedPercent => {
                decimal::format_rounded(decimal::percent_scaled(magnitude), precision, limits)?
            }
            FormatType::Rounded => decimal::format_rounded(magnitude, precision, limits)?,
            FormatType::SiPrefix => {
                return decimal::format_prefix_auto(magnitude, precision, limits)
            }
            FormatType::HexUpper => {
                // `Math.round(x).toString(16).toUpperCase()`, and the `0x` the `#`
                // symbol adds stays lowercase; section 4.4 lists that as a thing not
                // to normalize. The radix output is ASCII, including "Infinity", so
                // ASCII uppercasing is `toUpperCase` exactly.
                let mut text = decimal::round_to_string_radix(magnitude, 16, limits)?;
                text.make_ascii_uppercase();
                text
            }
            FormatType::HexLower => decimal::round_to_string_radix(magnitude, 16, limits)?,
            // `Formatter::compile` resolves `n`, the empty type and an unknown letter
            // to `g`, and the input-kind gate keeps `c` off the numeric path, so
            // these arms are unreachable through the public API. They are spelled
            // rather than made a panic because section 5.4 requires that no accepted
            // input can panic, and each one is the conversion its type resolves to.
            FormatType::General
            | FormatType::Grouped
            | FormatType::None
            | FormatType::Unknown(_) => decimal::to_precision(magnitude, precision, limits)?,
            FormatType::Character => decimal::number_to_string(magnitude, limits)?,
        };
        Ok((text, None))
    }
}

// ---------------------------------------------------------------------------
// The individual steps
// ---------------------------------------------------------------------------

/// The number of UTF-16 code units in `text`.
///
/// Section 4.4 requires this measure specifically. It is neither `text.len()`, which
/// counts UTF-8 bytes, nor `text.chars().count()`, which counts scalar values:
/// `"\u{1f600}"` is 4, 1 and 2 under the three.
fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The SI prefix for a `formatPrefixAuto` exponent.
///
/// The exponent is always a multiple of three in `[-24, 24]`, so the index is in
/// range; `get` rather than an index keeps a future change from turning an
/// arithmetic slip into a panic.
fn si_prefix_of(exponent: i32) -> &'static str {
    let index = exponent.div_euclid(3) + 8;
    usize::try_from(index)
        .ok()
        .and_then(|index| SI_PREFIXES.get(index))
        .copied()
        .unwrap_or("")
}

/// `src/formatTrim.js`, returning `None` when nothing is removed.
///
/// The JavaScript scans from index 1 and remembers two positions: `i0`, where a run
/// of insignificant characters starts, and `i1`, where it ends. A `.` opens a run, a
/// `0` extends it or opens a new one after a significant digit, a digit `1`-`9`
/// closes the run without discarding the fact that one was seen, and anything else
/// -- an `e`, a letter, a sign -- stops the scan. The result is the text with
/// `[i0, i1]` cut out.
fn trim_insignificant(text: &str) -> Option<String> {
    // `i0`: `None` with `ended == false` is d3's -1, `None` with `ended == true` is
    // its 0, and `Some(offset)` is a real position. Only the last one trims.
    let mut start: Option<usize> = None;
    let mut ended = false;
    let mut last = 0usize;

    for (offset, character) in text.char_indices().skip(1) {
        match character {
            '.' => {
                start = Some(offset);
                ended = false;
                last = offset;
            }
            '0' => {
                if ended {
                    start = Some(offset);
                    ended = false;
                }
                last = offset;
            }
            '1'..='9' => {
                if start.is_some() {
                    start = None;
                    ended = true;
                }
            }
            // `if (!+s[i]) break out`: every other character coerces to NaN or to
            // zero, both falsy, and ends the scan.
            _ => break,
        }
    }

    let start = start?;
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    // `i1` only ever points at a `.` or a `0`, so the next position is one byte on.
    out.push_str(&text[last + 1..]);
    Some(out)
}

/// The `maybeSuffix` split: the digits, the locale decimal point if the break was a
/// `.`, and the rest of the text.
fn split_at_first_non_digit<'a>(body: &'a str, locale: &'a Locale) -> (&'a str, &'a str, &'a str) {
    match body.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        None => (body, "", ""),
        Some((at, '.')) => (&body[..at], locale.decimal(), &body[at + 1..]),
        Some((at, _)) => (&body[..at], "", &body[at..]),
    }
}

/// `src/formatGroup.js`, over UTF-16 code units.
///
/// Working in code units rather than in `char`s is what makes `substring` here mean
/// what it means in JavaScript. The reassembly is lossy in exactly one case: a
/// supplementary-plane character split by a group boundary leaves an unpaired
/// surrogate, which a Rust `String` cannot hold and which any UTF-8 encoder turns
/// into U+FFFD anyway. Nothing d3-reachable produces one -- every grouped value is
/// ASCII digits or radix digits, and the fill character is confined to the Basic
/// Multilingual Plane by the parser.
fn group_units(
    units: &[u16],
    sizes: &[u32],
    thousands: &str,
    width: Option<i64>,
    limits: &FormatLimits,
) -> Result<String, FormatError> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    // Every iteration consumes at least one code unit, so there cannot be more spans
    // than there are units.
    spans
        .try_reserve(units.len())
        .map_err(|_| FormatError::AllocationFailed)?;

    let mut index: i64 = units.len() as i64;
    let mut cursor = 0usize;
    let mut size: i64 = sizes.first().copied().map_or(0, i64::from);
    let mut length: i64 = 0;

    while index > 0 && size > 0 {
        if let Some(budget) = width {
            if length + size + 1 > budget {
                size = (budget - length).max(1);
            }
        }
        let end = index as usize;
        index -= size;
        // `value.substring(i -= g, i + g)`: `substring` clamps a negative start to
        // zero, so the last group is whatever is left.
        spans.push((index.max(0) as usize, end));
        length += size + 1;
        if let Some(budget) = width {
            if length > budget {
                break;
            }
        }
        cursor = (cursor + 1) % sizes.len();
        size = i64::from(sizes[cursor]);
    }

    let separators = spans.len().saturating_sub(1);
    let total = spans
        .iter()
        .try_fold(0usize, |total, (start, end)| total.checked_add(end - start))
        .and_then(|total| total.checked_add(separators.checked_mul(utf16_len(thousands))?))
        .ok_or(FormatError::AllocationFailed)?;
    // One code unit is at least one UTF-8 byte, so the output budget bounds this
    // buffer too.
    if total > limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: total,
            maximum: limits.max_output_bytes,
        });
    }

    let mut joined: Vec<u16> = Vec::new();
    joined
        .try_reserve(total)
        .map_err(|_| FormatError::AllocationFailed)?;
    for (position, (start, end)) in spans.iter().rev().enumerate() {
        if position > 0 {
            joined.extend(thousands.encode_utf16());
        }
        joined.extend_from_slice(&units[*start..*end]);
    }
    Ok(String::from_utf16_lossy(&joined))
}

/// `text` as UTF-16 code units, allocated fallibly.
fn utf16_units(text: &str, limits: &FormatLimits) -> Result<Vec<u16>, FormatError> {
    let length = utf16_len(text);
    if length > limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: length,
            maximum: limits.max_output_bytes,
        });
    }
    let mut units = Vec::new();
    units
        .try_reserve(length)
        .map_err(|_| FormatError::AllocationFailed)?;
    units.extend(text.encode_utf16());
    Ok(units)
}

/// `padding + value` as UTF-16 code units, for the zero-fill grouping path.
fn padded_units(
    fill: char,
    padding: usize,
    value: &str,
    limits: &FormatLimits,
) -> Result<Vec<u16>, FormatError> {
    let length = padding
        .checked_mul(fill.len_utf16())
        .and_then(|fill_units| fill_units.checked_add(utf16_len(value)))
        .ok_or(FormatError::AllocationFailed)?;
    if length > limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: length,
            maximum: limits.max_output_bytes,
        });
    }
    let mut units = Vec::new();
    units
        .try_reserve(length)
        .map_err(|_| FormatError::AllocationFailed)?;
    let mut buffer = [0u16; 2];
    // The parser refuses a fill outside the Basic Multilingual Plane, so this is one
    // code unit in practice; encoding it properly costs nothing and keeps the length
    // arithmetic above honest either way.
    let fill_units = fill.encode_utf16(&mut buffer);
    for _ in 0..padding {
        units.extend_from_slice(fill_units);
    }
    units.extend(value.encode_utf16());
    Ok(units)
}

/// `src/formatNumerals.js`: `value.replace(/[0-9]/g, i => numerals[+i])`.
fn substitute_numerals(
    assembled: &str,
    numerals: &[String; 10],
    limits: &FormatLimits,
    out: &mut String,
) -> Result<(), FormatError> {
    let mut sink = Sink {
        out,
        maximum: limits.max_output_bytes,
    };
    for character in assembled.chars() {
        // `/[0-9]/` is the ASCII digits only: a locale's own numerals are not
        // re-substituted, and neither is any other Unicode decimal digit.
        if character.is_ascii_digit() {
            sink.text(&numerals[(character as u8 - b'0') as usize])?;
        } else {
            sink.character(character)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// `+value === 0`
// ---------------------------------------------------------------------------

/// `+text === 0`, the test that hides the sign of a negative value that formatted to
/// zero.
///
/// The subject is the *formatted string*, not the value, which is why this exists at
/// all: `format(".2f")(-0.001)` is `"-0.00"` in a naive port and `"0.00"` in d3.
fn formats_as_zero(text: &str) -> bool {
    js_number(text) == 0.0
}

/// ECMAScript's `ToNumber` applied to a string.
///
/// Complete rather than narrowed to the strings a conversion can produce, because
/// the NaN text is a locale's to choose and can be anything at all. Only the
/// zero-or-not answer is used, and for a well-formed decimal literal Rust's parser
/// and V8's are both correctly rounded, so they agree about which literals are zero.
fn js_number(text: &str) -> f64 {
    let text = text.trim_matches(is_js_whitespace);
    if text.is_empty() {
        // `Number("")` and `Number("   ")` are both `+0`.
        return 0.0;
    }
    if let Some((radix, digits)) = radix_literal(text) {
        return parse_radix(digits, radix);
    }
    parse_decimal_literal(text)
}

/// ECMAScript `WhiteSpace` and `LineTerminator`, which `ToNumber` strips from both
/// ends of a string.
fn is_js_whitespace(character: char) -> bool {
    // U+0009 through U+000D is `<TAB> <LF> <VT> <FF> <CR>`, and U+2000 through
    // U+200A is the `Zs` run from EN QUAD to HAIR SPACE. Both are spelled as ranges
    // because rustfmt will not leave a range in the middle of an or-pattern alone.
    matches!(
        character,
        '\u{9}'..='\u{d}' | '\u{2000}'..='\u{200a}'
    ) || matches!(
        character,
        '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// The `0b`, `0o` and `0x` prefixes `ToNumber` accepts, which take no sign.
fn radix_literal(text: &str) -> Option<(u32, &str)> {
    let prefix = text.get(..2)?;
    let digits = text.get(2..)?;
    match prefix {
        "0b" | "0B" => Some((2, digits)),
        "0o" | "0O" => Some((8, digits)),
        "0x" | "0X" => Some((16, digits)),
        _ => None,
    }
}

fn parse_radix(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut value = 0.0f64;
    for character in digits.chars() {
        match character.to_digit(radix) {
            None => return f64::NAN,
            Some(digit) => value = value * f64::from(radix) + f64::from(digit),
        }
    }
    value
}

/// `StrDecimalLiteral`, which is narrower than Rust's float syntax: it spells
/// infinity as exactly `Infinity` and admits no `nan`, no underscores and no
/// hexadecimal float.
fn parse_decimal_literal(text: &str) -> f64 {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    if matches!(bytes.first(), Some(b'+') | Some(b'-')) {
        at = 1;
    }
    let unsigned = &text[at..];
    if unsigned == "Infinity" {
        return if bytes[0] == b'-' {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    let mut integer = 0usize;
    while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
        at += 1;
        integer += 1;
    }
    let mut fraction = 0usize;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
            at += 1;
            fraction += 1;
        }
    }
    if integer == 0 && fraction == 0 {
        return f64::NAN;
    }
    if matches!(bytes.get(at), Some(b'e') | Some(b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+') | Some(b'-')) {
            at += 1;
        }
        let exponent_start = at;
        while matches!(bytes.get(at), Some(byte) if byte.is_ascii_digit()) {
            at += 1;
        }
        if at == exponent_start {
            return f64::NAN;
        }
    }
    if at != bytes.len() {
        return f64::NAN;
    }
    text.parse::<f64>().unwrap_or(f64::NAN)
}

// ---------------------------------------------------------------------------
// Bounded output
// ---------------------------------------------------------------------------

/// A `String` that respects [`FormatLimits::max_output_bytes`] and grows through
/// `try_reserve`.
///
/// Section 5.4 requires checked length arithmetic and fallible allocation on every
/// dynamic output buffer, and counts locale numerals, affixes, separators and text
/// input against the same budget. Padding is what makes this more than a formality:
/// a width of one million and a three-byte fill character is three megabytes before
/// any digit is written.
struct Sink<'a> {
    out: &'a mut String,
    maximum: usize,
}

impl Sink<'_> {
    fn reserve(&mut self, additional: usize) -> Result<(), FormatError> {
        let total = self
            .out
            .len()
            .checked_add(additional)
            .ok_or(FormatError::AllocationFailed)?;
        if total > self.maximum {
            return Err(FormatError::OutputLimit {
                requested: total,
                maximum: self.maximum,
            });
        }
        self.out
            .try_reserve(additional)
            .map_err(|_| FormatError::AllocationFailed)
    }

    fn text(&mut self, text: &str) -> Result<(), FormatError> {
        self.reserve(text.len())?;
        self.out.push_str(text);
        Ok(())
    }

    fn character(&mut self, character: char) -> Result<(), FormatError> {
        self.reserve(character.len_utf8())?;
        self.out.push(character);
        Ok(())
    }

    fn pieces(&mut self, pieces: &[&str]) -> Result<(), FormatError> {
        for piece in pieces {
            self.text(piece)?;
        }
        Ok(())
    }

    fn repeat(&mut self, character: char, count: usize) -> Result<(), FormatError> {
        if count == 0 {
            return Ok(());
        }
        let bytes = character
            .len_utf8()
            .checked_mul(count)
            .ok_or(FormatError::AllocationFailed)?;
        self.reserve(bytes)?;
        for _ in 0..count {
            self.out.push(character);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_length_is_neither_bytes_nor_chars() {
        assert_eq!("\u{1f600}".len(), 4);
        assert_eq!("\u{1f600}".chars().count(), 1);
        assert_eq!(utf16_len("\u{1f600}"), 2);
        assert_eq!(utf16_len("\u{2212}"), 1);
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len(""), 0);
    }

    #[test]
    fn trim_follows_format_trims_two_position_scan() {
        let trim = |text: &str| trim_insignificant(text).unwrap_or_else(|| text.to_owned());
        assert_eq!(trim("1.20000"), "1.2");
        assert_eq!(trim("1.000"), "1");
        assert_eq!(trim("100.00"), "100");
        assert_eq!(trim("0.500"), "0.5");
        // A trailing run of zeros in the *integer* part is significant.
        assert_eq!(trim("10000"), "10000");
        // The exponent stops the scan and survives.
        assert_eq!(trim("1.20e+21"), "1.2e+21");
        assert_eq!(trim("1.2e+21"), "1.2e+21");
        // Nothing to trim, and nothing that could be mistaken for a digit.
        assert_eq!(trim("Infinity"), "Infinity");
        assert_eq!(trim("NaN"), "NaN");
        assert_eq!(trim("\u{221e}"), "\u{221e}");
        assert_eq!(trim(""), "");
        assert_eq!(trim("0"), "0");
        // Index 0 is never a run start, so a leading zero is never removed.
        assert_eq!(trim("0.0"), "0");
    }

    #[test]
    fn the_zero_test_reads_the_formatted_string_not_the_value() {
        assert!(formats_as_zero("0"));
        assert!(formats_as_zero("0.00"));
        assert!(formats_as_zero("0.000e+0"));
        assert!(formats_as_zero("0e-7"));
        assert!(formats_as_zero(""));
        assert!(formats_as_zero("-0"));
        assert!(!formats_as_zero("0.01"));
        assert!(!formats_as_zero("NaN"));
        assert!(!formats_as_zero("Infinity"));
        // Type `d`'s infinity, and a hexadecimal digit run.
        assert!(!formats_as_zero("\u{221e}"));
        assert!(!formats_as_zero("deadbeef"));
        // `Number` accepts the radix prefixes and rejects a bare letter.
        assert!(formats_as_zero("0x0"));
        assert!(!formats_as_zero("0x10"));
        assert!(!formats_as_zero("b"));
    }

    #[test]
    fn js_number_matches_ecmascript_to_number_where_it_differs_from_rust() {
        // Rust's parser accepts all three of these; ECMAScript accepts none.
        assert!(js_number("inf").is_nan());
        assert!(js_number("infinity").is_nan());
        assert!(js_number("NaN").is_nan());
        assert_eq!(js_number("Infinity"), f64::INFINITY);
        assert_eq!(js_number("-Infinity"), f64::NEG_INFINITY);
        // Rust rejects the empty string; ECMAScript reads it as +0.
        assert_eq!(js_number(" \t\n "), 0.0);
        assert_eq!(js_number("1."), 1.0);
        assert_eq!(js_number(".5"), 0.5);
        assert!(js_number("1e").is_nan());
        assert!(js_number("1e+").is_nan());
        assert!(js_number(".").is_nan());
        assert!(js_number("1_0").is_nan());
        // Underflow really does reach zero, which is why the test is a parse rather
        // than a scan for non-zero digits.
        assert_eq!(js_number("1e-400"), 0.0);
        assert_ne!(js_number("4.94e-324"), 0.0);
    }

    #[test]
    fn grouping_is_d3s_substring_walk() {
        let limits = FormatLimits::DEFAULT;
        let group = |text: &str, sizes: &[u32], width: Option<i64>| {
            group_units(
                &utf16_units(text, &limits).unwrap(),
                sizes,
                ",",
                width,
                &limits,
            )
            .unwrap()
        };
        assert_eq!(group("1234567", &[3], None), "1,234,567");
        assert_eq!(group("12", &[3], None), "12");
        assert_eq!(group("", &[3], None), "");
        // A list of sizes cycles rather than repeating its last entry, so `[3, 2]`
        // is 3, 2, 3, 2 from the right and not the Indian 3, 2, 2, 2. Checked
        // against `formatLocale({grouping: [3, 2]}).format(",d")` in Node.
        assert_eq!(group("1234567", &[3, 2], None), "12,34,567");
        assert_eq!(group("1234567890", &[3, 2], None), "12,345,67,890");
        assert_eq!(group("123456789", &[2, 3], None), "12,34,567,89");
        // The width budget clamps the innermost group, which is what zero fill uses.
        assert_eq!(group("00001234", &[3], Some(8)), "0,001,234");
    }

    #[test]
    fn the_si_prefix_index_uses_floor_division() {
        // -3 / 3 and -3.div_euclid(3) agree, but the table index has to survive the
        // whole negative half of the range.
        assert_eq!(si_prefix_of(-24), "y");
        assert_eq!(si_prefix_of(-6), "\u{b5}");
        assert_eq!(si_prefix_of(0), "");
        assert_eq!(si_prefix_of(3), "k");
        assert_eq!(si_prefix_of(24), "Y");
        // Out of range answers with no suffix rather than panicking.
        assert_eq!(si_prefix_of(27), "");
        assert_eq!(si_prefix_of(-27), "");
    }
}
