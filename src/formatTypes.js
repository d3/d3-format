import formatBinaryPrefixAuto from "./formatBinaryPrefixAuto.js";
import formatPrefixAuto, { createFormatCurrencyPrefixAutoForLocale } from "./formatPrefixAuto.js";
import formatRounded from "./formatRounded.js";

export default {
  "%": function(x, p) { return (x * 100).toFixed(p); },
  "B": formatBinaryPrefixAuto,
  "b": function(x) { return Math.round(x).toString(2); },
  "c": function(x) { return x + ""; },
  "d": function(x) { return Math.round(x).toString(10); },
  "e": function(x, p) { return x.toExponential(p); },
  "f": function(x, p) { return x.toFixed(p); },
  "g": function(x, p) { return x.toPrecision(p); },
  "K": createFormatCurrencyPrefixAutoForLocale, // depends of the current locale
  "o": function(x) { return Math.round(x).toString(8); },
  "p": function(x, p) { return formatRounded(x * 100, p); },
  "r": formatRounded,
  "s": formatPrefixAuto,
  "X": function(x) { return Math.round(x).toString(16).toUpperCase(); },
  "x": function(x) { return Math.round(x).toString(16); }
};
