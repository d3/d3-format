# Porting d3-format 3.1.2 to Rust

This is the implementation plan for a native Rust crate that reproduces d3-format 3.1.2. It is
also the compatibility contract for the port.

> **How to read this document in this repository.**
>
> The port was developed inside a fork of the JavaScript `d3-format` repository, so that the
> reference implementation, its test suite, and the oracle harness were all on hand. This document
> describes *that* repository: paths in it are relative to its root, where the Rust crate lived
> under `rust/`, the JavaScript oracle tooling under `rust/tools/`, the recorded oracle corpora
> under `rust/fixtures/`, the gate scripts and archived evidence under `rust/ci/`, and the
> downstream consumers under `canaries/`.
>
> This repository is the crate extracted from that tree. The crate is at the root here, so `rust/`
> drops out of every path, and the JavaScript oracle, the differential harness, the gate scripts,
> and the CI workflows stayed behind — they need the JavaScript reference implementation to run,
> and they are not part of the library.
>
> The document is kept because it is the design record: the source comments throughout the crate
> cite its section numbers for *why* a decision was made, and those citations resolve here. Read it
> as history and rationale, not as a description of this repository's layout. `COMPATIBILITY.md`
> is the current, self-contained statement of what the crate guarantees.

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
| Deterministic fixture/test generation | Gate — passed | §3.7 passes; two clean runs produced byte-identical artifacts; coverage is 729/729 static sites, 1,078 executions, 793 calls |
| Locale generator and drift check | Gate — passed | 59 locales emitted with per-source hashes; `--check` verifies byte identity |
| Differential runner | Deliverable — present | numeric primitives wired; formatter operations decline until Phase 4 rather than claiming agreement |
| Exact number conversion | Gate — passed | 2,344,808 differential cases agree with Node v24.18.0 under MSRV, pinned, and stable, with zero mismatches |
| Specifier, locale, and public API | Gate — passed | §5 surface implemented; parser checked against an independent matcher over 62,823 inputs |
| Types, layout, SI, and locales | Gate — passed | all 793 recorded d3 calls agree exactly; 2,655,832 differential cases with zero mismatches; 161 of 168 conformance blocks execute and pass |
| Rust CI jobs | Deliverable — present | `rust/ci/check.ps1` plus pinned, MSRV, stable, and WASM jobs; every step reproduced locally, never yet run on GitHub |
| Parity CI job | Deliverable — present | `parity` in `test.yml`; every step but the `actions/*` ones reproduced locally |
| Property and hardening tests | Gate — passed | 17 property tests; 525,121 values through 15,913 accepted specifiers with no panic; owned/reused equivalence, trimming, padding, all-locale, and allocation-budget properties |
| Per-PR differential | Gate — passed | 213,784 cases, zero disagreements, in debug |
| Pre-release differential | Gate — passed | 13,355,832 cases and 10,268,703 formatter comparisons, zero disagreements, 550 s in release |
| Benchmarks and baseline | Deliverable — present | `rust/benches/format.rs`, 7 groups and 41 benchmarks with allocator instrumentation; baseline archived, not a gate |
| Ubuntu and hosted-runner legs | Gate — pending | no Linux host locally; `ubuntu-latest` jobs, the hosted Windows `dlltool` assumption, and the Actions plumbing are unverified until a real run |
| Release documentation | Deliverable — present | `rust/README.md`, crate-level rustdoc, `COMPATIBILITY.md`, `DIVERGENCES.md`, `CHANGELOG.md`; every API example is a compiled doctest |
| Crate documentation examples | Gate — passed | `cargo test --locked --doc --all-features` runs 32 doctests, including every Rust block in `README.md`, and all pass; 31 with default features |
| `cargo package` review | Gate — passed | `rust/ci/package/collect-listing.ps1`: 56 files, 187 KiB compressed, each matched to a rule that says why it ships; verified to fail on an unclassified file and on a lapsed exclusion; the archived SHA-256 reproduced across five packagings of the commit it names and matches both the tarball on disk and the canary run |
| Downstream canaries | Gate — passed | two consumers built from the unpacked `.crate` on 1.98.1 and 1.80.0; ~1.14 M formatted values across the four runs, zero mismatches |
| Parity artifacts archived | Deliverable — present | `rust/ci/parity/ci-differential.json` and `prerelease-differential.json`; the runner's own summary stays ephemeral under `rust/target/` |
| Release evidence collected from a commit | Gate — passed | all four archives name `ba5b9ea`, whose tree they measured with `git_worktree_dirty: false`; each collector refuses a dirty tree, verified firing on one under PowerShell 5.1 and 7 |
| Working-tree line endings | Gate — passed | `rust/ci/assert-line-endings.ps1` checks all 88 tracked paths under the declared `eol=lf` roots; wired into `-Profile Pinned`, the `parity` job and `collect-listing.ps1`; verified firing on a packaged and an unpackaged file under PowerShell 5.1 and 7 |
| Cargo publication | Gate — pending | `publish-rust.yml` written and statically checked, never executed; `d3-format` is unclaimed on crates.io; the protected environment and credentials are a human step |

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
│   ├── prerelease-differential.yml             # scheduled 10,000,000-case differential
│   ├── publish.yml                             # existing npm publication; unchanged
│   └── publish-rust.yml                        # gated Cargo publication
├── canaries/                                   # Phase 6 downstream consumers; never packaged
│   ├── run-canaries.ps1
│   ├── default-consumer/                       # default features + multithreaded workload
│   └── all-features-consumer/                  # locales + serde + locale-heavy workload
└── rust/
    ├── .cargo/config.toml
    ├── Cargo.toml
    ├── Cargo.lock
    ├── rust-toolchain.toml
    ├── README.md
    ├── CHANGELOG.md
    ├── DIVERGENCES.md
    ├── COMPATIBILITY.md
    ├── ci/
    │   ├── check.ps1
    │   ├── assert-line-endings.ps1
    │   ├── benchmarks/
    │   │   ├── collect-baseline.ps1
    │   │   └── baseline.json
    │   ├── package/
    │   │   ├── collect-listing.ps1
    │   │   └── package-listing.json
    │   └── parity/
    │       ├── assert-agreement.ps1
    │       ├── collect-differential.ps1
    │       ├── ci-differential.json
    │       └── prerelease-differential.json
    ├── src/
    │   ├── lib.rs
    │   ├── specifier.rs
    │   ├── decimal.rs
    │   ├── bignat.rs
    │   ├── types.rs
    │   ├── layout.rs
    │   ├── locale.rs
    │   ├── precision.rs
    │   ├── limits.rs
    │   ├── bin/dump_cases.rs
    │   └── locales/generated.rs
    ├── tests/
    │   ├── gate.rs
    │   ├── generated/
    │   ├── ported/
    │   ├── concurrency.rs
    │   ├── decimal.rs
    │   ├── layout.rs
    │   ├── properties.rs
    │   └── public_api.rs
    ├── benches/format.rs
    ├── fixtures/
    │   └── d3-format-3.1.2/
    │       ├── manifest.json
    │       ├── corpus-manifest.json
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

