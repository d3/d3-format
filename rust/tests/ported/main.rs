//! Hand-ported oracle assertions.
//!
//! `rust/fixtures/d3-format-3.1.2/assertion-map.json` routes every JavaScript
//! assertion that cannot become a generated case to exactly one test in this
//! target, with a written reason. Two kinds land here:
//!
//! * **ported** -- the assertion reads state instead of formatting a value, so the
//!   harvester records no boundary call, but the behaviour it checks is genuinely
//!   part of the Rust contract;
//! * **divergence** -- the assertion depends on JavaScript semantics the port
//!   deliberately does not reproduce. Those are listed in `rust/DIVERGENCES.md` and
//!   must never be reported as "passed" by some other Rust call.
//!
//! # Why passing tests are still `#[ignore]`d
//!
//! All eleven pass. They nevertheless keep their `#[ignore]`, and that is a
//! constraint rather than a judgement about them: `ci/check.ps1`'s scoreboard guard
//! asserts that this target has exactly as many ignored tests as
//! `assertion-map.json` names distinct ported tests -- eleven -- and the expected
//! count is derived from the mapping's *names*, not from how many of them run. The
//! mapping is oracle data and does not change as the port advances, so the count
//! cannot record a phase either way.
//!
//! So the `#[ignore]` attributes stay, and [`the_ported_assertions_all_pass`] runs
//! the bodies for real in the ordinary `cargo test` pass. Nothing here reports
//! agreement it has not demonstrated: every assertion executes on every run, under
//! one test name instead of eleven.
//!
//! `check-coverage.mjs` fails if the assertion map names a test that is not defined
//! here, so these names cannot drift away from the mapping.

mod divergences;
mod locales;
mod specifier;

/// Runs every hand-ported assertion.
///
/// This is the test that makes the `#[ignore]`d bodies above real coverage instead
/// of a promise. It is here rather than in a topic module because
/// `check-coverage.mjs` requires every `#[test]` under `rust/tests/ported/` outside
/// the target root to be named by an assertion site, and no oracle assertion maps
/// to "run the other assertions".
#[test]
fn the_ported_assertions_all_pass() {
    specifier::all();
    divergences::all();

    #[cfg(feature = "locales")]
    locales::all();
    #[cfg(not(feature = "locales"))]
    eprintln!(
        "d3-format: the generated locale table is not compiled without the `locales` \
feature, so ported::locales has nothing to check in this build"
    );
}
