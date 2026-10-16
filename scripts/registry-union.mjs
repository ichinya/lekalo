// Shared fail-closed admission for every landed 0.8.0 family gate.
// Run before dependency/schema/binary checks so missing sibling families
// are diagnosed as union gaps, not hidden by an unrelated unavailable lane.
import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
execFileSync(process.execPath, [resolve(root, "scripts/check-registry-union.mjs")], { cwd: root });
