import { createTask, focusTask } from "./tasks";

describe("tasks", () => {
  it("creates a backlog task", () => {
    expect(createTask("t1", "first").state).toBe("backlog");
  });

  it("focuses a task", () => {
    expect(focusTask(createTask("t1", "first")).state).toBe("focused");
  });
});
