var tape = require("tape"),
    format = require("../");

tape("locale({decimal: decimal}) observes the specified decimal point", function(test) {
  test.equal(format.locale({decimal: "|"}).format("06.2f")(2), "002|00");
  test.equal(format.locale({decimal: "/"}).format("06.2f")(2), "002/00");
  test.end();
});

tape("locale({currency: [prefix, suffix]}) observes the specified currency prefix and suffix", function(test) {
  test.equal(format.locale({decimal: ".", currency: ["฿", ""]}).format("$06.2f")(2), "฿02.00");
  test.equal(format.locale({decimal: ".", currency: ["", "฿"]}).format("$06.2f")(2), "02.00฿");
  test.end();
});

tape("locale({grouping: null}) does not perform any grouping", function(test) {
  test.equal(format.locale({decimal: ".", grouping: null}).format("012,.2f")(2), "000000002.00");
  test.end();
});

tape("locale({grouping: [sizes…]}) observes the specified group sizes", function(test) {
  test.equal(format.locale({decimal: ".", grouping: [3], thousands: ","}).format("012,.2f")(2), "0,000,002.00");
  test.equal(format.locale({decimal: ".", grouping: [2], thousands: ","}).format("012,.2f")(2), "0,00,00,02.00");
  test.equal(format.locale({decimal: ".", grouping: [2, 3], thousands: ","}).format("012,.2f")(2), "00,000,02.00");
  test.end();
});

tape("locale({thousands: separator}) observes the specified group separator", function(test) {
  test.equal(format.locale({decimal: ".", grouping: [3], thousands: " "}).format("012,.2f")(2), "0 000 002.00");
  test.equal(format.locale({decimal: ".", grouping: [3], thousands: "/"}).format("012,.2f")(2), "0/000/002.00");
  test.end();
});
