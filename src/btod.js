// Note: assumes that x is positive.
export default function(x, p) {
  var s = p == null ? x.toExponential() : x.toExponential(p - 1),
      i = s.indexOf("e");
  if (i < 0) return null; // NaN, ±Infinity
  var coefficient = s.slice(0, i),
      exponent = +s.slice(i + 1),
      j = coefficient.indexOf("."),
      decimal = j < 0 ? coefficient : coefficient.slice(0, j),
      fractional = j < 0 ? "" : coefficient.slice(j + 1);
  return {
    coefficient: decimal + fractional,
    exponent: exponent + decimal.length
  };
};
