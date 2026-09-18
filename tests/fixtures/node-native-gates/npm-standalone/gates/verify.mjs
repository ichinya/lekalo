// Synthetic deterministic gate (stage-only marker).
import { appendFileSync, readFileSync } from "node:fs";
const mode = process.argv[process.argv.indexOf("--mode") + 1] ?? "test";
const source = readFileSync(new URL("../src/app.ts", import.meta.url), "utf8");
if (!source.includes("appName")) {
  process.stderr.write("fixture gate: standalone source is not the expected synthetic content\n");
  process.exit(1);
}
appendFileSync(new URL("./gate-markers.txt", import.meta.url), `${mode}:@fixture/standalone\n`);
process.stdout.write(`fixture gate ok: ${mode}\n`);
