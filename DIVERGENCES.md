# Intentional deviations from d3-format 3.1.2

This is the complete list. Everything *not* listed here is subject to the byte-for-byte
differential contract described in [`COMPATIBILITY.md`](COMPATIBILITY.md) — including type `d`
infinity, negative zero, unknown type letters, exponent spelling, the Unicode minus and micro
signs, RTL marks, grouping cycles, and final numeral substitution. A behavior may only be treated
as a deviation once it appears below.

Every deviation is about the *interface* rather than the digits. None of them changes the string
any input d3 can express is formatted to; they change what happens to inputs d3 coerces, mutates,
or leaves to the engine.

| ID | Deviation |
|---|---|
| `no-mutable-default-locale` | No mutable process-global default locale |
| `no-global-si-prefix-leak` | No stale global SI-prefix leak |
| `immutable-typed-specifier` | `FormatSpecifier` is immutable after construction |
| `no-specifier-field-coercion` | No `String()`/`+`/`ToInt32` coercion of specifier fields |
| `construction-time-width-limits` | Width limits at parse and construction time; fallible allocation |
| `no-input-coercion` | Typed input-kind errors instead of JavaScript coercion |
| `no-non-bmp-or-newline-fill` | Control-character fill rejected, with a distinct error kind |
| `validated-locale-shape` | Locale grouping and numeral shapes validated at construction |
| `exact-radix-expansion` | Exact radix expansion where V8 approximates |

## How this document is kept honest

Each entry carries a stable `ID:` line, an `Oracle assertions:` count, and a `Covered by:` line.
A coverage checker in the migration repository enforces all three against the tree:

- every oracle assertion classified as a divergence in the assertion map must name an ID that
  exists here;
- every ID here must be either claimed by that mapping in the declared number, or explicitly
  marked `Oracle assertions: none`;
- every entry must name at least one Rust test, and every test it names must exist and still be a
  `#[test]`.

The last of those means a `Covered by:` line cannot quietly rot into a reference to a test that was
renamed or deleted. All nine entries below are demonstrated by tests that run and pass; the ones
under `tests/ported/` carry `#[ignore]` for a scoreboard reason their target root explains, and
`tests/ported/main.rs::the_ported_assertions_all_pass` is what executes them.

The reasoning behind each of these decisions is in
[`docs/MIGRATION_TO_RUST.md`](docs/MIGRATION_TO_RUST.md), which is not part of the published crate.

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
Covered by: `tests/ported/divergences.rs::default_locale_is_not_a_mutable_process_global`,
`tests/public_api.rs::cloning_a_locale_shares_it_rather_than_copying_it`, and the
`Send + Sync` bound in `tests/public_api.rs`, which is a compile error to break.

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
Covered by: `tests/public_api.rs::a_prefix_formatter_reports_type_f_whatever_the_caller_wrote`,
which shows the fixed prefix living on the formatter that chose it, and the `Send + Sync` bound in
the same file. The stronger statement is
`tests/concurrency.rs::prefix_formatters_do_not_share_an_exponent`, which formats through
mixed SI buckets on eight threads at once and requires every answer to equal the one a single
thread computed; `tests/concurrency.rs::one_shared_formatter_per_specifier_agrees_with_one_thread`
does the same for type `s` through one `&Formatter` shared by every thread.

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

One consequence reaches `Display`, and it is the only place a *parsed* specifier renders
differently from d3's. `FormatSpecifier.prototype.toString` spells its width as
`Math.max(1, this.width | 0)` and its precision as `Math.max(0, this.precision | 0)`, so both go
through ToInt32 on the way out. Above 2³¹ that wraps: Node renders `formatSpecifier("4294967295f")`
as `" >-1f"` and `formatSpecifier(".99999999999999999999f")` as `" >-.1661992960f"`. The port stores
the value it parsed and renders that, so the same inputs render as `" >-4294967295f"` and
`" >-.4294967295f"`. No formatted output differs, because d3 clamps the *number* into `[0, 21]`
before using it and the port clamps the same way; only the round trip through the spelling does,
and only for values no d3 caller can render at all.

