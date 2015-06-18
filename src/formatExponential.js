export default function(x, p) {
  x = (+x).toExponential(p);
  var i = x.indexOf("e");
  if (i < 0) i = x[0] === "-" ? 1 : 0; // blech
  return [
    x[0] === "-" ? "-" : "",
    x.slice(x[0] === "-" ? 1 : 0, i),
    x.slice(i)
  ];
};
