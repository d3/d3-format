var tape = require("tape"),
    format = require("../");

tape("format(specifier)(number) returns a string", function(test) {
  test.equal(typeof format.format("d")(0), "string");
  test.end();
});

tape("format(\"d\") can zero fill", function(test) {
  var f = format.format("08d");
  test.equal(f(0), "00000000");
  test.equal(f(42), "00000042");
  test.equal(f(42000000), "42000000");
  test.equal(f(420000000), "420000000");
  test.equal(f(-4), "-0000004");
  test.equal(f(-42), "-0000042");
  test.equal(f(-4200000), "-4200000");
  test.equal(f(-42000000), "-42000000");
  test.end();
});

tape("format(\"d\") can space fill", function(test) {
  var f = format.format("8d");
  test.equal(f(0), "       0");
  test.equal(f(42), "      42");
  test.equal(f(42000000), "42000000");
  test.equal(f(420000000), "420000000");
  test.equal(f(-4), "      -4");
  test.equal(f(-42), "     -42");
  test.equal(f(-4200000), "-4200000");
  test.equal(f(-42000000), "-42000000");
  test.end();
});

tape("format(\"d\") can underscore fill", function(test) {
  var f = format.format("_>8d");
  test.equal(f(0), "_______0");
  test.equal(f(42), "______42");
  test.equal(f(42000000), "42000000");
  test.equal(f(420000000), "420000000");
  test.equal(f(-4), "______-4");
  test.equal(f(-42), "_____-42");
  test.equal(f(-4200000), "-4200000");
  test.equal(f(-42000000), "-42000000");
  test.end();
});

tape("format(\"d\") can zero fill with sign and group", function(test) {
  var f = format.format("+08,d");
  test.equal(f(0), "+0,000,000");
  test.equal(f(42), "+0,000,042");
  test.equal(f(42000000), "+42,000,000");
  test.equal(f(420000000), "+420,000,000");
  test.equal(f(-4), "-0,000,004");
  test.equal(f(-42), "-0,000,042");
  test.equal(f(-4200000), "-4,200,000");
  test.equal(f(-42000000), "-42,000,000");
  test.end();
});

tape("format(\"d\") always uses zero precision", function(test) {
  var f = format.format(".2d");
  test.equal(f(0), "0");
  test.equal(f(42), "42");
  test.equal(f(-4.2), "");
  test.end();
});

tape("format(\"d\") returns the empty string for non-integers", function(test) {
  var f = format.format("d");
  test.equal(f(4.2), "");
  test.end();
});

tape("format(\"f\") can output fixed-point notation", function(test) {
  test.equal(format.format(".1f")(0.49), "0.5");
  test.equal(format.format(".2f")(0.449), "0.45");
  test.equal(format.format(".3f")(0.4449), "0.445");
  test.equal(format.format(".5f")(0.444449), "0.44445");
  test.equal(format.format(".1f")(100), "100.0");
  test.equal(format.format(".2f")(100), "100.00");
  test.equal(format.format(".3f")(100), "100.000");
  test.equal(format.format(".5f")(100), "100.00000");
  test.end();
});

tape("format(\"g\") can output general notation", function(test) {
  test.equal(format.format(".1g")(0.049), "0.05");
  test.equal(format.format(".1g")(0.49), "0.5");
  test.equal(format.format(".2g")(0.449), "0.45");
  test.equal(format.format(".3g")(0.4449), "0.445");
  test.equal(format.format(".5g")(0.444449), "0.44445");
  test.equal(format.format(".1g")(100), "1e+2");
  test.equal(format.format(".2g")(100), "1.0e+2");
  test.equal(format.format(".3g")(100), "100");
  test.equal(format.format(".5g")(100), "100.00");
  test.equal(format.format(".5g")(100.2), "100.20");
  test.equal(format.format(".2g")(0.002), "0.0020");
  test.end();
});

tape("format(\"e\") can output exponent notation", function(test) {
  var f = format.format("e");
  test.equal(f(0), "0e+0");
  test.equal(f(42), "4.2e+1");
  test.equal(f(42000000), "4.2e+7");
  test.equal(f(420000000), "4.2e+8");
  test.equal(f(-4), "-4e+0");
  test.equal(f(-42), "-4.2e+1");
  test.equal(f(-4200000), "-4.2e+6");
  test.equal(f(-42000000), "-4.2e+7");
  test.equal(format.format(".0e")(42), "4e+1")
  test.equal(format.format(".3e")(42), "4.200e+1")
  test.end();
});

