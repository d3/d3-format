# Porting d3-format 3.1.2 to Rust

This is the implementation plan for a native Rust crate that reproduces d3-format 3.1.2. It is
also the compatibility contract for the port.

## Executive summary

The deliverable is a native Rust library in `rust/`, published as package `d3-format` with library
name `d3_format`. The existing JavaScript package remains unchanged, published, and in CI as the
reference implementation. Replacing the npm package with WASM is **not** part of this migration.

The difficult part is ECMAScript number conversion, not parsing the format mini-language. The port
must implement `toFixed`, `toExponential`, `toPrecision`, base-10 `Number::toString`, integer radix
conversion, SI selection, and locale layout with byte-for-byte d3 behavior. Rust's ordinary float
formatting is used only where a separately tested contract says it is equivalent.

No phase is complete merely because this document describes it. Status is defined by committed
artifacts and executable gates.

## 0. Scope, status, and definitions

### 0.1 Product scope

In scope:

- A native `std` Rust crate under `rust/`.
- An idiomatic, immutable `Locale` and compiled `Formatter` API.
- Byte-for-byte compatibility with d3-format 3.1.2 for the declared numeric API.
- Explicit, tested deviations where JavaScript coercion, mutable globals, or unsafe allocation
  behavior should not be copied.
- Generated built-in locales, deterministic conformance fixtures, differential tests, benchmarks,
  CI, publication, and rollback procedures.

Out of scope for this migration:

- Replacing `package.json` exports or the npm implementation with Rust or WASM.
- A stable JavaScript API backed by WASM.
- `no_std`.
- Arbitrary JavaScript value coercion.

The core should continue to compile for `wasm32-unknown-unknown` so a later boundary crate is
possible, but that compile check is not a claim that a usable JS/WASM API exists. A future WASM
adapter must own its locale and formatter state; `wasm-bindgen` cannot directly export borrowed
Rust values or lifetime-bearing APIs. See §10.

### 0.2 Status labels

- **Evidence — verified:** an exploratory command or probe was run against the current JavaScript
  checkout. This is useful design evidence, not a committed Rust deliverable.
- **Deliverable — present:** the named file exists in the repository and performs the stated job.
- **Gate — pending/passed:** a gate is passed only when its command succeeds against committed
  artifacts in a clean checkout.

Current status:

| Area | Status | Evidence |
|---|---|---|
| JavaScript inventory and edge probes | Evidence — verified | `node tools/harvest/harvest.mjs` and `.arena` probes |
| Runtime capture prototype | Deliverable — present | `tools/harvest/` records calls in memory and prints counts |
| JavaScript baseline gates | Gate — passed | `packageManager` pinned; `yarn --frozen-lockfile`, `yarn vitest run` (24 files, 168 tests), and `yarn lint` pass |
| Rust toolchain on this machine | Gate — passed | GNU host; 1.80.0, 1.98.1, and stable installed; requires MinGW binutils per §2.2 |
| Rust crate and gate tests | Gate — passed | `rust/` skeleton present; `tests/gate.rs` passes on all three toolchains |
| Phase 0 exit gate (§2.4) | Gate — passed | all four commands pass in this checkout; `yarn.lock` and `rust/Cargo.lock` unchanged |
| Deterministic fixture/test generation | Gate — pending | the prototype does not emit fixtures or Rust source |
| Locale generator and drift check | Gate — pending | not implemented |
| Differential runner | Gate — pending | not implemented |
| CI and Cargo publication | Gate — pending | existing workflows are JavaScript-only; `rust/ci/check.ps1` and the Rust jobs are not yet written |

### 0.3 Verified exploratory evidence

The following was re-run against this checkout:

- 24 test files, 168 `test(...)` blocks, 1,078 executed assertions, and zero JavaScript failures
  under the bare-Node shim.
- 735 textual `assert.*` occurrences; six are comments; therefore 729 executable static assertion
  sites.
- 793 captured public-boundary calls: 624 `format`, 11 `formatPrefix`, 7 `formatSpecifier`,
  10 `precisionFixed`, 133 `precisionPrefix`, and 8 `precisionRound`.
- 194 distinct format specifiers, 49 distinct locale definitions exercised, and 59 locale JSON
  files, 23 with `numerals`.
- A half-even simulation passes all 1,078 existing assertions, proving the fixed suite does not
  detect the most likely rounding error.
- A dyadic fixed-point sweep found a 5.76% half-even/ECMAScript disagreement rate; random finite
  doubles found about 0.04%. Structured cases are therefore mandatory.
- All 17 reachable `Math.pow(10, -e)` SI scales matched their decimal-literal bit patterns in Node
  24.18.0. Multiplication and division were observably different in 4 of 11 sampled pairs.
- Criterion 0.8.2 has a mandatory `alloca` dependency which uses `cc`; Criterion 0.7.0 avoids that
  build dependency.

Every one of these counts was re-measured during Phase 0 against commit `c6a8b27` and reproduced
exactly, with no drift: 729 executable static assertion sites, 1,078 dynamic executions, 793
boundary calls with the per-operation split intact, 194 specifiers, 49 locale definitions, and 59
locale files of which 23 carry numerals. The assertion counts were confirmed by two independent
methods that agreed. The live `process.versions` values also matched the §3.1 sample exactly.

These measurements are recorded in generated provenance during Phase 1. If a clean rerun differs,
the gate fails and the plan is updated; counts are not silently changed.

### 0.4 Definition of done

Release requires all of the following:

1. Every one of the 729 executable static assertion sites maps to one or more generated or named
   Rust tests, with no orphaned mappings.
2. The generated suite reproduces all 1,078 baseline assertion executions and all 793 observed API
   calls, with raw `f64` bits preserved.
3. Numeric primitive differential tests, structured fixed- and significant-digit tie corpora,
   boundary corpora, the full formatter differential, and the 59-locale differential all report
   zero mismatches.
4. Pinned-toolchain, MSRV, current-stable, feature-combination, Windows GNU, Ubuntu, and core WASM
   compile gates pass.
5. Formatting never panics for any accepted specifier, locale, or `f64` bit pattern. Allocation
   limits fail with a documented error.
6. `cargo package` and `cargo publish --dry-run` succeed from `rust/` without reading outside it.
7. Release-candidate downstream smoke tests pass and a benchmark baseline is archived.

