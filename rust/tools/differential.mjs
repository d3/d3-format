#!/usr/bin/env node
// Differential runner (MIGRATION_TO_RUST.md section 3.6).
//
// Builds a corpus, computes the JavaScript answer for every case, asks
// `rust/src/bin/dump_cases` for the Rust answer, and compares decoded strings and
// numeric results exactly. It never writes a fixture: on disagreement it writes an
// artifact under rust/target/ and prints an oracle-diff.mjs command that reproduces
// a single case.
//
// Usage:
//   node rust/tools/differential.mjs
//   node rust/tools/differential.mjs --seed 1 --random 20000
//   node rust/tools/differential.mjs --corpus rust/target/corpus.jsonl
//   node rust/tools/differential.mjs --expect unimplemented   # Phase 1 wiring check
//
//   # Section 6.3's per-PR gate and its scheduled pre-release counterpart. `ci` and
//   # `prerelease` are named corpora, defined by NAMED_CORPORA below; --cases sizes
//   # the drawn formatter section each of them fills.
//   node rust/tools/differential.mjs --corpus ci --seed 1 --cases 200000
//   node rust/tools/differential.mjs --corpus prerelease --seed 1 --cases 11000000 --release
//
//   # Section 4.3's Phase 2 gate: the structured corpora plus the supplementary
//   # sweep, restricted to the operations the port has actually implemented.
//   node rust/tools/differential.mjs --sections boundary,fixed_ties,significant_ties,carry,sweep \
//     --ops primitives --release
//
// The --ops filter is not a way to make a failing run pass. It selects which
// operations are asked about at all; any case that is asked about and answers
// "unimplemented" still fails the run, and the summary always reports how many
// cases the filter excluded.

import {spawnSync} from "node:child_process";
import {mkdirSync, writeFileSync} from "node:fs";
import {dirname} from "node:path";

import {
  FIXTURE_DIR,
  canonicalJson,
  f64FromBits,
  isMain,
  namedLocaleIndex,
  readTextLf,
  repoPath
} from "./harvest.mjs";
import {DEFAULT_SEED, SECTIONS, SWEEP_SIZE, corpusHeader, iterateCases} from "./gen-corpus.mjs";
import {evaluateCase} from "./oracle-diff.mjs";

const ARTIFACT_DIR = "rust/target/differential";
const SUMMARY_SCHEMA = "d3-format-rust/differential-summary@1";

/**
 * Section 4.1's ECMAScript conversions: everything Phase 2 of the migration
 * implements. Named rather than open-ended so that adding an operation to the port
 * is a deliberate edit here, not something a corpus change can do by accident.
 */
export const PRIMITIVE_OPS = [
  "numberToString",
  "toFixed",
  "toExponential",
  "toPrecision",
  "toLocaleStringEn",
  "toStringRadix"
];

/** The operations that compare a *formatter* result rather than a numeric primitive. */
export const FORMATTER_OPS = ["format", "formatPrefix"];

/**
 * The named corpora section 6.3 invokes.
 *
 * Section 6.3 spells the per-PR gate `--corpus ci --seed 1 --cases 200000`, which
 * predates this tool: `--corpus` took a file path and there was no `--cases` at
 * all. The document's spelling is the one worth keeping -- a per-PR gate should
 * name a corpus, not a path into `target/` that nothing generated -- so the names
 * are defined here and the flag accepts either a name or a path.
 *
 * `sections` are the fixed sections, which always run in full. `--cases` sizes the
 * *drawn* section, which is the seeded formatter differential: section 7's exit
 * conditions read "per-PR 200,000-case differential" and "pre-release 10,000,000-case
 * and complete structured corpora", so the case budget is the drawn formatter count
 * and the structured corpora are additional in the run that names them.
 *
 * `minimumOutputComparisons` is section 6.3's "at least 10,000,000 formatter
 * comparisons" as an assertion rather than as prose. It is checked against what the
 * run actually compared, so a corpus change that quietly turned formatter cases
 * into declined ones would fail rather than pass; `defaultCases` carries the
 * headroom that makes it reachable, because roughly one drawn specifier in fifteen
 * is type `c` and is a policy check rather than an output comparison.
 */
