# Porting d3-format 3.1.2 to Rust

Target reader: a competent Rust engineer who has never seen d3-format.

Claims marked **[verified]** were checked by running code on this machine — Node v24.18.0 against
`src/`, `crates.io`, or (since the toolchain was installed, §1.2) real `rustc 1.98.1`.

Claims marked **[gate]** are about Rust's standard library. They were originally written blind,
because the box had no Rust. **They have since been run: all 20 pass on rustc 1.98.1
`x86_64-pc-windows-gnu`.** They are kept as a Phase 0 test file anyway, so that a future toolchain
bump re-checks them. If a gate assertion ever fails, the risk register is wrong on that point and
must be fixed before proceeding — not worked around later.

---

## 0. Measured inventory

Numbers everyone guesses wrong, measured on this checkout (git `ebdc2d5`):

| measure | value |
|---|---|
| files in `test/` | **24** |
| `test(...)` blocks | **168** |
| textual `assert.*` occurrences | **735** |
| …of those, on commented-out lines | **6** |
| executable static `assert.*` sites | **729** |
| assertions *actually executed* | **1,078** |
| distinct format specifiers exercised | **194** |
| distinct locale definitions exercised | **49** |
| library calls at the API boundary | **793** |
| files in `locale/` | **59** |
| `locale/*.json` files carrying `numerals` | **23** |
| implementation size | 16 files, ~13 KB |

The 735 figure is what `grep -c 'assert\.'` gives you and it is the number every summary of this repo
quotes, but six of those sit on commented-out lines **[verified]**, so 729 is the number a coverage
gate must target. Chasing the missing six is a guaranteed hour wasted.

The gap between 729 written and 1,078 executed is the first thing that shapes the test strategy.
Six call sites sit inside loops: `precisionPrefix-test.js` nests two sweeps over `1e-24 … 1e24`, and
`locale-test.js`'s `"locale data is valid"` walks `readdirSync("locale")` with 4 assertions per file
(4 × 59 = 236 on its own).

**Any strategy that statically scrapes literals out of the test files loses ~343 executed
assertions**, including the entire `precisionPrefix` sweep — which is the only systematic coverage of
`exponent()`, a function with a real divergence risk (R6).

**Done** means: (a) all 1,078 assertions reproduced as passing `cargo test` cases; (b) a differential
run against Node over ≥10⁷ `(specifier, value)` pairs with zero mismatches, including a *structured*
tie corpus; (c) `cargo bench` produces numbers; (d) CI runs both suites.

---

## 1. What to install on this machine

Measured state at the time of writing: no `rustc`, no `cargo`, no `rustup`, no `cl`, no `link`, no
`gcc`, no `clang`, and neither Visual Studio directory exists. Node v24.18.0, npm, yarn, git,
Python 3.14.6, PowerShell, and `winget` are present. `node_modules/` was absent.

There is still no C compiler and no Visual Studio, and none is needed.

**Status: the Rust half of this section is done.** `rustup 1.29.1` and
`stable-x86_64-pc-windows-gnu` (rustc 1.98.1) are installed and linking verified — see §1.2.

Machine-wide setup (any Rust or Node project on this box):

```powershell
winget install --id Rustlang.Rustup
winget install --id pnpm.pnpm
```

**Then close the terminal and open a new one.** winget writes `%USERPROFILE%\.cargo\bin` to the
persisted *user* PATH, but an already-running shell keeps its old copy, so the next command fails
with `The term 'rustup' is not recognized…` even though the install succeeded. To fix the current
session instead of opening a new one:

```powershell
$env:Path = [Environment]::GetEnvironmentVariable("Path","Machine") + ';' +
            [Environment]::GetEnvironmentVariable("Path","User")
```

rustup's default Windows host is MSVC, and this machine has no `link.exe`, so switch once — this is
not project-specific:

```powershell
rustup default stable-gnu
```

Use **pnpm**, not yarn or npm. `C:\Users\okhil\package.json` already declares `packageManager: pnpm`,
so Corepack will refuse yarn anyway.

### 1.1 The linker decision: GNU ABI, not MSVC

| option | extra install | verdict |
|---|---|---|
| `x86_64-pc-windows-msvc` + VS Build Tools | 2.3–4 GB, 15–45 min, admin | rejected — nothing here needs the MSVC ABI |
| `x86_64-pc-windows-msvc` + `rust-lld` | still needs the Windows SDK import libraries | rejected — LLD is a linker, not `kernel32.lib` |
| **`x86_64-pc-windows-gnu`** | **nothing beyond rustup** | **chosen** |

rustup's `rust-mingw` component ships the linker and platform libraries for the GNU target. The
rustup book is explicit: *"When targeting the GNU ABI, no additional software is strictly required
for basic use."* rustc's own bootstrap describes `rust-mingw` as *"all the bits and pieces to run the
MinGW Windows targets without any extra installed software."* For a pure-Rust crate this is genuinely
self-contained — **rustup is the only install**, ~120 MB instead of ~3 GB.

This holds *only* while no crate in the graph has a `cc` build dependency. See §1.4, which is exactly
where it bites.

### 1.2 Rust — **done and verified on this box**

```powershell
winget install --id Rustlang.Rustup
rustup default stable-gnu
```

`stable-gnu` pulls the GNU host and the bundled MinGW linker (`rust-mingw`) — 7 components, ~40 s.
Do not install `Rustlang.Rust.GNU` instead: that is a standalone compiler, not rustup, so the
`rust-toolchain.toml` pin below would not work.

**Verify — this is the step that actually proves the plan. [verified], run here:**

```powershell
rustc -vV                     # host: MUST read x86_64-pc-windows-gnu
cargo new --lib "$env:TEMP\linkcheck"
cargo test --manifest-path "$env:TEMP\linkcheck\Cargo.toml"
```

```
host: x86_64-pc-windows-gnu          rustc 1.98.1 (48a229cea 2026-09-01)
test result: ok. 1 passed; 0 failed
```

`cargo test` on a lib crate links a test executable, so this closes the linker question: **Rust
compiles and links on this machine with no MSVC, no Visual Studio and no C compiler of any kind**,
exactly as §1.1 predicted. Note `rustup default stable-gnu` leaves the pre-existing
`stable-x86_64-pc-windows-msvc` toolchain installed but inactive; that is harmless.

Then pin, so this box and CI agree — `rust/rust-toolchain.toml`:

```toml
[toolchain]
channel = "1.98.1"          # the version measured here; pin, don't float
```

Pin a concrete version rather than `"stable"`. The crate has zero runtime dependencies, so there is
no MSRV pressure and no reason to move except deliberately; a floating channel means a Rust release
can turn the differential fuzzer red overnight for reasons unrelated to your code.

### 1.3 The JavaScript oracle, via pnpm

Two separate needs, and only one of them requires a package manager.

**(a) The differential oracle needs nothing.** `src/` is dependency-free, so this works right now:

```powershell
node --input-type=module -e "const {format} = await import('./src/index.js'); console.log(format('.3s')(999.5))"
```

Every **[verified]** fact in this document was produced that way. The harvester and the fuzz oracle
both run on bare Node.

**(b) The vitest baseline needs `node_modules`.** Do not run yarn or npm.

```powershell
winget install --id pnpm.pnpm
pnpm import                   # first time: yarn.lock → pnpm-lock.yaml
pnpm install
```

After `pnpm-lock.yaml` is committed, later checkouts skip `import` and run only `pnpm install`.

This is the right direction on this machine, not a workaround. **[verified]**:

```
PS> yarn install --frozen-lockfile
This project is configured to use pnpm because C:\Users\okhil\package.json has a "packageManager" field
```

Leave Corepack alone. Do **not** set `COREPACK_ENABLE_STRICT=0` to revive yarn. Do **not** run
`npm install`: npm 7+ can rewrite `yarn.lock`, dropping platform-specific `@esbuild/*` entries and
breaking CI on other OSes.

**[verified]** with deps installed, the baseline is green: `npx vitest run` → 24 files, 168 tests,
3.4 s. Replay it with pnpm:

```powershell
pnpm exec vitest run          # not `pnpm test` — the "test" script is `vitest` (watch mode)
```

`"test": "vitest"` starts watch mode; `pnpm exec vitest run` (or `CI=true pnpm test`) is the
single-shot equivalent.

Everything in `locale-test.js` and `arabicLocale-test.js` reads `./locale/*.json` by **relative
path**, so both vitest and the harvester must run with cwd = repo root.

Upstream CI (`.github/workflows/test.yml`) still uses `yarn --frozen-lockfile`. Keep that job green
until `pnpm-lock.yaml` is committed and the workflow is switched; locally, ignore yarn.

### 1.4 criterion: pin to 0.7, or you reintroduce the C compiler

**[verified against crates.io]** `criterion 0.8.2` (current max stable) has a mandatory dependency on
`alloca`, and `alloca 0.4.0`'s only dependency is `cc` as a **build** dependency. `cc` needs a C
compiler. On a box with no C compiler and a GNU-ABI toolchain, that build script fails — and the
"obvious fix" is installing the 2.3 GB you just avoided.

