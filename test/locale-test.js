var fs = require('fs').promises,
    path = require("path"),
    tape = require("tape"),
    d3 = require("../");

tape("formatLocale({decimal: decimal}) observes the specified decimal point", function(test) {
  test.equal(d3.formatLocale({decimal: "|"}).format("06.2f")(2), "002|00");
  test.equal(d3.formatLocale({decimal: "/"}).format("06.2f")(2), "002/00");
  test.end();
});

tape("formatLocale({currency: [prefix, suffix]}) observes the specified currency prefix and suffix", function(test) {
  test.equal(d3.formatLocale({decimal: ".", currency: ["฿", ""]}).format("$06.2f")(2), "฿02.00");
  test.equal(d3.formatLocale({decimal: ".", currency: ["", "฿"]}).format("$06.2f")(2), "02.00฿");
  test.end();
});

tape("formatLocale({currency: [prefix, suffix]}) places the currency suffix after the SI suffix", function(test) {
  test.equal(d3.formatLocale({decimal: ",", currency: ["", " €"]}).format("$.3s")(1.2e9), "1,20G €");
  test.end();
});

tape("formatLocale({grouping: undefined}) does not perform any grouping", function(test) {
  test.equal(d3.formatLocale({decimal: "."}).format("012,.2f")(2), "000000002.00");
  test.end();
});

tape("formatLocale({grouping: [sizes…]}) observes the specified group sizes", function(test) {
  test.equal(d3.formatLocale({decimal: ".", grouping: [3], thousands: ","}).format("012,.2f")(2), "0,000,002.00");
  test.equal(d3.formatLocale({decimal: ".", grouping: [2], thousands: ","}).format("012,.2f")(2), "0,00,00,02.00");
  test.equal(d3.formatLocale({decimal: ".", grouping: [2, 3], thousands: ","}).format("012,.2f")(2), "00,000,02.00");
  test.equal(d3.formatLocale({decimal: ".", grouping: [3, 2, 2, 2, 2, 2, 2], thousands: ","}).format(",d")(1e12), "10,00,00,00,00,000");
  test.end();
});

tape("formatLocale(…) can format numbers using the Indian numbering system.", function(test) {
  var format = d3.formatLocale(require("../locale/en-IN")).format(",");
  test.equal(format(10), "10");
  test.equal(format(100), "100");
  test.equal(format(1000), "1,000");
  test.equal(format(10000), "10,000");
  test.equal(format(100000), "1,00,000");
  test.equal(format(1000000), "10,00,000");
  test.equal(format(10000000), "1,00,00,000");
  test.equal(format(10000000.4543), "1,00,00,000.4543");
  test.equal(format(1000.321), "1,000.321");
  test.equal(format(10.5), "10.5");
  test.equal(format(-10), "-10");
  test.equal(format(-100), "-100");
  test.equal(format(-1000), "-1,000");
  test.equal(format(-10000), "-10,000");
  test.equal(format(-100000), "-1,00,000");
  test.equal(format(-1000000), "-10,00,000");
  test.equal(format(-10000000), "-1,00,00,000");
  test.equal(format(-10000000.4543), "-1,00,00,000.4543");
  test.equal(format(-1000.321), "-1,000.321");
  test.equal(format(-10.5), "-10.5");
  test.end();
});

tape("formatLocale({thousands: separator}) observes the specified group separator", function(test) {
  test.equal(d3.formatLocale({decimal: ".", grouping: [3], thousands: " "}).format("012,.2f")(2), "0 000 002.00");
  test.equal(d3.formatLocale({decimal: ".", grouping: [3], thousands: "/"}).format("012,.2f")(2), "0/000/002.00");
  test.end();
});

tape("formatLocale({percent: percent}) observes the specified percent sign", function(test) {
  test.equal(d3.formatLocale({decimal: ".", percent: "!"}).format("06.2%")(2), "200.00!");
  test.equal(d3.formatLocale({decimal: ".", percent: "﹪"}).format("06.2%")(2), "200.00﹪");
  test.end();
});

tape("formatLocale({minus: minus}) observes the specified minus sign", function(test) {
  test.equal(d3.formatLocale({decimal: ".", minus: "-"}).format("06.2f")(-2), "-02.00");
  test.equal(d3.formatLocale({decimal: ".", minus: "−"}).format("06.2f")(-2), "−02.00");
  test.equal(d3.formatLocale({decimal: ".", minus: "➖"}).format("06.2f")(-2), "➖02.00");
  test.equal(d3.formatLocale({decimal: "."}).format("06.2f")(-2), "-02.00");
  test.end();
});

tape("formatLocale({nan: nan}) observes the specified not-a-number representation", function(test) {
  test.equal(d3.formatLocale({nan: "N/A"}).format("6.2f")(undefined), "   N/A");
  test.equal(d3.formatLocale({nan: "-"}).format("<6.2g")(undefined), "-     ");
  test.equal(d3.formatLocale({}).format(" 6.2f")(undefined), "   NaN");
  test.end();
});

tape("locale data is valid", async function(test) {
  await fs.readdir("locale").then(async locales => {
    for (const locale of locales) {
      await fs.readFile(path.join("locale", locale), "utf8").then(testLocale);
    }
  });
  test.end();

  function testLocale(locale) {
    locale = JSON.parse(locale);
    test.equal("currency" in locale, true);
    test.equal("decimal" in locale, true);
    test.equal("grouping" in locale, true);
    test.equal("thousands" in locale, true);
    locale = d3.formatLocale(locale);
  }
});
