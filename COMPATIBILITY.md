# Compatibility

What the `d3-format` Rust crate claims about its agreement with d3-format, what evidence backs the
claim, and — at least as importantly — what it does not claim.

## The claim

**For every specifier the grammar accepts, every locale the model accepts and every `f64` bit
pattern, this crate produces the same bytes as d3-format 3.1.2, except for the deviations
enumerated in [`DIVERGENCES.md`](DIVERGENCES.md).**

"The same bytes" is literal. Strings are compared exactly, including the Unicode minus sign
U+2212, the micro sign U+00B5, right-to-left marks, `∞` for type `d` infinity, the lowercase `0x`
prefix an uppercase `X` takes, and the ordering that makes numeral substitution visible in the base
prefix but not in the hexadecimal digits. Numeric results — the three precision suggestions — are
compared by `f64` bit pattern, so the sign and payload of a NaN are part of the contract.

The nine deviations are all about the *interface*: what happens to inputs d3 coerces, to fields d3
lets you mutate, and to widths d3 leaves to the engine. None of them changes the string that any
input d3 can express is formatted to.

## Where the evidence lives

The measurements below were made in the repository the port was developed in, a fork of the
JavaScript `d3-format` repository that carries d3's own source, its test suite, its 59 locale JSON
files, and the oracle harness. Comparing against d3 requires running d3, so that apparatus cannot
travel with a Rust crate.

This repository is the crate extracted from that tree. Paths in this document that name
`fixtures/`, `tools/`, `ci/` or `canaries/` refer to the migration repository; everything naming
`src/`, `tests/` or `benches/` is here. The recorded oracle values themselves *are* here — the
generated tests carry every expected value inline rather than reading a corpus at run time — so
`cargo test` re-checks 666 replayed d3 assertions locally, without Node and without a fixture file.
What cannot be re-run here is the part that needs d3 itself: harvesting the oracle, regenerating
the fixtures, and the differentials.

[`docs/MIGRATION_TO_RUST.md`](docs/MIGRATION_TO_RUST.md) describes that repository in full.

## The reference

| | |
|---|---|
| d3-format version | 3.1.2 |
| d3 source commit | `ebdc2d530277df379157f82fee6ea5623d179bd7` |
| Node | v24.18.0 |
| V8 | 13.6.233.17-node.50 |
| ICU | 78.3 |
| Unicode | 17.0 |
| CLDR | 48.0 |

Recorded in `fixtures/d3-format-3.1.2/oracle.json`, which is not part of the published crate.
`tools/check-oracle-metadata.mjs` compares every field to the running process and verifies the
oracle source hashes, so a differential run against a different engine fails at the first step
rather than reporting the engine change as several million port defects.

The engine versions are load-bearing rather than decorative. d3's type `d` reaches
`toLocaleString("en")` for magnitudes at or above `1e21`, which is an ICU code path, and shortest
round-tripping decimal conversion is V8's. Agreement is claimed against this engine. It is expected
to hold for others, because the behaviors relied on are specified, but it has not been measured
against any other, and updating the oracle is a reviewed compatibility event rather than a
refresh.

The d3 source, tests, locale JSON, package manifest and lockfile in the repository the fixtures
were generated from are byte-identical to upstream `ebdc2d5`.

## Artifact schemas and generator versions

| Artifact | Schema | Generator |
|---|---|---|
| `fixtures/d3-format-3.1.2/manifest.json` | `d3-format-rust/fixtures@1` | `tools/harvest.mjs` 1.0.0 |
| `fixtures/d3-format-3.1.2/corpus-manifest.json` | `d3-format-rust/corpus-manifest@1` | `tools/gen-corpus.mjs` 1.0.0 |
| differential run summary | `d3-format-rust/differential-summary@1` | `tools/differential.mjs` |
| `ci/parity/*-differential.json` | `d3-format/differential-archive/1` | `ci/parity/collect-differential.ps1` |
| `ci/benchmarks/baseline.json` | `d3-format/benchmark-baseline/1` | `ci/benchmarks/collect-baseline.ps1` |
| `ci/package/package-listing.json` | `d3-format/package-listing/1` | `ci/package/collect-listing.ps1` |

The fixture manifest carries the SHA-256 of every source test file, every locale JSON file, every
fixture and every generated Rust file. Two consecutive clean generations must produce identical
bytes; that is an executable gate, not a convention.

None of these files is part of the published crate. The `exclude` list in `Cargo.toml` keeps
verification tooling out of the distribution; they live in the repository.

Each of the three `ci/` archives carries a `provenance` block naming the commit whose tree it
measured, and its collector refuses to run against a working tree that does not match that commit —
so `git_commit` names something a reader can check out and reproduce, and `git_worktree_dirty` is
false. The artifact recording a measurement necessarily lands in a commit *after* the one it names;
that ordering is expected. `-AllowDirty` waives the refusal for local experimentation and says so in
`provenance.allow_dirty`, which marks the result as not being release evidence.

## The evidence

### The recorded oracle suite

d3's own test suite, executed under the pinned Node and recorded at the public boundary.

