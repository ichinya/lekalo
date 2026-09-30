#!/usr/bin/env node
// Issue #72 release gate: the client-SDK projection contract, the
// committed golden's canonical form, and the invalid wire vectors,
// validated with the same pinned third-party Draft 2020-12
// implementation as the other contract gates. Exact Ajv 8.17.1 is
// provisioned outside this checkout (CI does the same on Node 18 and
// 24) and exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.
//
// This script is the independent second canonical implementation: it
// re-checks the wire shape and the semantic-identity invariants of
// the committed golden without reusing any Rust code. The byte
// equality of the Rust projection against the golden is the core
// suite's job; the wire-level grammar and bounds are checked here.

import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
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
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2)}\n`,
  );
  process.exit(1);
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const ajv = new Ajv2020({ strict: false, allErrors: true });

// ---------------------------------------------------------------------------
// 1. The schema compiles and the identity constants hold.
// ---------------------------------------------------------------------------
const schema = read("contracts/client-sdk.schema.v0.4.0.json");
const validate = ajv.compile(schema);
if (schema.$id !== "https://dev.lekalo/client-sdk.schema.v0.4.0.json") {
  fail("schema-id", schema.$id);
}
if (schema.properties.schemaVersion.const !== "lekalo/client-sdk/v0.4.0") {
  fail("schema-discriminator", schema.properties.schemaVersion.const);
}
if (schema.properties.identity.const !== "dev.lekalo.client-sdk@0.4.0") {
  fail("schema-identity", schema.properties.identity.const);
}
// This contract was accepted at 0.4.0. Unchanged contracts retain their
// versions across product releases; check-contract-versions enforces the
// current product version only when contract bytes change.

// ---------------------------------------------------------------------------
// 2. The committed golden validates and its bytes are canonical
//    (compact, byte-sorted keys, digest matches the pinned file).
// ---------------------------------------------------------------------------
const goldenText = readText("tests/fixtures/client-sdk/golden/planner.expect.json");
const golden = JSON.parse(goldenText);
if (!validate(golden)) {
  fail("golden-schema", JSON.stringify(validate.errors.slice(0, 6)));
}
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
};
if (goldenText.trimEnd() !== canonical(golden)) {
  fail("golden-noncanonical", "the committed golden is not canonical bytes");
}
const pinnedDigest = readText(
  "tests/fixtures/client-sdk/golden/planner.expect.digest.txt",
).trim();
const computedDigest =
  "sha256:" + createHash("sha256").update(goldenText, "utf8").digest("hex");
if (pinnedDigest !== computedDigest) {
  fail("golden-digest", `pinned=${pinnedDigest} computed=${computedDigest}`);
}

// ---------------------------------------------------------------------------
// 3. The semantic-identity invariants over the golden (the second
//    implementation re-derives them from the committed inputs).
// ---------------------------------------------------------------------------
const attachment = read("tests/fixtures/transport-http/valid/planner.transport.json");
const registry = read("contracts/error-registry.v0.2.16.json");
const bindings = read(
  "tests/fixtures/transport-http/project/lekalo/modules/planner/bindings.yaml",
);
const endpoints = new Map(
  bindings.definitions
    .filter((definition) => definition.kind === "endpoint")
    .map((definition) => [definition.id, definition]),
);
if (golden.operations.length !== attachment.endpoints.length) {
  fail("operation-coverage", `${golden.operations.length} vs ${attachment.endpoints.length}`);
}
const camel = (symbol) =>
  symbol
    .split(".")
    .map((segment, index) =>
      index === 0
        ? segment
        : segment
            .split("_")
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
            .join(""),
    )
    .join("");
for (const operation of golden.operations) {
  const joined = endpoints.get(operation.endpoint);
  if (!joined) fail("endpoint-unjoined", operation.endpoint);
  if (operation.method !== joined.method || operation.path !== joined.path) {
    fail("endpoint-surface", operation.endpoint);
  }
  if (operation.invokes !== joined.invokes) {
    fail("invokes-mismatch", operation.endpoint);
  }
  // The method-name binding is the stable operation id.
  const expectedId = operation.operationId;
  if (operation.ident !== expectedId && operation.ident !== camel(operation.endpoint)) {
    fail("ident-binding", `${operation.endpoint}: ${operation.ident}`);
  }
  // Every error variant preserves the exact registry identity:
  // id, immutable code, closed category, and the projected status
  // equals the transport attachment's declared status.
  const transport = attachment.endpoints.find(
    (endpoint) => endpoint.endpoint === operation.endpoint,
  );
  for (const error of operation.errors ?? []) {
      const contract = registry.errors.find((entry) => entry.id === error.error);
      if (!contract) fail("error-unregistered", error.error);
      if (contract.code !== error.code) fail("error-code-drift", error.error);
      if (contract.category !== error.category) fail("error-category-drift", error.error);
      const declared = transport.errors.find((entry) => entry.error === error.error);
      if (declared && declared.status !== error.status) {
        fail("error-status-drift", error.error);
      }
      // The public payload never carries a private field.
      const publicNames = new Set(
        contract.payload.fields
          .filter((field) => field.exposure === "public")
          .map((field) => field.name),
      );
      for (const field of error.payload ?? []) {
        if (!publicNames.has(field.name)) fail("payload-private-leak", `${error.error}/${field.name}`);
      }
      // The conservative retry matrix, re-derived with the same
      // EffectClass/Idempotency downgrade the Rust `authorize`
      // applies: a `safe` retry on a write/external effect is only
      // safe when the operation is `guaranteed`; otherwise it
      // downgrades to `never` — so a regression emitting `safe` for a
      // non-guaranteed write is caught here.
      const policy = contract.retry.policy;
      const condition = contract.retry.condition;
      let expected;
      if (policy === "never") {
        expected = "never";
      } else if (policy === "safe") {
        const writeish = contract.effect !== "none" && contract.effect !== "read";
        expected = !writeish || contract.idempotency === "guaranteed"
          ? "safe"
          : "never";
      } else if (condition === "reconciliation") {
        expected = "reconciliation-only";
      } else if (
        condition === "idempotency-key"
        && (contract.idempotency === "key-required" || contract.idempotency === "guaranteed")
      ) {
        expected = "key-required";
      } else {
        expected = "never";
      }
      if (error.retry !== expected) fail("retry-derivation", `${error.error}: ${error.retry}`);
  }
}

// ---------------------------------------------------------------------------
// 4. Every invalid wire vector refuses with its closed violation.
// ---------------------------------------------------------------------------
const invalidDir = "tests/fixtures/client-sdk/invalid";
for (const name of readdirSync(resolve(root, invalidDir)).sort()) {
  if (!name.endsWith(".json") || name.endsWith(".expect.json")) continue;
  const expectation = read(`${invalidDir}/${name.replace(".json", ".expect.json")}`);
  if (expectation.schema !== "client-sdk.schema.v0.4.0.json") {
    fail("expectation-target", name);
  }
  const value = read(`${invalidDir}/${name}`);
  if (validate(value)) {
    fail("vector-accepted", name);
  }
  const detail = JSON.stringify(validate.errors);
  if (!detail.includes(expectation.detail)) {
    fail("vector-rule-mismatch", `${name}: ${detail.slice(0, 120)}`);
  }
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      ajv: ajvVersion,
      operations: golden.operations.length,
      types: golden.types.length,
      invalidVectors: readdirSync(resolve(root, invalidDir)).filter(
        (name) => name.endsWith(".json") && !name.endsWith(".expect.json"),
      ).length,
    },
    null,
    2,
  )}\n`,
);
