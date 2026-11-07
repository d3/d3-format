import {assert, test} from "vitest";
import {format, precisionRound} from "d3-format";

test("precisionRound(step, max) returns the expected value", () => {
  assert.strictEqual(precisionRound(0.1, 1.1), 2); // "1.0", "1.1"
  assert.strictEqual(precisionRound(0.01, 0.99), 2); // "0.98", "0.99"
  assert.strictEqual(precisionRound(0.01, 1.00), 2); // "0.99", "1.0"
  assert.strictEqual(precisionRound(0.01, 1.01), 3); // "1.00", "1.01"
});

test("precisionRound(0, max) returns NaN", () => {
  assert.isNaN(precisionRound(0, 1));
});

test("precisionRound(NaN, max) returns NaN", () => {
  assert.isNaN(precisionRound(NaN, 1));
});

test("precisionRound(Infinity, max) returns NaN", () => {
  assert.isNaN(precisionRound(Infinity, 1));
  assert.isNaN(precisionRound(-Infinity, 1));
});

test("precisionRound(step, step) returns a usable significant precision", () => {
  for (const step of [0.01, 0.1, 1, 10, 100]) {
    for (const sign of [-1, 1]) {
      const precision = precisionRound(step * sign, step * sign);
      assert.strictEqual(precision, 1);
      assert.strictEqual(+format("." + precision + "r")(step), step);
    }
  }
});
