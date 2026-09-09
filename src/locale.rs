//! Owned locales and the compiled formatters they produce.
//!
//! This is MIGRATION_TO_RUST.md section 5.3. `src/locale.js` is a closure factory
//! over a mutable definition object; the port is an `Arc`-backed immutable value
//! whose formatters own a cheap clone of it, so a `Formatter` has no lifetime
//! parameter and nothing in the crate is a process-global.
//!
//! # Resolution here, layout next door
//!
//! `newFormat` in `src/locale.js` does two separable jobs. It *resolves* a
//! specifier -- rewriting `n` to `,g` and an unknown type to `.12~g`, folding fill
//! `0` with align `=` into the zero flag, composing the affixes, and defaulting and
//! clamping the precision -- and it then returns a closure that *lays out* a value
//! using that resolution. This module is the first half: [`Formatter::compile`]
//! produces the [`Layout`] that resolution amounts to. The second half -- type
//! dispatch, trimming, sign policy, the SI suffix, grouping, UTF-16 padding,
//! alignment and numerals -- is [`crate::layout`], which the two `format_*` entry
//! points hand off to once the input-kind gate has passed.

use std::sync::{Arc, LazyLock};

use crate::decimal;
use crate::layout::Layout;
use crate::limits::{FormatError, FormatLimits};
use crate::specifier::FormatSpecifier;
use crate::types::{Align, FormatType, InputKind, Symbol, SI_PREFIXES};

/// d3's defaults for the keys a definition omits, from `src/locale.js`.
const DEFAULT_DECIMAL: &str = ".";
const DEFAULT_PERCENT: &str = "%";
const DEFAULT_MINUS: &str = "\u{2212}";
const DEFAULT_NAN: &str = "NaN";

/// A locale definition in the shape d3's `locale/*.json` files use.
///
/// Every field is optional and `None` means the key was absent, which is not the
/// same as an empty string: an absent key falls back to d3's default, and grouping
/// is disabled unless both `grouping` and `thousands` are present.
///
/// Under the `serde` feature this is the stable wire shape, and it round-trips the
/// bundled locale JSON: absent keys deserialize to `None` and serialize back to
/// nothing at all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct LocaleDefinition {
    /// The decimal point. Defaults to `.`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub decimal: Option<String>,
    /// The group separator. Grouping needs this *and* `grouping`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub thousands: Option<String>,
    /// Group sizes, innermost first, cycling on the last. Needs `thousands`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub grouping: Option<Vec<u32>>,
    /// The currency prefix and suffix. Both default to empty.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub currency: Option<[String; 2]>,
    /// Replacements for the ASCII digits, applied last. Exactly ten.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub numerals: Option<[String; 10]>,
    /// The percent sign. Defaults to `%`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub percent: Option<String>,
    /// The minus sign. Defaults to U+2212 MINUS SIGN, not the ASCII hyphen.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub minus: Option<String>,
    /// The not-a-number text. Defaults to `NaN`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub nan: Option<String>,
}

/// Why a locale definition was refused.
///
/// `formatLocale` performs no validation, so d3 accepts definitions that hang or
/// produce `undefined` mid-string. See `validated-locale-shape` in
/// `DIVERGENCES.md`. Every bundled locale and every definition in the oracle
/// suite passes, so nothing d3 actually formats becomes an error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocaleError {
    /// A grouping size was zero.
    ///
    /// d3's grouping loop consumes `grouping[j]` digits per iteration, so a zero
    /// consumes nothing and the loop never terminates. An *empty* grouping list is
    /// not an error: section 5.3 makes it disable grouping, which is what d3's loop
    /// does in effect, since `grouping[0]` is then `undefined` and `g > 0` fails at
    /// once.
    ZeroGroupingSize {
        /// Position of the offending entry in the grouping list.
        index: usize,
    },
}

impl core::fmt::Display for LocaleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LocaleError::ZeroGroupingSize { index } => write!(
                f,
                "grouping size at index {index} is zero, which would never consume a digit"
            ),
        }
    }
}

impl std::error::Error for LocaleError {}

/// The resolved, immutable locale text a formatter reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LocaleInner {
    decimal: String,
    /// `Some` only when grouping is actually enabled; see [`LocaleInner::grouping`].
    grouping: Option<Grouping>,
    currency: [String; 2],
    numerals: Option<[String; 10]>,
    percent: String,
    minus: String,
    nan: String,
}

/// A group separator together with the sizes it separates.
///
/// The two travel together because neither means anything alone: d3 disables
/// grouping unless both keys are present, so representing them as one optional
/// value makes the disabled state unrepresentable-by-halves.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Grouping {
    sizes: Vec<u32>,
    thousands: String,
}

