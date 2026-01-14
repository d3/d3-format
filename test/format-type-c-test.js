import {assert, test} from "vitest";
import {format, formatLocale} from "../src/index.js";

test("format(\"c\") unicode character", () => {
  assert.strictEqual(format("c")("☃"), "☃");
  assert.strictEqual(format("020c")("☃"),  "0000000000000000000☃");
  assert.strictEqual(format(" ^20c")("☃"), "         ☃          ");
  assert.strictEqual(format("$c")("☃"), "$☃");
});

test("format(\"c\") does not localize a decimal point", () => {
  assert.strictEqual(formatLocale({decimal: "/"}).format("c")("."), ".");
});