`canaries/` is outside `rust/` deliberately. `cargo package` walks the crate directory, so a
consumer nested under `rust/` would be published unless `exclude` kept saying so, and an exclusion
is precisely the thing Phase 6's packaging collector exists to catch when it stops matching. Two
mechanical reasons agree: `rust/Cargo.toml` declares no `[workspace]`, so a nested package is a
cargo error until it is listed as a member; and `rust/rust-toolchain.toml` applies to anything built
from inside `rust/`, which would make the MSRV half of the canary impossible — rustup resolves that
file by walking up from the build directory, so only a consumer outside `rust/` can choose its own
compiler, which is the property §7 excludes `rust-toolchain.toml` from the package to preserve.

The `.gitattributes` rule stays scoped to `rust/`. `canaries/` is not covered because no
byte-comparison gate reads it: its Rust sources are compiled, never diffed, and `cargo fmt` is
insensitive to line endings.

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
canaries/** text eol=lf
```

The rule is scoped to `rust/` and `canaries/`. The JavaScript oracle keeps its existing checkout
behavior, because renormalizing it is an unrelated change to the reference implementation.

The rule governs checkout, and Phase 6's evidence rework found what it does not govern. Nine tracked
files under those two directories had CRLF in the working tree — among them `rust/README.md`,
`rust/CHANGELOG.md` and `rust/COMPATIBILITY.md`, all three of which are packaged. `cargo package`
reads the working tree rather than the index, so the crate's SHA-256 quietly depended on which files
an editor had last rewritten and would not have matched a package built from a fresh checkout of the
same commit. The release evidence was collected only after deleting and re-checking-out those paths
so that the tree byte-matched a fresh clone.

`rust/ci/assert-line-endings.ps1` is the gate for it. It reads the `eol=lf` patterns out of
`.gitattributes` rather than carrying a second copy of them, runs `git ls-files --eol` over exactly
those roots, and fails unless every tracked path reports the declared attribute and an ending of
`lf` in both the index and the working tree. An ending of `none` passes: git prints that for a file
with no line terminator at all, which cannot carry a CRLF either way, and
`rust/fixtures/d3-format-3.1.2/regressions.jsonl` is legitimately empty. A path that stopped
carrying `eol=lf` fails too, since a file the rule no longer covers has nothing left to disagree
with. Only tracked paths are checked, because only committed content has a checked-out form that can
drift from it.

It runs in three places, all of them cheap because it is one `git` command. `-Profile Pinned` runs
it immediately before `cargo package`, which puts it on `windows-latest`, where the condition
arises, and on `ubuntu-latest`, where it must keep being absent. The `parity` job runs it as its
first step. `collect-listing.ps1` runs it before it packages anything, which is the path that
produced the unreproducible hash, and `-AllowDirty` does not waive it: that switch says "measure
work in progress and mark the artifact", not "measure bytes the checkout rule forbids". It is not in
the `Msrv` profile, for the reason §6.1 keeps formatting and packaging out of it — the checked-out
bytes are a property of the checkout, not of the compiler.

The gate was watched failing rather than assumed to work. `rust/README.md`, which is packaged, and
`rust/ci/check.ps1`, which `exclude` drops, were each rewritten with CRLF and then both together;
the gate named each offending path with its index ending, its working-tree ending and its
attributes, and exited 1 identically under Windows PowerShell 5.1 and PowerShell 7. Packaging the
contaminated tree and the restored tree at the same commit produced two different crate SHA-256
values, which is the damage of §7's Phase 6 restated as a measurement rather than an argument.

Why no existing gate sees this was measured on git 2.55.0 rather than inferred, and the answer is
sharper than "git reports the file as clean". Which half of git stays quiet depends on how the CRLF
arrived. A file git itself checked out as CRLF — which is every file committed before
`.gitattributes` existed, on a host with `core.autocrlf=true` — still matches the stat data cached
in the index, so `git status` never reads its contents and reports nothing whatsoever. That is the
state the nine files were in, and it is why the collectors' clean-tree refusal could not have caught
them. Rewrite the same file with an editor and the cached size stops matching, so `git status` does
print ` M` — but `git diff` stays empty and `git hash-object` returns the committed OID, because the
`text` attribute normalizes the bytes back to LF on the way to the index. §3.7's drift clause sees
neither, being scoped to the three generated paths, none of which was among the nine.

`.github/workflows/*.yml` is deliberately out of scope. The four files carry no `eol` attribute and
are `w/crlf` on a Windows checkout; none was added, so the gate does not see them. The hazard cannot
reach them: nothing byte-compares them, `cargo package` walks the crate directory and never reaches
`.github/`, and Actions reads the committed blob — `i/lf` for all four — rather than anyone's
working tree. They are also shared with the JavaScript product that this rule deliberately leaves
alone; `publish.yml` is the existing npm publication listed above as unchanged. Declaring an
attribute for them would change checkout behavior outside the port to buy a guarantee nothing needs.

## 2. Phase 0: machine, toolchains, baseline, and gates

**Status: passed. §2.4 succeeds in this checkout and the §7 CI skeleton is written, with every step
reproduced locally. The skeleton has not yet executed on GitHub, so the hosted-runner assumptions in
§2.2 and §6 remain unconfirmed.**

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
This is host configuration, not a repository artifact. With it in place the real graph links
Criterion through `windows-sys` successfully, so `raw-dylib` is proven for this toolchain
combination — on this host, which is not evidence about the hosted image.

GitHub's `windows-latest` images are believed to ship MinGW binutils, but that has not been
confirmed against a real run and remains the highest-risk unverified assumption in the Windows GNU
job. Rather than leave it silent, the job makes the assumption self-diagnosing: a step reports
whether `dlltool` is on `PATH`, adds it from the known install locations if it is merely absent from
`PATH`, and otherwise emits a warning and lets the real linker error surface rather than masking it.
If the image genuinely lacks binutils the job fails with the same `dlltool.exe: program not found`
error recorded above, now preceded by a step that says so plainly, and the fix is to install MinGW
binutils in the workflow. Confirm on the first CI execution. If GNU linking fails for some other reason, capture
the complete linker command and error before changing toolchains.

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

**Status: passed. The production tools in `rust/tools/` supersede the prototype, which remains
untracked evidence.**

### 3.1 Oracle identity

Node 24 as a moving major is not a pin. CI and fixture generation use exactly Node `24.18.0`.

The pre-existing `test` job keeps `node-version: 24` deliberately. That job tests the JavaScript
product's own compatibility across the major, where a moving version is the point; oracle identity
is satisfied by `parity`, which runs the same vitest suite at the exact pin.
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

Two recording conventions are part of the value-encoding contract, not incidental choices. A
rendered specifier is tagged `kind: "specifier"` rather than `"text"`, and `toLocaleStringEn` is
recorded already stripped of grouping separators, in the form §4.1 says type `d` consumes. Both were
observed to present as false engine drift when a second implementation of the JavaScript side
assumed otherwise, so any reimplementation must reproduce them exactly.

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

Boundary calls have a third category the bullets above do not anticipate. Seven of the 793 calls are
made in a test block's setup position and belong to no static assertion site. They are represented
by the generated test for their block, so a call carrying a null assertion ID is accepted when it
names a test block, and rejected when it names neither. This is a real structural property of the
suite, not a waiver.

### 3.4 Generated tests and locales

`gen-tests.mjs` consumes only versioned fixture files and emits ASCII-only Rust source under
`rust/tests/generated/`. It emits one test per original test block, preserving the source title and
including assertion IDs in failure messages. Generated files are never edited manually.

Generators emit rustfmt-shaped output directly. §6.1 runs `cargo fmt --check` across the whole
crate while §3.4 forbids hand-editing generated files, so generated Rust must already satisfy
rustfmt. It must not be produced *by* rustfmt: piping generated source through the installed
formatter would make committed bytes depend on that formatter's version, which is precisely what the
byte-identity gates cannot tolerate. The generators therefore reproduce the small set of layout
rules the emitted shapes can reach, and `cargo fmt --check` is the proof. This couples the
generators to rustfmt's default style, and a style-edition change will break it — loudly and
attributably, which is the acceptable failure mode. The escape hatch, if that coupling ever costs
more than it earns, is to exempt generated paths from §6.1's formatting check rather than to start
running rustfmt during generation.

Until the formatting API exists, generated tests must compile without being able to report success.
They carry `#[ignore]` with a reason naming the phase that will implement them, and their bodies
diverge through `unimplemented!()`. A cargo feature gate is the wrong mechanism: it hides the tests
from `--no-run`, which is the very thing §3.7 gates on. Because a summary-only reader sees just
`test result: ok`, each placeholder target also carries one non-ignored test whose *name* states
that the port is unimplemented, asserted against its placeholder seam so the claim fails the moment
it stops being true.

§6.1's `Pinned` profile additionally asserts the conformance **inventory** of each placeholder
target — the number of conformance tests defined, ignored and not — against the count recorded in
the generated artifacts, so the scoreboard survives summarization into a pass or a fail. It reports
the ignored count alongside, as the migration's progress metric, but does not gate on it.

It must gate on the inventory rather than on the ignored count, and getting this wrong made two
parts of this document unsatisfiable at once. §7's Phase 3 exit requires the hand-ported tests to
pass, while a guard keyed to the ignored count requires them to stay ignored, and no artifact
Phase 3 is allowed to touch can record the transition. Test existence is a stable property that a
generator records; how many of those tests currently pass is a moving one that changes with every
phase.

The two mechanisms still catch different mistakes. Un-ignoring a test before its API exists is
caught by the test run, which panics on the placeholder seam. A *deleted* generated test leaves
everything passing and is caught only by the inventory assertion.

Counting the inventory needs care, because each target also holds scaffolding tests that are not
conformance blocks. The guard applies two independent discriminators — declaration inside a
generated or hand-ported submodule, and the `tNNN_` name the generator controls — and fails if they
disagree, since they drift in opposite directions: a helper added inside a submodule inflates the
first, a change to the naming scheme deflates the second. It also requires every test name
`assertion-map.json` routes to still be defined, which catches a rename that no count can see. A
convention change is then a diagnosable failure that names the offending tests, rather than a
silently wrong count.

The recorded count for the generated target is `manifest.json`'s test-block count, not the number of
tests named in `assertion-map.json`. Those differ: the map names 155 generated tests against 168
blocks, because a block whose assertions are all ported or all divergent still emits a block test
while routing no assertion to it. The map is the correct source for the ported target, which exists
only to receive assertions the map routes to it.

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
cargo test --locked --manifest-path rust/Cargo.toml --no-run
cargo test --locked --manifest-path rust/Cargo.toml --all-features --no-run
$status = git status --porcelain -- rust/fixtures rust/tests/generated rust/src/locales/generated.rs
if ($status) { throw "generated artifacts are dirty:`n$status" }
pwsh rust/ci/assert-line-endings.ps1
```

The all-features build is not redundant. With only the default build, `--features locales` went an
entire phase without ever compiling: `src/lib.rs` re-exported a private module, which is a hard
`error[E0365]`, and no gate in §3 touched that feature. A generated-code gate that builds one
feature combination proves only that one combination generates. Any gate that builds generated
output builds every combination that generated output can reach.

The line-ending gate joins this block because it checks the one kind of drift the clause above it
cannot: `git status` compares the working tree against the index through the `text` attribute, which
normalizes CRLF away before the comparison happens, so a checked-out file that does not byte-match
what a fresh clone would produce is invisible to it. §1 states what the gate checks and why the
scope stops where it does. It is the only command here that is not part of Phase 1's own toolset,
because the hole it closes was not found until Phase 6.

The drift clause is a check against committed artifacts, so it cannot pass on the commit that
first introduces them: every generated path is reported as untracked until it is committed. On that
one commit, commit first and then run the gate. Every later run, including all of CI, evaluates it
as written. The clause is deliberately not narrowed to modified-and-deleted entries, because in a
committed tree a regenerated file appearing as untracked is itself real drift and must fail.

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
- preserve the ECMAScript spellings of zero and non-finite values;
- **break an exact tie toward the even digit**, which is the opposite of the rule above.

That last bullet is a correctness trap, not a detail. `toFixed`, `toExponential`, and `toPrecision`
round half away from zero; shortest conversion rounds half to even. The two must not be unified
behind one rounding helper. At `2^50 + ¼` shortest conversion yields `1125899906842624.2` while
`toFixed(1)` yields `1125899906842624.3`. This was found by a differential mismatch rather than by
reading the specification, so a port that reasons only from §4.1's `toFixed` wording will get it
wrong and the existing suite will not notice.

Integer radix conversion has a related asymmetry. V8's `Number.prototype.toString(radix)` is an
approximation outside the power-of-two radices: it stops emitting once the remainder falls below
half an ulp and pads the rest with zeros, which the specification permits because it requires only
"a generalization of" `Number::toString` for a radix other than ten. `(1e21).toString(36)` is
`5v1j4f4ds7c000` in Node against an exact `5v1j4f4ds79m9s`. Across 20,000 large integers, radices 2,
8, and 16 matched the exact expansion every time while radix 36 drifted in 9,389 of them, because a
binary64 integer's low digits genuinely are zero in a power-of-two radix. **d3 reaches only radices
2, 8, and 16, through types `b`, `o`, `x`, and `X`, so no d3-compatible output is affected.** The
port emits the exact expansion at every radix and the differential declines the radices d3 cannot
reach, rather than asserting an agreement it has not demonstrated. The choice is recorded in
`DIVERGENCES.md` because the conversion is reachable beyond those three radices.

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

These signatures name `FormatLimits` and `FormatError`, which §7 lists as Phase 3 deliverables. That
ordering is inconsistent, and Phase 2 resolves it by shipping the minimum each type needs now:
`FormatLimits` in its final shape, so no Phase 3 change reshapes a signature, and only the
`FormatError` variants a numeric conversion can actually produce. `FormatError` is `#[non_exhaustive]`
so Phase 3 adds the specifier, width, and input-kind variants without a breaking change.

The engine lives in a module that is deliberately **not** part of the §5 public API. It is reachable
because §3.6's differential harness is a separate binary target and cannot see crate-private items,
so it is `#[doc(hidden)]` and documented as unstable. Phase 6 may promote or seal it; until then it
carries no compatibility or semver guarantee, which is what keeps these ECMAScript primitives from
silently becoming published surface the plan never specified.

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

These corpora are the only thing standing between the port and the most likely error. §0.3 records
that a half-even simulation passes all 1,078 committed assertions; the generated ties, by contrast,
contain 2,144 fixed and 4,319 significant cases where half-even and ECMAScript produce literally
different strings. That set of 6,463 discriminating cases, not the ported suite, is what establishes
the rounding rule.

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
- exact `Math.pow` scale bits and multiplication order for `formatPrefix`;
- `Math.max` and `Math.min` returning the negative NaN bit pattern.

Grouping *cycles* through its sizes rather than repeating the last one, and the consequence is easy
to get backwards: `[3, 2]` renders ten digits as `12,345,67,890`, not as the Indian
`1,23,45,67,890`. A hand-written expectation is not evidence here; the oracle is.

Numeral substitution running last over the fully assembled string is observable, not an internal
detail. Under the Arabic locale, `format("#x")(48879)` is `"٠xbeef"` — the `0` of the `0x` base
prefix is substituted while the hex digits are not, which is true only in that order.

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
exclude = ["ci/", "fixtures/", "src/bin/", "tools/", "rust-toolchain.toml"]

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

Rechecked in Phase 6: both `d3-format` and `d3_format` return 404 from the crates.io API and from
the sparse index, and crates.io treats hyphen and underscore as the same name, so the name is
unclaimed. The manifest now carries `0.1.0-rc.1` rather than the `0.1.0` above; see Phase 6 in §7.
`Cargo.lock` was regenerated for the bump under the pinned toolchain, as §1 requires, and the only
line that changed is this crate's own version.

`exclude` keeps the published artifact independent of the verification and CI tooling. Without it
the `.crate` carries `ci/check.ps1` and `fixtures/`, and the latter is what matters: Phase 1 grows it
with `calls.jsonl`, `decimal.jsonl`, the tie and carry corpora, and the assertion map, and Phase 5
sizes it in millions of cases. The package would then
cross the crates.io 10 MiB limit and fail §6's packaging gate for a distribution reason unrelated to
correctness. `rust-toolchain.toml` is excluded for a separate reason: rustup resolves it by walking
up from the build directory, so shipping it would force channel 1.98.1 on anyone building the
unpacked crate, defeating §9's canary that the package builds under a consumer's own toolchain.
`tests/` stays packaged because `gate.rs` has no fixture dependency and §9 requires the packaged
crate to build and test independently; whether generated tests remain packaged is decided in Phase 1
against their measured size.

Phase 6 measured it and they stay. The whole package is 56 files, 1,108,967 bytes uncompressed and
191,741 compressed — under 2% of the crates.io limit — of which `tests/` is 36 files and 752,568
bytes. The generalized version of "no fixture dependency" turned out to hold for every test target,
not only `gate.rs`: nothing under `tests/` reads a file at run time, because `gen-tests.mjs` inlines
every oracle value and `properties.rs` generates its own. That is what makes a packaged suite
runnable with `fixtures/` excluded, and it is the property that would have to be rechecked before
adding a test that loads a corpus from disk.

`src/bin/` is excluded so the differential harness does not become a published command. Without it
`cargo install d3-format` installs a `dump_cases` binary from a formatting library. Excluding the
source is better than gating it behind a feature: cargo drops the target from the normalized
manifest entirely, leaving a genuinely library-only package rather than one carrying a binary that
is merely hard to build, and it avoids adding a public feature whose only purpose is to suppress a
development tool. `dump_cases` remains fully usable from the repository, which is the only place
§3.6 invokes it.

`.cargo/config.toml` ships, and that is a decision rather than an oversight. It is inert for an
ordinary registry consumer, because cargo builds dependencies from the registry cache and discovers
configuration from the consumer's own working directory. It applies only in §9's canary, which
builds the unpacked crate in place, and there MSRV-aware resolution is exactly the wanted behavior:
a `cargo update` in that directory would otherwise reselect the `clap` release Rust 1.80 cannot
parse. This does not contradict excluding `rust-toolchain.toml`. A toolchain pin forces a specific
compiler on the consumer and fails outright when it is absent, whereas the resolver key only makes
dependency selection respect `rust-version` fields that are already declared.

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

JavaScript's round trip is also lossy *without* mutation, which is a stronger statement than the
paragraph above. `FormatSpecifier.prototype.toString` spells the width as
`Math.max(1, this.width | 0)`, so a width at or above 2³¹ wraps: Node renders `4294967295f` as
`" >-1f"` and `.99999999999999999999f` as `" >-.1661992960f"`. The port renders what it parsed. No
*formatted output* differs, because both clamp into range before use, but the spelling does, so this
belongs in `DIVERGENCES.md` rather than being treated as a parser bug.

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

These helpers must not use `f64::max`. JavaScript's `Math.max` propagates NaN and Rust's `f64::max`
discards it, so `0.0_f64.max(f64::NAN)` is `0.0`. Substituting it converts every NaN answer the
contract requires into zero, and does so invisibly: any suite exercising only finite steps still
passes. The helpers use a NaN-propagating maximum of their own.

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
$ErrorActionPreference = 'Stop'

function Invoke-Cargo {
    param([Parameter(ValueFromRemainingArguments)] [string[]] $Arguments)

    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

Invoke-Cargo fmt --check
Invoke-Cargo clippy --locked --all-targets --all-features -- -D warnings
Invoke-Cargo test --locked --no-default-features
Invoke-Cargo test --locked --features locales
Invoke-Cargo test --locked --features serde
Invoke-Cargo test --locked --all-features
Invoke-Cargo bench --locked --no-run
Invoke-Cargo package --locked

$metadata = cargo metadata --locked --format-version 1 --all-features |
    ConvertFrom-Json
$forbidden = @($metadata.packages | Where-Object { $_.name -in @('cc', 'alloca') })
if ($forbidden.Count -ne 0) {
    foreach ($package in $forbidden) {
        cargo tree --all-features --invert $package.name
    }
    throw 'a forbidden C-backed benchmark dependency entered the graph'
}
```

Three details in that script are load-bearing.

**Every native exit code is checked.** A non-zero exit from `cargo` does not stop a PowerShell script,
and `$ErrorActionPreference` alone does not change that for native commands on Windows PowerShell
5.1. Written without the wrapper, this script reports success after any step fails, which is worse
than having no script. The check is verified by forcing a mid-script failure and confirming the run
stops there and exits non-zero.

**`--locked` is mandatory on every command that resolves dependencies**, including `cargo metadata`,
which rewrites `Cargo.lock` when it judges the lock stale. The migration rests on
byte-reproducibility, and a run that silently re-resolves is not reproducing the committed state.

This was measured, not assumed. Against a deliberately stale lock, `--locked` fails with
`cannot update the lock file ... because --locked was passed` and exit 101, leaving the lockfile
byte-identical. The same command with the flag removed exits **zero** and rewrites the lock,
admitting `alloca` and `cc` — precisely the two packages §0.3 pins Criterion to keep out. The
unlocked path therefore converts the guard's own subject matter into a silent success.

Two commands are excluded deliberately. `cargo fmt` does not merely have no need for the flag, it
rejects it outright with `unexpected argument '--locked'` and exit 2, so adding it breaks the
profile. `cargo tree` in the diagnostic path is left unlocked and unchecked because it runs only on
the way to a failure that has already been decided; a lock error there would suppress the
explanation rather than add one.

**The forbidden-dependency diagnostic iterates over what was actually found.** Against a clean graph
a literal `cargo tree -i cc` exits 101 with `package ID specification 'cc' did not match any
packages`, so in the `alloca`-without-`cc` case the hardcoded form produces an error where the
diagnostic should be. The loop reduces to exactly one `cc` invocation in the case §0.3 anticipates.
It passes `--all-features` so the tree matches the graph the guard just inspected; otherwise an
offender arriving through an optional feature comes back "not found".

The profile name selects a check set, not a toolchain: per §6.2 the current-stable job also runs
`-Profile Pinned`. A lighter MSRV profile runs the four feature-combination tests only, because
formatting, lint conformance, packaging, and dependency-graph policy are properties of the source
and lockfile rather than of the compiler, and pinning Clippy conformance to an older toolchain
invites failures unrelated to MSRV.

`cargo package` refuses to run against a dirty crate directory, so the literal invocation above
fails locally whenever work is in progress. The committed script takes an `-AllowDirty` switch for
local use; CI checks out clean and never sets it.

That refusal is narrower than it sounds, and Phase 6 measured which half is which rather than
leaving it to inference. Cargo counts only the files it would actually package: an untracked
`src/probe_dirty_check.rs` fails with `1 files in the working directory contain changes that were
not yet committed into git` and exit 101, while a modified `ci/parity/ci-differential.json` — a path
`exclude` drops — packages successfully and exits zero. So this profile still passes with
regenerated release archives sitting uncommitted under `ci/`, which is what lets the gate run and
the evidence reach a reviewer as separate steps. The switch stays necessary for work in progress on
a packaged path.

The Windows entry tests the same GNU host selected for local development. An additional MSVC job may
be added as compatibility coverage, but it does not replace the GNU job.

The profile also runs `cargo bench --locked -- --test` after `--no-run`. Compiling a benchmark is
not running it, and the smoke run is the only thing that catches a benchmark which builds and then
panics on its first iteration.

It then runs `ci/assert-line-endings.ps1`, immediately before packaging. §1 says what the gate is
for; it belongs at this point in this profile because `cargo package` is the step that reads the
working tree, and because `Pinned` is the only profile that runs on both a Windows and an Ubuntu
host. It stays out of `Msrv` for the same reason formatting and packaging do.

One PowerShell detail cuts across every script in this section and is worth stating once, because it
destroys a diagnostic at exactly the moment the diagnostic matters. PowerShell does not flatten an
array written inside an array literal, so `@('header:', ($items | ForEach-Object { "  $_" }))`
has two elements and `-join` renders the second as `System.Object[]` — and only when `$items` holds
more than one, so any single-item check of the message passes. Four diagnostics were built that way:
the two conformance-inventory messages in `check.ps1`, and the missing-estimate and stale-estimate
messages in `ci/benchmarks/collect-baseline.ps1`. Each now appends to the message array instead, as
the collectors' dirty-tree message already did, and each was rendered with more than one item under
both Windows PowerShell 5.1 and PowerShell 7. The edit is confined to the failure branch that builds
the string: no collector measures or writes anything differently, which is what lets
`baseline.json` stay valid rather than needing a forty-minute regeneration.

### 6.2 MSRV, stable, features, and WASM compile

Separate Ubuntu jobs install:

- `1.80.0-x86_64-unknown-linux-gnu` and run tests for no-default, `locales`, `serde`, and
  all-features;
- current `stable-x86_64-unknown-linux-gnu` and run the full pinned profile;
- current stable plus `wasm32-unknown-unknown`, then
  `cargo check --target wasm32-unknown-unknown --no-default-features`.

`rustup target add` installs into whichever toolchain is active **at that moment**, so it must run
with the job's `RUSTUP_TOOLCHAIN` already set, not merely before the build. Run at the repository
root it targets the default toolchain, while the build runs from `rust/` where the toolchain file
selects a different rustup toolchain; the target is then missing and the build fails with
`error[E0463]: can't find crate for 'std'` even though both toolchains resolve to the same compiler
version.

This gate passes as of Phase 0. That result is weaker than it sounds: under `--no-default-features`
the crate has no dependencies and no implementation, so it shows only that nothing in the manifest
or crate configuration blocks wasm. It begins constraining real code in Phase 2.

The stable job is intentionally moving compatibility coverage; it does not define fixture
semantics. Pinned 1.98.1 defines the reproducible implementation baseline.

As of Phase 5, 1.98.1 is still itself current stable, so the stable job builds the same compiler as
the pinned job and adds no independent coverage yet. That is expected and self-correcting once stable
advances; the job is not evidence of forward compatibility until then. Only the MSRV job currently
exercises a genuinely different compiler.

### 6.3 Oracle and parity job

```yaml
parity:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6
    - run: ./rust/ci/assert-line-endings.ps1
      shell: pwsh
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
    - run: node rust/tools/gen-corpus.mjs --check-manifest
    - run: node rust/tools/differential.mjs --corpus ci --seed 1 --cases 200000
    - run: |
        $status = git status --porcelain -- rust/fixtures rust/tests/generated rust/src/locales/generated.rs
        if ($status) { throw "generated artifacts are dirty:`n$status" }
      shell: pwsh
```

`ci` and `prerelease` are **named** corpora, not paths; `--corpus` accepts either, disambiguating a
path by a separator or a `.jsonl` extension. `--cases` sizes the *drawn formatter section*, and the
corpus's fixed structured sections run in full on top of it. That definition is what makes §7's two
exit clauses consistent under one flag: a per-PR run of 200,000 drawn cases and a pre-release run of
10,000,000 drawn cases *plus* the complete structured corpora.

Roughly one drawn specifier in fifteen is type `c`, which is a policy check rather than an output
comparison, so a drawn count is not a comparison count. The pre-release corpus declares a minimum
output-comparison count and asserts it, otherwise a corpus change that quietly converted comparisons
into declines would pass.

Three steps beyond §6.3's original list are load-bearing. `gen-corpus.mjs --check-manifest` gates the
bucket census, which §4.3 treats as gate material but which no workflow previously checked for
staleness. §3.7's drift clause is repeated here because the `--check` tools prove that generated
bytes match a fresh generation and cannot catch a generated file that is present but untracked. The
job also sets `RUSTUP_TOOLCHAIN` explicitly rather than inheriting the image default, which removes
the ambiguity §6.3 warns about at its source. The line-ending gate is first because it costs one
`git` command and everything after it byte-compares something; on this runner it is the `i/crlf`
half it can find, since checkout writes LF here whatever the attribute says, while the
`windows-latest` leg of §6.1 is where a `w/crlf` finding is reachable. Same command, and it has to
hold in both places.

The differential runs in **debug** here. Overflow checks and debug assertions stay live, which is
the stronger reading of "no accepted input panics", and it costs about 2.8× — well under a minute
for the whole per-PR corpus.

The parity runner invokes the Rust binary with `--manifest-path rust/Cargo.toml` or sets its child
working directory to `rust`; it does not rely on toolchain discovery from the repository root.

A scheduled workflow runs the versioned pre-release corpus, at least 10,000,000 formatter
comparisons, and uploads the seed, manifest, mismatch artifacts, and summary. Unchanged state is not
success unless the command completed and reported the expected case count.

It lives in `.github/workflows/prerelease-differential.yml`, a fourth workflow file beyond the three
§1 lists. A separate step re-asserts the counts **out of the written summary artifact** rather than
trusting the runner's exit status: zero findings, zero unimplemented, an empty census-problem list,
the output-comparison floor, drawn equal to asked, every declared structured section non-empty, and
the locale floor both applied and met. That step is itself verified against a mutated summary and a
missing summary, because an assertion nobody has seen fail is not known to work.

The evidence it produces is a CI artifact, not a committed file: the summary is written under
`rust/target/`, which is ignored. Phase 6 owns archiving a release's artifacts.

Phase 6 settled that division rather than moving it. The runner's own summary stays exactly where it
is and stays ephemeral: `differential.mjs` still writes `rust/target/differential/<corpus>-summary.json`,
the parity job and the scheduled workflow still assert against and upload that file, and nothing
reads a committed copy. What Phase 6 adds is a separate, committed *archive* per corpus —
`rust/ci/parity/ci-differential.json` and `rust/ci/parity/prerelease-differential.json` — written by
`rust/ci/parity/collect-differential.ps1`, which runs the differential itself, refuses a summary the
run did not just produce, refuses to archive a run that did not report exact agreement, and adds the
provenance the summary does not carry: commit, worktree cleanliness, Node, rustc, cargo, profile,
host, and elapsed time. The summary block inside the archive is the runner's output copied verbatim,
so the archive cannot drift from the run it claims to describe. The archives are records, not gates;
no CI job reads them. Duplicating the file into the tree would have created a second source of truth
that a later run could silently contradict, and leaving the evidence only in a CI artifact would
have made a release's parity unreadable once the retention window closed — the archive is the third
option and is the one that keeps a reader able to answer "what did the RC actually agree on" from
the repository alone.

**Release evidence is collected from a clean tree, and the collectors enforce it.** An archive is
worth exactly the tree it names, and the first set produced under this scheme was worth nothing:
all four artifacts — both parity archives, the packaging listing and the benchmark baseline — were
collected from Phase 5's commit plus the whole of Phase 6 uncommitted, a tree that exists nowhere in
history. Three of them recorded `git_worktree_dirty: true` and nothing acted on it. The packaging
listing shows why that is fatal rather than untidy: `cargo package` writes the commit into
`.cargo_vcs_info.json`, so the crate hash is a function of the commit, and the archived SHA-256 was
one no commit could ever produce. Each of `collect-listing.ps1`, `collect-differential.ps1` and
`collect-baseline.ps1` now reads `git status --porcelain --untracked-files=normal` before it
measures anything and again before it writes the result, and refuses at either point unless
`-AllowDirty` is passed. The second check is not redundant: it is what makes `git_worktree_dirty:
false` a property of the whole collection rather than of its first second. `-AllowDirty` is for
local experimentation and marks what it produced, through `provenance.allow_dirty`; an artifact
carrying it is not release evidence. Both collectors also re-read `HEAD` at the end and refuse if it
moved during the run. The refusal was exercised on a genuinely dirty tree, for all four artifacts,
under both Windows PowerShell 5.1 and PowerShell 7 — an assertion nobody has seen fail is not known
to work, and this one had never fired.

The ordering that creates is worth stating plainly, because it reads as a defect and is not.
`git_commit` names the commit of the tree that was *measured*, and the artifact recording that
measurement necessarily lands in a *later* commit; no artifact can name the commit that contains it.
What makes the record usable is `git_worktree_dirty: false` beside it, because that is the claim
that the measured tree was exactly the named commit and a reader can check it out and reproduce the
result. Each artifact now says this in its own `notes`, so a reader who meets one without this
document can still read its provenance. `prerelease-differential.json` had been missing
`git_worktree_dirty` entirely while the other three carried it; provenance fields are uniform across
all four, since a field that exists to make an artifact traceable is worth nothing on three artifacts
out of four.

Collecting all four in one sitting needs one accommodation, because the first artifact written makes
the tree dirty for the next collector. Either commit between collections, or point each collector's
`-OutputPath` at a scratch directory under the gitignored `rust/target/` and move the four into
place once they are all done. The second is what Phase 6's regeneration did, so that all four name
one commit and all four measured it clean.

The per-locale floor is half the mean, floored, applied only once the mean per locale reaches 200.
That threshold comes from a Chernoff bound — for Binomial(n, 1/59), P(count < μ/2) ≤ exp(−μ/8), so
at μ = 200 the union bound across 59 locales is below 1e−9 and a correct uniform draw cannot fail by
chance. Below that many draws the runner states outright that the floor did not apply, and a named
corpus is refused at parse time if `--cases` is small enough to reach the lenient path. "At least
zero" is not an assertion, and this is what keeps one from silently occurring.

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

"All generated tests pass" means every block the oracle recorded something replayable for. A block
is ignored only when it recorded no boundary call, or when every call it made passes a JavaScript
value the typed API declines; its assertions are then covered in `rust/tests/ported`. That rule is
derived from the recorded calls rather than hand-maintained, and the generated target asserts in
Rust that the emitted ignore table matches the rule, so a block that *could* run cannot sit skipped.

The concurrency stress test is `rust/tests/concurrency.rs`. Its specific target is d3's
`prefixExponent` module global: it must format from several threads through `formatPrefix` with
adjacent work items in different SI buckets, where a leak appears as one call's suffix on another's
output. It also covers per-call compilation, one `&Formatter` shared across threads, and a locale
moved off its building thread.

The 59-locale differential draws from every named locale. Per the same rule §4.3 applies to corpus
buckets, a locale drawing zero cases is a gate failure rather than a pass, so the runner records
per-locale counts and asserts a floor.

No accepted input panics is established by sweep, not by inspection: parse the specifier corpus,
compile every accepted specifier, and lay out awkward `f64` bit patterns and strings through each,
treating any error — not merely a panic — as a failure.

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

Reserving capacity outside the timed loop is asserted, not assumed: each reused case checks that the
buffer's capacity is unchanged after the run. A `String`'s capacity moves if and only if it
reallocated, so this is proof rather than inference, and it is what stops the reused group from
quietly reverting into the owned one.

The instrumentation is a counting global allocator wrapping `System`. It records `alloc` and
`alloc_zeroed` calls, `realloc` calls separately so that a growth-heavy path cannot advertise itself
as allocation-free while repeatedly copying, and fresh bytes requested. Each case is sampled twice
and any count the repeat does not reproduce is marked. Recording is gated off during the timed
portion, so the timings are not identical to those of an uninstrumented build.

What that licenses is exact per-call `GlobalAlloc` counts for one build. It does not license any
general "allocation-free" statement, anything about peak resident memory or fragmentation, or any
claim about a different allocator, optimization level, or toolchain. Measured: the reused path makes
exactly one fewer allocation than the owned path in every case, and only the type `c` text case,
which performs no numeric conversion, reaches zero. Every numeric path allocates, which is what
§5.3 already states.

The archived baseline is explicitly **not** a regression gate and no CI job reads it. Every field is
read out of the run rather than transcribed, and the collector fails if a defined benchmark has no
estimate or a stale one. For small outputs the owned/reused delta is within run-to-run spread and
individual pairs can invert; read the direction across the group, never a single pair.

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

Canaries 1 to 3 are executable without publishing anything and live in `canaries/`; see §1 for why
they sit outside `rust/`. Both consumers depend on `canaries/.packaged/d3-format`, which
`run-canaries.ps1` deletes and re-extracts from the tarball on every run, and the script reads
`cargo metadata` back to assert that the manifest cargo resolved really is the extracted one — a
path dependency that quietly pointed at `rust/` would otherwise pass while proving nothing. Each
consumer then runs on the pinned toolchain *and* on 1.80.0, which is the only place the
`rust-toolchain.toml` exclusion above is worth anything.

`rust/Cargo.toml` carries `0.1.0-rc.1` rather than §5.1's `0.1.0`. `publish-rust.yml` refuses to
publish unless the tag and the manifest agree, and they cannot agree on `v0.1.0-rc.1` while the
manifest says `0.1.0`. Stable `0.1.0` is a separate approval and a separate version bump.

`cargo package` review is an executable rule table rather than a read-through:
`rust/ci/package/collect-listing.ps1` requires every packaged path to match exactly one rule stating
what it is and why a consumer needs it, re-asserts each `exclude` entry by name, cross-checks the
tarball against `cargo package --list`, checks the crates.io size limit, and archives the listing
with a per-file SHA-256. A file added to the crate directory therefore cannot join the distribution
without someone classifying it. Both failure modes were exercised: an unclassified file, and an
`exclude` entry removed so that `rust-toolchain.toml` reappeared.

All four archived artifacts — `ci/package/package-listing.json`, `ci/parity/ci-differential.json`,
`ci/parity/prerelease-differential.json` and `ci/benchmarks/baseline.json` — are collected from a
clean tree, and their collectors refuse a dirty one. §6.3 records why, including the first set's
failure to meet it. `collect-listing.ps1` additionally runs §1's line-ending gate, because a clean
tree and a correctly checked-out one are not the same claim and only the second one determines the
SHA-256 it archives.

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
- rejection of control-character fill, and a distinct fill error kind;
- validated locale grouping and numeral shape.

This list is a minimum, not a closed set. Coverage enforces three independent rules: every ID above
must exist; every entry in `DIVERGENCES.md` must either be claimed by an assertion site or
declare `Oracle assertions: none`; and every entry must name at least one Rust test that exists and
is still a `#[test]`. Membership of the required list is deliberately not what licenses
an unclaimed entry — tying the two together makes the permitted divergences accidentally closed, so
a genuine new divergence cannot be documented without being mislabelled as required.

The third rule is Phase 6's. Every entry already carried a `Covered by:` line, but nothing read it,
so an entry could go on naming a test that had been renamed or deleted and the document would still
pass — which is the failure mode a "the deviation is demonstrated" claim is most likely to reach.
Enforcing it meant widening `rustTestIndex()` from the generated and ported targets to the whole
`rust/tests/` tree, because a deviation is by definition the thing no oracle assertion replays and
is therefore demonstrated in `public_api.rs`, `properties.rs`, `concurrency.rs` or `decimal.rs`.
Nothing else widened with it: the orphan rule still applies to `rust/tests/ported/` alone. Both new
failure modes were exercised — a named test renamed out from under the document, and an entry left
with no named test at all.

An entry with no oracle assertion behind it is legitimate in both directions. `validated-locale-shape`
is required yet unclaimed, because the locale-schema assertions in `test/locale-test.js` are
hand-ported rather than divergent: Rust reaches the same conclusion at construction time.
`exact-radix-expansion` is unclaimed because the divergence is unreachable from d3's own surface.
Neither can hide a real assertion, because every one of the 729 static sites must map somewhere
regardless, and a declared count that disagrees with the mapping fails separately.

That fill entry was originally written as "rejection of non-BMP/newline fill characters" and was
wrong in both halves. JavaScript's `.` already excludes line terminators, so a newline fill is not
accepted by d3 either and rejecting it is not a deviation. A supplementary-plane fill cannot reach
the slot at all, because the alignment character would have to be the low surrogate. The genuine
deviation is narrower and was found only by testing: `"\t>d"` is a valid d3 specifier that pads with
tabs, and the port rejects the remaining control characters and reports a distinct fill error kind.
State a deviation as what a test demonstrates, not as what it was assumed to be.

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

A box is ticked only where the command was run and succeeded, per §0.2's distinction between a
deliverable being present and a gate having passed. Everything below was last confirmed at the end
of Phase 6, on the one available host: `x86_64-pc-windows-gnu`. The four release-evidence archives
were recollected from the clean tree at `ba5b9ea`, which is the commit that added the clean-tree
enforcement §6.3 describes; the archives themselves are the working-tree change that follows it.

- [x] Phase 0 machine and gate commands pass.
- [x] Oracle metadata exactly matches `oracle.json`.
- [x] Generation is byte-deterministic in two clean runs.
- [x] Coverage reports 729/729 static sites, 1,078 dynamic executions, and 793 calls.
- [x] Fixed and significant exact-tie manifests contain the expected precision/bucket coverage.
- [x] All numeric primitive and carry corpora have zero mismatches.
- [x] Generated, ported, property, allocation, and concurrency tests pass.
- [x] Full formatter and all-locale differentials have zero mismatches.
- [ ] MSRV, pinned, stable, GNU Windows, Ubuntu, feature, and core WASM compile gates pass. —
      **Ubuntu is the outstanding one.** MSRV 1.80.0, pinned 1.98.1, stable, all four feature
      combinations, and the `wasm32-unknown-unknown` compile check pass locally on GNU Windows; no
      Linux host exists here and the hosted runners have never executed.
- [x] Benchmark and parity artifacts are archived, all four collected from a clean tree at
      `ba5b9ea` and recording `git_worktree_dirty: false`.
- [x] Every tracked path under the declared `eol=lf` roots is LF in the index and the working tree,
      and a gate says so rather than a reader remembering to look.
- [x] `cargo package` contents are reviewed and build independently.
- [x] Downstream canaries pass against the packaged crate.
- [x] Compatibility, deviations, release, and rollback documents are complete.

Two prerequisites of §7's Phase 6 canary 4 remain, and both are human decisions rather than gates:
the crate name is unclaimed on crates.io as of the Phase 6 run, and neither the `crates-io-release`
environment nor any credential has been configured.

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
