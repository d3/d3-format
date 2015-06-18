// Computes the sign, decimal coefficient and exponent of the specified number x
// with significant digits p, where p is in [1, 21] or undefined. For example,
// formatDecimal(1.23) returns [0, "123", 0].
export default function(x, p) {
  if ((i = (x = x.toExponential(p && p - 1)).indexOf("e")) < 0) return null; // NaN, ±Infinity
  var i,
      sign = x[0] === "-" ? 1 : 0,
      coefficient = x.slice(sign, i),
      exponent = +x.slice(i + 1);

  // The string returned by toExponential either has the form \d\.\d+e[-+]\d+
  // (e.g., 1.2e+3) or the form \de[-+]\d+ (e.g., 1e+3).
  return [
    sign,
    coefficient.length > 1 ? coefficient[0] + coefficient.slice(2) : coefficient,
    exponent
  ];
};
