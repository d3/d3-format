import btod from "./btod";

// TODO This duplicates toSystem in localeFormat.
var prefixes = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"].map(function(d, i) {
  var k = +("1e" + (8 - i) * 3);
  return {
    scale: function(d) { return d * k; },
    symbol: d
  };
});

export default function(x, p) {
  var i = 8;
  if (x = +x) {
    if (x < 0) x *= -1;
    i += Math.max(-8, Math.min(8, Math.floor((btod(x, p).exponent - 1) / 3)));
  }
  return prefixes[i];
};