## 1. Repository layout and source of truth

```text
d3-format-rust/
├── src/ test/ locale/ package.json yarn.lock   # unchanged JavaScript product and oracle
├── tools/harvest/                              # existing exploratory prototype; retained
├── .github/workflows/
│   ├── test.yml                                # JS, Rust, feature, and parity gates
│   ├── publish.yml                             # existing npm publication; unchanged
│   └── publish-rust.yml                        # gated Cargo publication
└── rust/
    ├── .cargo/config.toml
    ├── Cargo.toml
    ├── Cargo.lock
    ├── rust-toolchain.toml
    ├── README.md
    ├── DIVERGENCES.md
    ├── COMPATIBILITY.md
    ├── ci/check.ps1
    ├── src/
    │   ├── lib.rs
    │   ├── specifier.rs
    │   ├── decimal.rs
    │   ├── bignat.rs
    │   ├── types.rs
    │   ├── locale.rs
    │   ├── precision.rs
    │   ├── limits.rs
    │   ├── bin/dump_cases.rs
    │   └── locales/generated.rs
    ├── tests/
    │   ├── gate.rs
    │   ├── generated/
    │   ├── ported/
    │   ├── properties.rs
    │   └── public_api.rs
    ├── benches/format.rs
    ├── fixtures/
    │   └── d3-format-3.1.2/
    │       ├── manifest.json
    │       ├── oracle.json
    │       ├── calls.jsonl
    │       ├── decimal.jsonl
    │       ├── regressions.jsonl
    │       └── assertion-map.json
    └── tools/
        ├── vitest-shim.mjs
        ├── instrumented.mjs
        ├── harvest.mjs
        ├── gen-tests.mjs
        ├── check-coverage.mjs
        ├── gen-locales.mjs
        ├── gen-corpus.mjs
        ├── differential.mjs
        ├── oracle-diff.mjs
        └── check-oracle-metadata.mjs
```

The existing `tools/harvest/` remains evidence and a prototype. Phase 1 may reuse its code, but the
committed production tools live at the paths above and are not considered present until added.

Locale JSON remains the editable source. `rust/src/locales/generated.rs` is generated and committed.
No `build.rs` may read `../locale`: a published crate must build from its packaged contents.

Add `/rust/target/` to `.gitignore`. Commit `rust/Cargo.lock`, fixtures, generated tests, generated
locales, and the provenance manifest.

The pre-existing `.gitignore` entry for the prototype tool directory was the unanchored pattern
`tools/`, which also matches `rust/tools/` and would have silently untracked every production tool
in §3. It is anchored to `/tools/` so it still ignores the repository-root prototype only. Ignore
patterns that name a directory must be root-anchored whenever `rust/` has a directory of the same
name.

Two `.cargo/config.toml` files, one at the repository root and one in `rust/`, pin MSRV-aware
dependency resolution:

```toml
[resolver]
incompatible-rust-versions = "fallback"
```

Default resolution selects `clap` releases that require Rust 1.85 and edition 2024. Cargo 1.80
cannot parse those manifests at all, so the MSRV job fails before compiling anything. The committed
lockfile encodes the correct selection, but without this key any `cargo update` silently reintroduces
the failure. The key is stable as of Cargo 1.84 and is therefore honored by the pinned 1.98.1
toolchain.

The duplication is deliberate. Cargo discovers configuration by walking up from its **working
directory**, not from `--manifest-path`. A single copy in `rust/` would be bypassed by the
root-relative commands this document itself teaches in §2.4 and §3.7: run from the repository root,
`cargo generate-lockfile --manifest-path rust/Cargo.toml` resolves 43 packages and selects the
unparseable `clap`. The root copy covers those invocations; the `rust/` copy keeps the crate
directory correct if it is ever built in isolation. The key is static, so the copies cannot drift.

Pinning Criterion does not substitute for this. `=0.7.0` is justified in §0.3 and §7 solely as an
`alloca`/`cc` guard, and Criterion 0.7.0 under default resolution still pulls the MSRV-breaking
`clap`. Removing the resolver configuration on the theory that the version pin covers it
reintroduces the failure.

**The lockfile is regenerated only under the pinned toolchain.** Cargo 1.80 predates the key and
ignores it, printing `warning: unused config key resolver.incompatible-rust-versions` and exiting
zero. Running `cargo update` or `cargo generate-lockfile` on the MSRV toolchain therefore reselects
the unparseable `clap` and produces a lockfile that same toolchain cannot build. The MSRV job
consumes the committed lock; it never writes one.

This repository sets `core.autocrlf=true` and has no historical `.gitattributes`, so a Windows
checkout rewrites committed LF content to CRLF in the working tree. Every byte-comparison gate in
§3 would then fail against a fresh regeneration. A root `.gitattributes` pins the port's working-tree
form:

```gitattributes
rust/** text eol=lf
```

The rule is scoped to `rust/`. The JavaScript oracle keeps its existing checkout behavior, because
renormalizing it is an unrelated change to the reference implementation.

## 2. Phase 0: machine, toolchains, baseline, and gates

**Status: §2.4 passed. The §7 CI-skeleton half of the exit condition is outstanding, because
`rust/ci/check.ps1` and the Rust CI jobs are not yet written.**

### 2.1 Repository-local JavaScript tool

The repository already uses Yarn v1 and has a Yarn lockfile. A user-level
`C:\Users\okhil\package.json` selecting pnpm must not decide this project's package manager.
Phase 0 adds this field to the repository's own `package.json`:

```json
"packageManager": "yarn@1.22.22"
```

Use:

```powershell
corepack enable
corepack prepare yarn@1.22.22 --activate
yarn --frozen-lockfile
yarn vitest run
yarn lint
```

If Acorn is used by the source transformer/coverage checker, add it as a direct pinned development
dependency and update `yarn.lock`; do not rely on a transitive package. Do not introduce
`pnpm-lock.yaml`, and do not run npm against the Yarn lockfile.

### 2.2 Rust installation on this Windows machine

Install rustup, choose the GNU host because this machine has no required MSVC toolchain, and install
the pinned and MSRV toolchains:

```powershell
winget install --id Rustlang.Rustup

$env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ';' +
            [Environment]::GetEnvironmentVariable("Path", "User")

rustup set default-host x86_64-pc-windows-gnu
rustup toolchain install 1.98.1-x86_64-pc-windows-gnu --profile minimal `
  --component rustfmt --component clippy
