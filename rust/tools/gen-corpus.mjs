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
//   node rust/tools/gen-corpus.mjs --sections calls     # calls | decimal | random

import {existsSync, writeFileSync} from "node:fs";

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
export const DEFAULT_SEED = 1;

const CALLS_PATH = `${FIXTURE_DIR}/calls.jsonl`;
const DECIMAL_PATH = `${FIXTURE_DIR}/decimal.jsonl`;
const REGRESSIONS_PATH = `${FIXTURE_DIR}/regressions.jsonl`;
const ORACLE_PATH = `${FIXTURE_DIR}/oracle.json`;

export const SECTIONS = ["calls", "decimal", "regressions", "random"];

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
    cases.push(
      makeCase({
        id: `random-${seed}-${String(index).padStart(6, "0")}`,
        op: "format",
        origin: "random",
        locale: {id: locale.name, hash: locale.hash, definition: locale.definition},
        specifier: drawSpecifier(next),
        value: drawValue(next),
        oracle: {expected: null, assertion_id: null, test_file: null, test_index: null}
      })
    );
  }
  return cases;
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

export function buildCorpus({seed = DEFAULT_SEED, random = 0, sections = ["calls", "decimal", "regressions"]} = {}) {
  const cases = [];
  // Section order is fixed rather than following the command line, so two runs that
  // name the same sections in a different order still produce identical bytes.
  for (const section of SECTIONS) {
    if (!sections.includes(section)) continue;
    if (section === "calls") cases.push(...callCases());
    else if (section === "decimal") cases.push(...decimalCases());
    else if (section === "regressions") cases.push(...regressionCases());
    else if (section === "random") cases.push(...randomCases(random, seed));
  }

  const seen = new Set();
  for (const record of cases) {
    if (seen.has(record.id)) throw new Error(`gen-corpus: duplicate case id ${record.id}`);
    seen.add(record.id);
  }

  const header = corpusHeader({seed, sections: SECTIONS.filter((s) => sections.includes(s)), count: cases.length});
  return {header, cases};
}

export function renderCorpus(corpus) {
  return [corpus.header, ...corpus.cases].map((record) => canonicalJson(record) + "\n").join("");
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

export function parseArguments(argv) {
  const options = {seed: DEFAULT_SEED, random: 0, sections: null, out: null};
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
    else if (flag === "--out") options.out = value();
    else if (flag === "--sections") options.sections = value().split(",").map((s) => s.trim());
    else throw new Error(`gen-corpus.mjs: unknown argument ${flag}`);
  }
  if (!Number.isSafeInteger(options.seed)) throw new Error("gen-corpus.mjs: --seed must be an integer");
  if (!Number.isSafeInteger(options.random) || options.random < 0) {
    throw new Error("gen-corpus.mjs: --random must be a non-negative integer");
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

function main() {
  const options = parseArguments(process.argv);
  const corpus = buildCorpus(options);
  const text = renderCorpus(corpus);
  if (options.out) {
    writeFileSync(options.out, Buffer.from(text, "utf8"));
    console.error(`gen-corpus: ${corpus.cases.length} case(s) -> ${options.out}`);
  } else {
    process.stdout.write(text);
  }
}

if (isMain(import.meta.url)) {
  main();
}
