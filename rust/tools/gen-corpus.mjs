#!/usr/bin/env node
// Differential corpus generator (MIGRATION_TO_RUST.md section 3.6).
//
// Writes the canonical input JSONL that both sides of the differential read:
// `rust/src/bin/dump_cases` on stdin, and `differential.mjs` for the Node side.
// The corpus is a pure function of the committed fixtures and the seed, so the
// same seed always yields the same bytes on any platform.
//
// Every case carries a stable id, an operation, raw input bits, the specifier or
// precision, the locale definition together with its hash, and the oracle metadata
// recorded for it -- the six things section 3.6 requires. Recorded cases carry the
// output the oracle actually produced; seeded cases carry a null expectation,
// because nothing has observed them yet and differential.mjs computes the Node
// answer at run time.
//
// Usage:
//   node rust/tools/gen-corpus.mjs                      # recorded cases, to stdout
//   node rust/tools/gen-corpus.mjs --seed 1 --random 5000
//   node rust/tools/gen-corpus.mjs --out rust/target/corpus.jsonl
//   node rust/tools/gen-corpus.mjs --sections calls     # see SECTIONS below
//   node rust/tools/gen-corpus.mjs --write-manifest     # section 4.3's per-bucket census
//   node rust/tools/gen-corpus.mjs --check-manifest
//   node --max-old-space-size=6144 rust/tools/gen-corpus.mjs --verify-sweep

import {closeSync, existsSync, openSync, writeFileSync, writeSync} from "node:fs";

import {
  D3_FORMAT_VERSION,
  FIXTURE_DIR,
  GENERATOR_VERSION,
  SCHEMA_VERSION,
  canonicalJson,
  f64Bits,
  f64FromBits,
  isMain,
  namedLocaleIndex,
  readTextLf,
  repoPath
} from "./harvest.mjs";

export const CORPUS_SCHEMA = "d3-format-rust/corpus@1";
export const CORPUS_MANIFEST_SCHEMA = "d3-format-rust/corpus-manifest@1";
export const DEFAULT_SEED = 1;

/**
 * Section 4.3's supplementary sweep size. It is a floor, not a target: the exit
 * gate asks for at least two million additional deterministic finite bit patterns.
 */
export const SWEEP_SIZE = 2000000;

const CALLS_PATH = `${FIXTURE_DIR}/calls.jsonl`;
const DECIMAL_PATH = `${FIXTURE_DIR}/decimal.jsonl`;
const REGRESSIONS_PATH = `${FIXTURE_DIR}/regressions.jsonl`;
const ORACLE_PATH = `${FIXTURE_DIR}/oracle.json`;
export const CORPUS_MANIFEST_PATH = `${FIXTURE_DIR}/corpus-manifest.json`;

export const SECTIONS = [
  "calls",
  "decimal",
  "regressions",
  "boundary",
  "fixed_ties",
  "significant_ties",
  "carry",
  "sweep",
  "random"
];

/** The sections section 4.3 owns: the structured numeric corpora. */
export const STRUCTURED_SECTIONS = ["boundary", "fixed_ties", "significant_ties", "carry"];

// ---------------------------------------------------------------------------
// Deterministic pseudo-randomness
// ---------------------------------------------------------------------------

/**
 * SplitMix64. Chosen because it is four lines, has no state beyond a single
 * 64-bit word, and is defined entirely in integer arithmetic -- so it produces
 * identical bytes on every platform and every Node build, which a floating-point
 * generator would not guarantee.
 */
export function splitmix64(seed) {
  let state = BigInt.asUintN(64, BigInt(seed));
  const GOLDEN = 0x9e3779b97f4a7c15n;
  const MIX1 = 0xbf58476d1ce4e5b9n;
  const MIX2 = 0x94d049bb133111ebn;
  return function next() {
    state = BigInt.asUintN(64, state + GOLDEN);
    let z = state;
    z = BigInt.asUintN(64, (z ^ (z >> 30n)) * MIX1);
    z = BigInt.asUintN(64, (z ^ (z >> 27n)) * MIX2);
    return BigInt.asUintN(64, z ^ (z >> 31n));
  };
}

/**
 * Draws an f64 the port is likely to get wrong rather than one uniformly sampled
 * from the reals: subnormals, powers of ten near the notation switchovers, exact
 * ties for the rounding rules in section 4.1, and the specials. Roughly a third of
 * draws are raw bit patterns, which is what reaches NaN payloads and negative zero.
 */
export function drawValue(next) {
  const choice = Number(next() % 12n);
  switch (choice) {
    case 0:
      return f64FromBits(next().toString(16).toUpperCase().padStart(16, "0"));
    case 1:
      return f64FromBits(next().toString(16).toUpperCase().padStart(16, "0"));
    case 2:
      return f64FromBits(next().toString(16).toUpperCase().padStart(16, "0"));
    case 3: {
      // Powers of ten across the whole double range, including the 1e21 boundary
      // where toFixed and type `d` diverge from each other.
      const exponent = Number(next() % 640n) - 320;
      return Number(`1e${exponent}`);
    }
    case 4: {
      const exponent = Number(next() % 640n) - 320;
      return -Number(`1e${exponent}`);
    }
    case 5: {
      // Exact halfway cases: n + 0.5 for integers whose successor is representable.
      const n = Number(next() % 0x20000000000000n);
      return n + 0.5;
    }
    case 6: {
      const n = Number(next() % 1000000n);
      const d = Number(next() % 1000000n) + 1;
      return n / d;
    }
    case 7:
      return Number(next() % 0x20000000000000n);
    case 8:
      return -Number(next() % 0x20000000000000n);
    case 9: {
      const SPECIALS = [0, -0, NaN, Infinity, -Infinity, Number.MIN_VALUE, -Number.MIN_VALUE,
        Number.MAX_VALUE, -Number.MAX_VALUE, Number.EPSILON, 1, -1, 0.1, 1e-7, 1e21, 1e-21];
      return SPECIALS[Number(next() % BigInt(SPECIALS.length))];
    }
    case 10: {
      // Subnormals, where shortest-round-trip printing is hardest.
      const mantissa = next() % 0x10000000000000n;
      return f64FromBits(mantissa.toString(16).toUpperCase().padStart(16, "0"));
    }
    default: {
      const scale = Number(next() % 40n) - 20;
      const mantissa = Number(next() % 10000000n) / 1000000;
      return mantissa * Number(`1e${scale}`);
    }
  }
}

const FILLS = ["", " ", "0", "*", "_", "'"];
const ALIGNS = ["", ">", "<", "^", "="];
const SIGNS = ["", "-", "+", "(", " "];
const SYMBOLS = ["", "$", "#"];
const TYPES = ["", "e", "f", "g", "r", "s", "%", "p", "b", "o", "d", "x", "X", "c", "n"];