rustup toolchain install 1.80.0-x86_64-pc-windows-gnu --profile minimal
```

`rust/rust-toolchain.toml` is cross-platform and pins the development compiler, not the host:

```toml
[toolchain]
channel = "1.98.1"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

On each operating system rustup resolves that channel to its native host. CI uses fully qualified
toolchain names to remove ambiguity. The crate's MSRV is independently declared as Rust 1.80.0.

Linking gate:

```powershell
cargo test --manifest-path rust/Cargo.toml --all-features
```

An empty `cargo new --lib` crate is **not** an adequate linking gate. It has no dependency that uses
`raw-dylib`, so it links successfully on a machine that cannot build this crate. The gate must build
the real dev-dependency graph, which reaches `windows-sys` through Criterion.

On a GNU host that graph fails with `error calling dlltool 'dlltool.exe': program not found` until
MinGW binutils are available. Reinstalling `rust-mingw` does not fix this. The component is not
broken: it ships `dlltool.exe` under
`lib/rustlib/x86_64-pc-windows-gnu/bin/self-contained/`, but rustc's raw-dylib support resolves
`dlltool` from `PATH` only. Putting that directory on `PATH` then fails a second time, because
`dlltool` shells out to the GNU assembler and `rust-mingw` ships no `as.exe`.

The working remedy on this machine was the plan's second option: install MSYS2 with
`mingw-w64-x86_64-binutils` and append `C:\msys64\mingw64\bin` to the user `PATH`. Append rather
than prepend, so it shadows nothing. MSVC Build Tools remain the final fallback and were not needed.
This is host configuration, not a repository artifact; GitHub's `windows-latest` images already ship
MinGW binutils. If GNU linking fails for some other reason, capture the complete linker command and
error before changing toolchains.

### 2.3 Commit-ready Rust semantics gate

Create `rust/tests/gate.rs` before algorithm work. It must include, at minimum:

```rust
#[test]
fn rust_semantics_used_by_the_port() {
    assert_eq!(format!("{:.0}", 2.5_f64), "2");
    assert_eq!(format!("{:.2}", 0.125_f64), "0.12");
    assert_eq!(format!("{}", 1e21_f64), "1000000000000000000000");
    assert_eq!(format!("{}", 42.0_f64), "42");
    assert_eq!(format!("{}", f64::MAX).len(), 309);
    assert_eq!(format!("{:e}", 1.29e-30_f64), "1.29e-30");
    assert_eq!((-1.5_f64).round(), -2.0);
    assert_eq!(0.5_f64.round_ties_even(), 0.0);
    assert_eq!((-1_i32).div_euclid(3), -1);
    assert_eq!(1e40_f64 as u128, u128::MAX);
    assert!("".parse::<f64>().is_err());
    assert_eq!("inf".parse::<f64>().unwrap(), f64::INFINITY);
    assert_eq!("😀".len(), 4);
    assert_eq!("😀".chars().map(char::len_utf16).sum::<usize>(), 2);
    assert_eq!(0.07_f64 * 100.0, 7.000000000000001);
    assert_eq!(1e-9_f64.to_bits(), 0x3E11_2E0B_E826_D695);
    assert_eq!(1e-9_f64 * 12_345_678_900.0, 12.345678900000001);
    assert_ne!(1e-9_f64 * 12_345_678_900.0, 12_345_678_900.0 / 1e9);
}
```

The gate runs under Rust 1.80.0, pinned 1.98.1, and current stable. If behavior differs, code must
not paper over the test; either remove reliance on that standard-library behavior or revise the
compatibility contract with evidence.

### 2.4 Phase 0 exit gate

Phase 0 passes only when all commands below succeed in a clean checkout:

```powershell
yarn --frozen-lockfile
yarn vitest run
yarn lint
cargo test --manifest-path rust/Cargo.toml --test gate
```

Artifacts: repository-local package-manager declaration, `rust/Cargo.toml`,
`rust/rust-toolchain.toml`, `rust/tests/gate.rs`, and recorded Node oracle metadata.

## 3. Exact JavaScript oracle and deterministic generation

**Status: pending. Existing runtime capture is a prototype only.**

### 3.1 Oracle identity

Node 24 as a moving major is not a pin. CI and fixture generation use exactly Node `24.18.0`.
`rust/fixtures/d3-format-3.1.2/oracle.json` records:

```json
{
  "node": "v24.18.0",
  "v8": "13.6.233.17-node.50",
  "icu": "78.3",
  "unicode": "17.0",
  "cldr": "48.0",
  "d3_format_version": "3.1.2",
  "d3_source_commit": "ebdc2d530277df379157f82fee6ea5623d179bd7",
  "repository_input_commit": "<captured by the generator>"
}
```

`check-oracle-metadata.mjs` compares the Node/V8/ICU/Unicode/CLDR fields to the running process,
verifies the oracle source hashes, and exits non-zero on drift. The repository input commit is
provenance; unrelated later commits do not invalidate it.
The source, tests, locales, package manifest, and lockfile in the reviewed checkout are byte-identical
to upstream commit `ebdc2d5`; the repository itself may contain later migration-only commits.
Updating the oracle is a reviewed compatibility event: regenerate fixtures in a dedicated commit,
run the complete differential suite, and explain all output changes.

### 3.2 Harvester contracts

`harvest.mjs` runs from the repository root and:

1. Loads the real 24 test files in sorted path order.
2. Redirects only `vitest` and `src/index.js`, avoiding wrapper self-resolution.
3. Executes the assertions, failing on any JavaScript mismatch.
4. Records every boundary call with test file, test title, static assertion ID, dynamic execution
   ordinal, operation, locale ID, canonical specifier, raw input bits or text, and output.
5. Writes canonical UTF-8 JSONL with LF endings, fixed key order, uppercase 16-digit bit patterns,
   and JSON escaping for all strings.
6. Never serializes a numeric input through a JSON number. `-0`, NaNs, infinities, and payload bits
   are represented by `f64` bits.

Static assertion IDs are `relative/path:line:column` from an Acorn AST. A loader transform injects
the ID before evaluation of each assertion's actual argument, so calls made while evaluating that
argument are associated with the correct site. Loops retain one static ID and increment a dynamic
ordinal. `assert.throws` retains the ID while executing its callback.

