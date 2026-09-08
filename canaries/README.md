# Downstream canaries

Two sample applications that consume the **packaged** `d3-format` crate the way a
stranger on crates.io would, and the two workloads section 7's Phase 6 canary list names.

```powershell
pwsh canaries/run-canaries.ps1 -AllowDirty   # drop -AllowDirty on a clean checkout
```

`run-canaries.ps1` packages the crate, extracts the `.crate` into `.packaged/d3-format`, checks the
extraction against the `exclude` list in `rust/Cargo.toml`, then builds and runs each consumer twice
— once on the pinned toolchain and once on the declared MSRV. It exits non-zero on the first
failure and prints what failed.

| Consumer | Features | Workload |
|---|---|---|
| `default-consumer` | none | the ordinary path, the fallible surface, and the **multithreaded formatter workload** |
| `all-features-consumer` | `locales`, `serde` | the **locale-heavy workload**, over all 59 bundled locales |

## Why they live here and not under `rust/`

`cargo package` walks the crate directory. Anything under `rust/` is in the published artifact
unless `exclude` says otherwise, and an exclusion is a thing that can silently stop matching — which
is exactly the failure the packaging collector exists to catch. A directory outside `rust/` cannot
be packaged at all, so the canaries need no exclusion and cannot become a footgun for a later
change.

Two smaller reasons point the same way. `rust/Cargo.toml` has no `[workspace]` section, so a package
nested under it is a cargo error until it is listed as a member. And `rust/rust-toolchain.toml` pins
channel 1.98.1 for anything built from inside `rust/`, which would make the MSRV half of the canary
impossible to run: rustup resolves that file by walking up from the build directory, so only a
consumer *outside* `rust/` can genuinely choose its own compiler. Section 7 excludes
`rust-toolchain.toml` from the package for that reason, and this is where the exclusion is worth
something.

## What "against the packaged crate" means here

Section 7 allows a local registry or an unpacked path; this is the unpacked path. Each consumer
declares

```toml
d3-format = { path = "../.packaged/d3-format" }
```

and `.packaged/` is deleted and recreated from the tarball on every run, so there is nothing to go
stale. Three assertions stand behind the claim:

1. The extraction must not contain `rust-toolchain.toml`, `fixtures/`, `tools/`, `ci/` or
   `src/bin/`, and must contain `src/lib.rs`, `src/locales/generated.rs` and `README.md`. A tarball
   that failed those is not the artifact section 7 describes.
2. The unpacked `Cargo.toml` version must equal `rust/Cargo.toml`'s.
3. `cargo metadata` is read back and the manifest path cargo actually resolved for `d3-format` must
   be the extracted one. This is the assertion that a path typo or a stray `[patch]` cannot survive:
   nothing else distinguishes "built the tarball" from "built `rust/src` by accident".

The default consumer additionally asserts that its whole dependency graph is itself and
`d3-format` — section 5.1's claim that the default build has no dependencies, checked rather than
repeated.

## The workloads

**Multithreaded** (`default-consumer`). Every answer is computed once on one thread, then eight
threads recompute all of them concurrently and compare. Each thread starts at a different offset, so
at any instant they are in different SI buckets through different specifiers; one `&Formatter` and
one `&Formatter` from `prefix_formatter` are borrowed by all eight at once through `thread::scope`;
and a `Locale` built on the main thread is cloned into each worker. The target is d3's
`prefixExponent` module global — if the port had reproduced it, threads in different buckets would
read each other's exponent. About 520,000 formatted values per run, in the dev profile so that
overflow checks and debug assertions stay live.

**Locale-heavy** (`all-features-consumer`). All 59 bundled locales, 14 specifiers and 14 values —
including both zeros, NaN, both infinities and the magnitude where type `d` changes engine code
path — through three construction paths that must agree byte for byte: `Locale::named`,
`Locale::from_definition` from a hand-assembled `LocaleDefinition`, and the same definition after a
`serde_json` round trip. Roughly 46,000 formatted values. It also checks that the 23 locales with
numerals leave no ASCII digit in a plain `,d`, and that an absent key survives serialization as an
absent key rather than as `null`.

Both consumers anchor their assertions to values recorded from the pinned Node oracle, not to the
port's own output, so a shared misunderstanding of the mini-language fails rather than passes.

## Generated files

`.packaged/`, each consumer's `target/`, and each consumer's `Cargo.lock` are gitignored. The
lockfiles are regenerated on every run, under the pinned toolchain only, because they name the crate
version under test and because section 1 forbids writing a lockfile with cargo 1.80.
