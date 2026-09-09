//! The concurrency stress of section 7's Phase 4 exit.
//!
//! d3 keeps `prefixExponent` in a module-level variable that `formatPrefixAuto`
//! writes and the closure returned by `newFormat` reads on the next statement. In
//! JavaScript that is safe because there is one thread; a port that reproduced the
//! shape rather than the behaviour would have a data race, and the symptom would be
//! an SI suffix from *another* call appearing on this one. `no-global-si-prefix-leak`
//! in `DIVERGENCES.md` says the port carries that exponent in a return value
//! instead, and this is the test that would notice if it stopped.
//!
//! The method is the same throughout: compute every answer on one thread first,
//! then recompute the same work concurrently, from threads deliberately started at
//! different points in the workload so that a type `s` conversion on one thread
//! overlaps a type `f` conversion on another. Every disagreement is counted rather
//! than asserted one at a time, so a failure reports how badly it raced instead of
//! only that it did.
//!
//! `Locale` and `Formatter` being `Send + Sync` is a compile-time fact checked in
//! `tests/public_api.rs`. This is the observable half.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use d3_format::Locale;

/// Enough of the grammar that the threads are doing different kinds of work.
///
/// Type `s` and `formatPrefix` are the point -- they are the two that touch the SI
/// exponent -- and the rest are here to be interleaved with them.
const SPECIFIERS: [&str; 16] = [
    ".3s", "s", ",.2s", "$.3s", ".2f", "$,.2f", ",d", "#x", ".3e", ".4g", ".2%", ".3r", "020,.2f",
    "^12.3s", "+.1s", ".6~g",
];

/// Values spread across the SI table, plus the edges that pick no bucket at all.
const VALUES: [f64; 18] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    1234.5678,
    -1234.5678,
    1.3e6,
    -1.3e6,
    1e-7,
    9.999e-7,
    1e24,
    1e-24,
    1e25,
    f64::NAN,
    -f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
    999_999.0,
];

/// References for the `formatPrefix` half, including the degenerate ones.
const REFERENCES: [f64; 8] = [
    1e6,
    -1e6,
    1e-6,
    1.0,
    0.0,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
];

const THREADS: usize = 8;
const ROUNDS: usize = 3;

/// Every locale the crate can produce, named.
///
/// The generated table when it is compiled in -- that is the locale-heavy workload
/// section 7 asks for, 59 definitions with 23 distinct numeral sets -- and a small
/// hand-built set otherwise, so the test still says something without the feature.
fn locales() -> Vec<(String, Locale)> {
    #[cfg(feature = "locales")]
    {
        d3_format::locales::names()
            .map(|name| {
                let locale =
                    Locale::named(name).unwrap_or_else(|| panic!("{name} is in the table"));
                (name.to_owned(), locale)
            })
            .collect()
    }
    #[cfg(not(feature = "locales"))]
    {
        vec![
            ("en-US".to_owned(), Locale::en_us()),
            (
                "ar-001".to_owned(),
                Locale::builder()
                    .decimal("\u{66b}")
                    .thousands("\u{66c}")
                    .grouping([3])
                    .currency("", "")
                    .numerals([
                        "\u{660}".to_owned(),
                        "\u{661}".to_owned(),
                        "\u{662}".to_owned(),
                        "\u{663}".to_owned(),
                        "\u{664}".to_owned(),
                        "\u{665}".to_owned(),
                        "\u{666}".to_owned(),
                        "\u{667}".to_owned(),
                        "\u{668}".to_owned(),
                        "\u{669}".to_owned(),
                    ])
                    .build()
                    .expect("valid"),
            ),
            (
                "fr-FR".to_owned(),
                Locale::builder()
                    .decimal(",")
                    .thousands(".")
                    .grouping([3])
                    .currency("", "\u{a0}\u{20ac}")
                    .percent("\u{202f}%")
                    .build()
                    .expect("valid"),
            ),
        ]
    }
}

/// One unit of work: which locale, which specifier, which value.
#[derive(Clone, Copy)]
struct Item {
    locale: usize,
    specifier: usize,
    value: usize,
}

fn workload(locale_count: usize) -> Vec<Item> {
    let mut items = Vec::new();
    for locale in 0..locale_count {
        for specifier in 0..SPECIFIERS.len() {
            for value in 0..VALUES.len() {
                // A third of the cross product, offset per locale, which keeps the
                // run to a few hundred thousand formats without any locale seeing
                // the same subset as its neighbour.
                if (locale + specifier + value) % 3 == 0 {
                    items.push(Item {
                        locale,
                        specifier,
                        value,
                    });
                }
            }
        }
    }
    items
}

