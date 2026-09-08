#!/usr/bin/env node
// Single-case reproducer for the differential protocol (MIGRATION_TO_RUST.md 3.6).
//
// `differential.mjs` prints a command that invokes this tool when it finds a
// disagreement, so that one case can be inspected on its own without regenerating
// or re-running anything else. It also owns `evaluateCase`, the JavaScript side of
// the comparison, so that the runner and the reproducer cannot drift apart: there is
// exactly one implementation of "what does the oracle say about this case".
//
// Usage:
//   node rust/tools/oracle-diff.mjs --case call-000000
//   node rust/tools/oracle-diff.mjs --case decimal-000123 --seed 1 --sections decimal
//   node rust/tools/oracle-diff.mjs --corpus rust/target/corpus.jsonl --case random-1-000007
//   node rust/tools/oracle-diff.mjs --case call-000000 --rust    # also run dump_cases

import {spawnSync} from "node:child_process";

import {
  canonicalJson,
  canonicalLocaleDefinition,
  encodeValue,
  f64Bits,
  f64FromBits,
  isMain,
  readTextLf,
  repoPath
} from "./harvest.mjs";
import {DEFAULT_SEED, SECTIONS, buildCorpus} from "./gen-corpus.mjs";

const d3 = await import(new URL("../../src/index.js", import.meta.url).href);

/**
 * Turns a canonical locale definition back into the object shape d3 accepts.
 *
 * The canonical form spells an absent optional key as `null`; d3 tests for
 * `undefined`, so a `null` left in place would be coerced to the four-character
 * string "null". Dropping the key restores d3's own default.
 */
export function toD3Locale(definition) {
  const out = {};
  for (const [key, value] of Object.entries(definition)) {
    if (value === null) continue;
    out[key] = value;
  }
  return out;
}

/**
 * `d3.formatLocale` applied to a definition, memoized on the definition object.
 *
 * A ten-million-case formatter run would otherwise rebuild a locale for every
 * single case, which is the dominant cost of the JavaScript side and buys nothing:
 * `formatLocale` is a pure function of its argument, and the object it returns owns
 * no state that a later call could observe. (The one piece of state d3 does keep --
 * `prefixExponent` in `src/formatPrefixAuto.js` -- is a module global, so it is
 * shared between locale objects whether or not they are reused; see the
 * `no-global-si-prefix-leak` entry in DIVERGENCES.md.)
 *
 * The key is the definition object itself. `gen-corpus.mjs` hands every case that
 * draws a named locale the *same* definition object out of the locale index, so the
 * cache hits without hashing anything; a corpus replayed from a file parses a fresh
 * object per record and simply misses, which is correct but slower.
 */
const localeCache = new WeakMap();

function d3LocaleFor(definition) {
  const cached = localeCache.get(definition);
  if (cached !== undefined) return cached;
  const locale = d3.formatLocale(toD3Locale(definition));
  localeCache.set(definition, locale);
  return locale;
}

/**
 * Computes the JavaScript answer for one corpus case.
 *
 * Returns `{status: "ok", output}` with the fixtures' value shape, `{status: "skip"}`
 * for a case DIVERGENCES.md excuses, or `{status: "threw"}` when the JavaScript
 * implementation itself raises -- which is data, not a crash, and is how the width
 * limit and locale validation deviations are observed.
 */