The harvester writes to a temporary directory first. `--check` regenerates and byte-compares all
outputs without touching tracked files. `--write` atomically replaces outputs and is allowed only
for intentional source/oracle changes. Tools always write LF; the `.gitattributes` rule in §1 is
what keeps the checked-out bytes equal to the generated bytes on Windows.

### 3.3 Coverage mapping

`check-coverage.mjs` parses all test files with Acorn, ignores comments naturally, and requires:

- exactly 729 executable static assertion IDs for the current baseline;
- each ID mapped to at least one generated case or a named test in `assertion-map.json`;
- each of the 1,078 dynamic executions represented;
- all 793 recorded boundary calls represented;
- no unknown, duplicate, stale, or orphaned mapping;
- an explicit reason and Rust test path for every hand-ported or intentionally reinterpreted site.

The mutable-global identity assertions and JavaScript coercion assertions are not called “passed” by
a different Rust API. They are mapped to explicit entries in `DIVERGENCES.md` and named API tests.

### 3.4 Generated tests and locales

`gen-tests.mjs` consumes only versioned fixture files and emits ASCII-only Rust source under
`rust/tests/generated/`. It emits one test per original test block, preserving the source title and
including assertion IDs in failure messages. Generated files are never edited manually.

`gen-locales.mjs` reads the 59 root JSON files in sorted order, validates the observed schema,
requires ten numerals where present, and emits `rust/src/locales/generated.rs`. It writes a
name-to-definition index and the source SHA-256 for every locale. `--check` verifies byte identity,
file count, names, hashes, and generated output.

### 3.5 Provenance manifest

`manifest.json` contains:

- schema and generator versions;
- oracle metadata and d3 commit;
- SHA-256 of every source test and locale;
- counts for files, tests, static assertions, dynamic assertions, calls, specifiers, and locales;
- generator command lines and deterministic seeds;
- SHA-256 of every fixture except the manifest itself, and every generated Rust file.

Two consecutive clean generations must produce identical bytes and hashes.

### 3.6 Differential protocol

`gen-corpus.mjs` writes canonical input JSONL. `rust/src/bin/dump_cases.rs` reads that JSONL from
stdin and writes canonical output JSONL to stdout. `differential.mjs` computes Node outputs for the
same records and compares decoded strings and numeric results exactly.

Each case contains a stable ID, operation, raw input bits, precision/specifier, locale definition or
locale hash, and expected oracle metadata. Outputs are JSON strings, not tab-delimited text, so
embedded whitespace and Unicode are unambiguous. On mismatch the runner:

- exits non-zero;
- writes the first mismatch plus seed and provenance to an artifact;
- prints an `oracle-diff.mjs` command that reproduces one case;
- never rewrites fixtures.

### 3.7 Phase 1 exit gate

```powershell
node rust/tools/check-oracle-metadata.mjs
node rust/tools/harvest.mjs --check
node rust/tools/check-coverage.mjs
node rust/tools/gen-tests.mjs --check
node rust/tools/gen-locales.mjs --check
cargo test --manifest-path rust/Cargo.toml --no-run
$status = git status --porcelain -- rust/fixtures rust/tests/generated rust/src/locales/generated.rs
if ($status) { throw "generated artifacts are dirty:`n$status" }
```

The manifest must report the baseline counts in §0.3. Any count drift blocks implementation until
classified.

## 4. ECMAScript numeric contract

### 4.1 Required conversion behavior

All conversions operate on the exact binary64 value.

`toFixed(p)`:

- d3 reaches `p = 0..20`;
- round to nearest decimal, choosing the larger integer candidate on an exact tie;
- for `abs(x) >= 1e21`, return ECMAScript base-10 `Number::toString(x)`, not positional digits;
- therefore `(1e21).toFixed(2) == "1e+21"`.

`toExponential(p)`:

- with `Some(p)`, emit exactly `p` fraction digits, round with the same tie rule, renormalize carry,
  and spell the exponent as `e`, explicit `+`/`-`, and no zero padding;
- with `None`, emit the shortest round-tripping coefficient in exponential notation;
- non-finite values are `"NaN"`, `"Infinity"`, or `"-Infinity"` before locale sign handling.

`toPrecision(p)`:

- d3 reaches `p = 1..21`;
- round to `p` significant digits before selecting notation;
- use exponential notation iff the post-rounding exponent is `< -6` or `>= p`;
- preserve required trailing zeros.

Base-10 `Number::toString`:

- emit the shortest round-tripping decimal;
- use fixed notation for finite non-zero magnitudes in `[1e-6, 1e21)`;
- use exponential notation outside that interval, with explicit exponent sign;
- preserve the ECMAScript spellings of zero and non-finite values.

Type `d` is a separate rule. After non-negative `Math.round`, d3 uses
`toLocaleString("en").replace(/,/g, "")` for magnitudes at least `1e21`. For finite values this is
the full positional expansion of shortest digits, not the exact mathematical integer represented
by the bits. For infinity d3 emits `∞`. Tests must keep this distinct from `toFixed`'s `1e21`
fallback.

Required threshold cases include `1e21`, its immediate predecessor and successor, `1e20`,
`1e-6`, `1e-7`, their adjacent floats, `±0`, minimum subnormal, minimum normal, maximum finite,
NaNs with both sign bits, and both infinities.

`formatPrefix` has a separate reference-value edge: a zero, NaN, or infinite reference makes its
scale NaN, and formatting any numeric input returns the locale's NaN representation with no SI
suffix. Negative finite references select the bucket from their magnitude. These cases are explicit
fixtures; Rust must not cast a NaN exponent to an integer or index the prefix table with it.

### 4.2 Decimal engine

The default runtime dependency graph remains empty. `bignat.rs` implements only the operations
required for exact `m * 2^k` conversion and decimal digit generation. No third-party formatting
crate is used.

The shared engine provides:

```rust
fn to_fixed(x: f64, precision: u32, limits: &FormatLimits)
    -> Result<String, FormatError>;
fn to_exponential(x: f64, precision: Option<u32>, limits: &FormatLimits)
    -> Result<String, FormatError>;
fn to_precision(x: f64, precision: u32, limits: &FormatLimits)
    -> Result<String, FormatError>;
