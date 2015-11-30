var tape = require("tape"),
    format = require("../");

tape("localeEnUs is a U.S. English format", function(test) {
  var locale = format.localeEnUs;
  test.equal(locale.format("$,.2f")(1234.56), "$1,234.56");
  test.end();
});
