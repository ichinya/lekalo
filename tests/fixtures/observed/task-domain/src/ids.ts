// Stable identity helpers of the existing task board implementation.
export const TASK_ID = Symbol("task_id");
export const TASK_STATE = {
  backlog: "backlog",
  focused: "focused",
  done: "done",
} as const;
