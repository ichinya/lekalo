import { describe, it } from "../test-dsl";
import { registerRoutes } from "../routes";

describe("routes", () => {
  it("registers handlers", () => {
    registerRoutes();
  });
  it("dynamic name " + String(1 + 1), () => {});
});
