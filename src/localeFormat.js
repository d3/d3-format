import btod from "./btod";
import formatDefault from "./formatDefault";
import formatGroup from "./formatGroup";
import formatRounded from "./formatRounded";
import formatRoundedPercentage from "./formatRoundedPercentage";
import formatSystem from "./formatSystem";

// [[fill]align][sign][symbol][0][width][,][.precision][type]
var formatRe = /(?:([^{])?([<>=^]))?([+\- ])?([$#])?(0)?(\d+)?(,)?(\.-?\d+)?([a-z%])?/i;

var formatTypes = {
  "": formatDefault,
  "%": function(x, p) { return (x * 100).toFixed(p); },
  "b": function(x) { return x.toString(2); },
  "c": function(x) { return String.fromCharCode(x); },
  "d": formatDefault,
  "e": function(x, p) { return x.toExponential(p); },
  "f": function(x, p) { return x.toFixed(p); },
  "g": function(x, p) { return x.toPrecision(p); },
  "o": function(x) { return x.toString(8); },
  "p": formatRoundedPercentage,
  "r": formatRounded,
  "s": formatSystem,
  "X": function(x) { return x.toString(16).toUpperCase(); },
  "x": function(x) { return x.toString(16); }
};

function identity(x) {
  return x;
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
        zero = match[5],
        width = +match[6],
        comma = match[7],
        precision = match[8],
        type = match[9];

    // The "n" type is an alias for ",g".
    if (type === "n") comma = true, type = "g";

    // Map invalid types to the default format.
    else if (!(type in formatTypes)) type = "";

    // If zero fill is specified, padding goes after sign and before digits.
    if (zero || (fill === "0" && align === "=")) zero = fill = "0", align = "=";

    // Clamp the specified precision to the supported range.
    // For significant precision, it must be in [1, 21].
    // For fixed precision, it must be in [0, 20].
    if (precision) {
      precision = +precision.substring(1);
      precision = /[gprs]/.test(type)
          ? Math.max(1, Math.min(21, precision))
          : Math.max(0, Math.min(20, precision));
    }

    // Compute the fixed prefix and suffix.
    var prefix = symbol === "$" ? currency[0] : symbol === "#" && /[boxX]/.test(type) ? "0" + type.toLowerCase() : "",
        suffix = symbol === "$" ? currency[1] : /[%p]/.test(type) ? "%" : "";

    // What format function should we use?
    // Is this an integer type?
    // Can this type generate exponential notation?
    var format = formatTypes[type],
        integer = /[bcdoxX]/.test(type),
        maybeExponent = !type || /[deg]/.test(type),
        maybeDecimal = maybeExponent || /[fprs%]/.test(type);

    return function(value) {
      value = +value;

      // Return the empty string for floats formatted as ints.
      if (integer && (value % 1)) return "";

      // Convert negative to positive, and compute the prefix.
      // Note that -0 is not less than 0, but 1 / -0 is!
      var valueSuffix = suffix,
          valuePrefix = (value < 0 || 1 / value < 0 ? (value *= -1, "-")
              : sign === "-" ? ""
              : sign) + prefix;

      // Perform the initial formatting.
      value = format(value, precision);

      // Break the formatted value into the integer “value” part that can be
      // grouped, and fractional or exponential “suffix” part that is not.
      if (maybeDecimal) {
        var i = value.indexOf(".");
        if (i >= 0) {
          valueSuffix = decimal + value.substring(i + 1) + suffix;
          value = value.substring(0, i);
        } else if (maybeExponent) {
          i = value.indexOf("e");
          if (i >= 0) {
            valueSuffix = value.substring(i) + suffix;
            value = value.substring(0, i);
          }
        }
      }

      // If the fill character is not "0", grouping is applied before padding.
      if (comma && !zero) value = group(value, Infinity);

      // Compute the padding.
      var length = valuePrefix.length + value.length + valueSuffix.length,
          padding = length < width ? new Array(width - length + 1).join(fill) : "";

      // If the fill character is "0", grouping is applied after padding.
      if (comma && zero) value = group(padding + value, padding.length ? width - valueSuffix.length : Infinity), padding = "";

      // Reconstruct the final output based on the desired alignment.
      switch (align) {
        case "<": return valuePrefix + value + valueSuffix + padding;
        case "=": return valuePrefix + padding + value + valueSuffix;
        case "^": return padding.substring(0, length = padding.length >> 1) + valuePrefix + value + valueSuffix + padding.substring(length);
      }
      return padding + valuePrefix + value + valueSuffix;
    };
  };
};
