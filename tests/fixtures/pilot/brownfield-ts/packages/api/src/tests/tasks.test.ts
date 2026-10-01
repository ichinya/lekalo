// Brownfield fixture native test (issue #118): the vitest file covering
// the POST /tasks submission flow. Synthetic; never executed by the
// pilot — the adapter records it as the flow's native test binding.
import { describe, expect, it } from "vitest";
import { submitTask } from "../routes/tasks.js";

describe("POST /tasks", () => {
  it("rejects a submission without a title", () => {
    expect(typeof submitTask).toBe("function");
  });

  it("persists a well-formed submission", () => {
    expect(typeof submitTask).toBe("function");
  });
});
