import {assert, test} from "vitest";
import {format} from "d3-format";

test("format(\"p\") can output a percentage", () => {
  const f = format("p");
  assert.strictEqual(f(.00123), "0.123000%");
  assert.strictEqual(f(.0123), "1.23000%");
  assert.strictEqual(f(.123), "12.3000%");
  assert.strictEqual(f(.234), "23.4000%");
  assert.strictEqual(f(1.23), "123.000%");
  assert.strictEqual(f(-.00123), "−0.123000%");
  assert.strictEqual(f(-.0123), "−1.23000%");
  assert.strictEqual(f(-.123), "−12.3000%");
  assert.strictEqual(f(-1.23), "−123.000%");
});

test("format(\"+p\") can output a percentage with rounding and sign", () => {
  const f = format("+.2p");
  assert.strictEqual(f(.00123), "+0.12%");
  assert.strictEqual(f(.0123), "+1.2%");
  assert.strictEqual(f(.123), "+12%");
  assert.strictEqual(f(1.23), "+120%");
  assert.strictEqual(f(-.00123), "−0.12%");
  assert.strictEqual(f(-.0123), "−1.2%");
  assert.strictEqual(f(-.123), "−12%");
  assert.strictEqual(f(-1.23), "−120%");
});

test("format(\"p\") preserves finite magnitudes when percentage scaling overflows", () => {
  assert.strictEqual(format(".3p")(1e307), "1" + "0".repeat(309) + "%");
  assert.strictEqual(format(".3p")(1.234e307), "123" + "0".repeat(307) + "%");
  assert.strictEqual(format("+.3p")(-1.234e307), "−123" + "0".repeat(307) + "%");
  assert.strictEqual(format(".3p")(Number.MAX_VALUE), "180" + "0".repeat(308) + "%");
  assert.strictEqual(format(".1p")(Number.MAX_VALUE), "2" + "0".repeat(310) + "%");
  assert.strictEqual(format("(.3p")(-1e307), "(" + "1" + "0".repeat(309) + "%)");
});

test("format(\"p\") retains non-finite percentage representations", () => {
  assert.strictEqual(format(".3p")(Infinity), "Infinity%");
  assert.strictEqual(format(".3p")(-Infinity), "−Infinity%");
  assert.strictEqual(format(".3p")(NaN), "NaN%");
});
