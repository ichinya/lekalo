#!/usr/bin/env node
// Issue #94 contract gate: the format-policy and format-report schema and
// instance pairs plus the union diagnostic-registry successor 0.8.0,
// validated with the same pinned third-party Draft 2020-12 implementation
// as the other contract gates. Exact Ajv 8.17.1 is provisioned outside
// this checkout (CI does the same on Node 18 and 24) and exposed through
// NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { ACCEPTED_M8_REGISTRY_FAMILIES } from "./lib/registry-union.mjs";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message,
  }, null, 2)}\n`);
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    detail: `expected 8.17.1, found ${ajvVersion}`,
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const bytes = (relative) => readFileSync(resolve(root, relative));
const sha256 = (data) => createHash("sha256").update(data).digest("hex");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const ajv = new Ajv2020({ strict: true, allErrors: true });

// 1. The format-policy pair: schema validates its pinned instance.
const policySchema = read("contracts/format-policy.schema.v0.8.0.json");
const policy = read("contracts/format-policy.v0.8.0.json");
const validatePolicy = ajv.compile(policySchema);
if (!validatePolicy(policy)) fail("policy-instance-invalid", validatePolicy.errors);
if (policy.schema_version !== "lekalo/format-policy/v0.8.0") fail("policy-identity");
if (policy.identity !== "dev.lekalo.format-policy@0.8.0") fail("policy-identity");

// 2. The pinned policy digest recipe: SHA-256 over the canonical compact
// JSON of the whole document excluding its own policyDigest member.
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}
const reduced = { ...policy };
delete reduced.policyDigest;
const digest = `sha256:${sha256(canonical(reduced))}`;
if (digest !== policy.policyDigest) fail("policy-digest-mismatch", { computed: digest, pinned: policy.policyDigest });
if (policy.modelVersion !== "0.2.16") fail("policy-model-pin", policy.modelVersion);
if (policy.limits.maxDocumentBytes !== 8 * 1024 * 1024) fail("policy-document-bound");
if (policy.ordering.setLists.length !== 1 || policy.ordering.setLists[0].member !== "modules[].imports") {
  fail("policy-set-lists", policy.ordering.setLists);
}

// 3. The union registry successor: every 0.6.5 entry is preserved
// unchanged, plus the landed 0.8.0 families (`format.*`, `adoption.*`,
// `portability.*`, `worklog-changelog.*`, `managed.*`, and `module-api.*`), sorted
// by id bytes.
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
const successorSchema = read("contracts/diagnostic-registry.schema.v0.8.0.json");
const registry = read("contracts/diagnostic-registry.v0.8.0.json");
const validateRegistry = ajv.compile(successorSchema);
if (!validateRegistry(registry)) fail("registry-instance-invalid", validateRegistry.errors);
if (registry.schema_version !== "lekalo/diagnostic-registry/v0.8.0") fail("registry-identity");
if (registry.identity !== "dev.lekalo.diagnostic-registry@0.8.0") fail("registry-identity");
if (registry.registry_version !== "0.8.0") fail("registry-version");
if (registry.closed !== true) fail("registry-not-closed");
const byId = new Map(predecessor.entries.map((entry) => [entry.id, entry]));
const formatEntries = registry.entries.filter((entry) => entry.id.startsWith("format."));
const landedFamilies = ACCEPTED_M8_REGISTRY_FAMILIES;
const additions = registry.entries.filter((entry) => !byId.has(entry.id));
if (registry.entries.length !== predecessor.entries.length + additions.length || additions.length === 0) {
  fail("registry-not-a-union");
}
for (const entry of additions) {
  if (!landedFamilies.some((family) => entry.id.startsWith(family))) {
    fail("registry-addition-outside-landed-families", entry.id);
  }
}
for (const entry of registry.entries) {
  const previous = byId.get(entry.id);
  if (previous && JSON.stringify(previous) !== JSON.stringify(entry)) {
    fail("predecessor-entry-drifted", entry.id);
  }
}
const expectedFamily = [
  ["format.diff", "LEK-FORMAT-001", "denied"],
  ["format.comments-loss", "LEK-FORMAT-002", "denied,valid"],
  ["format.semantic-change", "LEK-FORMAT-003", "invalid"],
  ["format.selection-invalid", "LEK-FORMAT-004", "invalid"],
  ["format.source-changed", "LEK-FORMAT-005", "denied"],
  ["format.write-failed", "LEK-FORMAT-006", "invalid"],
  ["format.recovery-required", "LEK-FORMAT-007", "invalid"],
  ["format.limit-exceeded", "LEK-FORMAT-008", "invalid"],
  ["format.unsupported-surface", "LEK-FORMAT-009", "unsupported"],
  ["format.locked", "LEK-FORMAT-010", "denied"],
  ["format.stdin-loss", "LEK-FORMAT-011", "denied"],
];
if (formatEntries.length !== expectedFamily.length) {
  fail("format-family-size", formatEntries.length);
}
const ids = registry.entries.map((entry) => entry.id);
const sorted = [...ids].sort();
if (ids.some((id, index) => id !== sorted[index])) fail("registry-not-sorted");
for (const [id, code, statuses] of expectedFamily) {
  const entry = registry.entries.find((candidate) => candidate.id === id);
  if (!entry) fail("format-family-missing", id);
  if (entry.code !== code) fail("format-code", `${id}: ${entry.code}`);
  if (entry.allowed_statuses.join(",") !== statuses) {
    fail("format-statuses", `${id}: ${entry.allowed_statuses.join(",")}`);
  }
  if (entry.lifecycle !== "active") fail("format-lifecycle", id);
  if (entry.message_id !== entry.id) fail("format-message-id", id);
}
const categories = new Set(registry.entries.map((entry) => entry.category));
for (const entry of formatEntries) {
  if (!categories.has(entry.category)) fail("format-category", entry.id);
}

// 4. The format-report pair: schema shape plus the semantic invariants
// JSON Schema cannot express.
const reportSchema = read("contracts/format-report.schema.v0.8.0.json");
const validateReport = ajv.compile(reportSchema);
const baseReport = {
  schema_version: "lekalo/format-report/v0.8.0",
  identity: "dev.lekalo.format-report@0.8.0",
  mode: "check",
  complete: true,
  policy: { version: "0.8.0", digest: policy.policyDigest },
  model: { version: "0.2.16" },
  ir: { version: "0.2.16" },
  scope: { kind: "all", selector: null },
  files: [
    {
      path: "lekalo/project.yaml",
      syntax: "yaml",
      action: "changed",
      bytesBefore: 5,
      bytesAfter: 7,
      sha256Before: "a".repeat(64),
      sha256After: "b".repeat(64),
      equal: true,
    },
  ],
  summary: { selected: 1, changed: 1, unchanged: 0, applied: 0, lossCount: 0 },
};
if (!validateReport(baseReport)) fail("report-shape", validateReport.errors);
// Cross-member invariants (selected = changed + unchanged, no duplicate
// rows, byte-sorted paths, zero applied files in check mode) are enforced
// against fresh binary envelopes by scripts/test-format-cli.mjs; JSON
// Schema cannot express them without leaking the closed shape.
const portability = JSON.parse(bytes("tests/fixtures/format/portability/manifest.json"));
if (portability.files.length !== 2) fail("portability-manifest-size");
for (const file of portability.files) {
  const actual = sha256(bytes(file.path));
  if (actual !== file.sha256) fail("portability-manifest-digest", file.path);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "format-contracts",
  policyDigest: policy.policyDigest,
  registryEntries: registry.entries.length,
  formatFamily: formatEntries.length,
}, null, 2)}\n`);
