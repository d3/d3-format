import {assert, test} from "vitest";
import {format} from "../src/index.js";

test("format(\"x\") returns the expected hexadecimal (lowercase) string", () => {
  assert.strictEqual(format("x")(0xdeadbeef), "deadbeef");
});

test("format(\"#x\") returns the expected hexadecimal (lowercase) string with prefix", () => {
  assert.strictEqual(format("#x")(0xdeadbeef), "0xdeadbeef");
});

test("format(\",x\") groups thousands", () => {
  assert.strictEqual(format(",x")(0xdeadbeef), "de,adb,eef");
});

test("format(\",x\") groups thousands", () => {
  assert.strictEqual(format(",x")(0xdeadbeef), "de,adb,eef");
});

test("format(\"#,x\") does not group the prefix", () => {
  assert.strictEqual(format("#,x")(0xadeadbeef), "0xade,adb,eef");
});

test("format(\"+#x\") puts the sign before the prefix", () => {
  assert.strictEqual(format("+#x")(0xdeadbeef),  "+0xdeadbeef");
  assert.strictEqual(format("+#x")(-0xdeadbeef), "−0xdeadbeef");
  assert.strictEqual(format(" #x")(0xdeadbeef),  " 0xdeadbeef");
  assert.strictEqual(format(" #x")(-0xdeadbeef), "−0xdeadbeef");
});

test("format(\"$,x\") formats hexadecimal currency", () => {
  assert.strictEqual(format("$,x")(0xdeadbeef), "$de,adb,eef");
});

test("format(\"[.precision]x\") always has precision zero", () => {
  assert.strictEqual(format(".2x")(0xdeadbeef), "deadbeef");
  assert.strictEqual(format(".2x")(-4.2), "−4");
});

test("format(\"x\") rounds non-integers", () => {
  assert.strictEqual(format("x")(2.4), "2");
});

test("format(\"x\") can format negative zero as zero", () => {
  assert.strictEqual(format("x")(-0), "0");
  assert.strictEqual(format("x")(-1e-12), "0");
});

test("format(\"x\") does not consider -0xeee to be positive", () => {
  assert.strictEqual(format("x")(-0xeee), "−eee");
});

test("format(\"X\") returns the expected hexadecimal (uppercase) string", () => {
  assert.strictEqual(format("X")(0xdeadbeef), "DEADBEEF");
});

test("format(\"#X\") returns the expected hexadecimal (uppercase) string with prefix", () => {
  assert.strictEqual(format("#X")(0xdeadbeef), "0xDEADBEEF");
});

test("format(\"X\") can format negative zero as zero", () => {
  assert.strictEqual(format("X")(-0), "0");
  assert.strictEqual(format("X")(-1e-12), "0");
});

test("format(\"X\") does not consider -0xeee to be positive", () => {
  assert.strictEqual(format("X")(-0xeee), "−EEE");
});

test("format(\"#[width]x\") considers the prefix", () => {
  assert.strictEqual(format("20x")(0xdeadbeef),   "            deadbeef");
  assert.strictEqual(format("#20x")(0xdeadbeef),  "          0xdeadbeef");
  assert.strictEqual(format("020x")(0xdeadbeef),  "000000000000deadbeef");
  assert.strictEqual(format("#020x")(0xdeadbeef), "0x0000000000deadbeef");
});
