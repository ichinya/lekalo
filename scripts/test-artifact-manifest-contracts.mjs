#!/usr/bin/env node
// Issue #21 release gate: validate the closed ownership-manifest wire
// contract (contracts/artifact-manifest.schema.v1.0.0.json) against the
// fixture matrix under tests/fixtures/artifacts/wire with exact
// structural Ajv checks plus the canonical-byte, self-digest, and
// cross-field invariants the schema cannot express. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and 24)
// and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createHash } from "node:crypto";
import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);

let Ajv2020;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
} catch (error) {
  console.error(JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2));
  process.exit(1);
}
if (require("ajv/package.json").version !== "8.17.1") {
  console.error(JSON.stringify({ ok: false, reason: "ajv-version", detail: require("ajv/package.json").version }, null, 2));
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  console.error(JSON.stringify({ ok: false, reason, detail }, null, 2));
  process.exit(1);
};

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value && typeof value === "object") {
    const sorted = {};
    for (const key of Object.keys(value).sort()) sorted[key] = sortValue(value[key]);
    return sorted;
  }
  return value;
}

function canonicalBytes(document) {
  return Buffer.from(JSON.stringify(sortValue(document)), "utf8");
}

const schema = JSON.parse(readFileSync(join(root, "contracts", "artifact-manifest.schema.v1.0.0.json"), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validate = ajv.compile(schema);

// The cross-field invariants the JSON schema cannot express; the Rust
// parser is normative, this mirror keeps the fixtures honest.
function invariantError(document) {
  const keyOf = (a) => `${a.semantic_owner}\u0000${a.path}\u0000${a.artifact_kind}`;
  const artifacts = document.artifacts ?? [];
  for (let i = 1; i < artifacts.length; i += 1) {
    if (!(keyOf(artifacts[i - 1]) < keyOf(artifacts[i]))) return "artifacts-unsorted-or-duplicate";
  }
  const keys = new Set(artifacts.map(keyOf));
  const maps = document.source_maps ?? [];
  for (let i = 1; i < maps.length; i += 1) {
    if (!(keyOf(maps[i - 1].artifact) < keyOf(maps[i].artifact))) return "maps-unsorted-or-duplicate";
  }
  for (const map of maps) {
    if (!keys.has(keyOf(map.artifact))) return "map-without-artifact";
    const entries = map.entries ?? [];
    const orderKey = (e) => `${String(e.start).padStart(10, "0")}\u0000${String(e.end).padStart(10, "0")}\u0000${e.semantic_id}`;
    for (let i = 1; i < entries.length; i += 1) {
      if (!(orderKey(entries[i - 1]) < orderKey(entries[i]))) return "map-entries-unsorted";
    }
    for (const entry of entries) {
      if (entry.start >= entry.end) return "map-range-reversed";
    }
    for (let i = 1; i < entries.length; i += 1) {
      if (entries[i - 1].semantic_id === entries[i].semantic_id && entries[i].start < entries[i - 1].end) {
        return "map-range-overlap";
      }
    }
  }
  const lifecyclePolicy = {
    generated: "on-input-change",
    scaffolded: "once",
    checked: "validate-only",
    external: "reference-only",
    custom: "manual-only",
  };
  for (const artifact of artifacts) {
    const refs = artifact.input_refs ?? [];
    for (let i = 1; i < refs.length; i += 1) {
      if (!(refs[i - 1] < refs[i])) return "input-refs-unsorted-or-duplicate";
    }
    if (lifecyclePolicy[artifact.lifecycle] !== artifact.regeneration_policy) {
      return "policy-lifecycle-mismatch";
    }
  }
  return null;
}

// The complete refusal lattice for one document: Ajv structure, the
// invariants above, and the non-self-referential integrity digest.
function refusals(document) {
  const refused = [];
  if (!validate(document)) refused.push("structure");
  if (invariantError(document) !== null) refused.push("invariant");
  const draft = { ...document };
  delete draft.manifest_digest;
  const expected = `sha256:${sha256(canonicalBytes(draft))}`;
  if (document.manifest_digest !== expected) refused.push("digest");
  return refused;
}

const fixturesDir = join(root, "tests", "fixtures", "artifacts", "wire");
const valid = [];
const invalid = [];
for (const dir of readdirSync(fixturesDir).sort()) {
  if (dir !== "valid" && dir !== "invalid") continue;
  for (const name of readdirSync(join(fixturesDir, dir)).sort()) {
    if (!name.endsWith(".json")) continue;
    (dir === "valid" ? valid : invalid).push(join(dir, name));
  }
}
if (valid.length === 0 || invalid.length === 0) fail("fixture-matrix-empty", { valid: valid.length, invalid: invalid.length });

let checked = 0;
for (const name of valid) {
  const bytes = readFileSync(join(fixturesDir, name));
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    fail("valid-fixture-unparseable", { name, detail: String(error) });
  }
  if (!validate(document)) fail("valid-fixture-rejected", { name, errors: validate.errors });
  if (invariantError(document) !== null) fail("valid-fixture-invariant", { name, detail: invariantError(document) });

  // Canonical bytes: compact, key-sorted, exactly one trailing LF.
  const canonical = canonicalBytes(document).toString("utf8");
  if (bytes.toString("utf8") !== canonical + "\n") fail("fixture-noncanonical", { name });
  checked += 1;
}

for (const name of invalid) {
  const bytes = readFileSync(join(fixturesDir, name));
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch {
    fail("invalid-fixture-unparseable", { name });
  }
  const refused = refusals(document);
  if (refused.length === 0) fail("invalid-fixture-accepted", { name });
  checked += 1;
}

console.log(JSON.stringify({ ok: true, ajv: "8.17.1", valid: valid.length, invalid: invalid.length, checked }));
