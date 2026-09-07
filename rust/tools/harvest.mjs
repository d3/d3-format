#!/usr/bin/env node
// Deterministic oracle harvester for the Rust port (MIGRATION_TO_RUST.md section 3.2).
//
// Runs the real 24 oracle test files under bare Node, redirecting only `vitest`
// and `src/index.js`, records every public-boundary call with full attribution,
// and writes the versioned fixtures plus the generated Rust sources and the
// provenance manifest.
//
//   node rust/tools/harvest.mjs            # same as --check
//   node rust/tools/harvest.mjs --check    # regenerate into a temp dir, byte-compare
//   node rust/tools/harvest.mjs --write    # atomically replace the tracked artifacts
//
// Every byte this tool writes uses LF endings and an ASCII-only encoding, so the
// comparison is stable on Windows checkouts (see the .gitattributes rule in
// section 1) and immune to editor or transport re-encoding.
import {createHash} from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync
} from "node:fs";
import {registerHooks} from "node:module";
import {tmpdir} from "node:os";
import {dirname, join, resolve, sep} from "node:path";
import {fileURLToPath, pathToFileURL} from "node:url";
import * as acorn from "acorn";

export const SCHEMA = "d3-format-rust/fixtures@1";
export const SCHEMA_VERSION = 1;
export const GENERATOR_VERSION = "1.0.0";
export const D3_FORMAT_VERSION = "3.1.2";
export const FIXTURE_DIR = `rust/fixtures/d3-format-${D3_FORMAT_VERSION}`;

/** Repository root, derived from this file's location rather than the cwd. */
export const REPO_ROOT = fileURLToPath(new URL("../../", import.meta.url));

/** ECMAScript level used to parse the oracle test files. */
const ECMA_VERSION = 2022;

/** Identifier the loader transform injects into each test module. */
const SITE_FN = "__d3SiteMarker";

const SHIM_URL = new URL("vitest-shim.mjs", import.meta.url).href;
const RECORDER_URL = new URL("instrumented.mjs", import.meta.url).href;

/** Baseline counts from MIGRATION_TO_RUST.md section 0.3. Drift here blocks the phase. */
export const BASELINE = {
  test_files: 24,
  test_blocks: 168,
  static_assertion_sites: 729,
  // Section 0.3 contrasts 735 textual `assert.*` occurrences with 729 executable
  // sites: the difference is commented-out assertions, which parsing drops and
  // pattern matching would not.
  textual_assertion_occurrences: 735,
  commented_assertion_occurrences: 6,
  dynamic_assertion_executions: 1078,
  boundary_calls: 793,
  boundary_calls_by_operation: {
    format: 624,
    formatPrefix: 11,
    formatSpecifier: 7,
    precisionFixed: 10,
    precisionPrefix: 133,
    precisionRound: 8
  },
  distinct_specifier_inputs: 194,
  distinct_locale_definitions: 49,
  locale_files: 59,
  locale_files_with_numerals: 23
};

/** Field order of a canonical locale definition; matches section 5.3's LocaleDefinition. */
export const LOCALE_KEYS = [
  "decimal",
  "thousands",
  "grouping",
  "currency",
  "numerals",
  "percent",
  "minus",
  "nan"
];

// ---------------------------------------------------------------------------
// Paths and IO
// ---------------------------------------------------------------------------

export function repoPath(...parts) {
  return join(REPO_ROOT, ...parts);
}

export function toPosix(path) {
  return path.split(sep).join("/");
}

/**
 * Reads a repository text file as UTF-8 with CRLF normalized to LF.
 *
 * The oracle directories (`src/`, `test/`, `locale/`) are deliberately not covered
 * by the .gitattributes rule that pins `rust/**` to LF, so on a Windows checkout
 * with core.autocrlf=true they are CRLF in the working tree and LF in the object
 * database. Hashing or slicing the raw bytes would make every fixture
 * platform-dependent, so all reads normalize first.
 */
export function readTextLf(relativePath) {
  return readFileSync(repoPath(relativePath), "utf8").replace(/\r\n/g, "\n");
}

export function sha256Hex(input) {
  return createHash("sha256").update(input, typeof input === "string" ? "utf8" : undefined).digest("hex");
}

/** SHA-256 of a repository source file, computed over its LF-normalized text. */
export function sourceHash(relativePath) {
  return sha256Hex(readTextLf(relativePath));
}

export function listDir(relativeDir, filter) {
  return readdirSync(repoPath(relativeDir))
    .filter((name) => filter.test(name))
    .sort();
}

export function testFiles() {
  return listDir("test", /-test\.js$/).map((name) => `test/${name}`);
}

export function localeFiles() {
  return listDir("locale", /\.json$/i).map((name) => `locale/${name}`);
}

export function sourceFiles() {
  return listDir("src", /\.js$/).map((name) => `src/${name}`);
}

// ---------------------------------------------------------------------------
// Canonical JSON
// ---------------------------------------------------------------------------

/**
 * Serializes a string as a JSON string literal with every non-ASCII and every
 * control code unit escaped. The result is pure ASCII, so the fixture bytes cannot
 * be perturbed by encoding, BOM or Unicode-normalization differences between
 * platforms, and no raw CR or LF can appear inside a record.
 */
export function jsonString(value) {
  let out = '"';
  for (let i = 0; i < value.length; i++) {
    const code = value.charCodeAt(i);
    if (code === 0x22) out += '\\"';
    else if (code === 0x5c) out += "\\\\";
    else if (code === 0x08) out += "\\b";
    else if (code === 0x0c) out += "\\f";
    else if (code === 0x0a) out += "\\n";
    else if (code === 0x0d) out += "\\r";
    else if (code === 0x09) out += "\\t";
    else if (code >= 0x20 && code <= 0x7e) out += value[i];
    else out += "\\u" + code.toString(16).padStart(4, "0");
  }
  return out + '"';
}

/**
 * Deterministic JSON serializer. Object keys keep insertion order, which every
 * builder in this file sets explicitly, and numbers must be safe integers: a
 * numeric *input* must never reach a fixture as a JSON number (section 3.2,
 * contract 6), so anything fractional or non-finite is a bug rather than data.
 */
export function canonicalJson(value, indent = 0, level = 0) {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "string") return jsonString(value);
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value)) {
      throw new TypeError(
        `canonicalJson refuses the non-integer number ${String(value)}; ` +
          "numeric values belong in an f64 bit-pattern encoding"
      );
    }
    return String(value);
  }
  const pad = indent ? "\n" + " ".repeat(indent * (level + 1)) : "";
  const close = indent ? "\n" + " ".repeat(indent * level) : "";
  if (Array.isArray(value)) {
    if (value.length === 0) return "[]";
    const items = value.map((item) => canonicalJson(item, indent, level + 1));
    return indent ? `[${pad}${items.join("," + pad)}${close}]` : `[${items.join(",")}]`;
  }
  if (typeof value === "object") {
    const keys = Object.keys(value);
    if (keys.length === 0) return "{}";
    const items = keys.map((key) => `${jsonString(key)}:${indent ? " " : ""}${canonicalJson(value[key], indent, level + 1)}`);
    return indent ? `{${pad}${items.join("," + pad)}${close}}` : `{${items.join(",")}}`;
  }
  throw new TypeError(`canonicalJson cannot serialize ${typeof value}`);
}

export function jsonlDocument(records) {
  return records.map((record) => canonicalJson(record) + "\n").join("");
}

/**
 * Renders a Rust string literal that is pure ASCII: every code point outside
 * printable ASCII becomes `\u{...}`. Generated Rust sources are ASCII-only
 * (section 3.4), which keeps them byte-stable and readable in any editor.
 */
