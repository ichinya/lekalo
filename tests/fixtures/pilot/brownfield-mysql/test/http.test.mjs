import assert from "node:assert/strict";
import test from "node:test";
import { app } from "../packages/api/src/routes/tasks.ts";

test("real Hono request with an injected repository; no database claim", async () => {
  const calls = [];
  const repository = { async create(row) { calls.push(row); return row; } };
  const response = await app.request("/tasks", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ id: "task-a", title: "Synthetic task", priority: 2 }) }, { repository });
  assert.equal(response.status, 201);
  assert.deepEqual(await response.json(), { id: "task-a", title: "Synthetic task", priority: 2 });
  assert.equal(calls.length, 1);
  for (const body of ["{", "null", JSON.stringify({ id: "invalid/identity", title: "Task", priority: 0 })]) {
    const rejected = await app.request("/tasks", { method: "POST", headers: { "content-type": "application/json" }, body }, { repository });
    assert.equal(rejected.status, 400);
  }
  assert.equal(calls.length, 1, "invalid input never reaches persistence");
});
