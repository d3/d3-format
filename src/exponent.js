export default function(value) {
  return Math.floor(Math.log(value) / Math.LN10 + 1e-12);
};
