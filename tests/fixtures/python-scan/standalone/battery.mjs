import { spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
const p = readFileSync("profile.compact.json", "utf8").trim();
const r = spawnSync(
  "D:/Projects/lekalo/target/debug/lekalo.exe",
  [
    "adapter", "test", "--json", "--report", "json", "--repeats", "1",
    "--", "D:/Projects/lekalo/adapters/python/target/debug/lekalo-target-python.exe",
    "--lekalo-project-profile-json", p,
  ],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
);
writeFileSync("battery-profile.json", r.stdout || "");
console.log(r.stderr || "stderr-empty");
console.log("exit", r.status);
