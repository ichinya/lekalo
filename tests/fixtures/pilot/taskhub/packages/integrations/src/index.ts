// Taskhub integration contracts: the payloads and the narrow client
// interface every delivery target agrees on. Synthetic pilot fixture
// (issue #49). This package owns no behavior by design — it is the
// boundary the sync worker codes against.

export type SyncTargetKind = "calendar" | "webhook";

/** One calendar sync payload (a day summary for one calendar). */
export interface CalendarSyncPayload {
  readonly calendarId: string;
  readonly date: string;
  readonly title: string;
}

/** One webhook sync payload (an opaque body for one endpoint). */
export interface WebhookSyncPayload {
  readonly endpoint: string;
  readonly body: string;
}

/** The closed payload union of every integration contract. */
export type SyncPayload = CalendarSyncPayload | WebhookSyncPayload;

/** The acknowledgement a delivered payload earns. */
export interface DeliveryAck {
  readonly target: string;
  readonly acceptedAt: string;
}

/** The narrow client interface every integration target implements. */
export interface IntegrationClient {
  readonly kind: SyncTargetKind;
  readonly endpoint: string;
  deliver(payload: SyncPayload): DeliveryAck;
}
