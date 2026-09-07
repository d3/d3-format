#!/usr/bin/env node
// Oracle identity gate (MIGRATION_TO_RUST.md section 3.1).
//
// Every fixture under rust/fixtures/ is a recording of one exact JavaScript engine
// running one exact copy of the d3-format sources. This tool proves that the engine
// running right now, and the sources sitting on disk right now, are still that
// recording's inputs. It reads nothing but the running process, oracle.json, the
// provenance manifest, and the oracle sources; it writes nothing at all.
//
// Usage: node rust/tools/check-oracle-metadata.mjs

import {existsSync, readFileSync} from "node:fs";

import {
  D3_FORMAT_VERSION,
  FIXTURE_DIR,
  isMain,
  localeFiles,
  readTextLf,
  repoPath,
  sha256Hex,
  sourceFiles,
  sourceHash,
  testFiles
} from "./harvest.mjs";

const ORACLE_PATH = `${FIXTURE_DIR}/oracle.json`;
const MANIFEST_PATH = `${FIXTURE_DIR}/manifest.json`;

// Each entry pairs a field of oracle.json with the way the running process reports
// the same thing. Keeping them in one table means a new field is a one-line change
// and cannot be added to the fixture without also being checked here.
const ENGINE_FIELDS = [
  {field: "node", live: () => process.version, label: "Node"},
  {field: "v8", live: () => process.versions.v8, label: "V8"},
  {field: "icu", live: () => process.versions.icu, label: "ICU"},
  {field: "unicode", live: () => process.versions.unicode, label: "Unicode"},
  {field: "cldr", live: () => process.versions.cldr, label: "CLDR"}
];

export function readJson(relativePath) {
  return JSON.parse(readTextLf(relativePath));
}

/**
 * Compare oracle.json's engine fields to the process executing this function.
 *
 * A missing live value is a failure rather than a skip: a Node build without ICU
 * reports `undefined` for icu/unicode/cldr and would silently format every locale
 * differently from the recording.
 */
export function checkEngine(oracle) {
  const problems = [];
  for (const {field, live, label} of ENGINE_FIELDS) {
    const expected = oracle[field];
    const actual = live();
    if (typeof expected !== "string" || expected.length === 0) {
      problems.push(`${ORACLE_PATH}: field "${field}" is missing or not a string`);
    } else if (typeof actual !== "string" || actual.length === 0) {
      problems.push(
        `${label}: this Node build does not report a version, but the oracle requires ${expected}` +
          " (a small-icu or no-icu build cannot reproduce the fixtures)"
      );
    } else if (actual !== expected) {
      problems.push(`${label}: oracle recorded ${expected}, this process reports ${actual}`);
    }
  }
  return problems;
}

/**
 * Compare the recorded d3-format version and upstream commit to the checkout.
 *
 * `repository_input_commit` is deliberately not compared to HEAD. Section 3.1 calls
 * it provenance: later migration-only commits are expected and do not invalidate a
 * recording whose actual inputs -- the sources hashed below -- have not moved.
 */
export function checkIdentity(oracle) {
  const problems = [];
  const pkg = JSON.parse(readTextLf("package.json"));

  if (oracle.d3_format_version !== D3_FORMAT_VERSION) {
    problems.push(
      `${ORACLE_PATH}: d3_format_version is ${oracle.d3_format_version}, tools target ${D3_FORMAT_VERSION}`
    );
  }
  if (pkg.version !== oracle.d3_format_version) {
    problems.push(
      `package.json: version is ${pkg.version}, oracle recorded ${oracle.d3_format_version}`
    );
  }
  if (!/^[0-9a-f]{40}$/.test(oracle.d3_source_commit ?? "")) {
    problems.push(`${ORACLE_PATH}: d3_source_commit is not a full 40-character sha1`);
  }
  if (!/^[0-9a-f]{40}$/.test(oracle.repository_input_commit ?? "")) {
    problems.push(`${ORACLE_PATH}: repository_input_commit is not a full 40-character sha1`);
  }
  return problems;
}

/**
 * Re-hash every oracle source the manifest claims to have read.
 *
 * This is the check that actually protects the fixtures: an edit to src/, test/ or
 * locale/ changes formatting behavior or the assertion line numbers baked into the
 * generated tests, and both are invisible to a version string.
 */
