var tape = require("tape"),
    format = require("../");

tape("format(\"x\") returns the empty string for non-integers", function(test) {
  test.equal(format.format("x")(2.4), "");
  test.end();
});

tape("format(\"x\") returns the expected hexadecimal (lowercase) string", function(test) {
  test.equal(format.format("x")(0xdeadbeef), "deadbeef");
  test.end();
});

tape("format(\"#x\") returns the expected hexadecimal (lowercase) string with prefix", function(test) {
  test.equal(format.format("#x")(0xdeadbeef), "0xdeadbeef");
  test.end();
});

tape("format(\",x\") groups thousands", function(test) {
  test.equal(format.format(",x")(0xdeadbeef), "de,adb,eef");
  test.end();
});

tape("format(\",x\") groups thousands", function(test) {
  test.equal(format.format(",x")(0xdeadbeef), "de,adb,eef");
  test.end();
});

tape("format(\"#,x\") does not group the prefix", function(test) {
  test.equal(format.format("#,x")(0xadeadbeef), "0xade,adb,eef");
  test.end();
});

tape("format(\"+#x\") puts the sign before the prefix", function(test) {
  test.equal(format.format("+#x")(0xdeadbeef),  "+0xdeadbeef");
  test.equal(format.format("+#x")(-0xdeadbeef), "-0xdeadbeef");
  test.equal(format.format(" #x")(0xdeadbeef),  " 0xdeadbeef");
  test.equal(format.format(" #x")(-0xdeadbeef), "-0xdeadbeef");
  test.end();
});

tape("format(\"$,x\") formats hexadecimal currency", function(test) {
  test.equal(format.format("$,x")(0xdeadbeef), "$de,adb,eef");
  test.end();
});

tape("format(\"[.precision]x\") always has precision zero", function(test) {
  test.equal(format.format(".2x")(0xdeadbeef), "deadbeef");
  test.end();
});

tape("format(\"X\") returns the expected hexadecimal (uppercase) string", function(test) {
  test.equal(format.format("X")(0xdeadbeef), "DEADBEEF");
  test.end();
});

tape("format(\"#X\") returns the expected hexadecimal (uppercase) string with prefix", function(test) {
  test.equal(format.format("#X")(0xdeadbeef), "0xDEADBEEF");
  test.end();
});

tape("format(\"#[width]x\") considers the prefix", function(test) {
  test.equal(format.format("20x")(0xdeadbeef),   "            deadbeef");
  test.equal(format.format("#20x")(0xdeadbeef),  "          0xdeadbeef");
  test.equal(format.format("020x")(0xdeadbeef),  "000000000000deadbeef");
  test.equal(format.format("#020x")(0xdeadbeef), "0x0000000000deadbeef");
  test.end();
});