tape("format(\"s\") can output SI prefix notation", function(test) {
  var f = format.format("s");
  test.equal(f(0), "0");
  test.equal(f(1), "1");
  test.equal(f(10), "10");
  test.equal(f(100), "100");
  test.equal(f(999.5), "999.5");
  test.equal(f(999500), "999.5k");
  test.equal(f(1000), "1k");
  test.equal(f(100), "100");
  test.equal(f(1400), "1.4k");
  test.equal(f(1500.5), "1.5005k");
  test.equal(f(.00001), "10µ");
  test.equal(f(.000001), "1µ");
  test.end();
});

tape("format(\"s\") can output SI prefix notation with appropriate rounding", function(test) {
  var f = format.format(".3s");
  test.equal(f(0), "0.00");
  test.equal(f(1), "1.00");
  test.equal(f(10), "10.0");
  test.equal(f(100), "100");
  test.equal(f(999.5), "1.00k");
  test.equal(f(999500), "1.00M");
  test.equal(f(1000), "1.00k");
  test.equal(f(1500.5), "1.50k");
  test.equal(f(145500000), "146M");
  test.equal(f(145999999.999999347), "146M");
  test.equal(f(1e26), "100Y");
  test.equal(f(.000001), "1.00µ");
  test.equal(f(.009995), "10.0m");
  var f = format.format(".4s");
  test.equal(f(999.5), "999.5");
  test.equal(f(999500), "999.5k");
  test.equal(f(.009995), "9.995m");
  test.end();
});

tape("format(\"$s\") can output SI prefix notation with appropriate rounding and currency symbol", function(test) {
  var f = format.format("$.3s");
  test.equal(f(0), "$0.00");
  test.equal(f(1), "$1.00");
  test.equal(f(10), "$10.0");
  test.equal(f(100), "$100");
  test.equal(f(999.5), "$1.00k");
  test.equal(f(999500), "$1.00M");
  test.equal(f(1000), "$1.00k");
  test.equal(f(1500.5), "$1.50k");
  test.equal(f(145500000), "$146M");
  test.equal(f(145999999.999999347), "$146M");
  test.equal(f(1e26), "$100Y");
  test.equal(f(.000001), "$1.00µ");
  test.equal(f(.009995), "$10.0m");
  var f = format.format("$.4s");
  test.equal(f(999.5), "$999.5");
  test.equal(f(999500), "$999.5k");
  test.equal(f(.009995), "$9.995m");
  test.end();
});

tape("format(\"s\") SI prefix notation precision is consistent for small and large numbers", function(test) {
  test.deepEqual(
    [    1e-5,     1e-4,     1e-3,     1e-2,     1e-1,    1e-0,     1e1,     1e2,      1e3,      1e4,      1e5].map(format.format("s")),
    [    '10µ',   '100µ',    '1m',    '10m',   '100m',     '1',    '10',    '100',    '1k',    '10k',   '100k']);
  test.deepEqual(
    [    1e-5,     1e-4,     1e-3,     1e-2,     1e-1,    1e-0,     1e1,     1e2,      1e3,      1e4,      1e5].map(format.format(".4s")),
    ['10.00µ', '100.0µ', '1.000m', '10.00m', '100.0m', '1.000', '10.00', '100.0', '1.000k', '10.00k', '100.0k']);
  test.end();
});

tape("format(\"$\") can output a currency", function(test) {
  var f = format.format("$");
  test.equal(f(0), "$0");
  test.equal(f(.042), "$0.042");
  test.equal(f(.42), "$0.42");
  test.equal(f(4.2), "$4.2");
  test.equal(f(-.042), "-$0.042");
  test.equal(f(-.42), "-$0.42");
  test.equal(f(-4.2), "-$4.2");
  test.end();
});

tape("format(\"+$,f\") can output a currency with comma-grouping and sign", function(test) {
  var f = format.format("+$,.2f");
  test.equal(f(0), "+$0.00");
  test.equal(f(0.429), "+$0.43");
  test.equal(f(-0.429), "-$0.43");
  test.equal(f(-1), "-$1.00");
  test.equal(f(1e4), "+$10,000.00");
  test.end();
});