/// An immutable locale, cheap to clone.
///
/// Cloning copies an `Arc`, so handing a locale to a formatter or to another thread
/// costs a refcount bump. There is no process-global default locale to rebind:
/// [`Locale::en_us`] is a constant, and building another locale cannot affect it.
/// See `no-mutable-default-locale` in `DIVERGENCES.md`.
///
/// # Examples
///
/// ```
/// use d3_format::Locale;
///
/// let locale = Locale::en_us();
/// assert_eq!(locale.decimal(), ".");
/// assert_eq!(locale.grouping(), Some(&[3][..]));
/// assert_eq!(locale.currency_prefix(), "$");
/// assert_eq!(locale.minus(), "\u{2212}");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locale(Arc<LocaleInner>);

/// `Locale::en_us()` handed out repeatedly, without rebuilding it.
///
/// A `LazyLock` holding an immutable value is not a mutable global: nothing can
/// replace it and nothing observes its initialization order. Section 5.3 allows
/// exactly this, and it is available on the 1.80 MSRV.
static EN_US: LazyLock<Locale> = LazyLock::new(|| {
    Locale::builder()
        .decimal(".")
        .thousands(",")
        .grouping([3])
        .currency("$", "")
        .build()
        .expect("the en-US definition is valid")
});

impl Locale {
    /// d3's default locale: `.`, `,`, `[3]` and a `$` currency prefix.
    pub fn en_us() -> Locale {
        EN_US.clone()
    }

    /// A builder holding d3's defaults for every key.
    pub fn builder() -> LocaleBuilder {
        LocaleBuilder::new()
    }

    /// Validates a definition and freezes it.
    ///
    /// # Errors
    ///
    /// [`LocaleError::ZeroGroupingSize`] if any grouping size is zero.
    pub fn from_definition(value: LocaleDefinition) -> Result<Locale, LocaleError> {
        LocaleBuilder { definition: value }.build()
    }

    /// One of the 59 built-in locales, by name, or `None` if there is no such name.
    ///
    /// ```
    /// use d3_format::Locale;
    ///
    /// let locale = Locale::named("en-IN").expect("bundled");
    /// assert_eq!(locale.grouping(), Some(&[3, 2, 2, 2, 2, 2, 2, 2, 2, 2][..]));
    /// assert!(Locale::named("xx-XX").is_none());
    /// ```
    #[cfg(feature = "locales")]
    pub fn named(name: &str) -> Option<Locale> {
        let generated = crate::generated_locales::by_name(name)?;
        // The generator validates the same schema, so this cannot fail; `.ok()`
        // rather than an unwrap so that a future locale that somehow did not is a
        // missing locale, never a panic.
        Locale::from_definition(definition_of(generated)).ok()
    }

    /// The decimal point.
    pub fn decimal(&self) -> &str {
        &self.0.decimal
    }

    /// The group separator, or `""` when grouping is disabled.
    pub fn thousands(&self) -> &str {
        match &self.0.grouping {
            Some(grouping) => &grouping.thousands,
            None => "",
        }
    }

    /// The group sizes, or `None` when grouping is disabled.
    ///
    /// This is `None` whenever d3 would use `identity` instead of `formatGroup`:
    /// when either key was absent, or when the size list was empty.
    pub fn grouping(&self) -> Option<&[u32]> {
        self.0.grouping.as_ref().map(|grouping| &grouping.sizes[..])
    }

    /// The currency prefix.
    pub fn currency_prefix(&self) -> &str {
        &self.0.currency[0]
    }

    /// The currency suffix.
    pub fn currency_suffix(&self) -> &str {
        &self.0.currency[1]
    }

    /// The ten digit replacements, or `None` when the locale uses ASCII digits.
    pub fn numerals(&self) -> Option<&[String; 10]> {
        self.0.numerals.as_ref()
    }

    /// The percent sign.
    pub fn percent(&self) -> &str {
        &self.0.percent
    }

    /// The minus sign.
    pub fn minus(&self) -> &str {
        &self.0.minus
    }

    /// The not-a-number text.
    pub fn nan(&self) -> &str {
        &self.0.nan
    }

    /// Compiles a specifier against this locale, with the default limits.
    ///
    /// # Errors
    ///
    /// [`FormatError::InvalidSpecifier`] if `spec` is not in the grammar, and
    /// [`FormatError::WidthLimit`] if its width is above
    /// [`FormatLimits::max_width_utf16`].
    ///
    /// ```
    /// use d3_format::Locale;
    ///
    /// let formatter = Locale::en_us().formatter("d")?;
    /// assert_eq!(formatter.specifier().to_string(), " >-d");
    ///
    /// let error = Locale::en_us().formatter("foo").unwrap_err();
    /// assert_eq!(error.to_string(), "invalid format: foo");
    /// # Ok::<(), d3_format::FormatError>(())
    /// ```
    pub fn formatter(&self, spec: &str) -> Result<Formatter, FormatError> {
        self.formatter_with_limits(spec, FormatLimits::default())
    }

