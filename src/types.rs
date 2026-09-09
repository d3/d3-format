//! The small typed values the specifier grammar and the formatter are built from.
//!
//! d3 keeps all of these as strings on a mutable object: `align` is `">"`, `type` is
//! `"f"`, an absent symbol is `""`. MIGRATION_TO_RUST.md section 5.2 replaces that
//! with typed fields, which is what makes `FormatSpecifier` immutable after
//! validated construction rather than merely conventionally so. Every type here
//! knows its own d3 spelling, so `Display for FormatSpecifier` is assembled from
//! them rather than from a parallel table that could drift.

use core::fmt;

/// Where padding goes relative to the value, from the `[<>=^]` group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Align {
    /// `<` -- the value is left-aligned and padding follows it.
    Left,
    /// `>` -- the value is right-aligned and padding precedes it. d3's default.
    #[default]
    Right,
    /// `^` -- the value is centred within the padding.
    Center,
    /// `=` -- padding goes between the sign and the digits.
    AfterSign,
}

impl Align {
    /// The d3 spelling of this alignment.
    pub const fn as_char(self) -> char {
        match self {
            Align::Left => '<',
            Align::Right => '>',
            Align::Center => '^',
            Align::AfterSign => '=',
        }
    }

    /// The alignment `value` spells, or `None` if it spells none.
    pub const fn from_char(value: char) -> Option<Align> {
        match value {
            '<' => Some(Align::Left),
            '>' => Some(Align::Right),
            '^' => Some(Align::Center),
            '=' => Some(Align::AfterSign),
            _ => None,
        }
    }
}

impl fmt::Display for Align {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Align::Left => "<",
            Align::Right => ">",
            Align::Center => "^",
            Align::AfterSign => "=",
        })
    }
}

/// How the sign of a value is spelled, from the `[+\-( ]` group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Sign {
    /// `-` -- negatives take the locale's minus sign, positives take nothing.
    /// d3's default.
    #[default]
    Minus,
    /// `+` -- positives take a `+`.
    Plus,
    /// `(` -- negatives are parenthesized instead of signed.
    Parens,
    /// ` ` -- positives take a space, so columns line up.
    Space,
}

impl Sign {
    /// The d3 spelling of this sign policy.
    pub const fn as_char(self) -> char {
        match self {
            Sign::Minus => '-',
            Sign::Plus => '+',
            Sign::Parens => '(',
            Sign::Space => ' ',
        }
    }

    /// The sign policy `value` spells, or `None` if it spells none.
    pub const fn from_char(value: char) -> Option<Sign> {
        match value {
            '-' => Some(Sign::Minus),
            '+' => Some(Sign::Plus),
            '(' => Some(Sign::Parens),
            ' ' => Some(Sign::Space),
            _ => None,
        }
    }
}

impl fmt::Display for Sign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sign::Minus => "-",
            Sign::Plus => "+",
            Sign::Parens => "(",
            Sign::Space => " ",
        })
    }
}

/// The optional `[$#]` symbol.
///
/// This is one of the places d3's "everything is a string" model shows: an absent
/// symbol is the empty string, which is why [`Symbol::None`] exists rather than the
/// type being wrapped in an `Option`. Keeping the absent case inside the enum lets
/// the builder take a `Symbol` directly and lets `Display` treat all three cases
/// alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Symbol {
    /// No symbol. d3's default.
    #[default]
    None,
    /// `$` -- wrap the value in the locale's currency affixes.
    Currency,
    /// `#` -- prefix `0b`, `0o` or `0x` for the binary, octal and hexadecimal types.
    BasePrefix,
}

impl Symbol {
    /// The d3 spelling of this symbol, or `None` for [`Symbol::None`].
    pub const fn as_char(self) -> Option<char> {
        match self {
            Symbol::None => None,
            Symbol::Currency => Some('$'),
            Symbol::BasePrefix => Some('#'),
        }
    }

    /// The symbol `value` spells, or `None` if it spells none.
    ///
    /// This never yields [`Symbol::None`]: absence is the lack of a character, not
    /// a character of its own.
    pub const fn from_char(value: char) -> Option<Symbol> {
        match value {
            '$' => Some(Symbol::Currency),
            '#' => Some(Symbol::BasePrefix),
            _ => None,
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Symbol::None => "",
            Symbol::Currency => "$",
            Symbol::BasePrefix => "#",
        })
    }
}