export function rustStringLiteral(value) {
  let out = '"';
  for (const character of value) {
    const code = character.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) {
      throw new RangeError(`cannot render lone surrogate U+${code.toString(16).toUpperCase()} as a Rust string literal`);
    }
    if (character === '"') out += '\\"';
    else if (character === "\\") out += "\\\\";
    else if (character === "\n") out += "\\n";
    else if (character === "\r") out += "\\r";
    else if (character === "\t") out += "\\t";
    else if (character === "\0") out += "\\0";
    else if (code >= 0x20 && code <= 0x7e) out += character;
    else out += `\\u{${code.toString(16)}}`;
  }
  return out + '"';
}

// ---------------------------------------------------------------------------
// rustfmt-compatible layout
// ---------------------------------------------------------------------------

// Section 6.1 runs `cargo fmt --check` over the whole crate, and section 3.4
// forbids editing generated files by hand, so the generators have to emit code
// rustfmt already agrees with. Running rustfmt from the generators would make the
// committed bytes depend on the installed rustfmt version, which the byte-identity
// gates cannot tolerate, so instead the two layout rules rustfmt applies to
// generated data are reproduced here and the whole tree is verified with
// `cargo fmt --check`. That check is the proof: data that trips a rule these
// functions get wrong fails it loudly at generation time rather than silently.
//
// rustfmt defaults, style edition 2015/2018/2021.
const MAX_WIDTH = 100;
const SHORT_ELEMENT_WIDTH = 10; // short_array_element_width_threshold
const ARRAY_WIDTH = 60; // array_width
export const FN_CALL_WIDTH = 60; // fn_call_width, which also governs tuple literals

/**
 * Lays out the elements of a Rust array or slice literal the way rustfmt does.
 *
 * `prefix` is everything up to and including the opening bracket, already
 * indented (for example `        numerals: Some([`); `close` is the text that
 * follows the closing bracket on its own line (for example `]),`). `indent` is
 * the column the prefix starts at.
 *
 * rustfmt keeps the literal on one line when it fits inside both `array_width`
 * and `max_width`. Otherwise it breaks, and chooses between two layouts: elements
 * no wider than `short_array_element_width_threshold` are packed greedily, several
 * per line; anything wider gets one element per line.
 */
export function renderArrayLiteral({prefix, elements, close, indent}) {
  if (elements.length === 0) return [`${prefix}${close}`];

  const inline = elements.join(", ");
  const oneLine = `${prefix}${inline}${close}`;
  if (inline.length <= ARRAY_WIDTH && oneLine.length <= MAX_WIDTH) return [oneLine];

  const body = indent + 4;
  const lines = [prefix];
  if (elements.every((element) => element.length <= SHORT_ELEMENT_WIDTH)) {
    let row = [];
    // Each emitted row is `indent` spaces, the elements joined by ", ", and a
    // trailing comma, so a row of k elements costs 2*(k-1) + 1 beyond the elements.
    const rowWidth = (parts) => body + parts.reduce((sum, part) => sum + part.length, 0) + 2 * (parts.length - 1) + 1;
    for (const element of elements) {
      if (row.length > 0 && rowWidth([...row, element]) > MAX_WIDTH) {
        lines.push(" ".repeat(body) + row.join(", ") + ",");
        row = [];
      }
      row.push(element);
    }
    if (row.length > 0) lines.push(" ".repeat(body) + row.join(", ") + ",");
  } else {
    for (const element of elements) lines.push(" ".repeat(body) + element + ",");
  }
  lines.push(" ".repeat(indent) + close);
  return lines;
}

/**
 * Lays out an `Option`-wrapped array or slice field: `name: None,` when the value
 * is absent, otherwise `name: Some([...]),` laid out by `renderArrayLiteral`.
 *
 * `slice` picks `Some(&[...])` over `Some([...])`, for fields typed as a slice
 * rather than a fixed-size array.
 */
export function renderOptionalArrayField({name, value, renderElement, indent = 8, slice = false}) {
  const pad = " ".repeat(indent);
  if (value === null || value === undefined) return [`${pad}${name}: None,`];
  return renderArrayLiteral({
    prefix: `${pad}${name}: Some(${slice ? "&[" : "["}`,
    elements: value.map(renderElement),
    close: "]),",
    indent
  });
}

/**
 * Lays out a `name: value,` struct field the way rustfmt does.
 *
 * An over-long field is broken inside its value's call parentheses when it has any
 * -- `name: Value::Text(\n    "...",\n),` -- because rustfmt prefers that to
 * pushing the value down. A value with no parentheses to break, a bare string
 * literal in practice, moves to its own further-indented line instead.
 */
export function renderStructField({name, value, indent}) {
  const pad = " ".repeat(indent);
  const oneLine = `${pad}${name}: ${value},`;
  if (oneLine.length <= MAX_WIDTH) return [oneLine];

  const call = CALL.exec(value);
  if (call) {
    const [, callee, argument] = call;
    return [`${pad}${name}: ${callee}(`, ...renderCallArgument(argument, indent + 4), `${pad}),`];
  }
  return [`${pad}${name}:`, `${pad}    ${value},`];
}

const CALL = /^([A-Za-z_][A-Za-z0-9_:]*)\((.*)\)$/s;

/** The sole argument of a broken call, itself broken again if it still overflows. */
function renderCallArgument(argument, indent) {
  const pad = " ".repeat(indent);
  if (`${pad}${argument},`.length <= MAX_WIDTH) return [`${pad}${argument},`];
  const call = CALL.exec(argument);
  if (!call) return [`${pad}${argument},`];
  const [, callee, inner] = call;
  return [`${pad}${callee}(`, ...renderCallArgument(inner, indent + 4), `${pad}),`];
}

// ---------------------------------------------------------------------------
// Value encoding
// ---------------------------------------------------------------------------

const bitsView = new DataView(new ArrayBuffer(8));

/** Uppercase 16-digit big-endian bit pattern of an f64. */
export function f64Bits(value) {
  bitsView.setFloat64(0, value);
  return bitsView.getBigUint64(0).toString(16).toUpperCase().padStart(16, "0");
}

export function f64FromBits(bits) {
  bitsView.setBigUint64(0, BigInt("0x" + bits));
  return bitsView.getFloat64(0);
}

/**
 * Encodes one oracle value in the fixtures' uniform three-key shape. Numbers never
 * become JSON numbers: they are carried by their raw bit pattern, with the
 * ECMAScript `Number::toString` spelling alongside as oracle data. That is what
 * keeps -0, NaN payloads and the infinities intact.
 */
export function encodeValue(value) {
  if (typeof value === "number") return {kind: "f64", bits: f64Bits(value), text: String(value)};
  if (typeof value === "string") return {kind: "text", bits: null, text: value};
  if (typeof value === "boolean") return {kind: "boolean", bits: null, text: String(value)};
  if (value === undefined) return {kind: "undefined", bits: null, text: null};
  if (value === null) return {kind: "null", bits: null, text: null};
  if (typeof value === "object" && typeof value.text === "string" && value.constructor?.name === "SpecifierText") {
    return {kind: "specifier", bits: null, text: value.text};
  }
  throw new TypeError(`cannot encode oracle value of type ${typeof value}`);
}

// ---------------------------------------------------------------------------
// Locale definitions
// ---------------------------------------------------------------------------

/** Normalizes a d3 locale definition into the fixtures' fixed-key-order shape. */
export function canonicalLocaleDefinition(definition, where) {
  for (const key of Object.keys(definition)) {
    if (!LOCALE_KEYS.includes(key)) {
      throw new Error(`${where}: unexpected locale key ${JSON.stringify(key)}`);
    }
  }
  const out = {};
  for (const key of LOCALE_KEYS) {
    const value = definition[key];
    if (value === undefined) {
      out[key] = null;
      continue;
    }
    if (key === "grouping") {
      if (!Array.isArray(value)) throw new Error(`${where}: grouping must be an array`);
      out[key] = value.map((size) => {
        const n = Number(size);
        if (!Number.isSafeInteger(n) || n < 0) throw new Error(`${where}: bad grouping size ${String(size)}`);
        return n;
      });
    } else if (key === "currency") {
      if (!Array.isArray(value) || value.length !== 2) {
        throw new Error(`${where}: currency must be a two-element array`);
      }
      out[key] = value.map(String);
    } else if (key === "numerals") {
      if (!Array.isArray(value) || value.length !== 10) {
        throw new Error(`${where}: numerals must contain exactly ten strings, got ${value.length}`);
      }
      out[key] = value.map(String);
    } else {
      out[key] = String(value);
    }
  }
  return out;
}

