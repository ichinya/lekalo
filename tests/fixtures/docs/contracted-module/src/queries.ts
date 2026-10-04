// Maintained implementation: task queries (issue #40 fixture).
import type { TaskState } from "./ids.ts";

export interface TaskRow {
  taskId: string;
  title: string;
  state: TaskState;
  due: string | null;
}

export function listTasks(rows: TaskRow[]): TaskRow[] {
  return [...rows];
}

export function countFocused(rows: TaskRow[]): number {
  return rows.filter((row) => row.state === "focused").length;
}