export const NAMED_CORPORA = {
  ci: {
    description:
      "the per-PR gate: every recorded call, every recorded ECMAScript conversion," +
      " the pinned regressions, section 4.1's boundary corpus, and a drawn formatter sample",
    sections: ["calls", "decimal", "regressions", "boundary"],
    drawn: "random",
    defaultCases: 200000,
    sweep: 0,
    minimumOutputComparisons: 0
  },
  prerelease: {
    description:
      "the scheduled pre-release corpus: the ci corpus, the complete structured corpora," +
      " the supplementary sweep, and at least ten million drawn formatter comparisons",
    sections: [
      "calls",
      "decimal",
      "regressions",
      "boundary",
      "fixed_ties",
      "significant_ties",
      "carry",
      "sweep"
    ],
    drawn: "random",
    defaultCases: 11000000,
    sweep: SWEEP_SIZE,
    minimumOutputComparisons: 10000000
  }
};

/**
 * The smallest drawn sample the per-locale floor is applied to.
 *
 * Section 7 requires the 59-locale differential to record per-locale counts and
 * assert a floor, because section 4.3's rule -- a bucket drawing zero cases is a
 * gate failure, not a pass -- applies to locales too. The draw is uniform, so the
 * floor has to be one a correct uniform draw cannot fail by chance.
 *
 * With `n` drawn cases over `L` locales the per-locale count is Binomial(n, 1/L)
 * with mean `mu = n / L`, and a Chernoff bound gives P(count < mu/2) <= exp(-mu/8)
 * per locale. At `mu = 200` the union bound over 59 locales is under 1e-9, so a
 * half-of-mean floor cannot plausibly fail unless the draw is genuinely broken.
 * Below `59 * 200` drawn cases the sample is too small for that argument, so the
 * runner reports the census and says the floor did not apply -- and a *named*
 * corpus is refused outright at that size, so no gate can reach the lenient path.
 */
export const LOCALE_FLOOR_MEAN = 200;
export const LOCALE_FLOOR_FRACTION = 0.5;

/**
 * Runs the Rust side.
 *
 * `cargo run` is used rather than a hard-coded target path so the tool works on any
 * platform and any profile without knowing where cargo put the executable. Build
 * output goes to stderr and is left visible; only stdout is data. A run that is
 * going to spawn the binary hundreds of times passes `bin` instead, having built it
 * once; see `buildDumpCases`.
 */
export function runDumpCases(corpusText, {release = false, bin = null} = {}) {
  const command = bin ?? "cargo";
  const args = bin
    ? []
    : ["run", "--quiet", "--manifest-path", "rust/Cargo.toml", "--bin", "dump_cases"];
  if (!bin && release) args.push("--release");
  const result = spawnSync(command, args, {
    cwd: repoPath("."),
    // The corpus is ASCII by construction; encoding it explicitly keeps the pipe
    // free of any platform default, and leaving `encoding` unset means stdout comes
    // back as bytes that this function decodes exactly once, below.
    input: Buffer.from(corpusText, "utf8"),
    maxBuffer: 1024 * 1024 * 1024,
    shell: bin === null && process.platform === "win32"
  });
  if (result.error) throw result.error;
  return {
    status: result.status,
    // Decoded as UTF-8 exactly once, here. Comparing decoded strings rather than
    // bytes is what section 3.6 asks for, and it means an encoding difference shows
    // up as a mismatch on the string rather than silently as unequal bytes.
    stdout: result.stdout.toString("utf8"),
    stderr: result.stderr.toString("utf8")
  };
}

/**
 * Builds `dump_cases` once and reports where cargo put it.
 *
 * A two-million-case run is split into batches, and each batch is one process. Re-
 * resolving the build graph per batch would cost more than the comparison does, so
 * the build happens once and the batches exec the binary directly.
 */
export function buildDumpCases({release = false} = {}) {
  const args = ["build", "--quiet", "--manifest-path", "rust/Cargo.toml", "--bin", "dump_cases"];
  if (release) args.push("--release");
  const build = spawnSync("cargo", args, {
    cwd: repoPath("."),
    encoding: "utf8",
    shell: process.platform === "win32"
  });
  if (build.error) throw build.error;
  if (build.stderr && build.stderr.trim()) console.error(build.stderr.trimEnd());
  if (build.status !== 0) throw new Error(`differential: cargo build exited ${build.status}`);
  const suffix = process.platform === "win32" ? ".exe" : "";
  return repoPath(`rust/target/${release ? "release" : "debug"}/dump_cases${suffix}`);
}

export function parseOutputJsonl(text) {
  const outputs = new Map();
  for (const line of text.split("\n")) {
    if (line.trim().length === 0) continue;
    const record = JSON.parse(line);
    if (outputs.has(record.id)) throw new Error(`differential: dump_cases emitted ${record.id} twice`);
    outputs.set(record.id, record);
  }
  return outputs;
}

