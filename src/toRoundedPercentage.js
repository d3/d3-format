export default function(x, p) {
  var i = (x = p == null ? x.toExponential() : x.toExponential(p - 1)).indexOf("e");
  if (i < 0) return x;

  var coefficient = x.slice(0, i),
      exponent = +x.slice(i + 1),
      offset = exponent + 2,
      j = coefficient.indexOf("."),
      decimal = j < 0 ? coefficient : coefficient.slice(0, j),
      fractional = j < 0 ? "" : coefficient.slice(j + 1);

  if (offset < 0) {
    coefficient = "0." + decimal + fractional;
  } else if (offset > 0) {
    coefficient = decimal + (offset >= fractional.length
        ? fractional + new Array(offset - fractional.length + 1).join("0")
        : fractional.slice(0, offset) + "." + fractional.slice(offset));
  }

  return coefficient + "%";
};
