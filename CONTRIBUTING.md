# Contributing

Thanks for looking. This crate has one unusual property that shapes almost everything below: it is
not merely *inspired by* d3-format, it is verified byte-for-byte against it, and a large part of
the test suite was generated from recordings of d3 actually running. Knowing which parts those are
will save you from a confusing afternoon.

## Getting set up

```console
$ cargo test --all-features
$ cargo bench
```

That is the whole toolchain. The default build has no dependencies, and the only dev-dependency is
Criterion. `rust-toolchain.toml` pins 1.98.1; the minimum supported version is 1.80.0 and CI for
that is your own `rustup run 1.80.0 cargo test`.

Before opening a pull request:

```console
$ cargo fmt --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test --no-default-features
$ cargo test --features locales
$ cargo test --features serde
$ cargo test --all-features
```

Features are additive and must never change a formatting result, which is why every combination is
tested rather than just the union.

## What you may not hand-edit

Two parts of the tree are generated from recorded oracle data:

- `tests/generated/` — 666 replayed d3 assertions, one Rust test per JavaScript assertion site,
  with every expected value carried inline exactly as d3 produced it.
- `src/locales/generated.rs` — the 59 bundled locale definitions, emitted from d3's own locale JSON
  with the source SHA-256 of each file recorded alongside it.

Both carry an `@generated` header. Editing them by hand is how a port silently stops matching its
oracle: the expected values in those files are *evidence*, and rewriting an expectation to match
the implementation converts a failing test into a false claim of agreement. If a generated test
fails, the implementation is wrong until proven otherwise.

The generators, the recorded corpora and the differential harness live in the repository this crate
was extracted from — a fork of the JavaScript `d3-format` repository, which carries d3's source and
test suite so that the oracle can actually be executed. See
[`docs/MIGRATION_TO_RUST.md`](docs/MIGRATION_TO_RUST.md) for how that tree is laid out and how each
artifact is produced.

The crate lives under `rust/` there and at the root here, so re-extracting a regenerated file means
rewriting `rust/tests/`, `rust/src/` and `rust/DIVERGENCES.md` in it to drop the prefix. The
`@generated` headers are left alone: they name the generator and the corpora it read, and those
really are at `rust/tools/` and `rust/fixtures/` in that repository.

## Changing formatting behaviour

Any change to what a formatter *outputs* needs differential evidence, not a passing test suite. The
test suite here can only tell you that you did not break a recorded case; it cannot tell you that
you agree with d3 on the millions of cases nobody recorded. That check runs in the migration
repository, against d3 itself.

If you have found a genuine disagreement with d3, the most useful thing you can put in an issue is
the triple that reproduces it: the specifier, the locale, and the input value as an `f64` bit
pattern. Bit patterns rather than decimal literals, because the whole point is which `f64` you
meant.

Two areas look like bugs and are not:

- **Rounding is half away from zero against the exact value, not half even.** This is what
  ECMAScript's `toFixed` does, and a half-even implementation passes all 1,078 recorded assertions
  while being wrong — that is why the structured tie corpora exist.
- **The nine entries in [`DIVERGENCES.md`](DIVERGENCES.md) are deliberate.** They are all about the
  interface: inputs d3 coerces, fields d3 lets you mutate, widths d3 leaves to the engine. None of
  them changes the string that any input d3 can express is formatted to. If you want to change one,
  that is a compatibility decision and belongs in an issue first.

## Style

Match the code around you. The house style here runs to fewer, longer comments that explain *why* a
constraint exists rather than what the next line does; comments citing "section N" refer to
[`docs/MIGRATION_TO_RUST.md`](docs/MIGRATION_TO_RUST.md).

`#![forbid(unsafe_code)]` is set at the crate root and is not up for negotiation. Clippy runs with
`-D warnings` over all targets and all features.

New dependencies are a hard sell. The default build having no dependencies is a feature, and the
benchmark harness is pinned to Criterion `=0.7.0` specifically because 0.8 pulls in a C build
dependency.

## Line endings

Everything in this repository is LF, enforced by `.gitattributes`. This is not a style preference:
`cargo package` reads the *working tree*, so a source file that drifted to CRLF changes the
published crate's SHA-256, and `git status` will not tell you — a file git itself checked out as
CRLF still matches the stat data cached in the index. `git ls-files --eol` is what shows the real
state.

## Reporting a compatibility bug

Open an issue with the specifier, the locale, the input as an `f64` bit pattern, what this crate
produced, and what d3-format 3.1.2 produces. A disagreement with d3 that is not listed in
`DIVERGENCES.md` is a bug in this crate by definition, however reasonable its output looks.