/**
 * Compares one case's three answers: what the oracle recorded, what Node computes
 * now, and what Rust produced.
 *
 * Node-versus-recording is checked first. If those disagree the engine has drifted
 * and the Rust result is not evidence of anything, so it is reported as engine drift
 * rather than as a port failure.
 */
export function compareCase(record, nodeResult, rustRecord) {
  const recorded = record.oracle.expected;

  if (recorded !== null && nodeResult.status === "ok") {
    if (!sameValue(recorded, nodeResult.output)) {
      return {
        kind: "engine-drift",
        detail: "the running engine no longer reproduces the recorded oracle output",
        expected: recorded,
        actual: nodeResult.output
      };
    }
  }

  if (rustRecord === undefined) {
    return {kind: "missing", detail: "dump_cases emitted no record for this case"};
  }

  if (rustRecord.status === "unimplemented") {
    return {kind: "unimplemented", detail: rustRecord.error};
  }

  if (nodeResult.status === "skip") {
    // A case DIVERGENCES.md excuses. Rust must reject it, not answer it.
    if (rustRecord.status === "rejected") return null;
    return {
      kind: "should-reject",
      detail: `case is excused by DIVERGENCES.md entry "${nodeResult.reason}", so Rust must reject it`,
      expected: null,
      actual: rustRecord.output
    };
  }

  if (rustRecord.status === "rejected") {
    return {
      kind: "rejected",
      detail: `Rust rejected a case JavaScript answers: ${rustRecord.error}`,
      expected: nodeResult.output,
      actual: null
    };
  }

  if (!sameValue(nodeResult.output, rustRecord.output)) {
    return {kind: "mismatch", detail: "Rust and Node disagree", expected: nodeResult.output, actual: rustRecord.output};
  }

  return null;
}

/**
 * Exact equality on the fixtures' value shape.
 *
 * Numeric results compare by bit pattern, so `-0` never equals `0` and two NaNs
 * agree only if their payloads do. Text compares as decoded strings.
 */
export function sameValue(left, right) {
  if (left === null || right === null) return left === right;
  if (left.kind !== right.kind) return false;
  if (left.kind === "f64") return left.bits === right.bits;
  return left.text === right.text;
}

function summarize(value) {
  if (value === null || value === undefined) return "null";
  if (value.kind === "f64") return `f64 ${value.bits} (${String(f64FromBits(value.bits))})`;
  return JSON.stringify(value.text);
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
    ops: null,
    corpus: null,
    corpusName: null,
    cases: null,
    summary: null,
    release: false,
    batch: 50000,
    expect: "agreement",
    maxReport: 10
  };
  const flags = argv.slice(2);
  for (let index = 0; index < flags.length; index++) {
    const flag = flags[index];
    const value = () => {
      const next = flags[++index];
      if (next === undefined) throw new Error(`differential.mjs: ${flag} needs a value`);
      return next;
    };
    if (flag === "--seed") options.seed = Number(value());
    else if (flag === "--random") options.random = Number(value());
    else if (flag === "--sweep") options.sweep = Number(value());
    else if (flag === "--sections") options.sections = value().split(",").map((s) => s.trim());
    else if (flag === "--ops") options.ops = value();
    else if (flag === "--corpus") options.corpus = value();
    else if (flag === "--cases") options.cases = Number(value());
    else if (flag === "--summary") options.summary = value();
    else if (flag === "--release") options.release = true;
    else if (flag === "--batch") options.batch = Number(value());
    else if (flag === "--expect") options.expect = value();
    else throw new Error(`differential.mjs: unknown argument ${flag}`);
  }
  if (!["agreement", "unimplemented"].includes(options.expect)) {
    throw new Error("differential.mjs: --expect takes 'agreement' or 'unimplemented'");
  }
  if (!Number.isSafeInteger(options.batch) || options.batch < 1) {
    throw new Error("differential.mjs: --batch must be a positive integer");
  }

  // A named corpus and a corpus file are the same flag, because section 6.3 spells
  // it `--corpus ci` and the tool already spelled it `--corpus <path>`. A name wins
  // over a file of the same name: the names are reserved, and a corpus file is
  // written to a path in `rust/target/`, never to a bare word.
  if (options.corpus !== null && Object.prototype.hasOwnProperty.call(NAMED_CORPORA, options.corpus)) {
    options.corpusName = options.corpus;
    options.corpus = null;
  }

  if (options.cases !== null) {
    if (options.corpusName === null) {
      throw new Error(
        "differential.mjs: --cases sizes the drawn section of a named corpus, so it needs" +
          ` --corpus ${Object.keys(NAMED_CORPORA).join("|")}; use --random or --sweep to size a section directly`
      );
    }
    if (!Number.isSafeInteger(options.cases) || options.cases < 1) {
      throw new Error("differential.mjs: --cases must be a positive integer");
    }
  }

  if (options.corpusName !== null) {
    const corpus = NAMED_CORPORA[options.corpusName];
    if (options.sections !== null) {
      throw new Error(
        `differential.mjs: --corpus ${options.corpusName} defines its own sections; --sections cannot also be given`
      );
    }
    options.cases = options.cases ?? corpus.defaultCases;
    options.random = options.cases;
    options.sweep = corpus.sweep;
    options.sections = [...corpus.sections, corpus.drawn];

    // Section 7's per-locale floor is a gate, so a named corpus may not be small
    // enough to reach the sample size at which the floor stops being applied.
    const minimum = LOCALE_FLOOR_MEAN * namedLocaleCount();
    if (options.cases < minimum) {
      throw new Error(
        `differential.mjs: --corpus ${options.corpusName} draws ${options.cases} formatter case(s), which is below` +
          ` the ${minimum} needed for the per-locale floor to be assertable (section 7).` +
          " Raise --cases, or run an ad-hoc --random sample if a smaller check is what you want."
      );
    }
  }

  if (options.sections === null) {
    options.sections = options.random > 0 ? [...SECTIONS] : ["calls", "decimal", "regressions"];
  }
  return options;
}

