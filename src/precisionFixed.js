export default function(step) {
  return Math.max(0, -Math.floor(Math.log(Math.abs(step)) / Math.LN10));
};
