import formatDigits from "./formatDigits";

export default function(x, p) {
  var d = formatDigits(x, p);
  if (!d) return x + "";
  var sign = d[0],
      coefficient = d[1],
      exponent = d[2];
  return [
    sign ? "-" : "",
    exponent < 0 ? "0" : coefficient.length > exponent + 1 ? coefficient.slice(0, exponent + 1) : coefficient + new Array(exponent - coefficient.length + 2).join("0"),
    exponent < 0 ? "." + new Array(-exponent).join("0") + coefficient : coefficient.length > exponent + 1 ? "." + coefficient.slice(exponent + 1) : ""
  ];
};
