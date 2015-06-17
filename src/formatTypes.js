import formatDefault from "./formatDefault";
import formatPrefixAuto from "./formatPrefixAuto";
import formatRounded from "./formatRounded";

var formatHexadecimal = formatInteger(16);

export default {
  "": formatDefault,
  "%": function(x, p) { return (x * 100).toFixed(p); },
  "b": formatInteger(2),
  "c": function(x) { var i = Math.floor(x); return +x === i ? ["", String.fromCharCode(x), ""] : null; },
  "d": formatDecimal,
  "e": function(x, p) { return x.toExponential(p); },
  "f": function(x, p) { return x.toFixed(p); },
  "g": function(x, p) { return x.toPrecision(p); },
  "o": formatInteger(8),
  "p": function(x, p) { return formatRounded(x * 100, p); },
  "r": formatRounded,
  "s": formatPrefixAuto,
  "X": function(x) { x = formatHexadecimal(x); return x && (x[1] = x[1].toUpperCase(), x); },
  "x": formatInteger(16),
};

function formatInteger(b) {
  return function(x) {
    var i = Math.floor(x);
    if (+x !== i) return null;
    x = i.toString(b);
    return [
      x[0] === "-" ? "-" : "",
      x.slice(x[0] === "-" ? 1 : 0),
      ""
    ];
  };
}

function formatDecimal(x) {
  if (Math.floor(x) !== (x = +x)) return null;

  x = x.toFixed(0);

  var i = x.indexOf("e");
  return i > 0 ? [
    x[0] === "-" ? "-" : "",
    x.slice(x[0] === "-" ? 1 : 0, i),
    x.slice(i)
  ] : [
    x[0] === "-" ? "-" : "",
    x.slice(x[0] === "-" ? 1 : 0),
    ""
  ];
}
