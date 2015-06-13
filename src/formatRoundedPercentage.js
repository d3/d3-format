import btod from "./btod";

export default function(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent + 2;
  return i <= 0
      ? "0." + new Array(1 - i).join("0") + d.coefficient
      : d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"));
};
