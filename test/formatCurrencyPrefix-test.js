var tape = require("tape"),
    format = require("../dist/d3-format");

tape("formatCurrencyPrefix(\"K\", value)(number) formats with the thousands prefix if appropriate to the specified value", function(test) {
  test.equal(format.formatCurrencyPrefix(",.0K", 1e3)(42000), "42K");
  test.equal(format.formatCurrencyPrefix(",.0K", 1e3)(420000), "420K");
  test.equal(format.formatCurrencyPrefix(",.3K", 1e3)(420), "0.420K");
  test.end();
});

tape("formatCurrencyPrefix(\"K\", value)(number) formats with the millions prefix if appropriate to the specified value", function(test) {
  test.equal(format.formatCurrencyPrefix(",.0K", 1e6)(42000000), "42M");
  test.equal(format.formatCurrencyPrefix(",.0K", 1e6)(420000000), "420M");
  test.equal(format.formatCurrencyPrefix(",.3K", 1e6)(420000), "0.420M");
  test.end();
});

tape("formatCurrencyPrefix(\"K\", value)(number) formats with the billions prefix if appropriate to the specified value", function(test) {
  test.equal(format.formatCurrencyPrefix(",.0K", 1e9)(42 * 1e9), "42B");
  test.equal(format.formatCurrencyPrefix(",.0K", 1e9)(420 * 1e9), "420B");
  test.equal(format.formatCurrencyPrefix(",.3K", 1e9)(420 * 1e6), "0.420B");
  test.end();
});

tape("formatCurrencyPrefix(\"K\", value)(number) formats with the trillions prefix if appropriate to the specified value", function(test) {
  test.equal(format.formatCurrencyPrefix(",.0K", 1e12)(42 * 1e12), "42T");
  test.equal(format.formatCurrencyPrefix(",.0K", 1e12)(420 * 1e12), "420T");
  test.equal(format.formatCurrencyPrefix(",.3K", 1e12)(420 * 1e9), "0.420T");
  test.end();
});


tape("formatCurrencyPrefix(\"K\", value)(number) uses nothing for very small reference values", function(test) {
  test.equal(format.formatCurrencyPrefix(",.3K", 1e-3)(0.1), "0.100");
  test.end();
});

tape("formatCurrencyPrefix(\"K\", value)(number) uses trillions for very large reference values", function(test) {
  test.equal(format.formatCurrencyPrefix(",.0K", 1e15)(1e15), "1,000T");
  test.end();
});

tape("formatCurrencyPrefix(\"$,K\", value)(number) formats with the associated currency", function(test) {
  var f = format.formatCurrencyPrefix(" $12,.1K", 1e9);
  test.equal(f(-42e9),  "      -$42.0B");
  test.equal(f(+4.2e9), "        $4.2B");
  test.end();
});