/** Assembles a specifier from d3's grammar, so every draw is parseable by construction. */
export function drawSpecifier(next) {
  const align = ALIGNS[Number(next() % BigInt(ALIGNS.length))];
  // A fill is only meaningful with an explicit alignment; d3's regex reads them as a pair.
  const fill = align === "" ? "" : FILLS[Number(next() % BigInt(FILLS.length))];
  const sign = SIGNS[Number(next() % BigInt(SIGNS.length))];
  const symbol = SYMBOLS[Number(next() % BigInt(SYMBOLS.length))];
  const zero = next() % 4n === 0n ? "0" : "";
  const width = next() % 3n === 0n ? String(Number(next() % 24n)) : "";
  const comma = next() % 3n === 0n ? "," : "";
  const type = TYPES[Number(next() % BigInt(TYPES.length))];
  // d3 clamps precision per type internally; 0..20 covers every branch of section 4.1.
  const precision = next() % 2n === 0n ? "." + String(Number(next() % 21n)) : "";
  const trim = next() % 8n === 0n ? "~" : "";
  return `${fill}${align}${sign}${symbol}${zero}${width}${comma}${precision}${trim}${type}`;
}

// ---------------------------------------------------------------------------
// Case construction
// ---------------------------------------------------------------------------

/**
 * One corpus record, always in this key order.
 *
 * `value` and `reference` are bit patterns, never JSON numbers, for the reason
 * contract 6 of section 3.2 gives: a JSON number cannot carry -0, a NaN payload,
 * or an infinity, and all three appear in the recorded calls.
 */
export function makeCase({id, op, locale, precision, radix, specifier, reference, value, oracle, origin}) {
  return {
    id,
    op,
    origin,
    locale_id: locale ? locale.id : null,
    locale_sha256: locale ? locale.hash : null,
    locale_definition: locale ? locale.definition : null,
    specifier: specifier ?? null,
    precision: precision ?? null,
    radix: radix ?? null,
    reference: reference === null || reference === undefined ? null : f64Bits(reference),
    value: value === null || value === undefined ? null : f64Bits(value),
    oracle
  };
}

function localeOf(record) {
  if (!record.locale_definition) return null;
  return {
    id: record.locale_id,
    hash: record.locale_hash,
    definition: record.locale_definition
  };
}

/** Replays every recorded boundary call, carrying its recorded output as the expectation. */
export function callCases() {
  const cases = [];
  for (const line of readJsonlText(CALLS_PATH)) {
    const record = JSON.parse(line);
    const id = `call-${String(record.seq).padStart(6, "0")}`;
    const oracle = {
      expected: record.output,
      assertion_id: record.assertion_id,
      test_file: record.file,
      test_index: record.test_index
    };

    if (record.operation === "format" || record.operation === "formatPrefix") {
      const [argument] = record.args;
      cases.push(
        makeCase({
          id,
          op: record.operation,
          origin: "calls",
          locale: localeOf(record),
          specifier: record.specifier_input.text,
          reference: record.reference ? f64FromBits(record.reference.bits) : null,
          // Non-numeric arguments are the JavaScript coercion cases; DIVERGENCES.md
          // entry `no-input-coercion` explains why the Rust side cannot be asked
          // about them, so they carry a null value and a skip reason.
          value: argument.kind === "f64" ? f64FromBits(argument.bits) : null,
          oracle: argument.kind === "f64" ? oracle : {...oracle, skip: "no-input-coercion"}
        })
      );
      continue;
    }

    if (record.operation === "formatSpecifier") {
      cases.push(
        makeCase({
          id,
          op: "formatSpecifier",
          origin: "calls",
          locale: null,
          specifier: record.specifier_input.text,
          value: null,
          oracle
        })
      );
      continue;
    }

    // precisionFixed, precisionPrefix, precisionRound: numeric in, numeric out.
    const [first, second] = record.args;
    cases.push(
      makeCase({
        id,
        op: record.operation,
        origin: "calls",
        locale: null,
        specifier: null,
        reference: second && second.kind === "f64" ? f64FromBits(second.bits) : null,
        value: first.kind === "f64" ? f64FromBits(first.bits) : null,
        oracle
      })
    );
  }
  return cases;
}

/** Replays the ECMAScript numeric-conversion records that section 4.1 pins. */
export function decimalCases() {
  const cases = [];
  let ordinal = 0;
  for (const line of readJsonlText(DECIMAL_PATH)) {
    const record = JSON.parse(line);
    cases.push(
      makeCase({
        id: `decimal-${String(ordinal).padStart(6, "0")}`,
        op: record.operation,
        origin: "decimal",
        locale: null,
        specifier: null,
        precision: record.precision,
        radix: record.radix,
        value: record.input.kind === "f64" ? f64FromBits(record.input.bits) : null,
        oracle: {expected: record.output, assertion_id: null, test_file: null, test_index: null}
      })
    );
    ordinal += 1;
  }
  return cases;
}

/** Replays any case a past differential run pinned. Empty until Phase 3 finds one. */
export function regressionCases() {
  const cases = [];
  let ordinal = 0;
  for (const line of readJsonlText(REGRESSIONS_PATH)) {
    const record = JSON.parse(line);
    cases.push({...record, id: record.id ?? `regression-${String(ordinal).padStart(6, "0")}`});
    ordinal += 1;
  }
  return cases;
}

/**
 * Seeded exploration of the input space beyond what d3's own tests reach.
 *
 * These cases have no recorded expectation: differential.mjs asks the JavaScript
 * implementation for the answer at run time. That is sound because the oracle
 * identity gate has already proven the engine is the recorded one.
 */
export function randomCases(count, seed) {
  const next = splitmix64(seed);
  const {byName} = namedLocaleIndex();
  const names = [...byName.keys()].sort();
  const cases = [];
  for (let index = 0; index < count; index++) {
    const locale = byName.get(names[Number(next() % BigInt(names.length))]);
    const specifier = drawSpecifier(next);
    const oracle = {expected: null, assertion_id: null, test_file: null, test_index: null};
    cases.push(
      makeCase({
        id: `random-${seed}-${String(index).padStart(6, "0")}`,
        op: "format",
        origin: "random",
        locale: {id: locale.name, hash: locale.hash, definition: locale.definition},
        specifier,
        value: drawValue(next),
        // A type `c` formatter takes text. JavaScript coerces the drawn number to a
        // string and formats that; the Rust port answers `InputTypeMismatch`, which
        // is DIVERGENCES.md entry `no-input-coercion` -- the same entry callCases
        // attaches when a *recorded* call passes a non-number. Marking the case is
        // what makes the port's refusal the expected answer instead of a finding;
        // type `c` itself is covered with text input by the generated suite.
        oracle: isTextType(specifier) ? {...oracle, skip: "no-input-coercion"} : oracle
      })
    );
  }
  return cases;
}

/**
 * True when a drawn specifier's type is `c`, the one type that takes text.
 *
 * The type is the last character when there is one, and no other slot of a drawn
 * specifier can end in `c`: `FILLS` has no letter in it, and every other trailing
 * slot is a digit, a comma or a tilde.
 */
export function isTextType(specifier) {
  return specifier.endsWith("c");
}

