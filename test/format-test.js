var tape = require("tape"),
    format = require("../");

tape("format(specifier)(number) returns a string", function(test) {
  test.equal(typeof format.format("d")(0), "string");
  test.end();
});

tape("format(specifier).toString() returns the normalized specifier", function(test) {
  test.equal(format.format("d") + "", " >-d");
  test.end();
});

tape("format(specifier) throws an error for invalid formats", function(test) {
  test.throws(function() { format.format("foo"); }, /invalid format: foo/);
  test.throws(function() { format.format(".-2s"); }, /invalid format: \.-2s/);
  test.throws(function() { format.format(".f"); }, /invalid format: \.f/);
  test.end();
});

tape("format(\",.\") unreasonable precision values are clamped to reasonable values", function(test) {
  test.equal(format.format(".30f")(0), "0.00000000000000000000");
  test.equal(format.format(".0g")(1), "1");
  test.end();
});

tape("format(\"s\") handles very small and very large values", function(test) {
  test.equal(format.format("s")(Number.MIN_VALUE), "0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005y");
  test.equal(format.format("s")(Number.MAX_VALUE), "179769000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000Y");
  test.end();
});

tape("format(\"n\") is equivalent to format(\",g\")", function(test) {
  test.equal(format.format("n")(123456.78), "123,457");
  test.equal(format.format(",g")(123456.78), "123,457");
  test.end();
});

tape("format(\"012\") is equivalent to format(\"0=12\")", function(test) {
  test.equal(format.format("012")(123.456), "00000123.456");
  test.equal(format.format("0=12")(123.456), "00000123.456");
  test.end();
});