Oracle assertions: 23 (`test/formatSpecifier-test.js`)
Covered by:
`tests/ported/divergences.rs::format_specifier_is_immutable_after_construction`,
`tests/ported/divergences.rs::specifier_construction_does_not_coerce_arbitrary_javascript_values`,
and `tests/public_api.rs::a_huge_width_or_precision_renders_as_itself_rather_than_wrapping`

---

## Parse- and construction-time width limits, and fallible allocation

ID: `construction-time-width-limits`

`format("999999999999f")` asks d3 to build a string with a padding run of nearly a trillion
characters; JavaScript either throws `RangeError` from the engine or exhausts memory, depending on
the value and the host. The failure is unpredictable, happens at call time, and can be a hard
abort.

The Rust port rejects an out-of-range width at two points. A width the *syntax* cannot hold — above
`u32::MAX` — is a `ParseError` with kind `WidthOverflow`, so it never becomes a `FormatSpecifier`.
A width the syntax holds but the *limits* refuse — above `FormatLimits::max_width_utf16`, one
million UTF-16 code units by default — is a `FormatError::WidthLimit` at formatter construction, so
a formatter that exists cannot later fail to pad. A caller who genuinely wants a wider one raises
the limit through the `*_with_limits` entry points; formatting stays fallible either way.

This is a pure widening of the error surface: every width d3 can actually render is accepted and
renders identically. Only inputs whose JavaScript behavior is "throw or die" become a typed,
recoverable error at an earlier point.

Oracle assertions: none
Covered by: `tests/public_api.rs::parsing_rejects_a_width_the_syntax_cannot_hold`,
`tests/public_api.rs::formatter_construction_rejects_a_width_over_the_limit`,
`tests/public_api.rs::the_default_taking_entry_points_use_the_default_limits`, and
`tests/properties.rs::the_parser_accepts_exactly_the_grammar_the_reference_matcher_does`,
which checks the rejection against an independent implementation of d3's grammar over 62,823
inputs. The allocation half is
`tests/properties.rs::no_accepted_input_produces_an_unbounded_allocation`, which walks the
layout path that does the allocating and shows that an accepted input's output — and the capacity
reserved for it — stays inside what the limits admit.

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
Covered by: `tests/public_api.rs::a_numeric_formatter_refuses_text_instead_of_coercing_it` and
`tests/public_api.rs::a_text_formatter_refuses_numbers_instead_of_coercing_them`, which pin the
refusal in both directions.
`tests/ported/divergences.rs::numeric_formatters_reject_non_numeric_input` asserts the same
refusal against the three locales the oracle uses, and then the other half of the contract: that
`f64::NAN` through those three formatters produces the oracle's own `"   N/A"`, `"-     "` and
`"   NaN"`. The generated block for those assertions,
`tests/generated/locale_test.rs::t009_formatlocale_nan_nan_observes_the_specified_not_a_number_representation`,
is `#[ignore]`d precisely because every call in it passes `undefined`, so this is where they run.

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

Note how much of this d3 already does, and why the divergence is narrower than it looks. `.` does
not match a line terminator, so `"\n>d"`, `"\r>d"`, `"\u2028>d"` and `"\u2029>d"` are already
`invalid format:` errors in JavaScript. A supplementary-plane character never reaches the fill slot
either: `.` would capture only its high surrogate, and the alignment character would then have to be
the low surrogate, which is not one of `[<>=^]`. What the port adds is the *remaining* control
characters — `"\t>d"` is a valid d3 specifier that pads with tabs — and it reports them as a fill
problem rather than as a generic syntax error, through `ParseErrorKind::Fill`.

Oracle assertions: none
Covered by: `tests/public_api.rs::a_fill_that_cannot_be_padded_with_is_rejected_at_parse_time`
and `tests/properties.rs::the_parser_accepts_exactly_the_grammar_the_reference_matcher_does`,
whose reference matcher runs on UTF-16 code units precisely so that the surrogate case above is
modelled rather than assumed.

