export type TaskId = string;
export const MAX_TASKS = 100;
export enum TaskState {
  backlog = "backlog",
  focused = "focused",
  done = "done",
}
