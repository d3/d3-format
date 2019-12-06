var tape = require("tape"),
  format = require("../");

tape("format(\"*scale\") scales the value correctly", function (test) {
  var f = format.format(".0f*10000");
  test.equal(f(0), "0");
  test.equal(f(0.0384), "384");
  test.equal(f(-0.0384), "-384");
  test.end();
});
