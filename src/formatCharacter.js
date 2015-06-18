export default function(x) {
  var i = Math.floor(x);
  if (+x !== i) return null;
  return ["", String.fromCharCode(x), ""];
};
