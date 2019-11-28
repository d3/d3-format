import exponent from "./exponent.js";

export function createPrecisionPrefix(minimumPrefixOrder, maximumPrefixOrder) {
  return function (step, value) {
    return Math.max(0, Math.max(minimumPrefixOrder, Math.min(maximumPrefixOrder, Math.floor(exponent(value) / 3))) * 3 - exponent(Math.abs(step)));
  }
}

export default createPrecisionPrefix(-8, 8);
