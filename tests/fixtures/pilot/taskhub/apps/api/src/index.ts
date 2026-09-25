// Taskhub API surface: a minimal framework-free HTTP router over the
// task domain and the task event stream. The route table is data; the
// committed openapi.json beside this module documents the same surface
// for external consumers. Synthetic pilot fixture (issue #49).

import {
  NewTaskInput,
  Task,
  TASK_STATE,
  createTask,
  transitionTask,
} from "@taskhub/tasks";
import { TaskEvent, encodeTaskEvent } from "@taskhub/events";

/** The closed HTTP method set the router answers with. */
export type HttpMethod = "GET" | "POST";

/** One declared route of the taskhub API. */
export interface Route {
  readonly method: HttpMethod;
  readonly path: string;
  readonly name: string;
}

/** The data shape of the declared route table. */
export interface RouteTable {
  readonly size: number;
  at(index: number): Route | undefined;
  match(method: HttpMethod, path: string): Route | undefined;
}

/** One incoming API request. */
export interface ApiRequest {
  readonly method: HttpMethod;
  readonly path: string;
  readonly body: string;
}

/** One API answer: a status and an encoded body, nothing else. */
export interface ApiResponse {
  readonly status: number;
  readonly body: string;
}

const declared: Route[] = [
  { method: "GET", path: "/tasks", name: "tasks.list" },
  { method: "POST", path: "/tasks", name: "tasks.create" },
  { method: "POST", path: "/tasks/{taskId}/transition", name: "tasks.transition" },
];

/** The declared route table, queryable as pure data. */
export const ROUTES: RouteTable = {
  size: declared.length,
  at(index: number): Route | undefined {
    return declared[index];
  },
  match(method: HttpMethod, path: string): Route | undefined {
    return declared.find((route) => route.method === method && route.path === path);
  },
};

function parseTask(body: string): Task | undefined {
  const parsed: unknown = JSON.parse(body);
  if (typeof parsed !== "object" || parsed === null) {
    return undefined;
  }
  const candidate = parsed as Partial<NewTaskInput>;
  if (typeof candidate.taskId !== "string" || typeof candidate.title !== "string") {
    return undefined;
  }
  return createTask(candidate as NewTaskInput);
}

/** Dispatch one request through the route table. */
export function handle(request: ApiRequest): ApiResponse {
  const route = ROUTES.match(request.method, request.path);
  if (route === undefined) {
    return { status: 404, body: "" };
  }
  if (route.name === "tasks.create") {
    const task: Task | undefined = parseTask(request.body);
    if (task === undefined) {
      return { status: 400, body: "" };
    }
    return { status: 201, body: JSON.stringify(task) };
  }
  if (route.name === "tasks.transition") {
    const parsed: unknown = JSON.parse(request.body);
    const state: TASK_STATE | undefined =
      typeof parsed === "object" && parsed !== null && "state" in parsed
        ? (parsed as { state: TASK_STATE }).state
        : undefined;
    if (state === undefined) {
      return { status: 400, body: "" };
    }
    const moved = transitionTask(
      { taskId: "task-1", title: "demo", state: "backlog" },
      state,
    );
    return { status: 200, body: JSON.stringify(moved) };
  }
  const event: TaskEvent = {
    id: "event-1",
    kind: "created",
    occurredAt: "2026-01-01T00:00:00.000Z",
    payload: { taskId: "task-1", title: "demo" },
  };
  return { status: 200, body: JSON.stringify(encodeTaskEvent(event)) };
}