export function evaluateCase(record) {
  if (record.oracle?.skip) {
    return {status: "skip", reason: record.oracle.skip, output: null};
  }

  const value = record.value === null ? null : f64FromBits(record.value);
  const reference = record.reference === null ? null : f64FromBits(record.reference);

  try {
    switch (record.op) {
      case "format": {
        return ok(d3LocaleFor(record.locale_definition).format(record.specifier)(value));
      }
      case "formatPrefix": {
        return ok(d3LocaleFor(record.locale_definition).formatPrefix(record.specifier, reference)(value));
      }
      case "formatSpecifier": {
        // The recorder tags a rendered specifier `kind: "specifier"` rather than
        // `"text"`, because a normalized specifier and a formatted number are
        // different things even when both are strings. Matching that here is what
        // keeps a live evaluation comparable to the recording.
        return {status: "ok", output: {kind: "specifier", bits: null, text: String(d3.formatSpecifier(record.specifier))}, reason: null};
      }
      case "precisionFixed":
        return ok(d3.precisionFixed(value));
      case "precisionRound":
        return ok(d3.precisionRound(value, reference));
      case "precisionPrefix":
        return ok(d3.precisionPrefix(value, reference));

      // Section 4.1's ECMAScript numeric contract. These are the conversions the
      // Rust port must reimplement exactly, so the differential covers them
      // directly rather than only through the formatters that call them.
      case "numberToString":
        return ok(String(value));
      case "toFixed":
        return ok(value.toFixed(record.precision));
      case "toExponential":
        return ok(record.precision === null ? value.toExponential() : value.toExponential(record.precision));
      case "toPrecision":
        return ok(record.precision === null ? value.toPrecision() : value.toPrecision(record.precision));
      case "toLocaleStringEn":
        // Recorded the way type `d` actually consumes it (section 4.1): d3 calls
        // `toLocaleString("en")` and then strips the grouping separators, because
        // it wants the positional expansion, not a grouped rendering.
        return ok(value.toLocaleString("en").replace(/,/g, ""));
      case "toStringRadix":
        return ok(value.toString(record.radix));
      default:
        return {status: "threw", reason: `unknown operation ${record.op}`, output: null};
    }
  } catch (error) {
    return {status: "threw", reason: String(error && error.message ? error.message : error), output: null};
  }
}

function ok(value) {
  return {status: "ok", output: encodeValue(value), reason: null};
}

