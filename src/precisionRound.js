export default function(step, max) {
  return Math.max(0, Math.floor(Math.log(Math.abs(max)) / Math.LN10) - Math.floor(Math.log(Math.abs(step)) / Math.LN10)) + 1;
};