let namedLocaleCache = null;

/** The named locales the drawn section draws from, resolved once. */
function namedLocales() {
  if (namedLocaleCache === null) {
    namedLocaleCache = [...namedLocaleIndex().byName.keys()].sort();
  }
  return namedLocaleCache;
}

function namedLocaleCount() {
  return namedLocales().length;
}

/**
 * Section 7's per-locale floor.
 *
 * Returns the census the runner reports and the verdict it gates on. A locale that
 * drew nothing is the failure section 4.3 names; a locale that drew far less than
 * its share is the same failure caught earlier, before a draw degenerates all the
 * way to zero.
 */
export function assessLocaleCoverage(counts, drawn) {
  const names = namedLocales();
  const census = names.map((name) => ({name, count: counts.get(name) ?? 0}));
  const mean = drawn / names.length;
  const applicable = mean >= LOCALE_FLOOR_MEAN;
  const floor = applicable ? Math.floor(mean * LOCALE_FLOOR_FRACTION) : 0;
  const ordered = [...census].sort((left, right) =>
    left.count === right.count ? left.name.localeCompare(right.name) : left.count - right.count
  );
  const unnamed = [...counts.keys()].filter((name) => !names.includes(name)).sort();
  return {
    locales: names.length,
    drawn,
    mean,
    floor,
    applicable,
    census,
    unnamed,
    minimum: ordered[0] ?? null,
    maximum: ordered[ordered.length - 1] ?? null,
    below: ordered.filter((entry) => entry.count < floor)
  };
}

/**
 * Sections a run may legitimately draw nothing from.
 *
 * `regressions.jsonl` is empty until a differential run pins a case and a human
 * promotes it, so an empty regressions section is the normal state rather than the
 * generator failure section 4.3 describes. Every other declared section producing
 * nothing is a failure.
 */
const EMPTY_BY_DESIGN = ["regressions"];

/**
 * The census section 6.3 gates on: did this run actually do what was asked?
 *
 * "Unchanged state is not success unless the command completed and reported the
 * expected case count." A run that silently compared a tenth of the cases, or that
 * drew from a tenth of the locales, exits zero and changes nothing -- which is
 * indistinguishable from success unless the counts themselves are asserted.
 */
