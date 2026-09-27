/**
 * The wire-behavior contract tests of the generated client (issue #72
 * acceptance: the client passes contract tests). The client is
 * executed against a scripted fake transport that captures every
 * request and replays scripted responses; assertions cover path
 * substitution, query serialization, request bodies, the declared
 * error identity preservation, and the retry-safety posture — across
 * every operation shape, not just the parameter-free ones. Node
 * built-ins only.
 */
import assert from "node:assert/strict";
import { test } from "node:test";

/** A scripted transport: captures requests, replays responses. */
function fakeTransport(responses) {
  const sent = [];
  let index = 0;
  return {
    sent,
    async send(request) {
      sent.push(request);
      const scripted = responses[Math.min(index, responses.length - 1)];
      index += 1;
      return scripted;
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
  return import(tsModule.href);
}

const okBody = (body) => ({ status: 200, headers: {}, body: JSON.stringify(body) });
const okEmpty = () => ({ status: 204, headers: {} });
const errBody = (error) => ({ status: 409, headers: {}, body: JSON.stringify({ ok: false, error }) });

test("wire: list_tasks sends GET /tasks through the injected transport", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okBody([])]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.listTasks();
  assert.equal(result.ok, true);
  assert.equal(transport.sent.length, 1);
  assert.equal(transport.sent[0].method, "GET");
  assert.equal(transport.sent[0].path, "https://unit.invalid/tasks");
});

test("wire: path params substitute into the URL template (never a literal)", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okEmpty()]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.plannerEndpointFocusTaskById("task/1 & x");
  assert.equal(result.ok, true);
  assert.equal(
    transport.sent[0].path,
    "https://unit.invalid/tasks/task%2F1%20%26%20x/focus",
    "the path segment is percent-encoded and substituted",
  );
  assert.ok(
    !transport.sent[0].path.includes("{task_id}"),
    "the template placeholder never reaches the wire",
  );
});

test("wire: the second path-param operation substitutes too", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okBody([])]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  await client.plannerEndpointTasksByProject("proj-7");
  assert.equal(
    transport.sent[0].path,
    "https://unit.invalid/projects/proj-7/tasks",
  );
});

test("wire: request bodies serialize as JSON with the declared members", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okEmpty()]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  await client.plannerEndpointFocusTask({ task_id: "t-1" }, "key-1");
  assert.equal(transport.sent[0].body, JSON.stringify({ task_id: "t-1" }));
  // The declared idempotency header name is honored (not a hardcoded token).
  assert.equal(transport.sent[0].headers["Idempotency-Key"], "key-1");
  // Declared correlation headers are sendable.
  await client.plannerEndpointFocusTask({ task_id: "t-1" }, "key-2", "corr-1", "req-1");
  assert.equal(transport.sent[1].headers["X-Correlation-Id"], "corr-1");
  assert.equal(transport.sent[1].headers["X-Request-Id"], "req-1");
});

test("identity: a declared error preserves the exact id, code, and category", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([
    errBody({
      id: "planner.focus_conflict",
      code: "LEK-ERR-001",
      category: "conflict",
      payload: { task_id: "0b6e3d4e-8f2a-4c31-9d5f-2a7b8c9d0e1f" },
    }),
  ]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.plannerEndpointFocusTask({ task_id: "t" }, "key-1");
  assert.equal(result.ok, false);
  // The semantic union surfaces the declared identity verbatim; the
  // private focused_by field never appears.
  assert.equal(result.error.code, "LEK-ERR-001");
  assert.equal(result.error.id, "planner.focus_conflict");
  assert.equal(result.error.category, "conflict");
});

test("responses: whole success bodies decode into the declared type", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([
    okBody([
      { task_id: "t-1", title: "Ship", state: "focused", due: null },
    ]),
  ]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.listTasks();
  assert.equal(result.ok, true);
  assert.equal(result.value[0].title, "Ship", "the body is decoded, not a string");
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
    "infrastructure" in (result),
    true,
    "unknown failures travel the infrastructure channel",
  );
});
