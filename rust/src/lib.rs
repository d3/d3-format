//! A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.
//!
//! This crate is being ported incrementally; see `MIGRATION_TO_RUST.md` in the
//! repository root for the implementation plan and the compatibility contract.
//! Phase 0 establishes the toolchain, the crate skeleton, and the standard
//! library semantics gate in `tests/gate.rs`. No formatting API is exposed yet.

#![forbid(unsafe_code)]
