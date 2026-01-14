import {assert, test} from "vitest";
import {formatPrefix} from "../src/index.js";

test("formatPrefix(\",.0s\", value)(number) formats with the SI prefix appropriate to the specified value", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e-6)(.00042), "420µ");
  assert.strictEqual(formatPrefix(",.0s", 1e-6)(.0042), "4,200µ");
});

test("formatPrefix(\",.3s\", value)(number) formats with the SI prefix appropriate to the specified value", () => {
  assert.strictEqual(formatPrefix(",.3s", 1e-3)(.00042), "0.420m");
});

test("formatPrefix(\",.0s\", value)(number) uses yocto for very small reference values", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e-27)(1e-24), "1y");
});

test("formatPrefix(\",.0s\", value)(number) uses yotta for very small reference values", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e27)(1e24), "1Y");
});

test("formatPrefix(\" $12,.1s\", value)(number) formats with the specified SI prefix", () => {
  // The fixed length of 12 is inclusive of the unit 'M'
  const f = formatPrefix(" $12,.1s", 1e6);
  assert.strictEqual(f(-42e6),  "     −$42.0M");
  assert.strictEqual(f(+4.2e6), "       $4.2M");
});

test("formatPrefix(\" $12,.1s\", value)(number) matches format(\" $12,.2s\")(number) when the units are the same", () => {
  // The fixed length of 12 is inclusive of the unit 'M'
  const fp = formatPrefix(" $12,.1s", 1e6);
  const f = format(" $12,.2s");
  assert.strictEqual(fp(+4.2e6), "       $4.2M");
  assert.strictEqual(f(+4.2e6),  "       $4.2M");
});

test("formatPrefix(\"($~s\", value)(number) formats with the SI prefix inside parentheses", () => {
  assert.strictEqual(formatPrefix("($~s", 1e3)(1e3), "$1k");
  assert.strictEqual(formatPrefix("($~s", 1e3)(-1e3), "($1k)");
});
