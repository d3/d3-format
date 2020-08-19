var tape = require("tape"),
    d3 = require("../");

tape("formatLocale(…) can format numbers using ar-001 locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-001"));
  test.equal(locale.format("$,.2f")(-1234.56), "−١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-AE locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-AE"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.إ.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-BH locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-BH"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ب.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-DJ locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-DJ"));
  test.equal(locale.format("$,.2f")(1234.56), "\u200fFdj ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-DZ locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-DZ"));
  test.equal(locale.format("$,.2f")(1234.56), "د.ج. 1.234,56");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-EG locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-EG"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ج.م.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-EH locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-EH"));
  test.equal(locale.format("$,.2f")(1234.56), "د.م. 1,234.56");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-ER locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-ER"));
  test.equal(locale.format("$,.2f")(1234.56), "Nfk ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-IL locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-IL"));
  test.equal(locale.format("$,.2f")(1234.56), "₪ ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-IQ locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-IQ"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ع.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-JO locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-JO"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.أ.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-KM locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-KM"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ف.ج.ق.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-KW locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-KW"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ د.ك.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-LB locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-LB"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ل.ل.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-MA locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-MA"));
  test.equal(locale.format("$,.2f")(1234.56), "د.م. 1.234,56");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-MR locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-MR"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ أ.م.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-OM locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-OM"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ع.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-PS locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-PS"));
  test.equal(locale.format("$,.2f")(1234.56), "₪ ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-QA locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-QA"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ق.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-SA locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-SA"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.س.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-SD locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-SD"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ج.س.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-SO locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-SO"));
  test.equal(locale.format("$,.2f")(1234.56), "‏S ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-SS locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-SS"));
  test.equal(locale.format("$,.2f")(1234.56), "£ ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-SY locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-SY"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ل.س.");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-TD locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-TD"));
  test.equal(locale.format("$,.2f")(1234.56), "\u200fFCFA ١٬٢٣٤٫٥٦");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-TN locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-TN"));
  test.equal(locale.format("$,.2f")(1234.56), "د.ت. 1.234,56");
  test.end();
});

tape("formatLocale(…) can format numbers using ar-YE locale.", function(test) {
  var locale = d3.formatLocale(require("../locale/ar-YE"));
  test.equal(locale.format("$,.2f")(1234.56), "١٬٢٣٤٫٥٦ ر.ى.");
  test.end();
});
