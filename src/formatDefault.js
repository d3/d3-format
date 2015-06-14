export default function(x, p) {
  return x.toPrecision(p).replace(/(?:\.|(\.\d+?))0+(e|$)/, "$1$2");
};
