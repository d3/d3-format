import formatLocale from "./formatLocale";

export default formatLocale({
  decimal: ",",
  thousands: ".",
  grouping: [3],
  currency: ["", "\xa0ден."]
});
