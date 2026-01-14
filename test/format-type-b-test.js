import {assert, test} from "vitest";
import {format} from "../src/index.js";

test("format(\"b\") binary", () => {
  assert.strictEqual(format("b")(10), "1010");
});

test("format(\"#b\") binary with prefix", () => {
  assert.strictEqual(format("#b")(10), "0b1010");
});
