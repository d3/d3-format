import {assert, test} from "vitest";
import {precisionRound} from "../src/index.js";

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
