// Synthetic deterministic gate: appends one marker to the stage-only
// marker file and verifies the package source bytes. Never spawns
// anything; exits nonzero when the check fails.
import { appendFileSync, readFileSync } from "node:fs";
const mode = process.argv[process.argv.indexOf("--mode") + 1] ?? "test";
const source = readFileSync(new URL("../src/index.ts", import.meta.url), "utf8");
if (!source.includes("syncEvent")) {
  process.stderr.write("fixture gate: sync source is not the expected synthetic content\n");
  process.exit(1);
}
appendFileSync(new URL("./gate-markers.txt", import.meta.url), `${mode}:@taskhub/sync\n`);
process.stdout.write(`fixture gate ok: ${mode}\n`);
