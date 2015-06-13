// Note: assumes that x is positive.
export default function(x, p) {
  x = p == null ? x.toExponential() : x.toExponential(p - 1);
  if ((i = x.indexOf("e")) < 0) return null; // NaN, ±Infinity
  var i,
      coefficient = x.slice(0, i),
      exponent = +x.slice(i + 1),
      j = coefficient.indexOf("."),
      integer = j < 0 ? coefficient : coefficient.slice(0, j),
      fraction = j < 0 ? "" : coefficient.slice(j + 1);
  return {
    coefficient: integer + fraction,
    exponent: exponent + integer.length
  };
};
