import {formatDecimalParts} from "./formatDecimal.js";

export var prefixExponent;

export default function(x, p) {
  var d = formatDecimalParts(x, p);
  if (!d) return prefixExponent = undefined, x.toPrecision(p);
  var coefficient = d[0],
      exponent = d[1],
      i = exponent - (prefixExponent = Math.max(-8, Math.min(8, Math.floor(exponent / 3))) * 3) + 1;

  // Below the smallest prefix (yocto), the value is shown as “0.…”, which
  // leaves fewer significant digits; re-round to that reduced precision. This
  // rounding can carry into a higher power of ten (e.g. 9.99 → 10), so derive
  // the digit position from the re-rounded exponent rather than the original.
  if (i <= 0) {
    d = formatDecimalParts(x, Math.max(0, p + i - 1));
    coefficient = d[0];
    i = d[1] - prefixExponent + 1;
  }

  var n = coefficient.length;
  return i === n ? coefficient
      : i > n ? coefficient + new Array(i - n + 1).join("0")
      : i > 0 ? coefficient.slice(0, i) + "." + coefficient.slice(i)
      : "0." + new Array(1 - i).join("0") + coefficient; // less than 1y!
}
