import { add, Shape } from "./math";
import defaults from "./math";

export class UserRepository implements Shape {
  kind = "user";
  area(): number { return 0; }

  static defaultName = "user";

  find(id: number): number {
    return add(id, 0) + Number(defaults !== undefined);
  }
}

export const repo = new UserRepository();
export type ShapeRef = Shape;