/// The conversion a specifier selects, from the `[a-z%]` group with the `i` flag.
///
/// d3 stores the type letter verbatim and looks it up in `src/formatTypes.js` at
/// format time, so an unrecognized letter is not an error: `format("q")` builds a
/// formatter whose behaviour is `.12~g`. Section 5.2 keeps that, which is why
/// [`FormatType::Unknown`] exists.
///
/// # Invariants
///
/// [`FormatType::Unknown`] only ever carries an ASCII letter that is not one of the
/// letters the other variants already name. Both [`FormatType::from_char`] and the
/// specifier builder canonicalize, so a `FormatType` read back out of a
/// `FormatSpecifier` never spells a known type as `Unknown`. Writing
/// `FormatType::Unknown('f')` by hand is not a supported spelling of
/// [`FormatType::Fixed`]; hand it to the builder and it becomes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FormatType {
    /// The empty type. d3 treats it as `.12~g`, the same as an unknown letter,
    /// except that it renders as nothing.
    #[default]
    None,
    /// `%` -- multiply by 100 and append the locale's percent sign.
    Percent,
    /// `b` -- binary, after rounding to an integer.
    Binary,
    /// `c` -- character data, emitted verbatim. The only type that takes text.
    Character,
    /// `d` -- decimal integer, after rounding.
    Decimal,
    /// `e` -- exponent notation.
    Exponent,
    /// `f` -- fixed point.
    Fixed,
    /// `g` -- significant digits, in fixed or exponent notation.
    General,
    /// `n` -- an alias for `,g`.
    Grouped,
    /// `o` -- octal, after rounding to an integer.
    Octal,
    /// `p` -- a percentage rounded to significant digits.
    RoundedPercent,
    /// `r` -- rounded to significant digits, never in exponent notation.
    Rounded,
    /// `s` -- an SI prefix, chosen from the value.
    SiPrefix,
    /// `X` -- uppercase hexadecimal, after rounding to an integer.
    HexUpper,
    /// `x` -- lowercase hexadecimal, after rounding to an integer.
    HexLower,
    /// Any other ASCII letter. d3 formats it as `.12~g` and keeps the letter.
    Unknown(char),
}

impl FormatType {
    /// The type `value` spells, or `None` if the grammar does not admit `value`.
    ///
    /// The grammar admits `%` and the ASCII letters, and nothing else. A letter
    /// with a conversion behind it yields that conversion's variant, so this never
    /// returns `Unknown` for a known letter.
    pub const fn from_char(value: char) -> Option<FormatType> {
        match value {
            '%' => Some(FormatType::Percent),
            'b' => Some(FormatType::Binary),
            'c' => Some(FormatType::Character),
            'd' => Some(FormatType::Decimal),
            'e' => Some(FormatType::Exponent),
            'f' => Some(FormatType::Fixed),
            'g' => Some(FormatType::General),
            'n' => Some(FormatType::Grouped),
            'o' => Some(FormatType::Octal),
            'p' => Some(FormatType::RoundedPercent),
            'r' => Some(FormatType::Rounded),
            's' => Some(FormatType::SiPrefix),
            'X' => Some(FormatType::HexUpper),
            'x' => Some(FormatType::HexLower),
            _ if value.is_ascii_alphabetic() => Some(FormatType::Unknown(value)),
            _ => None,
        }
    }

    /// The d3 spelling of this type, or `None` for [`FormatType::None`].
    pub const fn as_char(self) -> Option<char> {
        match self {
            FormatType::None => None,
            FormatType::Percent => Some('%'),
            FormatType::Binary => Some('b'),
            FormatType::Character => Some('c'),
            FormatType::Decimal => Some('d'),
            FormatType::Exponent => Some('e'),
            FormatType::Fixed => Some('f'),
            FormatType::General => Some('g'),
            FormatType::Grouped => Some('n'),
            FormatType::Octal => Some('o'),
            FormatType::RoundedPercent => Some('p'),
            FormatType::Rounded => Some('r'),
            FormatType::SiPrefix => Some('s'),
            FormatType::HexUpper => Some('X'),
            FormatType::HexLower => Some('x'),
            FormatType::Unknown(letter) => Some(letter),
        }
    }

    /// Whether `src/formatTypes.js` has a conversion for this type.
    ///
    /// This is d3's `formatTypes[type]` truth test, and it drives the `.12~g`
    /// fallback. Note that [`FormatType::Grouped`] is *not* in that table: `n` is
    /// rewritten to `,g` a line earlier, so it never reaches the fallback.
    pub const fn is_known(self) -> bool {
        !matches!(
            self,
            FormatType::None | FormatType::Grouped | FormatType::Unknown(_)
        )
    }

    /// The kind of input this type consumes.
    ///
    /// Only `c` takes text. Section 5.3 makes the distinction typed rather than
    /// coerced, so a formatter built for any other type refuses text and a `c`
    /// formatter refuses numbers.
    pub const fn input_kind(self) -> InputKind {
        match self {
            FormatType::Character => InputKind::Text,
            _ => InputKind::Number,
        }
    }
}

impl fmt::Display for FormatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_char() {
            None => Ok(()),
            Some(letter) => write!(f, "{letter}"),
        }
    }
}