    /// Compiles a specifier against this locale, with the caller's limits.
    ///
    /// # Errors
    ///
    /// As [`Locale::formatter`], measured against `limits`.
    pub fn formatter_with_limits(
        &self,
        spec: &str,
        limits: FormatLimits,
    ) -> Result<Formatter, FormatError> {
        let specifier: FormatSpecifier = spec.parse()?;
        Formatter::compile(self.clone(), specifier, limits, None, None)
    }

    /// Compiles an already-parsed specifier, with the default limits.
    ///
    /// Exactly equivalent to `formatter(&spec.to_string())`, and implemented as
    /// that call, so it cannot drift from it or skip normalization. Section 5.2
    /// requires the equivalence; `tests/properties.rs` checks it over a generated
    /// corpus rather than taking this note's word for it.
    ///
    /// # Errors
    ///
    /// As [`Locale::formatter`]. A validated `FormatSpecifier` always re-parses, so
    /// in practice only [`FormatError::WidthLimit`] is reachable here.
    pub fn formatter_for(&self, spec: &FormatSpecifier) -> Result<Formatter, FormatError> {
        self.formatter(&spec.to_string())
    }

    /// Compiles an already-parsed specifier, with the caller's limits.
    ///
    /// # Errors
    ///
    /// As [`Locale::formatter_for`], measured against `limits`.
    pub fn formatter_for_with_limits(
        &self,
        spec: &FormatSpecifier,
        limits: FormatLimits,
    ) -> Result<Formatter, FormatError> {
        self.formatter_with_limits(&spec.to_string(), limits)
    }

    /// Compiles a specifier with an SI prefix fixed by `reference`, with the
    /// default limits.
    ///
    /// This is `locale.formatPrefix`. The type is forced to `f`, the SI prefix is
    /// chosen once from `reference` rather than per value, and the scale is the
    /// bit-pinned `Math.pow(10, -e)` from the section 4.2 table.
    ///
    /// A zero, NaN or infinite `reference` has no decimal exponent, so d3's scale
    /// is NaN and its suffix is absent. That is reproduced rather than rejected:
    /// the formatter builds, and renders the locale's NaN text with no SI suffix,
    /// whatever value it is given.
    ///
    /// # Errors
    ///
    /// As [`Locale::formatter`].
    pub fn prefix_formatter(&self, spec: &str, reference: f64) -> Result<Formatter, FormatError> {
        self.prefix_formatter_with_limits(spec, reference, FormatLimits::default())
    }

    /// Compiles a fixed-SI-prefix specifier, with the caller's limits.
    ///
    /// # Errors
    ///
    /// As [`Locale::formatter`], measured against `limits`.
    pub fn prefix_formatter_with_limits(
        &self,
        spec: &str,
        reference: f64,
        limits: FormatLimits,
    ) -> Result<Formatter, FormatError> {
        let specifier: FormatSpecifier = spec.parse()?;
        // `formatPrefix` assigns `specifier.type = "f"` on the parsed object before
        // handing it to `newFormat`, so the compiled formatter really does report
        // type `f`, whatever the caller wrote.
        let specifier = specifier
            .to_builder()
            .format_type(FormatType::Fixed)
            .build()?;
        let suffix = match decimal::si_prefix_exponent(reference) {
            // `prefixes[8 + e / 3]`, which is `undefined` for a NaN exponent, and
            // `newFormat` turns `undefined` back into the empty string.
            Some(exponent) => SI_PREFIXES[(exponent / 3 + 8) as usize],
            None => "",
        };
        let scale = decimal::si_prefix_scale(reference);
        Formatter::compile(self.clone(), specifier, limits, Some(suffix), Some(scale))
    }
}

/// Builds a [`Locale`] one key at a time.
///
/// Starts from d3's defaults, so setting nothing yields the locale
/// `formatLocale({})` produces: `.`, no grouping, no currency affixes, `%`, U+2212,
/// and `NaN`.
///
/// # Examples
///
/// ```
/// use d3_format::Locale;
///
/// let locale = Locale::builder().decimal(",").thousands(".").grouping([3]).build()?;
/// assert_eq!(locale.decimal(), ",");
/// assert_eq!(locale.grouping(), Some(&[3][..]));
///
/// // Grouping needs both keys; with only the sizes it stays off.
/// let ungrouped = Locale::builder().grouping([3]).build()?;
/// assert_eq!(ungrouped.grouping(), None);
/// # Ok::<(), d3_format::LocaleError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocaleBuilder {
    definition: LocaleDefinition,
}

