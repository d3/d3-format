# Security policy

## Supported versions

| Version | Supported |
|---|---|
| `0.1.0-rc.1` | yes, as a release candidate |

Nothing has been published to crates.io yet. Until `0.1.0`, fixes land on the latest release
candidate rather than being backported.

## Reporting a vulnerability

Report privately through GitHub's [security advisory][advisory] form on this repository rather than
by opening a public issue. Please include what you did, what happened, and the crate version and
toolchain you saw it on.

[advisory]: https://github.com/alexkhil/d3-format-rust/security/advisories/new

## What is in scope

This is a string-formatting library with no I/O, no network access, no deserialization of untrusted
formats outside the optional `serde` locale wire shape, and `#![forbid(unsafe_code)]` at the crate
root. The realistic threat is an input that makes the crate misbehave rather than one that escapes
it. Specifically in scope:

- a panic, arithmetic overflow, or unbounded allocation reachable from any accepted specifier,
  locale, or `f64` bit pattern — every formatting entry point is fallible precisely so that these
  are returned as errors, and reaching one is a bug;
- a width, precision, or locale affix that defeats `FormatLimits` and produces an allocation the
  caller did not sanction;
- unbounded memory growth or a hang on input the API accepts.

## What is not

- Disagreeing with d3-format. That is a correctness bug — open a normal issue, and see
  [`CONTRIBUTING.md`](CONTRIBUTING.md) for what makes such a report actionable.
- Denial of service from limits you configured yourself. `FormatLimits` lets a caller raise the
  ceiling deliberately; formatting stays fallible, but a large limit is a choice, not a flaw.
- Vulnerabilities in the JavaScript d3-format package. Report those to
  [d3/d3-format](https://github.com/d3/d3-format).
