import formatDecimal from "./formatDecimal";

export default function(x, p) {
  var d = formatDecimal(x, p);
  if (!d) return x + "";
  var coefficient = d[0],
      exponent = d[1],
      i = exponent + 1;
  return i <= 0
      ? "0." + new Array(1 - i).join("0") + coefficient
      : coefficient.slice(0, i)
      + (coefficient.length > i
      ? "." + coefficient.slice(i)
      : new Array(i - coefficient.length + 1).join("0"));
};
