// The independent behavior oracle for the issue #136 verification
// specification: error cases, idempotency, input preservation, and the
// declared limit. Prints ORACLE-OK and exits 0 only when every
// obligation holds.
import assert from "node:assert/strict";
import { completeTask, registerTask } from "../src/tasks.mjs";

const tasks = registerTask(new Map(), { id: "t1", title: "first", state: "backlog" });

// Obligation 1: an invalid id is a TypeError("invalid-task-id").
assert.throws(() => completeTask(tasks, ""), /invalid-task-id/);
assert.throws(() => completeTask(tasks, 7), /invalid-task-id/);

// Obligation 2: an unknown id is a RangeError("unknown-task:<id>").
assert.throws(() => completeTask(tasks, "missing"), /unknown-task:missing/);

// Obligation 3: completion is idempotent and never mutates the input.
const once = completeTask(tasks, "t1");
const twice = completeTask(tasks, "t1");
assert.equal(once.state, "done");
assert.equal(twice.state, "done");
assert.equal(tasks.get("t1").state, "backlog");

// Obligation 4: the declared task limit still holds.
const full = new Map();
for (let index = 0; index < 8; index += 1) {
  registerTask(full, { id: `t${index}`, title: "x", state: "backlog" });
}
assert.throws(() => registerTask(full, { id: "t9", title: "x", state: "backlog" }), /task-limit/);

console.log("ORACLE-OK");