export function localeHash(canonicalDefinition) {
  return sha256Hex(canonicalJson(canonicalDefinition));
}

/** Canonical definitions of the 59 committed locale files, keyed by hash. */
export function namedLocaleIndex() {
  const byHash = new Map();
  const byName = new Map();
  for (const path of localeFiles()) {
    const name = path.slice("locale/".length, -".json".length);
    const definition = canonicalLocaleDefinition(JSON.parse(readTextLf(path)), path);
    const hash = localeHash(definition);
    byName.set(name, {name, path, definition, hash, sourceHash: sourceHash(path)});
    if (!byHash.has(hash)) byHash.set(hash, name);
  }
  return {byHash, byName};
}

// ---------------------------------------------------------------------------
// Static analysis of the oracle test suite
// ---------------------------------------------------------------------------

const SKIP_AST_KEYS = new Set(["loc", "range", "start", "end"]);

function isAssertCall(node) {
  return (
    node.type === "CallExpression" &&
    node.callee.type === "MemberExpression" &&
    !node.callee.computed &&
    node.callee.object.type === "Identifier" &&
    node.callee.object.name === "assert" &&
    node.callee.property.type === "Identifier"
  );
}

function isTestCall(node) {
  return (
    node.type === "CallExpression" &&
    node.callee.type === "Identifier" &&
    node.callee.name === "test"
  );
}

// Only line breaks and their following indentation collapse; spacing inside string
// literals is preserved, because the snippet is what keeps a coverage override from
// silently re-pointing at a different assertion.
function snippetOf(source, node) {
  const text = source.slice(node.start, node.end).replace(/\n[ \t]*/g, " ").trim();
  return text.length <= 160 ? text : text.slice(0, 157) + "...";
}

/**
 * Counts `assert.` textually across the oracle tests, comments included.
 *
 * This is the number a grep-based harvester would get. Section 0.3 contrasts it
 * with the parsed count; keeping both in the manifest makes the six commented-out
 * assertions visible rather than a footnote.
 */
export function textualAssertionOccurrences() {
  let total = 0;
  for (const path of testFiles()) {
    total += (readTextLf(path).match(/\bassert\.[A-Za-z]/g) ?? []).length;
  }
  return total;
}

/**
 * Parses every oracle test file with Acorn and returns the static inventory:
 * test blocks in source order and executable assertion sites. Comments are
 * ignored naturally, which is what separates the 729 executable sites from the
 * 735 textual `assert.*` occurrences.
 *
 * Static assertion ids are `relative/path:line:column` with both coordinates
 * one-based, taken from the LF-normalized source.
 */
export function analyzeTestFiles() {
  const files = [];
  const sitesById = new Map();
  for (const path of testFiles()) {
    const source = readTextLf(path);
    const ast = acorn.parse(source, {
      ecmaVersion: ECMA_VERSION,
      sourceType: "module",
      locations: true,
      ranges: true
    });
    const blocks = [];
    const sites = [];
    const blockStack = [];
    (function descend(node) {
      const startsBlock = isTestCall(node);
      let block = null;
      if (startsBlock) {
        block = {
          index: blocks.length,
          title: node.arguments[0]?.value,
          line: node.loc.start.line,
          siteIds: []
        };
        if (typeof block.title !== "string") {
          throw new Error(`${path}:${block.line}: test() title is not a string literal`);
        }
        blocks.push(block);
        blockStack.push(block);
      }
      if (isAssertCall(node)) {
        const enclosing = blockStack[blockStack.length - 1];
        if (!enclosing) throw new Error(`${path}:${node.loc.start.line}: assert call outside any test block`);
        if (node.arguments.length === 0) {
          throw new Error(`${path}:${node.loc.start.line}: assert call with no arguments`);
        }
        const site = {
          id: `${path}:${node.loc.start.line}:${node.loc.start.column + 1}`,
          file: path,
          line: node.loc.start.line,
          column: node.loc.start.column + 1,
          method: node.callee.property.name,
          testIndex: enclosing.index,
          testTitle: enclosing.title,
          indexInTest: enclosing.siteIds.length,
          argStart: node.arguments[0].start,
          argEnd: node.arguments[0].end,
          snippet: snippetOf(source, node)
        };
        if (sitesById.has(site.id)) throw new Error(`duplicate static assertion id ${site.id}`);
        sitesById.set(site.id, site);
        enclosing.siteIds.push(site.id);
        sites.push(site);
      }
      for (const key of Object.keys(node)) {
        if (SKIP_AST_KEYS.has(key)) continue;
        const child = node[key];
        if (Array.isArray(child)) {
          for (const item of child) if (item && typeof item.type === "string") descend(item);
        } else if (child && typeof child.type === "string") {
          descend(child);
        }
      }
      if (startsBlock) blockStack.pop();
    })(ast);
    files.push({path, source, blocks, sites, sha256: sha256Hex(source)});
  }
  return {files, sitesById};
}

/**
 * Rewrites one oracle test file so that `__site(id)` runs immediately before the
 * actual argument of every assertion is evaluated. Wrapping the first argument in
 * a sequence expression keeps precedence and evaluation order intact, and leaves
 * the site current for the rest of the call -- which is what makes
 * `assert.throws` attribute the calls inside its callback correctly.
 *
 * The injected import is appended to line 1 rather than placed on its own line so
 * that line numbers in the transformed module still match the original file.
 */
export function transformTestSource(source, sites) {
  const edits = [];
  for (const site of sites) {
    edits.push({at: site.argStart, text: `(0,${SITE_FN}(${jsonString(site.id)}),`});
    edits.push({at: site.argEnd, text: ")"});
  }
  edits.sort((a, b) => b.at - a.at);
  let out = source;
  for (const edit of edits) out = out.slice(0, edit.at) + edit.text + out.slice(edit.at);
  return `import {__site as ${SITE_FN}} from "vitest";` + out;
}

// ---------------------------------------------------------------------------
// Running the oracle
// ---------------------------------------------------------------------------

let oracleRun = null;

/**
 * Executes the oracle suite once under the shim and the recorder, and returns the
 * runtime observations. Node's module hooks are process-wide and the oracle test
 * modules are cached after their first evaluation, so this memoizes.
 */
