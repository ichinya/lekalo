// The single bounded runner of the issue #136 verification
// specification: it really executes the pinned native test and the
// independent behavior oracle as child processes and aggregates both
// markers. A shallow happy-path pass alone can never print both.
import { spawnSync } from "node:child_process";
import process from "node:process";

let failed = false;

const testRun = spawnSync(process.execPath, ["--test", "test/tasks.test.mjs"], {
  encoding: "utf8",
});
if (testRun.status !== 0 || !testRun.stdout.includes("TASK-TEST-OK")) {
  failed = true;
  process.stdout.write("native-test-failed\n");
} else {
  process.stdout.write("TASK-TEST-OK\n");
}

const oracleRun = spawnSync(process.execPath, ["tools/oracle.mjs"], {
  encoding: "utf8",
});
if (oracleRun.status !== 0 || !oracleRun.stdout.includes("ORACLE-OK")) {
  failed = true;
  process.stdout.write("behavior-oracle-failed\n");
} else {
  process.stdout.write("ORACLE-OK\n");
}

process.exit(failed ? 1 : 0);
