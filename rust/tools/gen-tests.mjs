#!/usr/bin/env node
// Generates the Rust conformance suite under rust/tests/generated/ from the
// versioned fixtures (MIGRATION_TO_RUST.md section 3.4).
//
//   node rust/tools/gen-tests.mjs           # same as --check
//   node rust/tools/gen-tests.mjs --check   # regenerate into a temp dir, byte-compare
//   node rust/tools/gen-tests.mjs --write   # atomically replace the generated suite
//
// The generator reads only committed fixture files -- calls.jsonl and
// assertion-map.json -- so regenerating never depends on re-running Node against
// the JavaScript oracle. Output is ASCII-only Rust with LF endings, one test per
// original test block, preserving the source title and carrying the static
// assertion id of every case into the failure message.
//
// Phase 1 has no Rust formatting API. Every generated test therefore carries
// `#[ignore]` with an explicit reason *and* runs through `support::run`, whose body
// is `unimplemented!()`. The suite compiles, which is what section 3.7 gates on,
// but it cannot report a pass: `cargo test` shows the blocks as ignored, and
// `cargo test -- --ignored` panics on every one of them.
import {existsSync} from "node:fs";
import {
  FIXTURE_DIR,
  GENERATOR_VERSION,
  OWNED_GENERATED_TESTS,
  checkArtifacts,
  isMain,
  parseMode,
  readTextLf,
  FN_CALL_WIDTH,
  renderArrayLiteral,
  renderOptionalArrayField,
  renderStructField,
  repoPath,
  rustStringLiteral,
  writeArtifacts
} from "./harvest.mjs";

export const OUTPUT_DIR = "rust/tests/generated";

const IGNORE_REASON =
  "Phase 1 placeholder: the Rust formatting API does not exist yet " +
  "(MIGRATION_TO_RUST.md, Phase 2 through Phase 4)";

const OPERATION_VARIANTS = {
  format: "Op::Format",
  formatPrefix: "Op::FormatPrefix",
  formatSpecifier: "Op::FormatSpecifier",
  precisionFixed: "Op::PrecisionFixed",
  precisionPrefix: "Op::PrecisionPrefix",
  precisionRound: "Op::PrecisionRound"
};

// ---------------------------------------------------------------------------
// Naming
// ---------------------------------------------------------------------------

/** Rust module name for one oracle test file, e.g. `test/format-type-%-test.js`. */
export function moduleName(testFilePath) {
  const base = testFilePath.replace(/^test\//, "").replace(/\.js$/, "");
  const name = base
    .replace(/%/g, "percent")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
  if (!/^[a-z][a-z0-9_]*$/.test(name)) throw new Error(`cannot derive a Rust module name from ${testFilePath}`);
  return name;
}

/** Rust test function name for one oracle test block. The index keeps it unique. */
export function testFnName(index, title) {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 72)
    .replace(/_+$/, "");
  return `t${String(index).padStart(3, "0")}${slug ? `_${slug}` : ""}`;
}

