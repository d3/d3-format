var tape = require("tape"),
    format = require("../");

tape("formatSpecifier(specifier) throws an error for invalid formats", function(test) {
  test.throws(function() { format.formatSpecifier("foo"); }, /invalid format: foo/);
  test.throws(function() { format.formatSpecifier(".-2s"); }, /invalid format: \.-2s/);
  test.throws(function() { format.formatSpecifier(".f"); }, /invalid format: \.f/);
  test.end();
});

tape("formatSpecifier(specifier) returns an instanceof formatSpecifier", function(test) {
  var s = format.formatSpecifier("");
  test.equal(s instanceof format.formatSpecifier, true);
  test.end();
});

tape("formatSpecifier(\"\") has the expected defaults", function(test) {
  var s = format.formatSpecifier("");
  test.equal(s.fill, " ");
  test.equal(s.align, ">");
  test.equal(s.sign, "-");
  test.equal(s.symbol, "");
  test.equal(s.zero, false);
  test.equal(s.width, undefined);
  test.equal(s.comma, false);
  test.equal(s.precision, undefined);
  test.equal(s.trim, false);
  test.equal(s.type, "");
  test.end();
});

tape("formatSpecifier(specifier) preserves unknown types", function(test) {
  var s = format.formatSpecifier("q");
  test.equal(s.trim, false);
  test.equal(s.type, "q");
  test.end();
});

tape("formatSpecifier(specifier) preserves shorthand", function(test) {
  var s = format.formatSpecifier("");
  test.equal(s.trim, false);
  test.equal(s.type, "");
  test.end();
});

tape("formatSpecifier(specifier).toString() reflects current field values", function(test) {
  var s = format.formatSpecifier("");
  test.equal((s.fill = "_", s) + "", "_>-");
  test.equal((s.align = "^", s) + "", "_^-");
  test.equal((s.sign = "+", s) + "", "_^+");
  test.equal((s.symbol = "$", s) + "", "_^+$");
  test.equal((s.zero = true, s) + "", "_^+$0");
  test.equal((s.width = 12, s) + "", "_^+$012");
  test.equal((s.comma = true, s) + "", "_^+$012,");
  test.equal((s.precision = 2, s) + "", "_^+$012,.2");
  test.equal((s.type = "f", s) + "", "_^+$012,.2f");
  test.equal((s.trim = true, s) + "", "_^+$012,.2~f");
  test.equal(format.format(s)(42), "+$0,000,000,042");
  test.end();
});

tape("formatSpecifier(specifier).toString() clamps precision to zero", function(test) {
  var s = format.formatSpecifier("");
  test.equal((s.precision = -1, s) + "", " >-.0");
  test.end();
});

tape("formatSpecifier(specifier).toString() clamps width to one", function(test) {
  var s = format.formatSpecifier("");
  test.equal((s.width = -1, s) + "", " >-1");
  test.end();
});

tape("new FormatSpecifier({}) has the expected defaults", function(test) {
  var s = new format.FormatSpecifier({});
  test.strictEqual(s.fill, " ");
  test.strictEqual(s.align, ">");
  test.strictEqual(s.sign, "-");
  test.strictEqual(s.symbol, "");
  test.strictEqual(s.zero, false);
  test.strictEqual(s.width, undefined);
  test.strictEqual(s.comma, false);
  test.strictEqual(s.precision, undefined);
  test.strictEqual(s.trim, false);
  test.strictEqual(s.type, "");
  test.end();
});

tape("new FormatSpecifier({…}) coerces all inputs to the expected types", function(test) {
  var s = new format.FormatSpecifier({
    fill: 1,
    align: 2,
    sign: 3,
    symbol: 4,
    zero: 5,
    width: 6,
    comma: 7,
    precision: 8,
    trim: 9,
    type: 10
  });
  test.strictEqual(s.fill, "1");
  test.strictEqual(s.align, "2");
  test.strictEqual(s.sign, "3");
  test.strictEqual(s.symbol, "4");
  test.strictEqual(s.zero, true);
  test.strictEqual(s.width, 6);
  test.strictEqual(s.comma, true);
  test.strictEqual(s.precision, 8);
  test.strictEqual(s.trim, true);
  test.strictEqual(s.type, "10");
  test.end();
});