impl LocaleBuilder {
    /// A builder with every key absent.
    pub fn new() -> LocaleBuilder {
        LocaleBuilder::default()
    }

    /// Sets the decimal point.
    pub fn decimal(mut self, value: impl Into<String>) -> Self {
        self.definition.decimal = Some(value.into());
        self
    }

    /// Sets the group separator. Grouping also needs
    /// [`grouping`](LocaleBuilder::grouping).
    pub fn thousands(mut self, value: impl Into<String>) -> Self {
        self.definition.thousands = Some(value.into());
        self
    }

    /// Sets the group sizes. Grouping also needs
    /// [`thousands`](LocaleBuilder::thousands).
    ///
    /// An empty list disables grouping. A zero size is rejected by
    /// [`build`](LocaleBuilder::build).
    pub fn grouping(mut self, value: impl Into<Vec<u32>>) -> Self {
        self.definition.grouping = Some(value.into());
        self
    }

    /// Sets the currency affixes.
    pub fn currency(mut self, prefix: impl Into<String>, suffix: impl Into<String>) -> Self {
        self.definition.currency = Some([prefix.into(), suffix.into()]);
        self
    }

    /// Sets the ten digit replacements.
    ///
    /// The array length is the validation: section 5.3 requires exactly ten
    /// numerals, and `[String; 10]` makes any other count unwritable rather than a
    /// runtime error. d3 accepts a short array and emits `undefined` for the digits
    /// it does not cover.
    pub fn numerals(mut self, value: [String; 10]) -> Self {
        self.definition.numerals = Some(value);
        self
    }

    /// Sets the percent sign.
    pub fn percent(mut self, value: impl Into<String>) -> Self {
        self.definition.percent = Some(value.into());
        self
    }

    /// Sets the minus sign.
    pub fn minus(mut self, value: impl Into<String>) -> Self {
        self.definition.minus = Some(value.into());
        self
    }

    /// Sets the not-a-number text.
    pub fn nan(mut self, value: impl Into<String>) -> Self {
        self.definition.nan = Some(value.into());
        self
    }

    /// Validates the definition and freezes it into a [`Locale`].
    ///
    /// # Errors
    ///
    /// [`LocaleError::ZeroGroupingSize`] if any grouping size is zero.
    pub fn build(self) -> Result<Locale, LocaleError> {
        let LocaleDefinition {
            decimal,
            thousands,
            grouping,
            currency,
            numerals,
            percent,
            minus,
            nan,
        } = self.definition;

        // `locale.grouping === undefined || locale.thousands === undefined ?
        // identity : formatGroup(...)`, plus the two shape rules section 5.3 adds.
        let grouping = match (grouping, thousands) {
            (Some(sizes), Some(thousands)) => {
                for (index, size) in sizes.iter().enumerate() {
                    if *size == 0 {
                        return Err(LocaleError::ZeroGroupingSize { index });
                    }
                }
                if sizes.is_empty() {
                    None
                } else {
                    Some(Grouping { sizes, thousands })
                }
            }
            (Some(sizes), None) => {
                // Still validated: a definition carrying a zero size is malformed
                // whether or not the missing separator happens to hide it.
                for (index, size) in sizes.iter().enumerate() {
                    if *size == 0 {
                        return Err(LocaleError::ZeroGroupingSize { index });
                    }
                }
                None
            }
            (None, _) => None,
        };

        Ok(Locale(Arc::new(LocaleInner {
            decimal: decimal.unwrap_or_else(|| DEFAULT_DECIMAL.to_owned()),
            grouping,
            currency: currency.unwrap_or_default(),
            numerals,
            percent: percent.unwrap_or_else(|| DEFAULT_PERCENT.to_owned()),
            minus: minus.unwrap_or_else(|| DEFAULT_MINUS.to_owned()),
            nan: nan.unwrap_or_else(|| DEFAULT_NAN.to_owned()),
        })))
    }
}

/// A compiled formatter: one locale, one specifier, one set of limits.
///
/// Owns a cheap clone of its locale, so it has no lifetime parameter and can be
/// stored, cloned and sent between threads freely. Immutable: there is no scratch
/// buffer, no result cache, and no SI exponent left over from the last call.
///
/// # Examples
///
/// ```
/// use d3_format::{InputKind, Locale};
///
/// let locale = Locale::en_us();
/// assert_eq!(locale.formatter("d")?.input_kind(), InputKind::Number);
/// assert_eq!(locale.formatter("c")?.input_kind(), InputKind::Text);
/// # Ok::<(), d3_format::FormatError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatter {
    locale: Locale,
    specifier: FormatSpecifier,
    input_kind: InputKind,
    limits: FormatLimits,
    layout: Layout,
}

