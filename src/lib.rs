//! A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.
//!
//! d3's format mini-language, its 59 locales and its ECMAScript number conversion,
//! reproduced byte for byte in a `std` Rust crate with no runtime dependencies.
//!
//! ```
//! use d3_format::Locale;
//!
//! let locale = Locale::en_us();
//! assert_eq!(locale.formatter("$,.2f")?.format_number(1234.5)?, "$1,234.50");
//! assert_eq!(locale.formatter(".1%")?.format_number(0.123)?, "12.3%");
//! assert_eq!(locale.formatter(".3s")?.format_number(1.3e6)?, "1.30M");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # What agreement means here
//!
//! Byte-for-byte with d3-format 3.1.2 under Node, for every specifier the grammar
//! accepts, every locale the model accepts and every `f64` bit pattern. The port is
//! checked against a recorded oracle rather than against a reimplementation of the
//! specification: the tools in the repository run d3 itself and compare its answers
//! with this crate's. `COMPATIBILITY.md` states what is claimed and on what
//! evidence.
//!
//! The deliberate exceptions are enumerated in `DIVERGENCES.md`, and they are about
//! the *interface*, not the digits: no mutable process-global default locale, no
//! coercion of a non-number to a number, typed errors where d3 throws or silently
//! produces nonsense, and explicit limits on how much output a single call may
//! allocate.
//!
//! # A formatter is a locale plus a specifier
//!
//! d3 curries: `format(specifier)` closes over the default locale and returns a
//! function. Here the locale is an ordinary value and [`Locale::formatter`] is the
//! same step, so the compiled [`Formatter`] can be stored, cloned and shared
//! without a lifetime parameter.
//!
//! ```
//! use d3_format::Locale;
//!
//! let euro = Locale::builder()
//!     .decimal(",")
//!     .thousands(".")
//!     .grouping([3])
//!     .currency("", "\u{a0}\u{20ac}")
//!     .build()?;
//!
//! assert_eq!(euro.formatter("$,.2f")?.format_number(1234.5)?, "1.234,50\u{a0}\u{20ac}");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Under the `locales` feature, [`Locale::named`] hands back any of the 59 built-in
//! definitions generated from d3's own locale JSON. Numeral substitution runs last,
//! over the fully assembled string, which is observable rather than an internal
//! detail: the `0` of the `0x` base prefix is substituted while the hex digits are
//! not.
//!
//! ```
//! # #[cfg(feature = "locales")] {
//! use d3_format::Locale;
//!
//! let arabic = Locale::named("ar-001").expect("bundled");
//! assert_eq!(arabic.formatter(",d").unwrap().format_number(1234567.0).unwrap(), "١٬٢٣٤٬٥٦٧");
//! assert_eq!(arabic.formatter("#x").unwrap().format_number(48879.0).unwrap(), "٠xbeef");
//! # }
//! ```
//!
//! # The specifier is a value
//!
//! [`FormatSpecifier`] parses d3's grammar into an immutable, validated value.
//! `Display` emits the canonical spelling, and `Locale::formatter_for` is exactly
//! `Locale::formatter(&specifier.to_string())`, so the round trip cannot skip
//! normalization.
//!
//! ```
//! use d3_format::{FormatSpecifier, FormatType, Locale, Symbol};
//!
//! let specifier: FormatSpecifier = "$,.2f".parse()?;
//! assert_eq!(specifier.symbol(), Symbol::Currency);
//! assert_eq!(specifier.format_type(), FormatType::Fixed);
//! assert_eq!(specifier.precision(), Some(2));
//! assert_eq!(specifier.to_string(), " >-$,.2f");
//!
//! // Built rather than parsed, and changed by building a new one.
//! let wider = specifier.to_builder().precision(4u32).build()?;
//! assert_eq!(Locale::en_us().formatter_for(&wider)?.format_number(1234.5)?, "$1,234.5000");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Precision suggestions
//!
//! [`precision_fixed`], [`precision_round`] and [`precision_prefix`] answer in
//! `f64` because d3 does: the answer is NaN for a zero or non-finite step, and the
//! oracle asserts it. [`as_precision`] is the step from there to the `u32` a
//! specifier takes, and it returns `None` rather than substituting a zero.
//!
//! ```
//! use d3_format::{as_precision, precision_prefix, Locale};
//!
//! let step = 1e-6;
//! let value = 1.3e-3;
//! let digits = as_precision(precision_prefix(step, value)).expect("finite step");
//! assert_eq!(digits, 3);
//!
//! let specifier = format!(".{digits}s");
//! assert_eq!(Locale::en_us().formatter(&specifier)?.format_number(value)?, "1.30m");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Formatting is fallible
//!
//! Every entry point returns a `Result`. d3 coerces anything to a number and lets
//! the engine decide what a trillion-character pad does; this crate refuses both at
//! a typed, recoverable point. A numeric formatter declines text, a type `c`
//! formatter declines numbers, and a width or an output size beyond
//! [`FormatLimits`] is an error rather than an allocation.
//!
//! ```
//! use d3_format::{FormatError, FormatLimits, InputKind, Locale};
//!
//! let locale = Locale::en_us();
//!
//! // Not in the grammar. The message is d3's, exactly.
//! assert_eq!(locale.formatter("foo").unwrap_err().to_string(), "invalid format: foo");
//!
//! // The wrong kind of input, where d3 would coerce.
//! assert_eq!(
//!     locale.formatter("d")?.format_text("nope"),
//!     Err(FormatError::InputTypeMismatch {
//!         expected: InputKind::Number,
//!         actual: InputKind::Text,
//!     }),
//! );
//!
//! // Rejected at construction, so a formatter that exists cannot fail to pad.
//! let limits = FormatLimits { max_width_utf16: 64, ..FormatLimits::default() };
//! assert_eq!(
//!     locale.formatter_with_limits("1000d", limits),
//!     Err(FormatError::WidthLimit { requested: 1000, maximum: 64 }),
//! );
//!
//! // Raising the limit is the caller's decision; formatting stays fallible.
//! let generous = FormatLimits { max_width_utf16: 4096, ..FormatLimits::default() };
//! assert_eq!(locale.formatter_with_limits("1000d", generous)?.format_number(1.0)?.len(), 1000);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! [`Formatter::format_number_into`] and [`Formatter::format_text_into`] write into
//! a caller-owned `String`, reusing its capacity. They are not allocation-free:
//! exact decimal conversion, grouping and numerals all build their own buffers.
//!
//! # Threads
//!
//! [`Locale`], [`FormatSpecifier`] and [`Formatter`] are `Send + Sync`. There is no
//! mutable process-global default locale, no shared scratch buffer, no result
//! cache, and no leftover SI exponent between calls -- d3 has the last of those as a
//! module-level variable, and reproducing it would have been a data race.
//!
//! # Features
//!
//! | Feature | Default | Effect |
//! |---|---|---|
//! | `locales` | no | [`Locale::named`] and the 59 built-in definitions in [`locales`] |
//! | `serde` | no | `Serialize`/`Deserialize` for [`LocaleDefinition`], the stable wire shape |
//!
//! Features are additive and none of them changes a formatting result. The default
//! build has no dependencies at all.
//!
//! # Compatibility
//!
//! Minimum supported Rust version 1.80.0, validated on pinned 1.98.1 and on current
//! stable. `std` is required and `no_std` is not supported. The crate compiles for
//! `wasm32-unknown-unknown`, but no JavaScript boundary is published here.
//!
//! Version `0.1.0-rc.1` is the Rust API's own version and does not track d3's. The
//! *outputs* target d3-format 3.1.2 at commit `ebdc2d5`; `COMPATIBILITY.md` pins
//! the exact oracle those outputs were compared against.