fn number_to_string(x: f64, limits: &FormatLimits)
    -> Result<String, FormatError>;
fn format_decimal(x: f64, limits: &FormatLimits)
    -> Result<String, FormatError>;
fn format_decimal_parts(
    x: f64,
    precision: Option<u32>,
    limits: &FormatLimits,
) -> Result<Option<(String, i32)>, FormatError>;
fn exponent(x: f64) -> Option<i32>;
fn js_round_nonnegative(x: f64) -> f64;
```

`Some(0)` passed through d3's falsy `p` path is explicitly translated to `None` where required by
`formatPrefixAuto`; it must not underflow `p - 1`. Percent scaling is one ordinary `x * 100.0`
operation. Prefix scaling uses a 17-entry bit-pinned constant table and `scale * value`, never
division or `powi`. BigNat limbs, digit vectors, and returned strings use checked arithmetic and
`try_reserve`; internal numeric conversion cannot bypass the public allocation policy.

### 4.3 Exact fixed- and significant-digit ties

Random values are supplementary. The structured corpus has two independent generators:

1. Fixed precision: canonical `x = m * 2^k`, with odd `m`, is retained when exact rational
   arithmetic proves `x * 10^p` has remainder exactly one half. The closed form
   `k == -(p + 1)` is used only after canonicalization.
2. Significant precision: for each candidate, compute the exact decimal exponent `e`, then test
   whether `x * 10^(p - 1 - e)` has remainder exactly one half. This uses integer numerator and
   denominator arithmetic; `log10` is not the oracle.

The significant generator synthesizes decimal half candidates across the finite decimal exponent
range, checks each candidate and adjacent `f64` values with the exact predicate, and retains cases
where half-even and ECMAScript choose different outputs. It covers every d3-reachable precision and
records counts per precision and decimal-exponent bucket.

Separate carry corpora cover coefficients ending in runs of 9 around:

- fixed decimal-point carries;
- `toExponential` exponent increments;
- `toPrecision` fixed/exponential switches;
- SI bucket boundaries at `10^(3n)`;
- `9.5`, `99.5`, `999.5`, and scaled forms, each with adjacent floats.

Primitive acceptance is zero mismatches for:

- every generated fixed tie through `toFixed` and `%`;
- every generated significant tie through `toExponential`, `toPrecision`, `r`, `s`, `p`, `g`,
  `n`, and empty/unknown types;
- every carry case through all affected format families;
- at least 2,000,000 additional deterministic finite bit patterns and all boundary cases.

The manifest, not this prose, records actual corpus sizes. A generator producing zero cases for an
expected bucket is a gate failure, not a pass.

### 4.4 Numeric risks that remain explicit

The implementation and regression suite must preserve:

- half-away-from-zero decimal rounding against exact bits;
- `toFixed`'s `1e21` switch to `Number::toString`;
- post-rounding notation and SI exponent selection;
- floor division via `div_euclid(3)`;
- shortest-decimal exponent calculation rather than `log10().floor()`;
- abs-before-`Math.round` ordering for integer types;
- negative zero and negative-NaN sign policy;
- formatted-string zero suppression semantics;
- radix output up to the 1,024-bit representation of `f64::MAX`;
- UTF-16 code-unit width;
- trim-before-grouping and numeral-substitution-last ordering;
- lowercase `0x` prefix for uppercase `X`;
- ordinary f64 subtraction in `precisionRound`;
- exact `Math.pow` scale bits and multiplication order for `formatPrefix`.

## 5. Public Rust API and safety policy

### 5.1 Package and features

`rust/Cargo.toml` starts with:

```toml
[package]
name = "d3-format"
version = "0.1.0"
edition = "2021"
rust-version = "1.80"
license = "ISC"
repository = "https://github.com/alexkhil/d3-format-rust"
description = "A Rust implementation of d3-format 3.1.2"
exclude = ["fixtures/", "tools/", "rust-toolchain.toml"]

[lib]
name = "d3_format"

[features]
default = []
locales = []
serde = ["dep:serde"]

[dependencies]
serde = { version = "1", optional = true, features = ["derive"] }

[dev-dependencies]
criterion = { version = "=0.7.0", default-features = false, features = ["cargo_bench_support"] }
```

The proposed package name was unoccupied when this plan was reviewed, but availability must be
checked again before publication. The Rust API begins at `0.1.0`; it does not claim semver `3.1.2`.
`COMPATIBILITY.md` records that outputs target d3-format 3.1.2 at commit `ebdc2d5`.

`exclude` keeps the published artifact independent of the verification corpus. Without it the
`.crate` carries `fixtures/`, which Phase 1 grows with `calls.jsonl`, `decimal.jsonl`, the tie and
carry corpora, and the assertion map, and Phase 5 sizes in millions of cases. The package would then
cross the crates.io 10 MiB limit and fail §6's packaging gate for a distribution reason unrelated to
correctness. `rust-toolchain.toml` is excluded for a separate reason: rustup resolves it by walking
up from the build directory, so shipping it would force channel 1.98.1 on anyone building the
unpacked crate, defeating §9's canary that the package builds under a consumer's own toolchain.
`tests/` stays packaged because `gate.rs` has no fixture dependency and §9 requires the packaged
crate to build and test independently; whether generated tests remain packaged is decided in Phase 1
against their measured size.

Feature policy:

- default: core locale construction and formatting only;
- `locales`: all 59 generated named locales;
- `serde`: serialization/deserialization for the stable locale-definition wire shape;
- features are additive and do not change formatting results;
- no default runtime dependencies;
- no `wasm` feature until a real boundary crate exists.

### 5.2 Specifier

`FormatSpecifier` is immutable after validated construction. Fields are private with typed getters
and a builder. `FromStr` parses the d3 grammar, and `Display` emits its canonical normalized form.

```rust
pub struct FormatSpecifier { /* private validated fields */ }

