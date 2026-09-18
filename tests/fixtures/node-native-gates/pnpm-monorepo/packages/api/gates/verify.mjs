// Synthetic deterministic gate (stage-only marker).
import { appendFileSync, readFileSync } from "node:fs";
const mode = process.argv[process.argv.indexOf("--mode") + 1] ?? "typecheck";
const source = readFileSync(new URL("../src/server.ts", import.meta.url), "utf8");
if (!source.includes("planTask")) {
  process.stderr.write("fixture gate: api source is not the expected synthetic content\n");
  process.exit(1);
}
appendFileSync(new URL("./gate-markers.txt", import.meta.url), `${mode}:@fixture/api\n`);
process.stdout.write(`fixture gate ok: ${mode}\n`);
