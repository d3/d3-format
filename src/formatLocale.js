import formatPrefix from "./formatPrefix";
import formatPrecision from "./formatPrecision";
import round from "./round";

// [[fill]align][sign][symbol][0][width][,][.precision][type]
var formatRe = /(?:([^{])?([<>=^]))?([+\- ])?([$#])?(0)?(\d+)?(,)?(\.-?\d+)?([a-z%])?/i;

var formatTypes = {
  b: function(x) { return x.toString(2); },
  c: function(x) { return String.fromCharCode(x); },
  o: function(x) { return x.toString(8); },
  x: function(x) { return x.toString(16); },
  X: function(x) { return x.toString(16).toUpperCase(); },
  g: function(x, p) { return x.toPrecision(p); },
  e: function(x, p) { return x.toExponential(p); },
  f: function(x, p) { return x.toFixed(p); },
  r: function(x, p) { return (x = round(x, formatPrecision(x, p))).toFixed(Math.max(0, Math.min(20, formatPrecision(x * (1 + 1e-15), p)))); }
};

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
        scale = 1,
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
      case "%": scale = 100; suffix = "%"; type = "f"; break;
      case "p": scale = 100; suffix = "%"; type = "r"; break;
      case "b":
      case "o":
      case "x":
      case "X": if (symbol === "#") prefix = "0" + type.toLowerCase();
      case "c": exponent = false;
      case "d": integer = true; precision = 0; break;
      case "s": scale = -1; type = "r"; break;
    }

    if (symbol === "$") prefix = currency[0], suffix = currency[1];

    // If no precision is specified for r, fallback to general notation.
    if (type == "r" && !precision) type = "g";

    // Ensure that the requested precision is in the supported range.
    if (precision != null) {
      if (type == "g") precision = Math.max(1, Math.min(21, precision));
      else if (type == "e" || type == "f") precision = Math.max(0, Math.min(20, precision));
    }

    type = formatTypes[type] || stringOf;

    var zcomma = zfill && comma;

    return function(value) {

      // Return the empty string for floats formatted as ints.
      if (integer && (value % 1)) return "";

      // Convert negative to positive, and record the sign prefix.
      var valueSign = value < 0 || value === 0 && 1 / value < 0 ? (value = -value, "-") : sign === "-" ? "" : sign,
          valueSuffix = suffix;

      // Apply the scale, computing it from the value’s exponent for si format.
      // Preserve the existing suffix, if any, such as the currency symbol.
      if (scale < 0) {
        var valuePrefix = formatPrefix(value, precision);
        value = valuePrefix.scale(value);
        valueSuffix = valuePrefix.symbol + suffix;
      } else {
        value *= scale;
      }

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

      var length = prefix.length + before.length + after.length + (zcomma ? 0 : valueSign.length),
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
          : valueSign + (zcomma ? value : padding + value)) + valueSuffix;
    };
  };
};
