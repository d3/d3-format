# d3-format

Number formatting is one of those things you don’t normally think about until an ugly 0.30000000000000004 appears on your axis labels. Maybe you want to group thousands and use fixed precision, such as $1,240.10. Or maybe you want to display only the significant digits of a particular number.

Formatting numbers for human consumption is the purpose of the d3-format module. For example, to create a function that zero-fills to four digits, say:

```js
var zeroPad = format("04d");
```

Now you can conveniently format numbers:

```js
zeroPad(2);   // "0002"
zeroPad(123); // "0123"
```

The default [format](#format) instance uses the U.S. English ([`en-US`](https://github.com/d3/d3-format/tree/master/src/format-en-US.js)) locale; but a handful of other locales are also available:

* [`ca-ES`](https://github.com/d3/d3-format/tree/master/src/format-ca-ES.js) - Catalan (Spain)
* [`de-DE`](https://github.com/d3/d3-format/tree/master/src/format-de-DE.js) - German (Germany)
* [`en-CA`](https://github.com/d3/d3-format/tree/master/src/format-en-CA.js) - English (Canada)
* [`en-GB`](https://github.com/d3/d3-format/tree/master/src/format-en-GB.js) - English (United Kingdom)
* [`en-US`](https://github.com/d3/d3-format/tree/master/src/format-en-US.js) - English (United States)
* [`es-ES`](https://github.com/d3/d3-format/tree/master/src/format-es-ES.js) - Spanish (Spain)
* [`fi-FI`](https://github.com/d3/d3-format/tree/master/src/format-fi-FI.js) - Finnish (Finland)
* [`fr-CA`](https://github.com/d3/d3-format/tree/master/src/format-fr-CA.js) - French (Canada)
* [`fr-FR`](https://github.com/d3/d3-format/tree/master/src/format-fr-FR.js) - French (France)
* [`he-IL`](https://github.com/d3/d3-format/tree/master/src/format-he-IL.js) - Hebrew (Israel)
* [`it-IT`](https://github.com/d3/d3-format/tree/master/src/format-it-IT.js) - Italian (Italy)
* [`mk-MK`](https://github.com/d3/d3-format/tree/master/src/format-mk-MK.js) - Macedonian (Macedonia)
* [`nl-NL`](https://github.com/d3/d3-format/tree/master/src/format-nl-NL.js) - Dutch (Netherlands)
* [`pl-PL`](https://github.com/d3/d3-format/tree/master/src/format-pl-PL.js) - Polish (Poland)
* [`pt-BR`](https://github.com/d3/d3-format/tree/master/src/format-pt-BR.js) - Portuguese (Brazil)
* [`ru-RU`](https://github.com/d3/d3-format/tree/master/src/format-ru-RU.js) - Russian (Russia)
* [`zh-CN`](https://github.com/d3/d3-format/tree/master/src/format-zh-CN.js) - Chinese (China)

To switch locales, either create a custom build by editing [index.js](https://github.com/d3/d3-format/tree/master/src/index.js) or copy the desired code it from one of the above linked locale definitions.

<a name="format" href="#format">#</a> <b>format</b>(<i>specifier</i>)

Returns a new format function with the given string *specifier*. By default, uses the U.S. English locale; use [localeFormat](#localeFormat) to specify a different locale.

The returned function takes a number as the only argument, and returns a string representing the formatted number. The format specifier is modeled after Python 3.1’s [format specification mini-language](http://docs.python.org/release/3.1.3/library/string.html#formatspec). The general form of a specifier is:

```
[​[fill]align][sign][symbol][0][width][,][.precision][type]
```

The *fill* can be any character other than `{` or `}`. The presence of a fill character is signaled by the character following it, which must be one of the *align* options.

The *align* can be:

* `>` - Forces the field to be right-aligned within the available space. (Default behavior).
* `<` - Forces the field to be left-aligned within the available space.
* `^` - Forces the field to be centered within the available space.

The *sign* can be:

* `-` - a sign should be used only for negative numbers. (Default behavior.)
* `+` - a sign should be used for both positive and negative numbers.
* ` ` - a leading space should be used on positive numbers, and a minus sign on negative numbers.

The *symbol* can be:

* `$` - a currency symbol should be prefixed (or suffixed) per the locale.
* `#` - for binary, octal, or hexadecimal output, prefix by `0b`, `0o`, or `0x`, respectively.

The *zero* (`0`) option enables zero-padding.

The *width* defines the minimum field width. If not specified, then the width will be determined by the content.

The *comma* (`,`) option enables the use of a comma for a thousands separator.

The *precision* indicates how many digits should be displayed after the decimal point for a value formatted with types `f` and `%`, or before and after the decimal point for a value formatted with types `g`, `r` and `p`.

The available *type* values are:

* `e` - use [Number.toExponential](https://developer.mozilla.org/en/JavaScript/Reference/Global_Objects/Number/toExponential).
* `g` - use [Number.toPrecision](https://developer.mozilla.org/en/JavaScript/Reference/Global_Objects/Number/toPrecision).
* `f` - use [Number.toFixed](https://developer.mozilla.org/en/JavaScript/Reference/Global_Objects/Number/toFixed).
* `d` - use [Number.toString](https://developer.mozilla.org/en/JavaScript/Reference/Global_Objects/Number/toString), but ignore any non-integer values.
* `r` - [round](#round) to *precision* significant digits, then pad like `f`.
* `%` - like `f`, but multiply by 100 and suffix with `"%"`.
* `p` - like `r`, but multiply by 100 and suffix with `"%"`.
* `b` - binary (base 2).
* `o` - octal (base 8).
* `x` - hexadecimal (base 16), using lower-case letters.
* `X` - hexadecimal (base 16), using upper-case letters.
* `c` - converts the integer to the corresponding unicode character before printing.
* `s` - like `r`, but with an SI unit such as `"9.5M"` or `"1.00µ"`.

The type `n` is also supported as shorthand for `,g`. If no *precision* is specified for `r`, `g` is used instead; if no *precision* is specified for `p`, `%` is used instead.

<a name="formatPrefix" href="#formatPrefix">#</a> <b>formatPrefix</b>(<i>value</i>[, <i>precision</i>])

Returns the [SI prefix](https://en.wikipedia.org/wiki/Metric_prefix) for the specified *value*. If an optional *precision* is specified, the *value* is rounded accordingly using [round](#round) before computing the prefix. The returned prefix object has two properties:

* `symbol` - the prefix symbol, such as `"M"` for millions.
* `scale` - the scale function, for converting numbers to the appropriate prefixed scale.

For example:

```js
var prefix = formatPrefix(1.21e9);
console.log(prefix.symbol); // "G"
console.log(prefix.scale(1.21e9)); // 1.21
```

This method is used by [format](#format) for the `s` format type.

<a name="localeFormat" href="#localeFormat">#</a> <b>localeFormat</b>(<i>locale</i>)

Returns a [*format*](#format) function localized for the specified *locale*. The *locale* definition must include the following properties:

* `decimal` - the decimal point (e.g., `"."`).
* `thousands` - the group separator (e.g., `","`).
* `grouping` - the array of group sizes (e.g., `[3]`), cycled as needed.
* `currency` - the currency prefix and suffix (e.g., `["$", ""]`).

(Note that the *thousands* property is a misnomer, as the grouping definition allows groups other than thousands.)

For example, the default U.S. English locale is defined as:

```json
{
  "decimal": ".",
  "thousands": ",",
  "grouping": [3],
  "currency": ["$", ""]
}
```

See the [source](https://github.com/d3/d3-format/tree/master/src/) for available locale definitions.

<a name="round" href="#round">#</a> <b>round</b>(<i>x</i>[, <i>n</i>])

Returns the value *x* rounded to *n* digits after the decimal point. If *n* is omitted, it defaults to zero. The result is a number. Values are rounded to the closest multiple of 10 to the power minus *n*; if two multiples are equally close, the value is rounded up in accordance with the built-in [Math.round](https://developer.mozilla.org/en/JavaScript/Reference/Global_Objects/Math/round]) function. For example:

```js
round(1.23); // 1
round(1.23, 1); // 1.2
round(1.25, 1); // 1.3
round(12.5, 0); // 13
round(12, -1); // 10
```

Note that the resulting number when converted to a string may be imprecise due to IEEE floating point precision; to format a number to a string with a fixed number of decimal points, use [format](#format) instead.
