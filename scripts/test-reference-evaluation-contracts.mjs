#!/usr/bin/env node
// Issue #107 release gate: the reference-evaluation trace schema and
// the committed golden traces, validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract
// gates. Exact Ajv 8.17.1 is provisioned outside this checkout (CI
// does the same on Node 18 and 24) and exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH.
//
// The gate independently proves the canonical form of every golden:
// re-serializing the parsed document with byte-sorted object keys
// must reproduce the committed bytes exactly (the Rust canonical
// writer is verified against the same rule in
// crates/lekalo-core/tests/reference_evaluation.rs). A raw-text scan
// additionally rejects duplicate JSON keys, which a parsed-value
// representation cannot see, and the digest manifest must match the
// committed golden bytes.

import { createRequire } from "node:module";
import { readFileSync, readdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "reference-evaluation",
      error: `Ajv 8.17.1 is not reachable through NODE_PATH / LEKALO_AJV_NODE_PATH: ${error}`,
    })}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({
      ok: false,
      gate: "reference-evaluation",
      error: `exact Ajv 8.17.1 is required, found ${ajvVersion}`,
    })}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "reference-evaluation", reason, detail })}\n`,
  );
  process.exit(1);
}

// Raw-text duplicate-key scan: object-aware, so repeated key names in
// sibling objects never count as duplicates.
function duplicateKeys(text) {
  const duplicates = new Set();
  const stack = [new Set()];
  let index = 0;
  while (index < text.length) {
    const character = text[index];
    if (character === '"') {
      let end = index + 1;
      while (end < text.length) {
        if (text[end] === "\\") {
          end += 2;
          continue;
        }
        if (text[end] === '"') break;
        end += 1;
      }
      const content = text.slice(index + 1, end);
      let probe = end + 1;
      while (probe < text.length && /\s/.test(text[probe])) probe += 1;
      if (text[probe] === ":") {
        const keys = stack[stack.length - 1];
        if (keys.has(content)) duplicates.add(content);
        keys.add(content);
      }
      index = end + 1;
      continue;
    }
    if (character === "{") stack.push(new Set());
    else if (character === "}") stack.pop();
    index += 1;
  }
  return [...duplicates];
}

// Canonical form: byte-sorted object keys, compact separators, no
// trailing newline.
function canonicalize(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalize).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const keys = Object.keys(value).sort((a, b) =>
      Buffer.from(a, "utf8").compare(Buffer.from(b, "utf8")),
    );
    return `{${keys
      .map((key) => `${JSON.stringify(key)}:${canonicalize(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

const schemaPath = "contracts/reference-evaluation.schema.v1.0.0.json";
const goldenDir = "tests/fixtures/reference-evaluation/golden";

const ajv = new Ajv2020({ strict: true, allErrors: true });
const schema = JSON.parse(readFileSync(resolve(root, schemaPath), "utf8"));
let validateTrace;
try {
  validateTrace = ajv.compile(schema);
} catch (error) {
  fail("schema:strict-compile", String(error));
}

let goldenFiles = [];
try {
  goldenFiles = readdirSync(resolve(root, goldenDir)).filter((name) =>
    name.endsWith(".json"),
  );
  goldenFiles.sort();
} catch (error) {
  failEarly("golden-dir", String(error));
}
if (goldenFiles.length === 0) {
  fail("goldens", "no committed golden traces found");
}

for (const name of goldenFiles) {
  const raw = readFileSync(resolve(root, goldenDir, name), "utf8");
  if (raw.endsWith("\n")) {
    fail(`${name}:trailing-newline`, "canonical bytes carry no trailing LF");
  }
  for (const key of duplicateKeys(raw)) {
    fail(`${name}:duplicate-key`, key);
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    fail(`${name}:parse`, String(error));
    continue;
  }
  if (validateTrace && !validateTrace(parsed)) {
    fail(`${name}:schema`, JSON.stringify(validateTrace.errors));
  }
  if (!raw.includes('"schemaVersion":"lekalo/reference-evaluation/v1.0.0"')) {
    fail(`${name}:identity`, "wrong schema version discriminator");
  }
  const canonical = canonicalize(parsed);
  if (canonical !== raw) {
    fail(`${name}:canonical-form`, "re-serialization diverges from the committed bytes");
  }
}

// The digest manifest must bind every golden's exact bytes.
const manifestText = readFileSync(resolve(root, goldenDir, "digests.txt"), "utf8");
const manifestEntries = new Map(
  manifestText
    .trim()
    .split("\n")
    .map((line) => {
      const [name, digest] = line.split(" ");
      return [name, digest];
    }),
);
if (manifestEntries.size !== goldenFiles.length) {
  fail(
    "digests:coverage",
    `manifest lists ${manifestEntries.size} of ${goldenFiles.length} goldens`,
  );
}
for (const name of goldenFiles) {
  const raw = readFileSync(resolve(root, goldenDir, name));
  const digest = `sha256:${createHash("sha256").update(raw).digest("hex")}`;
  if (manifestEntries.get(name) !== digest) {
    fail(`${name}:digest`, `manifest ${manifestEntries.get(name)} != actual ${digest}`);
  }
}

// Schema self-check: a tampered trace must be rejected. The vector
// carries an unknown status word, and then drops the member
// entirely; both shapes must fail closed.
const tampered = {
  schemaVersion: "lekalo/reference-evaluation/v1.0.0",
  identity: "dev.lekalo.reference-evaluation@1.0.0",
  semantics: "dev.lekalo.reference-semantics@1.0.0",
  scenario: {
    digest: `sha256:${"0".repeat(64)}`,
    id: "board.scenario.x",
    version: "1.0.0",
  },
  modelVersion: "1.0.0",
  irDigest: `sha256:${"a".repeat(64)}`,
  attachmentRevision: "1.0.0",
  capabilities: { absent: [], supported: [] },
  status: "pass",
  steps: [],
  effects: [],
  effectDigest: `sha256:${"b".repeat(64)}`,
  state: [],
  stateDigest: `sha256:${"c".repeat(64)}`,
  assertions: [],
};
tampered.status = "surprisingly-fine";
if (validateTrace && validateTrace(tampered)) {
  fail("tampered:status", "unknown status must be rejected");
}
delete tampered.status;
if (validateTrace && validateTrace(tampered)) {
  fail("tampered:required", "missing status must be rejected");
}

if (failures.length > 0) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, gate: "reference-evaluation", failures }, null, 2)}\n`,
  );
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({ ok: true, gate: "reference-evaluation", goldens: goldenFiles.length })}\n`,
);
