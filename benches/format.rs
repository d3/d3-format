//! What a formatted value costs, and what it allocates.
//!
//! MIGRATION_TO_RUST.md section 7 dictates this file's shape, and every clause of
//! it guards against a specific way a formatting benchmark misleads:
//!
//! - *"owned output and reused-output paths separately"*, because
//!   `Formatter::format_number` and `Formatter::format_number_into` differ only in
//!   who owns the buffer, and one blended number hides the only figure a caller
//!   choosing between them wants;
//! - *"each case reserves enough capacity outside the timed loop"*, because a
//!   reused-buffer benchmark that lets the buffer grow inside the loop amortizes
//!   the first call's allocation over the sample and calls the result steady state;
//! - *"worst-case subnormal and maximum radix outputs are distinct groups"*,
//!   because both cost two to three orders of magnitude more than an ordinary
//!   value, so averaging them with one would be a statement about neither;
//! - *"no allocation-free claim is made without allocator instrumentation"*, which
//!   is why this file installs a counting global allocator and prints exact counts
//!   before it times anything.
//!
//! # What the two output paths actually compare
//!
//! They are the same work apart from the destination buffer. `format_number`
//! allocates a fresh `String`, fills it, hands it over, and the caller drops it;
//! `format_number_into` clears the caller's `String` and fills that. So the
//! difference between the two groups is one allocation and one deallocation per
//! call *plus* whatever the byte count makes of them -- which is why the same cases
//! appear in both groups under the same ids, and why the reused buffer is reserved
//! to the case's exact output length before the timer starts. Section 5.3 is
//! careful to say the reused path "reuses caller capacity but does not claim to be
//! allocation-free", and the allocation profile below is what turns that from a
//! disclaimer into a measurement: exact decimal conversion allocates limbs and
//! digit vectors regardless of who owns the output.
//!
//! # Why these are the worst cases
//!
//! **Subnormal.** `5e-324` is `2^-1074`, and its exact decimal expansion is 751
//! significant digits ending 1,074 places after the point. Nothing about that is
//! approximated here: section 4.2's engine generates the digits of the exact
//! rational value over `BigNat`, and for this one the denominator is `2^1074`, so
//! every rounding decision is a comparison between thousand-bit integers rather
//! than the machine arithmetic an ordinary value gets. High-precision fixed and
//! exponential output on the same value is the most work a single `f64` can ask
//! for, and `f64::MIN_POSITIVE` is included so the normal/subnormal boundary is
//! measured rather than assumed to behave like its neighbour.
//!
//! **Radix.** Section 4.4 requires "radix output up to the 1,024-bit
//! representation of `f64::MAX`", and type `b` is where that lands: `f64::MAX` is
//! an integer with a 1,024-bit binary expansion, so the output is 1,024 characters
//! from a `BigNat` shifted one bit at a time. Types `o`, `x` and `X` are the same
//! integer at three, four and four bits per digit, which is the cheap end of the
//! same path and the reason they are worth having next to it.
//!
//! # Why construction is its own group
//!
//! The point of section 5.3's compiled `Formatter` is that parsing the grammar,
//! resolving `n` to `,g`, composing the affixes and clamping the precision happen
//! once and per-value formatting pays none of it. That claim is only meaningful if
//! the two costs are reported apart, so `construction` measures the four entry
//! points a caller can reach -- `str::parse`, `Locale::formatter`,
//! `Locale::formatter_for` and `Locale::prefix_formatter` -- along with the
//! `to_string` that the third of those is implemented on top of and the locale
//! build itself, and nothing in the other groups includes any of them.
//!
//! # Timing configuration
//!
//! The warm-up and measurement times below are set on the `Criterion` value
//! *before* `configure_from_args`, so they are defaults a `--measurement-time`
//! argument still overrides. The warm-up is a third of Criterion's, because every
//! case here reaches millions of iterations in well under a second and the only
//! thing a longer warm-up buys is more of the CPU's frequency ramp. The
//! measurement window is Criterion's own, and it was not shortened after trying:
//! at 2 s per case the owned/reused difference on this host inverted on two of the
//! eight representative cases between consecutive runs, which is the ordinary
//! meaning of "the effect is at the noise floor" and not a result worth archiving.
//!
//! # Reading the timings honestly
//!
//! The counting allocator is installed for the whole process, so the timings
//! include its overhead on every allocation the measured code makes. With
//! recording off -- which is how it sits for the entire timed portion of the run --
//! that overhead is one relaxed load and a predictable branch per allocation, not
//! the atomic increments the profile phase pays. It is not zero, and a comparison
//! against a build without the allocator would not be exactly this.
//!
//! More importantly, the owned/reused difference is one allocation and one
//! deallocation per call, which is tens of nanoseconds against calls that take
//! hundreds. On a developer machine that is close enough to the run-to-run spread
//! that a single case's delta should not be read as a measurement of anything. The
//! part of that comparison which *is* exact is in the allocation profile: the
//! reused path makes exactly one fewer allocation per call, every time, and on the
//! cases where the owned buffer had to grow it also avoids the reallocations.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion};
use d3_format::{
    precision_fixed, precision_prefix, precision_round, FormatSpecifier, Formatter, Locale,
};

