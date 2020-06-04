var tape = require("tape"),
    format = require("../");

tape("format(\"B\") outputs binary-prefix notation with default precision 6", function(test) {
  var f = format.format("B");
  test.equal(f(0), "0.00000");
  test.equal(f(1), "1.00000");
  test.equal(f(10), "10.0000");
  test.equal(f(100), "100.000");
  test.equal(f(999.5), "999.500");
  test.equal(f(1000), "1000.00");
  test.equal(f(999500), "976.074Ki");
  test.equal(f(1000000), "976.563Ki");
  test.equal(f(100), "100.000");
  test.equal(f(1024), "1.00000Ki");
  test.equal(f(1280), "1.25000Ki");
  test.equal(f(1536.512), "1.50050Ki");
  test.equal(f(.00001), "0.00001");
  test.equal(f(.000001), "0.000001");
  test.end();
});

tape("format(\"[.precision]B\") outputs binary-prefix notation with precision significant digits", function(test) {
  var f1 = format.format(".3B");
  test.equal(f1(0), "0.00");
  test.equal(f1(1), "1.00");
  test.equal(f1(10), "10.0");
  test.equal(f1(100), "100");
  test.equal(f1(1023.5), "1.00Ki");
  test.equal(f1(1048576), "1.00Mi");
  test.equal(f1(1048064), "1.00Mi");
  test.equal(f1(1040000), "0.99Mi");
  test.equal(f1(1024), "1.00Ki");
  test.equal(f1(1536), "1.50Ki");
  test.equal(f1(152567808), "146Mi"); // 145.5Mi
  test.equal(f1(152567807), "145Mi"); // 145.499999Mi
  test.equal(f1(100 * Math.pow(2, 80)), "100Yi");
  var f2 = format.format(".4B");
  test.equal(f2(999.5), "999.5");
  test.equal(f2(1000), "1000");
  test.equal(f2(999.5 * 1024), "999.5Ki");
  test.equal(f2(1000 * 1024), "1000Ki");
  test.end();
});

tape("format(\"B\") formats numbers smaller than 1", function(test) {
  var f = format.format(".8B");
  test.equal(f(1.29e-6), "0.0000013"); // Note: rounded!
  test.equal(f(1.29e-5), "0.0000129");
  test.equal(f(1.29e-4), "0.0001290");
  test.equal(f(1.29e-3), "0.0012900");
  test.equal(f(1.29e-2), "0.0129000");
  test.equal(f(1.29e-1), "0.1290000");
  test.end();
});

tape("format(\"B\") formats numbers larger than 2**80 with yobi", function(test) {
  var f = format.format(".8B");
  test.equal(f(1.23 * Math.pow(2, 70)), "1.2300000Zi");
  test.equal(f(12.3 * Math.pow(2, 70)), "12.300000Zi");
  test.equal(f(123 * Math.pow(2, 70)), "123.00000Zi");
  test.equal(f(1.23 * Math.pow(2, 80)), "1.2300000Yi");
  test.equal(f(12.3 * Math.pow(2, 80)), "12.300000Yi");
  test.equal(f(123 * Math.pow(2, 80)), "123.00000Yi");
  test.equal(f(1230 * Math.pow(2, 80)), "1230.0000Yi");
  test.equal(f(12300 * Math.pow(2, 80)), "12300.000Yi");
  test.equal(f(123000 * Math.pow(2, 80)), "123000.00Yi");
  test.equal(f(1230000 * Math.pow(2, 80)), "1230000.0Yi");
  test.equal(f(1234567.89 * Math.pow(2, 80)), "1234567.9Yi");
  test.equal(f(-1.23 * Math.pow(2, 70)), "-1.2300000Zi");
  test.equal(f(-12.3 * Math.pow(2, 70)), "-12.300000Zi");
  test.equal(f(-123 * Math.pow(2, 70)), "-123.00000Zi");
  test.equal(f(-1.23 * Math.pow(2, 80)), "-1.2300000Yi");
  test.equal(f(-12.3 * Math.pow(2, 80)), "-12.300000Yi");
  test.equal(f(-123 * Math.pow(2, 80)), "-123.00000Yi");
  test.equal(f(-1230 * Math.pow(2, 80)), "-1230.0000Yi");
  test.equal(f(-12300 * Math.pow(2, 80)), "-12300.000Yi");
  test.equal(f(-123000 * Math.pow(2, 80)), "-123000.00Yi");
  test.equal(f(-1230000 * Math.pow(2, 80)), "-1230000.0Yi");
  test.equal(f(-1234567.89 * Math.pow(2, 80)), "-1234567.9Yi");
  test.end();
});

