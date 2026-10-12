// The shallow happy-path test: it passes for the original
// implementation and for the deliberately faulty candidate alike, which
// is exactly why the behavior oracle is a separate obligation.
import assert from "node:assert/strict";
import { completeTask, registerTask } from "../src/tasks.mjs";

const tasks = registerTask(new Map(), { id: "t1", title: "first", state: "backlog" });
const done = completeTask(tasks, "t1");
assert.equal(done.state, "done");
console.log("TASK-TEST-OK");
