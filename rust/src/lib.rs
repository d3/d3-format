//! A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.
//!
//! This crate is being ported incrementally; see `MIGRATION_TO_RUST.md` in the
//! repository root for the implementation plan and the compatibility contract.
//! Phase 0 establishes the toolchain, the crate skeleton, and the standard
//! library semantics gate in `tests/gate.rs`. No formatting API is exposed yet.

#![forbid(unsafe_code)]

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
