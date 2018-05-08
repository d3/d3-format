import formatTrim from "./formatTrim";

export default function(x, p) {
  return formatTrim(x.toPrecision(p));
}
