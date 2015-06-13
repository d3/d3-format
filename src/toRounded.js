export default function(x, p) {
  var i = (x = x.toPrecision(p)).indexOf("e");
  if (i < 0) return x;

  // Convert exponential notation to human form. For example,
  // with a negative exponent, 1.2e-2 becomes 0.012;
  // with a positive exponent, 1.2e2 becomes 120.
  var coefficient = x.slice(0, i),
      exponent = +x.slice(i + 1),
      j = coefficient.indexOf("."),
      decimal = j < 0 ? coefficient : coefficient.slice(0, j),
      fractional = j < 0 ? "" : coefficient.slice(j + 1);

  return exponent < 0
      ? "0." + new Array(1 - exponent).join("0") + decimal + fractional
      : decimal + fractional + new Array(exponent - fractional.length + 1).join("0");
};
