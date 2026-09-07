#!/usr/bin/env node
// Coverage gate (MIGRATION_TO_RUST.md section 3.3).
//
// harvest.mjs decides what the coverage mapping *is*. This tool is the adversary:
// it re-parses the oracle test files with Acorn itself, re-reads the committed
// fixtures, re-reads the Rust tree and DIVERGENCES.md, and refuses to accept the
// mapping unless all four independently agree. It never writes anything, so a
// disagreement can only be resolved by fixing the mapping or the tests.
//
// The seven conditions of section 3.3, in order:
//   1. exactly 729 executable static assertion ids for the current baseline;
//   2. every id mapped to at least one generated case or a named Rust test;
//   3. every one of the 1,078 dynamic executions represented;
//   4. every one of the 793 recorded boundary calls represented;
//   5. no unknown, duplicate, stale or orphaned mapping;
//   6. an explicit reason and Rust test path for every hand-ported or
//      intentionally reinterpreted site;
//   7. mutable-global identity and JavaScript coercion assertions mapped to
//      explicit DIVERGENCES.md entries and named API tests, never to a claim that
//      some different Rust API passes them.
//
// Usage: node rust/tools/check-coverage.mjs

import {existsSync, readdirSync, statSync} from "node:fs";

import {
  BASELINE,
  FIXTURE_DIR,
  analyzeTestFiles,
  isMain,
  readTextLf,
  repoPath,
  testFiles,
  textualAssertionOccurrences,
  toPosix
} from "./harvest.mjs";

const MAP_PATH = `${FIXTURE_DIR}/assertion-map.json`;
const CALLS_PATH = `${FIXTURE_DIR}/calls.jsonl`;
const MANIFEST_PATH = `${FIXTURE_DIR}/manifest.json`;
const DIVERGENCES_PATH = "rust/DIVERGENCES.md";

const COVERAGE_CLASSES = new Set(["generated", "ported", "divergence"]);

/** A `path::test_name` reference into the Rust tree, as written in `rust_tests`. */
const RUST_TEST_REFERENCE = /^(rust\/tests\/[A-Za-z0-9_/-]+\.rs)::([a-z0-9_]+)$/;

class Problems {
  constructor() {
    this.byCondition = new Map();
  }

  add(condition, message) {
    if (!this.byCondition.has(condition)) this.byCondition.set(condition, []);
    this.byCondition.get(condition).push(message);
  }

  get size() {
    let total = 0;
    for (const list of this.byCondition.values()) total += list.length;
    return total;
  }
}

export function readJsonl(relativePath) {
  const text = readTextLf(relativePath);
  if (text.length === 0) return [];
  return text.replace(/\n$/, "").split("\n").map((line) => JSON.parse(line));
}

/**
 * Collect the test function names each Rust file under `rust/tests/` defines.
 *
 * Only `#[test]`-attributed free functions count. A mapping that names a function
 * which is merely a helper, or which was renamed or deleted, is stale, and stale
 * mappings are exactly what condition 5 forbids.
 */
