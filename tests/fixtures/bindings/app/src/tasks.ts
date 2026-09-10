import { TaskId, TaskState } from "./ids";

export interface Task {
  id: TaskId;
  title: string;
  state: TaskState;
}

export function createTask(id: TaskId, title: string): Task {
  return { id, title, state: TaskState.backlog };
}

export function focusTask(task: Task): Task {
  return { ...task, state: TaskState.focused };
}