impl std::str::FromStr for FormatSpecifier {
    type Err = ParseError;
}
impl std::fmt::Display for FormatSpecifier { /* canonical d3 spelling */ }
```

Differences from JavaScript are intentional:

- no mutable dynamically typed fields;
- no `String(value)`, `Number(value)`, or ToInt32 mutation behavior;
- a non-BMP or newline fill is rejected;
- width syntax that overflows `u32` is rejected;
- unknown ASCII type letters remain stored and format as d3's `.12~g` fallback.

`Locale::formatter_for(&FormatSpecifier)` is exactly equivalent to
`Locale::formatter(&specifier.to_string())`. Since the Rust value is immutable and validated, this
round trip is stable. It must not bypass normalization. JavaScript's lossy round trip after mutating
public fields is documented as unsupported.

Invalid syntax uses `ParseError` whose `Display` text is exactly `invalid format: {input}` for
compatibility with the existing throw assertions. Limit, locale-validation, and input-kind errors
have distinct variants and are not disguised as parse errors.

### 5.3 Locale ownership and formatter behavior

```rust
#[derive(Clone)]
pub struct Locale(std::sync::Arc<LocaleInner>);

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LocaleDefinition {
    pub decimal: Option<String>,
    pub thousands: Option<String>,
    pub grouping: Option<Vec<u32>>,
    pub currency: Option<[String; 2]>,
    pub numerals: Option<[String; 10]>,
    pub percent: Option<String>,
    pub minus: Option<String>,
    pub nan: Option<String>,
}

pub struct LocaleBuilder { /* owned fields */ }

impl LocaleBuilder {
    pub fn decimal(self, value: impl Into<String>) -> Self;
    pub fn thousands(self, value: impl Into<String>) -> Self;
    pub fn grouping(self, value: impl Into<Vec<u32>>) -> Self;
    pub fn currency(
        self,
        prefix: impl Into<String>,
        suffix: impl Into<String>,
    ) -> Self;
    pub fn numerals(self, value: [String; 10]) -> Self;
    pub fn percent(self, value: impl Into<String>) -> Self;
    pub fn minus(self, value: impl Into<String>) -> Self;
    pub fn nan(self, value: impl Into<String>) -> Self;
    pub fn build(self) -> Result<Locale, LocaleError>;
}

impl Locale {
    pub fn en_us() -> Locale;
    pub fn builder() -> LocaleBuilder;
    pub fn from_definition(value: LocaleDefinition) -> Result<Locale, LocaleError>;
    #[cfg(feature = "locales")]
    pub fn named(name: &str) -> Option<Locale>;

    pub fn formatter(&self, spec: &str) -> Result<Formatter, FormatError>;
    pub fn formatter_for(&self, spec: &FormatSpecifier) -> Result<Formatter, FormatError>;
    pub fn formatter_with_limits(
        &self,
        spec: &str,
        limits: FormatLimits,
    ) -> Result<Formatter, FormatError>;
    pub fn prefix_formatter(
        &self,
        spec: &str,
        reference: f64,
    ) -> Result<Formatter, FormatError>;
    pub fn prefix_formatter_with_limits(
        &self,
        spec: &str,
        reference: f64,
        limits: FormatLimits,
    ) -> Result<Formatter, FormatError>;
}

#[derive(Clone)]
pub struct Formatter {
    locale: Locale,
    specifier: FormatSpecifier,
    input_kind: InputKind,
    /* immutable precomputed layout */
}

impl Formatter {
    pub fn input_kind(&self) -> InputKind;
    pub fn specifier(&self) -> &FormatSpecifier;
    pub fn format_number(&self, value: f64) -> Result<String, FormatError>;
    pub fn format_number_into(&self, out: &mut String, value: f64)
        -> Result<(), FormatError>;
    pub fn format_text(&self, value: &str) -> Result<String, FormatError>;
    pub fn format_text_into(&self, out: &mut String, value: &str)
        -> Result<(), FormatError>;
}
```

`Formatter` owns a cheap clone of the locale `Arc`; it has no lifetime parameter. A numeric
specifier rejects `format_text`, and type `c` rejects `format_number`, both with
`FormatError::InputTypeMismatch`. This is an intentional deviation from JavaScript coercion.
Type `c` preserves its text byte-for-byte except for d3-compatible prefix, suffix, width, and
alignment processing and final locale-numeral substitution.

`LocaleBuilder::default()` and missing `LocaleDefinition` fields use d3's defaults: decimal `.`,
empty currency affixes, percent `%`, minus U+2212, and NaN `NaN`; grouping is disabled unless both
`grouping` and `thousands` are present. `Locale::en_us()` additionally supplies grouping `[3]`,
thousands `,`, and currency prefix `$`.

`format_*_into` clears `out` before work and leaves it empty on error. It reuses caller capacity but
does not claim to be allocation-free: exact decimal conversion may allocate temporary limbs and
digits. The benchmark separately measures owned output, reused output, parser construction, and
worst-case subnormal/radix paths.

`LocaleBuilder` defines these edge policies:

- grouping is disabled unless both grouping and thousands are supplied;
- an empty grouping list disables grouping;
- a zero grouping size is rejected;
- numerals must contain exactly ten strings;
- locale text may be Unicode and is measured in UTF-16 code units for width;
- built locales and formatters are immutable.

`Locale`, `FormatSpecifier`, and `Formatter` must be `Send + Sync`. There is no mutable global
default locale, mutable SI exponent, shared scratch buffer, or result cache. A formatter caches only
immutable normalized specifier state, affixes, grouping configuration, and fixed-prefix scale.
Static tables may use immutable `LazyLock` values compatible with Rust 1.80.

The crate requires `std` for owned strings, `Arc`, error types, and float formatting helpers.
`no_std` is not supported and is not advertised.

### 5.4 Allocation limits and errors

The default limits are:

```rust
pub struct FormatLimits {
    pub max_width_utf16: u32,   // default 1_000_000
    pub max_output_bytes: usize // default 16 * 1024 * 1024
}
```

Parsing rejects a syntactic width above `u32::MAX`. Formatter construction rejects width above
`max_width_utf16`. Every dynamic output buffer uses checked length arithmetic and `try_reserve`;
locale numerals, affixes, separators, and text input are included in the output-byte limit.
`formatter`, `formatter_for`, and `prefix_formatter` use `FormatLimits::default()`; their
`*_with_limits` variants use the caller's limits.

Errors are typed:

```rust
pub enum FormatError {
    InvalidSpecifier(ParseError),
    WidthLimit { requested: u32, maximum: u32 },
    OutputLimit { requested: usize, maximum: usize },
    AllocationFailed,
    InputTypeMismatch { expected: InputKind, actual: InputKind },
}
```

Callers needing larger widths may construct higher limits, but formatting remains fallible. No
accepted input may trigger integer wrap, panic, or an intentional multi-gigabyte allocation.

### 5.5 Precision helpers

```rust
pub fn precision_fixed(step: f64) -> f64;
pub fn precision_round(step: f64, max: f64) -> f64;
pub fn precision_prefix(step: f64, value: f64) -> f64;
pub fn as_precision(value: f64) -> Option<u32>;
```

The primary helpers return `f64` because d3 returns NaN for zero and non-finite cases and existing
tests assert that contract. `as_precision` is an ergonomic conversion, not a semantic replacement.

## 6. CI design

All scripts are committed before the workflow references them. PowerShell 7 is used for project
scripts on Ubuntu and Windows; no workflow embeds Bash syntax in a PowerShell step.

### 6.1 Pinned host matrix

```yaml
rust-pinned:
  strategy:
    fail-fast: false
    matrix:
      include:
        - os: ubuntu-latest
          toolchain: 1.98.1-x86_64-unknown-linux-gnu
        - os: windows-latest
          toolchain: 1.98.1-x86_64-pc-windows-gnu
  runs-on: ${{ matrix.os }}
  env:
    RUSTUP_TOOLCHAIN: ${{ matrix.toolchain }}
  defaults:
    run:
      shell: pwsh
      working-directory: rust
  steps:
    - uses: actions/checkout@v6
    - run: rustup toolchain install $env:RUSTUP_TOOLCHAIN --profile minimal --component rustfmt --component clippy
    - uses: Swatinem/rust-cache@v2
      with:
        workspaces: rust
    - run: ./ci/check.ps1 -Profile Pinned
