import { score } from "../../core/src/score.js";
export function preview(value: string): string {
  return new score(value).value;
}
