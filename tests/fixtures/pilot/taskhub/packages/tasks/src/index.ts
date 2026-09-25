// Taskhub task domain: the record of work the whole workspace agrees
// on. Synthetic pilot fixture (issue #49): every name is fictional.
//
// The exported surface is exactly the shared contract the sync worker
// and the API consume: the Task record, its TASK_STATE lifecycle, the
// TASK_ID key constant, and the create/list/transition functions.

export type TASK_STATE = "backlog" | "focused" | "done";

/** The canonical key constant of the task domain. */
export const TASK_ID = "taskId";

/** One unit of work tracked by taskhub. */
export interface Task {
  readonly taskId: string;
  readonly title: string;
  state: TASK_STATE;
}

/** The input accepted by createTask. */
export interface NewTaskInput {
  readonly taskId: string;
  readonly title: string;
}

/** The bounded page shape listTasks answers with. */
export interface TaskPage {
  readonly total: number;
  readonly open: number;
  at(index: number): Task | undefined;
}

/** Create one backlog task from its input. */
export function createTask(input: NewTaskInput): Task {
  return { taskId: input.taskId, title: input.title, state: "backlog" };
}

/** Page through the recorded tasks (newest first, done last). */
export function listTasks(): TaskPage {
  const recorded: Task[] = [];
  const open = recorded.filter((task) => task.state !== "done");
  return {
    total: recorded.length,
    open: open.length,
    at(index: number): Task | undefined {
      return recorded[index];
    },
  };
}

/** Move one task to the next state and return the moved copy. */
export function transitionTask(task: Task, next: TASK_STATE): Task {
  return { taskId: task.taskId, title: task.title, state: next };
}
