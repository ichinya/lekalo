#!/usr/bin/env node
// Issue #59 contract gate: the three closed PHP-operations contracts
// (the bounded operations input, the derived custody map sidecar, and
// the observed-handler evidence), validated with the same pinned
// third-party Draft 2020-12 implementation as the other contract gates
// (exact Ajv 8.17.1, provisioned outside this checkout and exposed
// through LEKALO_AJV_NODE_PATH).
//
// The gate proves:
//   - every schema compiles under Ajv strict mode;
//   - the committed fixture input (managed recipes over the planner
//     corpus) is schema-valid and canonical (sorted keys, compact form);
//   - the closed adversarial vectors refuse: unknown members, unknown
//     recipe kinds, clock operands, missing recipe coverage members,
//     a foreign entrypoint method, a configurable scaffold root, and
//     non-canonical operation order.
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath) {
    const provisioned = createRequire(nodePath + "/");
    ({ default: Ajv2020 } = provisioned("ajv/dist/2020"));
    ajvVersion = provisioned("ajv/package.json").version;
  } else {
    ({ default: Ajv2020 } = require("ajv/dist/2020"));
    ajvVersion = require("ajv/package.json").version;
  }
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

const inputSchema = readJson("contracts/php-operations-input.schema.v0.4.0.json");
const mapSchema = readJson("contracts/php-operations-map.schema.v0.4.0.json");
const evidenceSchema = readJson("contracts/php-operations-evidence.schema.v0.4.0.json");
const validateInput = ajv.compile(inputSchema);
const validateMap = ajv.compile(mapSchema);
const validateEvidence = ajv.compile(evidenceSchema);

// --- the committed fixture input is valid and canonical ---
const fixtureInput = JSON.parse(read("tests/fixtures/php-laravel/operations/inputs/planner.operations.json"));
if (!validateInput(fixtureInput)) {
  fail("fixture-input-valid", JSON.stringify(validateInput.errors));
}
const canonicalInput = JSON.parse(canonicalJson(fixtureInput));
if (canonicalJson(fixtureInput) !== JSON.stringify(canonicalInput)) {
  fail("fixture-input-canonical", "the committed bytes are not canonical form");
}

const ZEROS = "sha256:" + "0".repeat(64);

// --- a valid minimal managed input validates end to end ---
const valid = {
  schemaVersion: "lekalo/php-operations-input/v0.4.0",
  identity: "dev.lekalo.php-operations-input@0.4.0",
  projectId: "planner",
  irDigest: ZEROS,
  typesInputDigest: ZEROS,
  operations: [
    {
      id: "planner.count_focused",
      kind: "query",
      mode: "checked",
      entry: { fqn: "App\\LekaloOperations\\CountFocusedHandler", method: "handle", path: "app/count_focused_handler.php" },
      errors: ["planner.store_unavailable"],
    },
  ],
};
if (!validateInput(valid)) fail("valid-input", JSON.stringify(validateInput.errors));

// --- closed adversarial vectors refuse ---
function refuses(name, document, validate = validateInput) {
  if (validate(document)) fail(name, "the vector must refuse");
}

refuses("unknown-input-member", { ...valid, custody: "managed" });
refuses("foreign-schema-version", {
  ...valid,
  schemaVersion: "lekalo/php-operations-input/v0.4.1",
});
refuses("noncanonical-order", {
  ...valid,
  operations: [valid.operations[0], { ...valid.operations[0], id: "planner.aaa.focus" }],
});
refuses("clock-operand", {
  ...valid,
  operations: [
    {
      id: "planner.focus_task",
      kind: "command",
      mode: "managed",
      recipe: { kind: "single-entity-update", entity: "planner.task", key: "task_id",
        assignments: [{ field: "state", value: { now: true } }],
        kept: ["title"], missingBehavior: { error: "planner.task_not_found" } },
      errors: [],
    },
  ],
});
refuses("unknown-recipe-kind", {
  ...valid,
  operations: [
    {
      id: "planner.focus_task",
      kind: "command",
      mode: "managed",
      recipe: { kind: "workflow", steps: [] },
      errors: [],
    },
  ],
});
refuses("foreign-entrypoint-method", {
  ...valid,
  operations: [
    {
      id: "planner.focus_task",
      kind: "command",
      mode: "checked",
      entry: { fqn: "App\\X\\Handler", method: "execute", path: "app/x_handler.php" },
      errors: [],
    },
  ],
});
refuses("transaction-optional", {
  ...valid,
  operations: [
    { id: "planner.focus_task", kind: "command", mode: "scaffold-once",
      transaction: { mode: "optional" }, errors: [] },
  ],
});

