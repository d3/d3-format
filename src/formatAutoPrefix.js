import btod from "./btod";

export var exponent;

export default function(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent;
  if (i >= 24) i -= 24;
  else if (i <= -24) d.coefficient = new Array(-22 - i).join("0") + d.coefficient.slice(0, i + 23), i = 1;
  else i %= 3;
  if (!i) i = 3;
  else if (i < 0) i += 3;
  exponent = Math.max(-24, Math.min(24, Math.floor((d.exponent - 1) / 3) * 3));
  return d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"));
};