export function checkSourceHashes(manifest) {
  const problems = [];
  const recorded = manifest.sources ?? {};
  // The same three directories harvest.mjs hashes: the implementation it records,
  // the tests whose line numbers become assertion ids, and the locale data.
  const present = [...sourceFiles(), ...testFiles(), ...localeFiles()];

  const recordedNames = Object.keys(recorded).sort();
  const presentNames = [...present].sort();

  for (const name of presentNames) {
    if (!Object.hasOwn(recorded, name)) {
      problems.push(`${name}: present in the checkout but absent from the manifest`);
    }
  }
  for (const name of recordedNames) {
    if (!present.includes(name)) {
      problems.push(`${name}: recorded in the manifest but missing from the checkout`);
      continue;
    }
    const actual = sourceHash(name);
    if (actual !== recorded[name]) {
      problems.push(`${name}: sha256 ${actual} does not match recorded ${recorded[name]}`);
    }
  }
  return problems;
}

/**
 * Confirm the fixture set the manifest describes is the fixture set on disk.
 *
 * The manifest never hashes itself, so this leaves the manifest out; harvest --check
 * is what proves the manifest's own bytes are reproducible.
 */
export function checkFixtureHashes(manifest) {
  const problems = [];
  for (const [name, expected] of Object.entries(manifest.fixtures ?? {})) {
    const relativePath = `${FIXTURE_DIR}/${name}`;
    if (!existsSync(repoPath(relativePath))) {
      problems.push(`${relativePath}: recorded in the manifest but missing`);
      continue;
    }
    // Fixtures are LF by construction and by .gitattributes, so they are hashed as
    // raw bytes: a stray CRLF here is drift worth reporting, not noise to normalize.
    const actual = sha256Hex(readFileSync(repoPath(relativePath)));
    if (actual !== expected) {
      problems.push(`${relativePath}: sha256 ${actual} does not match recorded ${expected}`);
    }
  }
  return problems;
}

/** oracle.json and the manifest must agree, or the manifest is describing a different recording. */
export function checkManifestAgreement(oracle, manifest) {
  const problems = [];
  for (const [key, value] of Object.entries(oracle)) {
    if (manifest.oracle?.[key] !== value) {
      problems.push(
        `${MANIFEST_PATH}: oracle.${key} is ${JSON.stringify(manifest.oracle?.[key])},` +
          ` ${ORACLE_PATH} says ${JSON.stringify(value)}`
      );
    }
  }
  return problems;
}

export function checkOracleMetadata() {
  const oracle = readJson(ORACLE_PATH);
  const manifest = readJson(MANIFEST_PATH);
  return [
    ...checkEngine(oracle),
    ...checkIdentity(oracle),
    ...checkManifestAgreement(oracle, manifest),
    ...checkSourceHashes(manifest),
    ...checkFixtureHashes(manifest)
  ];
}

function main() {
  const extra = process.argv.slice(2);
  if (extra.length > 0) {
    console.error(`check-oracle-metadata.mjs: unexpected argument ${extra[0]}; this tool takes none`);
    process.exit(2);
  }

  const problems = checkOracleMetadata();
  if (problems.length > 0) {
    console.error("check-oracle-metadata: the oracle recording no longer matches this environment");
    for (const problem of problems) console.error(`  - ${problem}`);
    console.error(
      "\nRegenerating fixtures is a reviewed compatibility event (MIGRATION_TO_RUST.md section 3.1):" +
        " do it in a dedicated commit, run the differential suite, and explain every output change."
    );
    process.exit(1);
  }

  const oracle = readJson(ORACLE_PATH);
  const manifest = readJson(MANIFEST_PATH);
  console.log(
    `check-oracle-metadata: node ${oracle.node}, v8 ${oracle.v8}, icu ${oracle.icu},` +
      ` unicode ${oracle.unicode}, cldr ${oracle.cldr}`
  );
  console.log(
    `check-oracle-metadata: d3-format ${oracle.d3_format_version} at ${oracle.d3_source_commit}` +
      `, ${Object.keys(manifest.sources).length} source(s) and` +
      ` ${Object.keys(manifest.fixtures).length} fixture(s) hash as recorded`
  );
}

if (isMain(import.meta.url)) {
  main();
}