`criterion 0.7.0` has no `alloca` and no `cc` anywhere in its required set (`anes`, `cast`,
`ciborium`, `clap`, `criterion-plot`, `itertools`, `num-traits`, `oorandom`, `regex`, `serde`,
`serde_json`, `tinytemplate`, `walkdir` — all pure Rust). **[verified]**

```toml
[dev-dependencies]
criterion = { version = "=0.7.0", default-features = false, features = ["cargo_bench_support"] }
```

Exact-pin, not `^0.7`, so a `cargo update` cannot walk you into a C compiler.
`default-features = false` also drops `plotters` and `rayon`, which you want gone in CI anyway.

To confirm, do **not** just run `cargo tree -i cc`. When `cc` is absent from the graph — the outcome
you want — that command prints `error: package ID specification 'cc' did not match any packages` and
exits non-zero. Used naively as a CI step it fails on success and passes on failure. Invert it:

```powershell
# local: prints the offender, or nothing at all
cargo tree -e build --prefix none --all-features | Select-String -Pattern '^cc v'
```

This is the clearest illustration of why the GNU decision has to be made consciously: it is free
until one transitive build script wants a C toolchain.

### 1.5 If the GNU toolchain does not work out

```powershell
winget install --id MSYS2.MSYS2
```

Then in a new shell: `C:\msys64\usr\bin\bash.exe -lc "pacman -S --noconfirm mingw-w64-x86_64-gcc"`
and add `C:\msys64\mingw64\bin` to PATH.

Only if that also fails:

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
rustup default stable-msvc
```

2.3–4 GB, 15–45 min, admin. The `--override` is required here: a default Build Tools install does
not include the C++ workload, so `link.exe` still would not appear. Diagnose before switching — an
empty result from

```powershell
& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * `
  -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
```

means the VCTools workload did not finish, which is a different problem from Rust not finding it.

### 1.6 CI

`ubuntu-latest` needs nothing installed by hand: `dtolnay/rust-toolchain@stable` (it honours
`rust-toolchain.toml`) plus `Swatinem/rust-cache@v2`. Add `windows-latest` with
`--target x86_64-pc-windows-gnu` once Ubuntu is green, so the box everyone actually develops on is
covered. GitHub's Windows image ships MSVC, so a Windows MSVC job is also free if you want both ABIs.

---

## 2. Repo layout

The crate goes in a **`rust/` subdirectory of this same repository**, not a sibling repo.

```
d3-format-rust/
├── src/  test/  locale/  package.json      <- untouched JS, stays indefinitely (§2.3)
└── rust/
    ├── Cargo.toml            # package d3-format
    ├── Cargo.lock            # committed
    ├── rust-toolchain.toml
    ├── src/
    │   ├── lib.rs
    │   ├── specifier.rs      # parse + Display, hand-written, no regex crate
    │   ├── decimal.rs        # the exact-digit core: to_fixed / to_exponential / to_precision
    │   ├── bignat.rs         # ~120 lines of u32-limb arithmetic, used only by decimal.rs
    │   ├── types.rs          # the 13 type codes
    │   ├── locale.rs         # Locale, LocaleBuilder, Formatter
    │   ├── precision.rs      # precision_fixed / _round / _prefix
    │   └── locales/generated.rs   # emitted from ../../locale/*.json, committed
    ├── tests/
    │   ├── generated/        # emitted by tools/harvest.mjs, committed, never hand-edited
    │   └── ported/           # the ~90 hand-written structural ones (§3.4)
    ├── benches/format.rs
    ├── fixtures/             # .jsonl, committed; input to the generator and the fuzzer
    └── tools/                # bare-node scripts; NOT part of the cargo build
        ├── vitest-shim.mjs
        ├── harvest.mjs
        ├── instrumented.mjs
        ├── gen-locales.mjs
        ├── differential.mjs
        └── oracle-diff.mjs   # debug one case: --spec ".3s" --value 145999999.99999347
```

Why a subdirectory rather than a separate repo: the harvester reaches the oracle at
`../../src/index.js` and the locale data at `../../locale/`, and `arabicLocale-test.js` /
`locale-test.js` load those JSON files *by name*. Splitting them across repos is how a numeral table
silently rots, and it reintroduces the "which d3 revision did these fixtures come from?" question.
It is still a separate directory in the sense the brief asks: nothing under `rust/` is on the npm
package's `files` list, and nothing outside it is on cargo's.

**Generate locale data into committed Rust; do not read `../locale` from `build.rs`.** A published
crate must build from its own directory, and a `build.rs` reaching outside the package root breaks
`cargo package`. `gen-locales.mjs` emits `rust/src/locales/generated.rs`; CI runs it in `--check`
mode and fails on any byte of drift or a locale count other than 59.

### 2.1 `.gitignore`

```diff
  *.sublime-workspace
  .DS_Store
  dist/
  node_modules
  npm-debug.log
+ rust/target/
+ perf.data*
```

Commit `rust/Cargo.lock`. Nominally a library, where convention says otherwise — but the lockfile is
what makes "the fuzzer was green" reproducible in six months, and it is what keeps criterion pinned
off `alloca`. Do **not** ignore `rust/fixtures/` or `rust/tests/generated/`; committing them is what
lets `cargo test` run without Node.

### 2.2 CI

Keep the existing `test` job. Add:

```yaml
  rust:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest]
    defaults: { run: { working-directory: rust } }
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: rust }
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --all-features
      - run: cargo bench --no-run
      # guards §1.4. `cargo tree -i cc` exits non-zero when cc is ABSENT, so it
      # cannot be used directly -- it would fail the good case and pass the bad one.
      - name: no C build dependency in the graph
        run: |
          if cargo tree -e build --prefix none --all-features | grep -qE '^cc v'; then
            cargo tree -i cc; echo "::error::a C build dependency entered the graph"; exit 1
          fi

  parity:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 24 }
      - uses: dtolnay/rust-toolchain@stable
      - run: node rust/tools/harvest.mjs --check       # regenerate; fail if the diff is non-empty
      - run: node rust/tools/gen-locales.mjs --check
      - run: node rust/tools/differential.mjs --seed 1 --cases 200000
```

`--check` mode is what stops fixture drift: if someone edits a generated test to make it pass, CI
regenerates from the JS and fails on the diff. **Fixtures are only ever refreshed when the JS
implementation or its tests intentionally change — never to make a Rust failure disappear.**

Pin Node 24 in CI. Two behaviours the fixtures depend on (R8, R9) come from the ICU build inside
Node, so the oracle version is part of the contract.

### 2.3 Does the JS get deleted? No.

Keep it, in place, with its suite still in CI. It is not legacy — it is the executable specification,
and the only way to regenerate fixtures or re-run a differential when d3-format 3.2 ships. What
changes is its *status*: note in `README.md` that `src/` is retained as the parity oracle for
`rust/`. Deleting it converts every future "is this a bug or a deliberate difference?" question from
a two-minute `node -e` into archaeology.

---

## 3. Turning 1,078 assertions into Rust tests

### 3.1 Mechanism: runtime capture, not static extraction

Static extraction of `f(x) → "y"` pairs from the test source fails here for three independent
reasons, all measured:

- **Loops.** ~343 of the 1,078 executed assertions are generated at runtime.
- **Call shape.** Only 41% of assertions match `assert.strictEqual(fN(arg), expected)`; another 41%
  are inline `assert.strictEqual(format(spec)(arg), expected)`; the remaining 18% are something else
  entirely. A regex tuned to one shape silently drops the others. **[verified]**
- **Mutation.** `formatSpecifier-test.js` threads one object through ten assertions
  (`(s.fill = "_", s) + ""`), and `f` is bound by an earlier `const f = format(...)`, so a scraper
  would be reimplementing scope resolution.

Instead: **run the real test files under bare node with a fake `vitest`, and record at the library
boundary.**

`tools/vitest-shim.mjs` provides exactly the surface the tests import (`test`, `assert.strictEqual`,
`assert.throws`, `assert.isNaN`). `harvest.mjs` redirects the `"vitest"` specifier to it, and
`src/index.js` to an instrumented wrapper, using `module.registerHooks` — **[verified]** present and
synchronous in Node v24.18.0, no experimental flag, no `node_modules` required:

```js
import {registerHooks} from "node:module";

registerHooks({
  resolve(spec, ctx, next) {
    if (spec === "vitest") return {url: shim, shortCircuit: true};
    // Skip the wrapper's own import of the real module, or it resolves to itself.
    if (spec.endsWith("src/index.js") && ctx.parentURL !== wrap) {
      return {url: wrap, shortCircuit: true};
    }
    return next(spec, ctx);
  }
});
```

`instrumented.mjs` re-exports the real `src/index.js` with `format`, `formatPrefix`, `formatLocale`,
`formatSpecifier` and the three `precision*` helpers wrapped, so every call the suite actually makes
is recorded — loop-generated ones included. The shim's asserts still throw on mismatch, so harvesting
doubles as a check that the suite is green.

