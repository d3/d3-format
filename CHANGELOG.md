# Changelog

All notable changes to the `d3-format` Rust crate. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this crate follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) from `0.1.0` onwards.

The version here is the Rust API's own. It does not track d3-format's: the *outputs* target
d3-format 3.1.2, and `COMPATIBILITY.md` records the exact commit and oracle they were measured
against.

## [Unreleased]

## [0.1.0-rc.1] — unreleased

First release candidate. Not yet published to crates.io: publication is gated on a package-name
recheck and on credentials being configured for the protected release environment.

### Added

- `Locale`, an immutable `Arc`-backed locale value, with `Locale::en_us`, `Locale::builder`,
  `Locale::from_definition` and accessors for every resolved field. There is no mutable
  process-global default locale.
- `Formatter`, compiled from a locale and a specifier, with `format_number`, `format_text` and the
  `*_into` variants that write into a caller-owned `String`. Formatters own a cheap clone of their
  locale, so they carry no lifetime parameter and are `Send + Sync`.
- `Locale::formatter`, `formatter_for`, `prefix_formatter` and their `*_with_limits` counterparts.
  `formatter_for` is implemented as `formatter(&spec.to_string())`, so it cannot skip
  normalization.
- `FormatSpecifier`, an immutable validated value with typed fields, `FromStr` over d3's grammar
  and `Display` over its canonical spelling, plus `FormatSpecifierBuilder`.
- `Align`, `Sign`, `Symbol`, `FormatType` and `InputKind`, each knowing its own d3 spelling.
- `LocaleDefinition` and `LocaleBuilder`, with d3's defaults for absent keys and construction-time
  validation of the grouping and numeral shapes.
- `precision_fixed`, `precision_round` and `precision_prefix`, returning `f64` because d3 returns
  NaN for a zero or non-finite step, plus `as_precision` for the conversion to `u32`.
- `FormatLimits` and the typed `FormatError`, `ParseError`, `ParseErrorKind` and `LocaleError`.
  Every formatting entry point is fallible; no accepted input can panic, wrap, or attempt an
  unbounded allocation.
- `locales` feature: `Locale::named` and the 59 locale definitions generated from d3's own locale
  JSON, with the source SHA-256 of each file recorded alongside it.
- `serde` feature: `Serialize` and `Deserialize` for `LocaleDefinition`, which round-trips the
  bundled locale JSON.

### Compatibility

- Byte-for-byte with d3-format 3.1.2 at commit `ebdc2d5`, measured against Node v24.18.0 as the
  oracle. See `COMPATIBILITY.md` for the corpora and their counts, and for what is not claimed.
- Nine intentional deviations, all about the interface rather than the digits, are documented in
  `DIVERGENCES.md`. Each names the test that demonstrates it.
- Minimum supported Rust version 1.80.0. Raising it is a minor-version change.
- `std` is required; `no_std` is not supported. The default build has no dependencies.

### Known gaps

- Every gate has been reproduced on one `x86_64-pc-windows-gnu` host and nowhere else. The crate
  has never been built for Linux, and no continuous integration has ever run against it.
- Publication has never been exercised. Nothing has been pushed to crates.io.

[Unreleased]: https://github.com/alexkhil/d3-format-rust/compare/v0.1.0-rc.1...HEAD
[0.1.0-rc.1]: https://github.com/alexkhil/d3-format-rust/releases/tag/v0.1.0-rc.1