// ---------------------------------------------------------------------------
// Exact rational predicates (MIGRATION_TO_RUST.md section 4.3)
// ---------------------------------------------------------------------------
//
// Every retention decision below is made in exact integer arithmetic over BigInt.
// `log10` is never the oracle, and neither is any floating-point comparison: a
// finite f64 is exactly `m * 2^k`, so "does scaling by a power of ten leave a
// remainder of exactly one half" is a question about two integers.

const TWO53 = 1n << 53n;

/** `|x| = significand * 2^exponent` with an *odd* significand: the canonical form. */
export function canonicalForm(x) {
  const bits = BigInt(`0x${f64Bits(Math.abs(x))}`);
  const biased = Number((bits >> 52n) & 0x7ffn);
  const fraction = bits & 0xfffffffffffffn;
  let significand = biased === 0 ? fraction : fraction | (1n << 52n);
  let exponent = biased === 0 ? -1074 : biased - 1075;
  if (significand === 0n) return {significand: 0n, exponent: 0};
  while (significand % 2n === 0n) {
    significand >>= 1n;
    exponent += 1;
  }
  return {significand, exponent};
}

/**
 * `|x| * 10^scale` as an exact fraction in lowest terms with respect to the powers
 * of two and ten applied, returned as a numerator and denominator.
 */
function scaled(x, scale) {
  const {significand, exponent} = canonicalForm(x);
  let numerator = significand;
  let denominator = 1n;
  if (exponent >= 0) numerator <<= BigInt(exponent);
  else denominator <<= BigInt(-exponent);
  if (scale >= 0) numerator *= 10n ** BigInt(scale);
  else denominator *= 10n ** BigInt(-scale);
  return {numerator, denominator};
}

/**
 * True when `|x| * 10^scale` has a fractional part of exactly one half.
 *
 * This is the single predicate behind both tie generators. It is also the only
 * place the two rounding rules can be told apart: an exact half is where
 * half-away-from-zero rounds up and half-to-even may not.
 */
export function isExactHalf(x, scale) {
  if (!Number.isFinite(x) || x === 0) return false;
  const {numerator, denominator} = scaled(x, scale);
  return 2n * (numerator % denominator) === denominator;
}

/** The integer part of `|x| * 10^scale`, exactly. */
export function scaledFloor(x, scale) {
  const {numerator, denominator} = scaled(x, scale);
  return numerator / denominator;
}

/**
 * The decimal exponent d3 reads, which is the exponent of the *shortest*
 * round-tripping decimal and not `floor(log10(x))`. `src/exponent.js` obtains it
 * by slicing `toExponential()`, so this does too.
 */
export function decimalExponent(x) {
  if (!Number.isFinite(x) || x === 0) return null;
  const text = Math.abs(x).toExponential();
  return Number(text.slice(text.indexOf("e") + 1));
}

export function nextUp(x) {
  if (Number.isNaN(x) || x === Infinity) return x;
  if (x === 0) return Number.MIN_VALUE;
  const bits = BigInt(`0x${f64Bits(x)}`);
  const next = x > 0 ? bits + 1n : bits - 1n;
  return f64FromBits(next.toString(16).toUpperCase().padStart(16, "0"));
}

export function nextDown(x) {
  if (Number.isNaN(x) || x === -Infinity) return x;
  if (x === 0) return -Number.MIN_VALUE;
  const bits = BigInt(`0x${f64Bits(x)}`);
  const next = x > 0 ? bits - 1n : bits + 1n;
  return f64FromBits(next.toString(16).toUpperCase().padStart(16, "0"));
}

/** `x` together with its immediate neighbours, deduplicated and order-stable. */
function withNeighbours(x) {
  const out = [];
  for (const candidate of [nextDown(x), x, nextUp(x)]) {
    if (!out.some((seen) => sameF64(seen, candidate))) out.push(candidate);
  }
  return out;
}

function sameF64(a, b) {
  return f64Bits(a) === f64Bits(b);
}

// ---------------------------------------------------------------------------
// Primitive operation sweep
// ---------------------------------------------------------------------------

/** d3 reaches `toFixed` at 0..20 and `toPrecision` at 1..21; section 4.1 pins both. */
const FIXED_PRECISIONS = Array.from({length: 21}, (_, index) => index);
const SIGNIFICANT_PRECISIONS = Array.from({length: 21}, (_, index) => index + 1);
const RADICES = [2, 8, 16];

/**
 * Every ECMAScript conversion section 4.1 pins, in a fixed order.
 *
 * The order is part of the corpus's identity: the sweep walks it cyclically so that
 * every operation gets an equal share of the deterministic bit patterns, which a
 * random choice would only approximate.
 */
export const PRIMITIVE_OPS = (() => {
  const ops = [{op: "numberToString", precision: null, radix: null}];
  for (const p of FIXED_PRECISIONS) ops.push({op: "toFixed", precision: p, radix: null});
  ops.push({op: "toExponential", precision: null, radix: null});
  for (const p of FIXED_PRECISIONS) ops.push({op: "toExponential", precision: p, radix: null});
  for (const p of SIGNIFICANT_PRECISIONS) ops.push({op: "toPrecision", precision: p, radix: null});
  ops.push({op: "toLocaleStringEn", precision: null, radix: null});
  for (const radix of RADICES) ops.push({op: "toStringRadix", precision: null, radix});
  return ops;
})();

/**
 * Adapts a value to the domain an operation is actually reached with.
 *
 * Type `d` and the integer radix types apply `Math.round(Math.abs(x))` before they
 * convert (section 4.4's abs-before-round ordering), so those two operations are
 * only ever handed a non-negative integer. Feeding them anything else would be
 * asking the port about behaviour d3 cannot produce.
 */
function operandFor(op, value) {
  return op === "toLocaleStringEn" || op === "toStringRadix" ? Math.round(Math.abs(value)) : value;
}

const EMPTY_ORACLE = {expected: null, assertion_id: null, test_file: null, test_index: null};

/** Emits one case per entry of `ops` for `value`, with ids `${prefix}-${ordinal}`. */
function* primitiveCases(section, counter, value, ops) {
  for (const {op, precision, radix} of ops) {
    yield makeCase({
      id: `${section}-${String(counter.next()).padStart(7, "0")}`,
      op,
      origin: section,
      locale: null,
      specifier: null,
      precision,
      radix,
      value: operandFor(op, value),
      oracle: EMPTY_ORACLE
    });
  }
}

function makeCounter() {
  let index = 0;
  return {
    next: () => index++,
    get count() {
      return index;
    }
  };
}

// ---------------------------------------------------------------------------
// Boundary corpus (section 4.1)
// ---------------------------------------------------------------------------

/**
 * The threshold values section 4.1 names, each with its immediate neighbours where
 * neighbours are meaningful.
 *
 * `1e21` is here twice over: it is where `toFixed` hands over to `Number::toString`
 * and, by a separate rule, where type `d` hands over to `toLocaleString("en")`.
 * The two rules disagree about the same input, so the predecessor and successor of
 * `1e21` are the sharpest cases in the whole corpus.
 */
