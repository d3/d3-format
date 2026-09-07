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
// Phase 1 note: the Rust formatting API does not exist, so `dump_cases` answers
// every case "unimplemented" and this tool exits non-zero. That is the correct
// result, not a bug -- see the --expect flag for the check that is meaningful now.

import {spawnSync} from "node:child_process";
import {mkdirSync, writeFileSync} from "node:fs";

import {FIXTURE_DIR, canonicalJson, f64FromBits, isMain, readTextLf, repoPath} from "./harvest.mjs";
import {DEFAULT_SEED, SECTIONS, buildCorpus, renderCorpus} from "./gen-corpus.mjs";
import {evaluateCase} from "./oracle-diff.mjs";

const ARTIFACT_DIR = "rust/target/differential";

/**
 * Runs the Rust side.
 *
 * `cargo run` is used rather than a hard-coded target path so the tool works on any
 * platform and any profile without knowing where cargo put the executable. Build
 * output goes to stderr and is left visible; only stdout is data.
 */
export function runDumpCases(corpusText, {release = false} = {}) {
  const args = ["run", "--quiet", "--manifest-path", "rust/Cargo.toml", "--bin", "dump_cases"];
  if (release) args.push("--release");
  const result = spawnSync("cargo", args, {
    cwd: repoPath("."),
    // The corpus is ASCII by construction; encoding it explicitly keeps the pipe
    // free of any platform default, and leaving `encoding` unset means stdout comes
    // back as bytes that this function decodes exactly once, below.
    input: Buffer.from(corpusText, "utf8"),
    maxBuffer: 1024 * 1024 * 512,
    shell: process.platform === "win32"
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
    sections: null,
    corpus: null,
    release: false,
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
    else if (flag === "--sections") options.sections = value().split(",").map((s) => s.trim());
    else if (flag === "--corpus") options.corpus = value();
    else if (flag === "--release") options.release = true;
    else if (flag === "--expect") options.expect = value();
    else throw new Error(`differential.mjs: unknown argument ${flag}`);
  }
  if (!["agreement", "unimplemented"].includes(options.expect)) {
    throw new Error("differential.mjs: --expect takes 'agreement' or 'unimplemented'");
  }
  if (options.sections === null) {
    options.sections = options.random > 0 ? [...SECTIONS] : ["calls", "decimal", "regressions"];
  }
  return options;
}

function main() {
  const options = parseArguments(process.argv);

  let corpusText;
  let cases;
  let header;
  if (options.corpus) {
    corpusText = readTextLf(options.corpus);
    const lines = corpusText.replace(/\n$/, "").split("\n").map((line) => JSON.parse(line));
    header = lines[0];
    cases = lines.slice(1);
  } else {
    const corpus = buildCorpus(options);
    header = corpus.header;
    cases = corpus.cases;
    corpusText = renderCorpus(corpus);
  }
  console.error(`differential: ${cases.length} case(s), seed ${header.seed}, sections ${header.sections.join(",")}`);

  const rust = runDumpCases(corpusText, {release: options.release});
  if (rust.stderr.trim().length > 0) console.error(rust.stderr.trimEnd());
  if (rust.status !== 0 && rust.status !== 3) {
    console.error(`differential: dump_cases exited ${rust.status}`);
    process.exit(1);
  }
  const outputs = parseOutputJsonl(rust.stdout);

  const findings = [];
  const tally = {ok: 0, unimplemented: 0};
  for (const record of cases) {
    const nodeResult = evaluateCase(record);
    const finding = compareCase(record, nodeResult, outputs.get(record.id));
    if (finding === null) {
      tally.ok += 1;
      continue;
    }
    if (finding.kind === "unimplemented") {
      tally.unimplemented += 1;
      continue;
    }
    findings.push({record, nodeResult, rustRecord: outputs.get(record.id), finding});
  }

  // Phase 1's meaningful assertion: the protocol round-trips and the Rust side is
  // uniformly, explicitly not implemented. This cannot be mistaken for conformance
  // because it fails the moment any case reports a value.
  if (options.expect === "unimplemented") {
    const problems = [];
    if (tally.unimplemented !== cases.length) {
      problems.push(
        `expected all ${cases.length} case(s) to report "unimplemented", got ${tally.unimplemented}` +
          ` (${tally.ok} answered, ${findings.length} other finding(s))`
      );
    }
    if (rust.status !== 3) {
      problems.push(`expected dump_cases to exit 3 while unimplemented, got ${rust.status}`);
    }
    for (const {record, finding} of findings.slice(0, 5)) {
      problems.push(`${record.id}: ${finding.kind} -- ${finding.detail}`);
    }
    if (problems.length > 0) {
      console.error("differential --expect unimplemented: the Phase 1 wiring check failed");
      for (const problem of problems) console.error(`  - ${problem}`);
      process.exit(1);
    }
    console.error(
      `differential: protocol round-trips over ${cases.length} case(s);` +
        " every case reports \"unimplemented\", as Phase 1 requires"
    );
    return;
  }

  if (tally.unimplemented > 0) {
    console.error(
      `differential: ${tally.unimplemented} of ${cases.length} case(s) reported "unimplemented".` +
        " The Rust formatting API does not exist yet (MIGRATION_TO_RUST.md, Phase 2 through Phase 4)." +
        " Run with --expect unimplemented to check the protocol wiring instead."
    );
    process.exit(1);
  }

  if (findings.length === 0) {
    console.error(`differential: ${tally.ok} case(s) agree exactly`);
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
            cases: cases.length,
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

  console.error(`\ndifferential: ${findings.length} disagreement(s) over ${cases.length} case(s)`);
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

if (isMain(import.meta.url)) {
  main();
}
