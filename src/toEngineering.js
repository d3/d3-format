var symbols = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"];

export default function(x, p) {
  var i = (x = p == null ? x.toExponential() : x.toExponential(p - 1)).indexOf("e"); // TODO ternary?
  if (i < 0) return x;

  var coefficient = x.slice(0, i),
      exponent = +x.slice(i + 1),
      offset = exponent % 3;

  if (offset) {
    var j = coefficient.indexOf("."),
        decimal = j < 0 ? coefficient : coefficient.slice(0, j),
        fractional = j < 0 ? "" : coefficient.slice(j + 1);
    if (offset < 0) offset += 3;
    coefficient = decimal + (offset >= fractional.length
        ? fractional + new Array(offset - fractional.length + 1).join("0")
        : fractional.slice(0, offset) + "." + fractional.slice(offset));
  }

  return coefficient + symbols[8 + Math.floor(exponent / 3)];
};
