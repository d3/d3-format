export default function(x, p) {
  x = x.toPrecision(p);

  out: for (var n = x.length, i = 1, i0 = -1, i1; i < n; ++i) {
    switch (x[i]) {
      case ".": i0 = i1 = i; break;
      case "0": if (i0 === 0) i0 = i; i1 = i; break;
      case "e": break out;
      default: if (i0 > 0) i0 = 0; break;
    }
  }

  if (i0 > 0) x = x.slice(0, i0) + x.slice(i1 + 1);

  return [
    x[0] === "-" ? x[0] : "",
    x.slice(x[0] === "-" ? 1 : 0, i),
    x.slice(i)
  ];
};