One wrinkle worth writing down: `format` and `formatPrefix` must be exported as `let`, not `const`.
`defaultLocale-test.js` calls `formatDefaultLocale()` and then asserts the module-level `format` was
rebound *by object identity*; a `const` wrapper breaks the live binding.

**This mechanism is already built, committed and proven in this repo** — see `tools/harvest/`, which
is Phase 1 done ahead of time. `node tools/harvest/harvest.mjs` on a clean checkout with no
`node_modules` at all:

```
test files      : 24
test() blocks   : 168
assertions RUN  : 1078
failures        : 0
library calls captured: 793   (fmt 624, precisionPrefix 133, pre 11, precisionFixed 10,
                               precisionRound 8, spec 7)
distinct specifiers: 194      distinct locales: 49
```

An independent implementation of the same idea driven through vitest (rather than bare node)
captured **exactly 793** calls as well — two different harnesses agreeing on the count is decent
evidence the capture is complete.

### 3.2 Intermediate format

One JSONL record per observed call:

```jsonl
{"k":"fmt","file":"format-type-s-test.js","t":"format(\"s\") outputs SI-prefix …","spec":"s","bits":"3EE4F8B588E368F1","js":"0.00001","out":"10.0000µ"}
{"k":"pre","file":"precisionPrefix-test.js","a":"3E112E0BE826D695","b":"3E112E0BE826D695","out":0}
{"k":"loc","id":0,"def":{"decimal":".","thousands":",","grouping":[3],"currency":["$",""]}}
```

Two deliberate choices:

- **`bits` is the IEEE-754 hex pattern** and is what Rust consumes (`f64::from_bits(0x3EE4…)`); `js`
  is a human-readable comment only. This removes every "did the Rust and JS literal parsers agree?"
  question — and there are real ones. JSON cannot even represent `-0`
  (`JSON.stringify({v:-0})` is `{"v":0}`), and `1.29e-30` and `5e-324` are exactly the sort of value
  that goes wrong in transit.
- **Locale definitions are interned by id**, not inlined per record: 27 recur across
  `arabicLocale-test.js` and all 59 appear again in `locale-test.js`'s validity sweep.

### 3.3 How the Rust side consumes it

`harvest.mjs` emits **Rust source**, not JSON parsed at test time, so the crate's dev-dependency list
stays at `criterion` alone:

```rust
// rust/tests/generated/format_type_s.rs  -- GENERATED by tools/harvest.mjs, do not edit
use d3_format::Locale;

#[test]
fn format_s_outputs_si_prefix_notation_with_default_precision_6() {
    let loc = Locale::en_us();
    let f = loc.formatter("s").unwrap();
    // f(0.00001)
    assert_eq!(f.format(f64::from_bits(0x3EE4F8B588E368F1)), "10.0000\u{b5}");
}
```

Non-ASCII is emitted as `\u{…}` so generated files are pure ASCII — no BOM, no CRLF, no "which
encoding did Windows write that in" failure mode with `−` (U+2212), `µ` (U+00B5), Arabic-Indic digits,
U+200F or U+202F.

One `#[test]` per original vitest `test()` block, named by slugging the original title, in a file
named after the original test file. `cargo test format_type_s` then maps 1:1 onto
`vitest format-type-s`, and a `git diff` on `tests/generated/` after regeneration shows exactly which
expectation moved.

**Completeness gate.** Runtime capture proves that what it recorded is real, but not that nothing was
missed. Add `tools/check-coverage.mjs`: parse all 24 test files with Acorn, assign every `assert.*`
call a stable `file:line:ordinal` id, and require all **729** executable ids to be accounted for by
either a generated case or a named entry in `assertion-map.json` pointing at a hand-written Rust
test. Acorn gives this for free: commented-out code is not in the AST, so the six dead occurrences
never enter the denominator — which is the reason to parse rather than grep. Orphaned mappings are
rejected too. A green subset is not parity.

### 3.4 The ~90 assertions that are not `f(input) -> "expected"`

| file | asserts | treatment |
|---|---|---|
| `formatSpecifier-test.js` | 51 | **hand-port.** Field defaults, the mutate-then-`toString()` chain, and three `assert.throws`. JS coercion (`new FormatSpecifier({fill: 1}).fill === "1"`) has no typed-struct analogue: port the observable half, drop the coercion half with a comment. |
| `locale-test.js` | 45 | 41 are ordinary `formatLocale(def).format(spec)(v)` → auto-captured with `def` interned. The other 4 are the `readdirSync` shape check → one Rust test over the generated locales asserting all 59 have `currency`/`decimal`/`grouping`/`thousands`. |
| `arabicLocale-test.js` | 27 | fully auto-captured; JSON interned, so the Rust test reads no files. |
| `defaultLocale-test.js` | 6 | **partly reinterpreted, deliberately.** Three assert `format === locale.format`, i.e. that a module global was rebound. The Rust API has no mutable global — that is the point. Port the output assertions against an explicit `fr-FR` locale; replace the identity ones with a doc comment. This is the only place "pass all existing tests" is met by reinterpreting rather than passing; flag it in the PR. |
| `precision{Fixed,Round,Prefix}-test.js` | 40 | auto-captured (151 executed, including both sweeps). `assert.isNaN` becomes `assert!(v.is_nan())` — which is why these return `f64`, not `usize` (§6.4). |
| `formatPrefix-test.js` | 11 | auto-captured via the wrapped two-argument form. |
| `format-test.js` | 13 | 10 auto-captured; the 3 `assert.throws` and `format("d") + "" === " >-d"` become `tests/ported/format_api.rs`. |
| `format-type-c-test.js` | 5 | inputs are **strings** (`"☃"`, `"."`), not numbers; type `c` skips `+value` and the locale-decimal split. `format("c")` cannot be `fn(f64)` only. |

### 3.5 The fixed suite is *not* sufficient evidence of parity

This is the most important section in the document, and it was settled by experiment rather than by
argument.

Replace `Number.prototype.toFixed`, `toExponential` and `toPrecision` with implementations that are
identical to ECMAScript in every respect — exact binary value, notation switches, carry handling —
**except that they round ties to even**, which is what Rust's `core::fmt` does. That is precisely the
bug a plausible-looking port introduces. Then run the untouched suite. **[verified]**:

```
$ node tools/harvest/rust-sim-run.mjs           # BASELINE (stock ECMAScript)
  168 blocks, 1078 assertions, FAILURES: 0
$ node tools/harvest/rust-sim-run.mjs --rust    # ties-to-even, i.e. the likely Rust bug
  168 blocks, 1078 assertions, FAILURES: 0
```

Both are committed (`tools/harvest/rustify.mjs` is the swapped-in rounding; `rust-sim-run.mjs` is the
runner), so this is reproducible rather than asserted.

The simulation was checked to be faithful first: it agrees with ECMAScript on `(1.005).toFixed(2)`,
`(2.675).toFixed(2)`, `(1.15).toFixed(1)`, `(0.1).toFixed(20)`, `(1234).toPrecision(3)`,
`(1e-7).toPrecision(3)`, `(999.5).toExponential(2)` and others, and it does actively change true ties
(`(2.5).toFixed(0)` → `"2"`, `(0.125).toFixed(2)` → `"0.12"`).

**A Rust port that gets tie-breaking exactly backwards passes 1,078 out of 1,078 assertions.**

That is risk R1, the single likeliest way this port silently diverges, and the suite cannot see it.
The suite does contain values that are exact binary ties, but never at a precision where the two
rounding rules disagree.

So the vitest suite is a necessary gate and not a sufficient one. The differential fuzzer is not a
nice-to-have appended at the end — it is the acceptance criterion.

Quantifying how hard the bug is to find by accident, comparing JS `toFixed` against a bit-exact
simulation of Rust's half-to-even rule **[verified]**:

| corpus | disagreement rate |
|---|---|
| structured dyadic rationals (`k/2ⁿ`, precisions 0–6) | **5.76 %** |
| significant-digit rounding on the same corpus (drives `s`, `r`, `g`, `p`) | **6.99 %** |
| 200 k uniformly random f64 bit patterns | **0.04 %** |

Random differential fuzzing is ~140× less likely to surface this than structured tie generation. A
plain "throw random doubles at it" fuzzer will report success. Concrete divergences from that sweep:

```
format(".0f")(0.5)   d3 => "1"    naive Rust => "0"
format(".0f")(2.5)   d3 => "3"    naive Rust => "2"
format(".2f")(0.125) d3 => "0.13" naive Rust => "0.12"
.1 sig digit on 9.5  d3 => "1"(e+1)  naive Rust => "10"   <- also a carry-propagation bug
```

Three things fill the gap:

