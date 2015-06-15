import precisionFixed from "./precisionFixed";
import {exponentOf} from "./formatAutoPrefix";

export default function(step, value) {
  return Math.max(0, exponentOf(value) - Math.floor(Math.log(Math.abs(step)) / Math.LN10));
};
