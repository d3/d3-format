import {assert, test} from "vitest";
import {precisionFixed} from "../src/index.js";

test("precisionFixed(number) returns the expected value", () => {
  assert.strictEqual(precisionFixed(8.9), 0);
  assert.strictEqual(precisionFixed(1.1), 0);
  assert.strictEqual(precisionFixed(0.89), 1);
  assert.strictEqual(precisionFixed(0.11), 1);
  assert.strictEqual(precisionFixed(0.089), 2);
  assert.strictEqual(precisionFixed(0.011), 2);
});

test("precisionFixed(0) returns NaN", () => {
  assert.isNaN(precisionFixed(0));
});

test("precisionFixed(NaN) returns NaN", () => {
  assert.isNaN(precisionFixed(NaN));
});

test("precisionFixed(Infinity) returns NaN", () => {
  assert.isNaN(precisionFixed(Infinity));
  assert.isNaN(precisionFixed(-Infinity));
});