export async function runOracle() {
  if (oracleRun) return oracleRun;

  const cwd = process.cwd();
  // The oracle tests read `./locale/*.json` relative to the process cwd.
  if (resolve(cwd) !== resolve(REPO_ROOT).replace(/[\\/]$/, "")) {
    const expected = toPosix(resolve(REPO_ROOT).replace(/[\\/]$/, ""));
    throw new Error(`run this tool from the repository root (expected cwd ${expected}, got ${toPosix(resolve(cwd))})`);
  }

  const analysis = analyzeTestFiles();
  const transformed = new Map();
  for (const file of analysis.files) {
    transformed.set(pathToFileURL(repoPath(file.path)).href, transformTestSource(file.source, file.sites));
  }

  registerHooks({
    resolve(specifier, context, nextResolve) {
      if (specifier === "vitest") return {url: SHIM_URL, shortCircuit: true};
      // Skip the recorder's own `import * as real from "../../src/index.js"`,
      // which would otherwise resolve to itself.
      if (specifier.endsWith("src/index.js") && context.parentURL !== RECORDER_URL) {
        return {url: RECORDER_URL, shortCircuit: true};
      }
      return nextResolve(specifier, context);
    },
    load(url, context, nextLoad) {
      const source = transformed.get(url);
      if (source !== undefined) return {format: "module", source, shortCircuit: true};
      return nextLoad(url, context);
    }
  });

  const shim = await import(SHIM_URL);
  const recorder = await import(RECORDER_URL);

  for (const file of analysis.files) {
    shim.beginFile(file.path);
    await import(pathToFileURL(repoPath(file.path)).href);
    const pending = shim.state.pending.splice(0);
    if (pending.length) {
      const before = shim.state.executions;
      await Promise.all(pending);
      if (shim.state.executions !== before) {
        throw new Error(
          `${file.path}: an async test body ran assertions after its module finished; ` +
            "attribution would be wrong"
        );
      }
    }
  }

  const {state} = shim;
  if (state.skipped.length) {
    throw new Error(`the oracle suite skipped ${state.skipped.length} test(s): ${state.skipped.join(", ")}`);
  }
  if (state.failures.length) {
    const lines = state.failures
      .slice(0, 20)
      .map((f) => `  ${f.site ?? "?"} (${f.file} :: ${f.title}): ${f.detail}`);
    throw new Error(
      `the oracle suite reported ${state.failures.length} JavaScript failure(s):\n${lines.join("\n")}`
    );
  }
  if (state.executions !== state.records) {
    throw new Error(
      `assertion counting disagreed: ${state.executions} site markers vs ${state.records} completed records`
    );
  }
  // A call made in a block's setup code has no static assertion site. That is
  // legitimate -- `const s = formatSpecifier("")` is one -- but it must still be
  // attributed to a real test block.
  for (const row of recorder.rows) {
    if (row.file === null || row.testIndex < 0 || row.title === null) {
      throw new Error(`a ${row.operation} call was made outside any test block`);
    }
  }

  oracleRun = {analysis, state, rows: recorder.rows};
  return oracleRun;
}

// ---------------------------------------------------------------------------
// Fixture construction
// ---------------------------------------------------------------------------

/**
 * Sites that no recorded boundary call can cover, each routed to a named Rust test
 * with an explicit reason. Section 3.3 requires this for every hand-ported or
 * intentionally reinterpreted site, and requires that mutable-global identity and
 * JavaScript coercion assertions be mapped to DIVERGENCES.md rather than being
 * declared "passed" by a different Rust API.
 *
 * Entries are matched on (file, test title, zero-based assertion index within the
 * block) and are verified against the current source snippet, so a change upstream
 * fails the harvest instead of silently re-pointing a mapping.
 */
function overrideGroup({file, title, coverage, divergence = null, reason, rustTests, snippets}) {
  return snippets.map((snippet, index) => ({
    file,
    title,
    index,
    snippet,
    coverage,
    divergence,
    reason,
    rustTests
  }));
}

const PORTED_SPECIFIER = "rust/tests/ported/specifier.rs";
const PORTED_DIVERGENCES = "rust/tests/ported/divergences.rs";
const PORTED_LOCALES = "rust/tests/ported/locales.rs";

