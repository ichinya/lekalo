import { makeBase, Base } from "../../base/src/index";
export const base: Base = makeBase("app-1");
export function describeBase(): string {
  return base.id;
}