tape("format(\"$s\") can output a currency with si-prefix notation", function(test) {
  var f = format.format("$.2s");
  test.equal(f(0), "$0.0");
  test.equal(f(2.5e5), "$250k");
  test.equal(f(-2.5e8), "-$250M");
  test.equal(f(2.5e11), "$250G");
  test.end();
});

tape("format(\"%\") can output a whole percentage", function(test) {
  var f = format.format("%");
  test.equal(f(0), "0%");
  test.equal(f(.042), "4%");
  test.equal(f(.42), "42%");
  test.equal(f(4.2), "420%");
  test.equal(f(-.042), "-4%");
  test.equal(f(-.42), "-42%");
  test.equal(f(-4.2), "-420%");
  test.end();
});

tape("format(\".%\") can output a percentage with precision", function(test) {
  var f = format.format(".1%");
  test.equal(f(.234), "23.4%");
  var f = format.format(".2%");
  test.equal(f(.234), "23.40%");
  test.end();
});

tape("format(\"%\") fill respects suffix", function(test) {
  test.equal(format.format("020.0%")(42), "0000000000000004200%");
  test.equal(format.format("20.0%")(42), "               4200%");
  test.end();
});

tape("format(\"p\") can output a percentage", function(test) {
  var f = format.format("p");
  test.equal(f(.00123), "0.123%");
  test.equal(f(.0123), "1.23%");
  test.equal(f(.123), "12.3%");
  test.equal(f(.234), "23.4%");
  test.equal(f(1.23), "123%");
  test.equal(f(-.00123), "-0.123%");
  test.equal(f(-.0123), "-1.23%");
  test.equal(f(-.123), "-12.3%");
  test.equal(f(-1.23), "-123%");
  test.end();
});

tape("format(\"+p\") can output a percentage with rounding and sign", function(test) {
  var f = format.format("+.2p");
  test.equal(f(.00123), "+0.12%");
  test.equal(f(.0123), "+1.2%");
  test.equal(f(.123), "+12%");
  test.equal(f(1.23), "+120%");
  test.equal(f(-.00123), "-0.12%");
  test.equal(f(-.0123), "-1.2%");
  test.equal(f(-.123), "-12%");
  test.equal(f(-1.23), "-120%");
  test.end();
});

tape("format(\"r\") can round to significant digits", function(test) {
  test.equal(format.format(".2r")(0), "0.0");
  test.equal(format.format(".1r")(0.049), "0.05");
  test.equal(format.format(".1r")(-0.049), "-0.05");
  test.equal(format.format(".1r")(0.49), "0.5");
  test.equal(format.format(".1r")(-0.49), "-0.5");
  test.equal(format.format(".2r")(0.449), "0.45");
  test.equal(format.format(".3r")(0.4449), "0.445");
  test.equal(format.format(".3r")(1.00), "1.00");
  test.equal(format.format(".3r")(0.9995), "1.00");
  test.equal(format.format(".5r")(0.444449), "0.44445");
  test.equal(format.format("r")(123.45), "123.45");
  test.equal(format.format(".1r")(123.45), "100");
  test.equal(format.format(".2r")(123.45), "120");
  test.equal(format.format(".3r")(123.45), "123");
  test.equal(format.format(".4r")(123.45), "123.5");
  test.equal(format.format(".5r")(123.45), "123.45");
  test.equal(format.format(".6r")(123.45), "123.450");
  test.equal(format.format(".1r")(.9), "0.9");
  test.equal(format.format(".1r")(.09), "0.09");
  test.equal(format.format(".1r")(.949), "0.9");
  test.equal(format.format(".1r")(.0949), "0.09");
  test.equal(format.format(".1r")(.0000000129), "0.00000001");
  test.equal(format.format(".2r")(.0000000129), "0.000000013");
  test.equal(format.format(".2r")(.00000000129), "0.0000000013");
  test.equal(format.format(".3r")(.00000000129), "0.00000000129");
  test.equal(format.format(".4r")(.00000000129), "0.000000001290");
  test.equal(format.format(".10r")(.9999999999), "0.9999999999");
  test.equal(format.format(".15r")(.999999999999999), "0.999999999999999");
  test.end();
});

tape("format(\"r\") can round very small numbers", function(test) {
  var f = format.format(".2r");
  test.equal(f(1e-22), "0.00000000000000000000010");
  test.end();
});

