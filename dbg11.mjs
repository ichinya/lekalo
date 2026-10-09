// Read the manifest the run sees: could recovery_code or plan homes
// differ? Print all entries of format home.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
const home = process.argv[2];
for (const entry of readdirSync(home)) {
  const p = join(home, entry);
  const st = statSync(p);
  console.log(entry, st.isDirectory() ? "dir" : "file");
}