/// Runs `answer` over the workload on one thread, then on `THREADS` at once.
///
/// Each thread walks the whole list, but starts at its own offset, so no two threads
/// are on the same item at the same time and the overlap between kinds of work is
/// different on every round.
fn stress(label: &str, answer: impl Fn(&Item) -> String + Sync) {
    let items = Arc::new(workload(locales().len()));
    let expected: Vec<String> = items.iter().map(&answer).collect();

    let mismatches = AtomicUsize::new(0);
    let formatted = AtomicUsize::new(0);
    let first = std::sync::Mutex::new(None::<String>);

    std::thread::scope(|scope| {
        for thread in 0..THREADS {
            let items = Arc::clone(&items);
            let expected = &expected;
            let answer = &answer;
            let mismatches = &mismatches;
            let formatted = &formatted;
            let first = &first;
            scope.spawn(move || {
                let start = thread * items.len() / THREADS;
                for round in 0..ROUNDS {
                    for step in 0..items.len() {
                        let index = (start + step + round) % items.len();
                        let actual = answer(&items[index]);
                        formatted.fetch_add(1, Ordering::Relaxed);
                        if actual != expected[index] {
                            mismatches.fetch_add(1, Ordering::Relaxed);
                            let mut first = first.lock().expect("not poisoned");
                            if first.is_none() {
                                *first = Some(format!(
                                    "item {index}: expected {:?}, got {actual:?}",
                                    expected[index]
                                ));
                            }
                        }
                    }
                }
            });
        }
    });

    let mismatches = mismatches.load(Ordering::Relaxed);
    assert_eq!(
        mismatches,
        0,
        "{label}: {mismatches} of {} concurrent results differ from the single-threaded \
answer; first was {}",
        formatted.load(Ordering::Relaxed),
        first
            .lock()
            .expect("not poisoned")
            .as_deref()
            .unwrap_or("(none recorded)")
    );
    eprintln!(
        "d3-format: {label}: {} concurrent formats across {THREADS} threads, 0 mismatches",
        formatted.load(Ordering::Relaxed)
    );
}

/// Formatters compiled per call, which is the shape a caller writes first.
#[test]
fn compiling_and_formatting_concurrently_agrees_with_one_thread() {
    let locales = locales();
    stress("format", |item| {
        let (name, locale) = &locales[item.locale];
        let specifier = SPECIFIERS[item.specifier];
        locale
            .formatter(specifier)
            .unwrap_or_else(|error| panic!("{name} {specifier:?}: {error}"))
            .format_number(VALUES[item.value])
            .unwrap_or_else(|error| panic!("{name} {specifier:?}: {error}"))
    });
}

/// One formatter per (locale, specifier), shared across every thread.
///
/// This is the arrangement that would race if a formatter held any interior state:
/// the same `&Formatter` is being asked for a type `s` conversion of `1e24` on one
/// thread and of `1e-24` on another at the same instant.
#[test]
fn one_shared_formatter_per_specifier_agrees_with_one_thread() {
    let locales = locales();
    let shared: Vec<Vec<_>> = locales
        .iter()
        .map(|(name, locale)| {
            SPECIFIERS
                .iter()
                .map(|specifier| {
                    locale
                        .formatter(specifier)
                        .unwrap_or_else(|error| panic!("{name} {specifier:?}: {error}"))
                })
                .collect()
        })
        .collect();

    stress("shared formatter", |item| {
        shared[item.locale][item.specifier]
            .format_number(VALUES[item.value])
            .expect("valid")
    });
}

/// `formatPrefix`, where the SI exponent is fixed at compile time.
///
/// The suffix belongs to the reference rather than to the value, so a leaked
/// exponent would show up as one formatter's suffix on another formatter's output.
/// The degenerate references are in the mix, and they must stay NaN under load.
#[test]
fn prefix_formatters_do_not_share_an_exponent() {
    let locales = locales();
    stress("formatPrefix", |item| {
        let (name, locale) = &locales[item.locale];
        // The reference is drawn from the item so that adjacent work items use
        // different buckets, which is what makes a leak visible.
        let reference = REFERENCES[(item.specifier + item.value) % REFERENCES.len()];
        let specifier = SPECIFIERS[item.specifier];
        locale
            .prefix_formatter(specifier, reference)
            .unwrap_or_else(|error| panic!("{name} {specifier:?} @ {reference}: {error}"))
            .format_number(VALUES[item.value])
            .unwrap_or_else(|error| panic!("{name} {specifier:?} @ {reference}: {error}"))
    });
}

/// A locale outliving the thread that built it, and shared by all of them.
///
/// Section 5.3 says a `Locale` is a value with no process-global state; the practical
/// consequence is that one can be built on a worker, moved, and read from everywhere
/// at once. A reference-counted definition that was not thread-safe would fail here
/// rather than in a review.
#[test]
fn a_locale_built_on_one_thread_serves_all_of_them() {
    let built = std::thread::spawn(|| {
        Locale::builder()
            .decimal("\u{66b}")
            .thousands("\u{66c}")
            .grouping([3])
            .currency("", " \u{62f}.\u{625}.")
            .numerals([
                "\u{660}".to_owned(),
                "\u{661}".to_owned(),
                "\u{662}".to_owned(),
                "\u{663}".to_owned(),
                "\u{664}".to_owned(),
                "\u{665}".to_owned(),
                "\u{666}".to_owned(),
                "\u{667}".to_owned(),
                "\u{668}".to_owned(),
                "\u{669}".to_owned(),
            ])
            .build()
            .expect("valid")
    })
    .join()
    .expect("no panic");

    let expected = built.formatter("$,.2f").expect("valid");
    let expected = expected.format_number(-1234.56).expect("valid");
    // `test/arabicLocale-test.js`, whose recorded answer this is, less the currency.
    assert_eq!(
        expected,
        "\u{2212}\u{661}\u{66c}\u{662}\u{663}\u{664}\u{66b}\u{665}\u{666} \u{62f}.\u{625}."
    );

    let built = Arc::new(built);
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            let built = Arc::clone(&built);
            let expected = expected.as_str();
            scope.spawn(move || {
                // Cloning the locale on each iteration exercises the shared handle
                // as well as the formatting.
                for _ in 0..2_000 {
                    let locale = Locale::clone(&built);
                    let actual = locale
                        .formatter("$,.2f")
                        .expect("valid")
                        .format_number(-1234.56)
                        .expect("valid");
                    assert_eq!(actual, expected);
                }
            });
        }
    });
}
