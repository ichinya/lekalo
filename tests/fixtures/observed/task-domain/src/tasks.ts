// The existing task board implementation. Lekalo observes this code; it
// never generates or overwrites it.
import { TASK_ID, TASK_STATE } from "./ids";

export interface Task {
  task_id: string;
  title: string;
  state: keyof typeof TASK_STATE;
}

export function createTask(title: string): Task {
  return { task_id: crypto.randomUUID(), title, state: TASK_STATE.backlog };
}

export function listTasks(all: Task[]): Task[] {
  return [...all].sort((a, b) => a.title.localeCompare(b.title));
}

export function taskFocused(task: Task): Task {
  return { ...task, state: TASK_STATE.focused };
}