1. **Differential fuzzing against Node.** `tools/differential.mjs` and a `--dump` mode on the Rust
   side each emit `bits<TAB>spec<TAB>output`; compare byte-for-byte. Case list ≈ 200 specifiers
   (every type code × `{"", ".0", ".1", ".3", ".6", ".17", ".21"}` × `{"", "~"}` ×
   `{"", ",", "0", "+", "($", "#", "020,"}`) crossed with:
   - uniform-random 64-bit patterns, rejecting non-finite;
   - log-uniform magnitudes over `1e-330 … 1e308`;
   - **generated exact ties — the part that earns its keep.** A double `x = m·2^k` with `m` odd is an
     exact `toFixed(p)` tie **iff `k == −(p+1)`**. For each `p ∈ 0..=20`, emit thousands of
     `f64::from_bits`-constructed values with that exponent. Closed-form; random sampling essentially
     never hits one.
   - boundaries: `±0`, `f64::MIN_POSITIVE`, `5e-324`, `f64::MAX`, `1e21` ±1 ulp, `1e-6`/`1e-7` (the
     `toPrecision` fixed↔exponential switch), `2^53 ± 1`, every power of ten `1e-30 … 1e30` ±1 ulp,
     and `10^(3n)` ±1 ulp for the SI buckets.

   10⁷ comparisons run in a couple of minutes. CI runs 2×10⁵ with a fixed seed; a nightly job runs the
   full set with a rotating seed.

2. **The 59-locale sweep.** All `locale/*.json` × ~40 specifiers × ~200 values, differentially
   compared. This is what catches numeral substitution and RTL-mark handling, which no unit test
   covers beyond one assertion per locale.

3. **Oracle-free property tests** (cheap, catch different bugs): `spec.to_string().parse() == spec`;
   `format(",d")(x)` with commas removed equals `format("d")(x)`; `trim(trim(s)) == trim(s)`;
   `format` never panics for any f64 bit pattern × any valid specifier; formatters are `Send + Sync`
   and produce identical output under concurrent use.

---

## 4. Risk register: where a correct-looking port silently diverges

Ordered by likelihood × blast radius. Each entry names the JS call site, a concrete input that
exposes the bug, and the fix. Every output below was **[verified]** on Node v24.18.0.

### R1 — Tie-breaking. JS rounds ties away from zero; Rust's `{:.p}` rounds ties to even.

*Call sites:* `formatTypes.f` (`x.toFixed(p)`), `formatTypes["%"]` (`(x*100).toFixed(p)`), and
`formatDecimalParts` (`x.toExponential(p-1)`) — which feeds **`r`, `s`, `p` and `g`**. Nearly the
whole library.

ECMA-262 specifies these as: pick the integer `n` minimising `|n/10^f − x|` against the **exact
binary value** of `x`, and *"if there are two such n, pick the larger n."* Rust's `flt2dec`, which
backs `format!("{:.*}", p, x)`, rounds ties to even.

| input | JS | naive Rust **[gate]** |
|---|---|---|
| `(2.5).toFixed(0)` | `"3"` | `"2"` |
| `(0.125).toFixed(2)` | `"0.13"` | `"0.12"` |
| `(1.25).toFixed(1)` | `"1.3"` | `"1.2"` |
| `format(".2s")(0.125)` | `"130m"` | `"120m"` |
| `format(".0%")(0.005)` | `"1%"` | `"0%"` (`0.005*100` is exactly `0.5`) |

Both languages round the **exact** double, so this bites only on genuine ties — dyadic rationals.
Note the counterpart **[verified]**: `(1.005).toFixed(2)` is `"1.00"`, not `"1.01"`, because the
double is `1.00499999999999989…`. An implementation that routes through a shortest-decimal string
first is wrong in the *other* direction.

*Mitigation:* **never call `format!("{:.*}")` in the numeric core.** Build `to_fixed`,
`to_exponential`, `to_precision` on an exact decimal expansion of the double, rounding
half-away-from-zero yourself (§6.2). This is the decision the whole port hangs on, and §3.5 explains
why the suite will not tell you if you skip it.

### R2 — `toFixed` bails to exponential at 1e21.

**[verified]** `(1e21).toFixed(2) === "1e+21"`, so `format(".2f")(1e21) === "1e+21"`, and
`format(".2%")(1e19) === "1e+21%"`. `(1e20).toFixed(2)` is still positional. Rust's `{:.2}` would
print 22 digits and a fraction. *Mitigation:* `if x >= 1e21 { return shortest_positional(x) }` at the
top of `to_fixed`. Not covered by the suite — put it in the probe corpus.

### R3 — `toPrecision` switches notation on the **post-rounding** exponent.

*Call site:* `formatTypes.g`, plus `formatPrefixAuto`'s zero/non-finite fallback. Rule: exponential
iff `e < −6 || e >= p`, where `e` is the decimal exponent *after* rounding to `p` significant digits.

| input | output | why it is not obvious |
|---|---|---|
| `(9.99).toPrecision(2)` | `"10"` | rounding bumped `e` 0→1; `1 >= 2` false → positional |
| `(99.9).toPrecision(2)` | `"1.0e+2"` | `e` became 2; `2 >= 2` → exponential |
| `(9.5e-7).toPrecision(1)` | `"0.000001"` | `e` bumped −7→−6, crossing back into positional |
| `(1e-6).toPrecision(2)` | `"0.0000010"` | `−6 < −6` is false |
| `(0).toPrecision(6)` | `"0.00000"` | `e = 0`, so `p−1−e = 5` fraction digits |

Round first, recompute `e`, *then* choose the branch. Implementing `g` by reusing the `r` path passes
`format-type-r-test.js` and fails `g`/`n`/`none`.

### R4 — `formatDecimalParts` treats `p === 0` as falsy and silently switches to shortest round-trip.

```js
var i = (x = p ? x.toExponential(p - 1) : x.toExponential()).indexOf("e")
```

Reachable through exactly one path — `formatPrefixAuto`'s sub-yocto branch,
`formatDecimalParts(x, Math.max(0, p + i - 1))[0]`. When `p + i − 1 <= 0` you do not get "zero
significant digits", you get **all** of them.

- `format(".5s")(1.234567e-30)` → `"0.000001234567y"` — twelve significant digits from a `.5`
  specifier.
- `format("s")(1.29e-30)` → `"0.00000129y"` (p=6, i=−5, so `p+i−1 == 0`).
- `format(".8s")(1.29e-30)` → `"0.0000013y"` (the normal path, for contrast).

A port typing this as `u32` and calling `to_exponential(p - 1)` either underflows or emits one digit.
Model it as `Option<u32>` where `Some(0)` maps to `None`.

### R5 — `Math.floor(exponent / 3)` is floor division; Rust's `/` truncates toward zero.

*Call sites:* `formatPrefixAuto`, `locale.js`'s `formatPrefix`, `precisionPrefix`.

`Math.floor(-1/3) === -1`; Rust `-1i32 / 3 == 0`. **[verified]** `format(".4s")(0.1) === "100.0m"` and
`format(".0s")(1e-5) === "10µ"`. With truncating division the first loses its `m` entirely. This only
bites for negative exponents that are not multiples of 3 — which the `.8s` yocto table (all exponents
≡ 0 mod 3) does **not** cover.

*Mitigation:* `e.div_euclid(3)`, then clamp to `[-8, 8]` before multiplying by 3. Test every exponent
from −30 to 30.

### R6 — `exponent()` must come from the decimal expansion, not `log10().floor()`.

It is the exponent of the **shortest round-trip decimal**. **[verified]** the two disagree at
`1e-323`: `precisionFixed(1e-323)` is `323`, while `Math.floor(Math.log10(1e-323))` is `−324`. All of
`precisionFixed`/`precisionPrefix`/`precisionRound`/`formatPrefix` route through it, and
`precisionPrefix-test.js` executes 133 captured calls hammering it.

*Mitigation:* `fn exponent(x: f64) -> Option<i32>`, parsed out of the shortest round-trip
representation, never via logarithms. `None` for 0/NaN/±∞ — callers turn that into `f64::NAN`,
because `precisionFixed(0)` is asserted NaN.

### R7 — `Math.round` is half-toward-**+∞**, and it is not `floor(x + 0.5)`.

*Call sites:* `formatDecimal` and types `b`, `o`, `x`, `X`. `Math.round(-1.5) === -1` versus Rust's
`(-1.5f64).round() == -2.0`; and **[verified]** `Math.round(-0.5)` is `-0`, not `-1`.

This is currently **latent**, which is the trap: `locale.js` always calls
`formatType(Math.abs(value), precision)`, so the argument is never negative, and on non-negative
input half-away-from-zero and half-up agree. **[verified]** `format("d")(-2.5) === "−3"` and
`format("d")(-0.5) === "−1"` — abs, then round, then prepend the minus. A port that "improves"
`format_d` into a function handling negatives directly returns `−2`. Note also that
`f64::round_ties_even` is wrong here in the other direction: it would turn `0.5` into `0`.

*Mitigation:* keep `.abs()` exactly where d3 has it and write

```rust
fn js_round(x: f64) -> f64 { let r = x.floor(); if x - r >= 0.5 { r + 1.0 } else { r } }
```

