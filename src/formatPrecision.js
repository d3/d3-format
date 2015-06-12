export default function(x, p) {
  return p - (x ? Math.ceil(Math.log(x) / Math.LN10) : 1);
};