impl Formatter {
    /// Everything `newFormat` computes before it returns its closure.
    fn compile(
        locale: Locale,
        specifier: FormatSpecifier,
        limits: FormatLimits,
        prefix_suffix: Option<&str>,
        prefix_scale: Option<f64>,
    ) -> Result<Formatter, FormatError> {
        let width = specifier.width().unwrap_or(0);
        if width > limits.max_width_utf16 {
            return Err(FormatError::WidthLimit {
                requested: width,
                maximum: limits.max_width_utf16,
            });
        }

        let mut comma = specifier.comma();
        let mut precision = specifier.precision();
        let mut trim = specifier.trim();
        let resolved_type = match specifier.format_type() {
            // `if (type === "n") comma = true, type = "g";`
            FormatType::Grouped => {
                comma = true;
                FormatType::General
            }
            // `else if (!formatTypes[type]) precision === undefined &&
            // (precision = 12), trim = true, type = "g";`
            other if !other.is_known() => {
                precision = precision.or(Some(12));
                trim = true;
                FormatType::General
            }
            known => known,
        };

        // `if (zero || (fill === "0" && align === "=")) zero = true, fill = "0",
        // align = "=";`
        let (zero, fill, align) = if specifier.zero()
            || (specifier.fill() == '0' && specifier.align() == Align::AfterSign)
        {
            (true, '0', Align::AfterSign)
        } else {
            (false, specifier.fill(), specifier.align())
        };

        let base_prefix = match resolved_type {
            FormatType::Binary => "0b",
            FormatType::Octal => "0o",
            // `"0" + type.toLowerCase()`: uppercase `X` still takes a lowercase
            // `0x`, which section 4.4 lists as a thing not to normalize away.
            FormatType::HexUpper | FormatType::HexLower => "0x",
            _ => "",
        };
        let symbol_prefix = match specifier.symbol() {
            Symbol::Currency => locale.currency_prefix(),
            Symbol::BasePrefix => base_prefix,
            Symbol::None => "",
        };
        let symbol_suffix = match specifier.symbol() {
            Symbol::Currency => locale.currency_suffix(),
            Symbol::None | Symbol::BasePrefix => match resolved_type {
                FormatType::Percent | FormatType::RoundedPercent => locale.percent(),
                _ => "",
            },
        };

        // The affixes are the first thing a formatting call would concatenate, and
        // section 5.4 counts locale affixes against the output budget, so they are
        // measured with checked arithmetic here rather than at every call.
        let prefix = concat_bounded(&[symbol_prefix], &limits)?;
        let suffix = concat_bounded(&[symbol_suffix, prefix_suffix.unwrap_or("")], &limits)?;

        // `precision === undefined ? 6 : /[gprs]/.test(type) ? clamp(1, 21) :
        // clamp(0, 20)`.
        let precision = match precision {
            None => 6,
            Some(requested) => match resolved_type {
                FormatType::General
                | FormatType::RoundedPercent
                | FormatType::Rounded
                | FormatType::SiPrefix => requested.clamp(1, 21),
                _ => requested.min(20),
            },
        };

        let sign = specifier.sign();
        let maybe_suffix = matches!(
            resolved_type,
            FormatType::Decimal
                | FormatType::Exponent
                | FormatType::Fixed
                | FormatType::General
                | FormatType::RoundedPercent
                | FormatType::Rounded
                | FormatType::SiPrefix
                | FormatType::Percent
        );

        Ok(Formatter {
            input_kind: resolved_type.input_kind(),
            locale,
            specifier,
            limits,
            layout: Layout {
                fill,
                align,
                sign,
                zero,
                width,
                comma,
                precision,
                trim,
                resolved_type,
                maybe_suffix,
                prefix,
                suffix,
                prefix_scale: prefix_scale.map(f64::to_bits),
            },
        })
    }

    /// The kind of input this formatter takes.
    pub fn input_kind(&self) -> InputKind {
        self.input_kind
    }

    /// The specifier this formatter was compiled from.
    ///
    /// This is the parsed specifier, *before* `n` becomes `,g` and an unknown type
    /// becomes `.12~g`, because that is what d3's `format.toString()` returns: the
    /// rewrites in `newFormat` are made to local variables, not to the object.
    ///
    /// ```
    /// use d3_format::Locale;
    ///
    /// assert_eq!(Locale::en_us().formatter("n")?.specifier().to_string(), " >-n");
    /// # Ok::<(), d3_format::FormatError>(())
    /// ```
    pub fn specifier(&self) -> &FormatSpecifier {
        &self.specifier
    }

