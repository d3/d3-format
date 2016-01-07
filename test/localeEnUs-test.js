var tape = require("tape"),
    format = require("../");

tape("formatEnUs is a U.S. English format locale", function(test) {
  var locale = format.formatEnUs;
  test.equal(locale.format("$,.2f")(1234.56), "$1,234.56");
  test.end();
});
