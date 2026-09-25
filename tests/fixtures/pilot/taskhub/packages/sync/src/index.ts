// Taskhub sync worker flow: consume TaskEvents, translate them into
// integration payloads, and deliver through the narrow client. The
// secrets such a worker would need (sync/calendar credentials) stay in
// the environment; this fixture module only moves typed payloads.
// Synthetic pilot fixture (issue #49).

import { Task } from "@taskhub/tasks";
import { TaskEvent } from "@taskhub/events";
import {
  CalendarSyncPayload,
  IntegrationClient,
  SyncPayload,
  SyncTargetKind,
  WebhookSyncPayload,
} from "@taskhub/integrations";

/** The outcome counters one sync worker run reports. */
export interface SyncOutcome {
  readonly processed: number;
  readonly failed: number;
}

/** Everything toPayload needs to translate one event. */
export interface SyncRequest {
  readonly event: TaskEvent;
  readonly task: Task;
  readonly target: SyncTargetKind;
}

/** Translate one sync request into the matching integration payload. */
export function toPayload(request: SyncRequest): SyncPayload {
  if (request.target === "calendar") {
    const calendar: CalendarSyncPayload = {
      calendarId: "taskhub-sync",
      date: request.event.occurredAt,
      title: request.task.title,
    };
    return calendar;
  }
  const webhook: WebhookSyncPayload = {
    endpoint: "taskhub-sync",
    body: request.event.id,
  };
  return webhook;
}

/** Deliver one sync request through the integration client. */
export function syncEvent(request: SyncRequest, client: IntegrationClient): SyncOutcome {
  const payload = toPayload(request);
  const ack = client.deliver(payload);
  if (ack.target !== client.endpoint) {
    return { processed: 0, failed: 1 };
  }
  return { processed: 1, failed: 0 };
}