    /// The limits in force for this formatter.
    pub fn limits(&self) -> FormatLimits {
        self.limits
    }

    /// Formats a number.
    ///
    /// Total over `f64`: every bit pattern, including both zeros, both infinities
    /// and every NaN payload, formats or returns an error. It never panics.
    ///
    /// ```
    /// use d3_format::Locale;
    ///
    /// let locale = Locale::en_us();
    /// assert_eq!(locale.formatter("$,.2f")?.format_number(-1234.5)?, "\u{2212}$1,234.50");
    /// assert_eq!(locale.formatter(".3s")?.format_number(0.00042)?, "420\u{b5}");
    /// # Ok::<(), d3_format::FormatError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`FormatError::InputTypeMismatch`] if this is a type `c` formatter, which
    /// takes text. d3 would coerce; see `no-input-coercion` in
    /// `DIVERGENCES.md`.
    ///
    /// [`FormatError::OutputLimit`] or [`FormatError::AllocationFailed`] if the
    /// result would exceed [`FormatLimits::max_output_bytes`] or the allocator
    /// refuses the buffer. A wide pad with a multi-byte fill is the usual way to
    /// reach either.
    pub fn format_number(&self, value: f64) -> Result<String, FormatError> {
        let mut out = String::new();
        self.format_number_into(&mut out, value)?;
        Ok(out)
    }

    /// Formats a number into `out`, which is cleared first and left empty on error.
    ///
    /// Section 5.3: the buffer's capacity is reused, and no claim is made that a
    /// warm buffer makes the call allocation-free -- grouping and numerals both
    /// build their own.
    ///
    /// # Errors
    ///
    /// As [`Formatter::format_number`].
    pub fn format_number_into(&self, out: &mut String, value: f64) -> Result<(), FormatError> {
        out.clear();
        self.accept(InputKind::Number)?;
        self.layout
            .number(&self.locale, &self.limits, out, value)
            .inspect_err(|_| out.clear())
    }

    /// Formats text, for a type `c` formatter.
    ///
    /// The text is preserved byte for byte, and only the prefix, the suffix, the
    /// width and alignment, and the final numeral substitution touch the result.
    ///
    /// The width is a UTF-16 code-unit count, as everywhere in d3, so the
    /// astral-plane character below is two units wide and takes six spaces to reach
    /// eight, not seven.
    ///
    /// ```
    /// use d3_format::Locale;
    ///
    /// assert_eq!(Locale::en_us().formatter(">8c")?.format_text("\u{1f600}")?, "      \u{1f600}");
    /// # Ok::<(), d3_format::FormatError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// [`FormatError::InputTypeMismatch`] unless this is a type `c` formatter, plus
    /// the limit errors [`Formatter::format_number`] documents.
    pub fn format_text(&self, value: &str) -> Result<String, FormatError> {
        let mut out = String::new();
        self.format_text_into(&mut out, value)?;
        Ok(out)
    }

    /// Formats text into `out`, which is cleared first and left empty on error.
    ///
    /// # Errors
    ///
    /// As [`Formatter::format_text`].
    pub fn format_text_into(&self, out: &mut String, value: &str) -> Result<(), FormatError> {
        out.clear();
        self.accept(InputKind::Text)?;
        self.layout
            .text(&self.locale, &self.limits, out, value)
            .inspect_err(|_| out.clear())
    }

    /// Rejects the input kind this formatter does not take.
    fn accept(&self, actual: InputKind) -> Result<(), FormatError> {
        if self.input_kind == actual {
            return Ok(());
        }
        Err(FormatError::InputTypeMismatch {
            expected: self.input_kind,
            actual,
        })
    }
}

/// Concatenates `parts` under the output-byte limit, with checked arithmetic.
///
/// Section 5.4: every dynamic output buffer uses checked length arithmetic and
/// `try_reserve`, and locale affixes count against the budget.
fn concat_bounded(parts: &[&str], limits: &FormatLimits) -> Result<String, FormatError> {
    let mut total: usize = 0;
    for part in parts {
        total = total
            .checked_add(part.len())
            .ok_or(FormatError::AllocationFailed)?;
    }
    if total > limits.max_output_bytes {
        return Err(FormatError::OutputLimit {
            requested: total,
            maximum: limits.max_output_bytes,
        });
    }
    let mut text = String::new();
    text.try_reserve_exact(total)
        .map_err(|_| FormatError::AllocationFailed)?;
    for part in parts {
        text.push_str(part);
    }
    Ok(text)
}

