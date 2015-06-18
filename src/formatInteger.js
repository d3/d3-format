export default function(b) {
  return function(x) {
    var i = Math.floor(x);
    if (+x !== i) return null;
    x = i.toString(b);
    return [
      x[0] === "-" ? "-" : "",
      x.slice(x[0] === "-" ? 1 : 0),
      ""
    ];
  };
};