export const COVERAGE_OVERRIDES = [
  ...overrideGroup({
    file: "test/defaultLocale-test.js",
    title: "formatDefaultLocale(definition) affects format",
    coverage: "divergence",
    divergence: "no-mutable-default-locale",
    reason:
      "Asserts that the module-level `format` binding is rebound by formatDefaultLocale. " +
      "The Rust API has no mutable process-global default locale, so there is no binding " +
      "whose identity could change; a different Rust call cannot be said to pass this.",
    rustTests: [`${PORTED_DIVERGENCES}::default_locale_is_not_a_mutable_process_global`],
    snippets: ["assert.strictEqual(format, locale.format)"]
  }),
  ...overrideGroup({
    file: "test/defaultLocale-test.js",
    title: "formatDefaultLocale(definition) affects formatPrefix",
    coverage: "divergence",
    divergence: "no-mutable-default-locale",
    reason:
      "Asserts that the module-level `formatPrefix` binding is rebound by formatDefaultLocale. " +
      "The Rust API has no mutable process-global default locale.",
    rustTests: [`${PORTED_DIVERGENCES}::default_locale_is_not_a_mutable_process_global`],
    snippets: ["assert.strictEqual(formatPrefix, locale.formatPrefix)"]
  }),
  ...overrideGroup({
    file: "test/format-test.js",
    title: "format(specifier).toString() returns the normalized specifier",
    coverage: "ported",
    reason:
      "Reads the compiled formatter's normalized specifier rather than formatting a value, " +
      "so it produces no boundary call. Ported to Formatter::specifier() and Display for FormatSpecifier.",
    rustTests: [`${PORTED_SPECIFIER}::formatter_reports_its_normalized_specifier`],
    snippets: ['assert.strictEqual(format("d") + "", " >-d")']
  }),
  ...overrideGroup({
    file: "test/format-test.js",
    title: "format(specifier) throws an error for invalid formats",
    coverage: "ported",
    reason:
      "The call throws before any formatter is produced, so nothing is recorded. Ported to " +
      "ParseError, whose Display text is exactly `invalid format: {input}` (section 5.2).",
    rustTests: [`${PORTED_SPECIFIER}::invalid_specifiers_are_rejected_with_the_d3_message`],
    snippets: [
      'assert.throws(() => { format("foo"); }, /invalid format: foo/)',
      'assert.throws(() => { format(".-2s"); }, /invalid format: \\.-2s/)',
      'assert.throws(() => { format(".f"); }, /invalid format: \\.f/)'
    ]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier) throws an error for invalid formats",
    coverage: "ported",
    reason:
      "The call throws before returning, so nothing is recorded. Ported to " +
      "`FormatSpecifier::from_str` returning ParseError with the same message.",
    rustTests: [`${PORTED_SPECIFIER}::invalid_specifiers_are_rejected_with_the_d3_message`],
    snippets: [
      'assert.throws(() => { formatSpecifier("foo"); }, /invalid format: foo/)',
      'assert.throws(() => { formatSpecifier(".-2s"); }, /invalid format: \\.-2s/)',
      'assert.throws(() => { formatSpecifier(".f"); }, /invalid format: \\.f/)'
    ]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier) returns an instanceof formatSpecifier",
    coverage: "divergence",
    divergence: "immutable-typed-specifier",
    reason:
      "Asserts JavaScript prototype identity between the factory function and its instances. " +
      "Rust parsing yields a concrete `FormatSpecifier` value; there is no prototype to compare.",
    rustTests: [`${PORTED_SPECIFIER}::parsing_yields_a_concrete_format_specifier_type`],
    snippets: ["assert.strictEqual(s instanceof formatSpecifier, true)"]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: 'formatSpecifier("") has the expected defaults',
    coverage: "ported",
    reason:
      "Reads parsed specifier fields directly, which produces no boundary call. Ported to the " +
      "typed getters in section 5.2; `undefined` width and precision become `None`.",
    rustTests: [`${PORTED_SPECIFIER}::empty_specifier_has_the_documented_defaults`],
    snippets: [
      'assert.strictEqual(s.fill, " ")',
      'assert.strictEqual(s.align, ">")',
      'assert.strictEqual(s.sign, "-")',
      'assert.strictEqual(s.symbol, "")',
      "assert.strictEqual(s.zero, false)",
      "assert.strictEqual(s.width, undefined)",
      "assert.strictEqual(s.comma, false)",
      "assert.strictEqual(s.precision, undefined)",
      "assert.strictEqual(s.trim, false)",
      'assert.strictEqual(s.type, "")'
    ]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier) preserves unknown types",
    coverage: "ported",
    reason:
      "Reads parsed specifier fields directly. Ported to the typed getters; unknown ASCII type " +
      "letters stay stored and format as d3's `.12~g` fallback.",
    rustTests: [`${PORTED_SPECIFIER}::unknown_type_letters_are_preserved`],
    snippets: ["assert.strictEqual(s.trim, false)", 'assert.strictEqual(s.type, "q")']
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier) preserves shorthand",
    coverage: "ported",
    reason: "Reads parsed specifier fields directly. Ported to the typed getters.",
    rustTests: [`${PORTED_SPECIFIER}::empty_specifier_has_the_documented_defaults`],
    snippets: ["assert.strictEqual(s.trim, false)", 'assert.strictEqual(s.type, "")']
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier).toString() reflects current field values",
    coverage: "divergence",
    divergence: "immutable-typed-specifier",
    reason:
      "Mutates public specifier fields between assertions and re-reads toString(). " +
      "`FormatSpecifier` is immutable after validated construction, so the sequence has no Rust " +
      "equivalent; the equivalent normalized spellings are asserted through the builder instead.",
    rustTests: [`${PORTED_DIVERGENCES}::format_specifier_is_immutable_after_construction`],
    snippets: [
      'assert.strictEqual((s.fill = "_", s) + "", "_>-")',
      'assert.strictEqual((s.align = "^", s) + "", "_^-")',
      'assert.strictEqual((s.sign = "+", s) + "", "_^+")',
      'assert.strictEqual((s.symbol = "$", s) + "", "_^+$")',
      'assert.strictEqual((s.zero = true, s) + "", "_^+$0")',
      'assert.strictEqual((s.width = 12, s) + "", "_^+$012")',
      'assert.strictEqual((s.comma = true, s) + "", "_^+$012,")',
      'assert.strictEqual((s.precision = 2, s) + "", "_^+$012,.2")',
      'assert.strictEqual((s.type = "f", s) + "", "_^+$012,.2f")',
      'assert.strictEqual((s.trim = true, s) + "", "_^+$012,.2~f")'
    ]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier).toString() clamps precision to zero",
    coverage: "divergence",
    divergence: "immutable-typed-specifier",
    reason:
      "Assigns a negative precision to a public field and relies on toString()'s `Math.max(0, p | 0)` " +
      "clamp. Rust precision is unsigned and validated at construction, so a negative precision is " +
      "unrepresentable rather than clamped.",
    rustTests: [`${PORTED_DIVERGENCES}::format_specifier_is_immutable_after_construction`],
    snippets: ['assert.strictEqual((s.precision = -1, s) + "", " >-.0")']
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "formatSpecifier(specifier).toString() clamps width to one",
    coverage: "divergence",
    divergence: "immutable-typed-specifier",
    reason:
      "Assigns a negative width to a public field and relies on toString()'s `Math.max(1, w | 0)` " +
      "clamp. Rust width is unsigned and validated at construction.",
    rustTests: [`${PORTED_DIVERGENCES}::format_specifier_is_immutable_after_construction`],
    snippets: ['assert.strictEqual((s.width = -1, s) + "", " >-1")']
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "new FormatSpecifier({}) has the expected defaults",
    coverage: "ported",
    reason:
      "Constructs a specifier from an empty object and reads its fields, producing no boundary " +
      "call. Ported to `FormatSpecifier`'s builder, whose defaults must match the empty specifier.",
    rustTests: [`${PORTED_SPECIFIER}::builder_defaults_match_the_empty_specifier`],
    snippets: [
      'assert.strictEqual(s.fill, " ")',
      'assert.strictEqual(s.align, ">")',
      'assert.strictEqual(s.sign, "-")',
      'assert.strictEqual(s.symbol, "")',
      "assert.strictEqual(s.zero, false)",
      "assert.strictEqual(s.width, undefined)",
      "assert.strictEqual(s.comma, false)",
      "assert.strictEqual(s.precision, undefined)",
      "assert.strictEqual(s.trim, false)",
      'assert.strictEqual(s.type, "")'
    ]
  }),
  ...overrideGroup({
    file: "test/formatSpecifier-test.js",
    title: "new FormatSpecifier({\u2026}) coerces all inputs to the expected types",
    coverage: "divergence",
    divergence: "no-specifier-field-coercion",
    reason:
      "Passes numbers where d3 expects strings and booleans and relies on `String(v)`, `!!v` and " +
      "ToInt32 coercion. The Rust builder takes typed values and rejects what it cannot represent, " +
      "so no Rust call reproduces this and none may be said to pass it.",
    rustTests: [`${PORTED_DIVERGENCES}::specifier_construction_does_not_coerce_arbitrary_javascript_values`],
    snippets: [
      'assert.strictEqual(s.fill, "1")',
      'assert.strictEqual(s.align, "2")',
      'assert.strictEqual(s.sign, "3")',
      'assert.strictEqual(s.symbol, "4")',
      "assert.strictEqual(s.zero, true)",
      "assert.strictEqual(s.width, 6)",
      "assert.strictEqual(s.comma, true)",
      "assert.strictEqual(s.precision, 8)",
      "assert.strictEqual(s.trim, true)",
      'assert.strictEqual(s.type, "10")'
    ]
  }),
  ...overrideGroup({
    file: "test/locale-test.js",
    title: "formatLocale({nan: nan}) observes the specified not-a-number representation",
    coverage: "divergence",
    divergence: "no-input-coercion",
    reason:
      "Passes JavaScript `undefined` to a numeric formatter and relies on `+undefined` being NaN. " +
      "Rust's `format_number` takes an f64 and `format_text` is rejected by a numeric specifier " +
      "with FormatError::InputTypeMismatch, so the coercion itself has no Rust equivalent. The " +
      "recorded call still carries the coerced NaN bits, and the named test asserts both the NaN " +
      "output and the rejection of non-numeric input.",
    rustTests: [`${PORTED_DIVERGENCES}::numeric_formatters_reject_non_numeric_input`],
    snippets: [
      'assert.strictEqual(formatLocale({nan: "N/A"}).format("6.2f")(undefined), "   N/A")',
      'assert.strictEqual(formatLocale({nan: "-"}).format("<6.2g")(undefined), "-     ")',
      'assert.strictEqual(formatLocale({}).format(" 6.2f")(undefined), "   NaN")'
    ]
  }),
  ...overrideGroup({
    file: "test/locale-test.js",
    title: "locale data is valid",
    coverage: "ported",
    reason:
      "Iterates the 59 locale JSON files and asserts the required keys are present; it makes no " +
      "boundary call. Ported to gen-locales.mjs schema validation plus a Rust test over the " +
      "generated locale table.",
    rustTests: [`${PORTED_LOCALES}::every_generated_locale_declares_the_required_keys`],
    snippets: [
      'assert.strictEqual("currency" in locale, true)',
      'assert.strictEqual("decimal" in locale, true)',
      'assert.strictEqual("grouping" in locale, true)',
      'assert.strictEqual("thousands" in locale, true)'
    ]
  })
];

function overrideKey(file, title, indexInTest) {
  return `${file}\u0000${title}\u0000${indexInTest}`;
}

function buildLocaleTable(rows) {
  const named = namedLocaleIndex();
  const byHash = new Map();
  for (const row of rows) {
    if (!row.localeDefinition) continue;
    const definition = canonicalLocaleDefinition(row.localeDefinition, `${row.file} :: ${row.title}`);
    const hash = localeHash(definition);
    if (!byHash.has(hash)) {
      byHash.set(hash, {
        id: named.byHash.get(hash) ?? `anon:${hash.slice(0, 16)}`,
        hash,
        definition
      });
    }
    row.canonicalLocale = byHash.get(hash);
  }
  return {byHash, named};
}

