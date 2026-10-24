// The single bounded runner of the issue #136 verification
// specification: it really executes the pinned native test and the
// independent behavior oracle as child processes and aggregates both
// markers. A shallow happy-path pass alone can never print both.
// The inherited streams remain inside the host's bounded transport.
// Node 24's single-process test mode avoids nested test-runner IPC;
// Windows LPAC does not admit libuv's ordinary captured child pipes.
//
// The consumed paths arrive through the specification's tool argv —
// the runner script, the native test, and the behavior oracle are all
// pinned by the verification manifest, so a swapped or trivialized
// input refuses at apply time instead of serving an unmeasured run.
import { spawnSync } from "node:child_process";
import process from "node:process";

const [testPath = "test/tasks.test.mjs", oraclePath = "tools/oracle.mjs"] =
  process.argv.slice(2);

let failed = false;

const runtimeFlags = ["--preserve-symlinks", "--preserve-symlinks-main"];
const testRun = spawnSync(process.execPath, [...runtimeFlags, "--test", "--test-isolation=none", testPath], {
  stdio: "inherit",
  timeout: 10000,
});
if (testRun.status !== 0) {
  failed = true;
  process.stdout.write("native-test-failed\n");
}

const oracleRun = spawnSync(process.execPath, [...runtimeFlags, oraclePath], {
  stdio: "inherit",
  timeout: 10000,
});
if (oracleRun.status !== 0) {
  failed = true;
  process.stdout.write("behavior-oracle-failed\n");
}

process.exit(failed ? 1 : 0);
