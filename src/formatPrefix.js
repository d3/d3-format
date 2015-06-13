import formatPrecision from "./formatPrecision";
import round from "./round";

var prefixes = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"].map(function(d, i) {
  var k = +("1e" + (8 - i) * 3);
  return {
    scale: function(d) { return d * k; },
    symbol: d
  };
});

export default function(value, precision) {
  var i = 0;
  if (value = +value) {
    if (value < 0) value *= -1;
    if (precision) value = round(value, formatPrecision(value, precision));
    i = Math.max(-24, Math.min(24, Math.floor((1e-12 + Math.log(value)) / (Math.LN10 * 3)) * 3));
  }
  return prefixes[8 + i / 3];
};
