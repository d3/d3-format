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
//! The bodies arrive with the APIs they exercise, in Phase 3 and Phase 4. Until
//! then every test is `#[ignore]`d *and* `unimplemented!()`, so neither `cargo test`
//! nor `cargo test -- --ignored` can mistake this target for working coverage.
//!
//! `check-coverage.mjs` fails if the assertion map names a test that is not defined
//! here, so these names cannot drift away from the mapping.

mod divergences;
mod locales;
mod specifier;

/// The body every not-yet-portable assertion in this target shares.
///
/// Routing all of them through one function writes the placeholder convention down
/// once and gives `no_ported_assertion_can_pass_before_its_api_exists` something
/// real to assert. `context` names the phase and the API that is still missing;
/// `expectation` states what the assertion will check once it arrives.
pub(crate) fn placeholder(context: &str, expectation: &str) -> ! {
    unimplemented!("{context}: expected {expectation}");
}

/// Proves the hand-ported assertions in this target cannot pass yet.
///
/// libtest reports `test result: ok.` for a target whose tests are all ignored, and
/// a summary that reads only that line would take this placeholder target for
/// working coverage. Stating the situation in a test name puts it somewhere no
/// summarization can drop it, and asserting the shared placeholder really does
/// diverge keeps the name honest: deleting `placeholder` in Phase 3 fails this test,
/// which is the intended prompt to un-`#[ignore]` the assertions in the same change.
#[test]
fn no_ported_assertion_can_pass_before_its_api_exists() {
    let outcome = std::panic::catch_unwind(|| {
        placeholder("Phase 1 self-check", "the shared placeholder to diverge")
    });
    assert!(
        outcome.is_err(),
        "the shared placeholder returned a value, so the #[ignore] attributes in this target are stale"
    );
}