```

`ci/check.ps1 -Profile Pinned` runs:

```powershell
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --no-default-features
cargo test --features locales
cargo test --features serde
cargo test --all-features
cargo bench --no-run
cargo package

$metadata = cargo metadata --format-version 1 --all-features |
    ConvertFrom-Json
$forbidden = @($metadata.packages | Where-Object { $_.name -in @('cc', 'alloca') })
if ($forbidden.Count -ne 0) {
    cargo tree -i cc
    throw 'a forbidden C-backed benchmark dependency entered the graph'
}
```

The Windows entry tests the same GNU host selected for local development. An additional MSVC job may
be added as compatibility coverage, but it does not replace the GNU job.

Until Phase 5 adds `benches/format.rs`, `cargo bench --no-run` compiles Criterion and the implicit
library bench target only. It is a real dependency-graph and linking check from Phase 0 onward, but
it is not benchmark coverage and must not be reported as such.

### 6.2 MSRV, stable, features, and WASM compile

Separate Ubuntu jobs install:

- `1.80.0-x86_64-unknown-linux-gnu` and run tests for no-default, `locales`, `serde`, and
  all-features;
- current `stable-x86_64-unknown-linux-gnu` and run the full pinned profile;
- current stable plus `wasm32-unknown-unknown`, then
  `cargo check --target wasm32-unknown-unknown --no-default-features`.

The stable job is intentionally moving compatibility coverage; it does not define fixture
semantics. Pinned 1.98.1 defines the reproducible implementation baseline.

As of Phase 0, 1.98.1 is itself current stable, so the stable job builds the same compiler as the
pinned job and adds no independent coverage yet. That is expected and self-correcting once stable
advances; the job is not evidence of forward compatibility until then. Only the MSRV job currently
exercises a genuinely different compiler.

### 6.3 Oracle and parity job

```yaml
parity:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - uses: actions/setup-node@v6
      with:
        node-version: 24.18.0
        cache: yarn
    - run: corepack enable
    - run: corepack prepare yarn@1.22.22 --activate
    - run: yarn --frozen-lockfile
    - run: yarn vitest run
    - run: yarn lint
    - run: node rust/tools/check-oracle-metadata.mjs
    - run: node rust/tools/harvest.mjs --check
    - run: node rust/tools/check-coverage.mjs
    - run: node rust/tools/gen-tests.mjs --check
    - run: node rust/tools/gen-locales.mjs --check
    - run: node rust/tools/differential.mjs --corpus ci --seed 1 --cases 200000
