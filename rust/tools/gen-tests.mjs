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
// Every block runs through `support::run`, which replays the block's recorded calls
// against the Rust port and compares each answer with the exact value the oracle
// returned. A block is `#[ignore]`d only where the recording cannot drive the port
// at all: either the oracle recorded no boundary call for it, or every call it did
// record passes a JavaScript value the typed API declines. Both conditions are
// computed from the fixtures, both carry a reason naming what is missing, and
// `main.rs` asserts that the ignored set is exactly the set those two rules select,
// so a block cannot be quietly ignored to make a suite green.
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

/**
 * The two reasons a generated block cannot be executed.
 *
 * They are `#[ignore]` text, constants in the emitted `support.rs`, *and* the third
 * column of `IGNORED` in the emitted `main.rs`, all from this one spelling.
 */
const NO_RECORDED_CALL =
  "the oracle recorded no boundary call for this block, so running it would assert nothing; " +
  "assertion-map.json routes its assertions to rust/tests/ported";
const NOT_A_PORTABLE_INPUT =
  "every recorded call in this block passes a JavaScript value the typed API declines; " +
  "see no-input-coercion in rust/DIVERGENCES.md";

/**
 * Whether the port can be asked one recorded call at all.
 *
 * `undefined`, `null` and booleans reach `format` only through JavaScript coercion,
 * which `no-input-coercion` in `rust/DIVERGENCES.md` says the typed API refuses. A
 * block containing such a call is ignored rather than half-run, because a test that
 * silently skipped part of its block would report coverage it does not have.
 *
 * This is the rule the emitted `main.rs` re-applies in Rust, over the same block
 * data, to check that the `#[ignore]` inventory still matches it.
 */
function isExecutableCall(record) {
  return record.args.every((argument) => argument.kind === "f64" || argument.kind === "text");
}

/** The `#[ignore]` reason for a block, or `null` when the block can be executed. */
export function ignoreReason(block) {
  if (block.cases.length === 0) return NO_RECORDED_CALL;
  if (!block.cases.every(isExecutableCall)) return NOT_A_PORTABLE_INPUT;
  return null;
}

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
  lines.push("//! Shared data model and executor for the generated conformance suite.");
  lines.push("//!");
  lines.push("//! Every oracle value is carried exactly as the fixtures record it: numbers as raw");
  lines.push("//! big-endian `f64` bit patterns, so negative zero, NaN payloads and the infinities");
  lines.push("//! survive the round trip; text as ASCII-escaped string literals.");
  lines.push("//!");
  lines.push("//! [`run`] replays a block's recorded calls against the public API and compares");
  lines.push("//! each answer with the value the JavaScript oracle returned, exactly.");
  lines.push("");
  lines.push("#![allow(dead_code)]");
  lines.push("");
  lines.push("use d3_format::{");
  lines.push("    precision_fixed, precision_prefix, precision_round, FormatSpecifier, Locale, LocaleDefinition,");
  lines.push("};");
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
  lines.push(RUNTIME.trimEnd());
  lines.push("");
  return lines.join("\n");
}

/**
 * The executor the generated tests call, emitted verbatim into `support.rs`.
 *
 * Written as source rather than assembled line by line because it is ordinary
 * hand-maintained Rust that happens to live in a generated file: nothing in it
 * varies with the fixtures. It must already satisfy rustfmt, per section 3.4.
 */
