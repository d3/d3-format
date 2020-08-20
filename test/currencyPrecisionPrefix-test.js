var tape = require("tape"),
    format = require("../");

// For currencies, only 4 prefixes are commonly used:
// thousands (K), millions (M), billions (B) and trillions (T)

tape("precisionPrefix(step, value) returns zero between 1 and 100 (unit step)", function (test) {
  test.equal(format.currencyPrecisionPrefix(1e+0, 1e+0), 0); // 1
  test.equal(format.currencyPrecisionPrefix(1e+0, 1e+1), 0); // 10
  test.equal(format.currencyPrecisionPrefix(1e+0, 1e+2), 0); // 100
  test.end()
});

tape("precisionPrefix(step, value) returns zero between 1 and 100 (thousand step)", function (test) {
  test.equal(format.currencyPrecisionPrefix(1e+3, 1e+3), 0); // 1K
  test.equal(format.currencyPrecisionPrefix(1e+3, 1e+4), 0); // 10K
  test.equal(format.currencyPrecisionPrefix(1e+3, 1e+5), 0); // 100K
  test.end()
});

tape("precisionPrefix(step, value) returns zero between 1 and 100 (million step)", function (test) {
  test.equal(format.currencyPrecisionPrefix(1e+6, 1e+6), 0); // 1M
  test.equal(format.currencyPrecisionPrefix(1e+6, 1e+7), 0); // 10M
  test.equal(format.currencyPrecisionPrefix(1e+6, 1e+8), 0); // 100M
  test.end()
});

tape("precisionPrefix(step, value) returns zero between 1 and 100 (billion step)", function (test) {
  test.equal(format.currencyPrecisionPrefix(1e+9, 1e+9), 0); // 1B
  test.equal(format.currencyPrecisionPrefix(1e+9, 1e+10), 0); // 10B
  test.equal(format.currencyPrecisionPrefix(1e+9, 1e+11), 0); // 100B
  test.end()
});

tape("currencyPrecisionPrefix(step, value) returns the expected precision when value is greater than one trillion", function(test) {
  test.equal(format.currencyPrecisionPrefix(1e+12, 1e+12), 0); // 1T
  test.equal(format.currencyPrecisionPrefix(1e+12, 1e+13), 0); // 10T
  test.equal(format.currencyPrecisionPrefix(1e+12, 1e+14), 0); // 100T
  test.equal(format.currencyPrecisionPrefix(1e+12, 1e+15), 0); // 1000T
  test.equal(format.currencyPrecisionPrefix(1e+11, 1e+15), 1); // 1000.0T
  test.end();
});

tape("currencyPrecisionPrefix(step, value) returns the expected precision when value is less than one unit", function(test) {
  test.equal(format.currencyPrecisionPrefix(1e+0, 1e+0), 0); // 1
  test.equal(format.currencyPrecisionPrefix(1e-1, 1e-1), 1); // 0.1
  test.equal(format.currencyPrecisionPrefix(1e-2, 1e-2), 2); // 0.01
  test.equal(format.currencyPrecisionPrefix(1e-3, 1e-3), 3); // 0.001
  test.equal(format.currencyPrecisionPrefix(1e-4, 1e-4), 4); // 0.0001
  test.end();
});