function buildCallRecords(rows) {
  return rows.map((row, seq) => ({
    seq,
    operation: row.operation,
    file: row.file,
    test_index: row.testIndex,
    test_title: row.title,
    assertion_id: row.site,
    assertion_ordinal: row.site === null ? null : row.ordinal,
    locale_id: row.canonicalLocale?.id ?? null,
    locale_hash: row.canonicalLocale?.hash ?? null,
    locale_definition: row.canonicalLocale?.definition ?? null,
    specifier_input: row.specifierInput
      ? {kind: row.specifierInput.kind, bits: null, text: row.specifierInput.text}
      : null,
    specifier: row.specifier,
    reference: row.reference ? encodeValue(row.reference[0]) : null,
    args: row.args.map(encodeValue),
    output: encodeValue(row.output)
  }));
}

/**
 * Classifies every static assertion site. A site whose every execution produced at
 * least one recorded boundary call is covered by generated cases; everything else
 * needs an explicit override, and the harvest fails loudly until it has one.
 *
 * Generated sites get their Rust test path attached later, once gen-tests has
 * decided the names -- see `attachGeneratedTests`.
 */
export function buildSiteInventory(analysis, state, callRecords) {
  const callsBySite = new Map();
  for (const record of callRecords) {
    if (record.assertion_id === null) continue;
    if (!callsBySite.has(record.assertion_id)) callsBySite.set(record.assertion_id, []);
    callsBySite.get(record.assertion_id).push(record);
  }

  const overrides = new Map();
  for (const entry of COVERAGE_OVERRIDES) {
    const key = overrideKey(entry.file, entry.title, entry.index);
    if (overrides.has(key)) throw new Error(`duplicate coverage override for ${key.replace(/\u0000/g, " :: ")}`);
    overrides.set(key, entry);
  }

  const sites = [];
  const unmapped = [];
  const usedOverrides = new Set();

  for (const file of analysis.files) {
    for (const site of file.sites) {
      const executions = state.executionsById.get(site.id) ?? 0;
      const calls = callsBySite.get(site.id) ?? [];
      const covered = new Set(calls.map((c) => c.assertion_ordinal));
      const key = overrideKey(site.file, site.testTitle, site.indexInTest);
      const override = overrides.get(key);
      if (override) usedOverrides.add(key);

      if (executions === 0) {
        unmapped.push(`${site.id} never executed (${site.file} :: ${site.testTitle})`);
        continue;
      }

      const fullyGenerated = executions > 0 && covered.size === executions;
      let entry;
      if (override) {
        if (override.snippet !== site.snippet) {
          throw new Error(
            `coverage override for ${site.id} is stale:\n  expected ${JSON.stringify(override.snippet)}\n  actual   ${JSON.stringify(site.snippet)}`
          );
        }
        entry = {
          id: site.id,
          file: site.file,
          line: site.line,
          column: site.column,
          method: site.method,
          test_index: site.testIndex,
          test_title: site.testTitle,
          index_in_test: site.indexInTest,
          snippet: site.snippet,
          executions,
          calls: calls.map((c) => c.seq),
          coverage: override.coverage,
          divergence: override.divergence ?? null,
          reason: override.reason,
          rust_tests: [...override.rustTests].sort()
        };
      } else if (fullyGenerated) {
        entry = {
          id: site.id,
          file: site.file,
          line: site.line,
          column: site.column,
          method: site.method,
          test_index: site.testIndex,
          test_title: site.testTitle,
          index_in_test: site.indexInTest,
          snippet: site.snippet,
          executions,
          calls: calls.map((c) => c.seq),
          coverage: "generated",
          divergence: null,
          reason: null,
          rust_tests: []
        };
      } else {
        unmapped.push(
          `${site.id} (${site.file} :: ${site.testTitle} #${site.indexInTest}) ` +
            `has ${executions} execution(s) but only ${covered.size} recorded call ordinal(s): ${site.snippet}`
        );
        continue;
      }
      sites.push(entry);
    }
  }

  const stale = [...overrides.keys()].filter((key) => !usedOverrides.has(key));
  if (stale.length) {
    throw new Error(
      `coverage overrides that match no assertion site:\n  ${stale.map((k) => k.replace(/\u0000/g, " :: ")).join("\n  ")}`
    );
  }
  if (unmapped.length) {
    throw new Error(
      `${unmapped.length} assertion site(s) have no coverage mapping. Add a COVERAGE_OVERRIDES entry ` +
        `in rust/tools/harvest.mjs for each:\n  ${unmapped.join("\n  ")}`
    );
  }
  return sites;
}

/** Points every generated site at the Rust test gen-tests emitted for its block. */
export function attachGeneratedTests(sites, generatedTestNames) {
  for (const site of sites) {
    if (site.coverage !== "generated") continue;
    const name = generatedTestNames.get(`${site.file}\u0000${site.test_index}`);
    if (!name) throw new Error(`no generated test was emitted for ${site.file} #${site.test_index}`);
    site.rust_tests = [name];
  }
  return sites;
}

function tally(rows) {
  const byOperation = {};
  for (const key of Object.keys(BASELINE.boundary_calls_by_operation)) byOperation[key] = 0;
  for (const row of rows) {
    if (!(row.operation in byOperation)) throw new Error(`unexpected operation ${row.operation}`);
    byOperation[row.operation]++;
  }
  return byOperation;
}

// ---------------------------------------------------------------------------
// Primitive numeric fixtures (section 4.1 threshold cases)
// ---------------------------------------------------------------------------

function nextUp(x) {
  if (Number.isNaN(x) || x === Infinity) return x;
  if (x === 0) return Number.MIN_VALUE;
  bitsView.setFloat64(0, x);
  const bits = bitsView.getBigUint64(0);
  bitsView.setBigUint64(0, x > 0 ? bits + 1n : bits - 1n);
  return bitsView.getFloat64(0);
}

function nextDown(x) {
  if (Number.isNaN(x) || x === -Infinity) return x;
  if (x === 0) return -Number.MIN_VALUE;
  bitsView.setFloat64(0, x);
  const bits = bitsView.getBigUint64(0);
  bitsView.setBigUint64(0, x > 0 ? bits - 1n : bits + 1n);
  return bitsView.getFloat64(0);
}

const FIXED_PRECISIONS = Array.from({length: 21}, (_, i) => i);
const SIGNIFICANT_PRECISIONS = Array.from({length: 21}, (_, i) => i + 1);

/**
 * The threshold values section 4.1 requires as explicit fixtures, plus the values
 * the oracle suite itself leans on. Ordering is fixed by construction.
 */
function thresholdValues() {
  const seeds = [
    ["1e21", 1e21],
    ["1e20", 1e20],
    ["1e-6", 1e-6],
    ["1e-7", 1e-7],
    ["one", 1],
    ["half", 0.5],
    ["three-halves", 1.5],
    ["five-halves", 2.5],
    ["one-eighth", 0.125],
    ["nine-and-a-half", 9.5],
    ["ninety-nine-and-a-half", 99.5],
    ["nine-hundred-ninety-nine-and-a-half", 999.5],
    ["seven-hundredths", 0.07],
    ["1e-9", 1e-9],
    ["1e-24", 1e-24],
    ["1.29e-30", 1.29e-30],
    ["1.23e30", 1.23e30],
    ["1.3e27", 1.3e27],
    ["1.3e107", 1.3e107],
    ["145999999.99999347", 145999999.99999347],
    ["min-normal", 2.2250738585072014e-308],
    ["max-finite", Number.MAX_VALUE],
    ["min-subnormal", Number.MIN_VALUE]
  ];
  const values = [];
  const push = (id, value) => values.push({id, value});
  push("zero", 0);
  push("negative-zero", -0);
  for (const [id, value] of seeds) {
    push(`${id}`, value);
    push(`${id}:pred`, nextDown(value));
    push(`${id}:succ`, nextUp(value));
    push(`-${id}`, -value);
    push(`-${id}:pred`, nextDown(-value));
    push(`-${id}:succ`, nextUp(-value));
  }
  push("infinity", Infinity);
  push("-infinity", -Infinity);
  push("nan", f64FromBits("7FF8000000000000"));
  push("nan:negative", f64FromBits("FFF8000000000000"));
  push("nan:payload", f64FromBits("7FF8000000000001"));
  push("nan:payload-negative", f64FromBits("FFF8000000000001"));
  return values;
}

