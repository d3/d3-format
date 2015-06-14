var tape = require("tape"),
    format = require("../");

tape("formatPrefix(\"s\", prefix)(number) formats with the specified SI prefix", function(test) {
  test.equal(format.formatPrefix(",.0s", "µ")(.00042), "420µ");
  test.equal(format.formatPrefix(",.0s", "µ")(.0042), "4,200µ");
  test.equal(format.formatPrefix(",.3s", "m")(.00042), "0.420m");
  test.end();
});

tape("formatPrefix(\"s\", prefix)(number) treats unknown SI prefix as the empty string", function(test) {
  test.equal(format.formatPrefix(",.0s", "F")(42), "42");
  test.end();
});

tape("formatPrefix(\"$,s\", prefix)(number) formats with the specified SI prefix", function(test) {
  var f = format.formatPrefix(" $12,.1s", "M");
  test.equal(f(-42e6),  "      -$42.0M");
  test.equal(f(+4.2e6), "        $4.2M");
  test.end();
});
