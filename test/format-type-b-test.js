var tape = require("tape"),
    format = require("../");

tape("format(\"b\") returns binary notation", function(test) {
  test.equal(format.format("b")(10), "1010");
  test.end();
});

tape("format(\"#b\") returns binary notation with prefix", function(test) {
  test.equal(format.format("#b")(10), "0b1010");
  test.end();
});

tape("format(\"b\") returns the empty string for non-integers", function(test) {
  test.equal(format.format("b")(10.2), "");
  test.end();
});
