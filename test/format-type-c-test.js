var tape = require("tape"),
    format = require("../");

tape("format(\"c\") unicode character", function(test) {
  test.equal(format.format("c")(9731), "☃");
  test.end();
});

tape("format(\"c\") does not localize a decimal point", function(test) {
  test.equal(format.localeFormat({decimal: "/"}).format("c")(46), ".");
  test.end();
});
