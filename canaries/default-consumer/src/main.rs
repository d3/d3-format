//! Downstream canary: the packaged crate, default features, plus the multithreaded
//! formatter workload from section 7's Phase 6 canary list.
//!
//! Everything here is written the way a downstream application would write it. It
//! reaches only for the public API, it does not use `dev-dependencies`, and it
//! resolves `d3-format` through a path to the unpacked `.crate` rather than to
//! `rust/`, so a file that failed to make it into the tarball is a build error here
//! rather than an invisible pass.
//!
//! Any failure panics. A canary that reported its own result would need a harness,
//! and the exit code is the whole protocol run-canaries.ps1 needs.

use std::thread;
use std::time::Instant;

use d3_format::{FormatError, FormatLimits, InputKind, Locale};

/// The whole SI prefix table, one value per bucket, with the oracle's answer for
/// `.3s`.
///
/// d3 keeps the exponent its SI formatter chose in a module-level variable
/// (`prefixExponent` in `src/formatPrefixAuto.js`), which is why the multithreaded
/// workload below spreads *these* values across threads specifically: if the port
/// had reproduced that global, threads sitting in different buckets would read each
/// other's exponent and the mismatch would land here.
const SI_LADDER: [(f64, &str); 17] = [
    (1.3e-24, "1.30y"),
    (1.3e-21, "1.30z"),
    (1.3e-18, "1.30a"),
    (1.3e-15, "1.30f"),
    (1.3e-12, "1.30p"),
    (1.3e-9, "1.30n"),
    (1.3e-6, "1.30\u{b5}"),
    (1.3e-3, "1.30m"),
    (1.3, "1.30"),
    (1.3e3, "1.30k"),
    (1.3e6, "1.30M"),
    (1.3e9, "1.30G"),
    (1.3e12, "1.30T"),
    (1.3e15, "1.30P"),
    (1.3e18, "1.30E"),
    (1.3e21, "1.30Z"),
    (1.3e24, "1.30Y"),
];

/// One assertion per interesting corner of the mini-language, with the answers taken
/// from the pinned Node oracle rather than from the port.
const ANCHORS: [(&str, f64, &str); 17] = [
    ("$,.2f", 1234.5, "$1,234.50"),
    ("$,.2f", -1234.5, "\u{2212}$1,234.50"),
    (",d", 1234567.0, "1,234,567"),
    (".1%", 0.123, "12.3%"),
    ("+.2e", 1234.5, "+1.23e+3"),
    ("08.2f", -3.5, "\u{2212}0003.50"),
    ("#x", 48879.0, "0xbeef"),
    (".3r", 0.00012345, "0.000123"),
    ("(,.2f", -1234.5, "(1,234.50)"),
    ("~g", 1500.0, "1500"),
    ("^12.2f", 3.5, "    3.50    "),
    (".2p", 0.1234, "12%"),
    ("=+12,.3f", -98765.4321, "\u{2212} 98,765.432"),
    // Type `d` hands magnitudes at or above 1e21 to toLocaleString, and answers
    // infinity with U+221E rather than d3's usual minus-sign machinery.
    ("d", 1e21, "1000000000000000000000"),
    ("d", f64::INFINITY, "\u{221e}"),
    ("s", 0.0, "0.00000"),
    (".12~g", 0.0, "0"),
];

/// Specifiers the multithreaded workload drives every ladder value through.
///
/// Deliberately mixed: SI and non-SI, grouped and ungrouped, padded and not, so a
/// thread is rarely doing the same kind of work as its neighbour.
const WORKLOAD_SPECIFIERS: [&str; 8] =
    [".3s", "~s", "$,.2f", ",.4g", "+.3e", "012,.2f", ".2p", "#o"];

const WORKLOAD_THREADS: usize = 8;
const WORKLOAD_ROUNDS: usize = 120;

fn main() {
    let started = Instant::now();

    let anchored = anchored_outputs();
    let fallible = fallible_surface();
    let threaded = multithreaded_workload();

    println!(
        "canary default-consumer: OK -- {anchored} anchored, {fallible} fallible, \
         {threaded} threaded, {} value(s) total in {:.2}s",
        anchored + fallible + threaded,
        started.elapsed().as_secs_f64()
    );
}

