import test from "node:test";
import assert from "node:assert/strict";
import { focusTask } from "../src/focus.ts";
import { listTasks } from "../src/queries.ts";

test("tutorial.focus", () => {
  assert.deepEqual(focusTask({ taskId: "11111111-1111-4111-8111-111111111111" }), { taskId: "11111111-1111-4111-8111-111111111111", state: "focused" });
});
test("tutorial.list", () => {
  const rows = [{ taskId: "synthetic-task", title: "Synthetic task", state: "focused", due: null }];
  const result = listTasks(rows);
  assert.deepEqual(result, rows);
  assert.notEqual(result, rows);
});
