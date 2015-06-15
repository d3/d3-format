import exponent from "./exponent";
import formatGroup from "./formatGroup";
import formatSpecifier from "./formatSpecifier";
import formatTypes from "./formatTypes";
import {prefixExponent} from "./formatAutoPrefix";

var prefixes = ["y","z","a","f","p","n","µ","m","","k","M","G","T","P","E","Z","Y"];

function identity(x) {
  return x;
}

export default function(locale) {
  var group = locale.grouping && locale.thousands ? formatGroup(locale.grouping, locale.thousands) : identity,
      currency = locale.currency,
      decimal = locale.decimal;

  function format(specifier) {
    specifier = formatSpecifier(specifier);

    var fill = specifier.fill,
        align = specifier.align,
        sign = specifier.sign,
        symbol = specifier.symbol,
        zero = specifier.zero,
        width = specifier.width,
        comma = specifier.comma,
        precision = specifier.precision,
        type = specifier.type;

    // Compute the prefix and suffix.
    // For SI-prefix, the suffix is lazily computed.
    var prefix = symbol === "$" ? currency[0] : symbol === "#" && /[boxX]/.test(type) ? "0" + type.toLowerCase() : "",
        suffix = symbol === "$" ? currency[1] : /[%p]/.test(type) ? "%" : "";

    // What format function should we use?
    // Is this an integer type?
    // Can this type generate exponential notation?
    var formatType = formatTypes[type],
        integer = /[bcdoxX]/.test(type),
        maybeExponent = !type || /[deg]/.test(type),
        maybeDecimal = maybeExponent || /[fprs%]/.test(type);

    // Set the default precision if not specified,
    // or clamp the specified precision to the supported range.
    // For significant precision, it must be in [1, 21].
    // For fixed precision, it must be in [0, 20].
    precision = precision == null ? (type ? 6 : 12)
        : /[gprs]/.test(type) ? Math.max(1, Math.min(21, precision))
        : Math.max(0, Math.min(20, precision));

    return function(value) {
      value = +value;

      // Return the empty string for floats formatted as ints.
      if (integer && (value % 1)) return "";

      // Convert negative to positive, and compute the prefix.
      // Note that -0 is not less than 0, but 1 / -0 is!
      var valuePrefix = (value < 0 || 1 / value < 0 ? (value *= -1, "-") : sign === "-" ? "" : sign) + prefix;

      // Perform the initial formatting.
      value = formatType(value, precision);

      // Compute the suffix.
      var valueSuffix = suffix + (type === "s" ? prefixes[8 + prefixExponent / 3] : "");

      // Break the formatted value into the integer “value” part that can be
      // grouped, and fractional or exponential “suffix” part that is not.
      if (maybeDecimal) {
        var i = value.indexOf(".");
        if (i >= 0) {
          valueSuffix = decimal + value.slice(i + 1) + valueSuffix;
          value = value.slice(0, i);
        } else if (maybeExponent) {
          i = value.indexOf("e");
          if (i >= 0) {
            valueSuffix = value.slice(i) + valueSuffix;
            value = value.slice(0, i);
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
        case "^": return padding.slice(0, length = padding.length >> 1) + valuePrefix + value + valueSuffix + padding.slice(length);
      }
      return padding + valuePrefix + value + valueSuffix;
    };
  }

  function formatPrefix(specifier, value) {
    var f = format((specifier = formatSpecifier(specifier), specifier.type = "f", specifier)),
        e = Math.max(-8, Math.min(8, Math.floor(exponent(value) / 3))) * 3,
        k = Math.pow(10, -e),
        prefix = prefixes[8 + e / 3];
    return function(value) {
      return f(k * value) + prefix;
    };
  }

  return {
    format: format,
    formatPrefix: formatPrefix
  };
};
