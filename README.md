# d3-format

A Rust implementation of [d3-format](https://github.com/d3/d3-format) 3.1.2.

d3's format mini-language, its 59 locales and its ECMAScript number conversion, reproduced byte
for byte. The default build has no dependencies.

```toml
[dependencies]
d3-format = "0.1.0-rc.1"
```

Every Rust snippet below is a doctest compiled and run by this crate's test suite.

```rust
use d3_format::Locale;

let locale = Locale::en_us();
let money = locale.formatter("$,.2f").expect("a valid specifier");

assert_eq!(money.format_number(1234.5).expect("in range"), "$1,234.50");

// The minus sign is the locale's, and d3's default is U+2212, not the ASCII hyphen.
assert_eq!(money.format_number(-1234.5).expect("in range"), "\u{2212}$1,234.50");
```

A `Formatter` is compiled once and reused. Nothing about it is mutable, so it can be stored,
cloned and shared between threads.

```rust
use d3_format::Locale;

let locale = Locale::en_us();
let cases = [
    (".1%", 0.123, "12.3%"),
    ("+.2e", 1234.5, "+1.23e+3"),
    (".3s", 1.3e6, "1.30M"),
    (",d", 1234567.0, "1,234,567"),
    ("08.2f", -3.5, "\u{2212}0003.50"),
    ("#x", 48879.0, "0xbeef"),
];

for (specifier, value, expected) in cases {
    let formatter = locale.formatter(specifier).expect("a valid specifier");
    assert_eq!(formatter.format_number(value).expect("in range"), expected);
}
```

## The mini-language

The grammar is d3's, unchanged:

```text
[[fill]align][sign][symbol][0][width][,][.precision][~][type]
```

| Field | Values | Meaning |
|---|---|---|
| `fill` | any single character | what padding is made of; requires an `align` after it |
| `align` | `<` `>` `^` `=` | left, right, centred, or between the sign and the digits |
| `sign` | `-` `+` `(` ` ` | minus only, always signed, parenthesized negatives, space for positives |
| `symbol` | `$` `#` | the locale's currency affixes, or a `0b`/`0o`/`0x` base prefix |
| `0` | | zero-pad; the same as fill `0` with align `=` |
| `width` | digits | minimum width, counted in UTF-16 code units |
| `,` | | group the integer digits with the locale's separator |
| `.precision` | digits | significant digits for `g`, `p`, `r` and `s`; decimal places otherwise |
| `~` | | trim insignificant trailing zeros |
| `type` | see below | the conversion |

| Type | Conversion |
|---|---|
| `e` | exponent notation |
| `f` | fixed point |
| `g` | significant digits, in fixed or exponent notation |
| `r` | significant digits, never in exponent notation |
| `s` | an SI prefix chosen from the value |
| `%` | multiplied by 100, with the locale's percent sign |
| `p` | a percentage rounded to significant digits |
| `b`, `o`, `x`, `X` | binary, octal, lowercase and uppercase hexadecimal, after rounding |
| `d` | decimal integer, after rounding |
| `c` | character data, emitted verbatim; the only type that takes text |
| *(empty)* | as `.12~g`, rendering as nothing |
| any other letter | as `.12~g`, keeping the letter |

## Locales

A `Locale` is an immutable value that is cheap to clone. There is no mutable process-global
default locale to rebind, so one library cannot change another library's formatting.

```rust
use d3_format::Locale;

let euro = Locale::builder()
    .decimal(",")
    .thousands(".")
    .grouping([3])
    .currency("", "\u{a0}\u{20ac}")
    .build()
    .expect("a valid definition");

let money = euro.formatter("$,.2f").expect("a valid specifier");
assert_eq!(money.format_number(1234.5).expect("in range"), "1.234,50\u{a0}\u{20ac}");
```

Grouping *cycles* through its sizes rather than repeating the last one, which is easy to get
backwards: `[3, 2]` is not the Indian grouping.

```rust
use d3_format::Locale;

let cycling = Locale::builder()
    .thousands(",")
    .grouping([3, 2])
    .build()
    .expect("a valid definition");

let grouped = cycling.formatter(",d").expect("a valid specifier");
assert_eq!(grouped.format_number(1234567890.0).expect("in range"), "12,345,67,890");
```

The `locales` feature adds `Locale::named` and the 59 definitions generated from d3's own locale
JSON — including the one that gets the Indian grouping right.

```rust
#[cfg(feature = "locales")]
{
    use d3_format::Locale;

    let india = Locale::named("en-IN").expect("a bundled locale");
    let grouped = india.formatter(",d").expect("a valid specifier");
    assert_eq!(grouped.format_number(1234567890.0).expect("in range"), "1,23,45,67,890");

    // Numeral substitution runs last, over the fully assembled string. That ordering is
    // observable: the `0` of the `0x` base prefix is substituted, the hexadecimal digits are not.
    let arabic = Locale::named("ar-001").expect("a bundled locale");
    let hex = arabic.formatter("#x").expect("a valid specifier");
    assert_eq!(hex.format_number(48879.0).expect("in range"), "٠xbeef");
}
```

## The specifier is a value

`FormatSpecifier` parses the grammar into an immutable, validated value with typed fields.
`Display` emits the canonical spelling, and `Locale::formatter_for(&spec)` is exactly
`Locale::formatter(&spec.to_string())`, so the round trip cannot skip normalization.

```rust
use d3_format::{FormatSpecifier, FormatType, Locale, Symbol};

let specifier: FormatSpecifier = "$,.2f".parse().expect("a valid specifier");
assert_eq!(specifier.symbol(), Symbol::Currency);
assert_eq!(specifier.format_type(), FormatType::Fixed);
assert_eq!(specifier.precision(), Some(2));
assert_eq!(specifier.to_string(), " >-$,.2f");

// A specifier is changed by building a new one; there is no field to assign to.
let wider = specifier.to_builder().precision(4u32).build().expect("still valid");
let money = Locale::en_us().formatter_for(&wider).expect("a valid specifier");
assert_eq!(money.format_number(1234.5).expect("in range"), "$1,234.5000");
```

## Precision suggestions

`precision_fixed`, `precision_round` and `precision_prefix` answer in `f64`, because d3 returns
NaN for a zero or non-finite step and its own tests assert that. `as_precision` is the step from
there to the `u32` a specifier takes; it returns `None` rather than substituting a zero.

```rust
use d3_format::{as_precision, precision_prefix, Locale};

let (step, value) = (1e-6, 1.3e-3);
let digits = as_precision(precision_prefix(step, value)).expect("a finite step has a precision");
assert_eq!(digits, 3);

let formatter = Locale::en_us()
    .formatter(&format!(".{digits}s"))
    .expect("a valid specifier");
assert_eq!(formatter.format_number(value).expect("in range"), "1.30m");

assert!(precision_prefix(0.0, 1.0).is_nan());
assert_eq!(as_precision(f64::NAN), None);
```

`Locale::prefix_formatter` is d3's `formatPrefix`: the SI prefix is chosen once from a reference
value instead of per value, so a whole column of numbers shares one unit.

```rust
use d3_format::Locale;

let micro = Locale::en_us()
    .prefix_formatter(",.0", 1e-6)
    .expect("a valid specifier");

assert_eq!(micro.format_number(0.00042).expect("in range"), "420\u{b5}");
assert_eq!(micro.format_number(0.0042).expect("in range"), "4,200\u{b5}");
```

## Formatting is fallible

Every entry point returns a `Result`. d3 coerces any value to a number and leaves it to the engine
to decide what a trillion-character pad does; this crate refuses both at a typed, recoverable
point.

```rust
use d3_format::{FormatError, FormatLimits, InputKind, Locale};

let locale = Locale::en_us();

// Not in the grammar. The message is d3's, exactly.
let error = locale.formatter("foo").expect_err("not a specifier");
assert_eq!(error.to_string(), "invalid format: foo");

// The wrong kind of input, where d3 would coerce. Only type `c` takes text.
let numeric = locale.formatter("d").expect("a valid specifier");
assert_eq!(
    numeric.format_text("nope"),
    Err(FormatError::InputTypeMismatch {
        expected: InputKind::Number,
        actual: InputKind::Text,
    }),
);

// A width beyond the limits is refused at construction, so a formatter that exists cannot later
// fail to pad. The default ceiling is a million UTF-16 code units.
let limits = FormatLimits { max_width_utf16: 64, ..FormatLimits::default() };
assert_eq!(
    locale.formatter_with_limits("1000d", limits),
    Err(FormatError::WidthLimit { requested: 1000, maximum: 64 }),
);
```

`format_number_into` and `format_text_into` write into a caller-owned `String`, clearing it first
and reusing its capacity. They are not allocation-free: exact decimal conversion, grouping and
numeral substitution all build buffers of their own.

```rust
use d3_format::Locale;

let formatter = Locale::en_us().formatter(",.2f").expect("a valid specifier");
let mut out = String::with_capacity(32);

for value in [1234.5, 6789.0] {
    formatter.format_number_into(&mut out, value).expect("in range");
}
assert_eq!(out, "6,789.00");
```

## Threads

`Locale`, `FormatSpecifier` and `Formatter` are `Send + Sync`. There is no mutable global default
locale, no shared scratch buffer, no result cache, and no SI exponent left over from the previous
call — d3 keeps the last of those in a module-level variable, and reproducing it would have been a
data race.

```rust
use std::thread;

use d3_format::Locale;

let formatter = Locale::en_us().formatter(".3s").expect("a valid specifier");
let workers: Vec<_> = [1.3e-6, 1.3e6, 1.3e12]
    .map(|value| {
        let formatter = formatter.clone();
        thread::spawn(move || formatter.format_number(value).expect("in range"))
    })
    .into_iter()
    .collect();

let formatted: Vec<String> = workers.into_iter().map(|w| w.join().expect("no panic")).collect();
assert_eq!(formatted, ["1.30\u{b5}", "1.30M", "1.30T"]);
```

## Features

| Feature | Default | Effect |
|---|---|---|
| `locales` | no | `Locale::named` and the 59 built-in definitions |
| `serde` | no | `Serialize`/`Deserialize` for `LocaleDefinition`, the stable wire shape |

Features are additive and none of them changes a formatting result.

## Compatibility

Minimum supported Rust version 1.80.0, validated on pinned 1.98.1 and on current stable. `std` is
required; `no_std` is not supported. The crate compiles for `wasm32-unknown-unknown`, but this
package publishes no JavaScript boundary.

`0.1.0-rc.1` is the Rust API's own version and does not track d3's. The outputs target d3-format
3.1.2 at commit `ebdc2d5`.

- [`COMPATIBILITY.md`](COMPATIBILITY.md) — exactly what is claimed, the oracle it was measured
  against, the corpora that measured it, and what is *not* claimed.
- [`DIVERGENCES.md`](DIVERGENCES.md) — the nine intentional deviations, each naming the test that
  demonstrates it. All of them are about the interface rather than the digits.
- [`CHANGELOG.md`](CHANGELOG.md) — release notes.
- [`docs/MIGRATION_TO_RUST.md`](docs/MIGRATION_TO_RUST.md) — the plan the port was executed
  against, kept as the design record. Comments throughout the source cite its section numbers for
  why a decision was made.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how the oracle-derived parts of this crate are produced,
  and what a change to them requires.

## License

ISC, the same as d3-format. See
[LICENSE](https://github.com/alexkhil/d3-format-rust/blob/main/LICENSE).
