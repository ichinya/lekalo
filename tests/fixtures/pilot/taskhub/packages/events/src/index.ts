// Taskhub external-task event envelope: the wire shape every consumer
// of taskhub activity agrees on. Synthetic pilot fixture (issue #49).
//
// The envelope keeps transport concerns out of the payload union: time
// travels as an ISO-8601 string (`occurredAt`), and the codec functions
// own the JSON envelope body end to end.

export type TaskEventKind = "created" | "transitioned";

/** One taskhub activity record, ready for transport. */
export interface TaskEvent {
  readonly id: string;
  readonly kind: TaskEventKind;
  readonly occurredAt: string;
  readonly payload: TaskEventPayload;
}

/** Payload of the `created` kind. */
export interface TaskCreatedPayload {
  readonly taskId: string;
  readonly title: string;
}

/** Payload of the `transitioned` kind. */
export interface TaskTransitionedPayload {
  readonly taskId: string;
  readonly from: string;
  readonly to: string;
}

/** The closed payload union of every TaskEvent. */
export type TaskEventPayload = TaskCreatedPayload | TaskTransitionedPayload;

/** The transport envelope the codec functions produce and consume. */
export interface Envelope {
  readonly contentType: string;
  readonly body: string;
}

/** Encode one TaskEvent into its transport envelope. */
export function encodeTaskEvent(event: TaskEvent): Envelope {
  return {
    contentType: "application/json; type=taskhub.task-event",
    body: JSON.stringify(event),
  };
}

/** Decode one transport envelope back into a TaskEvent. */
export function decodeTaskEvent(raw: Envelope): TaskEvent | undefined {
  if (!raw.contentType.startsWith("application/json")) {
    return undefined;
  }
  const parsed: unknown = JSON.parse(raw.body);
  if (typeof parsed !== "object" || parsed === null) {
    return undefined;
  }
  const candidate = parsed as Partial<TaskEvent>;
  if (
    typeof candidate.id !== "string"
    || typeof candidate.occurredAt !== "string"
    || (candidate.kind !== "created" && candidate.kind !== "transitioned")
    || typeof candidate.payload !== "object"
    || candidate.payload === null
  ) {
    return undefined;
  }
  return parsed as TaskEvent;
}
