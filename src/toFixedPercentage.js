export default function(x, p) {
  console.warn(x, p, x.toFixed((p || 0) + 2));
  x = x.toFixed((p || 0) + 2);

  var i = x.indexOf("."),
      decimal = x.slice(0, i),
      fractional = x.slice(i + 1);

  console.warn(
    JSON.stringify(decimal),
    JSON.stringify(fractional)
  );

  return (decimal.length <= 2
      ? fractional.slice(2 - decimal.length)
      : decimal.slice(2) + "." + fractional) + "%";

  // return (decimal + (2 >= fractional.length
  //     ? fractional + new Array(2 - fractional.length + 1).join("0")
  //     : fractional.slice(0, 2) + "." + fractional.slice(2))) + "%";

  // return x + "%";
  // var i = (x = p == null ? x.toExponential() : x.toExponential(p - 1)).indexOf("e");
  // if (i < 0) return x;

  // var coefficient = x.slice(0, i),
  //     exponent = +x.slice(i + 1),
  //     offset = exponent + 2,
  //     j = coefficient.indexOf("."),
  //     decimal = j < 0 ? coefficient : coefficient.slice(0, j),
  //     fractional = j < 0 ? "" : coefficient.slice(j + 1);

  // if (offset < 0) {
  //   coefficient = "0." + decimal + fractional;
  // } else if (offset > 0) {
  //   coefficient = decimal + (offset >= fractional.length
  //       ? fractional + new Array(offset - fractional.length + 1).join("0")
  //       : fractional.slice(0, offset) + "." + fractional.slice(offset));
  // }

  // return coefficient + "%";
};