/** Renders arbitrary oracle text as an ASCII-only Rust line comment. */
function asciiComment(text) {
  return rustStringLiteral(text).slice(1, -1).replace(/\\"/g, '"');
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function renderValue(value) {
  switch (value.kind) {
    case "f64":
      return `Value::F64(0x${value.bits})`;
    case "text":
      return `Value::Text(${rustStringLiteral(value.text)})`;
    case "specifier":
      return `Value::Specifier(${rustStringLiteral(value.text)})`;
    case "boolean":
      return `Value::Bool(${value.text})`;
    case "undefined":
      return "Value::Undefined";
    case "null":
      return "Value::Null";
    default:
      throw new Error(`unknown fixture value kind ${JSON.stringify(value.kind)}`);
  }
}

function renderOption(value, render) {
  return value === null || value === undefined ? "None" : `Some(${render(value)})`;
}

function renderCase(record, localeIndex, indent) {
  const field = (name, value) => renderStructField({name, value, indent});
  return [
    ...field("seq", String(record.seq)),
    ...field("assertion_id", renderOption(record.assertion_id, rustStringLiteral)),
    ...field("assertion_ordinal", renderOption(record.assertion_ordinal, String)),
    ...field("op", OPERATION_VARIANTS[record.operation]),
    ...field("locale", record.locale_hash === null ? "None" : `Some(${localeIndex.get(record.locale_hash)})`),
    ...field("specifier_input", renderOption(record.specifier_input, (v) => rustStringLiteral(v.text))),
    ...field("specifier", renderOption(record.specifier, rustStringLiteral)),
    ...field("reference", renderOption(record.reference, renderValue)),
    ...renderArrayLiteral({
      prefix: `${" ".repeat(indent)}args: &[`,
      elements: record.args.map(renderValue),
      close: "],",
      indent
    }),
    ...field("expected", renderValue(record.output))
  ];
}

function renderBlock(block, localeIndex) {
  const lines = [];
  lines.push("    Block {");
  lines.push(...renderStructField({name: "file", value: rustStringLiteral(block.file), indent: 8}));
  lines.push(`        test_index: ${block.testIndex},`);
  lines.push(...renderStructField({name: "title", value: rustStringLiteral(block.title), indent: 8}));
  lines.push(
    ...renderArrayLiteral({
      prefix: "        assertion_ids: &[",
      elements: block.assertionIds.map(rustStringLiteral),
      close: "],",
      indent: 8
    })
  );
  lines.push(`        executions: ${block.executions},`);
  if (block.cases.length === 0) {
    lines.push("        cases: &[],");
  } else if (block.cases.length === 1) {
    // rustfmt hugs a single block-like element rather than indenting it inside the
    // brackets, so a one-case block gets `cases: &[Case {` and closes with `}],`.
    lines.push("        cases: &[Case {");
    lines.push(...renderCase(block.cases[0], localeIndex, 12));
    lines.push("        }],");
  } else {
    lines.push("        cases: &[");
    for (const record of block.cases) {
      lines.push("            Case {");
      lines.push(...renderCase(record, localeIndex, 16));
      lines.push("            },");
    }
    lines.push("        ],");
  }
  lines.push("    },");
  return lines.join("\n");
}

function renderLocaleDef(locale) {
  const d = locale.definition;
  const str = (value) => (value === null ? "None" : `Some(${rustStringLiteral(value)})`);
  const field = (name, value) => renderStructField({name, value, indent: 8});
  return [
    "    LocaleDef {",
    ...field("id", rustStringLiteral(locale.id)),
    ...field("definition_sha256", rustStringLiteral(locale.hash)),
    ...field("decimal", str(d.decimal)),
    ...field("thousands", str(d.thousands)),
    ...renderOptionalArrayField({name: "grouping", value: d.grouping, renderElement: String, slice: true}),
    ...renderOptionalArrayField({name: "currency", value: d.currency, renderElement: rustStringLiteral}),
    ...renderOptionalArrayField({name: "numerals", value: d.numerals, renderElement: rustStringLiteral}),
    ...field("percent", str(d.percent)),
    ...field("minus", str(d.minus)),
    ...field("nan", str(d.nan)),
    "    },"
  ].join("\n");
}

function generatedHeader(source) {
  return [
    `// @generated by rust/tools/gen-tests.mjs v${GENERATOR_VERSION} from ${source}.`,
    "// Do not edit by hand: run `node rust/tools/gen-tests.mjs --write` instead."
  ];
}

function renderSupport(locales, totals) {
  const lines = [];
  lines.push(...generatedHeader(`${FIXTURE_DIR}/calls.jsonl`));
  lines.push("");
  lines.push("//! Shared data model for the generated conformance suite.");
  lines.push("//!");
  lines.push("//! Every oracle value is carried exactly as the fixtures record it: numbers as raw");
  lines.push("//! big-endian `f64` bit patterns, so negative zero, NaN payloads and the infinities");
  lines.push("//! survive the round trip; text as ASCII-escaped string literals.");
  lines.push("");
  lines.push("#![allow(dead_code)]");
  lines.push("");
  lines.push("/// One oracle value.");
  lines.push("#[derive(Debug, Clone, Copy, PartialEq)]");
  lines.push("pub enum Value {");
  lines.push("    /// A number, as its raw big-endian `f64` bit pattern.");
  lines.push("    F64(u64),");
  lines.push("    /// A JavaScript string.");
  lines.push("    Text(&'static str),");
  lines.push("    /// A `FormatSpecifier`, spelled in its normalized form.");
  lines.push("    Specifier(&'static str),");
  lines.push("    /// A JavaScript boolean.");
  lines.push("    Bool(bool),");
  lines.push("    /// JavaScript `undefined`. Rust has no equivalent; see rust/DIVERGENCES.md.");
  lines.push("    Undefined,");
  lines.push("    /// JavaScript `null`. Rust has no equivalent; see rust/DIVERGENCES.md.");
  lines.push("    Null,");
  lines.push("}");
  lines.push("");
  lines.push("impl Value {");
  lines.push("    /// The `f64` behind an [`Value::F64`], or `None` for any other kind.");
  lines.push("    pub fn as_f64(self) -> Option<f64> {");
  lines.push("        match self {");
  lines.push("            Value::F64(bits) => Some(f64::from_bits(bits)),");
  lines.push("            _ => None,");
  lines.push("        }");
  lines.push("    }");
  lines.push("");
  lines.push("    /// The text behind a [`Value::Text`] or [`Value::Specifier`].");
  lines.push("    pub fn as_text(self) -> Option<&'static str> {");
  lines.push("        match self {");
  lines.push("            Value::Text(text) | Value::Specifier(text) => Some(text),");
  lines.push("            _ => None,");
  lines.push("        }");
  lines.push("    }");
  lines.push("}");
  lines.push("");
  lines.push("/// The public d3-format operation a case exercises.");
  lines.push("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
  lines.push("pub enum Op {");
  lines.push("    /// `locale.format(specifier)(value)`.");
  lines.push("    Format,");
  lines.push("    /// `locale.formatPrefix(specifier, reference)(value)`.");
  lines.push("    FormatPrefix,");
  lines.push("    /// `formatSpecifier(text)`.");
  lines.push("    FormatSpecifier,");
  lines.push("    /// `precisionFixed(step)`.");
  lines.push("    PrecisionFixed,");
  lines.push("    /// `precisionPrefix(step, value)`.");
  lines.push("    PrecisionPrefix,");
  lines.push("    /// `precisionRound(step, max)`.");
  lines.push("    PrecisionRound,");
  lines.push("}");
  lines.push("");
  lines.push("/// A locale definition observed at the JavaScript boundary.");
  lines.push("///");
  lines.push("/// `None` means the definition omitted the key, which d3 treats differently from an");
  lines.push("/// empty string: absent keys fall back to d3's defaults, and grouping is disabled");
  lines.push("/// unless both `grouping` and `thousands` are present.");
  lines.push("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
  lines.push("pub struct LocaleDef {");
  lines.push("    /// Locale file name when the definition matches one, otherwise `anon:<hash>`.");
  lines.push("    pub id: &'static str,");
  lines.push("    /// SHA-256 of the canonical definition, matching the fixture `locale_hash`.");
  lines.push("    pub definition_sha256: &'static str,");
  lines.push("    pub decimal: Option<&'static str>,");
  lines.push("    pub thousands: Option<&'static str>,");
  lines.push("    pub grouping: Option<&'static [u32]>,");
  lines.push("    pub currency: Option<[&'static str; 2]>,");
  lines.push("    pub numerals: Option<[&'static str; 10]>,");
  lines.push("    pub percent: Option<&'static str>,");
  lines.push("    pub minus: Option<&'static str>,");
  lines.push("    pub nan: Option<&'static str>,");
  lines.push("}");
  lines.push("");
  lines.push("/// One recorded public-boundary call.");
  lines.push("#[derive(Debug, Clone, Copy)]");
  lines.push("pub struct Case {");
  lines.push("    /// Position of this call in `calls.jsonl`.");
  lines.push("    pub seq: u32,");
  lines.push("    /// Static assertion site, `relative/path:line:column`.");
  lines.push("    ///");
  lines.push("    /// `None` for a call made in a block's setup code, which belongs to the block");
  lines.push("    /// rather than to any single assertion.");
  lines.push("    pub assertion_id: Option<&'static str>,");
  lines.push("    /// Zero-based execution ordinal of that site; loops reuse one id.");
  lines.push("    pub assertion_ordinal: Option<u32>,");
  lines.push("    pub op: Op,");
  lines.push("    /// Index into [`LOCALES`], or `None` for the locale-free helpers.");
  lines.push("    pub locale: Option<usize>,");
  lines.push("    /// The specifier text as the oracle passed it in, before normalization.");
  lines.push("    pub specifier_input: Option<&'static str>,");
  lines.push("    /// The normalized specifier d3 itself reports.");
  lines.push("    pub specifier: Option<&'static str>,");
  lines.push("    /// `formatPrefix`'s reference value.");
  lines.push("    pub reference: Option<Value>,");
  lines.push("    /// Call arguments, in order.");
  lines.push("    pub args: &'static [Value],");
  lines.push("    /// The exact value the JavaScript oracle returned.");
  lines.push("    pub expected: Value,");
  lines.push("}");
  lines.push("");
  lines.push("/// One original `test(...)` block from the oracle suite.");
  lines.push("#[derive(Debug, Clone, Copy)]");
  lines.push("pub struct Block {");
  lines.push("    /// Repository-relative path of the oracle test file.");
  lines.push("    pub file: &'static str,");
  lines.push("    /// Zero-based index of the block within its file.");
  lines.push("    pub test_index: u32,");
  lines.push("    /// The original test title, verbatim.");
  lines.push("    pub title: &'static str,");
  lines.push("    /// Every static assertion site in the block, in source order.");
  lines.push("    pub assertion_ids: &'static [&'static str],");
  lines.push("    /// Dynamic assertion executions the block performed.");
  lines.push("    pub executions: u32,");
  lines.push("    /// Recorded boundary calls. Sites with no call are routed to a named test by");
  lines.push("    /// `rust/fixtures/d3-format-3.1.2/assertion-map.json`.");
  lines.push("    pub cases: &'static [Case],");
  lines.push("}");
  lines.push("");
  lines.push("/// Every distinct locale definition the oracle suite exercised.");
  lines.push(`pub static LOCALES: [LocaleDef; ${locales.length}] = [`);
  for (const locale of locales) lines.push(renderLocaleDef(locale));
  lines.push("];");
  lines.push("");
  lines.push("/// Oracle inventory recorded in the provenance manifest.");
  lines.push(`pub const TOTAL_BLOCKS: usize = ${totals.blocks};`);
  lines.push(`pub const TOTAL_CASES: usize = ${totals.cases};`);
  lines.push(`pub const TOTAL_EXECUTIONS: u32 = ${totals.executions};`);
  lines.push(`pub const TOTAL_LOCALES: usize = ${locales.length};`);
  lines.push("");
  lines.push("/// Runs one generated conformance block against the Rust port.");
  lines.push("///");
  lines.push("/// There is no Rust formatting API before Phase 2, so this deliberately does not");
  lines.push("/// exist yet. Every generated test is also `#[ignore]`d, so the placeholder state");
  lines.push("/// cannot be mistaken for a pass: `cargo test` reports the blocks as ignored, and");
  lines.push("/// `cargo test -- --ignored` panics here on every one of them.");
  lines.push("///");
  lines.push("/// The panic names every static assertion id the block covers, so a failure");
  lines.push("/// points straight at the JavaScript source it came from.");
  lines.push("pub fn run(block: &Block) -> ! {");
  lines.push("    unimplemented!(");
  lines.push('        "generated conformance block {} #{} ({}) is not runnable: {} case(s), {} \\');
  lines.push("assertion execution(s), assertion id(s) {} -- no Rust implementation until Phase 2 of \\");
  lines.push('MIGRATION_TO_RUST.md",');
  lines.push("        block.file,");
  lines.push("        block.test_index,");
  lines.push("        block.title,");
  lines.push("        block.cases.len(),");
  lines.push("        block.executions,");
  lines.push("        block.assertion_ids.join(\", \"),");
  lines.push("    )");
  lines.push("}");
  lines.push("");
  return lines.join("\n");
}

function renderModule(file, blocks, localeIndex) {
  const lines = [];
  lines.push(...generatedHeader(`${FIXTURE_DIR}/calls.jsonl`));
  lines.push("");
  lines.push(`//! Generated conformance tests for \`${file}\`.`);
  lines.push("");
  lines.push("#[allow(unused_imports)]");
  lines.push("use crate::support::{Block, Case, Op, Value};");
  lines.push("");
  lines.push("/// Every block from this oracle file, in source order.");
  lines.push("pub static BLOCKS: &[Block] = &[");
  for (const block of blocks) lines.push(renderBlock(block, localeIndex));
  lines.push("];");
  for (const block of blocks) {
    lines.push("");
    lines.push(`// ${file} #${block.testIndex}: ${asciiComment(block.title)}`);
    lines.push(`// ${block.executions} assertion execution(s), ${block.cases.length} recorded call(s)`);
    lines.push("#[test]");
    lines.push(`#[ignore = ${rustStringLiteral(IGNORE_REASON)}]`);
    lines.push(`fn ${block.fnName}() {`);
    lines.push(`    crate::support::run(&BLOCKS[${block.indexInModule}]);`);
    lines.push("}");
  }
  lines.push("");
  return lines.join("\n");
}

function renderMain(modules) {
  const blockCount = modules.reduce((sum, module) => sum + module.blocks.length, 0);
  const lines = [];
  lines.push(...generatedHeader(`${FIXTURE_DIR}/calls.jsonl and ${FIXTURE_DIR}/assertion-map.json`));
  lines.push("");
  lines.push("//! The generated d3-format conformance suite.");
  lines.push("//!");
  lines.push("//! One Rust test per original JavaScript test block, carrying the recorded oracle");
  lines.push("//! calls and the static assertion id of each one. The conformance tests are");
  lines.push("//! `#[ignore]`d and their bodies are `unimplemented!()` until the Rust formatting");
  lines.push("//! API exists; see `support::run`.");
  lines.push("//!");
  lines.push("//! Exactly two tests here run. Neither exercises the port: one asserts that the");
  lines.push("//! `support::run` seam still diverges, so this target's `test result: ok.` cannot");
  lines.push("//! be read as conformance, and the other checks the emitted inventory against the");
  lines.push("//! provenance manifest, so it cannot be read as a stale generation either.");
  lines.push("");
  lines.push("mod support;");
  lines.push("");
  // Declared in module-name order, not oracle-file order: rustfmt sorts `mod`
  // items, and section 6.1 runs `cargo fmt --check` over this generated file.
  // `MODULES` below stays in oracle-file order, which is what the inventory check
  // and every report are keyed on.
  for (const name of modules.map((module) => module.name).sort()) lines.push(`mod ${name};`);
  lines.push("");
  lines.push("/// Every generated module, paired with the oracle file it came from.");
  lines.push(`static MODULES: [(&str, &[support::Block]); ${modules.length}] = [`);
  for (const module of modules) {
    // rustfmt breaks a tuple whose contents exceed fn_call_width, which several of
    // the longer oracle file names do even though the whole line fits in max_width.
    const parts = [rustStringLiteral(module.file), `${module.name}::BLOCKS`];
    const inline = parts.join(", ");
    if (inline.length <= FN_CALL_WIDTH) lines.push(`    (${inline}),`);
    else lines.push("    (", ...parts.map((part) => `        ${part},`), "    ),");
  }
  lines.push("];");
  lines.push("");
  // libtest always prints `test result: ok.` for a target whose only outcomes are
  // "passed" and "ignored", and a CI dashboard that summarizes by that line would
  // read this placeholder suite as a working port. The per-test `ignored` reasons
  // say otherwise, but they are the lines a skimmer drops. So the state is also
  // stated in a test *name*, which survives every level of summarization, and the
  // name is backed by an assertion that fails the moment it stops being true.
  lines.push(`/// Proves the ${blockCount} generated conformance blocks cannot pass yet.`);
  lines.push("///");
  lines.push("/// `support::run` is the single seam where the Rust formatting API will be");
  lines.push("/// called. Until it exists the seam diverges, and this test asserts exactly");
  lines.push("/// that on a real block, so the suite cannot quietly begin reporting coverage.");
  lines.push("/// Implementing the API makes this test fail, which is the intended prompt to");
  lines.push("/// delete it and un-`#[ignore]` the blocks in the same change.");
  lines.push("#[test]");
  lines.push(
    `fn the_rust_formatting_api_is_unimplemented_so_all_${blockCount}_conformance_blocks_are_ignored() {`
  );
  lines.push("    let seam = std::panic::catch_unwind(|| support::run(&MODULES[0].1[0]));");
  lines.push("    assert!(");
  lines.push("        seam.is_err(),");
  lines.push("        \"support::run returned a value: the port now exists, so this test and the \\");
  lines.push("#[ignore] attributes on the generated blocks are both stale\"");
  lines.push("    );");
  lines.push("}");
  lines.push("");
  lines.push("/// The generated suite reproduces the recorded oracle inventory exactly.");
  lines.push("///");
  lines.push("/// This checks the generator, not the port: it says the emitted Rust still");
  lines.push("/// carries every block, case and execution the provenance manifest recorded.");
  lines.push("#[test]");
  lines.push("fn generated_suite_matches_the_recorded_oracle_inventory() {");
  lines.push("    let blocks: usize = MODULES.iter().map(|(_, blocks)| blocks.len()).sum();");
  lines.push("    let cases: usize = MODULES");
  lines.push("        .iter()");
  lines.push("        .flat_map(|(_, blocks)| blocks.iter())");
  lines.push("        .map(|block| block.cases.len())");
  lines.push("        .sum();");
  lines.push("    let executions: u32 = MODULES");
  lines.push("        .iter()");
  lines.push("        .flat_map(|(_, blocks)| blocks.iter())");
  lines.push("        .map(|block| block.executions)");
  lines.push("        .sum();");
  lines.push("");
  lines.push("    assert_eq!(blocks, support::TOTAL_BLOCKS, \"generated test blocks\");");
  lines.push("    assert_eq!(cases, support::TOTAL_CASES, \"generated oracle calls\");");
  // Pre-wrapped: both argument lists exceed rustfmt's fn_call_width, and section
  // 6.1 runs `cargo fmt --check` over this generated file.
  lines.push("    assert_eq!(");
  lines.push("        executions,");
  lines.push("        support::TOTAL_EXECUTIONS,");
  lines.push("        \"dynamic assertion executions\"");
  lines.push("    );");
  lines.push("    assert_eq!(");
  lines.push("        support::LOCALES.len(),");
  lines.push("        support::TOTAL_LOCALES,");
  lines.push("        \"distinct locales\"");
  lines.push("    );");
  lines.push("");
  lines.push("    for (file, blocks) in &MODULES {");
  lines.push("        for (index, block) in blocks.iter().enumerate() {");
  lines.push("            assert_eq!(block.file, *file, \"block file\");");
  lines.push("            assert_eq!(block.test_index as usize, index, \"block index in {file}\");");
  lines.push("            for case in block.cases {");
  lines.push("                if let Some(assertion_id) = case.assertion_id {");
  lines.push("                    assert!(");
  lines.push("                        assertion_id.starts_with(file),");
  lines.push("                        \"case {} claims assertion {assertion_id} from another file\",");
  lines.push("                        case.seq");
  lines.push("                    );");
  lines.push("                }");
  lines.push("                if let Some(index) = case.locale {");
  lines.push("                    assert!(index < support::LOCALES.len(), \"locale index out of range\");");
  lines.push("                }");
  lines.push("            }");
  lines.push("        }");
  lines.push("    }");
  lines.push("");
  lines.push(`    eprintln!(`);
  lines.push("        \"d3-format: {blocks} generated conformance blocks covering {cases} oracle calls \\");
  lines.push("are compiled but ignored; the Rust formatting API arrives in Phase 2 of \\");
  lines.push("MIGRATION_TO_RUST.md\"");
  lines.push("    );");
  lines.push("}");
  lines.push("");
  return lines.join("\n");
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/**
 * Builds the generated conformance suite from fixture-shaped inputs.
 *
 * `sites` is the assertion inventory (the `sites` array of assertion-map.json, or
 * the same structure freshly built by the harvester) and `callRecords` is
 * calls.jsonl. Nothing else is consulted.
 */
export function buildGeneratedTests({callRecords, sites}) {
  const localeByHash = new Map();
  for (const record of callRecords) {
    if (record.locale_hash === null) continue;
    if (!localeByHash.has(record.locale_hash)) {
      localeByHash.set(record.locale_hash, {
        id: record.locale_id,
        hash: record.locale_hash,
        definition: record.locale_definition
      });
    }
  }
  const locales = [...localeByHash.values()].sort((a, b) =>
    a.id < b.id ? -1 : a.id > b.id ? 1 : a.hash < b.hash ? -1 : a.hash > b.hash ? 1 : 0
  );
  const localeIndex = new Map(locales.map((locale, index) => [locale.hash, index]));

  const blocksByFile = new Map();
  const blockByKey = new Map();
  for (const site of sites) {
    const key = `${site.file}\u0000${site.test_index}`;
    let block = blockByKey.get(key);
    if (!block) {
      block = {
        file: site.file,
        testIndex: site.test_index,
        title: site.test_title,
        assertionIds: [],
        executions: 0,
        cases: []
      };
      blockByKey.set(key, block);
      if (!blocksByFile.has(site.file)) blocksByFile.set(site.file, []);
      blocksByFile.get(site.file).push(block);
    }
    if (block.title !== site.test_title) {
      throw new Error(`${key.replace("\u0000", " #")}: conflicting test titles in the assertion map`);
    }
    block.assertionIds.push({id: site.id, order: site.index_in_test});
    block.executions += site.executions;
  }
  for (const block of blockByKey.values()) {
    block.assertionIds.sort((a, b) => a.order - b.order);
    block.assertionIds = block.assertionIds.map((entry) => entry.id);
  }
  for (const record of callRecords) {
    const block = blockByKey.get(`${record.file}\u0000${record.test_index}`);
    if (!block) throw new Error(`call ${record.seq} references unknown block ${record.file} #${record.test_index}`);
    block.cases.push(record);
  }

  const files = new Map();
  const testNames = new Map();
  const modules = [];
  const seenModules = new Set();

  for (const file of [...blocksByFile.keys()].sort()) {
    const name = moduleName(file);
    if (seenModules.has(name)) throw new Error(`two oracle files map to the Rust module ${name}`);
    seenModules.add(name);
    const blocks = blocksByFile.get(file).sort((a, b) => a.testIndex - b.testIndex);
    const seenFns = new Set();
    blocks.forEach((block, indexInModule) => {
      block.indexInModule = indexInModule;
      block.fnName = testFnName(block.testIndex, block.title);
      if (seenFns.has(block.fnName)) throw new Error(`duplicate generated test name ${name}::${block.fnName}`);
      seenFns.add(block.fnName);
      block.cases.sort((a, b) => a.seq - b.seq);
      testNames.set(`${file}\u0000${block.testIndex}`, `${OUTPUT_DIR}/${name}.rs::${block.fnName}`);
    });
    modules.push({name, file, blocks});
    files.set(`${OUTPUT_DIR}/${name}.rs`, renderModule(file, blocks, localeIndex));
  }

  const totals = {
    blocks: blockByKey.size,
    cases: callRecords.length,
    executions: [...blockByKey.values()].reduce((sum, block) => sum + block.executions, 0)
  };

  files.set(`${OUTPUT_DIR}/support.rs`, renderSupport(locales, totals));
  files.set(`${OUTPUT_DIR}/main.rs`, renderMain(modules));

  return {files, testNames, locales, totals};
}

/** Reads the committed fixtures gen-tests is allowed to consume. */
export function loadFixtures() {
  const callsPath = `${FIXTURE_DIR}/calls.jsonl`;
  const mapPath = `${FIXTURE_DIR}/assertion-map.json`;
  for (const path of [callsPath, mapPath]) {
    if (!existsSync(repoPath(path))) {
      throw new Error(`${path} is missing; run \`node rust/tools/harvest.mjs --write\` first`);
    }
  }
  const callRecords = readTextLf(callsPath)
    .split("\n")
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line));
  const {sites} = JSON.parse(readTextLf(mapPath));
  return {callRecords, sites};
}

async function main() {
  const mode = parseMode(process.argv, "gen-tests.mjs");
  const {files, totals} = buildGeneratedTests(loadFixtures());

  if (mode === "write") {
    writeArtifacts(files, [OWNED_GENERATED_TESTS]);
    console.log(
      `gen-tests: wrote ${files.size} file(s) under ${OUTPUT_DIR}/; ` +
        "manifest.json is now stale, rerun `node rust/tools/harvest.mjs --write`"
    );
    return;
  }

  checkArtifacts(files, "gen-tests --check", [OWNED_GENERATED_TESTS]);
  console.log(
    `gen-tests: ${files.size} generated file(s) byte-identical; ` +
      `${totals.blocks} blocks, ${totals.cases} cases, ${totals.executions} assertion executions`
  );
}

if (isMain(import.meta.url)) {
  main().catch((error) => {
    console.error(`gen-tests: ${error.message}`);
    process.exitCode = 1;
  });
}