#![forbid(unsafe_code)]

// The README is the crates.io front page, and section 7's Phase 6 requires its API
// examples to be compiled doctests rather than prose snippets. Attaching it to a
// `#[cfg(doctest)]` item is what makes `cargo test --doc` run them: rustdoc collects
// the code blocks out of the attached documentation, while the item itself exists in
// no other build, so nothing is added to the public surface and nothing above is
// duplicated into the rendered crate documentation.
#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeExamplesAreCompiled;

mod bignat;

// A `decimal` module is not part of the crate's public surface: these are
// ECMAScript conversion primitives, not d3's interface. It is `pub` only because
// `tests/decimal.rs` is an integration test, and an integration test links the
// library as an external crate, so it can no more reach a private item than any
// other dependent can. `#[doc(hidden)]` is the mechanism because it keeps the
// module out of the documentation and out of the compatibility contract while
// leaving a later release free to promote it; the module's own header states the
// terms.
#[doc(hidden)]
pub mod decimal;

mod layout;
mod limits;
mod locale;
mod precision;
mod specifier;
mod types;

pub use crate::limits::{FormatError, FormatLimits};
pub use crate::locale::{Formatter, Locale, LocaleBuilder, LocaleDefinition, LocaleError};
pub use crate::precision::{as_precision, precision_fixed, precision_prefix, precision_round};
pub use crate::specifier::{FormatSpecifier, FormatSpecifierBuilder, ParseError, ParseErrorKind};
pub use crate::types::{Align, FormatType, InputKind, Sign, Symbol};

// The generated locale table is data only: it has no dependency on the formatting
// API. It is compiled unconditionally so that the section 3.7 drift gate --
// `cargo test --no-run` with default features -- really builds it,
// and it is only reachable from outside the crate under the `locales` feature, as
// section 5.1's feature policy requires.
// The generated file carries its own `#![allow(dead_code)]`, so repeating it here
// is what clippy::duplicated_attributes flags.
#[path = "locales/generated.rs"]
mod generated_locales;

/// Built-in locale definitions generated from the d3-format locale JSON.
///
/// The module behind this one stays private, so the feature is the only public
/// door to the table. That has to be a glob re-export of the module's items:
/// `pub use crate::generated_locales as locales` re-exports the *module*, which
/// rustc rejects with E0365 because a private module cannot be made public that
/// way.
#[cfg(feature = "locales")]
pub mod locales {
    pub use crate::generated_locales::*;
}
