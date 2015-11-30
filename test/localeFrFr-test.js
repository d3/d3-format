var tape = require("tape"),
    format = require("../");

tape("localeFrFr is a French format", function(test) {
  var locale = format.localeFrFr;
  test.equal(locale.format("$,.2f")(1234.56), "1.234,56 €");
  test.end();
});
