//! A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.
//!
//! This crate is being ported incrementally; see `MIGRATION_TO_RUST.md` in the
//! repository root for the implementation plan and the compatibility contract.
//! Phase 0 establishes the toolchain, the crate skeleton, and the standard
//! library semantics gate in `tests/gate.rs`. Phase 2 adds the exact number
//! conversion engine of section 4.2. The specifier parser, `Locale` and
//! `Formatter` arrive in Phases 3 and 4, so there is still no d3-shaped
//! formatting API.

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

mod limits;

pub use crate::limits::{FormatError, FormatLimits};

// The generated locale table is data only: it has no dependency on the formatting
// API that arrives in Phase 3. It is compiled unconditionally so that the Phase 1
// drift gate -- `cargo test --no-run` with default features -- really builds it,
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
