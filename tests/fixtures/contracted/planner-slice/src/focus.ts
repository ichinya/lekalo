// Maintained implementation: the focus-task command handler (issue #40).
// Plain TypeScript; the Lekalo Model is the contract, this file is the
// implementation, and no tool may rewrite it.
import { newTaskId, TaskState } from "./ids";

export interface FocusTaskInput {
  taskId: string;
}

export interface FocusTaskResult {
  taskId: string;
  state: TaskState;
}

export function focusTask(input: FocusTaskInput): FocusTaskResult {
  const taskId = newTaskId(input.taskId);
  return { taskId, state: "focused" };
}
