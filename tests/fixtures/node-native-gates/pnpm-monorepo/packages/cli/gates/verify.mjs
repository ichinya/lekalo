// Synthetic deterministic gate (stage-only marker).
import { appendFileSync, readFileSync } from "node:fs";
const mode = process.argv[process.argv.indexOf("--mode") + 1] ?? "test";
const source = readFileSync(new URL("../src/main.ts", import.meta.url), "utf8");
if (!source.includes("handle")) {
  process.stderr.write("fixture gate: cli source is not the expected synthetic content\n");
  process.exit(1);
}
appendFileSync(new URL("./gate-markers.txt", import.meta.url), `${mode}:@fixture/cli\n`);
process.stdout.write(`fixture gate ok: ${mode}\n`);