valid for `x >= 0.0`, which the call-site `.abs()` guarantees; debug-assert it. **[verified]**
`Math.round(0.49999999999999994)` is `0`, so the naive `(x + 0.5).floor()` — which gives `1` — is
also wrong.

### R8 — The `>= 1e21` branch uses **shortest** digits, not the exact binary value.

```js
Math.abs(x = Math.round(x)) >= 1e21 ? x.toLocaleString("en").replace(/,/g, "") : x.toString(10)
```

ICU renders the double via its shortest round-trip digits, then zero-pads. **[verified]**:

| value | what d3 emits | the exact integer |
|---|---|---|
| `1.3e27` | `1300000000000000000000000000` | `1300000000000000044761612288` |
| `1.3e107` | `13` + 106 zeros | `130000000000000005723571947…` |
| `2**70` | `1180591620717411300000` | `1180591620717411303424` |

`format-type-d-test.js` locks these in, so **a bit-exact BigInt port fails those rows.** The correct
mitigation is to expand the shortest `toString()`: match `^(\d+)(?:\.(\d+))?e\+(\d+)$`, concatenate
significand digits and pad zeros. **[verified]** identical to `toLocaleString("en")` for `1e21`,
`1.3e27`, `1.3e107`, `2^70`, `2^100`, `9.999e21` and `f64::MAX`.

**[gate]** Rust's `Display for f64` is also shortest-round-trip and never switches to exponential, so
`format!("{}", x)` on an integral double should already produce exactly those digits — meaning both
branches of `formatDecimal` may collapse into one `format!("{}", js_round(x))`. Verify with
`assert_eq!(format!("{}", 1e21_f64), "1000000000000000000000")` and
`assert_eq!(format!("{}", f64::MAX).len(), 309)` before relying on it. Note `{:?}` prints `42.0`
where JS prints `42`.

### R9 — `format("d")(Infinity)` is `"∞"`, not `"Infinity"`.

**[verified]**, and this is in no test:

```
format("d")(Infinity)   ->  "∞"          (U+221E)
format("d")(-Infinity)  ->  "−∞"
format("b")(Infinity)   ->  "Infinity"
format("f")(-Infinity)  ->  "−Infinity"
```

`Math.abs(Infinity) >= 1e21` is true, so `d` takes the ICU branch, and ICU renders infinity as `∞`.
Every other type stringifies it. The library is internally inconsistent here; a Rust port will emit
`"inf"` or `"Infinity"` unless told. *Mitigation:* special-case `x.is_infinite()` in `format_decimal`.

### R10 — Negative zero, and NaN's sign bit.

```js
var valueNegative = value < 0 || 1 / value < 0;
```

For `-0`, `1/-0 === -Infinity < 0` → true. For NaN, **both** comparisons are false, so NaN is never
negative *regardless of its sign bit* — but Rust's `f64::is_sign_negative()` returns `true` for a
negative NaN, so the obvious translation prints `−NaN`.

*Mitigation:* `let negative = !v.is_nan() && (v < 0.0 || v.is_sign_negative());`
*Covered by:* `format("+f")(-0) === "−0.000000"`, `format("f")(-0) === "0.000000"`,
`format("d")(-0) === "0"`.

### R11 — "If it rounded to zero, hide the sign" tests the **formatted string** with JS `Number()`.

```js
if (valueNegative && +value === 0 && sign !== "+") valueNegative = false;
```

`value` is a string here. JS `Number()` and Rust's `str::parse::<f64>()` disagree both ways:
**[verified]** `+"" === 0` (Rust: `Err`), `+"0x10" === 16` (Rust: `Err`); Rust accepts
`"inf"`/`"nan"` case-insensitively, JS accepts only `"Infinity"`. Reachable strings are digit/`.`/`e`
sequences plus `"Infinity"`, `"NaN"`, hex digits (types `x`/`X`) and `"∞"` (R9).

*Mitigation:* a small `fn js_number_is_zero(s: &str) -> bool` returning `true` for `""` and otherwise
`s.parse::<f64>().map_or(false, |v| v == 0.0)`. Do not inline `parse().unwrap_or(…)`.
*Covered by:* `format(".2f")(-1e-12) === "0.00"` vs `format("+.2f")(-1e-12) === "−0.00"`.

### R12 — Width and padding are counted in **UTF-16 code units**.

```js
var length = valuePrefix.length + value.length + valueSuffix.length,
    padding = length < width ? new Array(width - length + 1).join(fill) : "";
padding.slice(0, length = padding.length >> 1)   // for "^"
```

`String.prototype.length` is UTF-16 code units. Rust's `.len()` is UTF-8 bytes and `.chars().count()`
is scalar values — **all three differ**, because prefixes and suffixes are full of non-ASCII: `−`
(U+2212, one code unit, **three** UTF-8 bytes) is in every negative output; `µ`, `€`, `₹`, `₪`,
U+00A0, U+202F, U+200F and Arabic currency strings are all over `locale/`.

**[verified]** the suite proves the code-unit rule, and an astral case breaks the `chars()` shortcut
too:

- `formatPrefix(" $12,.1s", 1e6)(-42e6) === "     −$42.0M"` — length counted as 7, giving 5 spaces.
  With byte length you get 9 and 3 spaces.
- `format("020c")("😀")` → `"000000000000000000😀"`, i.e. **18** zeros, `.length === 20`.
  `.chars().count()` would emit 19 zeros; `.len()` would emit 16.

*Mitigation:* one helper, used for *every* length in `format()`:

```rust
fn utf16_len(s: &str) -> usize { s.chars().map(char::len_utf16).sum() }
```

Never `.len()`, never `.chars().count()`. For `^`, build `padding` as a `Vec<char>` and split that.

### R13 — The specifier regex operates on UTF-16 code units too.

`/^(?:(.)?([<>=^]))?…/` — `.` matches one code unit and does not match `\n`. **[verified]**
`formatSpecifier("😀>5d")` **throws** (the `.` eats only the high surrogate) and
`formatSpecifier("\n>5d")` throws. Read the fill as one UTF-16 code unit and reject lone surrogates
and `\n`, rather than "one `char`".

### R14 — `width | 0` and `precision | 0` in `toString()` are ToInt32.

**[verified]** `s.width = 1e10` → `" >-1410065408d"`; `s.width = 4294967296` → `" >-1d"` (wraps to 0,
then clamps to 1). The suite covers the easy half (`width = -1` → `1`) but not the wrap. Since
`Display` is public API and `formatPrefix` round-trips through it (R22), implement ToInt32 properly:
`v as i64 as i32` is *not* it — handle NaN/∞ → 0 and modulo-2³² wrapping.

### R15 — `width` is a JS Number, and huge widths throw at call time.

`(\d+)` matches arbitrarily many digits. **[verified]** `format("99999999999999999999d")(1)` throws
`RangeError` (from `new Array(1e20)`). A port storing `width: usize` will try to allocate 10²⁰ bytes
and abort the process. *Mitigation:* reject `width > u32::MAX` at **parse** time. That is an
intentional divergence — JS defers the error to call time, we surface it at construction, which is
strictly better for an API whose `format` returns `String` and not `Result`.

### R16 — `prefixExponent` is a module global that leaks across calls, formatters *and locales*.

Normally harmless — written and read within one synchronous call. But when `value` is NaN,
`locale.js` short-circuits (`isNaN(value) ? nan : formatType(...)`) and **never calls
`formatPrefixAuto`**, so the previous call's value survives. The guard `!isNaN(value)` is applied to
the *nan string*, so it only saves you when that string is non-numeric. **[verified]**, this is a real
d3 bug:

```js
const g = formatLocale({nan: "0"}).format("s");
g(1e6)   // "1.00000M"
g(NaN)   // "0M"   <- stale prefix, from a different value
```

*Decision:* **do not reproduce it.** No test covers it, reproducing it would need a `Cell` in a type
that should be `Sync`, and it is indefensible. Have the type function return
`(String, Option<i32>)`. Record it in `DIVERGENCES.md` and open an upstream issue.

### R17 — `toString(radix)` on huge values needs more than 128 bits.

*Call sites:* `b`, `o`, `x`, `X`. **[verified]**
`format("x")(1e40) === "1d6329f1c35ca500000000000000000000"` (34 hex digits), `format("b")(1e40)` is
133 characters, and `format("x")(f64::MAX)` is **256 hex digits** — 1024 bits. In Rust,
`1e40_f64 as u128` **saturates** to `u128::MAX` and you silently emit `ffff…`.

*Mitigation:* the value is always `js_round(x)`, i.e. an exact integer `m · 2^k` with `m` a 53-bit
mantissa. For radices 2/8/16 that is a **pure shift** — emit `m`'s digits and append `k / log2(radix)`
zeros, handling the non-multiple case by shifting `m` first. No division and no general bignum needed;
only type `d` needs the decimal path.

### R18 — `x * 100` for `%` and `p` must be a plain IEEE multiply.

