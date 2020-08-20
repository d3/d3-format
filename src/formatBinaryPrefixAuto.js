export var binaryPrefixExponent;

export default function(x, p) {
  var binaryExponent = 0;
  if (x === Infinity) return binaryPrefixExponent = 0, x;

  while (Math.round(x) >= 1024 && binaryExponent < 80) {
    binaryExponent += 10;
    x /= 1024;
  }

  if (p <= 3 && Math.round(x) >= 1000) {
    // Unlike SI prefixes, integers can take three digits.
    binaryExponent += 10;
    x /= 1024;
  }

  binaryPrefixExponent = Math.max(0, Math.min(8, Math.floor(binaryExponent / 10))) * 10;
  var i = binaryExponent - binaryPrefixExponent + 1,
      coefficient = x * i,
      split = ('' + coefficient).split('.'),
      integer = split[0],
      fraction = split[1] || '',
      n = (integer + fraction).length;

  if (n === p) return coefficient;

  if (n > p) {
    var fractionLength = Math.max(0, p - integer.length);

    while (+coefficient.toFixed(fractionLength) === 0) {
      fractionLength += 1;
    }

    coefficient = coefficient.toFixed(fractionLength);
  } else {
    coefficient = integer + '.' + fraction;

    while (n < p) {
      coefficient += '0';
      n += 1;
    }
  }

  return coefficient;
}
