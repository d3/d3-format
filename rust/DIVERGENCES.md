# Intentional deviations from d3-format 3.1.2

Everything not listed here is subject to the byte-for-byte differential contract in
`MIGRATION_TO_RUST.md` §3.6 — including type `d` infinity, negative zero, unknown type letters,
exponent spelling, the Unicode minus and micro signs, RTL marks, grouping cycles, and final
numeral substitution. A behavior may only be treated as a deviation once it appears below.

Each entry carries a stable `ID:` line. `rust/tools/check-coverage.mjs` requires that every oracle
assertion classified as a divergence in `rust/fixtures/d3-format-3.1.2/assertion-map.json` names an
ID that exists here, and that every ID here is either claimed by an assertion or explicitly marked
`Oracle assertions: none`. That check is what keeps this document from drifting away from the
recording.

Rust test paths in the `Covered by:` lines are `#[ignore]`d placeholders until the phase named in
their ignore reason lands. They exist now so that no oracle assertion is silently dropped.

---

## No mutable process-global default locale

ID: `no-mutable-default-locale`

d3-format exports mutable module-level bindings — `format`, `formatPrefix` — that
`formatDefaultLocale(definition)` rebinds for the whole process. `test/defaultLocale-test.js`
asserts this by identity: after calling `formatDefaultLocale`, the module's `format` *is* the new
locale's `format`.

The Rust port has no such global. `Locale` values are ordinary owned values, and a formatter is
obtained from the locale it belongs to. There is no binding whose identity could be observed to
change, so no Rust call can honestly be said to pass these assertions; they are recorded here
instead of being remapped onto a different API.

Consequences: a library cannot change another library's formatting behavior; there is no
initialization order to get wrong; and `Locale::en_us()` is a constant, not a snapshot of whatever
the process last configured.

Oracle assertions: 2 (`test/defaultLocale-test.js`)
Covered by: `rust/tests/ported/divergences.rs::default_locale_is_not_a_mutable_process_global`

---

## No stale global SI-prefix leak

ID: `no-global-si-prefix-leak`

`src/formatPrefixAuto.js` writes the chosen SI exponent to a module-level `prefixExponent`
variable, which `src/locale.js` reads afterwards when composing an `s`-type result. The value
survives between calls: after formatting with type `s`, the variable retains the last exponent, and
a subsequent code path that reads it without having written it observes the previous call's state.
It is also not thread- or reentrancy-safe in any environment that has either.

In Rust the auto-prefix routine returns the exponent to its caller as part of its result. Nothing
is stored between calls, so nothing can leak between them. Formatting is `Send`, `Sync`, and
reentrant.

This changes no output for any single formatting call, which is why no oracle assertion pins it:
d3's own tests never observe the stale value. It is listed because the absence of the global is a
deliberate structural difference a reviewer should be able to find.

Oracle assertions: none
Covered by: `rust/tests/properties.rs` (Phase 5: concurrent and interleaved formatting agree with
sequential formatting)

---

## Immutable typed `FormatSpecifier`; no field coercion or `ToInt32` mutation

ID: `immutable-typed-specifier`
ID: `no-specifier-field-coercion`

`formatSpecifier(specifier)` returns a mutable JavaScript object. `test/formatSpecifier-test.js`
exercises two JavaScript-only behaviors:

1. **Mutation after construction** (`immutable-typed-specifier`). Fields are assigned after the
   object exists and the object is then re-stringified. The Rust `FormatSpecifier` is an immutable
   value type: it is produced by parsing or by a builder, and both validate up front. A specifier
   is changed by building a new one, so there is no post-construction assignment to observe.
2. **Coercion on assignment** (`no-specifier-field-coercion`). d3 coerces whatever is assigned —
   `undefined`, `null`, numbers, arbitrary objects — through `String()`, `+`, or `ToInt32`, so
   `specifier.width = "x"` and `specifier.precision = undefined` are meaningful operations with
   defined results. The Rust fields are typed (`Option<u32>` for width and precision, an enum for
   type, `char` for fill), so the ill-typed assignments those assertions make cannot be written at
   all, and `ToInt32` wrapping never occurs.

These are the JavaScript coercion assertions §3.3 refuses to call "passed" by a different Rust
API. What the Rust tests do assert is the *defaults* and the *rendering* — the parts of each
assertion that survive translation — under their own names.