/// What a [`Formatter`](crate::Formatter) accepts.
///
/// d3's formatters accept anything and coerce it; section 5.3 replaces that with a
/// typed refusal, reported as
/// [`FormatError::InputTypeMismatch`](crate::FormatError::InputTypeMismatch). See
/// the `no-input-coercion` entry in `DIVERGENCES.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// An `f64`, through
    /// [`format_number`](crate::Formatter::format_number).
    Number,
    /// A `&str`, through [`format_text`](crate::Formatter::format_text).
    Text,
}

impl fmt::Display for InputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            InputKind::Number => "numeric",
            InputKind::Text => "text",
        })
    }
}

/// The SI prefixes `src/locale.js` indexes with `8 + exponent / 3`.
///
/// The micro sign is U+00B5 MICRO SIGN, not U+03BC GREEK SMALL LETTER MU; d3's
/// source uses the former and the two are different strings.
pub(crate) const SI_PREFIXES: [&str; 17] = [
    "y", "z", "a", "f", "p", "n", "\u{b5}", "m", "", "k", "M", "G", "T", "P", "E", "Z", "Y",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_align_round_trips_through_its_character() {
        for align in [Align::Left, Align::Right, Align::Center, Align::AfterSign] {
            assert_eq!(Align::from_char(align.as_char()), Some(align));
            assert_eq!(align.to_string(), align.as_char().to_string());
        }
        assert_eq!(Align::from_char('x'), None);
        assert_eq!(Align::default(), Align::Right);
    }

    #[test]
    fn every_sign_round_trips_through_its_character() {
        for sign in [Sign::Minus, Sign::Plus, Sign::Parens, Sign::Space] {
            assert_eq!(Sign::from_char(sign.as_char()), Some(sign));
            assert_eq!(sign.to_string(), sign.as_char().to_string());
        }
        assert_eq!(Sign::from_char(')'), None);
        assert_eq!(Sign::default(), Sign::Minus);
    }

    #[test]
    fn symbol_absence_is_not_a_character() {
        assert_eq!(Symbol::None.as_char(), None);
        assert_eq!(Symbol::None.to_string(), "");
        assert_eq!(Symbol::from_char('$'), Some(Symbol::Currency));
        assert_eq!(Symbol::from_char('#'), Some(Symbol::BasePrefix));
        assert_eq!(Symbol::from_char('\0'), None);
        assert_eq!(Symbol::default(), Symbol::None);
    }

    #[test]
    fn from_char_canonicalizes_every_known_type_letter() {
        // The grammar's whole alphabet, so a letter cannot be quietly dropped from
        // the table without this noticing.
        for letter in ('a'..='z').chain('A'..='Z').chain(core::iter::once('%')) {
            let parsed = FormatType::from_char(letter).expect("the grammar admits it");
            assert_eq!(parsed.as_char(), Some(letter), "round trip for {letter:?}");
            match parsed {
                FormatType::Unknown(carried) => {
                    assert_eq!(carried, letter);
                    assert!(!parsed.is_known());
                }
                known => assert_eq!(FormatType::from_char(letter), Some(known)),
            }
        }
    }

    #[test]
    fn from_char_rejects_everything_outside_the_grammar() {
        for letter in ['0', '.', ',', '~', '-', ' ', '\u{b5}', '\u{1f600}'] {
            assert_eq!(FormatType::from_char(letter), None, "{letter:?}");
        }
    }

    #[test]
    fn the_shorthand_and_alias_types_are_not_in_the_conversion_table() {
        // `formatTypes[type]` is falsy for "" and for an unknown letter, which is
        // what selects the `.12~g` fallback, and `n` is rewritten before the test
        // is reached.
        assert!(!FormatType::None.is_known());
        assert!(!FormatType::Grouped.is_known());
        assert!(!FormatType::Unknown('q').is_known());
        assert!(FormatType::Fixed.is_known());
        assert_eq!(FormatType::None.to_string(), "");
        assert_eq!(FormatType::default(), FormatType::None);
    }

    #[test]
    fn only_the_character_type_consumes_text() {
        assert_eq!(FormatType::Character.input_kind(), InputKind::Text);
        for other in [
            FormatType::None,
            FormatType::Percent,
            FormatType::Decimal,
            FormatType::Grouped,
            FormatType::Unknown('q'),
        ] {
            assert_eq!(other.input_kind(), InputKind::Number, "{other:?}");
        }
        assert_eq!(InputKind::Number.to_string(), "numeric");
        assert_eq!(InputKind::Text.to_string(), "text");
    }

    #[test]
    fn the_si_prefix_table_is_d3s() {
        assert_eq!(SI_PREFIXES[8], "");
        assert_eq!(SI_PREFIXES[6], "\u{b5}");
        assert_ne!(SI_PREFIXES[6], "\u{3bc}");
        assert_eq!(SI_PREFIXES[0], "y");
        assert_eq!(SI_PREFIXES[16], "Y");
    }
}