| | |
|---|---|
| Test files | 24 |
| Test blocks | 168 |
| Executable static assertion sites | 729 |
| Dynamic assertion executions | 1,078 |
| Recorded boundary calls | 793 |
| — by operation | 624 `format`, 133 `precisionPrefix`, 11 `formatPrefix`, 10 `precisionFixed`, 8 `precisionRound`, 7 `formatSpecifier` |
| Distinct specifier inputs | 194 (181 distinct canonical spellings) |
| Distinct locale definitions exercised | 49 |
| Bundled locale files | 59, of which 23 carry numerals |

All 729 static sites are accounted for: 666 replayed by a generated Rust test, 35 hand-ported, and
28 classified as divergent and answered by a named API test. 786 of the 793 boundary calls are
claimed by an assertion site and the remaining 7 are made in a test block's setup position, which
is a structural property of the suite rather than a waiver. `tools/check-coverage.mjs` re-parses
the JavaScript with its own Acorn pass and refuses the mapping unless the sources, the fixtures,
the Rust tree and `DIVERGENCES.md` all independently agree.

### The structured corpora

Random values do not find rounding-rule errors: a half-even simulation passes all 1,078 recorded
assertions. These corpora are what establishes the rounding rule instead. Counts are from
`fixtures/d3-format-3.1.2/corpus-manifest.json`.

| Section | Cases | What it is |
|---|---|---|
| `calls` | 793 | every recorded d3 boundary call, replayed |
| `decimal` | 10,042 | recorded ECMAScript conversions |
| `boundary` | 2,949 | 40 threshold values — `1e21` and its neighbours, `1e±6`, both zeros, subnormals, `f64::MAX`, both NaN signs, both infinities — through every operation |
| `fixed_ties` | 25,830 | exact `toFixed` half-way cases, proved by rational arithmetic, at every precision 0–20 |
| `significant_ties` | 49,740 | exact significant-digit half-way cases at every precision 1–21, of which 4,319 are values where half-even and ECMAScript produce literally different strings |
| `carry` | 266,478 | runs of nines around decimal-point carries, exponent increments, notation switches and SI bucket boundaries |
| `sweep` | 2,000,000 | deterministic finite bit patterns |
| `regressions` | 0 | minimized cases pinned by a past differential failure; empty because none has been needed |

A generator producing zero cases for a declared bucket is a gate failure, not a pass.

### The differentials

Both runs compare this crate's output with d3's, case by case, and both are archived under
`ci/parity/`.

| | per-PR (`ci`) | pre-release (`prerelease`) |
|---|---|---|
| Cases compared | 213,784 | 13,355,832 |
| Disagreements | 0 | 0 |
| Formatter cases | 200,824 | 11,000,824 |
| — output comparisons | 187,636 | 10,268,703 |
| — declined by input policy | 13,188 | 732,121 |
| Named locales drawn from | 59 | 59 |
| Per-locale floor | 1,694, applied; least `nl-NL` 3,254 | 93,220, applied; least `fi-FI` 185,260 |
| Cargo profile | dev | release |
| Where it runs | the `parity` job, every pull request | scheduled weekly |

A drawn case is not always an output comparison: roughly one drawn specifier in fifteen is type
`c`, which takes text, so a numeric case against it is an input-policy check rather than a string
comparison. The two numbers are reported separately for that reason, and the pre-release run
asserts a floor on the *comparison* count so that a corpus change cannot quietly convert
comparisons into declines.

The per-PR run uses the debug profile deliberately: Rust's integer overflow checks and debug
assertions stay live, which is the stronger reading of "no accepted input panics". The pre-release
run trades them for the throughput that makes ten million comparisons affordable.

### Property and hardening tests

- 17 property tests, including 525,121 values pushed through 15,913 accepted specifiers with no
  panic and no error.
- The parser checked against an independent implementation of d3's grammar, written against UTF-16
  code units, over 62,823 inputs.
- Owned-output and reused-output equivalence, trimming, padding, all-locale and allocation-budget
  properties.
- A concurrency suite whose specific target is d3's `prefixExponent` module global: mixed SI
  buckets formatted from eight threads at once, one `&Formatter` shared across threads, and a
  locale moved off the thread that built it.

### The packaged crate itself

Everything above is measured against the repository. Two downstream sample applications are built
against the *unpacked `.crate`* instead, so that the artifact a consumer downloads is what gets
exercised: one on default features and one on `locales` + `serde`, each run on both the pinned
toolchain and the declared MSRV. Between them they format about 1.14 million values, anchored to
oracle-recorded answers, and they cover the multithreaded and locale-heavy workloads. They live in
`canaries/` in the repository and are not part of the published crate.

The contents of the package are reviewed by rule rather than by reading: every one of the 56
packaged paths must match a rule in `ci/package/collect-listing.ps1` that states why a consumer
needs it, and every entry in the `exclude` list is re-asserted by name. The listing, with a
per-file SHA-256, is archived in `ci/package/package-listing.json`.

## Toolchains, targets and features

| | |
|---|---|
| Minimum supported Rust version | 1.80.0 |
| Pinned validation toolchain | 1.98.1 |
| Also validated on | current stable |
| Edition | 2021 |

