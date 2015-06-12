var tape = require("tape"),
    format = require("../");

require("./inDelta");

tape("round(x) returns a number", function(test) {
  test.equal(typeof format.round(42), "number");
  test.end();
});

tape("round(0) returns zero", function(test) {
  test.equal(format.round(0), 0);
  test.end();
});

tape("round(NaN) returns NaN", function(test) {
  test.ok(isNaN(format.round(NaN)));
  test.end();
});

tape("round(±Infinity) returns ±Infinity", function(test) {
  test.equal(format.round(Infinity), Infinity);
  test.equal(format.round(-Infinity), -Infinity);
  test.end();
});

tape("round(x) returns integers", function(test) {
  test.equal(format.round(10.6), 11);
  test.equal(format.round(10.4), 10);
  test.equal(format.round(0.6), 1);
  test.equal(format.round(0.4), 0);
  test.equal(format.round(-0.6), -1);
  test.equal(format.round(-0.4), 0);
  test.equal(format.round(-10.6), -11);
  test.equal(format.round(-10.4), -10);
  test.end();
});

tape("round(x, p) rounds to the specified decimal place p when p > 0", function(test) {
  test.inDelta(format.round(10.56, 1), 10.6);
  test.inDelta(format.round(10.54, 1), 10.5);
  test.inDelta(format.round(0.56, 1), 0.6);
  test.inDelta(format.round(0.54, 1), 0.5);
  test.inDelta(format.round(-0.56, 1), -0.6);
  test.inDelta(format.round(-0.54, 1), -0.5);
  test.inDelta(format.round(-10.56, 1), -10.6);
  test.inDelta(format.round(-10.54, 1), -10.5);
  test.inDelta(format.round(10.556, 2), 10.56);
  test.inDelta(format.round(10.554, 2), 10.55);
  test.inDelta(format.round(0.556, 2), 0.56);
  test.inDelta(format.round(0.554, 2), 0.55);
  test.inDelta(format.round(-0.556, 2), -0.56);
  test.inDelta(format.round(-0.554, 2), -0.55);
  test.inDelta(format.round(-10.556, 2), -10.56);
  test.inDelta(format.round(-10.554, 2), -10.55);
  test.end();
});

tape("round(x, p) rounds to the specified significant digits -p when p < 0", function(test) {
  test.equal(format.round(123.45, -1), 120);
  test.equal(format.round(345.67, -1), 350);
  test.equal(format.round(-123.45, -1), -120);
  test.equal(format.round(-345.67, -1), -350);
  test.equal(format.round(123.45, -2), 100);
  test.equal(format.round(456.78, -2), 500);
  test.equal(format.round(-123.45, -2), -100);
  test.equal(format.round(-456.78, -2), -500);
  test.equal(format.round(123.45, -3), 0);
  test.equal(format.round(567.89, -3), 1000);
  test.equal(format.round(-123.45, -3), 0);
  test.equal(format.round(-567.89, -3), -1000);
  test.end();
});