export function boundaryValues() {
  const named = [];
  const push = (name, value) => named.push({name, value});

  for (const [name, value] of [
    ["1e21", 1e21],
    ["1e20", 1e20],
    ["1e-6", 1e-6],
    ["1e-7", 1e-7]
  ]) {
    push(`${name}:pred`, nextDown(value));
    push(name, value);
    push(`${name}:succ`, nextUp(value));
    push(`-${name}:pred`, nextDown(-value));
    push(`-${name}`, -value);
    push(`-${name}:succ`, nextUp(-value));
  }

  push("zero", 0);
  push("negative-zero", -0);
  push("min-subnormal", Number.MIN_VALUE);
  push("-min-subnormal", -Number.MIN_VALUE);
  push("max-subnormal", nextDown(2.2250738585072014e-308));
  push("min-normal", 2.2250738585072014e-308);
  push("-min-normal", -2.2250738585072014e-308);
  push("max-finite", Number.MAX_VALUE);
  push("-max-finite", -Number.MAX_VALUE);
  push("infinity", Infinity);
  push("-infinity", -Infinity);
  // Both sign bits, and a payload with each, because the sign of a NaN survives
  // `Math.round` in V8 and section 4.4 requires the port to preserve it.
  push("nan", f64FromBits("7FF8000000000000"));
  push("nan:negative", f64FromBits("FFF8000000000000"));
  push("nan:payload", f64FromBits("7FF8000000000001"));
  push("nan:payload-negative", f64FromBits("FFF8000000000001"));
  push("nan:signalling", f64FromBits("7FF0000000000001"));
  return named;
}

/**
 * `formatPrefix`'s reference edge (section 4.1).
 *
 * A zero, NaN or infinite reference makes d3's scale NaN, and every input then
 * formats as the locale's NaN with no SI suffix. A negative finite reference picks
 * its bucket from the magnitude. These are `formatPrefix` cases, so Phase 2 cannot
 * answer them; they are in the corpus because section 4.1 requires them to be
 * explicit fixtures, and the differential's operation filter decides when they
 * count.
 */
function* prefixReferenceCases(counter) {
  const references = [0, -0, NaN, Infinity, -Infinity, -1.5e-7, -1e21, 1e-7, 1e21];
  const values = [0, 1, -1, 1.5e-7, 1e21, NaN, Infinity];
  for (const reference of references) {
    for (const value of values) {
      for (const specifier of ["s", ".3s", "~s"]) {
        yield makeCase({
          id: `boundary-${String(counter.next()).padStart(7, "0")}`,
          op: "formatPrefix",
          origin: "boundary",
          locale: defaultLocale(),
          specifier,
          reference,
          value,
          oracle: EMPTY_ORACLE
        });
      }
    }
  }
}

/**
 * The en-US definition, used wherever a structured case needs a locale at all.
 * Resolved on first use: reading the locale table is not free, and the numeric
 * sections never need it.
 */
let defaultLocaleCache = null;
function defaultLocale() {
  if (defaultLocaleCache === null) {
    const locale = namedLocaleIndex().byName.get("en-US");
    defaultLocaleCache = {id: locale.name, hash: locale.hash, definition: locale.definition};
  }
  return defaultLocaleCache;
}

export function* boundaryCases() {
  const counter = makeCounter();
  for (const {value} of boundaryValues()) {
    yield* primitiveCases("boundary", counter, value, PRIMITIVE_OPS);
  }
  yield* prefixReferenceCases(counter);
}

// ---------------------------------------------------------------------------
// Fixed-precision ties (section 4.3, generator 1)
// ---------------------------------------------------------------------------

/**
 * Canonicalize first, then use the closed form.
 *
 * With `|x| = m * 2^k` and `m` odd, `2 * x * 10^p = m * 5^p * 2^(k + p + 1)` is an
 * odd integer exactly when `k + p + 1 == 0`. So every fixed-precision tie at `p`
 * lives in the single binade whose canonical binary exponent is `-(p + 1)`, and
 * every odd significand there is one. The predicate is still evaluated exactly on
 * every candidate: the closed form chooses what to try, it does not decide what to
 * keep.
 */
export function* fixedTieValues(precision, seed, samples) {
  const k = -(precision + 1);
  // The raw IEEE significand lives in [2^52, 2^53) and must be odd for the
  // canonical exponent to be `k`; the biased exponent field follows from `k`.
  const biased = BigInt(k + 1075);
  if (biased <= 0n || biased >= 0x7ffn) return;
  const emit = (fraction) => {
    const bits = (biased << 52n) | fraction;
    return f64FromBits(bits.toString(16).toUpperCase().padStart(16, "0"));
  };

  const seen = new Set();
  const offer = (fraction) => {
    const odd = fraction | 1n;
    if (seen.has(odd)) return null;
    seen.add(odd);
    return emit(odd);
  };

  const fixed = [1n, 3n, (1n << 52n) - 1n, (1n << 51n) + 1n, (1n << 52n) - 3n];
  for (const fraction of fixed) {
    const value = offer(fraction);
    if (value !== null) yield value;
  }
  const next = splitmix64(seed * 1000 + precision);
  for (let index = 0; index < samples; index++) {
    const value = offer(next() & ((1n << 52n) - 1n));
    if (value !== null) yield value;
  }
}

