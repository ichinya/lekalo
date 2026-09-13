// Maintained implementation: task identity and state (issue #40 fixture).
// This file is user-owned; Lekalo never writes or rewrites it.
export type TaskState = "backlog" | "focused" | "done";

export function newTaskId(raw: string): string {
  return raw;
}
