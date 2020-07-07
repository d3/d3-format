import formatDecimal from "./formatDecimal.js";

export var prefixExponent;

function formatSignificantDigitsForPrefixes(x, p, minPrefixOrder, maxPrefixOrder) {
  var d = formatDecimal(x, p);
  if (!d) return x + "";
  var coefficient = d[0],
      exponent = d[1],
      i = exponent - (prefixExponent = Math.max(minPrefixOrder, Math.min(maxPrefixOrder, Math.floor(exponent / 3))) * 3) + 1,
      n = coefficient.length;
  return i === n ? coefficient
      : i > n ? coefficient + new Array(i - n + 1).join("0")
      : i > 0 ? coefficient.slice(0, i) + "." + coefficient.slice(i)
      : "0." + new Array(1 - i).join("0") + formatDecimal(x, Math.max(0, p + i - 1))[0]; // less than the smallest prefix
}

export function createFormatCurrencyPrefixAutoForLocale(currencyAbbreviations) {
  return function formatCurrencyPrefixAuto(x, p) {
    return formatSignificantDigitsForPrefixes(x, p, 0, currencyAbbreviations.length - 1);
  }
}

export default function(x, p) {
  return formatSignificantDigitsForPrefixes(x, p, -8, 8);
}