export function assessCensus({options, corpus, bySection, drawn, formatter, coverage}) {
  const problems = [];

  if (corpus !== null) {
    if (drawn !== options.cases) {
      problems.push(
        `--cases asked for ${options.cases} drawn ${corpus.drawn} case(s); the run compared ${drawn}` +
          (options.ops === null ? "" : ` (--ops ${options.ops} filtered some of them out)`)
      );
    }
    for (const section of corpus.sections) {
      if (EMPTY_BY_DESIGN.includes(section)) continue;
      if ((bySection.get(section) ?? 0) === 0) {
        problems.push(`corpus ${options.corpusName} declares section ${section}, and no case came from it`);
      }
    }
    if (formatter.outputComparisons < corpus.minimumOutputComparisons) {
      problems.push(
        `corpus ${options.corpusName} requires at least ${corpus.minimumOutputComparisons} formatter output` +
          ` comparison(s) (section 6.3); the run made ${formatter.outputComparisons}. Raise --cases.`
      );
    }
  }

  if (coverage.applicable && coverage.below.length > 0) {
    const named = coverage.below
      .slice(0, 5)
      .map((entry) => `${entry.name}=${entry.count}`)
      .join(", ");
    problems.push(
      `${coverage.below.length} of ${coverage.locales} named locale(s) drew fewer than the floor of` +
        ` ${coverage.floor} case(s) over ${coverage.drawn} draw(s): ${named}` +
        (coverage.below.length > 5 ? ", ..." : "") +
        ". Section 7: a locale drawing zero cases is a gate failure, not a pass."
    );
  }
  if (coverage.unnamed.length > 0) {
    problems.push(
      `the drawn section produced ${coverage.unnamed.length} locale id(s) that are not named locales:` +
        ` ${coverage.unnamed.slice(0, 5).join(", ")}`
    );
  }

  return problems;
}

/** Prints the census the gate above evaluates, whatever the verdict. */
function reportCensus({corpus, bySection, drawn, drawnSection, formatter, coverage, elapsedMs}) {
  const sections = SECTIONS.filter((section) => bySection.has(section))
    .map((section) => `${section}=${bySection.get(section)}`)
    .join(", ");
  console.error(`\ndifferential: by section: ${sections || "(none)"}`);
  console.error(
    `differential: formatter cases ${formatter.cases}` +
      ` (${formatter.outputComparisons} output comparison(s), ${formatter.declinedByPolicy} declined by policy)`
  );

  if (drawn === 0) {
    console.error(`differential: no ${drawnSection} case was drawn, so there is no per-locale census`);
  } else if (coverage.applicable) {
    console.error(
      `differential: ${coverage.locales} named locale(s) over ${coverage.drawn} drawn case(s):` +
        ` floor ${coverage.floor}, least ${coverage.minimum.name}=${coverage.minimum.count},` +
        ` most ${coverage.maximum.name}=${coverage.maximum.count}`
    );
  } else {
    console.error(
      `differential: ${coverage.locales} named locale(s) over ${coverage.drawn} drawn case(s):` +
        ` least ${coverage.minimum.name}=${coverage.minimum.count},` +
        ` most ${coverage.maximum.name}=${coverage.maximum.count};` +
        ` the floor needs ${LOCALE_FLOOR_MEAN * coverage.locales} draws to be assertable and is not applied here`
    );
  }
  if (corpus !== null) {
    console.error(`differential: wall clock ${(elapsedMs / 1000).toFixed(1)}s`);
  }
}

/**
 * Writes the machine-readable summary section 6.3 asks a scheduled run to upload.
 *
 * The scheduled workflow reads this file and asserts the count it reports, rather
 * than reading the exit code alone: an exit code cannot distinguish "compared ten
 * million cases" from "compared none of them".
 */
function writeSummary({options, corpus, header, tally, bySection, formatter, coverage, census, findings, elapsedMs, status}) {
  const path = options.summary ?? (corpus === null ? null : `${ARTIFACT_DIR}/summary.json`);
  if (path === null) return;

  const sections = {};
  for (const section of SECTIONS) {
    if (bySection.has(section)) sections[section] = bySection.get(section);
  }
  const locales = {};
  for (const {name, count} of coverage.census) locales[name] = count;

  const summary = {
    schema: SUMMARY_SCHEMA,
    corpus: options.corpusName,
    seed: header.seed,
    sections: header.sections,
    operations: options.ops ?? "all",
    release: options.release,
    batch: options.batch,
    oracle: header.oracle,
    status,
    duration_ms: elapsedMs,
    cases: {
      seen: tally.seen,
      asked: tally.asked,
      filtered: tally.filtered,
      agreed: tally.ok,
      unimplemented: tally.unimplemented,
      findings: findings.length
    },
    requested_drawn_cases: options.cases,
    by_section: sections,
    formatter: {
      cases: formatter.cases,
      output_comparisons: formatter.outputComparisons,
      declined_by_policy: formatter.declinedByPolicy,
      minimum_output_comparisons: corpus === null ? 0 : corpus.minimumOutputComparisons
    },
    locale_coverage: {
      named_locales: coverage.locales,
      drawn: coverage.drawn,
      floor: coverage.floor,
      floor_applied: coverage.applicable,
      minimum: coverage.minimum === null ? null : {name: coverage.minimum.name, count: coverage.minimum.count},
      maximum: coverage.maximum === null ? null : {name: coverage.maximum.name, count: coverage.maximum.count},
      counts: locales
    },
    census_problems: census
  };

  const resolved = repoPath(path);
  mkdirSync(dirname(resolved), {recursive: true});
  writeFileSync(resolved, Buffer.from(canonicalJson(summary, 2) + "\n", "utf8"));
  console.error(`differential: summary written to ${path}`);
}

