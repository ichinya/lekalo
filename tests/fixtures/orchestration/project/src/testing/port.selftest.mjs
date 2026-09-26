/**
 * The self-test of the fixture planner test port (issue #47, plan S5).
 * Run with `node port.selftest.mjs` — zero dependencies, zero output on
 * success, exit 1 with the failing surface on any violation. The port
 * contract mirrored here is the closed surface `lekalo/test-port.json`
 * declares; the generated scenario tests and the evidence harness both
 * depend on every rule below.
 */
import assert from "node:assert/strict";
import { port, resetPort } from "./port.mjs";

const str = (value) => ({ type: "string", value });

async function test(name, body) {
  resetPort();
  try {
    await body();
  } catch (error) {
    process.stderr.write(`port selftest FAILED: ${name}\n${error?.stack ?? error}\n`);
    process.exit(1);
  }
}

await test("invoke dispatches the command and persists the effect", async () => {
  await port.state.seed("planner.task", { task_id: "task-1" }, { user_id: "user-1" });
  const outcome = await port.invoke("planner.focus_task", { task_id: "task-1", user_id: "user-1" }, {});
  assert.equal(outcome.ok, true);
  const rows = await port.state.query("planner.task", { task_id: "task-1" });
  assert.equal(rows.length, 1);
  assert.equal(rows[0].focused, true);
});

await test("invoke answers the typed missing-task error", async () => {
  const outcome = await port.invoke("planner.focus_task", { task_id: "task-404" }, {});
  assert.deepEqual(outcome, {
    ok: false,
    error: { id: "planner.error.task_missing", fields: { task_id: "task-404" } },
  });
});

await test("the idempotency key deduplicates: one emission, identical outcomes", async () => {
  await port.state.seed("planner.task", { task_id: "task-1" }, { user_id: "user-1" });
  const ctx = { idempotencyKey: "user-1" };
  const first = await port.invoke("planner.focus_task", { task_id: "task-1" }, ctx);
  const replay = await port.invoke("planner.focus_task", { task_id: "task-1" }, ctx);
  assert.deepEqual(first, replay);
  assert.equal(
    port.emissions().filter((entry) => entry.operation === "planner.focus_task").length,
    1,
    "the replay never re-emits",
  );
});

await test("unknown operations are infrastructure faults (they throw)", async () => {
  await assert.rejects(() => port.invoke("planner.query.mystery", {}, {}));
});

await test("clock freeze, ids seed, actor, and authorize answer their surfaces", async () => {
  port.clock.freeze("2026-01-02T03:04:05Z");
  port.ids.seed({ algorithm: "sequence", seed: "planner-1" });
  assert.deepEqual(port.actor("planner/member", "planner"), { ref: "planner/member", scope: "planner" });
  assert.equal(await port.authorize({ ref: "planner/member" }, "planner.deny_bulk_focus", "planner.focus_task"), "allowed");
  assert.equal(await port.authorize("bulk-agent", "planner.deny_bulk_focus", "planner.focus_task"), "denied");
});

await test("emissions and effects logs capture entries with their operation", async () => {
  await port.state.seed("planner.task", { task_id: "task-1" }, { user_id: "user-1" });
  await port.invoke("planner.focus_task", { task_id: "task-1" }, {});
  assert.equal(port.emissions().length, 1);
  assert.equal(port.emissions()[0].id, "planner.task_focused");
  assert.equal(port.effects().length, 1);
  assert.equal(port.effects()[0].effect, "planner.create_task");
});

await test("reset wipes every surface: reruns start clean", async () => {
  await port.state.seed("planner.task", { task_id: "task-1" }, { user_id: "user-1" });
  await port.invoke("planner.focus_task", { task_id: "task-1" }, { idempotencyKey: "k" });
  assert.ok(port.emissions().length > 0);
  resetPort();
  assert.equal(port.emissions().length, 0);
  assert.equal(port.effects().length, 0);
  assert.equal((await port.state.query("planner.task", {})).length, 0);
  // The idempotency cache is gone: the same key runs the command again.
  await port.state.seed("planner.task", { task_id: "task-1" }, { user_id: "user-1" });
  const outcome = await port.invoke("planner.focus_task", { task_id: "task-1" }, { idempotencyKey: "k" });
  assert.equal(outcome.ok, true);
});

await test("fixtureDigest is stable and contractCheck is a structural subset", async () => {
  const first = await port.fixtureDigest("core/planner-seed");
  const second = await port.fixtureDigest("core/planner-seed");
  assert.equal(first, second);
  assert.match(first, /^sha256:[0-9a-f]{64}$/);
  assert.equal(await port.contractCheck("planner.task", ["focused"], { focused: true }), true);
  assert.equal(await port.contractCheck("planner.task", ["focused"], { focused: false }), true);
  assert.equal(await port.contractCheck("planner.task", ["focused"], {}), false);
});

process.stdout.write("port selftest: ok\n");
