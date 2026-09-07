// A minimal stand-in for the parts of `vitest` that the d3-format suite imports,
// so the real test files can run under bare Node with no test runner in the way.
//
// It also owns the assertion-site registry described in MIGRATION_TO_RUST.md
// section 3.2. `harvest.mjs` rewrites every `assert.<method>(actual, ...)` call in
// the oracle test files so that `__site(id)` runs immediately before the actual
// argument is evaluated. Everything the argument does -- including library calls
// made inside an `assert.throws` callback, which runs later -- is therefore
// attributed to the correct static site.

/** Assertion methods the oracle suite actually uses. Anything else is a hard error. */
const SUPPORTED = ["strictEqual", "isNaN", "throws"];

export const state = {
  /** Repository-relative POSIX path of the test file currently being imported. */
  file: null,
  /** Zero-based index of the `test(...)` block currently running, within its file. */
  testIndex: -1,
  /** Title of the `test(...)` block currently running. */
  title: null,
  /** Static assertion id (`relative/path:line:column`) currently being evaluated. */
  site: null,
  /** Zero-based execution ordinal of the current site. */
  ordinal: -1,
  /** Number of `test(...)` blocks seen. */
  tests: 0,
  /** Number of `__site()` calls, i.e. dynamic assertion executions. */
  executions: 0,
  /** Number of completed assertion records; an independent count of the same thing. */
  records: 0,
  /** Static assertion id -> execution count. */
  executionsById: new Map(),
  /** `${file}\u0000${testIndex}` -> title, for cross-checking against the AST. */
  titles: new Map(),
  /** Failures, each with full attribution. Any entry makes the harvest fail. */
  failures: [],
  /** Titles passed to `test.skip`. Any entry makes the harvest fail. */
  skipped: [],
  /** Promises returned by async test bodies, awaited by the harvester per file. */
  pending: []
};

/** Called by the harvester before importing each test file. */
export function beginFile(relativePath) {
  state.file = relativePath;
  state.testIndex = -1;
  state.title = null;
  state.site = null;
  state.ordinal = -1;
}

/**
 * Marks the static assertion site whose actual argument is about to be evaluated.
 * Injected by the harvester's loader transform; returns `0` so it composes inside
 * a sequence expression.
 */
export function __site(id) {
  if (state.file === null) throw new Error(`assertion site ${id} evaluated outside a test file`);
  const previous = state.executionsById.get(id) ?? 0;
  state.executionsById.set(id, previous + 1);
  state.site = id;
  state.ordinal = previous;
  state.executions++;
  return 0;
}

function describe(value) {
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number") return Object.is(value, -0) ? "-0" : String(value);
  if (typeof value === "function") return `[function ${value.name || "anonymous"}]`;
  if (typeof value === "object" && value !== null) {
    try {
      return JSON.stringify(value);
    } catch {
      return Object.prototype.toString.call(value);
    }
  }
  return String(value);
}

function record(ok, detail) {
  state.records++;
  if (!ok) {
    state.failures.push({
      file: state.file,
      testIndex: state.testIndex,
      title: state.title,
      site: state.site,
      ordinal: state.ordinal,
      detail
    });
  }
  // Clearing the site as the assertion completes is what keeps a library call made
  // in a test block's setup code -- `const s = formatSpecifier("")`, say -- from
  // being credited to the previous assertion. Such calls belong to the block, not
  // to any static site, and the harvester records them that way.
  state.site = null;
  state.ordinal = -1;
}

const methods = {
  strictEqual(actual, expected, message) {
    record(
      Object.is(actual, expected),
      message ?? `expected ${describe(expected)}, got ${describe(actual)}`
    );
  },
  isNaN(value, message) {
    record(Number.isNaN(value), message ?? `expected NaN, got ${describe(value)}`);
  },
  throws(fn, matcher, message) {
    let thrown = null;
    let threw = false;
    try {
      fn();
    } catch (error) {
      threw = true;
      thrown = error;
    }
    let ok = threw;
    if (ok && matcher instanceof RegExp) ok = matcher.test(String(thrown && thrown.message));
    else if (ok && typeof matcher === "function") ok = thrown instanceof matcher;
    record(ok, message ?? `expected a throw matching ${matcher}, got ${describe(thrown)}`);
  }
};

// Reading any assertion helper the oracle suite does not currently use is a hard
// error rather than `undefined`, so an upstream test change cannot be silently
// under-recorded.
export const assert = new Proxy(methods, {
  get(target, property) {
    if (typeof property === "symbol" || property in target) return target[property];
    throw new Error(
      `vitest-shim: assert.${String(property)} is not implemented; ` +
        `the harvester only models ${SUPPORTED.join(", ")}`
    );
  }
});

export function test(name, fn) {
  state.testIndex++;
  state.title = name;
  state.tests++;
  state.titles.set(`${state.file}\u0000${state.testIndex}`, name);
  let result;
  try {
    result = fn();
  } catch (error) {
    record(false, `test body threw: ${error && error.stack ? error.stack : error}`);
    return;
  }
  if (result && typeof result.then === "function") state.pending.push(result);
}

test.skip = (name) => {
  state.skipped.push(name);
};
test.only = test;
test.todo = test.skip;

export const it = test;
export const describeBlock = undefined;

export default {assert, test, it};