```

The parity runner invokes the Rust binary with `--manifest-path rust/Cargo.toml` or sets its child
working directory to `rust`; it does not rely on toolchain discovery from the repository root.

A scheduled workflow runs the versioned pre-release corpus, at least 10,000,000 formatter
comparisons, and uploads the seed, manifest, mismatch artifacts, and summary. Unchanged state is not
success unless the command completed and reported the expected case count.

## 7. Implementation and rollout phases

Every phase begins from a green previous phase. A failure returns to the lowest responsible phase;
fixtures are never edited to accommodate Rust output.

### Phase 0 — Environment and baseline

Dependencies: none.

Deliverables: package-manager declaration, crate skeleton, toolchain files, gate test, oracle
metadata.

Exit: §2.4 passes on this machine and the first pinned CI skeleton links.

Rollback: remove only the incomplete `rust/` skeleton and workflow job; the JS product is untouched.

### Phase 1 — Deterministic oracle and scoreboard

Dependencies: Phase 0.

Deliverables: all tools and fixtures in §3, generated placeholder tests, locale generator,
provenance, drift checks.

Exit: §3.7 passes twice with identical hashes; coverage is 729/729 static IDs, 1,078 dynamic
executions, and 793 calls.

Failure: fix the loader, mapper, or generator. Never edit generated Rust or waive an assertion
without a named divergence.

### Phase 2 — Exact number conversion

Dependencies: Phase 1.

Deliverables: `bignat.rs`, `decimal.rs`, primitive fixture set, fixed/significant tie corpora, carry
corpora, minimized regressions.

Exit: §4.3 primitive gates report zero mismatches under MSRV, pinned, and stable. Explicit threshold
tests include `1e21` and adjacent values for both `toFixed` and type `d`.

Failure: preserve bits, operation, precision, and oracle metadata in `regressions.jsonl`; fix the
shared primitive, then rerun the entire numeric corpus.

### Phase 3 — Specifier and public API

Dependencies: Phase 2.

Deliverables: immutable specifier, typed errors, limits, owned Locale/Formatter API, API tests,
initial `DIVERGENCES.md`.

Exit: parser/display properties, all hand-ported specifier tests, wrong-input-kind tests, allocation
limit tests, and `Send + Sync` compile assertions pass.

Failure: decide whether the case is a parser bug or an intentional typed-API deviation; record the
latter before proceeding.

### Phase 4 — Types, layout, SI, precision, and locales

Dependencies: Phase 3.

Deliverables: type dispatch, grouping, trim, UTF-16 width, signs, SI, `formatPrefix`, precision
helpers, generated locales.

Exit: all generated and ported tests pass; the 59-locale differential and concurrency stress tests
report zero mismatches; no accepted input panics.

Failure: minimize to `(assertion ID, specifier, locale hash, input bits)`, add a regression, and fix
the lowest layer. Locale or layout code must not compensate for wrong numeric digits.

### Phase 5 — Full hardening, benchmarks, and CI

Dependencies: Phase 4.

Deliverables: property tests, full differential evidence, benchmark baseline, complete CI matrix.

Exit:

- per-PR 200,000-case differential passes;
- pre-release 10,000,000-case and complete structured corpora pass;
- feature, MSRV, pinned, stable, GNU Windows, Ubuntu, and WASM compile jobs pass;
- `cargo fmt`, Clippy, tests, bench compilation, and `cargo package` pass.

Benchmark reports owned output and reused-output paths separately. Each case reserves enough
capacity outside the timed loop; worst-case subnormal and maximum radix outputs are distinct groups.
No “allocation-free” claim is made without allocator instrumentation.

### Phase 6 — Release candidate and canaries

Dependencies: Phase 5.

Deliverables: `README.md`, API examples, `COMPATIBILITY.md`, `DIVERGENCES.md`, changelog, package
listing, release workflow, archived parity/benchmark artifacts.

`.github/workflows/publish-rust.yml` is separate from the existing npm workflow. It runs only for a
protected manual/release environment, verifies that the Git tag and `Cargo.toml` version agree,
reruns pinned correctness and package gates, runs `cargo publish --dry-run --locked`, uploads the
exact `.crate` and SHA-256 as artifacts, and then runs `cargo publish --locked`. Authentication uses
a crates.io trusted publisher when configured, or a repository-scoped publishing token held by the
protected environment. The workflow never invokes `npm publish`.

Canaries:

1. Use the packaged `.crate` in at least two downstream sample applications via a local registry or
   unpacked path, one default-feature and one all-feature consumer.
2. Run one multithreaded formatter workload and one locale-heavy workload.
3. Build the packaged crate, not the workspace source.
4. Publish `0.1.0-rc.1` only after the package name is rechecked and credentials/trusted publishing
   are configured.

Exit: RC remains green through the scheduled full differential and downstream smoke tests. Stable
`0.1.0` is a separate approval.

Rollback: do not change or unpublish the npm package. If an RC or stable Rust release is defective,
yank the crate version and publish a fixed patch or new RC; crates.io versions are immutable and are
never overwritten. Downstream canaries pin the previous known-good version.

### Phase 7 — Maintenance

Toolchain, Node oracle, dependency, or upstream d3 updates occur in isolated pull requests:

1. Update one source of truth.
2. Regenerate provenance and fixtures.
3. Review byte-level output drift.
4. Run all structured and pre-release differentials.
5. Update compatibility/deviation documentation.

Criterion remains exactly pinned until its C-build dependency policy changes or the project
deliberately adopts another benchmark harness. `cargo tree` guards the actual graph.

## 8. Compatibility and intentional deviations

`DIVERGENCES.md` must list at least:

- no mutable process-global default locale;
- no stale global SI-prefix leak;
- immutable typed `FormatSpecifier`; no field coercion or ToInt32 mutation behavior;
- parse/construction-time width limits and fallible allocation;
- numeric/text input mismatch errors instead of JavaScript coercion;
- rejection of non-BMP/newline fill characters;
- validated locale grouping and numeral shape.

Everything not listed as a deviation is subject to the byte-for-byte differential contract,
including type `d` infinity, negative zero, unknown types, exponent spelling, Unicode minus and
micro signs, RTL marks, grouping cycles, and final numeral substitution.

`COMPATIBILITY.md` records:

- d3-format version and commit;
- exact Node/V8/ICU/Unicode/CLDR oracle;
- fixture schema and generator versions;
- Rust MSRV and pinned validation toolchain;
- supported targets and feature combinations;
- latest full differential manifest and known deviations.

## 9. Objective release checklist

- [ ] Phase 0 machine and gate commands pass.
- [ ] Oracle metadata exactly matches `oracle.json`.
- [ ] Generation is byte-deterministic in two clean runs.
- [ ] Coverage reports 729/729 static sites, 1,078 dynamic executions, and 793 calls.
- [ ] Fixed and significant exact-tie manifests contain the expected precision/bucket coverage.
- [ ] All numeric primitive and carry corpora have zero mismatches.
- [ ] Generated, ported, property, allocation, and concurrency tests pass.
- [ ] Full formatter and all-locale differentials have zero mismatches.
- [ ] MSRV, pinned, stable, GNU Windows, Ubuntu, feature, and core WASM compile gates pass.
- [ ] Benchmark and parity artifacts are archived.
- [ ] `cargo package` contents are reviewed and build independently.
- [ ] Downstream canaries pass against the packaged crate.
- [ ] Compatibility, deviations, release, and rollback documents are complete.

## 10. Optional future WASM/JavaScript boundary

This section is not a Phase 0–7 deliverable.

A future adapter should be a separate crate/package, for example `rust/wasm/`, depending on
`d3-format`. It should expose owned numeric handles:

- `WasmLocale` owns a cloned Rust `Locale`;
- `WasmFormatter` owns a Rust `Formatter`;
- constructors return JavaScript exceptions for `ParseError`, limits, and input mismatches;
- JavaScript wrappers recreate currying only at the JS layer;
- explicit number and text methods avoid accidental coercion;
- locale definitions cross the boundary through a validated plain object or JSON schema;
- Node and browser tests compare the actual WASM package against the same versioned oracle;
- bundle size, initialization time, memory growth, and UTF-8/UTF-16 conversion are measured.

Only after that boundary passes its own compatibility, browser, packaging, and rollout gates may an
npm migration be proposed. The native crate's success does not imply that replacement is safe.