**[verified]** `0.07 * 100 === 7.000000000000001` while `0.42 * 100 === 42` exactly. Rust reproduces
this bit-for-bit *provided* you write `x * 100.0` and nothing clever: no `mul_add` (which fuses and
gives a different result), no `x / 0.01`, no reassociation, no folding the 100 into decimal scaling.
Rust does not do FP contraction by default, so a literal `x * 100.0` is safe — say so in a comment,
because it looks like a candidate for optimisation.

### R19 — `formatTrim`'s `i0` is simultaneously an index and a flag.

```js
out: for (var n = s.length, i = 1, i0 = -1, i1; i < n; ++i) {
  switch (s[i]) {
    case ".": i0 = i1 = i; break;
    case "0": if (i0 === 0) i0 = i; i1 = i; break;
    default: if (!+s[i]) break out; if (i0 > 0) i0 = 0; break;
  }
}
```

`i0 === 0` means "a significant digit has been seen since the decimal point"; `i0 > 0` means "cut
here". The loop starts at `i = 1`, so `s[0]` is never trimmable. `!+s[i]` breaks on anything that is
not `'1'..'9'` (`+"e"` is NaN, `+" "` is 0 — both falsy), which is how `"4.200000e+1"` trims to
`"4.2e+1"` without eating the exponent. *Mitigation:* transliterate with `isize` and the same `-1`/`0`
sentinels. Do **not** tidy it into `Option<usize>` — the overload is load-bearing. Trim runs
**before** grouping: `format(",~g")(10000) === "10,000"`.

### R20 — `formatGroup` takes `Infinity` as a width in one call path.

`group(value, Infinity)` when `comma && !zero`; `group(padding + value, padding.length ? width -
valueSuffix.length : Infinity)` when `comma && zero`. Also `g = Math.max(1, width - length)`.
*Mitigation:* type the parameter `f64` and pass `f64::INFINITY`. With `usize`,
`width − valueSuffix.len()` underflows and the comparisons invert. Grouping is cyclic:
`en-IN` is `[3,2,2,2,2,2,2,2,2,2]` and `formatGroup` cycles modulo its length — do not "simplify" it
to `[3,2]`. *Covered by:* `format("06,d")(0) === "00,000"`,
`format("020,s")(42e12) === "00,000,000,042.0000T"`.

### R21 — `numerals` rewrites `[0-9]` in the **final** string.

Applied last: after padding, after currency affixes, after the exponent is built. So a locale with
numerals localises the digits of `"e+21"` too. It is ASCII `[0-9]`, not Unicode `\d`. Padding was
already computed pre-substitution, so copy the order. 23 of the 59 locale files have `numerals`.

### R22 — `formatPrefix` re-parses the specifier's `toString()`.

```js
f = newFormat((specifier = formatSpecifier(specifier), specifier.type = "f", specifier), {...})
```

`re.exec(object)` coerces via `toString()`, so `formatPrefix` parses, mutates `type`, stringifies and
parses again — a **lossy** round trip through the ToInt32 clamping of R14. **[verified]**
`formatSpecifier("($~s")` with `type = "f"` stringifies to `" >($~f"`. *Mitigation:* in Rust,
`prefix_formatter` must do `self.formatter(&spec.to_string())`, not clone the struct.
*Covered by:* `formatPrefix("($~s", 1e3)(-1e3) === "($1k)"`. Note the precision on
`formatPrefix(",.3s", 1e-3)` becomes 3 *fraction* digits after scaling, not 3 significant digits.

### R23 — `formatPrefix` scales by **multiplying** by `Math.pow(10, -e)`, and the order matters.

```js
var e = Math.max(-8, Math.min(8, Math.floor(exponent(value) / 3))) * 3,
    k = Math.pow(10, -e),
    prefix = prefixes[8 + e / 3];
return function(value) { return f(k * value) + prefix; };
```

Two separate traps in one line.

**The scale itself.** `Math.pow(10, -e)` is an implementation-defined transcendental in ECMAScript,
so `10f64.powi(-e)` in Rust is not guaranteed to produce the same bits. Fortunately the exposure is
finite: `e` is clamped to `[-24, 24]` in steps of 3, so there are exactly 17 possible scales, and
**[verified]** all 17 of `Math.pow(10, -e)` are bit-identical to the corresponding decimal literal.
*Mitigation:* hard-code them as a `const [f64; 17]` written as literals (`1e24 … 1e-24`), indexed by
`8 + e/3`. Never call `powi`. A gate test asserts each entry against its hex bit pattern:

```
e=-24 44EA784379D99DB4   e=-6 412E848000000000   e= 6 3EB0C6F7A0B5ED8D   e=18 3C32725DD1D243AC
e=-21 444B1AE4D6E2EF50   e=-3 408F400000000000   e= 9 3E112E0BE826D695   e=21 3B92E3B40A0E9B4F
e=-18 43ABC16D674EC800   e= 0 3FF0000000000000   e=12 3D719799812DEA11   e=24 3AF357C299A88EA7
e=-15 430C6BF526340000                           e=15 3CD203AF9EE75616
e=-12 426D1A94A2000000
e= -9 41CDCD6500000000
```

**The multiply.** `k * value` is **not** `value / 10^e`. For positive `e` the scale `k` is inexact
(`1e-6` is not representable), so the two disagree in the last bit. **[verified]** 4 of 11 sampled
pairs differ, and the difference is not subtle when printed: `1e-9 * 12345678900` is
`12.345678900000001`, whereas `12345678900 / 1e9` is exactly `12.3456789`. The division is the
answer you would want; the multiplication is the answer d3 gives. It reaches the output:
`formatPrefix(".17s", 1e9)(12345678900) === "12.34567890000000112G"`. One ulp is enough to move a
value across a rounding boundary at ordinary precisions. *Mitigation:* write the multiply, in that
operand order, and never "simplify" it to a division.

### R24 — SI bucket selection uses the **post-rounding** exponent.

`formatDecimalParts` rounds first and returns the possibly-incremented exponent; `formatPrefixAuto`
computes the bucket from *that*. **[verified]** `format(".3s")(999.5) === "1.00k"` (the coefficient
carried from `9.995e2` to `1.00e3`), `format(".0s")(999999) === "1M"`, `format(".1s")(9.99e-25) ===
"1y"` while `format("s")(9.999e-25) === "0.99990y"`. A port that picks the bucket from the *input's*
exponent and the digits from the rounded value produces garbage on exactly these carry cases — and
the `.8s` yocto/yotta tables contain no carries.

### R25 — The empty / unknown type is `.12~g`, and precision is defaulted only if absent.

```js
else if (!formatTypes[type]) precision === undefined && (precision = 12), trim = true, type = "g";
```

`format("q")` is **valid** — `q` parses (`[a-z%]/i`), is not in the dispatch table, and becomes
`.12~g` with trim forced on. So is uppercase `F`, since the dispatch keys are lowercase:
**[verified]** `format("F")(1) === "1"`, identical to `format("")(1)`. The specifier keeps the
original character (`formatSpecifier("q").type === "q"`, and `format("n") + "" === " >-n"`, not
`" >,g"`) — the alias is local to the formatter and is not written back. Model `type` as
`Option<char>` holding the original, with dispatch decided separately.

### R26 — Small things that are still wrong if you skip them.

- **`new Array(n).join("0")` produces `n − 1` zeros.** Call sites in `formatRounded`,
  `formatPrefixAuto` (both padding branches) and `locale.js` padding. Off-by-one if you write `n`.
- **`#X` yields a lowercase prefix with uppercase digits.** `locale.js` does `"0" + type.toLowerCase()`,
  so **[verified]** `format("#X")(0xdeadbeef) === "0xDEADBEEF"`.
- **Default minus is U+2212, and SI micro is U+00B5** (MICRO SIGN, not Greek μ U+03BC). Tests use
  `strictEqual`; the wrong code point fails `format("8d")(-4)` and `format("s")(1e-6)`.
- **`maybeSuffix` splits on the first non-ASCII-digit** via `charCodeAt`; `c === 46` (`.`) becomes
  `locale.decimal`. That is how `e`/`g` exponent tails stay out of the thousands groups — and why
  `format(",e")(Infinity)` runs the grouping code on an empty string and must not panic.
- **`precisionRound` subtracts in f64 first, and that subtraction must stay in f64.**
  `step = Math.abs(step), max = Math.abs(max) - step`, then the result feeds `exponent()`. The
  subtraction is correctly rounded, so it is not "lossy" in the usual sense — the trap is the
  opposite. When `step` is small enough relative to `max`, f64 **absorbs it entirely** and the
  subtraction becomes a no-op, leaving the result one decade higher than exact decimal arithmetic
  would. **[verified]** `1 - 1e-17` is exactly `1` in f64, where the true difference is
  `0.99999999999999999`; `exponent()` therefore sees `0` instead of `-1`, and
  `precisionRound(1e-17, 1)` returns **18** where decimal arithmetic gives 17. There are **88** such
  `(max, step)` pairs at plausible magnitudes.
  This is the one place where a `BigNat` port is *more* accurate than d3 and therefore wrong. Do the
  subtraction as an ordinary `f64` operation; do not route it through the decimal core.
