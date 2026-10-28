import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const lekalo = "D:/Projects/lekalo/target/debug/lekalo.exe";
const exe = "D:/Projects/lekalo/adapters/python/target/debug/lekalo-target-python.exe";
const profile = readFileSync("profile.compact.json", "utf8").trim();
const result = spawnSync(
  lekalo,
  [
    "scan", "--target", "python", "--profile", "standalone", "--project", ".", "--json",
    "--", exe, "--lekalo-project-profile-json", profile,
  ],
  { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
);
process.stdout.write(result.stdout ?? "");
process.stderr.write(result.stderr ?? "");
process.exit(result.status ?? 1);
