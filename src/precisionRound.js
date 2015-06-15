import exponent from "./exponent";

export default function(step, max) {
  return Math.max(0, exponent(Math.abs(max)) - exponent(Math.abs(step))) + 1;
};