tape("format(\"$B\") outputs binary-prefix notation with a currency symbol", function(test) {
  var f1 = format.format("$.2B");
  test.equal(f1(0), "$0.0");
  test.equal(f1(256000), "$250Ki");
  test.equal(f1(-250 * Math.pow(2, 20)), "-$250Mi");
  test.equal(f1(250 * Math.pow(2, 30)), "$250Gi");
  var f2 = format.format("$.3B");
  test.equal(f2(0), "$0.00");
  test.equal(f2(1), "$1.00");
  test.equal(f2(10), "$10.0");
  test.equal(f2(100), "$100");
  test.equal(f2(999.4), "$999");
  test.equal(f2(999.5), "$0.98Ki");
  test.equal(f2(.9995 * Math.pow(2, 10)), "$1.00Ki");
  test.equal(f2(.9995 * Math.pow(2, 20)), "$1.00Mi");
  test.equal(f2(1024), "$1.00Ki");
  test.equal(f2(1535.5), "$1.50Ki");
  test.equal(f2(152567808), "$146Mi");
  test.equal(f2(152567807), "$145Mi");
  test.equal(f2(100 * Math.pow(2, 80)), "$100Yi");
  test.equal(f2(.000001), "$0.000001");
  test.equal(f2(.009995), "$0.01");
  var f3 = format.format("$.4B");
  test.equal(f3(1023), "$1023");
  test.equal(f3(1023 * Math.pow(2, 10)), "$1023Ki");
  var f4 = format.format("$.5B");
  test.equal(f4(1023.5), "$0.9995Ki");
  test.equal(f4(1023.5 * Math.pow(2, 10)), "$0.9995Mi");
  test.end();
});

tape("format(\"B\") binary-prefix notation precision is consistent for small and large numbers", function(test) {
  var f1 = format.format(".0B");
  test.equal(f1(1e0 * Math.pow(2, 0)), "1");
  test.equal(f1(1e1 * Math.pow(2, 0)), "10");
  test.equal(f1(1e2 * Math.pow(2, 0)), "100");
  test.equal(f1(1e0 * Math.pow(2, 10)), "1Ki");
  test.equal(f1(1e1 * Math.pow(2, 10)), "10Ki");
  test.equal(f1(1e2 * Math.pow(2, 10)), "100Ki");
  var f2 = format.format(".4B");
  test.equal(f2(1e+0 * Math.pow(2, 0)), "1.000");
  test.equal(f2(1e+1 * Math.pow(2, 0)), "10.00");
  test.equal(f2(1e+2 * Math.pow(2, 0)), "100.0");
  test.equal(f2(1e+0 * Math.pow(2, 10)), "1.000Ki");
  test.equal(f2(1e+1 * Math.pow(2, 10)), "10.00Ki");
  test.equal(f2(1e+2 * Math.pow(2, 10)), "100.0Ki");
  test.end();
});

tape("format(\"0[width],B\") will group thousands due to zero fill", function(test) {
  var f = format.format("020,B");
  test.equal(f(42),    "000,000,000,042.0000");
  test.equal(f(42 * Math.pow(2, 40)), "0,000,000,042.0000Ti");
  test.end();
});

tape("format(\",B\") will group thousands for very large numbers", function(test) {
  var f = format.format(",B");
  test.equal(f(42e6 * Math.pow(2, 80)), "42,000,000Yi");
  test.end();
});

tape("format(\"B\") will not hang on Infinity", function(test) {
  var f = format.format("B");
  test.equal(f(Infinity), "Infinity");
  test.equal(f(-Infinity), "-Infinity");
  test.end();
});
