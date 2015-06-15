# d3-format

Ever noticed how sometimes JavaScript doesn’t display numbers the way you expect? Like, you tried to print 0.1, 0.2, … 0.9 with a simple loop:

```js
for (var i = 0; i < 10; i++) {
  console.log(.1 * i);
}
```

And you got this:

```js
0
0.1
0.2
0.30000000000000004
0.4
0.5
0.6000000000000001
0.7000000000000001
0.8
0.9
```

Welcome to binary floating point! ಠ_ಠ

Rounding error is not the only reason to customize number formatting. A table of numbers should be formatted consistently for comparison; above, 0.0 would be better than 0. Large numbers should have grouped digits (e.g., 42,000) or be in scientific or metric notation (4.2e+4, 42k). Currencies should have fixed precision ($3.50). Reported numerical results should be rounded to significant digits (4021 ↦ 4000). Number formats should appropriate to the reader’s locale (42.000,00 or 42,000.00). The list goes on.

Formatting numbers for human consumption is the purpose of d3-format, which is modeled after Python 3’s [format specification mini-language](https://docs.python.org/3/library/string.html#format-specification-mini-language) ([PEP 3101](https://www.python.org/dev/peps/pep-3101/)). Revisiting the example above:

```js
var f = format(".1f");
for (var i = 0; i < 10; i++) {
  console.log(f(.1 * i));
}
```

Now you get this:

```js
0.0
0.1
0.2
0.3
0.4
0.5
0.6
0.7
0.8
0.9
```

But d3-format is much more than an alias for [number.toFixed](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number/toFixed)! A few more examples:

```js
format(".0%")(.12);   // "12%"
format("$.2f")(3.5);  // "$3.50"
format("+20")(42);    // "                 +42"
format("*^20")(42);   // "*********42*********"
format(".2s")(42e6);  // "42M"
format("#x")(48879);  // "0xbeef"
format(",.2r")(4223); // "4,200"
```

See [*locale*.format](#locale_format) for a detailed specification, and try running [formatSpecifier](#formatSpecifier) on the above formats to decode their meaning.

<a name="format" href="#format">#</a> <b>format</b>(<i>specifier</i>)

An alias for [*locale*.format](#locale_format) on the default locale. While this method generates output for U.S. English-speaking humans ([`en-US`](https://github.com/d3/d3-format/tree/master/src/format-en-US.js)) by default, humans in other locales may be served using [localeFormat](#localeFormat) or by editing [index.js](https://github.com/d3/d3-format/tree/master/index.js) and rebuilding.

<a name="formatPrefix" href="#formatPrefix">#</a> <b>formatPrefix</b>(<i>specifier</i>)

An alias for [*locale*.formatPrefix](#locale_formatPrefix) on the default locale. While this method generates output for U.S. English-speaking humans ([`en-US`](https://github.com/d3/d3-format/tree/master/src/format-en-US.js)) by default, humans in other locales may be served using [localeFormat](#localeFormat) or by editing [index.js](https://github.com/d3/d3-format/tree/master/index.js) and rebuilding.

<a name="locale_format" href="#locale_format">#</a> <i>locale</i>.<b>format</b>(<i>specifier</i>)

Returns a new format function with the given string *specifier*. The returned function takes a number as the only argument, and returns a string representing the formatted number. The general form of a specifier is:

```
[​[fill]align][sign][symbol][0][width][,][.precision][type]
```

The *fill* can be any character. The presence of a fill character is signaled by the *align* character following it, which must be one of the following:

* `>` - Forces the field to be right-aligned within the available space. (Default behavior).
* `<` - Forces the field to be left-aligned within the available space.
* `^` - Forces the field to be centered within the available space.
* `=` - like `>`, but with any sign and symbol to the left of any padding.

The *sign* can be:

* `-` - nothing for positive and a minus sign for negative. (Default behavior.)
* `+` - a plus sign for positive and a minus sign for negative.
* ` ` (space) - a space for positive and a minus sign for negative.

The *symbol* can be:

* `$` - apply currency symbols per the locale definition.
* `#` - for binary, octal, or hexadecimal notation, prefix by `0b`, `0o`, or `0x`, respectively.

The *zero* (`0`) option enables zero-padding; this implicitly sets *fill* to `0` and *align* to `=`. The *width* defines the minimum field width. If not specified, then the width will be determined by the content. The *comma* (`,`) option enables the use of a group separator, such as a comma for thousands.

Depending on the *type*, the *precision* either indicates the number of digits that follow the decimal point (types `f` and `%`), or the number of significant digits (types `​`, `e`, `g`, `r`, `s` and `p`). If the precision is not specified, it defaults to 6 for all types except `​` (none), which defaults to 12. Precision is ignored for integer formats (types `b`, `o`, `d`, `x`, `X` and `c`). See [precisionFixed](#precisionFixed) and [precisionRound](#precisionRound) for help picking an appropriate precision.

The available *type* values are:

* `e` - exponent notation.
* `f` - fixed point notation.
* `g` - round to significant digits, and then either decimal or exponent notation.
* `r` - round to significant digits, and then decimal notation.
* `s` - round to significant digits, and then decimal notation with an [SI prefix](#locale_formatPrefix).
* `%` - multiply by 100, and then decimal notation with a percent sign.
* `p` - multiply by 100, round to significant digits, and then decimal notation with a percent sign.
* `b` - binary notation; ignores non-integers.
* `o` - octal notation; ignores non-integers.
* `d` - decimal notation; ignores non-integers.
* `x` - hexadecimal notation, using lower-case letters; ignores non-integers.
* `X` - hexadecimal notation, using upper-case letters; ignores non-integers.
* `c` - converts the integer to the corresponding unicode character before printing.
* `​` (none) - like `g`, but trim insignificant trailing zeros.

The type `n` is also supported as shorthand for `,g`.

<a name="locale_formatPrefix" href="#locale_formatPrefix">#</a> <i>locale</i>.<b>formatPrefix</b>(<i>specifier</i>, <i>value</i>)

Equivalent to [*locale*.format](#locale_format), except the returned function will convert values to the units of the appropriate [SI prefix](https://en.wikipedia.org/wiki/Metric_prefix#List_of_SI_prefixes) for the specified numeric reference *value* before formatting in fixed point notation. The following prefixes are supported:

* `y` - yocto, 10⁻²⁴
* `z` - zepto, 10⁻²¹
* `a` - atto, 10⁻¹⁸
* `f` - femto, 10⁻¹⁵
* `p` - pico, 10⁻¹²
* `n` - nano, 10⁻⁹
* `µ` - micro, 10⁻⁶
* `m` - milli, 10⁻³
* `​` (none) - 10⁰
* `k` - kilo, 10³
* `M` - mega, 10⁶
* `G` - giga, 10⁹
* `T` - peta, 10¹²
* `P` - peta, 10¹⁵
* `E` - exa, 10¹⁸
* `Z` - zetta, 10²¹
* `Y` - yotta, 10²⁴

Unlike [*locale*.format](#locale_format) with the `s` format type, this method returns a formatter with a consistent SI prefix, rather than computing the prefix dynamically for each number. In addition, the *precision* for the given *specifier* represents the number of digits past the decimal point (as with `f` fixed point notation), not the number of significant digits. For example:

```js
var f = formatPrefix(",.0", 1e-6);
f(.00042); // "420µ"
f(.0042); // "4,200µ"
```

This method is useful when formatting multiple numbers in the same units for easy comparison. See [precisionPrefix](#precisionPrefix) for help picking an appropriate precision.

<a name="localeFormat" href="#localeFormat">#</a> <b>localeFormat</b>(<i>definition</i>)

Returns a *locale* object for the specified *definition*, with [*locale*.format](#locale_format) and [*locale*.formatPrefix](#locale_formatPrefix) methods. The locale *definition* must include the following properties:

* `decimal` - the decimal point (e.g., `"."`).
* `thousands` - the group separator (e.g., `","`).
* `grouping` - the array of group sizes (e.g., `[3]`), cycled as needed.
* `currency` - the currency prefix and suffix (e.g., `["$", ""]`).

(Note that the *thousands* property is a misnomer, as the grouping definition allows groups other than thousands.) The following locale definitions are available in the source:

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

To change the default locale, edit [index.js](https://github.com/d3/d3-format/tree/master/index.js) and run `npm run prepublish`.

<a name="formatSpecifier" href="#formatSpecifier">#</a> <b>formatSpecifier</b>(<i>specifier</i>)

Parses the specified *specifier*, returning an object with exposed fields that correspond to the [format specification mini-language](#locale_format) and a toString method that reconstructs the specifier. For example, `formatSpecifier("s")` returns:

```js
{
  "fill": " ",
  "align": ">",
  "sign": "-",
  "symbol": "",
  "zero": false,
  "width": undefined,
  "comma": false,
  "precision": 6,
  "type": "s"
}
```

This method is useful for understanding how format specifiers are parsed and for deriving new specifiers. For example, you might compute an appropriate precision based on the numbers you want to format using [precisionFixed](#precisionFixed) and then create a new format:

```js
var s = formatSpecifier("f");
s.precision = precisionFixed(.01);
var f = format(s);
f(42); // "42.00";
```

<a name="precisionFixed" href="#precisionFixed">#</a> <b>precisionFixed</b>(<i>step</i>)

Returns a suggested decimal precision for fixed point notation given the specified numeric *step* value. The *step* represents the minimum absolute difference between values that will be formatted. For example, given the numbers 1, 1.5, and 2, the *step* should be 0.5 and the suggested precision is 1:

```js
var p = precisionFixed(0.5),
    f = format("." + p + "f");
f(1);   // "1.0"
f(1.5); // "1.5"
f(2);   // "2.0"
```

Whereas for the numbers 1, 2 and 3, the *step* should be 1 and the suggested precision is 0:

```js
var p = precisionFixed(1),
    f = format("." + p + "f");
f(1); // "1"
f(2); // "2"
f(3); // "3"
```

Note: for the `%` format type, subtract two:

```js
var p = Math.max(0, precisionFixed(0.05) - 2),
    f = format("." + p + "%");
f(.45); // "45%"
f(.50); // "50%"
f(.55); // "55%"
```

<a name="precisionPrefix" href="#precisionPrefix">#</a> <b>precisionRound</b>(<i>step</i>, <i>value</i>)

Returns a suggested decimal precision for use with [*locale*.formatPrefix](#locale_formatPrefix) given the specified numeric *step* and reference *value*. The *step* represents the minimum absolute difference between values that will be formatted, and *value* determines which SI prefix will be used. For example, given the numbers 1.1e6, 1.2e6, and 1.3e6, the *step* should be 1e5, the *value* could be 1.3e6, and the suggested precision is 1:

```js
var p = precisionPrefix(1e5, 1.3e6),
    f = formatPrefix("." + p, 1.3e6);
f(1.1e6); // "1.1M"
f(1.2e6); // "1.2M"
f(1.3e6); // "1.3M"
```

<a name="precisionRound" href="#precisionRound">#</a> <b>precisionRound</b>(<i>step</i>, <i>max</i>)

Returns a suggested decimal precision for format types that round to significant digits given the specified numeric *step* and *max* values. The *step* represents the minimum absolute difference between values that will be formatted, and the *max* represents the largest absolute value that will be formatted. For example, given the numbers 0.99, 1.0, and 1.01, the *step* should be 0.01, the *max* should be 1.01, and the suggested precision is 3:

```js
var p = precisionRound(0.01, 1.01),
    f = format("." + p + "r");
f(0.99); // "0.990"
f(1.0);  // "1.00"
f(1.01); // "1.01"
```

Whereas for the numbers 0.9, 1.0, and 1.1, the *step* should be 0.1, the *max* should be 1.1, and the suggested precision is 2:

```js
var p = precisionRound(0.1, 1.1),
    f = format("." + p + "r");
f(0.9); // "0.90"
f(1.0); // "1.0"
f(1.1); // "1.1"
```

Note: for the `e` format type, subtract one:

```js
var p = Math.max(0, precisionRound(0.01, 1.01) - 1),
    f = format("." + p + "e");
f(0.01); // "1.00e-2"
f(1.01); // "1.01e+0"
```
