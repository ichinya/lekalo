// Which file conflicts? Print the full --json envelope of the dbg7 run.
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
const root = "C:/Users/User/orca/workspaces/lekalo/m8-issue-94";
const binary = join(root, "target/debug/lekalo.exe");
const fixtures = join(root, "tests/fixtures/format");
const sha256 = (data) => createHash("sha256").update(data).digest("hex");
const workspace = mkdtempSync(join(tmpdir(), "dbg8-"));
const project = join(workspace, "p-" + process.pid);
cpSync(join(fixtures, "planner-canonical"), project, { recursive: true });
const module = join(project, "lekalo/modules/scheduling/module.yaml");
const forged = Buffer.from('# forged semantic edit\n"definitions":\n  - "description": "FORGED SEMANTIC EDIT"\n    "id": "scheduling"\n    "kind": "module"\n    "version": 1\n"schema_version": "0.2.16"\n');
const forgedDigest = sha256(forged);
const embeddedPolicyDigest = JSON.parse(readFileSync(join(root, "contracts/format-policy.v0.8.0.json"), "utf8")).policyDigest;
const base = sha256(Buffer.concat([
  Buffer.from(embeddedPolicyDigest), Buffer.from([0x1f]),
  Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
  Buffer.from(forgedDigest), Buffer.from([0x1f]),
  Buffer.from(forgedDigest),
]));
const planId = `${base.slice(0, 47)}-1`;
const home = join(project, ".lekalo/cache/format", planId);
mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
writeFileSync(join(home, "before/modules/scheduling/module.yaml"), forged);
const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"${embeddedPolicyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${forgedDigest}","after_sha256":"${forgedDigest}"}]}`;
writeFileSync(join(home, "manifest.json"), Buffer.from(manifest));
writeFileSync(join(home, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","manifest_sha256":"${sha256(manifest)}"}\n`);
const applied = spawnSync(binary, ["format", "--json"], { cwd: project, encoding: "utf8" });
const parsed = JSON.parse(applied.stdout);
console.log(JSON.stringify(parsed.diagnostics[0].data));
console.log("current == real digest?", parsed.diagnostics[0].data.currentDigest === sha256(readFileSync(module)));
