import exponent from "./exponent.js";

export default function(step, value) {
  var e = +value === 0 ? 0 : Math.max(-8, Math.min(8, Math.floor(exponent(value) / 3))) * 3; // a zero value has 0 exponent
  return Math.max(0, e - exponent(Math.abs(step)));
}