/// The ordinary path: build a formatter once, reuse it, and get d3's bytes.
fn anchored_outputs() -> usize {
    let locale = Locale::en_us();
    let mut formatted = 0;

    let si = locale.formatter(".3s").expect("`.3s` is a valid specifier");
    for (value, expected) in SI_LADDER {
        assert_eq!(
            si.format_number(value).expect("in range"),
            expected,
            "`.3s` of {value:e}"
        );
        formatted += 1;
    }

    for (specifier, value, expected) in ANCHORS {
        let formatter = locale.formatter(specifier).expect("a valid specifier");
        assert_eq!(
            formatter.format_number(value).expect("in range"),
            expected,
            "`{specifier}` of {value}"
        );
        formatted += 1;
    }

    // formatPrefix: the SI prefix comes from the reference value once, so a column of
    // numbers shares a unit instead of each picking its own.
    let micro = locale
        .prefix_formatter(",.0", 1e-6)
        .expect("a valid specifier");
    assert_eq!(micro.format_number(0.00042).expect("in range"), "420\u{b5}");
    assert_eq!(
        micro.format_number(0.0042).expect("in range"),
        "4,200\u{b5}"
    );
    let mega = locale
        .prefix_formatter(",.0", 1e6)
        .expect("a valid specifier");
    assert_eq!(mega.format_number(1.3e9).expect("in range"), "1,300M");
    formatted += 3;

    // The specifier is a value with typed fields and a canonical spelling, and
    // `formatter_for` must agree with parsing that spelling back.
    let specifier: d3_format::FormatSpecifier = "$,.2f".parse().expect("a valid specifier");
    assert_eq!(specifier.to_string(), " >-$,.2f");
    assert_eq!(specifier.precision(), Some(2));
    assert_eq!(
        locale
            .formatter_for(&specifier)
            .expect("a valid specifier")
            .format_number(1234.5),
        locale
            .formatter(&specifier.to_string())
            .expect("a valid specifier")
            .format_number(1234.5)
    );
    formatted += 2;

    // The precision helpers answer in f64 because d3 does, and `as_precision` is the
    // documented step to the u32 a specifier takes.
    let digits = d3_format::as_precision(d3_format::precision_prefix(1e-6, 1.3e-3))
        .expect("a finite step has a precision");
    assert_eq!(digits, 3);
    let suggested = locale
        .formatter(&format!(".{digits}s"))
        .expect("a valid specifier");
    assert_eq!(suggested.format_number(1.3e-3).expect("in range"), "1.30m");
    assert_eq!(
        d3_format::as_precision(d3_format::precision_fixed(0.0)),
        None
    );
    assert_eq!(
        d3_format::as_precision(d3_format::precision_round(1e-3, 1.0)),
        Some(3)
    );
    formatted += 1;

    formatted
}

/// The fallible surface, which is where this crate deliberately parts company with
/// d3: a `Result` at a typed, recoverable point instead of coercion or an engine
/// limit.
fn fallible_surface() -> usize {
    let locale = Locale::en_us();

    // The message is d3's, exactly, because it is the one thing a caller may have
    // been matching on.
    let invalid = locale
        .formatter("foo")
        .expect_err("`foo` is not a specifier");
    assert_eq!(invalid.to_string(), "invalid format: foo");

    // Only type `c` takes text; everything else takes an f64. d3 would coerce.
    let numeric = locale.formatter("d").expect("a valid specifier");
    assert_eq!(numeric.input_kind(), InputKind::Number);
    assert_eq!(
        numeric.format_text("nope"),
        Err(FormatError::InputTypeMismatch {
            expected: InputKind::Number,
            actual: InputKind::Text,
        })
    );
    let text = locale.formatter("c").expect("a valid specifier");
    assert_eq!(text.input_kind(), InputKind::Text);
    assert_eq!(text.format_text("verbatim").expect("in range"), "verbatim");
    assert!(text.format_number(1.0).is_err());

    // A width past the ceiling is refused when the formatter is built, so a formatter
    // that exists cannot later fail to pad.
    let limits = FormatLimits {
        max_width_utf16: 64,
        ..FormatLimits::default()
    };
    assert_eq!(
        locale.formatter_with_limits("1000d", limits),
        Err(FormatError::WidthLimit {
            requested: 1000,
            maximum: 64
        })
    );
    // ...and the same width is fine under the default ceiling.
    let wide = locale
        .formatter("1000d")
        .expect("a valid specifier under the default limits");
    assert_eq!(
        wide.format_number(1.0).expect("in range").chars().count(),
        1000
    );

    // A caller-owned buffer is cleared, not appended to.
    let grouped = locale.formatter(",.2f").expect("a valid specifier");
    let mut out = String::with_capacity(32);
    grouped
        .format_number_into(&mut out, 1234.5)
        .expect("in range");
    grouped
        .format_number_into(&mut out, 6789.0)
        .expect("in range");
    assert_eq!(out, "6,789.00");

    5
}