tape("format(\",d\") can group thousands", function(test) {
  var f = format.format(",d");
  test.equal(f(0), "0");
  test.equal(f(42), "42");
  test.equal(f(42000000), "42,000,000");
  test.equal(f(420000000), "420,000,000");
  test.equal(f(-4), "-4");
  test.equal(f(-42), "-42");
  test.equal(f(-4200000), "-4,200,000");
  test.equal(f(-42000000), "-42,000,000");
  test.equal(f(1e21), "1e+21");
  test.end();
});

tape("format(\"0,d\") can group thousands and zero fill", function(test) {
  test.equal(format.format("01,d")(0), "0");
  test.equal(format.format("01,d")(0), "0");
  test.equal(format.format("02,d")(0), "00");
  test.equal(format.format("03,d")(0), "000");
  test.equal(format.format("04,d")(0), "0,000");
  test.equal(format.format("05,d")(0), "0,000");
  test.equal(format.format("06,d")(0), "00,000");
  test.equal(format.format("08,d")(0), "0,000,000");
  test.equal(format.format("013,d")(0), "0,000,000,000");
  test.equal(format.format("021,d")(0), "0,000,000,000,000,000");
  test.equal(format.format("013,d")(-42000000), "-0,042,000,000");
  test.equal(format.format("012,d")(1e21), "0,000,001e+21");
  test.equal(format.format("013,d")(1e21), "0,000,001e+21");
  test.equal(format.format("014,d")(1e21), "00,000,001e+21");
  test.equal(format.format("015,d")(1e21), "000,000,001e+21");
  test.end();
});

tape("format(\"0,d\") can group thousands and zero fill with overflow", function(test) {
  test.equal(format.format("01,d")(1), "1");
  test.equal(format.format("01,d")(1), "1");
  test.equal(format.format("02,d")(12), "12");
  test.equal(format.format("03,d")(123), "123");
  test.equal(format.format("05,d")(12345), "12,345");
  test.equal(format.format("08,d")(12345678), "12,345,678");
  test.equal(format.format("013,d")(1234567890123), "1,234,567,890,123");
  test.end();
});

tape("format(\",d\") can group thousands and space fill", function(test) {
  test.equal(format.format("1,d")(0), "0");
  test.equal(format.format("1,d")(0), "0");
  test.equal(format.format("2,d")(0), " 0");
  test.equal(format.format("3,d")(0), "  0");
  test.equal(format.format("5,d")(0), "    0");
  test.equal(format.format("8,d")(0), "       0");
  test.equal(format.format("13,d")(0), "            0");
  test.equal(format.format("21,d")(0), "                    0");
  test.end();
});

tape("format(\",d\") can group thousands and space fill with overflow", function(test) {
  test.equal(format.format("1,d")(1), "1");
  test.equal(format.format("1,d")(1), "1");
  test.equal(format.format("2,d")(12), "12");
  test.equal(format.format("3,d")(123), "123");
  test.equal(format.format("5,d")(12345), "12,345");
  test.equal(format.format("8,d")(12345678), "12,345,678");
  test.equal(format.format("13,d")(1234567890123), "1,234,567,890,123");
  test.end();
});

tape("format(\",g\") can group thousands with general notation", function(test) {
  var f = format.format(",g");
  test.equal(f(0), "0");
  test.equal(f(42), "42");
  test.equal(f(42000000), "42,000,000");
  test.equal(f(420000000), "420,000,000");
  test.equal(f(-4), "-4");
  test.equal(f(-42), "-42");
  test.equal(f(-4200000), "-4,200,000");
  test.equal(f(-42000000), "-42,000,000");
  test.end();
});

tape("format(\",.f\") can group thousands, space fill, and round to significant digits", function(test) {
  test.equal(format.format("10,.1f")(123456.49), " 123,456.5");
  test.equal(format.format("10,.2f")(1234567.449), "1,234,567.45");
  test.equal(format.format("10,.3f")(12345678.4449), "12,345,678.445");
  test.equal(format.format("10,.5f")(123456789.444449), "123,456,789.44445");
  test.equal(format.format("10,.1f")(123456), " 123,456.0");
  test.equal(format.format("10,.2f")(1234567), "1,234,567.00");
  test.equal(format.format("10,.3f")(12345678), "12,345,678.000");
  test.equal(format.format("10,.5f")(123456789), "123,456,789.00000");
  test.end();
});

