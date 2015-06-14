import formatPrecision from "./src/formatPrecision";
import formatSpecifier from "./src/formatSpecifier";
import locale from "./src/format-en-US";
import localeFormat from "./src/localeFormat";

export var format = locale.format;
export var formatPrefix = locale.formatPrefix;

export {
  formatPrecision,
  formatSpecifier,
  localeFormat
};
