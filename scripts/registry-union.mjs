// Shared fail-closed admission for every landed 0.8.0 family gate.
// Run before dependency/schema/binary checks so missing sibling families
// are diagnosed as union gaps, not hidden by an unrelated unavailable lane.
// The live-line base ref is overridable (LEKALO_UNION_BASE) so a fixer
// task can pin the merge-base its HEAD was cut from.
import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
execFileSync(process.execPath, [resolve(root, "scripts/check-registry-union.mjs"), ...(process.env.LEKALO_UNION_BASE ? ["--base", process.env.LEKALO_UNION_BASE] : [])], { cwd: root });
