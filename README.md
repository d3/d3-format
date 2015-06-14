# d3-format

Want to get rid of that ugly 0.30000000000000004 on your axis? Or to group thousands and use fixed precision to display currency, such as $1,240.10? Or perhaps you want to display only the significant digits of a particular number?

You’ve come to the right place. Formatting numbers for human consumption is the purpose of the d3-format module. For example, to create a function that zero-fills to four digits, say:

```js
var zeroPad = format("04d");
```

Now you can conveniently format numbers:

```js
zeroPad(2);   // "0002"
zeroPad(123); // "0123"
```

The default locale is U.S. English ([`en-US`](https://github.com/d3/d3-format/tree/master/src/format-en-US.js)), but a handful of other locales are available:

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

To switch locales, either create a custom build by editing [index.js](https://github.com/d3/d3-format/tree/master/index.js) or copy the desired code it from one of the above linked locale definitions.

<a name="format" href="#format">#</a> <b>format</b>(<i>specifier</i>)

Equivalent to [*locale*.format](#locale_format) on the default U.S. English locale. Use [localeFormat](#localeFormat) to specify a different locale.

<a name="formatPrefix" href="#formatPrefix">#</a> <b>formatPrefix</b>(<i>specifier</i>)

Equivalent to [*locale*.formatPrefix](#locale_formatPrefix) on the default U.S. English locale. Use [localeFormat](#localeFormat) to specify a different locale.

<a name="locale_format" href="#locale_format">#</a> <i>locale</i>.<b>format</b>(<i>specifier</i>)

Returns a new format function with the given string *specifier*. The returned function takes a number as the only argument, and returns a string representing the formatted number. The format specifier is modeled after Python 3.1’s [format specification mini-language](http://docs.python.org/release/3.1.3/library/string.html#formatspec). The general form of a specifier is:

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

<a name="locale_formatPrefix" href="#locale_formatPrefix">#</a> <i>locale</i>.<b>formatPrefix</b>(<i>value</i>, <i>prefix</i>)

Equivalent to [*locale*.format](#locale_format), except converts the value to the units of the specified SI *prefix*. The following prefixes are supported:

* `y` - yocto, 10⁻²⁴
* `z` - zepto, 10⁻²¹
* `a` - atto, 10⁻¹⁸
* `f` - femto, 10⁻¹⁵
* `p` - pico, 10⁻¹²
* `n` - nano, 10⁻⁹
* `µ` - micro, 10⁻⁶
* `m` - milli, 10⁻³
* `​` - 10⁰
* `k` - kilo, 10³
* `M` - mega, 10⁶
* `G` - giga, 10⁹
* `T` - peta, 10¹²
* `P` - peta, 10¹⁵
* `E` - exa, 10¹⁸
* `Z` - zetta, 10²¹
* `Y` - yotta, 10²⁴

For example:

```js
var f = formatPrefix(".0s", "µ");
f(.00042); // "420µ"
```

<a name="localeFormat" href="#localeFormat">#</a> <b>localeFormat</b>(<i>definition</i>)

Returns a *locale* object localized for the specified *definition*, with [*locale*.format](#locale_format) and [*locale*.formatPrefix](#locale_formatPrefix) methods. The locale *definition* must include the following properties:

* `decimal` - the decimal point (e.g., `"."`).
* `thousands` - the group separator (e.g., `","`).
* `grouping` - the array of group sizes (e.g., `[3]`), cycled as needed.
* `currency` - the currency prefix and suffix (e.g., `["$", ""]`).

(Note that the *thousands* property is a misnomer, as the grouping definition allows groups other than thousands.) For example, the default U.S. English locale definition is:

```json
{
  "decimal": ".",
  "thousands": ",",
  "grouping": [3],
  "currency": ["$", ""]
}
```

See the [source](https://github.com/d3/d3-format/tree/master/src/) for available locale definitions.
