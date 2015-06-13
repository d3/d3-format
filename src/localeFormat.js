import btod from "./btod";

// [[fill]align][sign][symbol][0][width][,][.precision][type]
var formatRe = /(?:([^{])?([<>=^]))?([+\- ])?([$#])?(0)?(\d+)?(,)?(\.-?\d+)?([a-z%])?/i,
    systemSymbols = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"];

export default function(x, p) {
  var i = (x = p == null ? x.toExponential() : x.toExponential(p - 1)).indexOf("e"); // TODO ternary?
  if (i < 0) return x;

  var coefficient = x.slice(0, i),
      exponent = +x.slice(i + 1),
      offset = exponent % 3;

  if (offset) {
    var j = coefficient.indexOf("."),
        decimal = j < 0 ? coefficient : coefficient.slice(0, j),
        fractional = j < 0 ? "" : coefficient.slice(j + 1);
    if (offset < 0) offset += 3;
    coefficient = decimal + (offset >= fractional.length
        ? fractional + new Array(offset - fractional.length + 1).join("0")
        : fractional.slice(0, offset) + "." + fractional.slice(offset));
  }

  return coefficient + symbols[8 + Math.floor(exponent / 3)];
};

var formatTypes = {
  "b": function(x) { return x.toString(2); },
  "c": function(x) { return String.fromCharCode(x); },
  "o": function(x) { return x.toString(8); },
  "x": function(x) { return x.toString(16); },
  "X": function(x) { return x.toString(16).toUpperCase(); },
  "g": function(x, p) { return x.toPrecision(p); },
  "e": function(x, p) { return x.toExponential(p); },
  "f": function(x, p) { return x.toFixed(p); },
  "%": function(x, p) { return (x * 100).toFixed(p); },
  "p": toRoundedPercentage,
  "r": toRounded,
  "s": toSystem
};

function toRoundedPercentage(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent + 2;
  return i <= 0
      ? "0." + new Array(1 - i).join("0") + d.coefficient
      : d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"));
}

function toRounded(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent;
  return i <= 0
      ? "0." + new Array(1 - i).join("0") + d.coefficient
      : d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"));
}

function toSystem(x, p) {
  var d = btod(x, p);
  if (!d) return x + "";
  var i = d.exponent % 3;
  if (!i) i = 3;
  else if (i < 0) i += 3;
  return d.coefficient.slice(0, i)
      + (d.coefficient.length > i
      ? "." + d.coefficient.slice(i)
      : new Array(i - d.coefficient.length + 1).join("0"))
      + systemSymbols[8 + Math.floor((d.exponent - 1) / 3)]; // TODO this screws up grouping, but only if grouping < 3
}

function stringOf(x) {
  return x + "";
}

function identity(x) {
  return x;
}

function formatGroup(grouping, thousands) {
  return function(value, width) {
    var i = value.length,
        t = [],
        j = 0,
        g = grouping[0],
        length = 0;

    while (i > 0 && g > 0) {
      if (length + g + 1 > width) g = Math.max(1, width - length);
      t.push(value.substring(i -= g, i + g));
      if ((length += g + 1) > width) break;
      g = grouping[j = (j + 1) % grouping.length];
    }

    return t.reverse().join(thousands);
  };
}

export default function(locale) {
  var group = locale.grouping && locale.thousands ? formatGroup(locale.grouping, locale.thousands) : identity,
      currency = locale.currency,
      decimal = locale.decimal;

  return function(specifier) {
    var match = formatRe.exec(specifier),
        fill = match[1] || " ",
        align = match[2] || ">",
        sign = match[3] || "-",
        symbol = match[4] || "",
        zfill = match[5],
        width = +match[6],
        comma = match[7],
        precision = match[8],
        type = match[9],
        prefix = "",
        suffix = "",
        integer = false,
        exponent = true;

    if (precision) precision = +precision.substring(1);

    if (zfill || fill === "0" && align === "=") {
      zfill = fill = "0";
      align = "=";
    }

    switch (type) {
      case "n": comma = true; type = "g"; break;
      case "%": suffix = "%"; break;
      case "p": suffix = "%"; break;
      case "b":
      case "o":
      case "x":
      case "X": if (symbol === "#") prefix = "0" + type.toLowerCase();
      case "c": exponent = false;
      case "d": integer = true; precision = 0; break;
    }

    if (symbol === "$") prefix = currency[0], suffix = currency[1];

    // Ensure that the requested precision is in the supported range.
    if (precision != null) {
      if (type == "g") precision = Math.max(1, Math.min(21, precision));
      else if (type == "e" || type == "f" || type == "%") precision = Math.max(0, Math.min(20, precision));
    }

    type = formatTypes[type] || stringOf;

    var zcomma = zfill && comma;

    return function(value) {
      value = +value;

      // Return the empty string for floats formatted as ints.
      if (integer && (value % 1)) return "";

      // Convert negative to positive, and record the sign prefix.
      var valueSign = value < 0 || value === 0 && 1 / value < 0 ? (value *= -1, "-") : sign === "-" ? "" : sign;

      // Convert to the desired precision.
      value = type(value, precision);

      // Break the value into the integer part (before) and decimal part (after).
      var i = value.lastIndexOf("."),
          before,
          after;

      // If there is no decimal, break on "e" where appropriate.
      if (i < 0) {
        var j = exponent ? value.lastIndexOf("e") : -1;
        if (j < 0) before = value, after = "";
        else before = value.substring(0, j), after = value.substring(j);
      } else {
        before = value.substring(0, i);
        after = decimal + value.substring(i + 1);
      }

      // If the fill character is not "0", grouping is applied before padding.
      if (!zfill && comma) before = group(before, Infinity);

      var length = prefix.length + before.length + after.length + suffix.length + (zcomma ? 0 : valueSign.length),
          padding = length < width ? new Array(length = width - length + 1).join(fill) : "";

      // If the fill character is "0", grouping is applied after padding.
      if (zcomma) before = group(padding + before, padding.length ? width - after.length : Infinity);

      // Apply prefix.
      valueSign += prefix;

      // Rejoin integer and decimal parts.
      value = before + after;

      return (align === "<" ? valueSign + value + padding
          : align === ">" ? padding + valueSign + value
          : align === "^" ? padding.substring(0, length >>= 1) + valueSign + value + padding.substring(length)
          : valueSign + (zcomma ? value : padding + value)) + suffix;
    };
  };
};