function decimalRecord(id, operation, value, precision, radix, output) {
  return {
    id,
    operation,
    input: encodeValue(value),
    precision,
    radix,
    output: encodeValue(output)
  };
}

/**
 * ECMAScript primitive-conversion oracle records for the section 4.1 threshold set.
 * These are the contract Phase 2's decimal engine is written against.
 */
export function buildDecimalRecords() {
  const records = [];
  for (const {id, value} of thresholdValues()) {
    records.push(decimalRecord(id, "numberToString", value, null, null, String(value)));
    for (const p of FIXED_PRECISIONS) {
      records.push(decimalRecord(id, "toFixed", value, p, null, value.toFixed(p)));
    }
    records.push(decimalRecord(id, "toExponential", value, null, null, value.toExponential()));
    for (const p of FIXED_PRECISIONS) {
      records.push(decimalRecord(id, "toExponential", value, p, null, value.toExponential(p)));
    }
    for (const p of SIGNIFICANT_PRECISIONS) {
      records.push(decimalRecord(id, "toPrecision", value, p, null, value.toPrecision(p)));
    }
    // Type "d" and the integer radix types round first and never see a negative
    // zero, matching src/formatDecimal.js and src/formatTypes.js.
    const rounded = Math.round(Math.abs(value));
    if (Number.isFinite(rounded)) {
      records.push(
        decimalRecord(id, "toLocaleStringEn", rounded, null, null, rounded.toLocaleString("en").replace(/,/g, ""))
      );
      for (const radix of [2, 8, 16]) {
        records.push(decimalRecord(id, "toStringRadix", rounded, null, radix, rounded.toString(radix)));
      }
    }
  }
  return records;
}

// ---------------------------------------------------------------------------
// Artifact generation
// ---------------------------------------------------------------------------

/**
 * Produces every generated artifact as LF text keyed by repository-relative path.
 * `manifest.json` is built last because it hashes all the others.
 */
export async function generateAll() {
  const {analysis, state, rows} = await runOracle();

  const localeTable = buildLocaleTable(rows);
  const callRecords = buildCallRecords(rows);
  const byOperation = tally(rows);

  const {buildGeneratedTests} = await import("./gen-tests.mjs");
  const {buildGeneratedLocales} = await import("./gen-locales.mjs");

  const specifierInputs = new Set(
    rows.filter((row) => row.specifierInput && row.operation !== "formatSpecifier").map((row) => row.specifierInput.text)
  );
  const canonicalSpecifiers = new Set(rows.filter((row) => row.specifier !== null).map((row) => row.specifier));

  const localeGeneration = buildGeneratedLocales();
  const inventory = buildSiteInventory(analysis, state, callRecords);
  const testGeneration = buildGeneratedTests({callRecords, sites: inventory});
  const assertionSites = attachGeneratedTests(inventory, testGeneration.testNames);

  const counts = {
    test_files: analysis.files.length,
    test_blocks: analysis.files.reduce((sum, file) => sum + file.blocks.length, 0),
    static_assertion_sites: analysis.files.reduce((sum, file) => sum + file.sites.length, 0),
    // Recorded so the manifest itself shows what parsing bought over pattern
    // matching, which is the whole justification for the Acorn dependency.
    textual_assertion_occurrences: textualAssertionOccurrences(),
    commented_assertion_occurrences:
      textualAssertionOccurrences() - analysis.files.reduce((sum, file) => sum + file.sites.length, 0),
    dynamic_assertion_executions: state.executions,
    boundary_calls: callRecords.length,
    boundary_calls_by_operation: byOperation,
    distinct_specifier_inputs: specifierInputs.size,
    distinct_canonical_specifiers: canonicalSpecifiers.size,
    distinct_locale_definitions: localeTable.byHash.size,
    locale_files: localeGeneration.locales.length,
    locale_files_with_numerals: localeGeneration.locales.filter((l) => l.definition.numerals !== null).length,
    decimal_records: 0,
    generated_rust_files: 0
  };

  const decimalRecords = buildDecimalRecords();
  counts.decimal_records = decimalRecords.length;

  const artifacts = new Map();
  artifacts.set(`${FIXTURE_DIR}/calls.jsonl`, jsonlDocument(callRecords));
  artifacts.set(`${FIXTURE_DIR}/decimal.jsonl`, jsonlDocument(decimalRecords));
  artifacts.set(
    `${FIXTURE_DIR}/assertion-map.json`,
    canonicalJson(
      {
        schema: "d3-format-rust/assertion-map@1",
        schema_version: SCHEMA_VERSION,
        generator: "rust/tools/harvest.mjs",
        generator_version: GENERATOR_VERSION,
        counts: {
          static_assertion_sites: assertionSites.length,
          dynamic_assertion_executions: assertionSites.reduce((sum, s) => sum + s.executions, 0),
          boundary_calls: callRecords.length,
          generated: assertionSites.filter((s) => s.coverage === "generated").length,
          ported: assertionSites.filter((s) => s.coverage === "ported").length,
          divergence: assertionSites.filter((s) => s.coverage === "divergence").length
        },
        sites: assertionSites
      },
      2
    ) + "\n"
  );
  artifacts.set("rust/src/locales/generated.rs", localeGeneration.source);
  for (const [path, source] of testGeneration.files) artifacts.set(path, source);
  counts.generated_rust_files = 1 + testGeneration.files.size;

  const manifest = buildManifest({analysis, counts, artifacts, localeGeneration});
  artifacts.set(`${FIXTURE_DIR}/manifest.json`, canonicalJson(manifest, 2) + "\n");

  return {artifacts, counts, callRecords, assertionSites, manifest, localeGeneration, testGeneration};
}

function buildManifest({analysis, counts, artifacts, localeGeneration}) {
  const oracle = JSON.parse(readTextLf(`${FIXTURE_DIR}/oracle.json`));

  const sources = {};
  for (const path of [...sourceFiles(), ...testFiles(), ...localeFiles()]) sources[path] = sourceHash(path);

  const fixtures = {};
  const generatedRust = {};
  for (const [path, text] of [...artifacts.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))) {
    if (path.startsWith(`${FIXTURE_DIR}/`)) fixtures[path.slice(FIXTURE_DIR.length + 1)] = sha256Hex(text);
    else generatedRust[path] = sha256Hex(text);
  }
  // Untracked-by-the-generator fixtures still belong in the provenance record.
  fixtures["oracle.json"] = sourceHash(`${FIXTURE_DIR}/oracle.json`);
  const regressions = `${FIXTURE_DIR}/regressions.jsonl`;
  fixtures["regressions.jsonl"] = existsSync(repoPath(regressions)) ? sourceHash(regressions) : sha256Hex("");

  const orderedFixtures = {};
  for (const key of Object.keys(fixtures).sort()) orderedFixtures[key] = fixtures[key];

  return {
    schema: SCHEMA,
    schema_version: SCHEMA_VERSION,
    generator_version: GENERATOR_VERSION,
    d3_format_version: D3_FORMAT_VERSION,
    oracle,
    counts,
    generation: {
      commands: [
        "node rust/tools/harvest.mjs --write",
        "node rust/tools/gen-locales.mjs --write",
        "node rust/tools/gen-tests.mjs --write"
      ],
      verification: [
        "node rust/tools/check-oracle-metadata.mjs",
        "node rust/tools/check-coverage.mjs",
        "node rust/tools/differential.mjs --expect unimplemented"
      ],
      seeds: {
        // The decimal corpus is enumerated from the recorded inputs, not sampled,
        // so it has no seed. The differential's exploratory section does.
        decimal: null,
        differential_default: 1
      },
      notes: [
        "All generated bytes use LF endings and an ASCII-only encoding.",
        "Source SHA-256 values are computed over CRLF-normalized (LF) text, because " +
          "src/, test/ and locale/ are not covered by the rust/** eol=lf attribute and are " +
          "CRLF in a Windows working tree.",
        "f64 values are carried as uppercase 16-digit big-endian bit patterns, never as JSON numbers.",
        "Static assertion ids are relative/path:line:column with one-based line and column.",
        "distinct_specifier_inputs counts the raw specifier arguments passed to format and " +
          "formatPrefix; distinct_canonical_specifiers counts their normalized spellings."
      ]
    },
    assertion_id_scheme: "relative/path:line:column (one-based line and column, LF-normalized source)",
    sources,
    source_test_files: analysis.files.map((file) => file.path),
    locales: localeGeneration.locales.map((locale) => ({
      name: locale.name,
      path: locale.path,
      source_sha256: locale.sourceHash,
      definition_sha256: locale.hash
    })),
    fixtures: orderedFixtures,
    generated_rust: generatedRust
  };
}

