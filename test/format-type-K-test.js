var tape = require("tape"),
    format = require("../dist/d3-format");

tape("format(\"K\") outputs currency prefix notation with default 3 significant digits", function(test) {
  var f = format.format("K");
  test.equal(f(0), "0.00");
  test.equal(f(0.2), "0.20");
  test.equal(f(1), "1.00");
  test.equal(f(10), "10.0");
  test.equal(f(100), "100");
  test.equal(f(1000), "1.00K");
  test.end();
});

tape("format(\"[.precision]K\") outputs currency-prefix notation with precision significant digits", function(test) {
  var f1 = format.format(".2K");
  test.equal(f1(0), "0.0");
  test.equal(f1(1), "1.0");
  test.equal(f1(10), "10");
  test.equal(f1(100), "100");
  test.equal(f1(999.5), "1.0K");
  test.equal(f1(999500), "1.0M");
  test.equal(f1(1000), "1.0K");
  test.equal(f1(1500.5), "1.5K");
  test.equal(f1(145500000), "150M");
  test.equal(f1(145999999.999999347), "150M");
  test.equal(f1(1e17), "100000T");
  test.equal(f1(.000001), "0.000001");
  var f2 = format.format(".4K");
  test.equal(f2(999.5), "999.5");
  test.equal(f2(999500), "999.5K");
  test.end();
});

tape("format(\"K\") formats numbers smaller than 1", function(test) {
  var f = format.format(".2K");
  test.equal(f(1.29e-2), "0.0129");
  test.equal(f(1.29e-3), "0.00129");
  test.equal(f(-1.29e-2), "-0.0129");
  test.equal(f(-1.29e-3), "-0.00129");
  test.end();
});

tape("format(\"K\") formats numbers larger than thousands of trillions", function(test) {
  var f = format.format(".2K");
  test.equal(f(1.23e+15), "1200T");
  test.equal(f(1.23e+16), "12000T");
  test.equal(f(-1.23e+15), "-1200T");
  test.equal(f(-1.23e+16), "-12000T");
  test.end();
});

tape("format(\"$K\") outputs currency-prefix notation with a currency symbol", function(test) {
  var f1 = format.format("$.2K");
  test.equal(f1(0), "$0.0");
  test.equal(f1(2.5e5), "$250K");
  test.equal(f1(-2.5e8), "-$250M");
  test.equal(f1(2.5e11), "$250B");
  var f2 = format.format("$.3K");
  test.equal(f2(0), "$0.00");
  test.equal(f2(1), "$1.00");
  test.equal(f2(10), "$10.0");
  test.equal(f2(100), "$100");
  test.equal(f2(999.5), "$1.00K");
  test.equal(f2(999500), "$1.00M");
  test.equal(f2(1000), "$1.00K");
  test.equal(f2(1500.5), "$1.50K");
  test.equal(f2(145500000), "$146M");
  test.equal(f2(145999999.999999347), "$146M");
  test.equal(f2(1e18), "$1000000T");
  test.equal(f2(.000001), "$0.000001");
  test.equal(f2(.009995), "$0.01");
  var f3 = format.format("$.4K");
  test.equal(f3(999.5), "$999.5");
  test.equal(f3(999500), "$999.5K");
  test.equal(f3(.009995), "$0.001");
  test.end();
});

tape("format(\"0[width],K\") will group thousands due to zero fill", function(test) {
  var f = format.format("015,K");
  test.equal(f(42),    "0,000,000,042.0");
  test.equal(f(42e12), "0,000,000,042.0T");
  test.end();
});

tape("format(\",K\") will group thousands for very large numbers", function(test) {
  var f = format.format(",K");
  test.equal(f(42e30), "42,000,000,000,000,000,000T");
  test.end();
});