tape("format(\"f\") can display integers in fixed-point notation", function(test) {
  test.equal(format.format("f")(42), "42");
  test.end();
});

tape("format(\"d\") will not display non-integers in integer format", function(test) {
  test.equal(format.format("d")(4.2), "");
  test.end();
});

tape("format(\"c\") unicode character", function(test) {
  test.equal(format.format("c")(9731), "☃");
  test.end();
});

tape("format(\"c\") does not localize a decimal point", function(test) {
  test.equal(format.localeFormat({decimal: "/"}).format("c")(46), ".");
  test.end();
});

tape("format(\"b\") binary", function(test) {
  test.equal(format.format("b")(10), "1010");
  test.end();
});

tape("format(\"#b\") binary with prefix", function(test) {
  test.equal(format.format("#b")(10), "0b1010");
  test.end();
});

tape("format(\"o\") octal", function(test) {
  test.equal(format.format("o")(10), "12");
  test.end();
});

tape("format(\"#o\") octal with prefix", function(test) {
  test.equal(format.format("#o")(10), "0o12");
  test.end();
});

tape("format(\"x\") hexadecimal (lowercase)", function(test) {
  test.equal(format.format("x")(3735928559), "deadbeef");
  test.end();
});

tape("format(\"#x\") hexadecimal (lowercase) with prefix", function(test) {
  test.equal(format.format("#x")(3735928559), "0xdeadbeef");
  test.end();
});

tape("format(\"X\") hexadecimal (uppercase)", function(test) {
  test.equal(format.format("X")(3735928559), "DEADBEEF");
  test.end();
});

tape("format(\"#X\") hexadecimal (uppercase) with prefix", function(test) {
  test.equal(format.format("#X")(3735928559), "0xDEADBEEF");
  test.end();
});

tape("format(\"#x\") fill respects prefix", function(test) {
  test.equal(format.format("#20x")(3735928559), "          0xdeadbeef");
  test.equal(format.format("#020x")(3735928559), "0x0000000000deadbeef");
  test.end();
});

tape("format(\"<\") align left", function(test) {
  test.equal(format.format("<1,d")(0), "0");
  test.equal(format.format("<1,d")(0), "0");
  test.equal(format.format("<2,d")(0), "0 ");
  test.equal(format.format("<3,d")(0), "0  ");
  test.equal(format.format("<5,d")(0), "0    ");
  test.equal(format.format("<8,d")(0), "0       ");
  test.equal(format.format("<13,d")(0), "0            ");
  test.equal(format.format("<21,d")(0), "0                    ");
  test.end();
});

tape("format(\">\") align right", function(test) {
  test.equal(format.format(">1,d")(0), "0");
  test.equal(format.format(">1,d")(0), "0");
  test.equal(format.format(">2,d")(0), " 0");
  test.equal(format.format(">3,d")(0), "  0");
  test.equal(format.format(">5,d")(0), "    0");
  test.equal(format.format(">8,d")(0), "       0");
  test.equal(format.format(">13,d")(0), "            0");
  test.equal(format.format(">21,d")(0), "                    0");
  test.equal(format.format(">21,d")(1000), "                1,000");
  test.equal(format.format(">21,d")(1e21), "                1e+21");
  test.end();
});

tape("format(\"^\") align center", function(test) {
  test.equal(format.format("^1,d")(0), "0");
  test.equal(format.format("^1,d")(0), "0");
  test.equal(format.format("^2,d")(0), "0 ");
  test.equal(format.format("^3,d")(0), " 0 ");
  test.equal(format.format("^5,d")(0), "  0  ");
  test.equal(format.format("^8,d")(0), "   0    ");
  test.equal(format.format("^13,d")(0), "      0      ");
  test.equal(format.format("^21,d")(0), "          0          ");
  test.equal(format.format("^21,d")(1000), "        1,000        ");
  test.equal(format.format("^21,d")(1e21), "        1e+21        ");
  test.end();
});

tape("format(\"^%\") align center puts suffix adjacent to number", function(test) {
  test.equal(format.format("^21.0%")(.42),    "         42%         ");
  test.equal(format.format("^21,.0%")(422),   "       42,200%       ");
  test.equal(format.format("^21,.0%")(-422),  "      -42,200%       ");
  test.end();
});