/**
 * Resolves `--ops` into a predicate over case records.
 *
 * This selects which operations are asked about; it is not a way to excuse a
 * failure. A case that is asked about and answers "unimplemented" still fails the
 * run, and the summary always states how many cases the filter left out.
 */
export function operationFilter(spec) {
  if (spec === null || spec === "all") return null;
  const names = spec === "primitives" ? PRIMITIVE_OPS : spec.split(",").map((name) => name.trim());
  for (const name of names) {
    if (name.length === 0) throw new Error("differential.mjs: --ops takes a non-empty operation list");
  }
  return (record) => names.includes(record.op);
}

/** Yields batches of at most `size` records from an iterable. */
function* batched(records, size) {
  let batch = [];
  for (const record of records) {
    batch.push(record);
    if (batch.length === size) {
      yield batch;
      batch = [];
    }
  }
  if (batch.length > 0) yield batch;
}

function main() {
  const options = parseArguments(process.argv);
  const accepts = operationFilter(options.ops);
  const corpus = options.corpusName === null ? null : NAMED_CORPORA[options.corpusName];

  let records;
  let header;
  if (options.corpus) {
    const lines = readTextLf(options.corpus).replace(/\n$/, "").split("\n");
    header = JSON.parse(lines[0]);
    records = (function* () {
      for (let index = 1; index < lines.length; index++) yield JSON.parse(lines[index]);
    })();
  } else {
    header = corpusHeader({
      seed: options.seed,
      sections: SECTIONS.filter((section) => options.sections.includes(section)),
      count: null
    });
    records = iterateCases(options);
  }
  if (corpus !== null) {
    console.error(`differential: corpus ${options.corpusName} -- ${corpus.description}`);
    console.error(`differential: ${options.cases} drawn ${corpus.drawn} case(s) requested`);
  }
  console.error(
    `differential: seed ${header.seed}, sections ${header.sections.join(",")}` +
      `, operations ${options.ops ?? "all"}, batches of ${options.batch}`
  );

  // Built once, then executed directly. A two-million-case run is hundreds of
  // batches, and re-resolving cargo's build graph for each of them would cost more
  // than the comparison it is there to perform.
  const bin = buildDumpCases({release: options.release});

  const findings = [];
  const tally = {seen: 0, asked: 0, filtered: 0, ok: 0, unimplemented: 0};
  // Section 6.3 says unchanged state is not success unless the run "completed and
  // reported the expected case count", and section 7 wants per-locale counts with a
  // floor. Both are censuses of what was actually asked, so they are taken here
  // rather than inferred afterwards from the corpus definition.
  const bySection = new Map();
  const localeCounts = new Map();
  const formatter = {cases: 0, outputComparisons: 0, declinedByPolicy: 0};
  let sawExitThree = false;
  const started = Date.now();

  for (const batch of batched(records, options.batch)) {
    const asked = [];
    for (const record of batch) {
      tally.seen += 1;
      if (accepts !== null && !accepts(record)) {
        tally.filtered += 1;
        continue;
      }
      asked.push(record);
    }
    if (asked.length === 0) continue;
    tally.asked += asked.length;

    const batchHeader = corpusHeader({seed: header.seed, sections: header.sections, count: asked.length});
    const corpusText = [batchHeader, ...asked].map((record) => canonicalJson(record) + "\n").join("");

    const rust = runDumpCases(corpusText, {release: options.release, bin});
    if (rust.status !== 0 && rust.status !== 3) {
      console.error(rust.stderr.trimEnd());
      console.error(`differential: dump_cases exited ${rust.status}`);
      process.exit(1);
    }
    if (rust.status === 3) sawExitThree = true;
    const outputs = parseOutputJsonl(rust.stdout);

    for (const record of asked) {
      const section = record.origin ?? "unknown";
      bySection.set(section, (bySection.get(section) ?? 0) + 1);
      // The drawn section is the 59-locale differential, whether it was reached
      // through a named corpus or through an ad-hoc `--random` sample, so its census
      // is taken either way.
      if (section === "random" && record.locale_id !== null) {
        localeCounts.set(record.locale_id, (localeCounts.get(record.locale_id) ?? 0) + 1);
      }

      const nodeResult = evaluateCase(record);
      const finding = compareCase(record, nodeResult, outputs.get(record.id));

      if (FORMATTER_OPS.includes(record.op)) {
        formatter.cases += 1;
        // A drawn type `c` specifier handed a number is a policy check, not an
        // output comparison: DIVERGENCES.md entry `no-input-coercion` makes the
        // port's refusal the expected answer. Counting the two separately is what
        // lets the pre-release floor be asserted on comparisons that produced a
        // string on both sides.
        if (nodeResult.status === "ok") formatter.outputComparisons += 1;
        else formatter.declinedByPolicy += 1;
      }

      if (finding === null) {
        tally.ok += 1;
        continue;
      }
      if (finding.kind === "unimplemented") {
        tally.unimplemented += 1;
        continue;
      }
      // Bounded, because a systematic defect disagrees on every case and a report
      // that tries to hold two million findings runs out of memory before it prints.
      if (findings.length < 1000) {
        findings.push({record, nodeResult, rustRecord: outputs.get(record.id), finding});
      }
    }
    console.error(
      `differential: ${tally.asked} asked, ${tally.ok} agree, ${findings.length} disagreement(s)` +
        `, ${((Date.now() - started) / 1000).toFixed(1)}s`
    );
  }
  const elapsedMs = Date.now() - started;

  // Phase 1's meaningful assertion: the protocol round-trips and the Rust side is
  // uniformly, explicitly not implemented. This cannot be mistaken for conformance
  // because it fails the moment any case reports a value.
  if (options.expect === "unimplemented") {
    const problems = [];
    if (tally.unimplemented !== tally.asked) {
      problems.push(
        `expected all ${tally.asked} case(s) to report "unimplemented", got ${tally.unimplemented}` +
          ` (${tally.ok} answered, ${findings.length} other finding(s))`
      );
    }
    if (!sawExitThree) problems.push("expected dump_cases to exit 3 while unimplemented");
    for (const {record, finding} of findings.slice(0, 5)) {
      problems.push(`${record.id}: ${finding.kind} -- ${finding.detail}`);
    }
    if (problems.length > 0) {
      console.error("differential --expect unimplemented: the Phase 1 wiring check failed");
      for (const problem of problems) console.error(`  - ${problem}`);
      process.exit(1);
    }
    console.error(
      `differential: protocol round-trips over ${tally.asked} case(s);` +
        " every case reports \"unimplemented\", as Phase 1 requires"
    );
    return;
  }

  const scope =
    tally.filtered > 0
      ? ` (${tally.filtered} of ${tally.seen} case(s) not asked about, per --ops ${options.ops})`
      : "";

  // Section 7's per-locale floor, and section 6.3's "reported the expected case
  // count". Both are computed before any early exit so that the summary artifact
  // describes the run whatever its verdict was.
  const drawnSection = corpus === null ? "random" : corpus.drawn;
  const drawn = bySection.get(drawnSection) ?? 0;
  const coverage = assessLocaleCoverage(localeCounts, drawn);
  const census = assessCensus({options, corpus, bySection, drawn, formatter, coverage});
  reportCensus({corpus, bySection, drawn, drawnSection, formatter, coverage, elapsedMs});

  const status =
    tally.unimplemented > 0
      ? "unimplemented"
      : findings.length > 0
        ? "mismatch"
        : census.length > 0
          ? "census"
          : "agreement";
  writeSummary({options, corpus, header, tally, bySection, formatter, coverage, census, findings, elapsedMs, status});

  if (tally.unimplemented > 0) {
    console.error(
      `differential: ${tally.unimplemented} of ${tally.asked} asked case(s) reported "unimplemented"${scope}.` +
        " The Rust formatting API is incomplete (MIGRATION_TO_RUST.md, Phases 3 and 4)." +
        " Restrict --ops to what the port implements, or run --expect unimplemented to check the wiring."
    );
    process.exit(1);
  }

  if (findings.length === 0) {
    if (census.length > 0) {
      console.error(
        `\ndifferential: ${tally.ok} case(s) agree exactly${scope}, but the run is not the one that was asked for:`
      );
      for (const problem of census) console.error(`  - ${problem}`);
      console.error(
        "\ndifferential: section 6.3 -- unchanged state is not success unless the command completed" +
          " and reported the expected case count."
      );
      process.exit(1);
    }
    console.error(`differential: ${tally.ok} case(s) agree exactly${scope}`);
    return;
  }

  const first = findings[0];
  mkdirSync(repoPath(ARTIFACT_DIR), {recursive: true});
  const artifactPath = `${ARTIFACT_DIR}/first-mismatch.json`;
  writeFileSync(
    repoPath(artifactPath),
    Buffer.from(
      canonicalJson(
        {
          schema: "d3-format-rust/differential-mismatch@1",
          provenance: {
            corpus_schema: header.schema,
            generator_version: header.generator_version,
            d3_format_version: header.d3_format_version,
            oracle: header.oracle,
            seed: header.seed,
            sections: header.sections,
            cases: tally.asked,
            fixtures: `${FIXTURE_DIR}`
          },
          finding: first.finding.kind,
          detail: first.finding.detail,
          case: first.record,
          node: first.nodeResult.output ?? null,
          rust: first.rustRecord ?? null,
          total_findings: findings.length
        },
        2
      ) + "\n",
      "utf8"
    )
  );
  writeRegressionCandidates(findings);

  console.error(`\ndifferential: ${findings.length} disagreement(s) over ${tally.asked} case(s)${scope}`);
  for (const {record, finding} of findings.slice(0, options.maxReport)) {
    console.error(`  ${record.id} [${finding.kind}] ${finding.detail}`);
    console.error(`    node: ${summarize(finding.expected)}`);
    console.error(`    rust: ${summarize(finding.actual)}`);
  }
  if (findings.length > options.maxReport) {
    console.error(`  ... and ${findings.length - options.maxReport} more`);
  }
  console.error(`\ndifferential: first mismatch written to ${artifactPath}`);
  console.error("differential: reproduce that single case with\n");
  console.error(`  node rust/tools/oracle-diff.mjs --case ${JSON.stringify(first.record.id)} \\`);
  console.error(`    --seed ${header.seed} --sections ${header.sections.join(",")}`);
  console.error("\ndifferential: fixtures are never edited to make this agree (section 7).");
  process.exit(1);
}

