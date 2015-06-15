import localeFormat from "../localeFormat";

export default localeFormat({
  decimal: ",",
  thousands: "\xa0",
  grouping: [3],
  currency: ["", "\xa0руб."]
});