const RUNTIME = `
// ---------------------------------------------------------------------------
// The executor
// ---------------------------------------------------------------------------

/// The port's answer for one recorded call, in the fixtures' value shape.
///
/// A number is carried as its bit pattern, like [\`Value::F64\`], so that negative
/// zero, the infinities and NaN payloads all compare exactly rather than through
/// \`f64\` equality, under which two NaNs are never equal and the two zeros always
/// are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// A formatted string.
    Text(String),
    /// A normalized specifier, which the recording tags apart from text.
    Specifier(String),
    /// A numeric answer, by bit pattern.
    F64(u64),
}

impl Answer {
    /// Exact agreement with a recorded oracle value, kind included.
    fn agrees_with(&self, expected: Value) -> bool {
        match (self, expected) {
            (Answer::Text(actual), Value::Text(expected)) => actual == expected,
            (Answer::Specifier(actual), Value::Specifier(expected)) => actual == expected,
            (Answer::F64(actual), Value::F64(expected)) => *actual == expected,
            _ => false,
        }
    }
}

impl Case {
    /// Whether the port can be asked this call at all.
    ///
    /// \`undefined\`, \`null\` and booleans reach d3's \`format\` only through JavaScript
    /// coercion, and \`no-input-coercion\` in \`rust/DIVERGENCES.md\` records that the
    /// typed API declines them instead. The block containing such a call is
    /// \`#[ignore]\`d, and \`main.rs\` asserts that the ignored set is exactly the set
    /// this rule selects.
    pub fn is_executable(&self) -> bool {
        self.args
            .iter()
            .all(|argument| matches!(argument, Value::F64(_) | Value::Text(_)))
    }
}

/// A block whose assertions the recording routed to \`rust/tests/ported\` instead.
pub const NO_RECORDED_CALL: &str = ${rustStringLiteral(NO_RECORDED_CALL)};
/// A block that only ever called d3 with a value the typed API declines.
pub const NOT_A_PORTABLE_INPUT: &str = ${rustStringLiteral(NOT_A_PORTABLE_INPUT)};

impl Block {
    /// Why this block cannot be replayed, or \`None\` when it can.
    ///
    /// The generator applies this same rule to the fixtures to decide which blocks
    /// get \`#[ignore]\`; \`main.rs\` reapplies it here, over the data as compiled in,
    /// and asserts the two agree. So the skip list is derived from the oracle
    /// rather than asserted about the port, and no block can sit ignored once its
    /// calls become replayable.
    pub fn ignore_reason(&self) -> Option<&'static str> {
        if self.cases.is_empty() {
            Some(NO_RECORDED_CALL)
        } else if !self.cases.iter().all(Case::is_executable) {
            Some(NOT_A_PORTABLE_INPUT)
        } else {
            None
        }
    }
}

/// Replays one recorded block against the port, comparing every call exactly.
///
/// The failure message names the static assertion site the call came from, so a
/// disagreement points straight at the JavaScript source that recorded it.
pub fn run(block: &Block) {
    assert!(
        !block.cases.is_empty(),
        "generated block {} #{} ({}) recorded no boundary call, so running it would assert \\
nothing",
        block.file,
        block.test_index,
        block.title
    );
    for case in block.cases {
        assert!(
            case.is_executable(),
            "generated block {} #{} case {} passes a JavaScript value the typed API declines; \\
see no-input-coercion in rust/DIVERGENCES.md",
            block.file,
            block.test_index,
            case.seq
        );
        let actual = answer(case);
        assert!(
            actual.agrees_with(case.expected),
            "{}\\n  block    : {} #{} ({})\\n  call     : {}\\n  expected : {:?}\\n  actual   : {:?}",
            case.assertion_id
                .unwrap_or("(block setup, no assertion site)"),
            block.file,
            block.test_index,
            block.title,
            describe(case),
            case.expected,
            actual
        );
    }
}

/// Asks the port one recorded call.
fn answer(case: &Case) -> Answer {
    match case.op {
        Op::Format | Op::FormatPrefix => {
            let specifier = case
                .specifier_input
                .unwrap_or_else(|| panic!("case {}: no recorded specifier", case.seq));
            let locale = locale_of(case);
            let formatter = match (case.op, case.reference.and_then(Value::as_f64)) {
                (Op::FormatPrefix, Some(reference)) => {
                    locale.prefix_formatter(specifier, reference)
                }
                (Op::FormatPrefix, None) => {
                    panic!("case {}: formatPrefix with no numeric reference", case.seq)
                }
                _ => locale.formatter(specifier),
            };
            let formatter = formatter.unwrap_or_else(|error| {
                panic!("case {}: {specifier:?} did not compile: {error}", case.seq)
            });
            let formatted = match case.args {
                [Value::F64(bits)] => formatter.format_number(f64::from_bits(*bits)),
                [Value::Text(text)] => formatter.format_text(text),
                args => panic!("case {}: unexpected arguments {args:?}", case.seq),
            };
            Answer::Text(formatted.unwrap_or_else(|error| {
                panic!(
                    "case {}: {specifier:?} refused its input: {error}",
                    case.seq
                )
            }))
        }
        Op::FormatSpecifier => {
            let specifier = case
                .specifier_input
                .unwrap_or_else(|| panic!("case {}: no recorded specifier", case.seq));
            let parsed = specifier
                .parse::<FormatSpecifier>()
                .unwrap_or_else(|error| {
                    panic!("case {}: {specifier:?} did not parse: {error}", case.seq)
                });
            Answer::Specifier(parsed.to_string())
        }
        Op::PrecisionFixed => Answer::F64(precision_fixed(number(case, 0)).to_bits()),
        Op::PrecisionPrefix => {
            Answer::F64(precision_prefix(number(case, 0), number(case, 1)).to_bits())
        }
        Op::PrecisionRound => {
            Answer::F64(precision_round(number(case, 0), number(case, 1)).to_bits())
        }
    }
}

/// The \`n\`th argument as an \`f64\`.
fn number(case: &Case, index: usize) -> f64 {
    case.args
        .get(index)
        .copied()
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("case {}: argument {index} is not a number", case.seq))
}

/// The locale a case names, rebuilt from its recorded definition.
///
/// A null field in the recording means the definition *omitted* the key, which d3
/// answers with its own default; \`LocaleDefinition\`'s \`None\` means the same, so the
/// two line up field for field.
fn locale_of(case: &Case) -> Locale {
    let index = case
        .locale
        .unwrap_or_else(|| panic!("case {}: no recorded locale", case.seq));
    let recorded = &LOCALES[index];
    let definition = LocaleDefinition {
        decimal: recorded.decimal.map(str::to_owned),
        thousands: recorded.thousands.map(str::to_owned),
        grouping: recorded.grouping.map(<[u32]>::to_vec),
        currency: recorded.currency.map(|pair| pair.map(str::to_owned)),
        numerals: recorded.numerals.map(|digits| digits.map(str::to_owned)),
        percent: recorded.percent.map(str::to_owned),
        minus: recorded.minus.map(str::to_owned),
        nan: recorded.nan.map(str::to_owned),
    };
    Locale::from_definition(definition).unwrap_or_else(|error| {
        panic!(
            "case {}: locale {} ({}) is not accepted: {error}",
            case.seq, recorded.id, recorded.definition_sha256
        )
    })
}

/// The call as the JavaScript wrote it, for a failure message.
fn describe(case: &Case) -> String {
    let arguments = case
        .args
        .iter()
        .map(|argument| match argument {
            Value::F64(bits) => format!("{} /* {bits:016X} */", f64::from_bits(*bits)),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let locale = case
        .locale
        .map_or_else(|| "-".to_owned(), |index| LOCALES[index].id.to_owned());
    match case.op {
        Op::Format => format!(
            "locale({locale}).format({:?})({arguments})",
            case.specifier_input.unwrap_or_default()
        ),
        Op::FormatPrefix => format!(
            "locale({locale}).formatPrefix({:?}, {:?})({arguments})",
            case.specifier_input.unwrap_or_default(),
            case.reference.and_then(Value::as_f64)
        ),
        Op::FormatSpecifier => format!("formatSpecifier({arguments})"),
        Op::PrecisionFixed => format!("precisionFixed({arguments})"),
        Op::PrecisionPrefix => format!("precisionPrefix({arguments})"),
        Op::PrecisionRound => format!("precisionRound({arguments})"),
    }
}
`;

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
    const reason = ignoreReason(block);
    if (reason !== null) lines.push(`#[ignore = ${rustStringLiteral(reason)}]`);
    lines.push(`fn ${block.fnName}() {`);
    lines.push(`    crate::support::run(&BLOCKS[${block.indexInModule}]);`);
    lines.push("}");
  }
  lines.push("");
  return lines.join("\n");
}