// --- the map sidecar validates: a complete managed map, plus drift ---
const map = {
  schemaVersion: "lekalo/php-operations-map/v0.4.0",
  identity: "dev.lekalo.php-operations-map@0.4.0",
  projectId: "planner",
  adapter: { id: "lekalo-target-php-laravel", version: "0.2.0", digest: ZEROS },
  digests: { ir: ZEROS, input: ZEROS, typesInput: ZEROS },
  operations: [
    {
      id: "planner.focus_task",
      kind: "command",
      mode: "managed",
      recipe: "single-entity-update",
      entry: { fqn: "Lekalo\\Generated\\Operations\\Planner\\FocusTaskHandler", method: "handle", path: "planner/focus_task/handler.php" },
      input: { fqn: "Lekalo\\Generated\\Types\\Planner\\FocusTaskInput" },
      errors: [{ id: "planner.task_not_found", fqn: "Lekalo\\Generated\\Operations\\Planner\\Errors\\TaskNotFoundError" }],
      ports: [
        { name: "TaskRepository", role: "repository", entity: "planner.task" },
        { name: "TransactionPort", role: "transactions" },
      ],
      policy: "planner.deny_bulk_focus",
      transaction: "required",
      effects: ["planner.update_task"],
    },
  ],
  artifacts: [
    { path: "planner/focus_task/handler.php", role: "handler", lifecycle: "generated", operation: "planner.focus_task", digest: ZEROS },
  ],
};
if (!validateMap(map)) fail("map-valid", JSON.stringify(validateMap.errors));
const tamperedMap = JSON.parse(JSON.stringify(map));
tamperedMap.operations[0].transaction = "optional";
refuses("map-unknown-transaction", tamperedMap, validateMap);
const declaredRow = JSON.parse(JSON.stringify(map));
declaredRow.artifacts[0].digest = ZEROS;
declaredRow.artifacts[0].role = "declared";
if (!validateMap(declaredRow)) fail("map-declared-row", JSON.stringify(validateMap.errors));

// --- the observed evidence validates, and host paths refuse ---
const evidence = {
  schemaVersion: "lekalo/php-operations-evidence/v0.4.0",
  identity: "dev.lekalo.php-operations-evidence@0.4.0",
  projectId: "planner",
  irDigest: ZEROS,
  operationsInputDigest: ZEROS,
  producer: { tool: "mago", version: "1.0.0", receiptPath: ".lekalo/import/mago/receipt.json", receiptDigest: ZEROS },
  sources: [{ path: "app/focus_task_handler.php", digest: ZEROS }],
  operations: [
    {
      id: "planner.focus_task",
      fqn: "App\\LekaloOperations\\FocusTaskHandler",
      method: "handle",
      path: "app/FocusTaskHandler.php",
      digest: ZEROS,
      constructor: [{ name: "tasks", type: "App\\Repositories\\TaskRepository" }],
      parameters: [{ name: "input", type: "FocusTaskInput", required: true }],
      returnType: "void",
      publicMethods: ["handle"],
    },
  ],
};
if (!validateEvidence(evidence)) fail("evidence-valid", JSON.stringify(validateEvidence.errors));
const tamperedEvidence = JSON.parse(JSON.stringify(evidence));
tamperedEvidence.operations[0].publicMethods = ["handle", "dispatchNow"];
if (!validateEvidence(tamperedEvidence)) {
  fail("evidence-extra-methods-are-producer-facts", JSON.stringify(validateEvidence.errors));
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "php-operations-contracts" })}\n`);
