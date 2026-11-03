import {assert, test} from "vitest";
import {format} from "d3-format";

test("format(\"o\") octal", () => {
  assert.strictEqual(format("o")(10), "12");
});

test("format(\"#o\") octal with prefix", () => {
  assert.strictEqual(format("#o")(10), "0o12");
});
