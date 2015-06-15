import localeFormat from "../localeFormat";

export default localeFormat({
  decimal: ",",
  thousands: ".",
  grouping: [3],
  currency: ["", "\xa0€"]
});