tape("format(\"=+,d\") pad after sign", function(test) {
  test.equal(format.format("=+1,d")(0), "+0");
  test.equal(format.format("=+1,d")(0), "+0");
  test.equal(format.format("=+2,d")(0), "+0");
  test.equal(format.format("=+3,d")(0), "+ 0");
  test.equal(format.format("=+5,d")(0), "+   0");
  test.equal(format.format("=+8,d")(0), "+      0");
  test.equal(format.format("=+13,d")(0), "+           0");
  test.equal(format.format("=+21,d")(0), "+                   0");
  test.equal(format.format("=+21,d")(1e21), "+               1e+21");
  test.end();
});

tape("format(\"=+$,d\") pad after sign with currency", function(test) {
  test.equal(format.format("=+$1,d")(0), "+$0");
  test.equal(format.format("=+$1,d")(0), "+$0");
  test.equal(format.format("=+$2,d")(0), "+$0");
  test.equal(format.format("=+$3,d")(0), "+$0");
  test.equal(format.format("=+$5,d")(0), "+$  0");
  test.equal(format.format("=+$8,d")(0), "+$     0");
  test.equal(format.format("=+$13,d")(0), "+$          0");
  test.equal(format.format("=+$21,d")(0), "+$                  0");
  test.equal(format.format("=+$21,d")(1e21), "+$              1e+21");
  test.end();
});

tape("format(\" ,d\") a space can denote positive numbers", function(test) {
  test.equal(format.format(" 1,d")(-1), "-1");
  test.equal(format.format(" 1,d")(0), " 0");
  test.equal(format.format(" 2,d")(0), " 0");
  test.equal(format.format(" 3,d")(0), "  0");
  test.equal(format.format(" 5,d")(0), "    0");
  test.equal(format.format(" 8,d")(0), "       0");
  test.equal(format.format(" 13,d")(0), "            0");
  test.equal(format.format(" 21,d")(0), "                    0");
  test.equal(format.format(" 21,d")(1e21), "                1e+21");
  test.end();
});

tape("format(\"-,d\") explicitly only use a sign for negative numbers", function(test) {
  test.equal(format.format("-1,d")(-1), "-1");
  test.equal(format.format("-1,d")(0), "0");
  test.equal(format.format("-2,d")(0), " 0");
  test.equal(format.format("-3,d")(0), "  0");
  test.equal(format.format("-5,d")(0), "    0");
  test.equal(format.format("-8,d")(0), "       0");
  test.equal(format.format("-13,d")(0), "            0");
  test.equal(format.format("-21,d")(0), "                    0");
  test.end();
});

tape("format(\"d\") can format negative zero", function(test) {
  test.equal(format.format("1d")(-0), "-0");
  test.end();
});

tape("format(\"f\") can format negative zero", function(test) {
  test.equal(format.format("1f")(-0), "-0");
  test.end();
});

tape("format(\"n\") is an alias for \",g\"", function(test) {
  var f = format.format("n");
  test.equal(f(.0042), "0.0042");
  test.equal(f(.42), "0.42");
  test.equal(f(0), "0");
  test.equal(f(42), "42");
  test.equal(f(42000000), "42,000,000");
  test.equal(f(420000000), "420,000,000");
  test.equal(f(-4), "-4");
  test.equal(f(-42), "-42");
  test.equal(f(-4200000), "-4,200,000");
  test.equal(f(-42000000), "-42,000,000");
  test.equal(f(1e21), "1e+21");
  test.end();
});

tape("format(\"n\") uses zero padding", function(test) {
  test.equal(format.format("01n")(0), "0");
  test.equal(format.format("01n")(0), "0");
  test.equal(format.format("02n")(0), "00");
  test.equal(format.format("03n")(0), "000");
  test.equal(format.format("05n")(0), "0,000");
  test.equal(format.format("08n")(0), "0,000,000");
  test.equal(format.format("013n")(0), "0,000,000,000");
  test.equal(format.format("021n")(0), "0,000,000,000,000,000");
  test.equal(format.format("013n")(-42000000), "-0,042,000,000");
  test.end();
});

tape("format(\",.\") unreasonable precision values are clamped to reasonable values", function(test) {
  test.equal(format.format(".30f")(0), "0.00000000000000000000");
  test.equal(format.format(".0g")(1), "1");
  test.equal(format.format(",.-1f")(12345), "12,345");
  test.equal(format.format("+,.-1%")(123.45), "+12,345%");
  test.end();
});