Raising the MSRV is a minor-version change. As of this release current stable *is* 1.98.1, so
building on stable builds the same compiler as the pin and adds no independent coverage yet; only
1.80.0 currently exercises a genuinely different compiler.

| Target | Status |
|---|---|
| `x86_64-pc-windows-gnu` | full test suite, all feature combinations, benchmarks |
| `x86_64-unknown-linux-gnu` | **never built** — no Linux host was available |
| `wasm32-unknown-unknown` | `cargo check --no-default-features` only; no JavaScript boundary is published |

Every feature combination is tested on every toolchain: no default features, `locales`, `serde`,
and all features. Features are additive and none of them changes a formatting result.

## What is not claimed

1. **Nothing outside d3-format 3.1.2 at `ebdc2d5`.** No claim about other d3 versions, and no
   claim about the npm package's JavaScript-level API, which is unaffected by this crate and
   remains the reference implementation.
2. **Nothing about other engines.** Agreement was measured against Node v24.18.0 with the V8, ICU,
   Unicode and CLDR versions above. Another engine is expected to agree and has not been checked.
3. **Nothing about radices other than 2, 8 and 16.** `decimal::round_to_string_radix` produces the
   exact expansion at every radix from 2 to 36; V8 approximates outside the power-of-two radices.
   d3 can only reach 2, 8 and 16, so no d3-compatible output is affected, and the differential
   *declines* the other radices rather than asserting an agreement nobody has demonstrated. See
   `exact-radix-expansion` in [`DIVERGENCES.md`](DIVERGENCES.md).
4. **Nothing about JavaScript coercion.** A numeric formatter takes `f64` and a type `c` formatter
   takes `&str`; the calls that pass `undefined`, `null` or an arbitrary object to d3 cannot be
   written, so this crate does not reproduce their results. See `no-input-coercion`.
5. **Nothing about the specifier round trip above 2³¹.** d3 spells a width as
   `Math.max(1, this.width | 0)`, so `formatSpecifier("4294967295f").toString()` wraps to
   `" >-1f"`; this crate renders what it parsed. No *formatted output* differs, because both clamp
   before use. See `immutable-typed-specifier`.
6. **No hosted-CI evidence.** Every gate has been reproduced on one `x86_64-pc-windows-gnu` host
   and nowhere else. The migration repository's `ubuntu-latest` and `windows-latest` jobs, the
   assumption that the hosted Windows image ships MinGW `dlltool`, and the GitHub Actions plumbing
   itself have never run, and one host is not evidence about a hosted image. This repository ships
   no CI configuration at all.
7. **No allocation-freedom, and no performance claim.** Every numeric path allocates. The archived
   benchmark baseline in `ci/benchmarks/baseline.json` is one host's observation with allocator
   instrumentation, explicitly not a regression gate, and no CI job reads it. Its allocation counts
   are exact `GlobalAlloc` call counts for one build; they say nothing about peak resident memory,
   fragmentation, or another allocator or optimization level.
8. **No stability for `d3_format::decimal`.** That module is public only because `tests/decimal.rs`
   is an integration test and cannot see crate-private items. It is `#[doc(hidden)]`, carries no
   semver guarantee, and may be sealed or changed at any time.
9. **No semver stability yet.** `0.1.0-rc.1` is a release candidate. The API may change before
   `0.1.0`.
10. **No `no_std`, and no WASM API.** `std` is required. The crate compiles for
    `wasm32-unknown-unknown` so that a future boundary crate stays possible; that compile check is
    not a claim that a usable JavaScript API exists.

## Reproducing any of this

### Here

The oracle's answers are compiled into the test suite, so most of the agreement evidence re-runs
with nothing but a Rust toolchain:

```console
$ cargo test --all-features   # 666 replayed d3 assertions, 11 hand-ported, the property suite
$ cargo bench                 # timings, and the exact allocation profile
```

That covers the recorded oracle suite, the hand-ported assertions, the divergence contracts, the
property and concurrency tests, and the benchmark instrumentation.

### In the migration repository

The differentials, the fixture regeneration and the coverage audit all execute d3, so they need
that tree, its pinned Node, and its JavaScript dependencies:

```powershell
node rust/tools/check-oracle-metadata.mjs       # the engine is still the recorded oracle
node rust/tools/harvest.mjs --check             # the fixtures are what a fresh recording produces
node rust/tools/check-coverage.mjs              # 729/729 sites, and this document is not stale
node rust/tools/gen-tests.mjs --check           # the generated tests are byte-identical
node rust/tools/gen-locales.mjs --check         # the generated locale table is byte-identical
node rust/tools/gen-corpus.mjs --check-manifest # the structured corpora still fill every bucket

./rust/ci/check.ps1 -Profile Pinned             # fmt, clippy, four feature combinations, package
node rust/tools/differential.mjs --corpus ci --seed 1 --cases 200000

pwsh rust/ci/package/collect-listing.ps1        # what ships, and why; clean tree only
pwsh canaries/run-canaries.ps1                  # the packaged crate, from a consumer's side
```

A single disagreeing case can be reproduced on its own with the `oracle-diff.mjs` command the
differential runner prints.