function renderMain(modules) {
  const blocks = modules.flatMap((module) => module.blocks);
  const blockCount = blocks.length;
  const ignored = blocks
    .map((block) => ({block, reason: ignoreReason(block)}))
    .filter((entry) => entry.reason !== null);
  const lines = [];
  lines.push(...generatedHeader(`${FIXTURE_DIR}/calls.jsonl and ${FIXTURE_DIR}/assertion-map.json`));
  lines.push("");
  lines.push("//! The generated d3-format conformance suite.");
  lines.push("//!");
  lines.push("//! One Rust test per original JavaScript test block, carrying the recorded oracle");
  lines.push("//! calls and the static assertion id of each one. Each test replays its calls");
  lines.push("//! against the public API and compares every answer with what d3 returned; see");
  lines.push("//! `support::run`.");
  lines.push("//!");
  lines.push(`//! ${blockCount - ignored.length} of the ${blockCount} blocks run. The other ${ignored.length} are`);
  lines.push("//! `#[ignore]`d because the oracle recorded nothing they could replay -- either no");
  lines.push("//! boundary call at all, or only calls whose JavaScript arguments the typed API");
  lines.push("//! declines. `IGNORED` below lists them, and a test asserts that list is exactly");
  lines.push("//! the set the recorded calls justify, so a passing block cannot stay skipped.");
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
  // libtest prints `test result: ok.` for a target whose only outcomes are "passed"
  // and "ignored", so the count of blocks that did not run is the number a CI
  // dashboard drops. It is pinned here instead, as data a test compares against.
  lines.push("/// Every `#[ignore]`d block: its file, its index in that file, and the reason.");
  lines.push("///");
  lines.push("/// Emitted by the generator from the recorded calls, and checked below against");
  lines.push("/// the same calls as compiled in. Nothing here is a judgement call about the");
  lines.push("/// port: a block is listed when the oracle gave it nothing to replay.");
  lines.push(`static IGNORED: [(&str, u32, &str); ${ignored.length}] = [`);
  for (const {block, reason} of ignored) {
    const parts = [rustStringLiteral(block.file), `${block.testIndex}`, rustStringLiteral(reason)];
    const inline = parts.join(", ");
    if (inline.length <= FN_CALL_WIDTH) lines.push(`    (${inline}),`);
    else lines.push("    (", ...parts.map((part) => `        ${part},`), "    ),");
  }
  lines.push("];");
  lines.push("");
  lines.push("/// The skipped blocks are exactly the ones the oracle cannot ask about.");
  lines.push("///");
  lines.push("/// The `#[ignore]` attributes are baked into the generated source, so on their");
  lines.push("/// own they are a claim from generation time. This recomputes the reasons from");
  lines.push("/// the recorded calls and compares, which fails if a block that could run is");
  lines.push("/// still skipped -- the one way this suite could report agreement it never");
  lines.push("/// tested.");
  lines.push("#[test]");
  lines.push("fn the_ignored_blocks_are_exactly_the_ones_with_no_replayable_call() {");
  lines.push("    let expected: Vec<(&str, u32, &str)> = MODULES");
  lines.push("        .iter()");
  lines.push("        .flat_map(|(_, blocks)| blocks.iter())");
  lines.push("        .filter_map(|block| {");
  lines.push("            let reason = block.ignore_reason()?;");
  lines.push("            Some((block.file, block.test_index, reason))");
  lines.push("        })");
  lines.push("        .collect();");
  lines.push("");
  lines.push("    assert_eq!(expected, IGNORED, \"ignored conformance blocks\");");
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
  lines.push("        \"d3-format: {blocks} generated conformance blocks covering {cases} oracle calls, \\");
  lines.push("{} of them ignored for want of a replayable call\",");
  lines.push("        IGNORED.len()");
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