/// Criterion's defaults are 3 s warm-up and 5 s measurement per case; see the
/// timing-configuration note above for why the warm-up is shorter and the
/// measurement window is not.
const WARM_UP_TIME: Duration = Duration::from_secs(1);
const MEASUREMENT_TIME: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Counting global allocator
// ---------------------------------------------------------------------------

/// Calls to `alloc` and `alloc_zeroed` while recording.
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
/// Calls to `realloc` while recording.
///
/// Counted apart from `ALLOCATIONS` rather than added to it. Appending to a warm
/// `String` never calls `alloc`, but outgrowing one calls `realloc`, so a single
/// "allocations" figure would let a growth-heavy path advertise itself as
/// allocation-free while it repeatedly copied its buffer.
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
/// Fresh bytes requested while recording: `alloc` and `alloc_zeroed` sizes, plus
/// the growth part of a `realloc`. A shrinking `realloc` contributes nothing,
/// which is why this is not simply the sum of the sizes passed in.
static BYTES: AtomicU64 = AtomicU64::new(0);
/// Whether the counters are live.
///
/// The gate exists for the timings, not for the profile. Criterion's own sampling
/// allocates, and so does every measured call, so leaving three atomic
/// read-modify-writes on the hot path would tax exactly the paths this file
/// compares. A relaxed load of one `bool` is what the timed portion of the run
/// pays instead.
///
/// Everything here is `Relaxed` because the profile runs on one thread before
/// Criterion starts: a thread always observes its own writes to a location in
/// program order, and no ordering with respect to *other* locations is needed to
/// count events that all happen between two stores to this one.
static RECORDING: AtomicBool = AtomicBool::new(false);

/// `System`, plus the counters section 7 requires before anything may be said
/// about allocation.
struct CountingAllocator;

// SAFETY: every method forwards to `System` with the caller's own pointer and
// layout arguments and returns what `System` returned, so the `GlobalAlloc`
// contract this implementation must uphold reduces to the one `System` already
// upholds. The added code is atomic arithmetic on four statics, which allocates
// nothing -- a counter that allocated would re-enter the allocator -- and cannot
// affect a pointer or a layout.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if RECORDING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: `layout` is the caller's, forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if RECORDING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: `layout` is the caller's, forwarded unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if RECORDING.load(Ordering::Relaxed) {
            REALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(
                new_size.saturating_sub(layout.size()) as u64,
                Ordering::Relaxed,
            );
        }
        // SAFETY: `ptr`, `layout` and `new_size` are the caller's, forwarded
        // unchanged, so `ptr` is still a block this allocator handed out under
        // `layout` and `new_size` is still whatever the caller had already
        // established as valid.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Deliberately uncounted. A deallocation is not an allocation, and pairing
        // the two would only restate the count under another name.
        // SAFETY: `ptr` and `layout` are the caller's, forwarded unchanged.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// What one call allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Allocated {
    allocations: u64,
    reallocations: u64,
    bytes: u64,
}

impl Allocated {
    fn read() -> Allocated {
        Allocated {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            reallocations: REALLOCATIONS.load(Ordering::Relaxed),
            bytes: BYTES.load(Ordering::Relaxed),
        }
    }
}

/// One call's allocation count, and whether the next identical call matched it.
#[derive(Debug, Clone, Copy)]
struct Sample {
    allocated: Allocated,
    repeatable: bool,
}