export function rustTestIndex() {
  const index = new Map();
  const files = new Set();
  const stack = ["rust/tests/generated", "rust/tests/ported"].filter((root) =>
    existsSync(repoPath(root))
  );
  while (stack.length > 0) {
    const dir = stack.pop();
    for (const entry of readdirSync(repoPath(dir)).sort()) {
      const relative = `${dir}/${entry}`;
      if (statSync(repoPath(relative)).isDirectory()) {
        stack.push(relative);
      } else if (relative.endsWith(".rs")) {
        files.add(relative);
      }
    }
  }

  for (const relative of [...files].sort()) {
    const source = readTextLf(relative);
    const names = new Set();
    // `#[test]` may be followed by further attributes (`#[ignore = "..."]`) before
    // the signature, so scan forward from each attribute to the next `fn`.
    const attribute = /#\[test\]/g;
    let match;
    while ((match = attribute.exec(source)) !== null) {
      const rest = source.slice(match.index, match.index + 4096);
      const signature = /\bfn\s+([a-z0-9_]+)\s*\(/.exec(rest);
      if (signature) names.add(signature[1]);
    }
    index.set(relative, names);
  }
  return index;
}

/**
 * Parse the `ID:` and `Oracle assertions:` lines out of DIVERGENCES.md.
 *
 * One heading may declare several ids when a single deviation explains several
 * distinct JavaScript behaviors, so ids accumulate onto the most recent heading.
 */
export function readDivergenceDocument() {
  if (!existsSync(repoPath(DIVERGENCES_PATH))) {
    return {entries: new Map(), missing: true};
  }
  const entries = new Map();
  let heading = null;
  let claimed = null;
  for (const line of readTextLf(DIVERGENCES_PATH).split("\n")) {
    const headingMatch = /^##\s+(.+?)\s*$/.exec(line);
    if (headingMatch) {
      heading = headingMatch[1];
      claimed = null;
      continue;
    }
    const idMatch = /^ID:\s*`([a-z0-9-]+)`\s*$/.exec(line);
    if (idMatch && heading) {
      entries.set(idMatch[1], {heading, ids: [], assertions: null, tests: []});
      continue;
    }
    const countMatch = /^Oracle assertions:\s*(none|\d+)\b/.exec(line);
    if (countMatch && heading) {
      claimed = countMatch[1] === "none" ? 0 : Number(countMatch[1]);
      for (const entry of entries.values()) {
        if (entry.heading === heading) entry.assertions = claimed;
      }
    }
  }
  return {entries, missing: false};
}

// ---------------------------------------------------------------------------
// Conditions
// ---------------------------------------------------------------------------

/** Condition 1: Acorn, run here and now, must find exactly the baseline static sites. */
function checkStaticSites(problems, analysis, map) {
  const parsed = [...analysis.sitesById.keys()];
  const mapped = map.sites.map((site) => site.id);

  if (parsed.length !== BASELINE.static_assertion_sites) {
    problems.add(
      1,
      `Acorn found ${parsed.length} executable assertion site(s) in ${testFiles().length} test file(s),` +
        ` baseline is ${BASELINE.static_assertion_sites}. Investigate the drift; do not adjust the baseline.`
    );
  }

  const parsedSet = new Set(parsed);
  const mappedSet = new Set(mapped);
  if (mapped.length !== mappedSet.size) {
    const seen = new Set();
    for (const id of mapped) {
      if (seen.has(id)) problems.add(5, `${MAP_PATH}: duplicate mapping for ${id}`);
      seen.add(id);
    }
  }
  for (const id of parsed) {
    if (!mappedSet.has(id)) problems.add(2, `${id}: parsed from source but absent from ${MAP_PATH}`);
  }
  for (const id of mapped) {
    if (!parsedSet.has(id)) {
      problems.add(5, `${MAP_PATH}: ${id} is mapped but no longer exists in the source (stale)`);
    }
  }

  // Comments are ignored naturally by parsing rather than by pattern matching, but
  // the difference between the two is the whole point of section 0.3's 735-vs-729
  // note, so it is asserted rather than assumed.
  const textual = textualAssertionOccurrences();
  if (textual - parsed.length !== BASELINE.commented_assertion_occurrences) {
    problems.add(
      1,
      `${textual} textual assert.* occurrence(s) minus ${parsed.length} executable site(s) is` +
        ` ${textual - parsed.length}, baseline expects ${BASELINE.commented_assertion_occurrences}` +
        " commented-out occurrence(s)"
    );
  }
}

/** Conditions 2 and 6: every site is covered, and non-generated coverage is justified. */
function checkSiteCoverage(problems, map, rustTests, divergences) {
  for (const site of map.sites) {
    if (!COVERAGE_CLASSES.has(site.coverage)) {
      problems.add(5, `${site.id}: unknown coverage class ${JSON.stringify(site.coverage)}`);
      continue;
    }

    const hasCase = site.calls.length > 0;
    const named = site.rust_tests ?? [];

    if (site.coverage === "generated") {
      if (!hasCase) {
        problems.add(2, `${site.id}: classified generated but maps to no recorded call`);
      }
      if (site.reason !== null) {
        problems.add(6, `${site.id}: generated sites carry no reason, found ${JSON.stringify(site.reason)}`);
      }
      if (site.divergence !== null) {
        problems.add(6, `${site.id}: generated sites carry no divergence id`);
      }
    } else {
      // Condition 6. A hand-ported or reinterpreted site is only covered if a human
      // wrote down why and pointed at the test that takes its place.
      if (typeof site.reason !== "string" || site.reason.trim().length < 20) {
        problems.add(6, `${site.id}: ${site.coverage} site needs an explicit reason`);
      }
      if (named.length === 0) {
        problems.add(6, `${site.id}: ${site.coverage} site names no Rust test`);
      }
    }

    if (named.length === 0 && !hasCase) {
      problems.add(2, `${site.id}: mapped to neither a generated case nor a named Rust test`);
    }

    for (const reference of named) {
      const parts = RUST_TEST_REFERENCE.exec(reference);
      if (!parts) {
        problems.add(5, `${site.id}: malformed Rust test reference ${JSON.stringify(reference)}`);
        continue;
      }
      const [, file, name] = parts;
      if (!rustTests.has(file)) {
        problems.add(5, `${site.id}: names ${file}, which does not exist (stale mapping)`);
      } else if (!rustTests.get(file).has(name)) {
        problems.add(5, `${site.id}: names ${file}::${name}, which is not a #[test] there (stale mapping)`);
      }
    }

    // Condition 7.
    if (site.coverage === "divergence") {
      if (typeof site.divergence !== "string" || site.divergence.length === 0) {
        problems.add(7, `${site.id}: divergence site names no DIVERGENCES.md id`);
      } else if (divergences.missing) {
        problems.add(7, `${DIVERGENCES_PATH} does not exist, but ${site.id} points into it`);
      } else if (!divergences.entries.has(site.divergence)) {
        problems.add(
          7,
          `${site.id}: names divergence id "${site.divergence}", which has no entry in ${DIVERGENCES_PATH}`
        );
      }
      if (named.length === 0) {
        problems.add(
          7,
          `${site.id}: a divergence must be answered by a named Rust test, not by a different API passing`
        );
      }
    }
  }
}

/** Condition 3: dynamic executions are conserved across the mapping. */
function checkDynamicExecutions(problems, analysis, map) {
  let total = 0;
  for (const site of map.sites) {
    if (!Number.isInteger(site.executions) || site.executions < 1) {
      problems.add(3, `${site.id}: executions is ${site.executions}, expected a positive integer`);
      continue;
    }
    total += site.executions;
  }
  if (total !== BASELINE.dynamic_assertion_executions) {
    problems.add(
      3,
      `mapped dynamic executions total ${total}, baseline is ${BASELINE.dynamic_assertion_executions}`
    );
  }
  if (map.counts.dynamic_assertion_executions !== total) {
    problems.add(3, `${MAP_PATH}: counts.dynamic_assertion_executions disagrees with the site totals`);
  }
  if (analysis.sitesById.size !== map.sites.length) {
    problems.add(3, "the parsed site list and the mapped site list are different lengths");
  }
}

/** Condition 4: every recorded boundary call is claimed exactly once, or explained. */
function checkBoundaryCalls(problems, map, calls) {
  if (calls.length !== BASELINE.boundary_calls) {
    problems.add(4, `${CALLS_PATH} holds ${calls.length} call(s), baseline is ${BASELINE.boundary_calls}`);
  }

  const claimedBy = new Map();
  for (const site of map.sites) {
    for (const seq of site.calls) {
      if (claimedBy.has(seq)) {
        problems.add(5, `call ${seq} is claimed by both ${claimedBy.get(seq)} and ${site.id}`);
      }
      claimedBy.set(seq, site.id);
    }
  }

  const bySeq = new Map(calls.map((call) => [call.seq, call]));
  for (const [seq, id] of claimedBy) {
    if (!bySeq.has(seq)) {
      problems.add(5, `${id}: claims call ${seq}, which is not in ${CALLS_PATH} (orphaned mapping)`);
    }
  }

  for (const call of calls) {
    if (claimedBy.has(call.seq)) {
      const site = claimedBy.get(call.seq);
      if (call.assertion_id !== null && call.assertion_id !== site) {
        problems.add(5, `call ${call.seq} records assertion ${call.assertion_id} but is claimed by ${site}`);
      }
      continue;
    }
    // A call with no static site is a library call made in a test block's setup
    // position. It is represented by the block's generated test, so it is covered;
    // a call that belongs to neither a site nor a block is not.
    if (call.assertion_id !== null) {
      problems.add(4, `call ${call.seq} records assertion ${call.assertion_id} but no site claims it`);
    } else if (call.test_index === null || call.file === null) {
      problems.add(4, `call ${call.seq} is attributed to no assertion and no test block`);
    }
  }

  const claimed = claimedBy.size;
  const setup = calls.length - claimed;
  if (map.counts.boundary_calls !== calls.length) {
    problems.add(4, `${MAP_PATH}: counts.boundary_calls is ${map.counts.boundary_calls}, saw ${calls.length}`);
  }
  return {claimed, setup};
}

/** Condition 5, the other direction: DIVERGENCES.md must not accumulate dead entries. */
function checkDivergenceDocument(problems, map, divergences, rustTests) {
  if (divergences.missing) {
    problems.add(7, `${DIVERGENCES_PATH} is required by section 8 and does not exist`);
    return;
  }

  const used = new Map();
  for (const site of map.sites) {
    if (site.coverage !== "divergence" || !site.divergence) continue;
    used.set(site.divergence, (used.get(site.divergence) ?? 0) + 1);
  }

  const byHeading = new Map();
  for (const [id, entry] of divergences.entries) {
    if (!byHeading.has(entry.heading)) byHeading.set(entry.heading, {ids: [], assertions: entry.assertions});
    byHeading.get(entry.heading).ids.push(id);
  }

  for (const [heading, entry] of byHeading) {
    const actual = entry.ids.reduce((sum, id) => sum + (used.get(id) ?? 0), 0);
    if (entry.assertions === null) {
      problems.add(
        7,
        `${DIVERGENCES_PATH}: entry "${heading}" has no "Oracle assertions:" line` +
          ' (write a count, or "none")'
      );
    } else if (entry.assertions !== actual) {
      problems.add(
        7,
        `${DIVERGENCES_PATH}: entry "${heading}" claims ${entry.assertions} oracle assertion(s),` +
          ` the mapping attributes ${actual}`
      );
    }
  }

  // Section 8's required list. An entry may exist with no assertions behind it --
  // several deviations are structural -- but it may not simply be absent.
  const REQUIRED = [
    "no-mutable-default-locale",
    "no-global-si-prefix-leak",
    "immutable-typed-specifier",
    "no-specifier-field-coercion",
    "construction-time-width-limits",
    "no-input-coercion",
    "no-non-bmp-or-newline-fill",
    "validated-locale-shape"
  ];
  for (const id of REQUIRED) {
    if (!divergences.entries.has(id)) {
      problems.add(7, `${DIVERGENCES_PATH}: section 8 requires an entry for "${id}"`);
    }
  }

  for (const [id, entry] of divergences.entries) {
    if (used.has(id) || REQUIRED.includes(id)) continue;
    problems.add(5, `${DIVERGENCES_PATH}: entry "${entry.heading}" (${id}) is claimed by no site (orphaned)`);
  }

  // Every Rust test named anywhere in the mapping must exist. Checked per-site above;
  // here the reverse: a ported/divergence test that no site points at is dead weight
  // in the scoreboard and hides the fact that nothing requires it.
  //
  // The target root, rust/tests/ported/main.rs, is exempt. Ported assertions live in
  // the topic modules it declares; main.rs holds target-level scaffolding, including
  // the test whose name tells a reader skimming `cargo test` output that nothing in
  // this target verifies the port yet. Requiring an oracle assertion to justify that
  // test would mean inventing a mapping for it, which is exactly the kind of fiction
  // this condition exists to prevent.
  const TARGET_SCAFFOLDING = "rust/tests/ported/main.rs";
  const named = new Set();
  for (const site of map.sites) for (const reference of site.rust_tests ?? []) named.add(reference);
  for (const [file, names] of rustTests) {
    if (!file.startsWith("rust/tests/ported/") || file === TARGET_SCAFFOLDING) continue;
    for (const name of [...names].sort()) {
      if (!named.has(`${file}::${name}`)) {
        problems.add(5, `${file}::${name} is a ported test that no assertion maps to (orphaned)`);
      }
    }
  }
  // The exemption is narrow, so it must not become a place to hide real assertions:
  // anything the mapping points at has to live in a topic module.
  for (const reference of named) {
    if (reference.startsWith(`${TARGET_SCAFFOLDING}::`)) {
      problems.add(6, `${reference}: ported assertions belong in a topic module, not the target root`);
    }
  }
}

/** The manifest is the published scoreboard; it must not disagree with the mapping. */
function checkManifestAgreement(problems, map, manifest) {
  const pairs = [
    ["static_assertion_sites", map.counts.static_assertion_sites],
    ["dynamic_assertion_executions", map.counts.dynamic_assertion_executions],
    ["boundary_calls", map.counts.boundary_calls]
  ];
  for (const [key, value] of pairs) {
    if (manifest.counts[key] !== value) {
      problems.add(5, `${MANIFEST_PATH}: counts.${key} is ${manifest.counts[key]}, ${MAP_PATH} says ${value}`);
    }
    if (BASELINE[key] !== value) {
      problems.add(1, `${key} is ${value}, section 0.3 baseline is ${BASELINE[key]}`);
    }
  }
}

export function checkCoverage() {
  const problems = new Problems();
  const analysis = analyzeTestFiles();
  const map = JSON.parse(readTextLf(MAP_PATH));
  const manifest = JSON.parse(readTextLf(MANIFEST_PATH));
  const calls = readJsonl(CALLS_PATH);
  const rustTests = rustTestIndex();
  const divergences = readDivergenceDocument();

  checkStaticSites(problems, analysis, map);
  checkSiteCoverage(problems, map, rustTests, divergences);
  checkDynamicExecutions(problems, analysis, map);
  const callTotals = checkBoundaryCalls(problems, map, calls);
  checkDivergenceDocument(problems, map, divergences, rustTests);
  checkManifestAgreement(problems, map, manifest);

  const byCoverage = {generated: 0, ported: 0, divergence: 0};
  for (const site of map.sites) {
    if (site.coverage in byCoverage) byCoverage[site.coverage] += 1;
  }
  return {problems, map, byCoverage, callTotals, divergences};
}

const CONDITIONS = {
  1: "exactly 729 executable static assertion ids",
  2: "every id mapped to a generated case or a named test",
  3: "every dynamic execution represented",
  4: "every recorded boundary call represented",
  5: "no unknown, duplicate, stale or orphaned mapping",
  6: "an explicit reason and Rust test path for every ported or reinterpreted site",
  7: "divergences mapped to DIVERGENCES.md entries and named API tests"
};

function main() {
  const extra = process.argv.slice(2);
  if (extra.length > 0) {
    console.error(`check-coverage.mjs: unexpected argument ${extra[0]}; this tool takes none`);
    process.exit(2);
  }

  const {problems, map, byCoverage, callTotals, divergences} = checkCoverage();

  if (problems.size > 0) {
    console.error(`check-coverage: ${problems.size} problem(s) across ${problems.byCondition.size} condition(s)`);
    for (const condition of [...problems.byCondition.keys()].sort()) {
      const list = problems.byCondition.get(condition);
      console.error(`\n  condition ${condition} -- ${CONDITIONS[condition]} (${list.length}):`);
      for (const message of list.slice(0, 25)) console.error(`    - ${message}`);
      if (list.length > 25) console.error(`    ... and ${list.length - 25} more`);
    }
    process.exit(1);
  }

  console.log(
    `check-coverage: ${map.sites.length} static site(s) -- ${byCoverage.generated} generated,` +
      ` ${byCoverage.ported} ported, ${byCoverage.divergence} divergence`
  );
  console.log(
    `check-coverage: ${map.counts.dynamic_assertion_executions} dynamic execution(s);` +
      ` ${callTotals.claimed} boundary call(s) claimed by a site,` +
      ` ${callTotals.setup} by a test block's setup position`
  );
  console.log(
    `check-coverage: ${divergences.entries.size} divergence id(s) in ${toPosix(DIVERGENCES_PATH)}, all reachable`
  );
}

if (isMain(import.meta.url)) {
  main();
}
