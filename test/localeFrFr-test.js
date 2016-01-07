var tape = require("tape"),
    format = require("../");

tape("formatFrFr is a French format locale", function(test) {
  var locale = format.formatFrFr;
  test.equal(locale.format("$,.2f")(1234.56), "1.234,56 €");
  test.end();
});