// ---------------------------------------------------------------------------
// Writing and checking
// ---------------------------------------------------------------------------

export function writeFileLf(absolutePath, text) {
  mkdirSync(dirname(absolutePath), {recursive: true});
  const temporary = `${absolutePath}.tmp-${process.pid}`;
  writeFileSync(temporary, Buffer.from(text, "utf8"));
  // On Windows a replacing rename intermittently fails with EPERM while a scanner
  // still holds the freshly written file. Retrying is the standard remedy and keeps
  // the replace atomic; only after several failures do we fall back to a plain
  // overwrite, which is still correct, just not atomic.
  for (let attempt = 0; ; attempt++) {
    try {
      renameSync(temporary, absolutePath);
      return;
    } catch (error) {
      if (attempt >= 10 || (error.code !== "EPERM" && error.code !== "EACCES" && error.code !== "EBUSY")) {
        if (attempt < 10) throw error;
        writeFileSync(absolutePath, Buffer.from(text, "utf8"));
        rmSync(temporary, {force: true});
        return;
      }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20);
    }
  }
}

function readIfPresent(absolutePath) {
  try {
    return readFileSync(absolutePath, "utf8");
  } catch {
    return null;
  }
}

/**
 * Directories whose entire generated contents a tool owns, so a file that stops
 * being generated is removed rather than left behind as a stale artifact. Each
 * tool passes only the scope it actually regenerates.
 */
export const OWNED_GENERATED_TESTS = {dir: "rust/tests/generated", pattern: /\.rs$/};
export const OWNED_FIXTURES = {
  dir: FIXTURE_DIR,
  pattern: /^(calls|decimal)\.jsonl$|^(assertion-map|manifest)\.json$/
};

function unexpectedFiles(artifacts, owned) {
  const extra = [];
  for (const {dir, pattern} of owned) {
    const absolute = repoPath(dir);
    if (!existsSync(absolute)) continue;
    for (const name of readdirSync(absolute).sort()) {
      if (!pattern.test(name)) continue;
      if (!statSync(join(absolute, name)).isFile()) continue;
      const rel = `${dir}/${name}`;
      if (!artifacts.has(rel)) extra.push(rel);
    }
  }
  return extra;
}

export function writeArtifacts(artifacts, owned = []) {
  for (const rel of unexpectedFiles(artifacts, owned)) rmSync(repoPath(rel));
  for (const [rel, text] of [...artifacts.entries()].sort(([a], [b]) => (a < b ? -1 : 1))) {
    writeFileLf(repoPath(rel), text);
  }
  const regressions = repoPath(`${FIXTURE_DIR}/regressions.jsonl`);
  if (!existsSync(regressions)) writeFileLf(regressions, "");
}

/**
 * Regenerates into a fresh temporary directory and byte-compares against the
 * tracked artifacts. Nothing tracked is touched, whatever the outcome.
 */
export function checkArtifacts(artifacts, label, owned = []) {
  const scratch = mkdtempSync(join(tmpdir(), "d3-format-rust-"));
  const mismatches = [];
  try {
    for (const [rel, text] of [...artifacts.entries()].sort(([a], [b]) => (a < b ? -1 : 1))) {
      writeFileLf(join(scratch, ...rel.split("/")), text);
      const expected = Buffer.from(text, "utf8");
      const actualText = readIfPresent(repoPath(rel));
      if (actualText === null) {
        mismatches.push(`${rel}: missing (regenerate with --write)`);
        continue;
      }
      const actual = readFileSync(repoPath(rel));
      if (!actual.equals(expected)) {
        mismatches.push(
          `${rel}: ${actual.length} tracked bytes vs ${expected.length} regenerated bytes` +
            (actualText.includes("\r") ? " (tracked copy contains CR; check the .gitattributes rule)" : "") +
            `\n    tracked     sha256 ${sha256Hex(actual)}\n    regenerated sha256 ${sha256Hex(expected)}`
        );
      }
    }
    for (const rel of unexpectedFiles(artifacts, owned)) {
      mismatches.push(`${rel}: stale generated file (remove with --write)`);
    }
  } finally {
    rmSync(scratch, {recursive: true, force: true});
  }
  if (mismatches.length) {
    throw new Error(`${label}: ${mismatches.length} artifact(s) differ from a fresh generation:\n  ${mismatches.join("\n  ")}`);
  }
}

export function assertBaseline(counts) {
  const drift = [];
  for (const [key, expected] of Object.entries(BASELINE)) {
    const actual = counts[key];
    const same =
      typeof expected === "object"
        ? canonicalJson(expected) === canonicalJson(actual ?? null)
        : expected === actual;
    if (!same) drift.push(`  ${key}: baseline ${canonicalJson(expected)}, measured ${canonicalJson(actual ?? null)}`);
  }
  if (drift.length) {
    throw new Error(
      "measured counts drifted from the MIGRATION_TO_RUST.md section 0.3 baseline. " +
        "Investigate the cause; do not adjust the baseline to match:\n" +
        drift.join("\n")
    );
  }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

export function parseMode(argv, tool) {
  const flags = argv.slice(2);
  const unknown = flags.filter((flag) => flag !== "--check" && flag !== "--write");
  if (unknown.length) {
    throw new Error(`${tool}: unknown argument(s) ${unknown.join(" ")}; expected --check or --write`);
  }
  if (flags.includes("--check") && flags.includes("--write")) {
    throw new Error(`${tool}: --check and --write are mutually exclusive`);
  }
  return flags.includes("--write") ? "write" : "check";
}

export function isMain(url) {
  const entry = process.argv[1];
  if (!entry) return false;
  return resolve(fileURLToPath(url)) === resolve(entry);
}

async function main() {
  const mode = parseMode(process.argv, "harvest.mjs");
  const {artifacts, counts} = await generateAll();
  assertBaseline(counts);

  const owned = [OWNED_GENERATED_TESTS, OWNED_FIXTURES];
  if (mode === "write") {
    writeArtifacts(artifacts, owned);
    console.log(`harvest: wrote ${artifacts.size} artifact(s)`);
  } else {
    checkArtifacts(artifacts, "harvest --check", owned);
    console.log(`harvest: ${artifacts.size} artifact(s) byte-identical to a fresh generation`);
  }

  console.log(
    `harvest: ${counts.test_files} test files, ${counts.test_blocks} test blocks, ` +
      `${counts.static_assertion_sites} static sites, ${counts.dynamic_assertion_executions} dynamic executions, ` +
      `${counts.boundary_calls} boundary calls, ${counts.distinct_specifier_inputs} specifiers, ` +
      `${counts.distinct_locale_definitions} locale definitions`
  );
}

if (isMain(import.meta.url)) {
  main().catch((error) => {
    console.error(`harvest: ${error.message}`);
    process.exitCode = 1;
  });
}