/** Asks the Rust binary about exactly one case. */
export function runSingleCase(record) {
  const line = canonicalJson(record) + "\n";
  const result = spawnSync(
    "cargo",
    ["run", "--quiet", "--manifest-path", "rust/Cargo.toml", "--bin", "dump_cases"],
    {cwd: repoPath("."), input: line, encoding: "utf8", shell: process.platform === "win32"}
  );
  if (result.error) throw result.error;
  return result;
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

export function parseArguments(argv) {
  const options = {caseId: null, corpus: null, seed: DEFAULT_SEED, random: 0, sweep: 0, sections: null, rust: false};
  const flags = argv.slice(2);
  for (let index = 0; index < flags.length; index++) {
    const flag = flags[index];
    const value = () => {
      const next = flags[++index];
      if (next === undefined) throw new Error(`oracle-diff.mjs: ${flag} needs a value`);
      return next;
    };
    if (flag === "--case") options.caseId = value();
    else if (flag === "--corpus") options.corpus = value();
    else if (flag === "--seed") options.seed = Number(value());
    else if (flag === "--random") options.random = Number(value());
    else if (flag === "--sections") options.sections = value().split(",").map((s) => s.trim());
    else if (flag === "--rust") options.rust = true;
    else throw new Error(`oracle-diff.mjs: unknown argument ${flag}`);
  }
  if (options.caseId === null) throw new Error("oracle-diff.mjs: --case <id> is required");

  // A `random-<seed>-<ordinal>` id names its own seed and its own position, so a
  // reproduction command copied from differential.mjs works whether or not the
  // person running it remembers to repeat --seed and --random.
  const seeded = /^(random|sweep)-(-?\d+)-(\d+)$/.exec(options.caseId);
  if (seeded) {
    options.seed = Number(seeded[2]);
    const ordinal = Number(seeded[3]) + 1;
    if (seeded[1] === "random") options.random = Math.max(options.random, ordinal);
    else options.sweep = Math.max(options.sweep ?? 0, ordinal);
    if (options.sections === null) options.sections = [seeded[1]];
  } else if (options.sections === null) {
    // Each generated section uses its own id prefix, so a reproduction command
    // copied out of a differential report needs no other flags.
    const prefixes = {
      "decimal-": "decimal",
      "call-": "calls",
      "regression-": "regressions",
      "boundary-": "boundary",
      "fixed_ties-": "fixed_ties",
      "significant_ties-": "significant_ties",
      "carry-": "carry"
    };
    const matched = Object.entries(prefixes).find(([prefix]) => options.caseId.startsWith(prefix));
    options.sections = matched ? [matched[1]] : [...SECTIONS];
  }
  return options;
}

export function findCase(options) {
  if (options.corpus) {
    for (const line of readTextLf(options.corpus).replace(/\n$/, "").split("\n")) {
      const record = JSON.parse(line);
      if (record.id === options.caseId) return record;
    }
    return null;
  }
  const {cases} = buildCorpus(options);
  return cases.find((record) => record.id === options.caseId) ?? null;
}

function describe(result) {
  if (result.status === "skip") return `skipped: DIVERGENCES.md entry "${result.reason}"`;
  if (result.status === "threw") return `threw: ${result.reason}`;
  const {output} = result;
  if (output.kind === "f64") return `f64 ${output.bits} (${output.text})`;
  return `${output.kind} ${JSON.stringify(output.text)}`;
}

function main() {
  const options = parseArguments(process.argv);
  const record = findCase(options);
  if (record === null) {
    console.error(
      `oracle-diff: no case ${JSON.stringify(options.caseId)} in sections ${options.sections.join(",")}` +
        ` at seed ${options.seed}`
    );
    process.exit(2);
  }

  console.log(canonicalJson(record, 2));
  console.log("");

  const recorded = record.oracle.expected;
  const node = evaluateCase(record);
  console.log(`recorded : ${recorded === null ? "(not recorded; seeded case)" : describe({status: "ok", output: recorded})}`);
  console.log(`node     : ${describe(node)}`);

  if (recorded !== null && node.status === "ok" && !sameOutput(recorded, node.output)) {
    console.log("");
    console.log("oracle-diff: the running engine disagrees with the recording. This is engine drift,");
    console.log("oracle-diff: not a port defect; run check-oracle-metadata.mjs.");
    process.exit(1);
  }

  if (!options.rust) {
    console.log("");
    console.log("oracle-diff: pass --rust to also ask rust/src/bin/dump_cases about this case.");
    return;
  }

  const result = runSingleCase(record);
  if (result.stderr.trim()) console.log(result.stderr.trimEnd());
  const line = result.stdout.split("\n").find((l) => l.trim().length > 0);
  if (!line) {
    console.log("rust     : dump_cases emitted nothing");
    process.exit(1);
  }
  const rust = JSON.parse(line);
  if (rust.status === "unimplemented") {
    console.log(`rust     : unimplemented -- ${rust.error}`);
    process.exit(1);
  }
  if (rust.status === "rejected") {
    console.log(`rust     : rejected -- ${rust.error}`);
    process.exit(node.status === "skip" ? 0 : 1);
  }
  console.log(`rust     : ${describe({status: "ok", output: rust.output})}`);
  if (node.status !== "ok" || !sameOutput(node.output, rust.output)) {
    console.log("");
    console.log("oracle-diff: MISMATCH");
    process.exit(1);
  }
  console.log("");
  console.log("oracle-diff: agree");
}

function sameOutput(left, right) {
  if (left === null || right === null) return left === right;
  if (left.kind !== right.kind) return false;
  return left.kind === "f64" ? left.bits === right.bits : left.text === right.text;
}

if (isMain(import.meta.url)) {
  main();
}

// Re-exported so a caller that already has this module does not need harvest.mjs
// just to spell a value the same way the corpus does.
export {canonicalLocaleDefinition, f64Bits, f64FromBits};
