// A recording stand-in for `src/index.js`. It re-exports the real d3-format API
// unchanged in behaviour and records every public-boundary call, attributing it to
// the test file, test block, static assertion site and dynamic execution ordinal
// that the vitest shim currently has active.
//
// `format` and `formatPrefix` stay reassignable `let` exports so that
// defaultLocale-test.js's live-binding identity assertions still hold.
import * as real from "../../src/index.js";
import {state} from "./vitest-shim.mjs";

/** Every recorded boundary call, in execution order. */
export const rows = [];

/** Marks an output that is a `FormatSpecifier` rather than a string or a number. */
export class SpecifierText {
  constructor(text) {
    this.text = text;
  }
}

// The definition d3 installs in src/defaultLocale.js at module load. Note that it
// carries no `decimal` key; defaultLocale-test.js later replaces it with one that
// does, so the harvester must track the live value rather than assume en-US.
const BUILTIN_DEFAULT = {thousands: ",", grouping: [3], currency: ["$", ""]};

let currentDefault = snapshot(BUILTIN_DEFAULT);

function snapshot(definition) {
  if (definition === null || typeof definition !== "object") {
    throw new TypeError(`locale definition must be an object, got ${String(definition)}`);
  }
  const copy = {};
  for (const key of Object.keys(definition)) {
    const value = definition[key];
    copy[key] = Array.isArray(value) ? value.slice() : value;
  }
  return copy;
}

function push(row) {
  rows.push({
    file: state.file,
    testIndex: state.testIndex,
    title: state.title,
    site: state.site,
    ordinal: state.ordinal,
    ...row
  });
}

function specifierInput(specifier) {
  return typeof specifier === "string"
    ? {kind: "text", text: specifier}
    : {kind: "specifier", text: String(specifier)};
}

/** The normalized specifier d3 uses inside `formatPrefix`, which forces type "f". */
function canonicalPrefix(specifier) {
  const parsed = real.formatSpecifier(String(specifier));
  parsed.type = "f";
  return String(parsed);
}

function wrapFormat(factory, definition) {
  return (specifier, ...rest) => {
    const compiled = factory(specifier, ...rest);
    const locale = snapshot(definition);
    const input = specifierInput(specifier);
    // d3 gives every compiled formatter a `toString` returning its normalized
    // specifier; that is the canonical spelling the fixtures record.
    const canonical = String(compiled);
    const wrapped = (value) => {
      const output = compiled(value);
      push({
        operation: "format",
        localeDefinition: locale,
        specifierInput: input,
        specifier: canonical,
        reference: null,
        args: [value],
        output
      });
      return output;
    };
    wrapped.toString = () => canonical;
    return wrapped;
  };
}

function wrapFormatPrefix(factory, definition) {
  return (specifier, reference) => {
    const compiled = factory(specifier, reference);
    const locale = snapshot(definition);
    const input = specifierInput(specifier);
    const canonical = canonicalPrefix(specifier);
    return (value) => {
      const output = compiled(value);
      push({
        operation: "formatPrefix",
        localeDefinition: locale,
        specifierInput: input,
        specifier: canonical,
        reference: [reference],
        args: [value],
        output
      });
      return output;
    };
  };
}

let format = wrapFormat(real.format, currentDefault);
let formatPrefix = wrapFormatPrefix(real.formatPrefix, currentDefault);
export {format, formatPrefix};

export function formatLocale(definition) {
  const locale = real.formatLocale(definition);
  return {
    format: wrapFormat(locale.format, definition),
    formatPrefix: wrapFormatPrefix(locale.formatPrefix, definition)
  };
}

export function formatDefaultLocale(definition) {
  const locale = real.formatDefaultLocale(definition);
  currentDefault = snapshot(definition);
  format = wrapFormat(locale.format, currentDefault);
  formatPrefix = wrapFormatPrefix(locale.formatPrefix, currentDefault);
  return {format, formatPrefix};
}

export function formatSpecifier(specifier) {
  const parsed = real.formatSpecifier(specifier);
  push({
    operation: "formatSpecifier",
    localeDefinition: null,
    specifierInput: specifierInput(specifier),
    specifier: String(parsed),
    reference: null,
    args: [specifier],
    output: new SpecifierText(String(parsed))
  });
  return parsed;
}
formatSpecifier.prototype = real.FormatSpecifier.prototype; // instanceof

export const FormatSpecifier = real.FormatSpecifier;

function wrapPrecision(operation) {
  return (...args) => {
    const output = real[operation](...args);
    push({
      operation,
      localeDefinition: null,
      specifierInput: null,
      specifier: null,
      reference: null,
      args,
      output
    });
    return output;
  };
}

export const precisionFixed = wrapPrecision("precisionFixed");
export const precisionPrefix = wrapPrecision("precisionPrefix");
export const precisionRound = wrapPrecision("precisionRound");
