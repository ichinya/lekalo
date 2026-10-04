#!/usr/bin/env node
// Schema + live CLI semantic gate. CI build-test requires the real binary.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync, existsSync, readdirSync, statSync, mkdtempSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { root, family, read, digest, normalized, mappingDigest } from "./lib/trace-assessment-contracts.mjs";

const require = createRequire(import.meta.url);
let Ajv2020, version;
try { Ajv2020 = require("ajv/dist/2020.js").default; version = require("ajv/package.json").version; }
catch { const path = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv"); Ajv2020 = require(join(path, "dist/2020.js")).default; version = JSON.parse(readFileSync(join(path, "package.json"), "utf8")).version; }
assert.equal(version, "8.17.1", "exact Ajv version");
const ajv = new Ajv2020({ strict: true, allErrors: true });
ajv.addSchema(read("contracts/diagnostic.schema.v0.2.16.json"), "https://dev.lekalo/diagnostic.schema.v0.2.16.json");
const valid = ajv.compile(read("contracts/trace-assessment.schema.v0.6.4.json"));
const diagnostic = ajv.getSchema("dev.lekalo.diagnostic@0.2.16");
const cases = read(`${family}/cases.json`);
const reportOf = (receipt) => receipt.assessment ?? receipt.payload?.assessment;
const rules = new Set();
for (const item of cases) {
  const receipt = read(`${family}/golden/${item.id}.json`);
  assert.equal(receipt.status, item.expect.status, item.id);
  for (const d of receipt.diagnostics ?? reportOf(receipt)?.diagnostics ?? []) { assert.ok(diagnostic(d)); assert.equal(d.registry_version, "0.6.4"); rules.add(d.id); }
  if (item.expect.status === "invalid") { assert.equal(receipt.diagnostics[0].id, item.expect.rule); continue; }
  assert.ok(valid(receipt), `${item.id}: ${JSON.stringify(valid.errors)}`);
  const report = reportOf(receipt);
  for (const key of ["verdict", "coverage"]) assert.equal(report[key], item.expect[key], `${item.id}/${key}`);
  if (item.expect.execution) assert.equal(report.chains[0].execution, item.expect.execution, item.id);
  const input = normalized(read(`${family}/input/${item.id}.json`));
  assert.equal(report.evidenceDigest, digest(input), item.id);
  assert.equal(report.mappingDigest, mappingDigest(input), item.id);
  assert.deepEqual(report.evidence, input.evidence, `${item.id}: receipt loss`);
  if (receipt.status === "denied") {
    assert.deepEqual(receipt.diagnostics, report.diagnostics);
    assert.deepEqual(receipt.reasonCodes, report.diagnostics.map((d) => d.id));
  }
}
for (const rule of read("contracts/diagnostic-registry.v0.6.4.json").entries.filter((r) => r.id.startsWith("trace.bridge-"))) assert.ok(rules.has(rule.id), `no golden diagnostic witness: ${rule.id}`);
assert.notDeepEqual(reportOf(read(`${family}/golden/hlv-fail.json`)).evidence[0], reportOf(read(`${family}/golden/hlv-unavailable.json`)).evidence[0]);
const failedHlv = reportOf(read(`${family}/golden/hlv-fail.json`)).evidence.find((r) => r.provider === "hlv");
assert.deepEqual(failedHlv.diagnostics.map((d) => d.code), ["CTR-030", "GATE-005"]);
const mutated = read(`${family}/golden/ready.json`); mutated.assessment.evidence[0].diagnostics.push({ code: "CTR-030", severity: "error", subject: "gate:focus" }); assert.equal(valid(mutated), false);
const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const live = existsSync(bin);
assert.ok(live || !process.argv.includes("--require-binary"), "built Lekalo binary required");
const expectedExit = { valid: 0, invalid: 1, denied: 3 };
function run(input, cwd = root, trace = "trace.json") {
  return spawnSync(bin, ["--json", "trace", "assess", resolve(root, family, trace), "--evidence", input], { cwd, encoding: "utf8", timeout: 30000, maxBuffer: 32 * 1024 * 1024 });
}
const snapshot = (dir) => {
  const rows = [];
  function walk(path) { for (const name of readdirSync(path).sort()) { const p = join(path, name); if (statSync(p).isDirectory()) walk(p); else rows.push([p.slice(dir.length), createHash("sha256").update(readFileSync(p)).digest("hex")]); } }
  walk(dir); return rows;
};
if (live) {
  const before = snapshot(resolve(root, family));
  for (const item of cases) {
    const result = run(resolve(root, family, "input", `${item.id}.json`), root, item.trace);
    assert.ifError(result.error); assert.equal(result.status, expectedExit[item.expect.status], item.id);
    const stream = item.expect.status === "invalid" ? result.stderr : result.stdout;
    assert.equal(item.expect.status === "invalid" ? result.stdout : result.stderr, "", `${item.id}: wrong stream`);
    assert.deepEqual(JSON.parse(stream), read(`${family}/golden/${item.id}.json`), `${item.id}: golden drift`);
  }
  for (const layout of ["greenfield", "adopt"]) {
    const result = run(resolve(root, family, layout, "evidence.json"), resolve(root, family, layout));
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), read(`${family}/golden/ready.json`), `${layout}: layout influenced core`);
  }
  assert.deepEqual(snapshot(resolve(root, family)), before, "assessment changed fixture or HLV-owned artifacts");
  const sandbox = mkdtempSync(join(tmpdir(), "lekalo-trace-assessment-"));
  const ready = read(`${family}/input/ready.json`);
  const permutations = structuredClone(ready);
  permutations.evidence.reverse(); permutations.providerPins.reverse(); permutations.requiredProviders.reverse(); permutations.chains[0].relationRefs.reverse(); permutations.chains[0].evidenceRefs.reverse();
  writeFileSync(join(sandbox, "permuted.json"), JSON.stringify(permutations));
  const result = run(join(sandbox, "permuted.json"));
  assert.equal(result.status, 0, result.stderr); assert.deepEqual(JSON.parse(result.stdout), read(`${family}/golden/ready.json`));
  for (const [name, mutate] of [
    ["total-diagnostics", (d) => { d.evidence = Array.from({ length: 16 }, (_, i) => ({ ...structuredClone(d.evidence[0]), id: `receipt:many-${i}`, diagnostics: Array(128).fill({ code: "CTR-001", severity: "info", subject: "gate:focus" }) })); }],
    ["total-scope", (d) => { d.scope.requirements = Array.from({ length: 10000 }, (_, i) => `requirement:r${i}`); }],
    ["unicode-identity", (d) => { d.chains[0].id = "chain:秘密"; }],
  ]) {
    const d = structuredClone(ready); mutate(d); writeFileSync(join(sandbox, `${name}.json`), JSON.stringify(d));
    const refusal = run(join(sandbox, `${name}.json`)); assert.equal(refusal.status, 1, name); assert.equal(JSON.parse(refusal.stderr).diagnostics[0].id, "trace.bridge-input-invalid");
  }
  writeFileSync(join(sandbox, "oversized.json"), " ".repeat(8 * 1024 * 1024 + 1));
  assert.equal(run(join(sandbox, "oversized.json")).status, 1);
}
console.log(JSON.stringify({ ok: true, gate: "trace-assessment-contracts", ajv: version, cases: cases.length, live, execution: "synthetic receipts; no HLV invocation" }));
