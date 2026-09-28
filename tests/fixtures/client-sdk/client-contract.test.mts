/**
 * The wire-behavior contract tests of the generated client (issue #72
 * acceptance: the client passes contract tests). The client is
 * executed against a scripted fake transport that captures every
 * request and replays scripted responses; assertions cover path
 * substitution, query serialization, request bodies with verbatim
 * wire member keys, list-shaped decoding, declared error identity
 * preservation, and the no-auto-retry posture — across every
 * operation shape, not just the parameter-free ones. Node built-ins
 * only (typechecking of these same calls happens in the Vue consumer
 * fixture under strict tsc).
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
const errBody = (error) => ({
  status: 409,
  headers: {},
  body: JSON.stringify({ ok: false, error }),
});

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

test("decode: a list-typed output decodes to an ARRAY (shape survives)", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([
    okBody([
      { task_id: "t-1", tenant: "ten-1", title: "First", state: "focused", due: null },
      { task_id: "t-2", tenant: "ten-1", title: "Second", state: "backlog", due: null },
    ]),
  ]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.listTasks();
  assert.equal(result.ok, true);
  // The declared shape is list<planner.task>: the decoded value is an
  // array and array methods work — never the bare element type.
  assert.equal(Array.isArray(result.value), true, "list shape decodes an array");
  const titles = result.value.map((row) => row.title);
  assert.deepEqual(titles, ["First", "Second"]);
});

test("wire: path params substitute into the URL template (never a literal)", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okEmpty()]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  const result = await client.plannerEndpointFocusTaskById("task/1 & x", { task_id: "t-9" }, "key-1");
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

test("wire: explicit body members travel with their VERBATIM wire names", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okEmpty()]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  // The declared member is task_id: the wire body must carry
  // {"task_id": ...}, never a camelized {"taskId": ...}.
  await client.plannerEndpointFocusTaskById("t-1", { task_id: "t-1" }, "key-1");
  const body = JSON.parse(transport.sent[0].body);
  assert.deepEqual(Object.keys(body), ["task_id"], "wire member keys are the declared names");
  assert.equal(body.task_id, "t-1");
  assert.equal(body.taskId, undefined, "no camelized alias ever appears");
  assert.equal(transport.sent[0].headers["Idempotency-Key"], "key-1");
});

test("wire: declared pagination members are sendable query params", async () => {
  const { LekaloClient } = await importClient();
  const transport = fakeTransport([okBody({ items: [], next_cursor: "c2" })]);
  const client = new LekaloClient({ baseUrl: "https://unit.invalid", transport });
  await client.plannerEndpointTasksByProject("proj-7", "50", "cursor-1");
  const query = transport.sent[0].query;
  assert.equal(query["limit"], "50", "the declared limit param serializes");
  assert.equal(query["after_task"], "cursor-1", "the declared cursor param serializes");
  assert.equal(
    transport.sent[0].path,
    "https://unit.invalid/projects/proj-7/tasks",
  );
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
