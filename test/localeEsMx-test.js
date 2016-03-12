var tape = require("tape"),
    format = require("../");

tape("formatEsMx is a Mexican format locale", function(test) {
  var locale = format.formatEsMx;
  test.equal(locale.format("$,.2f")(1234.56), "$1,234.56");
  test.end();
});
