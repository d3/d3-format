import formatDefault from "./formatDefault";
import formatPrefixAuto from "./formatPrefixAuto";
import formatRounded from "./formatRounded";

export default {
  "": formatDefault,
  "%": function(x, p) { return (x * 100).toFixed(p); },
  "b": function(x) { return x.toString(2); },
  "c": function(x) { return String.fromCharCode(x); },
  "d": function(x) { return x.toString(10); },
  "e": function(x, p) { return x.toExponential(p); },
  "f": function(x, p) { return x.toFixed(p); },
  "g": function(x, p) { return x.toPrecision(p); },
  "o": function(x) { return x.toString(8); },
  "p": function(x, p) { return formatRounded(x * 100, p); },
  "r": formatRounded,
  "s": formatPrefixAuto,
  "X": function(x) { return x.toString(16).toUpperCase(); },
  "x": function(x) { return x.toString(16); }
};
