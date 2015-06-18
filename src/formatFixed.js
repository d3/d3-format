export default function(x, p) {
  x = (+x).toFixed(p);
  var i = x.indexOf(".");
  if (i < 0) i = x.length;
  return [
    x[0] === "-" ? "-" : "",
    x.slice(x[0] === "-" ? 1 : 0, i),
    x.slice(i)
  ];
};
