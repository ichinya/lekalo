#!/usr/bin/env node
// Issue #58 contract gate: the three closed PHP-types contracts (the
// bounded types-input document, the emitted mapping sidecar, and the
// observed class-shape evidence), validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates
// (exact Ajv 8.17.1, provisioned outside this checkout and exposed
// through NODE_PATH / LEKALO_AJV_NODE_PATH).
//
// The gate proves:
//   - every schema compiles under Ajv strict mode;
//   - the committed fixture inputs (managed, scaffold-once, checked)
//     are schema-valid and canonical (sorted keys, compact form);
//   - the stale-digest input is schema-valid (its refusal is a runtime
//     binding decision, not a schema shape);
//   - the committed class-shape evidence golden is schema-valid;
//   - the closed adversarial vectors refuse: unknown members, unknown
//     custody values, a scaffold root outside the closed vocabulary,
//     the scaffold-root/custody pairing rules, and a policy member
//     that would silently provision a library codec.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  failEarly("ajv-unavailable", String(error));
}
if (ajvVersion !== "8.17.1") failEarly("ajv-version", ajvVersion);

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => readFileSync(resolve(root, relative), "utf8");
const readJson = (relative) => JSON.parse(read(relative));

const failures = [];
const fail = (caseName, detail) => failures.push({ case: caseName, detail });
function failEarly(reason, detail) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
}

// --- canonical form: sorted object keys, compact separators ---
function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value && typeof value === "object") {
    const body = Object.keys(value)
      .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`);
    return `{${body.join(",")}}`;
  }
  return JSON.stringify(value);
}

const ajv = new Ajv2020({ strict: true, allErrors: true });

const inputSchema = readJson("contracts/php-types-input.schema.v0.4.0.json");
const mapSchema = readJson("contracts/php-types-map.schema.v0.4.0.json");
const evidenceSchema = readJson("contracts/php-types-evidence.schema.v0.4.0.json");
const validateInput = ajv.compile(inputSchema);
const validateMap = ajv.compile(mapSchema);
const validateEvidence = ajv.compile(evidenceSchema);

// --- the committed fixture inputs are valid and canonical ---
const VALID_INPUTS = [
  "tests/fixtures/php-laravel/types/inputs/planner.types.json",
  "tests/fixtures/php-laravel/types/inputs/planner-scaffold.types.json",
  "tests/fixtures/php-laravel/types/inputs/planner-checked.types.json",
  "tests/fixtures/php-laravel/types/inputs/planner-stale-digest.types.json",
  "tests/fixtures/php-laravel/types/inputs/edge.types.json",
];
let validCount = 0;
for (const relative of VALID_INPUTS) {
  const text = read(relative);
  let document;
  try {
    document = JSON.parse(text);
  } catch (error) {
    fail(`${relative}:json`, String(error));
    continue;
  }
  if (!validateInput(document)) {
    fail(`${relative}:schema`, validateInput.errors);
    continue;
  }
  if (text.trim() !== canonicalJson(document)) {
    fail(`${relative}:canonical`, "the committed bytes are not the canonical form");
    continue;
  }
  validCount += 1;
}

// --- the committed evidence golden is valid ---
const evidenceText = read("tests/fixtures/php-laravel/types/evidence/golden-evidence.json");
if (!validateEvidence(JSON.parse(evidenceText))) {
  fail("evidence-golden:schema", validateEvidence.errors);
}

// --- an emitted sidecar over the corpus is valid (shape authority) ---
// The minimal structural sidecar proves the schema accepts the exact
// member sets the emitter produces; the full emitted document is
// exercised end to end by scripts/test-php-laravel-types.mjs.
const sidecarSample = {
  schemaVersion: "lekalo/php-types-map/v0.4.0",
  identity: "dev.lekalo.php-types-map@0.4.0",
  adapter: { id: "lekalo-target-php-laravel", version: "0.2.0" },
  projectId: "planner",
  custody: "managed",
  namespacePrefix: "Lekalo\\Generated\\Types",
  generatedRoot: ".lekalo/generated/php-laravel/types",
  digests: {
    ir: "sha256:" + "1".repeat(64),
    input: "sha256:" + "2".repeat(64),
  },
  types: [
    {
      semanticId: "planner.task_state",
      kind: "enum",
      fqn: "Lekalo\\Generated\\Types\\Planner\\TaskState",
      path: "planner/task_state.php",
      values: [{ case: "Backlog", value: "backlog" }],
      codec: "Lekalo\\Generated\\Types\\Planner\\TaskState",
    },
    {
      semanticId: "planner.task",
      kind: "entity",
      fqn: "Lekalo\\Generated\\Types\\Planner\\TaskDto",
      path: "planner/task_dto.php",
      fields: [
        {
          name: "task_id",
          property: "taskId",
          type: { leaf: "planner.task_id" },
          presence: "required-nonnull",
          nullable: false,
        },
      ],
      codec: "Lekalo\\Generated\\Types\\Planner\\TaskDtoCodec",
    },
  ],
  artifacts: [
    {
      path: "planner/task_dto.php",
      fqn: "Lekalo\\Generated\\Types\\Planner\\TaskDto",
      role: "type",
      semanticId: "planner.task",
    },
  ],
};
if (!validateMap(sidecarSample)) {
  fail("map-sample:schema", validateMap.errors);
}

// --- the closed adversarial input vectors refuse ---
const rejects = [
  ["unknown-member", { ...JSON.parse(read(VALID_INPUTS[0])), crateDigest: "sha256:" + "3".repeat(64) }],
  ["unknown-custody", {
    ...JSON.parse(read(VALID_INPUTS[0])),
    policy: { custody: "best-effort" },
  }],
  ["unrecognized-scaffold-root", {
    ...JSON.parse(read(VALID_INPUTS[0])),
    policy: { custody: "scaffold-once", scaffoldRoot: "src/Domain/Generated" },
  }],
  ["scaffold-root-without-mode", {
    ...JSON.parse(read(VALID_INPUTS[0])),
    policy: { scaffoldRoot: "app/lekalo-types" },
  }],
  ["scaffold-root-on-managed", {
    ...JSON.parse(read(VALID_INPUTS[0])),
    policy: { custody: "managed", scaffoldRoot: "app/lekalo-types" },
  }],
  ["library-codec-member", {
    ...JSON.parse(read(VALID_INPUTS[0])),
    policy: { custody: "managed", classMap: true, library: { package: "ramsey/uuid" } },
  }],
  ["bad-ir-digest", { ...JSON.parse(read(VALID_INPUTS[0])), irDigest: "md5:deadbeef" }],
  ["dotted-project-id", { ...JSON.parse(read(VALID_INPUTS[0])), projectId: "planner.project" }],
];
let rejectCount = 0;
for (const [name, vector] of rejects) {
  if (validateInput(vector)) {
    fail(`reject:${name}`, "the adversarial vector was accepted");
    continue;
  }
  rejectCount += 1;
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "php-types-contracts",
  validInputs: validCount,
  rejects: rejectCount,
  evidence: true,
  sidecar: true,
})}\n`);
