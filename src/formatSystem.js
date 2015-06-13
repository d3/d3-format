import btod from "./btod";

var symbols = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"];

export default function(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent % 3;
  if (!i) i = 3;
  else if (i < 0) i += 3;
  return d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"))
      + symbols[8 + Math.floor((d.exponent - 1) / 3)]; // TODO this screws up grouping, but only if grouping < 3
};
