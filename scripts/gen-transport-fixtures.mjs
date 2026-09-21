#!/usr/bin/env node
// Issue #70 fixture generator: deterministically emits the committed
// transport-http projection goldens from the valid planner attachment
// joined with the fixture Model endpoint symbols.
//
// The projection here is the independent second canonical
// implementation: the Rust `transport_http::project` must reproduce
// these bytes exactly (the core suite byte-compares them), so the
// cross-runtime parity claim rests on two implementations agreeing.
// The goldens are canonical JSON (compact, byte-sorted keys, no
// trailing LF). Run from the repository root:
//
//     node scripts/gen-transport-fixtures.mjs
//
// The script never reads the network and writes only into
// tests/fixtures/transport-http/projected/.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtureRoot = join(root, "tests", "fixtures", "transport-http");

const attachment = JSON.parse(
  readFileSync(join(fixtureRoot, "valid", "planner.transport.json"), "utf8"),
);
// The fixture Model endpoint symbols live in a JSON-syntax YAML home.
const bindings = JSON.parse(
  readFileSync(
    join(fixtureRoot, "project", "lekalo", "modules", "planner", "bindings.yaml"),
    "utf8",
  ),
);
const endpoints = new Map(
  bindings.definitions
    .filter((definition) => definition.kind === "endpoint")
    .map((definition) => [definition.id, definition]),
);

const NAMESPACES = ["go", "laravel", "node", "rust"];

const handlerIdentity = (namespace, projectId, endpointId, invokes) => {
  const module = endpointId.split(".")[0];
  const tail = invokes.split(".").pop();
  // The project root segment: the PascalCase spelling of the project
  // id (mirrors the Rust `handler_root`), never a hardcoded root.
  const root = String(projectId)
    .split(/[^A-Za-z0-9]+/)
    .filter((word) => word.length > 0)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join("");
  if (namespace === "laravel") return `${root}/${module}/${tail}Controller`;
  if (namespace === "go") return `${module}.${tail}Handler`;
  if (namespace === "rust") return `${module}::${tail}Route`;
  return `${module}/${tail}.handler`;
};

const effectiveOperationId = (binding) =>
  binding.operationId ??
  binding.endpoint
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

const route = (binding, namespace) => {
  const symbol = endpoints.get(binding.endpoint);
  if (!symbol) throw new Error(`unresolved endpoint ${binding.endpoint}`);
  const entry = {
    operationId: effectiveOperationId(binding),
    method: symbol.method,
    pathTemplate: symbol.path,
    invokes: symbol.invokes,
    handler: handlerIdentity(namespace, attachment.projectId, binding.endpoint, symbol.invokes),
    decode: {
      params: (binding.params ?? []).map((param) => ({
        name: param.name,
        in: param.in,
        field: param.field,
        required: param.required,
        ...(param.style === undefined ? {} : { style: param.style }),
      })),
      ...(binding.body === undefined
        ? {}
        : {
            body: {
              mode: binding.body.mode,
              ...(binding.body.fields === undefined
                ? {}
                : {
                    fields: binding.body.fields.map((field) => ({
                      name: field.name,
                      field: field.field,
                      required: field.required,
                    })),
                  }),
            },
          }),
    },
    encode: {
      success: {
        status: binding.success.status,
        ...(binding.success.body === undefined
          ? {}
          : {
              body: {
                mode: binding.success.body.mode,
                ...(binding.success.body.fields === undefined
                  ? {}
                  : {
                      fields: binding.success.body.fields.map((field) => ({
                        name: field.name,
                        field: field.field,
                        required: field.required,
                      })),
                    }),
              },
            }),
      },
      ...(binding.errors === undefined
        ? {}
        : {
            errors: binding.errors.map((entry_) => ({
              error: entry_.error,
              status: entry_.status,
            })),
          }),
      errorDefaults: binding.errorDefaults,
    },
    ...(binding.auth === undefined
      ? {}
      : {
          security: {
            actor: binding.auth.actor,
            schemes: binding.auth.schemes,
            ...(binding.auth.policyRef === undefined
              ? {}
              : { policyRef: binding.auth.policyRef }),
          },
        }),
    ...(binding.idempotency === undefined &&
      binding.correlation === undefined
      ? {}
      : {
          headers: [
            ...(binding.idempotency === undefined
              ? []
              : [
                  `${binding.idempotency.header}:${binding.idempotency.required ? "required" : "optional"}`,
                ]),
            ...(binding.correlation === undefined
              ? []
              : [
                  `correlation:${[...binding.correlation.headers].sort().join(",")}`,
                ]),
          ].sort(),
        }),
    ...(binding.pagination === undefined ? {} : { pagination: binding.pagination }),
    ...(binding.rateLimit === undefined ? {} : { rateLimit: binding.rateLimit }),
    ...(binding.cache === undefined ? {} : { cache: binding.cache }),
    ...(binding.apiVersion === undefined ? {} : { apiVersion: binding.apiVersion }),
    ...(binding.capabilities === undefined
      ? {}
      : { capabilities: binding.capabilities }),
  };
  return entry;
};

/** Compact JSON with byte-sorted keys at every level. */
const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .filter((key) => value[key] !== undefined)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
};

for (const namespace of NAMESPACES) {
  const surface = {
    namespace,
    projectId: attachment.projectId,
    wire: attachment.wire.dialect,
    errorEnvelope: attachment.defaults.errorEnvelope,
    routes: [...attachment.endpoints]
      .sort((left, right) => (left.endpoint < right.endpoint ? -1 : left.endpoint > right.endpoint ? 1 : 0))
      .map((binding) => route(binding, namespace)),
  };
  const bytes = canonicalJson(surface);
  const path = join(fixtureRoot, "projected", namespace, `${namespace}.expect.json`);
  writeFileSync(path, bytes);
  process.stdout.write(`${JSON.stringify({ written: path, bytes: bytes.length })}\n`);
}