/// Counts one call, then counts it again to see whether the count holds.
///
/// A single sample would assert a steady state it never demonstrates. The first
/// call down a path can pay for a `LazyLock` table -- `Locale::en_us()` is one --
/// and a buffer that is still growing allocates once and then stops, so the
/// interesting number is the second call's, and whether a third agrees with it is
/// evidence rather than an assumption. Nothing here averages: these are exact
/// integer event counts, and an average of them would only obscure a case where
/// they differ.
fn sample<T>(body: &mut impl FnMut() -> T) -> Sample {
    drop(black_box(body()));
    let first = count_once(body);
    let second = count_once(body);
    Sample {
        allocated: first,
        repeatable: first == second,
    }
}

/// Counts exactly one call.
///
/// `black_box` on the result is not decoration: in a release build LLVM is
/// entitled to delete an allocation whose result is never observed, and deleting
/// the thing being counted is the one failure this measurement cannot detect.
fn count_once<T>(body: &mut impl FnMut() -> T) -> Allocated {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    REALLOCATIONS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);

    RECORDING.store(true, Ordering::Relaxed);
    let value = black_box(body());
    RECORDING.store(false, Ordering::Relaxed);

    let counted = Allocated::read();
    // After the counters are read, so the drop's deallocation is outside the
    // recorded window and cannot be mistaken for part of the call.
    drop(value);
    counted
}

// ---------------------------------------------------------------------------
// The cases
// ---------------------------------------------------------------------------

/// Which locale a case compiles against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    EnUs,
    /// A locale with Arabic-Indic numerals, which section 4.4 pins as substituted
    /// last over the fully assembled string. That ordering means the substitution
    /// walks the padding and the affixes too, so it is a layout cost the default
    /// locale never pays and worth measuring on its own.
    ArabicNumerals,
}

/// What a case feeds its formatter.
#[derive(Debug, Clone, Copy)]
enum Input {
    Number(f64),
    Text(&'static str),
}

/// One benchmarked call, named once and shared by the timing groups and the
/// allocation profile so that the two cannot describe different work.
#[derive(Debug, Clone, Copy)]
struct Case {
    /// The benchmark id inside its group, and the label in the profile. The same
    /// id names the same case in `owned_output` and in `reused_output`, which is
    /// what makes those two groups comparable line for line.
    id: &'static str,
    locale: Which,
    spec: &'static str,
    input: Input,
}

impl Case {
    const fn number(id: &'static str, locale: Which, spec: &'static str, value: f64) -> Case {
        Case {
            id,
            locale,
            spec,
            input: Input::Number(value),
        }
    }

    const fn text(
        id: &'static str,
        locale: Which,
        spec: &'static str,
        value: &'static str,
    ) -> Case {
        Case {
            id,
            locale,
            spec,
            input: Input::Text(value),
        }
    }

    fn formatter(&self) -> Formatter {
        locale(self.locale)
            .formatter(self.spec)
            .unwrap_or_else(|error| panic!("case `{}` must compile: {error}", self.id))
    }

    /// The owned path.
    fn render(&self, formatter: &Formatter) -> String {
        match self.input {
            Input::Number(value) => formatter.format_number(value),
            Input::Text(value) => formatter.format_text(value),
        }
        .unwrap_or_else(|error| panic!("case `{}` must format: {error}", self.id))
    }

    /// The reused path. Returns the byte length written, so that the caller
    /// observes the buffer and the optimizer cannot discard the stores into it.
    fn render_into(&self, formatter: &Formatter, out: &mut String) -> usize {
        match self.input {
            Input::Number(value) => formatter.format_number_into(out, value),
            Input::Text(value) => formatter.format_text_into(out, value),
        }
        .unwrap_or_else(|error| panic!("case `{}` must format: {error}", self.id));
        out.len()
    }

