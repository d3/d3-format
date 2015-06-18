import exponent from "./exponent";
import formatGroup from "./formatGroup";
import formatSpecifier from "./formatSpecifier";
import formatTypes from "./formatTypes";
import {prefixExponent} from "./formatPrefixAuto";

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
    var prefix = symbol === "$" ? currency[0] : symbol === "#" && /[boxX]/.test(type) ? "0" + type.toLowerCase() : "", // TODO remove regex
        suffix = symbol === "$" ? currency[1] : "";

    // What format function should we use?
    // Is this an integer type?
    // Can this type generate exponential notation?
    var formatType = formatTypes[type];

    // Set the default precision if not specified,
    // or clamp the specified precision to the supported range.
    // For significant precision, it must be in [1, 21].
    // For fixed precision, it must be in [0, 20].
    precision = precision == null ? (type ? 6 : 12)
        : /[gprs]/.test(type) ? Math.max(1, Math.min(21, precision))
        : Math.max(0, Math.min(20, precision));

    return function(value) {
      var parts = formatType(value, precision);

      if (!parts) return "";

      var pre = (parts[0] ? "-" : sign === "-" ? "" : sign) + prefix,
          mid = parts[1],
          post = parts[2] + suffix;

      // If the fill character is not "0", grouping is applied before padding.
      if (comma && !zero) mid = group(mid, Infinity);

      // Compute the padding.
      var length = pre.length + mid.length + post.length,
          padding = length < width ? new Array(width - length + 1).join(fill) : "";

      // If the fill character is "0", grouping is applied after padding.
      if (comma && zero) mid = group(padding + mid, padding.length ? width - post.length : Infinity), padding = "";

      // Reconstruct the final output based on the desired alignment.
      switch (align) {
        case "<": return pre + mid + post + padding;
        case "=": return pre + padding + mid + post;
        case "^": return padding.slice(0, length = padding.length >> 1) + pre + mid + post + padding.slice(length);
        default: return padding + pre + mid + post;
      }
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
