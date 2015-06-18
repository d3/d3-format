import formatCharacter from "./formatCharacter";
import formatDecimal from "./formatDecimal";
import formatDefault from "./formatDefault";
import formatExponential from "./formatExponential";
import formatFixed from "./formatFixed";
import formatGeneral from "./formatGeneral";
import formatInteger from "./formatInteger";
import formatPrefixAuto from "./formatPrefixAuto";
import formatRounded from "./formatRounded";

var formatBinary = formatInteger(2),
    formatOctal = formatInteger(8),
    formatHexadecimal = formatInteger(16);

export default {
  "": formatDefault,
  "%": function(x, p) { x = formatFixed(x * 100, p); return x && (x[2] += "%", x); },
  "b": formatBinary,
  "c": formatCharacter,
  "d": formatDecimal,
  "e": formatExponential,
  "f": formatFixed,
  "g": formatGeneral,
  "o": formatOctal,
  "p": function(x, p) { x = formatRounded(x * 100, p); return x && (x[2] += "%", x); },
  "r": formatRounded,
  "s": formatPrefixAuto,
  "x": formatHexadecimal,
  "X": function(x) { x = formatHexadecimal(x); return x && (x[1] = x[1].toUpperCase(), x); }
};
