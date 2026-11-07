import exponent from "./exponent.js";

export default function(step, max) {
  step = Math.abs(step), max = Math.abs(max) - step;
  return Math.max(0, exponent(max === 0 ? step : max) - exponent(step)) + 1;
}