    /// The input, spelled for the profile record. `{:?}` on an `f64` is the
    /// shortest round-tripping form, which is the one that names the bit pattern.
    fn input_label(&self) -> String {
        match self.input {
            Input::Number(value) => format!("{value:?}"),
            Input::Text(value) => value.to_owned(),
        }
    }
}

/// Ordinary values across the type letters a caller actually reaches, wide enough
/// to be representative and short enough that the owned/reused comparison is not
/// dominated by one outlier. Grouping, currency affixes, a sign, zero fill, an SI
/// prefix and the trimmed general form are each here once.
const REPRESENTATIVE: &[Case] = &[
    Case::number("integer_grouped", Which::EnUs, ",d", 1_234_567.0),
    Case::number("currency_grouped", Which::EnUs, "$,.2f", 1234.5678),
    Case::number("fixed_2", Which::EnUs, ".2f", 1234.5678),
    Case::number("exponential_6", Which::EnUs, ".6e", 1234.5678),
    Case::number("general_trimmed", Which::EnUs, "", 1234.5678),
    Case::number("si_prefix_3", Which::EnUs, ".3s", 1.3e6),
    Case::number("percent_1", Which::EnUs, ".1%", 0.4567),
    Case::number("zero_padded_negative", Which::EnUs, "020,.4f", -1234.5678),
];

/// The subnormal worst cases. `5e-324` is the minimum subnormal, `2^-1074`, whose
/// exact expansion runs to 751 significant digits, and the four specifiers below
/// reach it through a different conversion each: the trimmed general form at 12
/// significant digits that an empty specifier resolves to, the exponential
/// conversion at 20 fraction digits, the 21 significant digits that are the most
/// d3 asks for, and fixed notation at 20 places. `f64::MIN_POSITIVE` is the
/// smallest normal, one ulp away from a different path through the significand.
const SUBNORMAL: &[Case] = &[
    Case::number("general_12_min_subnormal", Which::EnUs, "", 5e-324),
    Case::number("exponential_20_min_subnormal", Which::EnUs, ".20e", 5e-324),
    Case::number("rounded_21_min_subnormal", Which::EnUs, ".21r", 5e-324),
    Case::number("fixed_20_min_subnormal", Which::EnUs, ".20f", 5e-324),
    Case::number("general_12_min_normal", Which::EnUs, "", f64::MIN_POSITIVE),
];

/// The maximum radix outputs. `f64::MAX` in binary is the 1,024-bit expansion
/// section 4.4 names; `o`, `x` and `X` are the same integer read three, four and
/// four bits at a time, for 342, 256 and 256 characters.
const MAX_RADIX: &[Case] = &[
    Case::number("binary_f64_max", Which::EnUs, "b", f64::MAX),
    Case::number("octal_f64_max", Which::EnUs, "o", f64::MAX),
    Case::number("hex_lower_f64_max", Which::EnUs, "x", f64::MAX),
    Case::number("hex_upper_f64_max", Which::EnUs, "X", f64::MAX),
];

/// Layout work that no amount of numeric conversion accounts for: type `c`, which
/// runs no conversion at all, and two cases under a numeral-substituting locale.
/// `arabic_hex_prefixed` is section 4.4's `format("#x")(48879)` -- the `0` of the
/// `0x` prefix is substituted and the hex digits are not, which is observable only
/// because substitution runs last over the assembled string.
const LAYOUT: &[Case] = &[
    Case::text("text_padded", Which::EnUs, ">8c", "hello"),
    Case::number(
        "arabic_grouped_integer",
        Which::ArabicNumerals,
        ",d",
        1_234_567.0,
    ),
    Case::number("arabic_hex_prefixed", Which::ArabicNumerals, "#x", 48879.0),
];

/// Every case the allocation profile reports, with the group it belongs to.
const PROFILED: &[(&str, &[Case])] = &[
    ("owned_output/reused_output", REPRESENTATIVE),
    ("subnormal", SUBNORMAL),
    ("max_radix", MAX_RADIX),
    ("text_and_locale", LAYOUT),
];

fn locale(which: Which) -> Locale {
    match which {
        Which::EnUs => Locale::en_us(),
        Which::ArabicNumerals => Locale::builder()
            .grouping([3])
            .thousands(",")
            .numerals(
                [
                    "\u{660}", "\u{661}", "\u{662}", "\u{663}", "\u{664}", "\u{665}", "\u{666}",
                    "\u{667}", "\u{668}", "\u{669}",
                ]
                .map(String::from),
            )
            .build()
            .expect("the Arabic-numeral locale must build"),
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

/// The specifier the construction measurements compile. It exercises a symbol, a
/// grouping flag and a precision, so nothing in the grammar is skipped by being
/// absent.
const CONSTRUCTION_SPEC: &str = "$,.2f";

/// The reference `prefix_formatter` fixes its SI bucket from.
const PREFIX_REFERENCE: f64 = 1.3e6;

/// One construction path.
///
/// `run` is a plain `fn` so that the timing group and the allocation profile
/// invoke the identical body; a closure written out twice would be two bodies
/// that merely look alike.
struct Construction {
    id: &'static str,
    /// The expression measured, spelled as source for the profile record.
    detail: &'static str,
    run: fn(&Locale, &FormatSpecifier),
}

const CONSTRUCTIONS: &[Construction] = &[
    Construction {
        id: "parse_specifier",
        detail: "\"$,.2f\".parse::<FormatSpecifier>()",
        // `black_box(value);` as a statement rather than `drop(black_box(value))`:
        // the value is still materialized and still dropped at the end of the
        // statement, and `FormatSpecifier` is a plain value type, which makes the
        // explicit `drop` a `clippy::drop_non_drop` error.
        run: |_, _| {
            black_box(
                black_box(CONSTRUCTION_SPEC)
                    .parse::<FormatSpecifier>()
                    .expect("the construction specifier must parse"),
            );
        },
    },
    Construction {
        id: "specifier_to_string",
        // Not a construction path of its own, but the one `formatter_for` adds on
        // top of `formatter`: section 5.3 implements `formatter_for` as
        // `formatter(&spec.to_string())` precisely so it cannot skip
        // normalization, and this is what that costs.
        detail: "FormatSpecifier::to_string",
        run: |_, specifier| {
            black_box(black_box(specifier).to_string());
        },
    },
    Construction {
        id: "formatter_from_str",
        detail: "Locale::formatter(\"$,.2f\")",
        run: |locale, _| {
            black_box(
                locale
                    .formatter(black_box(CONSTRUCTION_SPEC))
                    .expect("the construction specifier must compile"),
            );
        },
    },
    Construction {
        id: "formatter_for_specifier",
        detail: "Locale::formatter_for(&FormatSpecifier)",
        run: |locale, specifier| {
            black_box(
                locale
                    .formatter_for(black_box(specifier))
                    .expect("the construction specifier must compile"),
            );
        },
    },
    Construction {
        id: "prefix_formatter",
        detail: "Locale::prefix_formatter(\"$,.2f\", 1.3e6)",
        run: |locale, _| {
            black_box(
                locale
                    .prefix_formatter(black_box(CONSTRUCTION_SPEC), black_box(PREFIX_REFERENCE))
                    .expect("the construction specifier must compile"),
            );
        },
    },
    Construction {
        id: "locale_with_numerals",
        // The other once-per-program cost. `Locale::en_us()` is a `LazyLock` and
        // an `Arc` clone, so it would measure nothing; a builder run with ten
        // numerals is what a caller supplying their own locale actually pays.
        detail: "Locale::builder() with grouping, thousands and numerals",
        run: |_, _| {
            black_box(locale(Which::ArabicNumerals));
        },
    },
];

// ---------------------------------------------------------------------------
// The allocation profile
// ---------------------------------------------------------------------------

/// Escapes a string for the machine-readable block.
///
/// Every label in this file is ASCII with no quote or backslash in it, so this is
/// currently the identity. It is here so that adding a case with an awkward
/// specifier cannot silently emit a document that a reader of the profile block
/// then fails to parse.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            control if control < ' ' => out.push_str(&format!("\\u{:04x}", control as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn json_sample(sample: Sample) -> String {
    format!(
        "{{\"allocations\":{},\"reallocations\":{},\"bytes\":{},\"repeatable\":{}}}",
        sample.allocated.allocations,
        sample.allocated.reallocations,
        sample.allocated.bytes,
        sample.repeatable
    )
}

/// One profiled case: the same call down both output paths.
struct Profiled {
    group: &'static str,
    case: &'static Case,
    output_bytes: usize,
    /// The capacity the reused buffer was given before any measurement, and still
    /// had after all of them.
    reserved: usize,
    owned: Sample,
    reused: Sample,
}

fn profile_case(group: &'static str, case: &'static Case) -> Profiled {
    let formatter = case.formatter();
    let expected = case.render(&formatter);

    // The reused buffer is reserved to the exact output length, once, here --
    // outside every measurement, which is section 7's requirement and also the
    // only way the profile answers the question it is asked. "Does the reused path
    // allocate when the caller has already provided the room?" is not a question a
    // buffer that grows during the measurement can answer.
    let mut out = String::new();
    out.reserve(expected.len());
    let reserved = out.capacity();

    // A case table is a place to make a typo. Formatting once down each path and
    // comparing costs nothing here and means a mislabelled case cannot quietly
    // become a benchmark of something else.
    case.render_into(&formatter, &mut out);
    assert_eq!(
        out, expected,
        "case `{}`: the two output paths disagree",
        case.id
    );

    let owned = sample(&mut || case.render(&formatter));
    let reused = sample(&mut || case.render_into(&formatter, &mut out));

    // What makes the reused row readable. Several cases count a reallocation on
    // both paths, and the number only means "the conversion grew a buffer of its
    // own" if the caller's buffer demonstrably did not grow. A `String`'s capacity
    // changes if and only if it reallocated, so this is that proof rather than an
    // inference from the counts -- and it is also the check that `reserve(len)`
    // really was "enough capacity" in section 7's sense.
    assert_eq!(
        out.capacity(),
        reserved,
        "case `{}`: the reused buffer grew during the measurement, so it was not \
         reserved to enough capacity and the reused row is measuring the wrong thing",
        case.id
    );

    Profiled {
        group,
        case,
        output_bytes: expected.len(),
        reserved,
        owned,
        reused,
    }
}

/// Measures and prints the allocation profile.
///
/// Printed to stderr, before the timing runs and independently of them, for two
/// reasons. Criterion owns stdout and interleaving with its report would make both
/// harder to read; and these are exact event counts, taken while the process is
/// still single-threaded and nothing else is running, which is a different kind of
/// number from a bootstrapped confidence interval and should not look like one.
///
/// # What these numbers license
///
/// They are counts of `GlobalAlloc` calls made by one call to the named entry
/// point on this machine, this build and this toolchain. They support statements
/// of the form "the reused path made N allocations for this case". They do **not**
/// support "allocation-free": a zero here is a zero for one case on one build, and
/// section 5.3 says outright that exact decimal conversion may allocate temporary
/// limbs and digits, which the subnormal and radix rows show it doing. They also
/// say nothing about peak resident memory, about fragmentation, or about what a
/// different allocator would do with the same request sequence.
fn report_allocation_profile() {
    let profiled: Vec<Profiled> = PROFILED
        .iter()
        .flat_map(|(group, cases)| cases.iter().map(move |case| profile_case(group, case)))
        .collect();

    let constructed: Vec<(&'static Construction, Sample)> = CONSTRUCTIONS
        .iter()
        .map(|construction| {
            let locale = locale(Which::EnUs);
            let specifier: FormatSpecifier = CONSTRUCTION_SPEC
                .parse()
                .expect("the construction specifier must parse");
            let sampled = sample(&mut || (construction.run)(&locale, &specifier));
            (construction, sampled)
        })
        .collect();

    eprintln!();
    eprintln!("d3-format allocation profile (MIGRATION_TO_RUST.md section 7)");
    eprintln!("  exact GlobalAlloc call counts for one call, taken before the timing runs");
    eprintln!("  allocs   calls to alloc and alloc_zeroed");
    eprintln!("  reallocs calls to realloc");
    eprintln!("  bytes    fresh bytes requested: alloc sizes, plus the growth part of a realloc");
    eprintln!("  owned    format_number / format_text, returning a new String");
    eprintln!("  reused   ..._into a String reserved to the exact output length beforehand");
    eprintln!("  a trailing ? marks a count the immediate repeat call did not reproduce");
    eprintln!("  every reused row leaves the buffer's capacity untouched -- asserted, not");
    eprintln!("  assumed -- so a reallocation counted there grew a buffer inside the");
    eprintln!("  conversion and not the caller's output");
    eprintln!();
    eprintln!(
        "  {:<28} {:>6}  {:^24}  {:^24}",
        "", "output", "owned", "reused"
    );
    eprintln!(
        "  {:<28} {:>6}  {:>7} {:>8} {:>7}  {:>7} {:>8} {:>7}",
        "case", "bytes", "allocs", "reallocs", "bytes", "allocs", "reallocs", "bytes"
    );
    eprintln!("  {}", "-".repeat(28 + 8 + 26 + 26));

    let mut group = "";
    for entry in &profiled {
        if entry.group != group {
            group = entry.group;
            eprintln!("  [{group}]");
        }
        eprintln!(
            "  {:<28} {:>6}  {:>7} {:>8} {:>7}  {:>7} {:>8} {:>7}",
            entry.case.id,
            entry.output_bytes,
            marked(entry.owned.allocated.allocations, entry.owned.repeatable),
            marked(entry.owned.allocated.reallocations, entry.owned.repeatable),
            marked(entry.owned.allocated.bytes, entry.owned.repeatable),
            marked(entry.reused.allocated.allocations, entry.reused.repeatable),
            marked(
                entry.reused.allocated.reallocations,
                entry.reused.repeatable
            ),
            marked(entry.reused.allocated.bytes, entry.reused.repeatable),
        );
    }

    eprintln!("  [construction] paid once per formatter, never per value");
    for (construction, sampled) in &constructed {
        eprintln!(
            "  {:<28} {:>6}  {:>7} {:>8} {:>7}",
            construction.id,
            "",
            marked(sampled.allocated.allocations, sampled.repeatable),
            marked(sampled.allocated.reallocations, sampled.repeatable),
            marked(sampled.allocated.bytes, sampled.repeatable),
        );
    }

    // The same numbers again, one JSON document per line, between markers. A
    // baseline collector reads this block out of the run's stderr rather than
    // scraping the human table, so that an archived baseline records what the run
    // measured rather than what someone typed afterwards.
    eprintln!();
    eprintln!("#!d3-format-allocation-profile-begin");
    eprintln!(
        "{{\"kind\":\"defaults\",\"warm_up_time_ms\":{},\"measurement_time_ms\":{},\"note\":{}}}",
        WARM_UP_TIME.as_millis(),
        MEASUREMENT_TIME.as_millis(),
        // No apostrophe: PowerShell 5.1's JSON writer escapes one as \u0027, and
        // the archive is meant to be read by people as well as by tools.
        json_string(
            "the criterion warm-up and measurement windows this build sets before \
             configure_from_args; --warm-up-time and --measurement-time still override them"
        ),
    );
    for entry in &profiled {
        eprintln!(
            "{{\"kind\":\"output\",\"group\":{},\"case\":{},\"locale\":{},\"specifier\":{},\
             \"input\":{},\"output_bytes\":{},\"reserved_bytes\":{},\"owned\":{},\"reused\":{}}}",
            json_string(entry.group),
            json_string(entry.case.id),
            json_string(match entry.case.locale {
                Which::EnUs => "en-US",
                Which::ArabicNumerals => "arabic-numerals",
            }),
            json_string(entry.case.spec),
            json_string(&entry.case.input_label()),
            entry.output_bytes,
            entry.reserved,
            json_sample(entry.owned),
            json_sample(entry.reused),
        );
    }
    for (construction, sampled) in &constructed {
        eprintln!(
            "{{\"kind\":\"construction\",\"group\":\"construction\",\"case\":{},\"detail\":{},\
             \"once\":{}}}",
            json_string(construction.id),
            json_string(construction.detail),
            json_sample(*sampled),
        );
    }
    eprintln!("#!d3-format-allocation-profile-end");
    eprintln!();
}

/// A count, with `?` appended when the repeat call disagreed.
fn marked(value: u64, repeatable: bool) -> String {
    if repeatable {
        value.to_string()
    } else {
        format!("{value}?")
    }
}

// ---------------------------------------------------------------------------
// The timing groups
// ---------------------------------------------------------------------------

fn bench_owned(group: &mut BenchmarkGroup<'_, WallTime>, id: &str, case: &Case) {
    let formatter = case.formatter();
    match case.input {
        Input::Number(value) => {
            group.bench_function(id, |bencher| {
                bencher.iter(|| formatter.format_number(black_box(value)).unwrap());
            });
        }
        Input::Text(value) => {
            group.bench_function(id, |bencher| {
                bencher.iter(|| formatter.format_text(black_box(value)).unwrap());
            });
        }
    }
}

fn bench_reused(group: &mut BenchmarkGroup<'_, WallTime>, id: &str, case: &Case) {
    let formatter = case.formatter();

    // Outside the timed loop, and outside `bencher.iter`, so that not even the
    // first iteration of the first sample pays for the buffer. Reserving the exact
    // output length rather than a round number keeps the comparison against the
    // owned group honest: both paths end up with the same number of bytes, and the
    // only difference left is who allocated them and when.
    let mut out = String::new();
    out.reserve(case.render(&formatter).len());
    let reserved = out.capacity();

    // The buffer is made opaque once, outside `iter`, and not on every iteration.
    // It has to be made opaque at all: the loop never reads `out` afterwards, so
    // the stores into a heap block that is only ever freed are dead code the
    // optimizer is entitled to remove. Doing it once is what keeps the comparison
    // fair, though. The owned path carries no `black_box` of its own -- Criterion
    // already applies one to the value `iter` returns -- so a per-iteration
    // `black_box(&mut out)` would put a barrier in one arm of a two-arm comparison
    // and charge the difference to buffer reuse.
    match case.input {
        Input::Number(value) => {
            group.bench_function(id, |bencher| {
                let buffer = black_box(&mut out);
                bencher.iter(|| {
                    formatter
                        .format_number_into(buffer, black_box(value))
                        .unwrap();
                });
            });
        }
        Input::Text(value) => {
            group.bench_function(id, |bencher| {
                let buffer = black_box(&mut out);
                bencher.iter(|| {
                    formatter
                        .format_text_into(buffer, black_box(value))
                        .unwrap();
                });
            });
        }
    }

    // A `String`'s capacity moves if and only if it reallocated, so this fails the
    // run if any of the millions of iterations just timed grew the buffer -- which
    // is the failure that would quietly turn this group back into the owned one.
    assert_eq!(
        out.capacity(),
        reserved,
        "case `{id}`: the reused buffer grew during the timed loop"
    );
}

/// `Formatter::format_number`, returning a fresh `String` each call.
fn owned_output(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("owned_output");
    for case in REPRESENTATIVE {
        bench_owned(&mut group, case.id, case);
    }
    group.finish();
}

/// The same cases under the same ids through `Formatter::format_number_into`.
fn reused_output(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("reused_output");
    for case in REPRESENTATIVE {
        bench_reused(&mut group, case.id, case);
    }
    group.finish();
}

/// The exact-conversion worst case.
///
/// Owned output only. The buffer-ownership difference is a property of the
/// buffer, not of the value, and it is already measured case for case by the two
/// groups above and exactly by the allocation profile; what is distinctive here is
/// the conversion, and repeating each case twice would double the group's runtime
/// to restate a difference that is a few tens of nanoseconds against a subnormal's
/// microseconds.
fn subnormal(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("subnormal");
    for case in SUBNORMAL {
        bench_owned(&mut group, case.id, case);
    }
    group.finish();
}

/// The longest output the port can produce.
///
/// `binary_f64_max_into` is the one reused-path case here, because a kilobyte of
/// output is where owning the buffer could plausibly cost something a caller would
/// notice, and a claim about that is worth a measurement rather than an argument.
fn max_radix(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("max_radix");
    for case in MAX_RADIX {
        bench_owned(&mut group, case.id, case);
    }
    bench_reused(&mut group, "binary_f64_max_into", &MAX_RADIX[0]);
    group.finish();
}

/// Parsing and compiling, which a caller pays once.
fn construction(criterion: &mut Criterion) {
    let locale = locale(Which::EnUs);
    let specifier: FormatSpecifier = CONSTRUCTION_SPEC
        .parse()
        .expect("the construction specifier must parse");

    let mut group = criterion.benchmark_group("construction");
    for case in CONSTRUCTIONS {
        group.bench_function(case.id, |bencher| {
            bencher.iter(|| (case.run)(&locale, &specifier));
        });
    }
    group.finish();
}

/// Layout without much conversion behind it: type `c`, and numeral substitution.
fn text_and_locale(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("text_and_locale");
    for case in LAYOUT {
        bench_owned(&mut group, case.id, case);
        bench_reused(&mut group, &format!("{}_into", case.id), case);
    }
    group.finish();
}

/// Section 5.5's suggestions. They allocate nothing and return `f64`, so they are
/// here to establish that scale, not because anything suspects them.
fn precision_helpers(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("precision_helpers");
    group.bench_function("precision_fixed", |bencher| {
        bencher.iter(|| precision_fixed(black_box(0.01)));
    });
    group.bench_function("precision_round", |bencher| {
        bencher.iter(|| precision_round(black_box(0.01), black_box(1.01)));
    });
    group.bench_function("precision_prefix", |bencher| {
        bencher.iter(|| precision_prefix(black_box(1e-6), black_box(1.3e-3)));
    });
    group.finish();
}

/// The harness.
///
/// `harness = false` in `Cargo.toml` means this is the whole entry point, and
/// `criterion_main!` would expand to exactly the three statements after the
/// profile call with nothing in front of them. Writing it out is what makes room
/// for the profile, and it is also where the two timing defaults are set: on the
/// `Criterion` value before `configure_from_args`, which only overwrites a setting
/// when the corresponding argument is actually present, so `--measurement-time`
/// still wins.
fn main() {
    report_allocation_profile();

    let mut criterion = Criterion::default()
        .warm_up_time(WARM_UP_TIME)
        .measurement_time(MEASUREMENT_TIME)
        .configure_from_args();

    owned_output(&mut criterion);
    reused_output(&mut criterion);
    subnormal(&mut criterion);
    max_radix(&mut criterion);
    construction(&mut criterion);
    text_and_locale(&mut criterion);
    precision_helpers(&mut criterion);

    criterion.final_summary();
}
