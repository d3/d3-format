import trim from "./trim";

export default function(x, p) {
  return trim(x.toPrecision(p));
}