/// Section 7's multithreaded formatter workload.
///
/// The property under test is that a formatted string depends on nothing but the
/// formatter and the value. So the workload computes every answer once on one
/// thread, then has eight threads recompute all of them concurrently -- each
/// starting at a different offset, so at any instant the threads are in different SI
/// buckets, formatting through different specifiers -- and compares every result
/// with the single-threaded one.
fn multithreaded_workload() -> usize {
    let locale = Locale::en_us();

    let cases: Vec<(&str, f64)> = WORKLOAD_SPECIFIERS
        .iter()
        .flat_map(|specifier| SI_LADDER.iter().map(move |(value, _)| (*specifier, *value)))
        .collect();

    let formatters: Vec<_> = cases
        .iter()
        .map(|(specifier, _)| locale.formatter(specifier).expect("a valid specifier"))
        .collect();

    let reference: Vec<String> = formatters
        .iter()
        .zip(&cases)
        .map(|(formatter, (_, value))| formatter.format_number(*value).expect("in range"))
        .collect();

    // One shared formatter and one shared prefix formatter, borrowed by every thread
    // at once. `thread::scope` is what lets the borrow be a borrow rather than an Arc,
    // and it only compiles because `Formatter` is `Sync`.
    let shared = locale.formatter(".3s").expect("a valid specifier");
    // formatPrefix rewrites the type to `f`, so this is `,.0f` scaled by 1e-6 with an
    // "M" suffix -- not `,.0s`. Getting that backwards is what the anchors are for.
    let shared_prefix = locale
        .prefix_formatter(",.0", 1e6)
        .expect("a valid specifier");

    thread::scope(|scope| {
        for worker in 0..WORKLOAD_THREADS {
            let cases = &cases;
            let formatters = &formatters;
            let reference = &reference;
            let shared = &shared;
            let shared_prefix = &shared_prefix;
            // A locale built on this thread and moved to another one. `Locale` is an
            // Arc over immutable data, so the clone is a refcount bump and the two
            // threads genuinely share the definition.
            let moved = locale.clone();

            scope.spawn(move || {
                let mut out = String::new();
                for round in 0..WORKLOAD_ROUNDS {
                    let offset = worker * cases.len() / WORKLOAD_THREADS + round;
                    for step in 0..cases.len() {
                        let index = (offset + step) % cases.len();
                        let (specifier, value) = cases[index];

                        // The reused-buffer path and the owned-String path must agree
                        // with each other and with the single-threaded answer.
                        formatters[index]
                            .format_number_into(&mut out, value)
                            .expect("in range");
                        assert_eq!(out, reference[index], "`{specifier}` of {value:e}");
                        assert_eq!(
                            formatters[index].format_number(value).expect("in range"),
                            reference[index],
                            "`{specifier}` of {value:e}"
                        );

                        // The shared formatter walks the ladder too, so eight threads
                        // are inside the same `Formatter` in eight different buckets.
                        let (ladder_value, ladder_expected) = SI_LADDER[index % SI_LADDER.len()];
                        assert_eq!(
                            shared.format_number(ladder_value).expect("in range"),
                            ladder_expected
                        );
                        // formatPrefix pins the bucket; interleaving must not move it.
                        assert_eq!(
                            shared_prefix.format_number(1.3e9).expect("in range"),
                            "1,300M"
                        );
                        // And a formatter built inside the worker from the moved locale
                        // behaves like one built on the main thread.
                        assert_eq!(
                            moved
                                .formatter(specifier)
                                .expect("a valid specifier")
                                .format_number(value)
                                .expect("in range"),
                            reference[index]
                        );
                    }
                }
            });
        }
    });

    reference.len() + WORKLOAD_THREADS * WORKLOAD_ROUNDS * cases.len() * 4
}
