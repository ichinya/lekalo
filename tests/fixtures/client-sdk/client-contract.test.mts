/**
 * The wire-behavior contract tests of the generated client (issue #72
 * acceptance: the client passes contract tests). The client is
 * executed against a scripted fake transport that captures every
 * request and replays scripted responses; assertions cover the
 * request shape (method, path), the declared error identity
 * preservation, and the retry-safety posture. Node built-ins only.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

type Request = {
  method: string;
  path: string;
  query: Record<string, string>;
  headers: Record<string, string>;
};

/** A scripted transport: captures requests, replays responses. */
function fakeTransport(responses: Array<unknown>) {
  const sent: Request[] = [];
  let index = 0;
  return {
    sent,
    async send(request: Request) {
      sent.push(request);
      const scripted = responses[Math.min(index, responses.length - 1)];
      index += 1;
      return scripted as { status: number; headers: Record<string, string>; body?: string };
    },
  };
}

/** Import the generated client module through the fixture source. */
async function importClient() {
  const tsModule = new URL(
    "./vue-consumer/generated/planner.client.ts",
    import.meta.url,
  );
  // Node 24 strips types for .ts imports natively.
  return import(tsModule.href, { with: { type: "flavor-json" } }).catch(() =>
    import(tsModule.href),
  );
}

test("wire: list_tasks sends GET /tasks through the injected transport", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([{ status: 200, headers: {}, body: "[]" }]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.listTasks();
  assert.equal(result.ok, true);
  assert.equal(transport.sent.length, 1);
  assert.equal(transport.sent[0].method, "GET");
  assert.equal(transport.sent[0].path, "https://unit.invalid/tasks");
});

test("identity: a declared error preserves the exact id, code, and category", async () => {
  const { LekaloClient } = await importClient();
  const conflict = {
    ok: false,
    error: {
      id: "planner.focus_conflict",
      code: "LEK-ERR-001",
      category: "conflict",
      payload: { task_id: "0b6e3d4e-8f2a-4c31-9d5f-2a7b8c9d0e1f" },
    },
  };
  const transport = fakeTransport([{ status: 409, headers: {}, body: JSON.stringify(conflict) }]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.plannerEndpointFocusTask("key-1");
  assert.equal(result.ok, false);
  // The semantic union surfaces the declared identity verbatim; the
  // private focused_by field never appears.
  assert.equal(result.error.code, "LEK-ERR-001");
  assert.equal(result.error.id, "planner.focus_conflict");
  assert.equal(result.error.category, "conflict");
});

test("retry-safety: an infrastructure failure never masquerades as declared", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([{ status: 503, headers: {}, body: "not json" }]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.plannerEndpointCountFocused();
  assert.equal(result.ok, false);
  // A 503 without a recognized declared body is the unknown channel,
  // even though the operation maps planner.store_unavailable to 503:
  // a status alone is insufficient to claim a semantic error.
  assert.equal(
    "infrastructure" in (result as Record<string, unknown>),
    true,
    "unknown failures travel the infrastructure channel",
  );
});