- **Rounding carry must renormalise the exponent.** `(999.5).toExponential(2) === "1.00e+3"`.
  Forgetting this yields `"10.0e+2"`.
- **`Number.MIN_VALUE` is the minimum *subnormal*** (`f64::from_bits(1)`), not `f64::MIN_POSITIVE`.
  `format("s")(Number.MIN_VALUE)` is a 303-character string in `format-test.js`.

---

## 5. Phased plan

Risk is retired front-to-back: the toolchain question, then the scoreboard, then the rounding core,
then everything else. Nothing in Phases 3–5 can produce an ambiguous failure, because by then the
decimal core is proven against millions of oracle comparisons.

### Phase 0 — Toolchain, baselines, and **Rust-semantics gate tests** — *toolchain half done*

§1.2 is **complete and verified**: rustc 1.98.1 `x86_64-pc-windows-gnu`, `cargo test` links. §1.3
(pnpm) is still to do.

The gate assertions below have also been **run, and all 20 pass** on that toolchain, so every
**[gate]** claim in the risk register is now **[verified]**. Keep the file as `rust/tests/gate.rs`
regardless: it costs nothing and it is what will catch a future `rustc` changing float formatting
under you.

```rust
#[test] fn what_rust_actually_does() {
    assert_eq!(format!("{:.0}", 2.5f64), "2");                       // R1 ties-to-even
    assert_eq!(format!("{:.2}", 0.125f64), "0.12");                  // R1
    assert_eq!(format!("{}",  1e21f64), "1000000000000000000000");   // R8 no exponential
    assert_eq!(format!("{}",  42.0f64), "42");                       // R8 Display, not Debug
    assert_eq!(format!("{}",  f64::MAX).len(), 309);                 // R8
    assert_eq!(format!("{:e}", 1.29e-30f64), "1.29e-30");            // shortest, for R4/R6
    assert_eq!((-1.5f64).round(), -2.0);                             // R7 half-away
    assert_eq!(0.5f64.round_ties_even(), 0.0);                       // R7 also wrong
    assert_eq!((-1i32).div_euclid(3), -1);                           // R5
    assert_eq!(1e40f64 as u128, u128::MAX);                          // R17 saturation
    assert!("".parse::<f64>().is_err());                             // R11
    assert_eq!("inf".parse::<f64>().unwrap(), f64::INFINITY);        // R11
    assert_eq!("😀".len(), 4);
    assert_eq!("😀".chars().count(), 1);
    assert_eq!("😀".chars().map(char::len_utf16).sum::<usize>(), 2); // R12
    assert_eq!(0.07f64 * 100.0, 7.000000000000001);                  // R18
    // R23: the SI scale table must be literals, and the multiply must stay a multiply.
    assert_eq!(1e-9f64.to_bits(), 0x3E11_2E0B_E826_D695);
    assert_eq!(1e-9f64 * 12345678900.0, 12.345678900000001);
    assert_ne!(1e-9f64 * 12345678900.0, 12345678900.0 / 1e9);
}
```

**Done:** that file passes (**[verified]** — 20/20 on rustc 1.98.1), `cargo test` links on this box
(**[verified]**), and the JS suite reports 168 passing (still pending pnpm, §1.3).
**Red:** a failing gate means the risk register is wrong on that item — fix it now, not in Phase 2.
Linking failure → §1.5, +45 minutes.

Notably `assert_eq!(format!("{:.0}", 2.5f64), "2")` passed, confirming on real Rust the divergence
that §3.5 showed the JS suite cannot detect. R1 is not hypothetical.

### Phase 1 — The oracle harness, before any algorithm (half a day)

`tools/vitest-shim.mjs`, `harvest.mjs`, `instrumented.mjs`, `gen-locales.mjs`, `differential.mjs`,
`check-coverage.mjs`. Generate `tests/generated/*.rs` against a stub crate whose public functions
compile and return placeholders (`String::new()` / `f64::NAN`) rather than `unimplemented!()`, so the
tests *fail* instead of panicking and the failure count is meaningful.

**Done:** `cargo test` compiles and reports exactly N failures, where N is the harvested case count;
the harvester independently confirms 1,078 executed assertions; `check-coverage.mjs` accounts for all
729 executable ids with zero orphans; `harvest.mjs --check` is clean. That is your burndown chart.
**Red:** a generated file does not compile → fix the emitter, never the file.

### Phase 2 — The decimal core (1.5 days) — *the risky part, done early*

`bignat.rs` + `decimal.rs`: exact digit expansion, `js_round`, `to_fixed`, `to_exponential`,
`to_exponential_shortest`, `to_precision`, `format_decimal_parts`, `exponent`, radix-by-shift.

**Done:** a *dedicated* differential run over these primitives alone — 2×10⁶ random doubles **plus the
full closed-form exact-tie set** from §3.5, crossed with `p ∈ 0..=21` — with **zero** mismatches
against Node. Not "the unit tests pass". R1, R2, R3, R4, R6, R8, R17 and the carry rule all live or
die here.
**Red:** expected, repeatedly. Dump each mismatch as `(bits, p, fn, js, rust)`, fix, re-run. Do not
start Phase 3 while non-zero — afterwards a failure could be in either layer and you lose the bisect.
**Timebox:** if the tie set is still red after a day, the digit generator is wrong at the root;
rewrite it from `m·2^k` rather than patching.

### Phase 3 — Specifier, types, locale pipeline (1.5 days)

`specifier.rs`, `types.rs`, `locale.rs`, `precision.rs`. Port `locale.js`'s `format()` closure
statement by statement, including the awkward parts (R19, R20, R12). **Implement type `c` early** —
it is the cheapest test of the UTF-16 width rule.

**Done:** all of `tests/generated/` green; the Phase 1 burndown reaches zero.
**Red:** each failure names one vitest test and one input. Because Phase 2 is proven, the bug is in
the string plumbing. Quick triage: wrong minus or width → R12/R26; `999.5` at `.3s` not `1.00k` → R1
or R24; a `formatPrefix` result off by one ulp → R23; `0.1` at `.4s` missing its `m` → R5;
`format(".1g")(100)` not `"1e+2"` → R3;
`,d` wrong under 1e21 → grouping (R20), not R8.

### Phase 4 — Hand-ported tests and API surface (half a day)

`tests/ported/` (§3.4), rustdoc on every public item, `DIVERGENCES.md` (R9, R15, R16 and the
`defaultLocale` reinterpretation).
**Done:** `cargo test` fully green; `cargo clippy -- -D warnings` and `cargo fmt --check` clean.

### Phase 5 — Full differential, locale sweep, criterion, CI (1 day)

10⁷-case fuzz; the 59-locale sweep; `benches/format.rs`; the CI jobs of §2.2; a concurrent-formatter
stress test.
**Done:** zero mismatches on both platforms; `cargo bench` saves a baseline.
**Red:** a fuzz mismatch here is a genuine find — add the exact bits to `fixtures/regressions.jsonl`
(which the generator turns into a permanent `#[test]`) *before* fixing.

### Phase 6 — Publish and freeze the JS (half a day)

`cargo publish --dry-run` (confirm the crate builds with no path outside `rust/`), README, and a note
that `src/` is now the parity oracle.

**Total: ~5 working days** after the toolchain is in. The two half-days at the front are what make the
middle three days boring, which is the entire point.

**When any phase goes red:** reproduce the same bits in `node tools/oracle-diff.mjs --spec … --value …`
and in a ten-line Rust unit test. Fix the primitive, usually `decimal.rs` or a floor-division. Never
edit a fixture to match Rust. Never add `if x == 1.3e107 { return … }` to the formatter.

---

## 6. The Rust crate

Zero runtime dependencies. `criterion` is the only dev-dependency (pinned, §1.4). No `regex`, no
`once_cell` (use `std::sync::LazyLock`), no `num-bigint` — the ~120-line `BigNat` below is all the
arbitrary precision this needs, and radix conversion needs none of it (R17).

### 6.1 Specifier

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct FormatSpecifier {
    pub fill:      char,
    pub align:     Align,          // Left | Right | Center | AfterSign   ("<" ">" "^" "=")
    pub sign:      Sign,           // Minus | Plus | Parens | Space       ("-" "+" "(" " ")
    pub symbol:    Option<Symbol>, // Currency | Base                     ("$" "#")
    pub zero:      bool,
    pub width:     Option<f64>,    // f64, not usize: R14, R15
    pub comma:     bool,
    pub precision: Option<f64>,    // f64: clamped at use; Display clamps via ToInt32
    pub trim:      bool,
    pub type_:     Option<char>,   // the *original* char, even if unknown (R25)
}

