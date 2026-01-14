import {assert, test} from "vitest";
import {formatPrefix} from "../src/index.js";

test("formatPrefix(\"s\", value)(number) formats with the SI prefix appropriate to the specified value", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e-6)(.00042), "420µ");
  assert.strictEqual(formatPrefix(",.0s", 1e-6)(.0042), "4,200µ");
  assert.strictEqual(formatPrefix(",.3s", 1e-3)(.00042), "0.420m");
});

test("formatPrefix(\"s\", value)(number) uses yocto for very small reference values", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e-27)(1e-24), "1y");
});

test("formatPrefix(\"s\", value)(number) uses yotta for very small reference values", () => {
  assert.strictEqual(formatPrefix(",.0s", 1e27)(1e24), "1Y");
});

test("formatPrefix(\"$,s\", value)(number) formats with the specified SI prefix", () => {
  const f = formatPrefix(" $12,.1s", 1e6);
  assert.strictEqual(f(-42e6),  "      −$42.0M");
  assert.strictEqual(f(+4.2e6), "        $4.2M");
});