/**
 * Writes the minimized regressions section 7's Phase 2 failure rule asks for: the
 * input bits, the operation, the precision, and the oracle metadata, one canonical
 * JSON object per line.
 *
 * The file lands under `rust/target/`, never in `rust/fixtures/`. Section 7 is
 * explicit that when Rust and the oracle disagree the oracle wins, so a tool that
 * could write into the fixture directory on failure is a tool that could hide the
 * defect it just found. Promoting a candidate into `regressions.jsonl` is a
 * deliberate, reviewed act.
 */
function writeRegressionCandidates(findings) {
  const lines = [];
  for (const {record, nodeResult, rustRecord, finding} of findings) {
    if (record.value === undefined) continue;
    lines.push(
      canonicalJson({
        id: record.id,
        op: record.op,
        origin: record.origin ?? null,
        locale_id: record.locale_id ?? null,
        locale_sha256: record.locale_sha256 ?? null,
        locale_definition: record.locale_definition ?? null,
        specifier: record.specifier ?? null,
        precision: record.precision ?? null,
        radix: record.radix ?? null,
        reference: record.reference ?? null,
        value: record.value ?? null,
        oracle: {
          expected: nodeResult.output ?? null,
          assertion_id: record.oracle?.assertion_id ?? null,
          test_file: record.oracle?.test_file ?? null,
          test_index: record.oracle?.test_index ?? null
        },
        finding: finding.kind,
        rust: rustRecord?.output ?? null
      }) + "\n"
    );
  }
  if (lines.length === 0) return;
  const path = `${ARTIFACT_DIR}/regression-candidates.jsonl`;
  writeFileSync(repoPath(path), Buffer.from(lines.join(""), "utf8"));
  console.error(
    `differential: ${lines.length} minimized regression candidate(s) written to ${path};` +
      " review them and promote by hand into rust/fixtures/*/regressions.jsonl"
  );
}

if (isMain(import.meta.url)) {
  main();
}