/// A generated locale as a [`LocaleDefinition`].
#[cfg(feature = "locales")]
fn definition_of(generated: &crate::generated_locales::GeneratedLocale) -> LocaleDefinition {
    LocaleDefinition {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn en_us_formatter(spec: &str) -> Formatter {
        Locale::en_us()
            .formatter(spec)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    #[test]
    fn the_default_locale_is_d3s() {
        let locale = Locale::builder().build().expect("valid");
        assert_eq!(locale.decimal(), ".");
        assert_eq!(locale.thousands(), "");
        assert_eq!(locale.grouping(), None);
        assert_eq!(locale.currency_prefix(), "");
        assert_eq!(locale.currency_suffix(), "");
        assert_eq!(locale.numerals(), None);
        assert_eq!(locale.percent(), "%");
        assert_eq!(locale.minus(), "\u{2212}");
        assert_eq!(locale.nan(), "NaN");
    }

    #[test]
    fn en_us_adds_grouping_a_separator_and_a_currency_prefix() {
        let locale = Locale::en_us();
        assert_eq!(locale.decimal(), ".");
        assert_eq!(locale.thousands(), ",");
        assert_eq!(locale.grouping(), Some(&[3][..]));
        assert_eq!(locale.currency_prefix(), "$");
        assert_eq!(locale.currency_suffix(), "");
        assert_eq!(locale.minus(), "\u{2212}");
    }

    #[test]
    fn grouping_needs_both_keys_and_a_non_empty_list() {
        let sizes_only = Locale::builder().grouping([3]).build().expect("valid");
        assert_eq!(sizes_only.grouping(), None);

        let separator_only = Locale::builder().thousands(",").build().expect("valid");
        assert_eq!(separator_only.grouping(), None);
        assert_eq!(separator_only.thousands(), "");

        let empty = Locale::builder()
            .grouping(Vec::new())
            .thousands(",")
            .build()
            .expect("valid");
        assert_eq!(empty.grouping(), None);

        let both = Locale::builder()
            .grouping([2, 3])
            .thousands(",")
            .build()
            .expect("valid");
        assert_eq!(both.grouping(), Some(&[2, 3][..]));
        assert_eq!(both.thousands(), ",");
    }

    #[test]
    fn a_zero_group_size_is_refused_wherever_it_appears() {
        let error = Locale::builder()
            .grouping([3, 0])
            .thousands(",")
            .build()
            .unwrap_err();
        assert_eq!(error, LocaleError::ZeroGroupingSize { index: 1 });
        assert!(error.to_string().contains("index 1"));

        // Also without a separator: the definition is malformed either way.
        assert!(Locale::builder().grouping([0]).build().is_err());
    }

    #[test]
    fn the_type_rewrites_match_new_format() {
        // `n` becomes `,g` but still reports itself as `n`.
        let grouped = en_us_formatter("n");
        assert_eq!(grouped.specifier().to_string(), " >-n");
        assert_eq!(grouped.layout.resolved_type, FormatType::General);
        assert!(grouped.layout.comma);
        assert!(!grouped.layout.trim);
        assert_eq!(grouped.layout.precision, 6);

        // An unknown letter and the empty type both become `.12~g`.
        for spec in ["q", ""] {
            let fallback = en_us_formatter(spec);
            assert_eq!(
                fallback.layout.resolved_type,
                FormatType::General,
                "{spec:?}"
            );
            assert!(fallback.layout.trim, "{spec:?}");
            assert_eq!(fallback.layout.precision, 12, "{spec:?}");
        }

        // An explicit precision survives the fallback.
        let explicit = en_us_formatter(".3q");
        assert_eq!(explicit.layout.precision, 3);
        assert!(explicit.layout.trim);
    }

    #[test]
    fn zero_fill_is_folded_in_from_either_spelling() {
        let flag = en_us_formatter("08d");
        assert!(flag.layout.zero);
        assert_eq!(flag.layout.fill, '0');
        assert_eq!(flag.layout.align, Align::AfterSign);

        let spelled_out = en_us_formatter("0=12");
        assert!(spelled_out.layout.zero);
        assert_eq!(spelled_out.layout.fill, '0');
        assert_eq!(spelled_out.layout.align, Align::AfterSign);

        // Fill `0` with any other alignment is not zero fill.
        let left = en_us_formatter("0<12d");
        assert!(!left.layout.zero);
        assert_eq!(left.layout.fill, '0');
        assert_eq!(left.layout.align, Align::Left);
    }

    #[test]
    fn the_affixes_come_from_the_symbol_and_the_locale() {
        assert_eq!(en_us_formatter("$.2f").layout.prefix, "$");
        assert_eq!(en_us_formatter("$.2f").layout.suffix, "");
        assert_eq!(en_us_formatter("#x").layout.prefix, "0x");
        // Lowercase `0x` even for uppercase `X`.
        assert_eq!(en_us_formatter("#X").layout.prefix, "0x");
        assert_eq!(en_us_formatter("#b").layout.prefix, "0b");
        assert_eq!(en_us_formatter("#o").layout.prefix, "0o");
        // `#` means nothing to a type that has no base prefix.
        assert_eq!(en_us_formatter("#f").layout.prefix, "");
        assert_eq!(en_us_formatter(".0%").layout.suffix, "%");
        assert_eq!(en_us_formatter(".2p").layout.suffix, "%");

        let euro = Locale::builder()
            .decimal(",")
            .currency("", " \u{20ac}")
            .build()
            .expect("valid");
        let formatter = euro.formatter("$.3s").expect("valid");
        assert_eq!(formatter.layout.prefix, "");
        assert_eq!(formatter.layout.suffix, " \u{20ac}");
    }

    #[test]
    fn precision_is_defaulted_then_clamped_to_its_types_range() {
        assert_eq!(en_us_formatter("f").layout.precision, 6);
        assert_eq!(en_us_formatter(".30f").layout.precision, 20);
        assert_eq!(en_us_formatter(".0f").layout.precision, 0);
        assert_eq!(en_us_formatter(".0g").layout.precision, 1);
        assert_eq!(en_us_formatter(".30g").layout.precision, 21);
        assert_eq!(en_us_formatter(".0s").layout.precision, 1);
        assert_eq!(en_us_formatter(".0r").layout.precision, 1);
        assert_eq!(en_us_formatter(".0p").layout.precision, 1);
        // A saturated precision clamps to the same place a huge JavaScript number
        // does, so the specifier d3 renders is the specifier this compiles.
        assert_eq!(
            en_us_formatter(".99999999999999999999f").layout.precision,
            20
        );
    }

    #[test]
    fn a_prefix_formatter_forces_type_f_and_pins_its_scale() {
        let locale = Locale::en_us();
        let formatter = locale.prefix_formatter(",.0s", 1e6).expect("valid");
        assert_eq!(formatter.specifier().format_type(), FormatType::Fixed);
        assert_eq!(formatter.layout.suffix, "M");
        assert_eq!(
            formatter.layout.prefix_scale.map(f64::from_bits),
            Some(1e-6)
        );

        // A reference with no decimal exponent: NaN scale, no suffix, still builds.
        for reference in [0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let degenerate = locale.prefix_formatter("s", reference).expect("valid");
            assert_eq!(degenerate.layout.suffix, "");
            assert!(degenerate
                .layout
                .prefix_scale
                .map(f64::from_bits)
                .is_some_and(f64::is_nan));
        }

        // A negative reference selects from its magnitude.
        let negative = locale.prefix_formatter("s", -1e6).expect("valid");
        assert_eq!(negative.layout.suffix, "M");
    }

    #[test]
    fn the_width_limit_is_enforced_at_construction() {
        let locale = Locale::en_us();
        let error = locale.formatter("1000001f").unwrap_err();
        assert_eq!(
            error,
            FormatError::WidthLimit {
                requested: 1_000_001,
                maximum: 1_000_000
            }
        );

        // The caller can raise it.
        let raised = FormatLimits {
            max_width_utf16: 2_000_000,
            ..FormatLimits::DEFAULT
        };
        assert!(locale.formatter_with_limits("1000001f", raised).is_ok());
    }

    #[test]
    fn only_type_c_takes_text() {
        assert_eq!(en_us_formatter("d").input_kind(), InputKind::Number);
        assert_eq!(en_us_formatter("c").input_kind(), InputKind::Text);
        assert_eq!(en_us_formatter("020c").input_kind(), InputKind::Text);
        // A prefix formatter is always numeric, because the type is forced to `f`.
        let prefixed = Locale::en_us().prefix_formatter("c", 1.0).expect("valid");
        assert_eq!(prefixed.input_kind(), InputKind::Number);
    }

    #[test]
    fn concatenation_respects_the_output_budget() {
        let tight = FormatLimits {
            max_output_bytes: 2,
            ..FormatLimits::DEFAULT
        };
        assert_eq!(concat_bounded(&["ab"], &tight), Ok("ab".to_owned()));
        assert_eq!(
            concat_bounded(&["ab", "c"], &tight),
            Err(FormatError::OutputLimit {
                requested: 3,
                maximum: 2
            })
        );
    }

    #[test]
    fn a_locale_affix_over_the_budget_fails_at_construction() {
        let locale = Locale::builder()
            .currency("$$$$$$$$", "")
            .build()
            .expect("valid");
        let tight = FormatLimits {
            max_output_bytes: 4,
            ..FormatLimits::DEFAULT
        };
        assert_eq!(
            locale.formatter_with_limits("$d", tight),
            Err(FormatError::OutputLimit {
                requested: 8,
                maximum: 4
            })
        );
    }
}
