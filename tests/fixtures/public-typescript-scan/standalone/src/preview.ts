import { score } from "./score.js";
export function preview(value: string): string {
  return new score(value).value;
}
