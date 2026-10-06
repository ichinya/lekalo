// Synthetic legacy implementation for the issue #136 adoption fixture.
// Behavior contract (the oracle obligations): completeTask is idempotent,
// rejects a non-string or empty id with TypeError("invalid-task-id"),
// rejects an unknown id with RangeError("unknown-task:<id>"), and never
// mutates its input map.
import { MAX_TASKS } from "./limits.mjs";

export function registerTask(tasks, task) {
  if (tasks.size >= MAX_TASKS) {
    throw new RangeError("task-limit");
  }
  tasks.set(task.id, task);
  return tasks;
}

export function completeTask(tasks, id) {
  if (typeof id !== "string" || id.length === 0) {
    throw new TypeError("invalid-task-id");
  }
  if (!tasks.has(id)) {
    throw new RangeError("unknown-task:" + id);
  }
  const task = tasks.get(id);
  if (task.state === "done") {
    return task;
  }
  const done = { ...task, state: "done" };
  return done;
}
