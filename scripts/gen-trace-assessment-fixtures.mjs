#!/usr/bin/env node
// Explicit reviewed golden update recipe. Requires the built Lekalo binary;
// synthetic receipts never claim a live HLV or native gate execution.
import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { execFileSync, spawnSync } from "node:child_process";
import { join } from "node:path";
import { root, family, read, mappingDigest } from "./lib/trace-assessment-contracts.mjs";

const bin = process.env.LEKALO_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const put = (path, value) => writeFileSync(join(root, path), typeof value === "string" ? value : `${JSON.stringify(value, null, 2)}\n`);
for (const dir of [family, `${family}/input`, `${family}/golden`, `${family}/greenfield/contracts`, `${family}/adopt/.hlv/contracts`]) mkdirSync(join(root, dir), { recursive: true });
const source = read("tests/fixtures/trace/full.trace.json");
const trace = structuredClone(source);
trace.manifestId = "planner-trace-assessment";
trace.nodes = trace.nodes.filter((n) => !["requirement:PLANNER-REQ-001", "symbol:planner.switch_focus", "diagnostic:hlv-diag-focus"].includes(n.nodeId));
trace.relations = trace.relations.filter((r) => !["requirements.switch_focus", "bindings.focus_task_shadow", "references.focus_diag"].includes(r.occurrence));
const nodeIds = new Set(trace.nodes.map((n) => n.nodeId));
trace.relations = trace.relations.filter((r) => nodeIds.has(r.fromNode) && nodeIds.has(r.toNode));
const test = trace.nodes.find((n) => n.nodeKind === "native_test");
test.externalRefs.push({ system: "hlv", originalId: "HLV-TEST-FOCUS", contractVersion: "1.0.0", revision: trace.sourceRevision });
trace.nodes.find((n) => n.nodeKind === "gate").externalRefs[0].revision = trace.sourceRevision;
put(`${family}/trace.json`, trace);
const exported = JSON.parse(execFileSync(bin, ["--json", "trace", "export", `${family}/trace.json`], { cwd: root, encoding: "utf8" }));
assert.equal(exported.status, "valid");
const sha = (c) => `sha256:${c.repeat(64)}`;
const pins = [
  { provider: "hlv", tool: { id: "org.hlv/cli", version: "1.0.0", digest: sha("1") }, protocol: { id: "org.aifhub/hlv-cli", version: "1.0.0", digest: sha("2") } },
  { provider: "openspec", tool: { id: "org.openspec/cli", version: "1.0.0", digest: sha("3") }, protocol: { id: "org.aifhub/openspec", version: "1.0.0", digest: sha("4") } },
  { provider: "source-native", tool: { id: "dev.lekalo/native-runner", version: "1.0.0", digest: sha("5") }, protocol: { id: "dev.lekalo/native-receipt", version: "0.4.0", digest: sha("6") } },
];
const artifact = trace.nodes.find((n) => n.nodeKind === "artifact");
const gate = trace.nodes.find((n) => n.nodeKind === "gate");
const scenario = trace.nodes.find((n) => n.nodeKind === "scenario");
const requirement = trace.nodes.find((n) => n.nodeKind === "requirement");
const symbol = trace.nodes.find((n) => n.nodeKind === "symbol");
const baseReceipt = (pin) => ({ id: `receipt:${pin.provider}`, provider: pin.provider, kind: "check", outcome: "pass", tool: pin.tool, protocol: pin.protocol, sourceRevision: trace.sourceRevision, modelRef: trace.modelRef, workingSetDigest: sha("7"), resultDigest: sha("8"), diagnostics: [] });
const evidence = pins.map((pin) => structuredClone(baseReceipt(pin)));
Object.assign(evidence[2], { kind: "execution", test: test.nodeId, gate: gate.nodeId, artifact: artifact.nodeId, artifactDigest: artifact.contentDigest, resultDigest: gate.evidenceDigest });
const base = { schemaVersion: "lekalo/trace-validation-evidence/v0.6.4", identity: "dev.lekalo.trace-validation-evidence@0.6.4", projectRef: trace.projectRef, sourceRevision: trace.sourceRevision, modelRef: trace.modelRef, workingSetDigest: sha("7"), traceDigest: exported.manifestDigest, mappingDigest: sha("0"), policy: "required", requiredProviders: ["hlv", "openspec"], providerPins: pins, requireExecution: true, scope: { requirements: [requirement.nodeId], artifacts: [artifact.nodeId], scenarios: [scenario.nodeId] }, chains: [{ id: "chain:focus", occurrence: "focus.primary", requirement: requirement.nodeId, symbol: symbol.nodeId, artifact: artifact.nodeId, scenario: scenario.nodeId, test: test.nodeId, gate: gate.nodeId, relationRefs: trace.relations.map((r) => r.relationId), evidenceRefs: evidence.map((r) => r.id) }], evidence };
const cases = [];
function add(id, edit, expect) {
  const doc = structuredClone(base); edit(doc);
  doc.mappingDigest = mappingDigest(doc);
  cases.push({ id, expect });
  put(`${family}/input/${id}.json`, doc);
}
add("ready", () => {}, { status: "valid", verdict: "ready", coverage: "complete", execution: "passed" });
add("optional-unavailable", (d) => { d.policy = "optional"; d.evidence[0].outcome = "unavailable"; delete d.evidence[0].tool; }, { status: "valid", verdict: "degraded", coverage: "complete", execution: "passed" });
for (const outcome of ["fail", "warn", "unavailable", "unsupported", "infrastructure", "configuration"]) add(`hlv-${outcome}`, (d) => {
  d.evidence[0].outcome = outcome;
  if (outcome === "unavailable") delete d.evidence[0].tool;
  if (outcome === "fail") d.evidence[0].diagnostics = [{ code: "CTR-030", severity: "error", subject: requirement.nodeId }, { code: "GATE-005", severity: "error", subject: gate.nodeId }];
}, { status: "denied", verdict: "blocked", coverage: "complete", execution: "passed" });
add("missing-mapping", (d) => { delete d.chains[0].test; delete d.chains[0].gate; d.chains[0].relationRefs = trace.relations.filter((r) => !["verifies", "evidences"].includes(r.relationKind)).map((r) => r.relationId); }, { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" });
add("dangling-mapping", (d) => { d.chains[0].test = "test:unknown"; }, { status: "denied", verdict: "blocked", coverage: "conflicting", execution: "unverified" });
add("duplicate-mapping", (d) => { d.chains.push({ ...structuredClone(d.chains[0]), id: "chain:shadow" }); }, { status: "denied", verdict: "blocked", coverage: "conflicting" });
add("uncovered-scenario", (d) => { d.scope.scenarios.push("scenario:unknown"); }, { status: "denied", verdict: "blocked", coverage: "partial" });
add("stale-git", (d) => { d.sourceRevision = "b".repeat(40); }, { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" });
add("stale-model", (d) => { d.modelRef.digest = sha("9"); }, { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" });
add("stale-receipt", (d) => { d.evidence[0].sourceRevision = "b".repeat(40); }, { status: "denied", verdict: "blocked", coverage: "partial" });
add("stale-worktree", (d) => { d.workingSetDigest = sha("9"); }, { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" });
add("unsupported-protocol", (d) => { d.evidence[0].protocol.version = "1.0.1"; }, { status: "denied", verdict: "blocked", coverage: "complete" });
add("unqualified-tool", (d) => { d.evidence[0].tool.digest = sha("9"); }, { status: "denied", verdict: "blocked", coverage: "complete" });
add("execution-fail", (d) => { d.evidence[2].outcome = "fail"; }, { status: "denied", verdict: "blocked", coverage: "complete", execution: "failed" });
add("execution-digest", (d) => { d.evidence[2].artifactDigest = sha("9"); }, { status: "denied", verdict: "blocked", coverage: "complete", execution: "unverified" });
add("execution-not-requested", (d) => { d.requireExecution = false; }, { status: "valid", verdict: "ready", coverage: "complete", execution: "not-requested" });
add("unreferenced-failure", (d) => { d.evidence.push({ ...structuredClone(d.evidence[0]), id: "receipt:hlv-shadow", outcome: "fail" }); }, { status: "denied", verdict: "blocked", coverage: "conflicting" });
add("missing-provider", (d) => { d.requiredProviders.push("aifhub"); }, { status: "denied", verdict: "blocked", coverage: "complete" });
add("missing-pin", (d) => { d.providerPins = d.providerPins.filter((p) => p.provider !== "hlv"); }, { status: "denied", verdict: "blocked", coverage: "complete" });
add("without-hlv", (d) => { d.evidence = d.evidence.filter((r) => r.provider !== "hlv"); d.providerPins = d.providerPins.filter((p) => p.provider !== "hlv"); d.requiredProviders = ["openspec"]; d.chains[0].evidenceRefs = d.evidence.map((r) => r.id); }, { status: "valid", verdict: "ready", coverage: "complete", execution: "passed" });
add("missing-binding", (d) => { d.chains[0].relationRefs = trace.relations.filter((r) => r.relationKind !== "binds").map((r) => r.relationId); }, { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" });
add("unrelated-execution", (d) => { d.evidence[2].test = "test:other"; }, { status: "denied", verdict: "blocked", coverage: "conflicting", execution: "unverified" });
add("duplicate-provider", (d) => { d.evidence.push({ ...structuredClone(d.evidence[0]), id: "receipt:hlv-shadow", outcome: "fail" }); d.chains[0].evidenceRefs.push("receipt:hlv-shadow"); }, { status: "denied", verdict: "blocked", coverage: "conflicting", execution: "passed" });
const malformed = [
  ["unknown-member", (d) => { d.hlvRoot = ".hlv"; }],
  ["contradictory-pass", (d) => { d.evidence[0].diagnostics.push({ code: "CTR-030", severity: "error", subject: requirement.nodeId }); }],
  ["null-test", (d) => { d.chains[0].test = null; }],
  ["duplicate-identity", (d) => { d.chains.push(structuredClone(d.chains[0])); }],
  ["overlong-code", (d) => { d.evidence[0].diagnostics.push({ code: "X".repeat(129), severity: "info", subject: gate.nodeId }); }],
];
for (const [id, edit] of malformed) add(id, edit, { status: "invalid", rule: "trace.bridge-input-invalid" });
const badDigest = read(`${family}/input/ready.json`); badDigest.mappingDigest = sha("0"); put(`${family}/input/mapping-digest.json`, badDigest); cases.push({ id: "mapping-digest", expect: { status: "invalid", rule: "trace.bridge-input-invalid" } });
for (const [id, name] of [["duplicate-key", "policy"], ["escaped-duplicate", "pol\\u0069cy"]]) {
  const text = JSON.stringify(read(`${family}/input/ready.json`)); put(`${family}/input/${id}.json`, text.slice(0, -1) + `,"${name}":"optional"}`); cases.push({ id, expect: { status: "invalid", rule: "trace.bridge-input-invalid" } });
}
for (const [id, mutate] of [
  ["missing-hlv-test-id", (t) => { const n = t.nodes.find((n) => n.nodeKind === "native_test"); n.externalRefs = n.externalRefs.filter((r) => r.system !== "hlv"); }],
  ["stale-hlv-gate-id", (t) => { t.nodes.find((n) => n.nodeKind === "gate").externalRefs[0].revision = "b".repeat(40); }],
]) {
  const t = structuredClone(trace); mutate(t);
  put(`${family}/${id}.trace.json`, t);
  const e = JSON.parse(execFileSync(bin, ["--json", "trace", "export", `${family}/${id}.trace.json`], { cwd: root, encoding: "utf8" }));
  const d = read(`${family}/input/ready.json`); d.traceDigest = e.manifestDigest;
  put(`${family}/input/${id}.json`, d);
  cases.push({ id, trace: `${id}.trace.json`, expect: { status: "denied", verdict: "blocked", coverage: "partial", execution: "unverified" } });
}
for (const layout of ["greenfield", "adopt"]) {
  const home = layout === "adopt" ? ".hlv/" : "";
  put(`${family}/${layout}/${home}contracts/sentinel.yaml`, "# Synthetic HLV-owned sentinel. Never parsed, invoked or rewritten by Lekalo.\n");
  put(`${family}/${layout}/evidence.json`, read(`${family}/input/ready.json`));
}
for (const item of cases) {
  const result = spawnSync(bin, ["--json", "trace", "assess", `${family}/${item.trace ?? "trace.json"}`, "--evidence", `${family}/input/${item.id}.json`], { cwd: root, encoding: "utf8" });
  const stream = item.expect.status === "invalid" ? result.stderr : result.stdout;
  const parsed = JSON.parse(stream); assert.equal(parsed.status, item.expect.status, item.id);
  put(`${family}/golden/${item.id}.json`, parsed);
}
put(`${family}/cases.json`, cases);
console.log(JSON.stringify({ ok: true, cases: cases.length, synthetic: true }));