---

## Validated locale grouping and numeral shape

ID: `validated-locale-shape`

`formatLocale(definition)` performs no validation. A `grouping` of `[]` makes d3's grouping loop
consume no digits and iterate forever; a `grouping` containing `0` does the same; a `numerals`
array of any length other than ten silently produces `undefined` in the output for digits it does
not cover, and a non-array `numerals` throws deep inside formatting rather than at construction.

`Locale` construction in Rust validates the definition: every `grouping` entry must be a positive
count, and `numerals`, when present, must contain exactly ten entries. Two of the three checks are
made by the *types* rather than at run time — `numerals` is `[String; 10]`, so a short array cannot
be written — and the third, a zero group size, is `LocaleError::ZeroGroupingSize` naming the index
it found. An empty `grouping` list is not an error but a disabled grouping, which is what d3's loop
does in effect once `grouping[0]` is `undefined`; grouping is likewise disabled unless `thousands`
is supplied too. Failures are returned at construction time, before any formatting is attempted.

All 59 bundled locale definitions satisfy this schema, as does every inline definition in the
oracle tests, so no recorded call changes. The locale-table generator enforces the same schema at
generation time, which is why a malformed locale cannot reach the generated table.

Oracle assertions: none. The schema-shaped assertions in `test/locale-test.js` are classified
`ported`, not divergent: Rust reaches the same conclusion, just at construction time and by a
different route, so a named Rust test can honestly stand in for them.
Covered by: `tests/ported/locales.rs::every_generated_locale_declares_the_required_keys`,
which walks all 59 bundled locales as the oracle walks the 59 JSON files, and the four policy tests
in `tests/public_api.rs` —
`policy_grouping_is_disabled_unless_both_keys_are_supplied`,
`policy_an_empty_grouping_list_disables_grouping`,
`policy_a_zero_grouping_size_is_rejected` and
`policy_numerals_are_exactly_ten_strings`.

---

## Exact radix expansion where V8 approximates

ID: `exact-radix-expansion`

**No d3-compatible output is affected.** d3 reaches `Number.prototype.toString(radix)` only through
`Math.round(x).toString(radix)` in types `b`, `o`, `x` and `X`, so its reachable radices are 2, 8
and 16. For those, the exact expansion and V8's are the same string, because a binary64 integer's
digits below its own precision genuinely are zero in a power-of-two radix. Every recorded d3 call,
every generated test, and the whole differential corpus fall inside that set.

Outside it the two disagree. `Number::toString` — base ten — is fully specified, but for any other
radix the specification only says the result "should be a generalization of" it, and V8's
generalization is an approximation: it stops emitting digits once the remaining value falls below
half an ulp of the input and pads the rest with zeros. So Node prints `(1e21).toString(36)` as
`5v1j4f4ds7c000`, where the exact expansion of the integer those bits denote is `5v1j4f4ds79m9s`.
Measured over 20,000 pseudorandom large finite integers, radices 2, 8 and 16 matched the exact
`BigInt` expansion in every case, while radix 36 drifted in 9,389 of them.

`decimal::round_to_string_radix` produces the exact expansion at every radix from 2 to 36. Keeping
it exact costs nothing where d3 is concerned and is the better answer where d3 is silent;
reproducing V8's zero-padding would mean porting an engine-specific approximation that no
specification requires and no d3 output observes.

Because the port has not demonstrated agreement outside 2, 8 and 16, it does not claim any: the
differential harness answers those three radices and *rejects* every other one, so a run reports a
declined case rather than a passing one. Declining is the honest answer where the oracle and the
port genuinely differ and no d3 output can tell them apart.

Oracle assertions: none. No oracle assertion reaches a radix outside 2, 8 and 16 — the divergence
is unreachable from d3's own surface, which is why it is recorded here rather than treated as a
compatibility break.
Covered by: `tests/decimal.rs::radix_output_reaches_the_full_1024_bit_expansion`