export function* fixedTieCases({seed = DEFAULT_SEED, samples = 200} = {}) {
  const counter = makeCounter();
  for (const precision of FIXED_PRECISIONS) {
    for (const magnitude of fixedTieValues(precision, seed, samples)) {
      if (!isExactHalf(magnitude, precision)) {
        throw new Error(
          `gen-corpus: ${magnitude} is not an exact tie at toFixed(${precision}); the closed form and the exact predicate disagree`
        );
      }
      for (const value of [magnitude, -magnitude]) {
        // `toFixed(p)` is the rule under test. `toFixed(p)` applied to `x * 100` is
        // exactly what type `%` does, so it is the second half of section 4.3's
        // "through toFixed and %".
        yield* primitiveCases("fixed_ties", counter, value, [
          {op: "toFixed", precision, radix: null},
          {op: "numberToString", precision: null, radix: null}
        ]);
        yield* primitiveCases("fixed_ties", counter, value * 100, [
          {op: "toFixed", precision, radix: null}
        ]);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Significant-precision ties (section 4.3, generator 2)
// ---------------------------------------------------------------------------

/**
 * The same algebra one level up.
 *
 * A significant tie at precision `p` is a value whose exact `x * 10^(p - 1 - e)`
 * ends in one half, where `e` is the decimal exponent. Writing `t = p - 1 - e` and
 * `|x| = m * 2^k` with `m` odd, `2 * x * 10^t = m * 5^t * 2^(k + t + 1)` is an odd
 * integer exactly when `t >= 0` and `k = e - p`. So the ties at `(p, e)` are the
 * odd `m` for which `m * 2^(e - p)` really does have decimal exponent `e`, which is
 * the integer interval computed here.
 */
function significantTieRange(precision, exponent) {
  const shift = BigInt(precision - exponent);
  if (shift < 1n) return null;
  const power = 1n << shift;
  let low;
  let high;
  if (exponent >= 0) {
    const ten = 10n ** BigInt(exponent);
    low = ten * power;
    high = ten * 10n * power;
  } else {
    const ten = 10n ** BigInt(-exponent);
    low = ceilDiv(power, ten);
    high = ceilDiv(power * 10n, ten);
  }
  if (low < 1n) low = 1n;
  if (high > TWO53) high = TWO53;
  return low >= high ? null : {low, high};
}

function ceilDiv(numerator, denominator) {
  return (numerator + denominator - 1n) / denominator;
}

/** `significand * 2^exponent`, exactly, for the ranges the tie generator produces. */
function scalb(significand, exponent) {
  // Every tie has a normal magnitude (see `significantTieRange`), so the exponent
  // stays inside the range where `2 ** exponent` is itself exact and the product
  // does not round.
  return Number(significand) * 2 ** exponent;
}

/**
 * Binary-side construction: every odd significand in the interval that `(p, e)`
 * admits, sampled deterministically when the interval is large.
 */
export function* significantTiesByConstruction(precision, seed, samples) {
  for (let exponent = precision - 1; exponent >= -340; exponent--) {
    const range = significantTieRange(precision, exponent);
    if (range === null) continue;
    const binary = exponent - precision;
    if (binary < -1074) continue;

    const candidates = [];
    const offer = (significand) => {
      const odd = significand % 2n === 0n ? significand + 1n : significand;
      if (odd < range.low || odd >= range.high) return;
      if (!candidates.includes(odd)) candidates.push(odd);
    };
    offer(range.low);
    offer(range.high - 2n);
    offer(range.low + (range.high - range.low) / 2n);
    const span = range.high - range.low;
    const next = splitmix64(seed * 100000 + precision * 1000 + (exponent + 400));
    for (let index = 0; index < samples && BigInt(candidates.length) < span; index++) {
      offer(range.low + (next() % span));
    }
    candidates.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));

    for (const significand of candidates) {
      const value = scalb(significand, binary);
      if (!Number.isFinite(value) || value === 0) continue;
      // The construction predicts the decimal exponent; the oracle confirms it.
      if (decimalExponent(value) !== exponent) continue;
      if (!isExactHalf(value, precision - 1 - exponent)) continue;
      yield {value, exponent};
    }
  }
}

/**
 * Decimal-side synthesis, which is what section 4.3 asks for in so many words:
 * spell a `p + 1` digit decimal ending in 5, read it back as an `f64`, and test
 * that `f64` and its neighbours with the exact predicate. Most candidates are not
 * exactly representable and are discarded; the ones that survive are ties.
 */
export function* significantTiesBySynthesis(precision, seed, samples) {
  const next = splitmix64(seed * 7919 + precision);
  const prefixes = [];
  const offerPrefix = (digits) => {
    if (digits.length !== precision) return;
    if (digits[0] === "0") return;
    if (!prefixes.includes(digits)) prefixes.push(digits);
  };
  offerPrefix("1".padEnd(precision, "0"));
  offerPrefix("9".repeat(precision));
  offerPrefix("1".repeat(precision));
  offerPrefix("1".padEnd(precision, "2"));
  offerPrefix(`${"9".repeat(Math.max(1, precision - 1))}8`.slice(0, precision).padEnd(precision, "9"));
  for (let index = 0; index < samples; index++) {
    let digits = String(1 + Number(next() % 9n));
    while (digits.length < precision) digits += String(Number(next() % 10n));
    offerPrefix(digits);
  }

  for (const prefix of prefixes) {
    for (let exponent = -324; exponent <= 308; exponent++) {
      const mantissa = precision === 1 ? `${prefix}.5` : `${prefix[0]}.${prefix.slice(1)}5`;
      const candidate = Number(`${mantissa}e${exponent}`);
      if (!Number.isFinite(candidate) || candidate === 0) continue;
      for (const value of withNeighbours(candidate)) {
        const actual = decimalExponent(value);
        if (actual === null) continue;
        const scale = precision - 1 - actual;
        if (scale < 0) continue;
        if (!isExactHalf(value, scale)) continue;
        yield {value, exponent: actual};
      }
    }
  }
}

/**
 * Where half-to-even and ECMAScript actually part company.
 *
 * On an exact tie, ECMAScript always takes the larger candidate. Half-to-even takes
 * it only when the last retained digit is odd. So the two rules disagree exactly
 * when the retained integer is even, and counting those is the evidence that this
 * corpus can tell the rules apart at all.
 */
export function halfEvenDisagrees(value, scale) {
  return isExactHalf(value, scale) && scaledFloor(value, scale) % 2n === 0n;
}

export function* significantTieCases({seed = DEFAULT_SEED, samples = 24} = {}) {
  const counter = makeCounter();
  for (const precision of SIGNIFICANT_PRECISIONS) {
    const seen = new Set();
    const found = [];
    const collect = (source) => {
      for (const {value, exponent} of source) {
        const bits = f64Bits(value);
        if (seen.has(bits)) continue;
        seen.add(bits);
        found.push({value, exponent});
      }
    };
    collect(significantTiesByConstruction(precision, seed, samples));
    collect(significantTiesBySynthesis(precision, seed, samples));
    found.sort((a, b) => (f64Bits(a.value) < f64Bits(b.value) ? -1 : 1));

    for (const {value} of found) {
      for (const signed of [value, -value]) {
        yield* primitiveCases("significant_ties", counter, signed, [
          {op: "toExponential", precision: precision - 1, radix: null},
          {op: "toPrecision", precision, radix: null},
          {op: "numberToString", precision: null, radix: null}
        ]);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Carry corpora (section 4.3)
// ---------------------------------------------------------------------------

/**
 * Coefficients ending in runs of nines, in the five shapes section 4.3 names.
 *
 * Each family is a set of decimal spellings; every spelling contributes the `f64`
 * nearest to it and that `f64`'s two neighbours, because a carry that is correct
 * for the decimal is not automatically correct for the double next door.
 */
export function carryFamilies() {
  const families = new Map();
  const add = (family, text) => {
    if (!families.has(family)) families.set(family, []);
    const value = Number(text);
    if (!Number.isFinite(value)) return;
    const bucket = families.get(family);
    for (const candidate of withNeighbours(value)) {
      if (!bucket.some((seen) => sameF64(seen, candidate))) bucket.push(candidate);
    }
  };

  // Fixed decimal-point carries: a run of nines immediately before the digit that
  // rounds, at every place `toFixed` can be asked about.
  for (let nines = 1; nines <= 17; nines++) {
    const run = "9".repeat(nines);
    add("fixed-point", `${run}.5`);
    add("fixed-point", `0.${run}5`);
    add("fixed-point", `${run}.${run}`);
    for (let fraction = 1; fraction <= 6; fraction++) {
      add("fixed-point", `${run}.${"9".repeat(fraction)}5`);
    }
  }

  // `toExponential` exponent increments: a coefficient that rounds up to ten.
  for (let digits = 1; digits <= 17; digits++) {
    const coefficient = digits === 1 ? "9" : `9.${"9".repeat(digits - 1)}`;
    for (const exponent of [-308, -30, -7, -6, -1, 0, 1, 6, 20, 21, 22, 100, 308]) {
      add("exponential-increment", `${coefficient}e${exponent}`);
      add("exponential-increment", `${coefficient}5e${exponent}`);
    }
  }

  // `toPrecision` fixed/exponential switches: the post-rounding exponent crosses
  // `p` or `-6`, which is the whole of section 4.1's notation rule.
  for (let digits = 1; digits <= 21; digits++) {
    add("precision-switch", `${"9".repeat(digits)}.5`);
    add("precision-switch", `0.${"0".repeat(6)}${"9".repeat(digits)}5`);
    add("precision-switch", `0.${"0".repeat(5)}${"9".repeat(digits)}5`);
  }

  // SI bucket boundaries at 10^(3n): the coefficient rounds up across a bucket.
  for (let bucket = -8; bucket <= 8; bucket++) {
    const exponent = bucket * 3;
    add("si-boundary", `1e${exponent}`);
    add("si-boundary", `9.9995e${exponent + 2}`);
    add("si-boundary", `999.9995e${exponent}`);
    add("si-boundary", `999999.5e${exponent - 3}`);
    add("si-boundary", `0.9999995e${exponent}`);
  }

  // Section 4.3 names these three and their scaled forms explicitly.
  for (const base of ["9.5", "99.5", "999.5"]) {
    for (let exponent = -24; exponent <= 24; exponent += 3) {
      add("nine-halves", exponent === 0 ? base : `${base}e${exponent}`);
    }
  }

  return families;
}

export function* carryCases() {
  const counter = makeCounter();
  for (const [, values] of carryFamilies()) {
    for (const magnitude of values) {
      for (const value of [magnitude, -magnitude]) {
        yield* primitiveCases("carry", counter, value, PRIMITIVE_OPS);
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Supplementary sweep (section 4.3)
// ---------------------------------------------------------------------------

const FINITE_EXPONENT_MASK = 0x7ffn;

/**
 * A deterministic finite `f64`, drawn from a raw 64-bit pattern.
 *
 * Half the draws are uniform over the finite patterns, which is the only way to
 * reach arbitrary significands in arbitrary binades; the other half reuse
 * `drawValue`'s structured distribution, which is where the powers of ten, the
 * halfway cases and the subnormals come from. A draw that lands on a non-finite
 * value has its exponent field rewritten rather than being rejected, so the number
 * of draws is exactly the number of values and the stream stays reproducible.
 */
export function drawFinite(next) {
  if (next() % 2n === 0n) return drawUniformFinite(next);
  const value = drawValue(next);
  return Number.isFinite(value) ? value : Math.sign(value || 1) * Number.MAX_VALUE;
}

/** A uniform draw over the finite bit patterns. */
export function drawUniformFinite(next) {
  let bits = next();
  if (((bits >> 52n) & FINITE_EXPONENT_MASK) === FINITE_EXPONENT_MASK) {
    bits &= ~(1n << 52n);
  }
  return f64FromBits(bits.toString(16).toUpperCase().padStart(16, "0"));
}

/** Keeps the sweep's stream distinct from every other section's at the same seed. */
const SWEEP_STREAM = 0x5eed_5eed;

/**
 * A set of 64-bit patterns held in a typed array rather than a `Set` of strings.
 *
 * The sweep has to be two million *distinct* bit patterns, so it needs to remember
 * every pattern it has produced. A `Set` of 16-character strings costs a couple of
 * hundred megabytes for that; open addressing over a `BigUint64Array` costs 32, and
 * the sweep already runs inside the differential's memory budget.
 */
class BitPatternSet {
  constructor(capacity) {
    let slots = 16;
    while (slots < capacity * 2) slots *= 2;
    this.slots = new BigUint64Array(slots);
    this.mask = BigInt(slots - 1);
    // Zero is the empty-slot sentinel, so the pattern that *is* zero -- positive
    // zero -- is tracked on the side rather than in the table.
    this.hasZero = false;
    this.size = 0;
  }

  add(key) {
    if (key === 0n) {
      if (this.hasZero) return false;
      this.hasZero = true;
      this.size += 1;
      return true;
    }
    let index = Number(mix64(key) & this.mask);
    while (this.slots[index] !== 0n) {
      if (this.slots[index] === key) return false;
      index = (index + 1) & Number(this.mask);
    }
    this.slots[index] = key;
    this.size += 1;
    return true;
  }
}

/** SplitMix64's finalizer, reused as the hash. */
function mix64(value) {
  let z = BigInt.asUintN(64, value);
  z = BigInt.asUintN(64, (z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n);
  z = BigInt.asUintN(64, (z ^ (z >> 27n)) * 0x94d049bb133111ebn);
  return BigInt.asUintN(64, z ^ (z >> 31n));
}

/**
 * Section 4.3's supplementary corpus: `sweep` distinct finite bit patterns, one
 * primitive operation each.
 *
 * Distinctness is enforced rather than hoped for. The structured half of
 * `drawFinite` deliberately revisits the interesting values -- the powers of ten,
 * the half-integers, the specials -- so a plain two-million-draw stream yields
 * closer to 1.75 million distinct patterns. Redrawing on a repeat costs about a
 * seventh more draws and makes the count exact.
 *
 * The operation table is walked cyclically rather than sampled, so every conversion
 * gets the same share of the patterns instead of an approximately equal one.
 */
export function* sweepCases({seed = DEFAULT_SEED, sweep = SWEEP_SIZE} = {}) {
  const next = splitmix64(BigInt.asUintN(64, BigInt(seed) ^ BigInt(SWEEP_STREAM)));
  const seen = new BitPatternSet(sweep);
  for (let index = 0; index < sweep; index++) {
    const {op, precision, radix} = PRIMITIVE_OPS[index % PRIMITIVE_OPS.length];
    // Distinctness is enforced on the operand, not on the draw. Type `d` and the
    // radix types take `Math.round(Math.abs(x))` first, so two different draws can
    // reach the same operand, and it is the operand that is actually tested.
    let operand = operandFor(op, drawFinite(next));
    let attempts = 0;
    while (!seen.add(BigInt(`0x${f64Bits(operand)}`))) {
      // The uniform branch has 2^64 minus the non-finite patterns to choose from,
      // so it cannot stay blocked; falling back to it after a run of repeats keeps
      // the loop bounded no matter how saturated the structured branch gets.
      operand = operandFor(op, attempts < 64 ? drawFinite(next) : drawUniformFinite(next));
      attempts += 1;
    }
    yield makeCase({
      id: `sweep-${seed}-${String(index).padStart(8, "0")}`,
      op,
      origin: "sweep",
      locale: null,
      specifier: null,
      precision,
      radix,
      value: operand,
      oracle: EMPTY_ORACLE
    });
  }
}

function readJsonlText(relativePath) {
  // regressions.jsonl is empty until a differential run pins a case, and a fresh
  // checkout that has never run harvest --write does not have it at all. Both mean
  // the same thing here: no records.
  if (!existsSync(repoPath(relativePath))) return [];
  const text = readTextLf(relativePath);
  if (text.length === 0) return [];
  return text.replace(/\n$/, "").split("\n");
}

/**
 * The first line of every corpus. It pins the provenance the runner reports on a
 * mismatch, so an artifact is interpretable without the corpus it came from.
 */
export function corpusHeader({seed, sections, count}) {
  return {
    kind: "header",
    schema: CORPUS_SCHEMA,
    schema_version: SCHEMA_VERSION,
    generator: "rust/tools/gen-corpus.mjs",
    generator_version: GENERATOR_VERSION,
    d3_format_version: D3_FORMAT_VERSION,
    oracle: JSON.parse(readTextLf(ORACLE_PATH)),
    seed,
    sections,
    cases: count
  };
}

/**
 * Streams every selected case in section order.
 *
 * A generator rather than an array because the sweep alone is two million records:
 * materializing it would cost gigabytes for no benefit, and every consumer either
 * writes it straight out or feeds it to the differential in batches. Section order
 * is fixed rather than following the command line, so two runs that name the same
 * sections in a different order still produce identical bytes.
 */
export function* iterateCases({seed = DEFAULT_SEED, random = 0, sweep = SWEEP_SIZE, sections} = {}) {
  const selected = sections ?? ["calls", "decimal", "regressions"];
  for (const section of SECTIONS) {
    if (!selected.includes(section)) continue;
    if (section === "calls") yield* callCases();
    else if (section === "decimal") yield* decimalCases();
    else if (section === "regressions") yield* regressionCases();
    else if (section === "boundary") yield* boundaryCases();
    else if (section === "fixed_ties") yield* fixedTieCases({seed});
    else if (section === "significant_ties") yield* significantTieCases({seed});
    else if (section === "carry") yield* carryCases();
    else if (section === "sweep") yield* sweepCases({seed, sweep});
    else if (section === "random") yield* randomCases(random, seed);
  }
}

export function buildCorpus({seed = DEFAULT_SEED, random = 0, sweep = SWEEP_SIZE, sections} = {}) {
  const selected = sections ?? ["calls", "decimal", "regressions"];
  const cases = [...iterateCases({seed, random, sweep, sections: selected})];

  const seen = new Set();
  for (const record of cases) {
    if (seen.has(record.id)) throw new Error(`gen-corpus: duplicate case id ${record.id}`);
    seen.add(record.id);
  }

  const header = corpusHeader({
    seed,
    sections: SECTIONS.filter((section) => selected.includes(section)),
    count: cases.length
  });
  return {header, cases};
}

export function renderCorpus(corpus) {
  return [corpus.header, ...corpus.cases].map((record) => canonicalJson(record) + "\n").join("");
}

// ---------------------------------------------------------------------------
// Corpus manifest
// ---------------------------------------------------------------------------

/**
 * Per-bucket census of the structured corpora.
 *
 * Section 4.3 is explicit that the manifest, not the prose, records the sizes, and
 * that a generator producing zero cases for an expected bucket is a gate failure.
 * `--check-manifest` is that gate: it recounts and byte-compares, so a generator
 * that quietly stops producing a bucket fails rather than passes.
 *
 * The sweep is described rather than enumerated. It is a pure function of the seed
 * and its size, and counting two million records to write the same two million
 * back into a file would be a slow way to restate the definition.
 */
export function buildCorpusManifest({seed = DEFAULT_SEED, sweep = SWEEP_SIZE} = {}) {
  const boundary = boundaryValues();
  const boundaryOps = {primitive: 0, format_prefix: 0};
  for (const record of boundaryCases()) {
    if (record.op === "formatPrefix") boundaryOps.format_prefix += 1;
    else boundaryOps.primitive += 1;
  }

  const fixed = {by_precision: {}, cases: 0};
  for (const precision of FIXED_PRECISIONS) {
    let values = 0;
    for (const magnitude of fixedTieValues(precision, seed, 200)) {
      if (!isExactHalf(magnitude, precision)) throw new Error("gen-corpus: non-tie in the fixed corpus");
      values += 1;
    }
    fixed.by_precision[String(precision)] = values;
  }
  for (const _ of fixedTieCases({seed})) fixed.cases += 1;

  const significant = {
    by_precision: {},
    by_decimal_exponent_bucket: {},
    half_even_disagreements: 0,
    cases: 0
  };
  for (const precision of SIGNIFICANT_PRECISIONS) {
    const seen = new Set();
    let values = 0;
    for (const source of [
      significantTiesByConstruction(precision, seed, 24),
      significantTiesBySynthesis(precision, seed, 24)
    ]) {
      for (const {value, exponent} of source) {
        const bits = f64Bits(value);
        if (seen.has(bits)) continue;
        seen.add(bits);
        values += 1;
        // The bucket is the decimal exponent itself. Ties only exist for
        // `e <= p - 1` and only where the admissible significand interval is
        // non-empty, so the reachable range is narrow enough that a decade bucket
        // would hide which exponents the generator actually reaches.
        const bucket = String(exponent);
        significant.by_decimal_exponent_bucket[bucket] =
          (significant.by_decimal_exponent_bucket[bucket] ?? 0) + 1;
        if (halfEvenDisagrees(value, precision - 1 - exponent)) {
          significant.half_even_disagreements += 1;
        }
      }
    }
    significant.by_precision[String(precision)] = values;
  }
  for (const _ of significantTieCases({seed})) significant.cases += 1;

  const carry = {by_family: {}, cases: 0};
  for (const [family, values] of carryFamilies()) carry.by_family[family] = values.length;
  for (const _ of carryCases()) carry.cases += 1;

  const sweepByOperation = {};
  for (let index = 0; index < PRIMITIVE_OPS.length; index++) {
    const {op, precision, radix} = PRIMITIVE_OPS[index];
    const key = radix !== null ? `${op}(${radix})` : precision !== null ? `${op}(${precision})` : op;
    // The sweep walks the operation table cyclically, so each operation's share is
    // exact arithmetic on the sweep size rather than an observation.
    sweepByOperation[key] =
      Math.floor(sweep / PRIMITIVE_OPS.length) + (index < sweep % PRIMITIVE_OPS.length ? 1 : 0);
  }

  const manifest = {
    schema: CORPUS_MANIFEST_SCHEMA,
    schema_version: SCHEMA_VERSION,
    generator: "rust/tools/gen-corpus.mjs",
    generator_version: GENERATOR_VERSION,
    d3_format_version: D3_FORMAT_VERSION,
    oracle: JSON.parse(readTextLf(ORACLE_PATH)),
    seed,
    sections: {
      boundary: {values: boundary.length, cases: boundaryOps.primitive + boundaryOps.format_prefix, by_operation_kind: boundaryOps},
      fixed_ties: fixed,
      significant_ties: significant,
      carry,
      // `distinct_operands` is a claim the generator enforces rather than a
      // count it takes: `sweepCases` redraws until the operand's bit pattern is
      // one it has not yielded before, so the sweep's case count and its count of
      // distinct finite operands are the same number.
      sweep: {values: sweep, cases: sweep, distinct_operands: sweep, by_operation: sweepByOperation}
    }
  };

  const empty = [];
  for (const [section, body] of Object.entries(manifest.sections)) {
    if (body.cases === 0) empty.push(section);
    for (const key of ["by_precision", "by_family", "by_decimal_exponent_bucket"]) {
      if (!(key in body)) continue;
      for (const [bucket, count] of Object.entries(body[key])) {
        if (count === 0) empty.push(`${section}.${key}.${bucket}`);
      }
    }
  }
  if (empty.length > 0) {
    throw new Error(
      `gen-corpus: section 4.3 requires every expected bucket to be non-empty; these are empty: ${empty.join(", ")}`
    );
  }

  return manifest;
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

export function parseArguments(argv) {
  const options = {
    seed: DEFAULT_SEED,
    random: 0,
    sweep: SWEEP_SIZE,
    sections: null,
    out: null,
    manifest: null,
    verifySweep: false
  };
  const flags = argv.slice(2);
  for (let index = 0; index < flags.length; index++) {
    const flag = flags[index];
    const value = () => {
      const next = flags[++index];
      if (next === undefined) throw new Error(`gen-corpus.mjs: ${flag} needs a value`);
      return next;
    };
    if (flag === "--seed") options.seed = Number(value());
    else if (flag === "--random") options.random = Number(value());
    else if (flag === "--sweep") options.sweep = Number(value());
    else if (flag === "--out") options.out = value();
    else if (flag === "--sections") options.sections = value().split(",").map((s) => s.trim());
    else if (flag === "--write-manifest") options.manifest = "write";
    else if (flag === "--check-manifest") options.manifest = "check";
    else if (flag === "--verify-sweep") options.verifySweep = true;
    else throw new Error(`gen-corpus.mjs: unknown argument ${flag}`);
  }
  if (!Number.isSafeInteger(options.seed)) throw new Error("gen-corpus.mjs: --seed must be an integer");
  if (!Number.isSafeInteger(options.random) || options.random < 0) {
    throw new Error("gen-corpus.mjs: --random must be a non-negative integer");
  }
  if (!Number.isSafeInteger(options.sweep) || options.sweep < 0) {
    throw new Error("gen-corpus.mjs: --sweep must be a non-negative integer");
  }
  if (options.sections === null) {
    options.sections = options.random > 0 ? [...SECTIONS] : ["calls", "decimal", "regressions"];
  }
  for (const section of options.sections) {
    if (!SECTIONS.includes(section)) {
      throw new Error(`gen-corpus.mjs: unknown section ${section}; expected one of ${SECTIONS.join(", ")}`);
    }
  }
  return options;
}

function runManifest(options) {
  const text = canonicalJson(buildCorpusManifest(options), 2) + "\n";
  if (options.manifest === "write") {
    writeFileSync(repoPath(CORPUS_MANIFEST_PATH), Buffer.from(text, "utf8"));
    console.error(`gen-corpus: corpus manifest -> ${CORPUS_MANIFEST_PATH}`);
    return 0;
  }
  if (!existsSync(repoPath(CORPUS_MANIFEST_PATH))) {
    console.error(`gen-corpus: ${CORPUS_MANIFEST_PATH} does not exist; run --write-manifest`);
    return 1;
  }
  if (readTextLf(CORPUS_MANIFEST_PATH) !== text) {
    console.error(`gen-corpus: ${CORPUS_MANIFEST_PATH} is stale; the corpora and the manifest disagree`);
    return 1;
  }
  console.error(`gen-corpus: ${CORPUS_MANIFEST_PATH} is current`);
  return 0;
}

/**
 * Enumerates the sweep and checks the claim the manifest makes about it.
 *
 * `--check-manifest` compares counts, and the sweep's counts are arithmetic on its
 * size rather than an observation -- so on its own it would let the generator claim
 * two million distinct operands while producing fewer. This walks the whole stream
 * and fails if the distinct count falls short, and it reports the shape of what it
 * walked, because "two million patterns" all drawn from one binade would satisfy
 * the letter of section 4.3 and none of its intent.
 */
function verifySweep(options) {
  const seen = new BitPatternSet(options.sweep);
  const binades = new Set();
  const operations = new Map();
  let subnormal = 0;
  let negative = 0;
  let cases = 0;

  for (const record of sweepCases(options)) {
    cases += 1;
    const bits = BigInt(`0x${record.value}`);
    seen.add(bits);
    const biased = Number((bits >> 52n) & 0x7ffn);
    binades.add(biased);
    if (biased === 0) subnormal += 1;
    if (bits >> 63n === 1n) negative += 1;
    const key = record.radix !== null ? `${record.op}(${record.radix})` : record.precision !== null ? `${record.op}(${record.precision})` : record.op;
    operations.set(key, (operations.get(key) ?? 0) + 1);
  }

  const counts = [...operations.values()];
  console.error(
    `gen-corpus: sweep at seed ${options.seed}: ${cases} case(s), ${seen.size} distinct operand pattern(s),` +
      ` ${binades.size} of 2048 binade(s), ${subnormal} subnormal, ${negative} negative,` +
      ` ${operations.size} operation(s) with ${Math.min(...counts)}..${Math.max(...counts)} case(s) each`
  );
  if (seen.size !== cases) {
    console.error(
      `gen-corpus: the sweep must be ${cases} distinct finite bit pattern(s); it repeated ${cases - seen.size}`
    );
    return 1;
  }
  return 0;
}

function main() {
  const options = parseArguments(process.argv);
  if (options.verifySweep) {
    process.exitCode = verifySweep(options);
    return;
  }
  if (options.manifest !== null) {
    process.exitCode = runManifest(options);
    return;
  }

  // Written incrementally rather than assembled: the sweep section is two million
  // records, which is a gigabyte of JSON that nothing needs in memory at once. The
  // header carries the case count, so the cases are counted first and written
  // second; both passes are the same pure generator, so they cannot disagree.
  if (options.out) {
    let count = 0;
    for (const _ of iterateCases(options)) count += 1;
    const header = corpusHeader({
      seed: options.seed,
      sections: SECTIONS.filter((section) => options.sections.includes(section)),
      count
    });
    const handle = openSync(options.out, "w");
    try {
      let buffer = canonicalJson(header) + "\n";
      for (const record of iterateCases(options)) {
        buffer += canonicalJson(record) + "\n";
        if (buffer.length >= 1 << 22) {
          writeSync(handle, buffer);
          buffer = "";
        }
      }
      if (buffer.length > 0) writeSync(handle, buffer);
    } finally {
      closeSync(handle);
    }
    console.error(`gen-corpus: ${count} case(s) -> ${options.out}`);
    return;
  }
  const corpus = buildCorpus(options);
  process.stdout.write(renderCorpus(corpus));
}

if (isMain(import.meta.url)) {
  main();
}
