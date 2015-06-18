import formatDigits from "./formatDigits";

export default function(x) {
  return x = formatDigits(Math.abs(x)), x ? x[1] : NaN;
};