Oracle assertions: 23 (`test/formatSpecifier-test.js`)
Covered by:
`rust/tests/ported/divergences.rs::format_specifier_is_immutable_after_construction`,
`rust/tests/ported/divergences.rs::specifier_construction_does_not_coerce_arbitrary_javascript_values`

---

## Parse- and construction-time width limits, and fallible allocation

ID: `construction-time-width-limits`

`format("999999999999f")` asks d3 to build a string with a padding run of nearly a trillion
characters; JavaScript either throws `RangeError` from the engine or exhausts memory, depending on
the value and the host. The failure is unpredictable, happens at call time, and can be a hard
abort.

The Rust port rejects an out-of-range width when the specifier is parsed or built, returning an
error rather than attempting the allocation. Padding is bounded by a compile-time constant that
`limits.rs` documents, and the formatting path itself performs no unbounded allocation.

This is a pure widening of the error surface: every width d3 can actually render is accepted and
renders identically. Only inputs whose JavaScript behavior is "throw or die" become a typed,
recoverable error at an earlier point.

Oracle assertions: none
Covered by: `rust/tests/ported/specifier.rs::invalid_specifiers_are_rejected_with_the_d3_message`
(Phase 2) and `rust/tests/properties.rs` (Phase 5: no input produces an unbounded allocation)

---

## Numeric and text input mismatch errors instead of JavaScript coercion

ID: `no-input-coercion`

d3's formatters accept anything and coerce it. `format("d")(undefined)` is not an error; `+undefined`
is `NaN`, so the result is `"NaN"`. The oracle tests contain assertions that depend on exactly this:
a non-numeric value is passed to a numeric formatter and the JavaScript coercion result is checked.

Rust formatters take `f64`. There is no value of that type corresponding to `undefined`, so the
call cannot be written, and no Rust call reproduces the assertion. Callers that hold optional or
textual data decide explicitly what a missing value should format as; the library does not decide
for them.

`f64::NAN` itself formats exactly as d3's `NaN` does — this deviation is about the *coercion* of
non-numeric inputs, not about NaN handling, which is covered by the differential contract.

Oracle assertions: 3
Covered by: `rust/tests/ported/divergences.rs::numeric_formatters_reject_non_numeric_input`

---

## Rejection of non-BMP and newline fill characters

ID: `no-non-bmp-or-newline-fill`

A d3 specifier's fill is one UTF-16 code unit taken by regex. A supplementary-plane character
supplied as fill is therefore split: only its high surrogate is used, and the emitted padding is a
run of unpaired surrogates — a string that is not well-formed UTF-16 and that round-trips through
UTF-8 as replacement characters. A newline or other control character is accepted silently and
produces padded output containing raw control codes.

Rust strings are guaranteed well-formed UTF-8, so the lone-surrogate result is not representable.
The port rejects a fill character that is outside the Basic Multilingual Plane, or that is a line
terminator or other control character, when the specifier is parsed or built.

Every fill character d3-format's own tests and locale data use is inside the accepted set, and all
of them render identically.

Oracle assertions: none
Covered by: `rust/tests/ported/specifier.rs::invalid_specifiers_are_rejected_with_the_d3_message`
(Phase 2)

---

## Validated locale grouping and numeral shape

ID: `validated-locale-shape`

`formatLocale(definition)` performs no validation. A `grouping` of `[]` makes d3's grouping loop
consume no digits and iterate forever; a `grouping` containing `0` does the same; a `numerals`
array of any length other than ten silently produces `undefined` in the output for digits it does
not cover, and a non-array `numerals` throws deep inside formatting rather than at construction.

`Locale` construction in Rust validates the definition: `grouping` must be non-empty and every
entry must be a positive count, and `numerals`, when present, must contain exactly ten entries.
Failures are returned as a typed error naming the offending field, at construction time, before any
formatting is attempted.

All 59 bundled locale definitions satisfy this schema, as does every inline definition in the
oracle tests, so no recorded call changes. `rust/tools/gen-locales.mjs` enforces the same schema at
generation time, which is why a malformed locale cannot reach the generated table.

Oracle assertions: none. The schema-shaped assertions in `test/locale-test.js` are classified
`ported`, not divergent: Rust reaches the same conclusion, just at construction time and by a
different route, so a named Rust test can honestly stand in for them.
Covered by: `rust/tests/ported/locales.rs::every_generated_locale_declares_the_required_keys`
