import btod from "./btod";

export var exponent;

export default function(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent % 3;
  if (!i) i = 3;
  else if (i < 0) i += 3;
  exponent = d.exponent - 1;
  return d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"));
};
