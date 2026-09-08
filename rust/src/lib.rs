//! A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.
//!
//! This crate is being ported incrementally; see `MIGRATION_TO_RUST.md` in the
//! repository root for the implementation plan and the compatibility contract.
//! Phase 0 establishes the toolchain, the crate skeleton, and the standard
//! library semantics gate in `tests/gate.rs`. Phase 2 adds the exact number
//! conversion engine of section 4.2. Phase 3 adds section 5's public surface: the
//! specifier grammar, the locale model, the precision suggestions, and the errors
//! and limits they report through. Phase 4 -- this one -- adds value layout, so the
//! crate now produces d3's output.
//!
//! # What agreement means here
//!
//! Byte-for-byte with `d3-format` 3.1.2 under Node, for every specifier the grammar
//! accepts, every locale the model accepts and every `f64` bit pattern. The port is
//! checked against a recorded oracle rather than against a reimplementation of the
//! spec: `rust/tools/` records d3's own answers and compares them.
//!
//! The deliberate exceptions are enumerated in `rust/DIVERGENCES.md`, and they are
//! about the *interface*, not the digits: no mutable process-global default locale,
//! no coercion of a non-number to a number, typed errors where d3 throws or
//! silently produces nonsense, and explicit limits on how much output a single call
//! may allocate.
//!
//! # Examples
//!
//! ```
//! use d3_format::{FormatSpecifier, FormatType, Locale, Symbol};
//!
//! // The grammar, parsed into an immutable value.
//! let specifier: FormatSpecifier = "$,.2f".parse()?;
//! assert_eq!(specifier.symbol(), Symbol::Currency);
//! assert_eq!(specifier.format_type(), FormatType::Fixed);
//! assert_eq!(specifier.to_string(), " >-$,.2f");
//!
//! // Compiled against a locale, which owns no global state.
//! let formatter = Locale::en_us().formatter_for(&specifier)?;
//! assert_eq!(formatter.format_number(1234.5)?, "$1,234.50");
//!
//! // The minus sign is the locale's, and it is U+2212, not a hyphen.
//! assert_eq!(formatter.format_number(-1234.5)?, "\u{2212}$1,234.50");
//!
//! // SI prefixes, and the grouping the `n` type turns on.
//! assert_eq!(Locale::en_us().formatter(".3s")?.format_number(1.3e6)?, "1.30M");
//! assert_eq!(Locale::en_us().formatter(",d")?.format_number(1234567.0)?, "1,234,567");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]

mod bignat;

// Section 5 declares the crate's public surface, and a `decimal` module is not in
// it: these are ECMAScript conversion primitives, not d3's interface. It is `pub`
// only because section 3.6's differential harness -- `src/bin/dump_cases.rs` -- is a
// separate binary target, and a binary target links the library as an external
// crate, so it can no more reach a private item than any other dependent can. The
// same is true of `tests/decimal.rs`. `#[doc(hidden)]` is the mechanism because it
// keeps the module out of the documentation and out of the compatibility contract
// while leaving Phase 6 free to promote it; the module's own header states the
// terms. See the note on `decimal` in the module documentation.
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
