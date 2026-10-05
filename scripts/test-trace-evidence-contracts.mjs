#!/usr/bin/env node
// Closed neutral evidence schema gate. Exact Ajv only; no provider execution.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { join } from "node:path";
import { readFileSync } from "node:fs";
import { family, read, mappingDigest } from "./lib/trace-assessment-contracts.mjs";

const require = createRequire(import.meta.url);
let Ajv2020, version;
try { Ajv2020 = require("ajv/dist/2020.js").default; version = require("ajv/package.json").version; }
catch { const path = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv"); Ajv2020 = require(join(path, "dist/2020.js")).default; version = JSON.parse(readFileSync(join(path, "package.json"), "utf8")).version; }
assert.equal(version, "8.17.1", "exact Ajv version");
const ajv = new Ajv2020({ strict: true, allErrors: true });
const valid = ajv.compile(read("contracts/trace-validation-evidence.schema.v0.6.4.json"));
const shapeInvalid = new Set(["unknown-member", "contradictory-pass", "null-test", "overlong-code"]);
let count = 0;
for (const item of read(`${family}/cases.json`)) {
  if (["duplicate-key", "escaped-duplicate"].includes(item.id)) continue; // Runtime parser rejects decoded duplicates.
  const doc = read(`${family}/input/${item.id}.json`);
  assert.equal(valid(doc), !shapeInvalid.has(item.id), `${item.id}: ${JSON.stringify(valid.errors)}`);
  if (item.expect.status !== "invalid") assert.equal(doc.mappingDigest, mappingDigest(doc), item.id);
  count++;
}
for (const layout of ["greenfield", "adopt"]) assert.ok(valid(read(`${family}/${layout}/evidence.json`)), layout);
const ready = read(`${family}/input/ready.json`);
const mutations = [
  (d) => { d.rawStdout = "secret"; },
  (d) => { d.evidence[0].protocol.command = "hlv check"; },
  (d) => { d.evidence[0].diagnostics.push({ code: "CTR-030", severity: "info", subject: "bad/path" }); },
  (d) => { d.evidence[2].artifactDigest = null; },
  (d) => { d.evidence[0].gate = "gate:injected"; },
  (d) => { delete d.evidence[2].test; },
  (d) => { delete d.evidence[0].tool; },
  (d) => { d.chains[0].relationRefs.push(d.chains[0].relationRefs[0]); },
  (d) => { d.requiredProviders.push("unknown"); },
  (d) => { d.scope.scenarios = []; },
  (d) => { d.chains = Array(10001).fill(d.chains[0]); },
  (d) => { d.evidence = Array(2001).fill(d.evidence[0]); },
];
for (const mutate of mutations) { const doc = structuredClone(ready); mutate(doc); assert.equal(valid(doc), false, "hostile shape accepted"); count++; }
console.log(JSON.stringify({ ok: true, gate: "trace-evidence-contracts", ajv: version, checks: count, synthetic: true }));
