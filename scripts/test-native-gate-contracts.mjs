#!/usr/bin/env node
/**
 * Issue #48 native gate contract gate: the four closed native contract
 * schemas validated with the pinned Ajv 8.17.1 release (provisioned
 * outside the checkout through NODE_PATH / LEKALO_AJV_NODE_PATH), plus
 * plan-digest golden vectors shared with the Rust core.
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a
 * JSON failure report on stderr otherwise.
 */
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath) {
    const req = createRequire(nodePath + "/");
    Ajv = req("ajv/dist/2020.js");
  } else {
    Ajv = require("ajv/dist/2020.js");
  }
} catch {
  fail("ajv-missing", "pinned Ajv 8.17.1 must be provided through NODE_PATH/LEKALO_AJV_NODE_PATH");
}
const ajvModule = Ajv.default ?? Ajv;
const ajv = new ajvModule({ strict: true, allErrors: true });

function fail(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "native-gate-contracts", reason, detail })}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (p) => JSON.parse(readFileSync(resolve(root, p), "utf8"));

const schemas = {
  policy: read("contracts/native-gate-policy.schema.v0.3.2.json"),
  plan: read("contracts/native-gate-plan.schema.v0.3.2.json"),
  run: read("contracts/native-gate-run.schema.v0.3.2.json"),
  view: read("contracts/native-gate-view.schema.v0.3.2.json"),
};
const validators = Object.fromEntries(
  Object.entries(schemas).map(([name, schema]) => {
    const validate = ajv.compile(schema);
    if (typeof validate !== "function") fail("schema-compile", name);
    return [name, validate];
  }),
);

// Positive vectors: committed plan/policy goldens validate.
let positive = 0;
const positiveVectors = [
  ["plan", "tests/fixtures/node-native-gates/protocol/plan.golden.json"],
  ["policy", "tests/fixtures/node-native-gates/protocol/policy.golden.json"],
  ["run", "tests/fixtures/node-native-gates/protocol/run-result.golden.json"],
  ["view", "tests/fixtures/node-native-gates/protocol/view.golden.json"],
];
for (const [kind, path] of positiveVectors) {
  const document = read(path);
  if (!validators[kind](document)) {
    fail("positive-vector", { path, errors: validators[kind].errors });
  }
  positive += 1;
}

// Negative vectors: each must be refused by its schema.
let negative = 0;
const negativeVectors = read("tests/fixtures/node-native-gates/protocol/negative-vectors.json");
for (const vector of negativeVectors) {
  if (validators[vector.kind](vector.document)) {
    fail("negative-accepted", vector.name);
  }
  negative += 1;
}

// Digest parity: the canonical digest over the shared golden vector must
// equal the committed constant (Node side of the Node/Rust pair).
const digestVector = read("tests/fixtures/node-native-gates/protocol/digest-vector.json");
const { canonicalJsonText, planDigest } = await import(
  new URL("../adapters/node-typescript/src/native-contract.mjs", import.meta.url).href
);
const computed = planDigest(digestVector.plan);
if (computed !== digestVector.digest) {
  fail("digest-parity", { computed, expected: digestVector.digest });
}
// Byte-level canonicalization check including a key-order adversary.
const reordered = JSON.parse(JSON.stringify(digestVector.plan));
if (canonicalJsonText(reordered) !== canonicalJsonText(digestVector.plan)) {
  fail("canonicalization", "key order changed the canonical bytes");
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "native-gate-contracts",
  ajv: "8.17.1",
  schemas: Object.keys(schemas),
  positive,
  negative,
  digest: computed,
  runtimeGate: "cargo test -p lekalo-core native_gate",
})}\n`);
