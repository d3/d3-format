import formatDecimal from "./formatDecimal";

export var prefixExponent;

export default function(x, p) {
  var d = formatDecimal(x, p);
  if (!d) return x + "";
  var coefficient = d[0],
      exponent = d[1],
      i = exponent + 1;
  if (i >= 24) i -= 24;
  else if (i <= -24) coefficient = new Array(-22 - i).join("0") + coefficient.slice(0, i + 23), i = 1;
  else i %= 3;
  if (!i) i = 3;
  else if (i < 0) i += 3;
  prefixExponent = Math.max(-24, Math.min(24, Math.floor(exponent / 3) * 3));
  return coefficient.slice(0, i)
      + (coefficient.length > i
      ? "." + coefficient.slice(i)
      : new Array(i - coefficient.length + 1).join("0"));
};
