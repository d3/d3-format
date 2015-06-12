import formatLocale from "./formatLocale";

export default formatLocale({
  decimal: ",",
  thousands: "\xa0",
  grouping: [3],
  currency: ["", "\xa0руб."]
});
