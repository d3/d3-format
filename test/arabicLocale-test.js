import {readFileSync} from "node:fs";
import {assert, test} from "vitest";
import {formatLocale} from "../src/index.js";

function locale(locale) {
  return formatLocale(JSON.parse(readFileSync(`./locale/${locale}.json`, "utf8")));
}

test("formatLocale() can format numbers using ar-001 locale", () => {
  assert.strictEqual(locale("ar-001").format("$,.2f")(-1234.56), "−١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-AE locale", () => {
  assert.strictEqual(locale("ar-AE").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.إ.");
});

test("formatLocale() can format numbers using ar-BH locale", () => {
  assert.strictEqual(locale("ar-BH").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ب.");
});

test("formatLocale() can format numbers using ar-DJ locale", () => {
  assert.strictEqual(locale("ar-DJ").format("$,.2f")(1234.56), "\u200fFdj ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-DZ locale", () => {
  assert.strictEqual(locale("ar-DZ").format("$,.2f")(1234.56), "د.ج. 1.234,56");
});

test("formatLocale() can format numbers using ar-EG locale", () => {
  assert.strictEqual(locale("ar-EG").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ج.م.");
});

test("formatLocale() can format numbers using ar-EH locale", () => {
  assert.strictEqual(locale("ar-EH").format("$,.2f")(1234.56), "د.م. 1,234.56");
});

test("formatLocale() can format numbers using ar-ER locale", () => {
  assert.strictEqual(locale("ar-ER").format("$,.2f")(1234.56), "Nfk ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-IL locale", () => {
  assert.strictEqual(locale("ar-IL").format("$,.2f")(1234.56), "₪ ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-IQ locale", () => {
  assert.strictEqual(locale("ar-IQ").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ع.");
});

test("formatLocale() can format numbers using ar-JO locale", () => {
  assert.strictEqual(locale("ar-JO").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.أ.");
});

test("formatLocale() can format numbers using ar-KM locale", () => {
  assert.strictEqual(locale("ar-KM").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ف.ج.ق.");
});

test("formatLocale() can format numbers using ar-KW locale", () => {
  assert.strictEqual(locale("ar-KW").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ك.");
});

test("formatLocale() can format numbers using ar-LB locale", () => {
  assert.strictEqual(locale("ar-LB").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ل.ل.");
});

test("formatLocale() can format numbers using ar-MA locale", () => {
  assert.strictEqual(locale("ar-MA").format("$,.2f")(1234.56), "د.م. 1.234,56");
});

test("formatLocale() can format numbers using ar-MR locale", () => {
  assert.strictEqual(locale("ar-MR").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ أ.م.");
});

test("formatLocale() can format numbers using ar-OM locale", () => {
  assert.strictEqual(locale("ar-OM").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ع.");
});

test("formatLocale() can format numbers using ar-PS locale", () => {
  assert.strictEqual(locale("ar-PS").format("$,.2f")(1234.56), "₪ ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-QA locale", () => {
  assert.strictEqual(locale("ar-QA").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ق.");
});

test("formatLocale() can format numbers using ar-SA locale", () => {
  assert.strictEqual(locale("ar-SA").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.س.");
});

test("formatLocale() can format numbers using ar-SD locale", () => {
  assert.strictEqual(locale("ar-SD").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ج.س.");
});

test("formatLocale() can format numbers using ar-SO locale", () => {
  assert.strictEqual(locale("ar-SO").format("$,.2f")(1234.56), "‏S ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-SS locale", () => {
  assert.strictEqual(locale("ar-SS").format("$,.2f")(1234.56), "£ ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-SY locale", () => {
  assert.strictEqual(locale("ar-SY").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ل.س.");
});

test("formatLocale() can format numbers using ar-TD locale", () => {
  assert.strictEqual(locale("ar-TD").format("$,.2f")(1234.56), "\u200fFCFA ١٬٢٣٤٫٥٦");
});

test("formatLocale() can format numbers using ar-TN locale", () => {
  assert.strictEqual(locale("ar-TN").format("$,.2f")(1234.56), "د.ت. 1.234,56");
});

test("formatLocale() can format numbers using ar-YE locale", () => {
  assert.strictEqual(locale("ar-YE").format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ى.");
});
