import {formatDecimalParts} from "./formatDecimal.js";
import exponent from "./exponent.js";

export default function(x, p, scale) {
  if (!isFinite(x)) return x + "";

  var zero = "0" + (p ? "." + new Array(p + 1).join("0") : "");
  if (x === 0) return zero;

  var precision = exponent(x) - scale + p + 1,
      carry = precision === 0 && formatDecimalParts(x)[0][0] >= "5",
      coefficient,
      exponentScaled,
      integer,
      fraction,
      i,
      n;

  if (precision <= 0) {
    return carry ? (p ? "0." + new Array(p).join("0") + "1" : "1") : zero;
  }

  if (precision > 100) return (x * Math.pow(10, -scale)).toFixed(p);

  x = formatDecimalParts(x, precision);
  coefficient = x[0];
  exponentScaled = x[1] - scale;
  i = exponentScaled + 1;
  n = coefficient.length;

  if (i > 0) {
    integer = i >= n ? coefficient + new Array(i - n + 1).join("0") : coefficient.slice(0, i);
    fraction = coefficient.slice(i);
  } else {
    integer = "0";
    fraction = new Array(1 - i).join("0") + coefficient;
  }

  if (fraction.length < p) fraction += new Array(p - fraction.length + 1).join("0");
  return integer + (p ? "." + fraction : "");
}
