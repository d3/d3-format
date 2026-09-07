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
//! Phase 3 implements ten of these eleven assertions; the eleventh
//! ([`divergences::numeric_formatters_reject_non_numeric_input`]) asserts the half
//! of its contract Phase 3 owns and then diverges, because the rest is Phase 4
//! layout.
//!
//! They nevertheless all keep their `#[ignore]`, and that is a constraint rather
//! than a judgement about them. `ci/check.ps1`'s scoreboard guard asserts that this
//! target has exactly as many ignored tests as `assertion-map.json` names distinct
//! ported tests -- eleven -- and Phase 3's exit gate requires both that the guard
//! pass unmodified and that `assertion-map.json` stay untouched. Un-`#[ignore]`ing
//! anything here would fail the guard with no committed artifact able to record the
//! new state, because the expected count is derived from the mapping's *names*, not
//! from how many of them pass. That is a Phase 1 assumption -- "a ported test that
//! runs is a ported test that lies" -- which stops being true the moment the API
//! it needs exists.
//!
//! So the `#[ignore]` attributes stay, and [`the_phase_3_assertions_all_pass`] runs
//! the bodies for real in the ordinary `cargo test` pass. Nothing here reports
//! agreement it has not demonstrated: the assertions execute, and the one that
//! cannot yet pass is asserted to still diverge.
//!
//! `check-coverage.mjs` fails if the assertion map names a test that is not defined
//! here, so these names cannot drift away from the mapping.

mod divergences;
mod locales;
mod specifier;

/// The body every not-yet-portable assertion in this target shares.
///
/// Routing all of them through one function writes the placeholder convention down
/// once and gives [`the_phase_4_assertion_still_diverges`] something real to
/// assert. `context` names the phase and the API that is still missing;
/// `expectation` states what the assertion will check once it arrives.
pub(crate) fn placeholder(context: &str, expectation: &str) -> ! {
    unimplemented!("{context}: expected {expectation}");
}

/// Runs every hand-ported assertion Phase 3 completes.
///
/// This is the test that makes the `#[ignore]`d bodies above real coverage instead
/// of a promise. It is here rather than in a topic module because
/// `check-coverage.mjs` requires every `#[test]` under `rust/tests/ported/` outside
/// the target root to be named by an assertion site, and no oracle assertion maps
/// to "run the other assertions".
#[test]
fn the_phase_3_assertions_all_pass() {
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

/// Proves the one remaining hand-ported assertion has not quietly started passing.
///
/// [`divergences::numeric_formatters_reject_non_numeric_input`] needs the NaN
/// rendering Phase 4 brings. Until then it must diverge, exactly as the generated
/// suite's seam does. Implementing the layout makes this test fail, which is the
/// intended prompt to finish that assertion and delete this one.
#[test]
fn the_phase_4_assertion_still_diverges() {
    // The placeholder writes to stderr on the way past; the panic is the payload.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome =
        std::panic::catch_unwind(divergences::numeric_formatters_reject_non_numeric_input);
    std::panic::set_hook(previous);

    assert!(
        outcome.is_err(),
        "numeric_formatters_reject_non_numeric_input returned, so Phase 4 has landed \
and both this test and that assertion's placeholder are stale"
    );
}