impl FromStr      for FormatSpecifier { type Err = InvalidFormat; }
impl fmt::Display for FormatSpecifier { /* d3's toString(), including ToInt32 (R14) */ }
impl Default      for FormatSpecifier { /* ' ', '>', '-', None, false, … */ }
```

Hand-written parser, ~60 lines. The grammar is
`[[fill]align][sign][symbol][0][width][,][.precision][~][type]`; the only subtleties are that `fill`
is optional *only if* `align` is present, plus the UTF-16 rules of R13. A regex crate would add
~1.5 MB of dependencies to parse nine optional tokens. `InvalidFormat` must render
`invalid format: {spec}` for the throw tests.

### 6.2 The decimal core

```rust
// bignat.rs -- ~120 lines, u32 limbs: mul_small / shl / pow5 / to_decimal_digits.
struct BigNat(Vec<u32>);

/// Exact decimal expansion of a finite non-zero positive f64:
/// x == 0.d[0] d[1] … d[n-1] * 10^exp10
struct Exact { digits: Vec<u8>, exp10: i32 }

fn exact(x: f64) -> Exact {
    let (m, k) = mantissa_exp(x);                  // x == m * 2^k
    let n = if k >= 0 { BigNat::from(m) << k } else { BigNat::from(m) * BigNat::pow5(-k) };
    let digits = n.to_decimal_digits();
    let frac = if k < 0 { -k } else { 0 };
    Exact { exp10: digits.len() as i32 - frac, digits }
}

/// Round to `keep` significant digits, half AWAY FROM ZERO (R1), renormalising on carry (R24/R26).
fn round_significant(e: &mut Exact, keep: usize);

pub fn to_fixed(x: f64, p: u32) -> String;                // R1, R2
pub fn to_exponential(x: f64, p: Option<u32>) -> String;  // None == shortest (R4)
pub fn to_precision(x: f64, p: u32) -> String;            // R1, R3
pub fn js_round(x: f64) -> f64;                           // R7
pub fn exponent(x: f64) -> Option<i32>;                   // R6
```

Cost: `5^1074` is ~2500 bits, so the worst case (`5e-324`) is ~80 limbs and a few thousand word
operations — microseconds, and only for subnormals. `to_exponential(x, None)` delegates to
`format!("{:e}", x)` and re-parses, reusing std's shortest-round-trip algorithm rather than
reimplementing Ryū; that and `format!("{}", …)` in R8 are the *only* two places std's float
formatting is trusted, and the Phase 0 gates are what earn that trust.

### 6.3 Locale and Formatter — no globals

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locale {
    decimal:   String,          // default "."
    thousands: Option<String>,  // grouping is off unless BOTH this and `grouping` are set
    grouping:  Option<Vec<u32>>,
    currency:  (String, String),
    numerals:  Option<[String; 10]>,
    percent:   String,          // default "%"
    minus:     String,          // default "−"  U+2212, NOT ASCII hyphen
    nan:       String,          // default "NaN"
}

impl Locale {
    pub fn en_us() -> &'static Locale;                    // LazyLock; d3's implicit default
    pub fn builder() -> LocaleBuilder;
    pub fn named(name: &str) -> Option<&'static Locale>;  // "ar-EG", "en-IN", … (feature `locales`)

    pub fn formatter(&self, spec: &str) -> Result<Formatter<'_>, InvalidFormat>;
    pub fn formatter_for(&self, spec: &FormatSpecifier) -> Result<Formatter<'_>, InvalidFormat>;
    pub fn prefix_formatter(&self, spec: &str, reference: f64)
        -> Result<Formatter<'_>, InvalidFormat>;
}

pub struct Formatter<'a> { locale: &'a Locale, spec: FormatSpecifier, /* precomputed */ }

impl Formatter<'_> {
    pub fn format(&self, x: f64) -> String;
    /// Allocation-free hot path; what the benchmark measures.
    pub fn format_into(&self, out: &mut String, x: f64);
    /// Type "c": passthrough, no numeric handling, width still counted in UTF-16 units.
    pub fn format_str(&self, s: &str) -> String;
    pub fn specifier(&self) -> &FormatSpecifier;          // == d3's format.toString()
}
```

`Formatter` borrows its `Locale`, which keeps it `Send + Sync`; `Locale::en_us()` is `&'static` so
the common case has no lifetime friction. There is no `formatDefaultLocale`, no mutable global, and
no `prefixExponent` — type dispatch returns `struct Typed { text: String, si_exponent: Option<i32> }`
and the caller reads the field (R16).

Type `c` is split into `format_str` rather than imitating JS's `+value` coercion: passing text to a
numeric type should be a type error, not a silent coercion. Precision clamping stays exactly where d3
has it, at formatter-construction time (`[1,21]` for `gprs`, `[0,20]` otherwise).

### 6.4 Precision helpers — `f64`, not `usize`

```rust
pub fn precision_fixed (step: f64)             -> f64;  // NaN for 0, NaN, ±inf
pub fn precision_round (step: f64, max: f64)   -> f64;
pub fn precision_prefix(step: f64, value: f64) -> f64;

/// Ergonomic accessor for callers who want an integer.
pub fn as_precision(v: f64) -> Option<u32>;
```

`f64` because `precisionFixed(0)`, `precisionRound(0, 1)` and `precisionPrefix(NaN, 1)` are all
asserted `isNaN` (12 such assertions), and callers feed the result straight back into a specifier.
`Option<u32>` reads better in isolation but cannot express the tested contract; offer it as a helper
instead of making it the return type.

### 6.5 The 59 locale files

`tools/gen-locales.mjs` reads `../../locale/*.json` and emits `rust/src/locales/generated.rs` —
committed, ASCII-escaped, behind a non-default `locales` feature so the core crate stays small:

```rust
pub const AR_001: LocaleDef = LocaleDef {
    decimal: "\u{66b}", thousands: Some("\u{66c}"), grouping: Some(&[3]),
    currency: ("", ""),
    numerals: Some(["\u{660}","\u{661}","\u{662}","\u{663}","\u{664}",
                    "\u{665}","\u{666}","\u{667}","\u{668}","\u{669}"]),
    percent: None, minus: None, nan: None,
};
```

**[verified]** across all 59 files the only keys present are `decimal`, `thousands`, `grouping`,
`currency`, `numerals` and `percent`. **No file sets `minus` or `nan`**, so those always take the
`−` / `"NaN"` defaults. 23 files have `numerals`. `ar-DZ`, `ar-EH`, `ar-LY`, `ar-MA` and `ar-TN` do
not, and their expected strings stay ASCII (`1.234,56` / `1,234.56`); `ar-DJ` and `ar-TD` prefixes
include U+200F RLM.

An optional `serde` feature derives `Deserialize` for users who want to load JSON at runtime — kept
off by default, because a formatting crate landing in someone's WASM bundle should not drag in a
derive macro.

### 6.6 Benchmark

```rust
fn bench(c: &mut Criterion) {
    let loc = Locale::en_us();
    let mut g = c.benchmark_group("format_into");
    for (spec, v) in [(",.2f", 1234567.891f64), (".3s", 1.4995e9), (".12~g", 42.0),
                      ("$,.2f", -1234.56), ("08d", 42.0), (".2e", 6.02214076e23),
                      (",d", 1.3e107),        // the >=1e21 path
                      (".8s", 5e-324)] {      // subnormal: the bignum worst case
        let f = loc.formatter(spec).unwrap();
        g.bench_with_input(BenchmarkId::from_parameter(spec), &v, |b, &v| {
            let mut out = String::with_capacity(64);
            b.iter(|| { out.clear(); f.format_into(&mut out, black_box(v)); });
        });
    }
    g.finish();
    // Parsing is a cost the API lets you pay once; measure it separately so the amortised
    // story stays visible instead of being baked into the format numbers.
    c.bench_function("parse/$,.2f", |b| b.iter(|| black_box("$,.2f").parse::<FormatSpecifier>()));
}
```

`format_into` is what the group measures, because `String` allocation would otherwise dominate and
hide the algorithm. Save a baseline in Phase 5 (`cargo bench -- --save-baseline main`).

---

## 7. Intentional divergences from d3

All of these belong in `rust/DIVERGENCES.md`.

| # | d3 behaviour | this crate | why |
|---|---|---|---|
| R15 | `format("99999999999999999999d")(1)` throws `RangeError` at call time | `Err(InvalidFormat)` at parse time | `format()` returns `String`, not `Result`; a 10²⁰-byte allocation aborts the process |
| R16 | stale `prefixExponent` leaks an SI suffix onto a numeric-looking nan string | no leak | upstream bug; reproducing it needs interior mutability in a `Sync` type |
| §3.4 | `formatDefaultLocale` rebinds a module global; 3 tests assert the rebinding | no global; output assertions ported against an explicit locale | the brief explicitly asks for `Locale` as a struct |
| R14 | `FormatSpecifier` fields are freely mutable and coerced (`{fill: 1}` → `"1"`) | typed fields, no coercion | no Rust analogue |
| §6.3 | type `c` accepts anything and applies `+value` | separate `format_str` | typed API |

Everything else — including `format("d")(Infinity) === "∞"` — is reproduced exactly.
