#!/usr/bin/env node

// Independent conformance suite for exact Lekalo Model 0.1.0 and 1.0.0.
// It exercises the reference checker as a real subprocess and separately
// interprets every JSON Schema keyword used by the shipped contract.

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  MODEL_COMPAT_GOLDEN_PAIR,
  MODEL_DOCUMENT_SCHEMA,
  MODEL_FIXTURE_SETS,
  fixtureManifestParityFailure,
  manifestFixtureNames,
  manifestFixtureSet,
  manifestIntegrityFailure
} from "./model-fixture-manifest.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checker = join(root, "scripts", "check-model.mjs");
const fixturesDir = join(root, "tests", "fixtures", "model");
const validPlanner = join(fixturesDir, "valid-planner");
const schemaPath = join(root, "contracts", "model.schema.v0.1.0.json");
const fixturesV1Dir = join(root, "tests", "fixtures", "model-v1");
const validPlannerV1 = join(fixturesV1Dir, "valid-planner");
const schemaV1Path = join(root, "contracts", "model.schema.v1.0.0.json");
const semanticIdsPath = join(root, "contracts", "semantic-ids.v0.1.0.json");
const compatibilityDir = join(root, MODEL_COMPAT_GOLDEN_PAIR.root);
const PUBLISHED_V0_SHA256 = "24d772729bb6806cd69f1064e656358ab99a283ae797ff280f9272100c5c7dae";
const SEMANTIC_IDS_SHA256 = "ed3672fbb52f9771677089e8ca7b6ba2f57527296112b4d97535a7ffa1a1038a";

const ALL_KINDS = ["project", "module", "scalar", "enum", "value-object", "entity", "command", "query", "policy", "event", "effect", "endpoint", "scenario", "target-binding"];
// The exact logical consumer mapping of the closed canonical-key list. The
// contract suite must enforce name-to-kind identity, not mere kind
// membership, so each consumer is pinned to its exact kind and the list
// stays at six.
const CANONICAL_KEY_CONSUMER_KINDS = new Map([
  ["context.capsule", "artifact"],
  ["generated-artifacts", "artifact"],
  ["impact", "command"],
  ["inspect", "command"],
  ["target-adapters", "adapter"],
  ["trace.manifest", "manifest"]
]);
const CANONICAL_KEY_CONSUMERS = [...CANONICAL_KEY_CONSUMER_KINDS.keys()];
const V0_FIXTURE_SET = manifestFixtureSet("0.1.0");
const V1_FIXTURE_SET = manifestFixtureSet("1.0.0");
const EXACT_FIXTURES = manifestFixtureNames(V0_FIXTURE_SET);
const SCHEMA_INVALID_FIXTURES = new Set(V0_FIXTURE_SET.schemaInvalid);
const V1_SCHEMA_INVALID_FIXTURES = new Set(V1_FIXTURE_SET.schemaInvalid);
const EXACT_V1_FIXTURES = manifestFixtureNames(V1_FIXTURE_SET);
const DOCUMENT_SCHEMA = MODEL_DOCUMENT_SCHEMA;

const failures = [];
let fixtureCases = 0;
let subprocessCases = 0;
let schemaCases = 0;
let mutationCases = 0;
let unicodeCases = 0;
let readFailureCases = 0;
let hostileKindCases = 0;
let physicalContainmentCases = 0;
let v1FixtureCases = 0;
let semanticIdCases = 0;
let registryCases = 0;
let compatibilityCases = 0;
let duplicateJsonCases = 0;
let unknownFieldPrivacyCases = 0;

const MODEL_USAGE_ENVELOPE = failureEnvelope("invalid", ["model.usage"]);

// Checker subprocesses must stay deterministic against ambient Node
// injection: every ambient NODE_* variable (NODE_OPTIONS, NODE_PATH,
// NODE_V8_COVERAGE, NODE_EXTRA_CA_CERTS, ...) is stripped unless a call
// overrides it explicitly. The exact-Ajv release gate keeps its intentional
// out-of-checkout NODE_PATH because scripts/test-model-ajv.mjs runs
// directly rather than through this helper.
function checkerEnvironment(overrides = {}) {
  const env = {};
  for (const [name, value] of Object.entries(process.env)) {
    if (!/^NODE_/i.test(name)) {
      env[name] = value;
    }
  }
  return Object.assign(env, overrides);
}

function run(args) {
  const result = spawnSync(process.execPath, [checker, ...args], { cwd: root, encoding: "utf8", env: checkerEnvironment() });
  subprocessCases += 1;
  return { code: result.status, stdout: result.stdout, stderr: result.stderr };
}

async function withWindowsExclusiveLock(path, action) {
  const script = [
    "$stream=[System.IO.File]::Open($env:LEKALO_MODEL_LOCK_PATH,[System.IO.FileMode]::Open,[System.IO.FileAccess]::Read,[System.IO.FileShare]::None)",
    "try { [Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); Start-Sleep -Seconds 30 } finally { $stream.Dispose() }"
  ].join("; ");
  const holder = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    env: { ...process.env, LEKALO_MODEL_LOCK_PATH: path },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true
  });
  holder.stdout.setEncoding("utf8");
  holder.stderr.setEncoding("utf8");
  let stderr = "";
  holder.stderr.on("data", (chunk) => { stderr += chunk; });
  await new Promise((resolveReady, rejectReady) => {
    const timeout = setTimeout(() => rejectReady(new Error(`exclusive lock timeout: ${stderr}`)), 5000);
    holder.stdout.on("data", (chunk) => {
      if (chunk.includes("READY")) {
        clearTimeout(timeout);
        resolveReady();
      }
    });
    holder.once("error", (error) => {
      clearTimeout(timeout);
      rejectReady(error);
    });
    holder.once("exit", (code) => {
      clearTimeout(timeout);
      rejectReady(new Error(`exclusive lock holder exited ${code}: ${stderr}`));
    });
  });
  try {
    return action();
  } finally {
    if (holder.exitCode === null) {
      const exited = once(holder, "exit");
      holder.kill();
      await exited;
    }
  }
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function check(name, condition, detail) {
  if (!condition) {
    failures.push({ case: name, detail });
  }
}

function failureEnvelope(status, reasonCodes) {
  return `${JSON.stringify({ status, reasonCodes }, null, 2)}\n`;
}

function hasAbsolutePathLeak(text, paths) {
  const variants = new Set();
  for (const path of paths) {
    variants.add(path);
    variants.add(resolve(path));
    variants.add(path.replaceAll("\\", "/"));
    variants.add(path.replaceAll("/", "\\"));
  }
  return [...variants].some((path) => path.length > 0 && text.includes(path));
}

function expectFailureEnvelope(name, result, status, reasonCodes, forbiddenPaths = []) {
  const expectedCode = status === "denied" ? 3 : 1;
  const expectedText = failureEnvelope(status, reasonCodes);
  const actualText = status === "denied" ? result.stdout : result.stderr;
  check(`${name}:exit`, result.code === expectedCode, { expectedCode, result });
  check(`${name}:json-only`, actualText === expectedText && parseJson(actualText) !== null, { expectedText, result });
  check(`${name}:single-stream`, status === "denied" ? result.stderr === "" : result.stdout === "", result);
  check(`${name}:no-stack`, !/(?:TypeError|ReferenceError|\n\s+at\s|check-model\.mjs:\d+)/.test(actualText), actualText);
  check(`${name}:no-absolute-path`, !hasAbsolutePathLeak(actualText, forbiddenPaths), { forbiddenPaths, actualText });
}

function canonical(value) {
  if (Array.isArray(value)) {
    return `[${value.map(canonical).join(",")}]`;
  }
  if (value !== null && typeof value === "object") {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

function hasExactKeys(value, keys) {
  return value !== null
    && typeof value === "object"
    && !Array.isArray(value)
    && canonical(Object.keys(value).sort()) === canonical([...keys].sort());
}

function collectDifferences(left, right, at = "$") {
  if (canonical(left) === canonical(right)) {
    return [];
  }
  if (Array.isArray(left) && Array.isArray(right)) {
    const differences = [];
    for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
      differences.push(...collectDifferences(left[index], right[index], `${at}/${index}`));
    }
    return differences;
  }
  if (left !== null && right !== null && typeof left === "object" && typeof right === "object"
      && !Array.isArray(left) && !Array.isArray(right)) {
    const differences = [];
    const keys = [...new Set([...Object.keys(left), ...Object.keys(right)])].sort();
    for (const key of keys) {
      differences.push(...collectDifferences(left[key], right[key], `${at}/${key}`));
    }
    return differences;
  }
  return [{ path: at, left, right }];
}

// Hermetic regression for Ajv's strictTypes rule: every type-specific keyword
// must have its applicable type in the same subschema. The shipped checker
// remains dependency-free; the release gate also compiles these bytes with
// pinned Ajv 2020 strict mode.
function strictTypeErrors(schemaNode, at = "$") {
  if (schemaNode === true || schemaNode === false || schemaNode === null || typeof schemaNode !== "object" || Array.isArray(schemaNode)) {
    return [];
  }

  const errors = [];
  const localTypes = Array.isArray(schemaNode.type) ? schemaNode.type : schemaNode.type === undefined ? [] : [schemaNode.type];
  const keywordGroups = [
    ["object", ["additionalProperties", "dependentRequired", "dependentSchemas", "maxProperties", "minProperties", "patternProperties", "properties", "propertyNames", "required"]],
    ["array", ["contains", "items", "maxContains", "maxItems", "minContains", "minItems", "prefixItems", "uniqueItems"]],
    ["string", ["maxLength", "minLength", "pattern"]],
    ["number", ["exclusiveMaximum", "exclusiveMinimum", "maximum", "minimum", "multipleOf"]]
  ];
  for (const [neededType, keywords] of keywordGroups) {
    const used = keywords.filter((keyword) => Object.hasOwn(schemaNode, keyword));
    const hasType = neededType === "number"
      ? localTypes.includes("number") || localTypes.includes("integer")
      : localTypes.includes(neededType);
    if (used.length > 0 && !hasType) {
      errors.push(`${at}: missing local type ${neededType} for ${used.join(",")}`);
    }
  }

  for (const keyword of ["allOf", "anyOf", "oneOf", "prefixItems"]) {
    for (const [index, child] of (schemaNode[keyword] ?? []).entries()) {
      errors.push(...strictTypeErrors(child, `${at}/${keyword}/${index}`));
    }
  }
  for (const keyword of ["$defs", "definitions", "dependentSchemas", "patternProperties", "properties"]) {
    for (const [name, child] of Object.entries(schemaNode[keyword] ?? {})) {
      errors.push(...strictTypeErrors(child, `${at}/${keyword}/${name}`));
    }
  }
  for (const keyword of ["additionalProperties", "contains", "else", "if", "items", "not", "propertyNames", "then", "unevaluatedItems", "unevaluatedProperties"]) {
    if (Object.hasOwn(schemaNode, keyword)) {
      errors.push(...strictTypeErrors(schemaNode[keyword], `${at}/${keyword}`));
    }
  }
  return errors;
}

function resolveRef(schemaRoot, ref) {
  if (typeof ref !== "string" || !ref.startsWith("#/")) {
    throw new Error(`unsupported schema ref: ${String(ref)}`);
  }
  return ref.slice(2).split("/").reduce((node, segment) => node[segment.replace(/~1/g, "/").replace(/~0/g, "~")], schemaRoot);
}

function schemaErrors(value, node, schemaRoot, at = "$") {
  if (node === true) {
    return [];
  }
  if (node === false || node === null || typeof node !== "object") {
    return [`${at}:schema-false`];
  }
  if (node.$ref !== undefined) {
    return schemaErrors(value, resolveRef(schemaRoot, node.$ref), schemaRoot, at);
  }

  const errors = [];
  for (const branch of node.allOf ?? []) {
    errors.push(...schemaErrors(value, branch, schemaRoot, at));
  }
  if (node.oneOf !== undefined) {
    const matches = node.oneOf.filter((branch) => schemaErrors(value, branch, schemaRoot, at).length === 0).length;
    if (matches !== 1) {
      errors.push(`${at}:oneOf:${matches}`);
    }
  }
  if (node.anyOf !== undefined) {
    const matches = node.anyOf.filter((branch) => schemaErrors(value, branch, schemaRoot, at).length === 0).length;
    if (matches < 1) {
      errors.push(`${at}:anyOf:0`);
    }
  }
  if (node.not !== undefined && schemaErrors(value, node.not, schemaRoot, at).length === 0) {
    errors.push(`${at}:not`);
  }
  if (node.const !== undefined && canonical(value) !== canonical(node.const)) {
    errors.push(`${at}:const`);
  }
  if (node.enum !== undefined && !node.enum.some((entry) => canonical(entry) === canonical(value))) {
    errors.push(`${at}:enum`);
  }

  const typeMatches = node.type === undefined
    || (node.type === "object" && value !== null && typeof value === "object" && !Array.isArray(value))
    || (node.type === "array" && Array.isArray(value))
    || (node.type === "string" && typeof value === "string")
    || (node.type === "integer" && Number.isInteger(value));
  if (!typeMatches) {
    errors.push(`${at}:type:${node.type}`);
    return errors;
  }

  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    if (node.minProperties !== undefined && Object.keys(value).length < node.minProperties) {
      errors.push(`${at}:minProperties`);
    }
    if (node.maxProperties !== undefined && Object.keys(value).length > node.maxProperties) {
      errors.push(`${at}:maxProperties`);
    }
    for (const required of node.required ?? []) {
      if (!Object.hasOwn(value, required)) {
        errors.push(`${at}:required:${required}`);
      }
    }
    const properties = node.properties ?? {};
    for (const [key, child] of Object.entries(properties)) {
      if (Object.hasOwn(value, key)) {
        errors.push(...schemaErrors(value[key], child, schemaRoot, `${at}/${key}`));
      }
    }
    if (node.additionalProperties === false) {
      for (const key of Object.keys(value)) {
        if (!Object.hasOwn(properties, key)) {
          errors.push(`${at}:additional:${key}`);
        }
      }
    }
  }
  if (Array.isArray(value)) {
    if (node.minItems !== undefined && value.length < node.minItems) {
      errors.push(`${at}:minItems`);
    }
    if (node.maxItems !== undefined && value.length > node.maxItems) {
      errors.push(`${at}:maxItems`);
    }
    if (node.uniqueItems === true && new Set(value.map(canonical)).size !== value.length) {
      errors.push(`${at}:uniqueItems`);
    }
    if (node.items !== undefined) {
      value.forEach((entry, index) => errors.push(...schemaErrors(entry, node.items, schemaRoot, `${at}/${index}`)));
    }
  }
  if (typeof value === "string") {
    if (node.minLength !== undefined && [...value].length < node.minLength) {
      errors.push(`${at}:minLength`);
    }
    if (node.maxLength !== undefined && [...value].length > node.maxLength) {
      errors.push(`${at}:maxLength`);
    }
    if (node.pattern !== undefined && !new RegExp(node.pattern, "u").test(value)) {
      errors.push(`${at}:pattern`);
    }
  }
  if (typeof value === "number" && node.minimum !== undefined && value < node.minimum) {
    errors.push(`${at}:minimum`);
  }
  return errors;
}

async function modelDocuments(projectRoot) {
  const output = [];
  async function walk(path) {
    for (const entry of (await readdir(path, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) {
        await walk(child);
      } else if (DOCUMENT_SCHEMA[entry.name] !== undefined) {
        output.push(child);
      }
    }
  }
  await walk(join(projectRoot, "lekalo"));
  return output;
}

async function validateFixtureSchema(projectRoot, schema) {
  const errors = [];
  let parseFailed = false;
  for (const path of await modelDocuments(projectRoot)) {
    const text = await readFile(path, "utf8");
    const document = parseJson(text);
    if (document === null) {
      parseFailed = true;
      errors.push(`${path}:parse`);
      continue;
    }
    errors.push(...schemaErrors(document, schema.$defs[DOCUMENT_SCHEMA[basename(path)]], schema, path));
    errors.push(...schemaErrors(document, schema, schema, `${path}:umbrella`));
  }
  return { errors, parseFailed };
}

const schema = parseJson(await readFile(schemaPath, "utf8"));
schemaCases += 1;
check("schema:parses", schema !== null, schemaPath);
schemaCases += 1;
check("schema:id-versioned", schema?.$id === "https://lekalo.dev/schemas/model/0.1.0/schema.json", schema?.$id);
schemaCases += 1;
check("schema:version-const", schema?.$defs?.schemaVersion?.const === "0.1.0", schema?.$defs?.schemaVersion);
schemaCases += 1;
const kindEnum = schema?.$defs?.commonFields?.properties?.kind?.enum;
check("schema:all-14-kinds", Array.isArray(kindEnum) && canonical([...kindEnum].sort()) === canonical([...ALL_KINDS].sort()), kindEnum);
schemaCases += 1;
const defName = (kind) => `${kind.replace(/-([a-z])/g, (_, char) => char.toUpperCase())}Definition`;
check(
  "schema:closed-kind-shapes",
  ALL_KINDS.every((kind) => schema?.$defs?.[defName(kind)]?.allOf?.some((branch) => branch.type === "object" && branch.additionalProperties === false)),
  "every final kind branch must be closed"
);
schemaCases += 1;
check(
  "schema:common-fields-extendable",
  schema?.$defs?.commonFields?.additionalProperties !== false,
  "a reusable allOf branch must not reject kind-specific fields"
);
schemaCases += 1;
check(
  "schema:type-depth-machine-readable",
  schema?.$defs?.typeExpression?.oneOf?.[1]?.properties?.list?.$ref === "#/$defs/typeExpressionLevel2"
    && schema?.$defs?.typeExpressionLevel4?.$ref === "#/$defs/typeExpressionLeaf",
  "finite depth chain"
);
schemaCases += 1;
const schemaText = await readFile(schemaPath, "utf8");
check("schema:target-neutral", !/\b(PHP|Laravel|Node\.js|TypeScript|Go|Rust|npm|composer)\b/.test(schemaText), "target-specific token in schema");
schemaCases += 1;
const strictTypes = strictTypeErrors(schema);
check("schema:ajv-strict-types", strictTypes.length === 0, strictTypes);
schemaCases += 1;
check(
  "schema:document-specializations-have-local-types",
  Object.values(DOCUMENT_SCHEMA).every((name) => {
    const branch = schema?.$defs?.[name]?.allOf?.[1];
    return branch?.type === "object" && branch?.properties?.definitions?.type === "array";
  }),
  "every document specialization must declare object/array types in the same subschema as properties/items"
);

const fixtureDirs = (await readdir(fixturesDir, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
check("fixtures:exact-scope", canonical(fixtureDirs) === canonical(EXACT_FIXTURES), { expected: EXACT_FIXTURES, actual: fixtureDirs });
check("fixture-manifest:integrity", manifestIntegrityFailure() === null, manifestIntegrityFailure());
check("fixture-manifest:v0-parity", fixtureManifestParityFailure(V0_FIXTURE_SET, fixtureDirs) === null, fixtureManifestParityFailure(V0_FIXTURE_SET, fixtureDirs));

// Direct mutation probes for manifestIntegrityFailure: every probe hands the
// function directly mutated manifest inputs and must be rejected (except the
// unmutated baseline), proving the compatibility golden pair fails closed on
// missing, extra, duplicate, swapped, mismatched, malformed, and missing or
// empty field values instead of matching versions and schemas independently.
// Duplicate-side proofs are count-neutral: the second side is replaced in
// place, so rejection comes from the integrity rules themselves rather than
// from a changed side count.
const integrityProbeFixtureSets = structuredClone(MODEL_FIXTURE_SETS);
const integrityProbeGoldenPair = structuredClone(MODEL_COMPAT_GOLDEN_PAIR);
let integrityProbeCases = 0;
for (const [probeName, mustReject, mutate] of [
  ["unmutated-manifest-accepted", false, () => {}],
  ["missing-golden-side-rejected", true, (_sets, golden) => { golden.sides.shift(); }],
  ["extra-golden-side-rejected", true, (_sets, golden) => {
    golden.sides.push({ version: "9.9.9", schema: "contracts/model.schema.v9.9.9.json", directory: "9.9.9" });
  }],
  ["duplicate-golden-side-rejected", true, (_sets, golden) => { golden.sides[1] = structuredClone(golden.sides[0]); }],
  ["duplicate-golden-version-rejected", true, (_sets, golden) => { golden.sides[1].version = golden.sides[0].version; }],
  ["duplicate-golden-schema-rejected", true, (_sets, golden) => { golden.sides[1].schema = golden.sides[0].schema; }],
  ["duplicate-golden-directory-rejected", true, (_sets, golden) => { golden.sides[1].directory = golden.sides[0].directory; }],
  ["swapped-golden-pair-rejected", true, (sets, golden) => {
    golden.sides[0].schema = sets[1].schema;
    golden.sides[1].schema = sets[0].schema;
  }],
  ["mismatched-golden-schema-rejected", true, (_sets, golden) => {
    golden.sides[0].schema = "contracts/model.schema.v9.9.9.json";
  }],
  ["missing-golden-directory-rejected", true, (_sets, golden) => { delete golden.sides[1].directory; }],
  ["empty-golden-directory-rejected", true, (_sets, golden) => { golden.sides[1].directory = ""; }],
  ["malformed-golden-version-rejected", true, (_sets, golden) => { golden.sides[0].version = 0.1; }],
  ["empty-golden-root-rejected", true, (_sets, golden) => { golden.root = ""; }],
  ["missing-golden-root-rejected", true, (_sets, golden) => { delete golden.root; }],
  ["empty-fixture-set-version-rejected", true, (sets) => { sets[0].version = ""; }],
  ["malformed-fixture-set-schema-rejected", true, (sets) => { sets[1].schema = null; }],
  ["missing-fixture-set-root-rejected", true, (sets) => { delete sets[0].fixtures; }],
  ["golden-side-without-fixture-set-rejected", true, (sets) => { sets.pop(); }],
  ["duplicate-fixture-set-version-rejected", true, (sets, golden) => {
    sets.push(structuredClone(sets[0]));
    golden.sides.push(structuredClone(golden.sides[0]));
  }]
]) {
  const probeSets = structuredClone(integrityProbeFixtureSets);
  const probeGolden = structuredClone(integrityProbeGoldenPair);
  mutate(probeSets, probeGolden);
  const probeFailure = manifestIntegrityFailure(probeSets, probeGolden);
  check(`fixture-manifest:integrity-probe:${probeName}`, mustReject ? probeFailure !== null : probeFailure === null, probeFailure);
  integrityProbeCases += 1;
}

const conformance = run([]);
fixtureCases += 1;
const conformanceDoc = parseJson(conformance.stdout);
check("conformance:exit", conformance.code === 0, conformance);
check("conformance:counts", conformanceDoc?.status === "valid" && conformanceDoc.fixtures?.valid === 1 && conformanceDoc.fixtures?.invalid === 12, conformanceDoc);

for (const name of fixtureDirs) {
  fixtureCases += 1;
  const selector = `tests/fixtures/model/${name}`;
  const result = run(["--project", selector]);
  const independent = await validateFixtureSchema(join(fixturesDir, name), schema);
  if (name === "valid-planner") {
    const doc = parseJson(result.stdout);
    check(`fixture:${name}:exit`, result.code === 0 && result.stderr === "", result);
    check(`fixture:${name}:status`, doc?.status === "valid", doc);
    check(`fixture:${name}:all-kinds`, canonical(Object.keys(doc?.kinds ?? {}).sort()) === canonical([...ALL_KINDS].sort()), doc?.kinds);
    check(`fixture:${name}:schema`, independent.errors.length === 0, independent.errors);
    continue;
  }
  const expectation = parseJson(await readFile(join(fixturesDir, name, "expect.json"), "utf8"));
  const doc = parseJson(result.stderr);
  check(`fixture:${name}:expect`, expectation !== null, name);
  check(`fixture:${name}:exit`, result.code === 1 && result.stdout === "", result);
  check(`fixture:${name}:reason`, doc?.reasonCodes?.[0] === expectation?.reasonCodes?.[0], { expected: expectation?.reasonCodes?.[0], actual: doc?.reasonCodes?.[0] });
  if (SCHEMA_INVALID_FIXTURES.has(name)) {
    check(`fixture:${name}:schema-invalid`, independent.errors.length > 0, independent);
  } else {
    check(`fixture:${name}:schema-valid-semantic-invalid`, independent.errors.length === 0, independent.errors);
  }
}

for (const [name, args, expectedReason] of [
  ["missing-project", ["--project", "tests/fixtures/model/does-not-exist"], "model.structure-invalid"],
  ["unknown-option", ["--wat"], "model.usage"],
  ["missing-value", ["--project"], "model.usage"],
  ["option-as-value", ["--project", "--project"], "model.usage"],
  ["duplicate-project", ["--project", "tests/fixtures/model/valid-planner", "--project", "tests/fixtures/model/valid-planner"], "model.usage"]
]) {
  const result = run(args);
  check(`cli:${name}:exit`, result.code === 1 && result.stdout === "", result);
  check(`cli:${name}:reason`, parseJson(result.stderr)?.reasonCodes?.[0] === expectedReason, result.stderr);
  if (expectedReason === "model.usage") {
    check(`cli:${name}:static-envelope`, result.stderr === MODEL_USAGE_ENVELOPE, { expected: MODEL_USAGE_ENVELOPE, actual: result.stderr });
  }
}

// Raw argv is operator-controlled and may carry secrets, paths, or canaries.
// The usage envelope must stay byte-exact and free of any echo whatever the
// offending token contains.
for (const [name, args] of [
  ["canary-flag", ["--SECRET_SOURCE_PAYLOAD"]],
  ["canary-path-flag", ["--C:\\Users\\operator\\secret.key"]],
  ["canary-unc-flag", ["--\\\\?\\UNC\\host\\share\\secret.key"]],
  ["canary-unicode-flag", ["--planner.focüs🙂"]],
  ["canary-control-flag", ["--\u0007\u001b[31m"]],
  ["unknown-option-with-secret-value", ["--wat", "SECRET_SOURCE_PAYLOAD"]],
  ["duplicate-check-id-secret", ["--check-id", "planner.focus", "--check-id", "C:\\secret\\path"]]
]) {
  const result = run(args);
  check(`cli:${name}:usage-exit`, result.code === 1 && result.stdout === "", result);
  check(`cli:${name}:usage-no-echo`, result.stderr === MODEL_USAGE_ENVELOPE, { expected: MODEL_USAGE_ENVELOPE, actual: result.stderr });
}
check("cli:usage-deterministic", run(["--SECRET_SOURCE_PAYLOAD"]).stderr === MODEL_USAGE_ENVELOPE, MODEL_USAGE_ENVELOPE);

const first = run(["--project", "tests/fixtures/model/valid-planner"]);
const second = run(["--project", "tests/fixtures/model/valid-planner"]);
check("determinism:project", first.code === second.code && first.stdout === second.stdout && first.stderr === second.stderr, { first, second });

// Ambient Node injection regression: a hostile NODE_OPTIONS must never reach
// the checker subprocess because checkerEnvironment strips ambient NODE_*
// variables, so the poisoned run stays byte-identical to the clean baseline.
const ambientNodeOptions = process.env.NODE_OPTIONS;
process.env.NODE_OPTIONS = "--require ./lekalo-ambient-node-options-probe.mjs";
const poisoned = run(["--project", "tests/fixtures/model/valid-planner"]);
if (ambientNodeOptions === undefined) {
  delete process.env.NODE_OPTIONS;
} else {
  process.env.NODE_OPTIONS = ambientNodeOptions;
}
check("determinism:ambient-node-options-sanitized", poisoned.code === first.code && poisoned.stdout === first.stdout && poisoned.stderr === first.stderr, poisoned);

const schemaV1Text = await readFile(schemaV1Path, "utf8");
const schemaV1 = parseJson(schemaV1Text);
const semanticIdsText = await readFile(semanticIdsPath, "utf8");
const semanticIds = parseJson(semanticIdsText);
schemaCases += 1;
check(
  "schema:v0-published-bytes-immutable",
  createHash("sha256").update(schemaText, "utf8").digest("hex") === PUBLISHED_V0_SHA256,
  createHash("sha256").update(schemaText, "utf8").digest("hex")
);
schemaCases += 1;
check("schema:v1-parses", schemaV1 !== null, schemaV1Path);
schemaCases += 1;
check("schema:v1-id-versioned", schemaV1?.$id === "https://lekalo.dev/schemas/model/1.0.0/schema.json", schemaV1?.$id);
schemaCases += 1;
check("schema:v1-version-const", schemaV1?.$defs?.schemaVersion?.const === "1.0.0", schemaV1?.$defs?.schemaVersion);
schemaCases += 1;
check(
  "schema:v1-segment-exact",
  schemaV1?.$defs?.segmentId?.pattern === "^[a-z][a-z0-9_]{0,62}$"
    && schemaV1?.$defs?.segmentId?.minLength === 1
    && schemaV1?.$defs?.segmentId?.maxLength === 63,
  schemaV1?.$defs?.segmentId
);
schemaCases += 1;
check(
  "schema:v1-symbol-class",
  schemaV1?.$defs?.symbolId?.pattern === "^[a-z][a-z0-9_]{0,62}\\.[a-z][a-z0-9_]{0,62}(\\.[a-z][a-z0-9_]{0,62})?$"
    && schemaV1?.$defs?.symbolId?.minLength === 3
    && schemaV1?.$defs?.symbolId?.maxLength === 191,
  schemaV1?.$defs?.symbolId
);
schemaCases += 1;
check(
  "schema:v1-reserved-project-module",
  ["projectId", "moduleId"].every((name) => canonical(schemaV1?.$defs?.[name]?.allOf?.[1]?.not?.enum) === canonical(["lekalo", "dev"])),
  { projectId: schemaV1?.$defs?.projectId, moduleId: schemaV1?.$defs?.moduleId }
);
schemaCases += 1;
check(
  "schema:v1-symbol-only-history",
  ["project", "module"].every((kind) => !Object.hasOwn(schemaV1?.$defs?.[defName(kind)]?.allOf?.[1]?.properties ?? {}, "renamed_from"))
    && [...ALL_KINDS].filter((kind) => kind !== "project" && kind !== "module").every((kind) =>
      schemaV1?.$defs?.[defName(kind)]?.allOf?.[1]?.properties?.renamed_from?.$ref === "#/$defs/renamedFrom"),
  "renamed_from must exist only on the twelve symbol kinds"
);
schemaCases += 1;
check(
  "schema:v1-registry-closed",
  schemaV1?.$defs?.idRegistry?.type === "object"
    && schemaV1?.$defs?.idRegistry?.additionalProperties === false
    && schemaV1?.$defs?.idRegistry?.minProperties === 1
    && schemaV1?.$defs?.idRegistry?.properties?.rename_history?.minItems === 1
    && schemaV1?.$defs?.idRegistry?.properties?.tombstones?.minItems === 1,
  schemaV1?.$defs?.idRegistry
);
schemaCases += 1;
check(
  "schema:v1-finite-type-depth",
  schemaV1?.$defs?.typeExpression?.oneOf?.[1]?.properties?.list?.$ref === "#/$defs/typeExpressionLevel2"
    && schemaV1?.$defs?.typeExpressionLevel4?.$ref === "#/$defs/typeExpressionLeaf",
  "finite depth chain"
);
schemaCases += 1;
check("schema:v1-common-fields-extendable", schemaV1?.$defs?.commonFields?.additionalProperties !== false, schemaV1?.$defs?.commonFields);
schemaCases += 1;
check("schema:v1-ajv-strict-types", strictTypeErrors(schemaV1).length === 0, strictTypeErrors(schemaV1));
schemaCases += 1;
check("schema:v1-target-neutral", !/\b(PHP|Laravel|Node\.js|TypeScript|Go|Rust|npm|composer)\b/.test(schemaV1Text), "target-specific token in schema");

const allowedAddedDefs = new Set([
  "segmentId", "projectId", "moduleId", "symbolId", "renamedFrom",
  "renameHistoryEntry", "tombstoneEntry", "idRegistry"
]);
const symbolDefinitionNames = new Set([...ALL_KINDS]
  .filter((kind) => kind !== "project" && kind !== "module")
  .map(defName));
const unexpectedSchemaDifferences = collectDifferences(schema, schemaV1).filter((difference) => {
  const path = difference.path;
  if (["$/$id", "$/title", "$/description", "$/$defs/schemaVersion/const", "$/$defs/definitionId"].includes(path)) return false;
  const addedDefinition = path.match(/^\$\/\$defs\/([^/]+)/)?.[1];
  if (addedDefinition !== undefined && allowedAddedDefs.has(addedDefinition)) return false;
  if (path.startsWith("$/$defs/commonFields/properties/id")) return false;
  if (difference.left === "#/$defs/definitionId" && difference.right === "#/$defs/symbolId") return false;
  if (path.startsWith("$/$defs/projectDefinition/allOf/1/properties/id")) return false;
  if (path.startsWith("$/$defs/projectDefinition/allOf/1/properties/id_registry")) return false;
  if (path.startsWith("$/$defs/moduleDefinition/allOf/1/properties/id")) return false;
  if (path.startsWith("$/$defs/moduleDefinition/allOf/1/properties/imports/items")) return false;
  const definitionMatch = path.match(/^\$\/\$defs\/([^/]+)\/allOf\/1\/properties\/(id|renamed_from)/);
  return !(definitionMatch && symbolDefinitionNames.has(definitionMatch[1]));
});
schemaCases += 1;
check("schema:v1-only-version-id-registry-diff", unexpectedSchemaDifferences.length === 0, unexpectedSchemaDifferences);

schemaCases += 1;
check("semantic-contract:parses", semanticIds !== null, semanticIdsPath);
schemaCases += 1;
check(
  "semantic-contract:closed-bytes",
  createHash("sha256").update(semanticIdsText, "utf8").digest("hex") === SEMANTIC_IDS_SHA256,
  createHash("sha256").update(semanticIdsText, "utf8").digest("hex")
);
schemaCases += 1;
check(
  "semantic-contract:closed-shape",
  hasExactKeys(semanticIds, [
    "contractId", "version", "status", "closed", "releaseBinding", "segment",
    "idClasses", "kindNamespaces", "canonicalOrdering", "identityHistory",
    "renameGraph", "tombstones", "targetCanonicalKey", "migration", "diagnostics"
  ])
    && hasExactKeys(semanticIds?.releaseBinding, ["productCandidate", "modelSchemaSuccessor", "versionsIndependent"])
    && hasExactKeys(semanticIds?.segment, [
      "pattern", "encoding", "minBytes", "maxBytes", "normalization", "caseFolding",
      "wordSeparator", "forbiddenExamples"
    ])
    && hasExactKeys(semanticIds?.idClasses, ["projectId", "moduleId", "symbolId"])
    && hasExactKeys(semanticIds?.kindNamespaces, [
      "optional", "exactKindTokenRequiredWhenPresent", "kindToTokenTransform", "tokens", "fuzzyEquivalence"
    ])
    && hasExactKeys(semanticIds?.canonicalOrdering, [
      "encoding", "comparisonUnit", "direction", "completeDottedId", "sourceOrderRequired", "localeComparison"
    ])
    && hasExactKeys(semanticIds?.identityHistory, [
      "scope", "projectAndModuleIdsImmutable", "renamedFromField", "renamedFromMeaning", "historicalIdsResolveAsLiveReferences"
    ])
    && hasExactKeys(semanticIds?.renameGraph, [
      "registryPath", "functional", "acyclic", "sourceMayBeLive", "sourceMayBeTombstoned", "target",
      "chainTermination", "directIncomingMustEqualRenamedFrom", "versionField", "versionRule", "convergence"
    ])
    && hasExactKeys(semanticIds?.tombstones, [
      "registryPath", "reasons", "unique", "disjointFromLiveAndRenameGraph", "permanentNonReuse",
      "deletedAllowsReplacedBy", "replacedRequiresLiveReplacedBy", "sinceIsProjectDefinitionVersion",
      "sinceMustNotExceedProjectVersion"
    ])
    && hasExactKeys(semanticIds?.targetCanonicalKey, [
      "source", "verbatim", "transformation", "caseFolding", "truncation", "translation", "adapterMayInventCanonicalId",
      "canonicalKeyConsumers"
    ])
    && hasExactKeys(semanticIds?.migration, [
      "automaticChangeAllowed", "ownerAuthoredChangeRequiredForNonconformingIds", "autoCaseFold",
      "autoTruncate", "autoTranslateHyphens"
    ])
    && hasExactKeys(semanticIds?.diagnostics, ["precedence", "semanticIdCodes"]),
  semanticIds
);
schemaCases += 1;
check(
  "semantic-contract:identity",
  semanticIds?.contractId === "dev.lekalo.semantic-ids"
    && semanticIds?.version === "0.1.0"
    && semanticIds?.closed === true
    && semanticIds?.releaseBinding?.productCandidate === "0.1.4"
    && semanticIds?.releaseBinding?.versionsIndependent === true,
  semanticIds?.releaseBinding
);
schemaCases += 1;
check(
  "semantic-contract:grammar-parity",
  semanticIds?.segment?.pattern === schemaV1?.$defs?.segmentId?.pattern
    && semanticIds?.idClasses?.symbolId?.maxLength === schemaV1?.$defs?.symbolId?.maxLength
    && canonical(semanticIds?.idClasses?.projectId?.reserved) === canonical(["lekalo", "dev"])
    && canonical(semanticIds?.idClasses?.moduleId?.reserved) === canonical(["lekalo", "dev"]),
  semanticIds?.idClasses
);
schemaCases += 1;
check(
  "semantic-contract:kind-tokens",
  canonical(semanticIds?.kindNamespaces?.tokens) === canonical([
    "scalar", "enum", "value_object", "entity", "command", "query", "policy",
    "event", "effect", "endpoint", "scenario", "target_binding"
  ]),
  semanticIds?.kindNamespaces?.tokens
);
schemaCases += 1;
check(
  "semantic-contract:history-and-target-boundaries",
  semanticIds?.identityHistory?.scope === "symbol-only"
    && semanticIds?.identityHistory?.projectAndModuleIdsImmutable === true
    && semanticIds?.identityHistory?.historicalIdsResolveAsLiveReferences === false
    && semanticIds?.renameGraph?.convergence?.requiredOnEveryConvergingEdge === true
    && semanticIds?.renameGraph?.functional === true
    && semanticIds?.renameGraph?.acyclic === true
    && semanticIds?.renameGraph?.sourceMayBeLive === false
    && semanticIds?.renameGraph?.sourceMayBeTombstoned === false
    && semanticIds?.renameGraph?.directIncomingMustEqualRenamedFrom === true
    && semanticIds?.tombstones?.permanentNonReuse === true
    && semanticIds?.tombstones?.unique === true
    && semanticIds?.tombstones?.disjointFromLiveAndRenameGraph === true
    && semanticIds?.tombstones?.deletedAllowsReplacedBy === false
    && semanticIds?.tombstones?.replacedRequiresLiveReplacedBy === true
    && semanticIds?.targetCanonicalKey?.verbatim === true
    && semanticIds?.targetCanonicalKey?.transformation === "none"
    && semanticIds?.targetCanonicalKey?.caseFolding === false
    && semanticIds?.targetCanonicalKey?.truncation === false
    && semanticIds?.targetCanonicalKey?.translation === false
    && semanticIds?.targetCanonicalKey?.adapterMayInventCanonicalId === false
    && semanticIds?.migration?.ownerAuthoredChangeRequiredForNonconformingIds === true
    && semanticIds?.migration?.autoCaseFold === false
    && semanticIds?.migration?.autoTruncate === false
    && semanticIds?.migration?.autoTranslateHyphens === false,
  semanticIds
);
schemaCases += 1;
check(
  "semantic-contract:canonical-key-consumers-closed",
  hasExactKeys(semanticIds?.targetCanonicalKey?.canonicalKeyConsumers, ["closed", "binding", "entries", "implementedSurfaces"])
    && semanticIds?.targetCanonicalKey?.canonicalKeyConsumers?.closed === true
    && typeof semanticIds?.targetCanonicalKey?.canonicalKeyConsumers?.binding === "string"
    && semanticIds.targetCanonicalKey.canonicalKeyConsumers.binding.includes("verbatim")
    && Array.isArray(semanticIds.targetCanonicalKey.canonicalKeyConsumers.entries)
    && semanticIds.targetCanonicalKey.canonicalKeyConsumers.entries.length === CANONICAL_KEY_CONSUMERS.length
    && canonical(semanticIds.targetCanonicalKey.canonicalKeyConsumers.entries.map((entry) => entry?.name).sort()) === canonical(CANONICAL_KEY_CONSUMERS)
    && semanticIds.targetCanonicalKey.canonicalKeyConsumers.entries.every((entry) =>
      hasExactKeys(entry, ["kind", "name", "status"])
        && CANONICAL_KEY_CONSUMER_KINDS.get(entry.name) === entry.kind
        && entry.status === "deferred")
    && semanticIds?.targetCanonicalKey?.canonicalKeyConsumers?.implementedSurfaces?.length === 1
    && semanticIds.targetCanonicalKey.canonicalKeyConsumers.implementedSurfaces[0]?.name === "model-checker"
    && semanticIds.targetCanonicalKey.canonicalKeyConsumers.implementedSurfaces[0]?.status === "implemented",
  semanticIds?.targetCanonicalKey?.canonicalKeyConsumers
);
schemaCases += 1;
check(
  "semantic-contract:canonical-order",
  semanticIds?.canonicalOrdering?.encoding === "UTF-8"
    && semanticIds?.canonicalOrdering?.comparisonUnit === "unsigned-byte"
    && semanticIds?.canonicalOrdering?.direction === "ascending"
    && semanticIds?.canonicalOrdering?.completeDottedId === true
    && semanticIds?.canonicalOrdering?.sourceOrderRequired === false
    && semanticIds?.canonicalOrdering?.localeComparison === false,
  semanticIds?.canonicalOrdering
);
schemaCases += 1;
check(
  "semantic-contract:diagnostics-closed",
  canonical(semanticIds?.diagnostics?.precedence) === canonical([
    "structure containment",
    "document parse and closed shape",
    "exact and uniform schema version",
    "ID grammar, class, and reserved IDs",
    "module and live-symbol uniqueness",
    "module qualification and kind namespace",
    "rename and tombstone registry",
    "live-only reference resolution",
    "named-type recursion"
  ])
    && new Set(semanticIds?.diagnostics?.semanticIdCodes ?? []).size === 19
    && semanticIds?.diagnostics?.semanticIdCodes?.every((code) => code.startsWith("semantic-id.")),
  semanticIds?.diagnostics
);

// Issue #6 is a contract-foundation issue: the docs must enumerate exactly
// the machine contract's closed canonical-key consumer list and mark every
// listed consumer deferred, with the checker as the only implemented surface.
const semanticIdsDocText = await readFile(join(root, "docs", "semantic-ids.md"), "utf8");
const targetKeysSection = semanticIdsDocText.split("## Target canonical keys")[1]?.split(/\n## /)[0] ?? "";
schemaCases += 1;
check(
  "docs:semantic-ids-canonical-consumer-parity",
  targetKeysSection.length > 0
    && CANONICAL_KEY_CONSUMERS.every((name) => targetKeysSection.includes(name))
    && /closed/i.test(targetKeysSection)
    && /deferred/i.test(targetKeysSection)
    && /verbatim/i.test(targetKeysSection),
  { sectionLength: targetKeysSection.length }
);

const fixtureDirsV1 = (await readdir(fixturesV1Dir, { withFileTypes: true }))
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
check("fixtures:v1-exact-scope", canonical(fixtureDirsV1) === canonical(EXACT_V1_FIXTURES), {
  expected: EXACT_V1_FIXTURES,
  actual: fixtureDirsV1
});
check("fixture-manifest:v1-parity", fixtureManifestParityFailure(V1_FIXTURE_SET, fixtureDirsV1) === null, fixtureManifestParityFailure(V1_FIXTURE_SET, fixtureDirsV1));

const combinedConformance = run([]);
const combinedConformanceDoc = parseJson(combinedConformance.stdout);
check("conformance:combined-exit", combinedConformance.code === 0 && combinedConformance.stderr === "", combinedConformance);
check(
  "conformance:combined-counts",
  combinedConformanceDoc?.fixtures?.valid === 1
    && combinedConformanceDoc?.fixtures?.invalid === 12
    && combinedConformanceDoc?.versionedFixtures?.["1.0.0"]?.valid === 11
    && combinedConformanceDoc?.versionedFixtures?.["1.0.0"]?.invalid === 36,
  combinedConformanceDoc
);

for (const name of fixtureDirsV1) {
  v1FixtureCases += 1;
  const selector = `tests/fixtures/model-v1/${name}`;
  const result = run(["--project", selector]);
  const repeated = run(["--project", selector]);
  const independent = await validateFixtureSchema(join(fixturesV1Dir, name), schemaV1);
  check(
    `fixture-v1:${name}:deterministic`,
    result.code === repeated.code && result.stdout === repeated.stdout && result.stderr === repeated.stderr,
    { result, repeated }
  );
  if (name.startsWith("valid-")) {
    const report = parseJson(result.stdout);
    check(`fixture-v1:${name}:exit`, result.code === 0 && result.stderr === "", result);
    check(`fixture-v1:${name}:schema`, independent.errors.length === 0, independent.errors);
    check(`fixture-v1:${name}:version`, report?.schemaVersion === "1.0.0", report);
    const byteSorted = [...(report?.symbols ?? [])].sort((left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")));
    check(`fixture-v1:${name}:canonical-order`, canonical(report?.symbols) === canonical(byteSorted), report?.symbols);
    if (name === "valid-module-move") {
      check(`fixture-v1:${name}:semantic-module`, canonical(report?.modules) === canonical(["planner"]), report?.modules);
    }
    if (name === "valid-target-canonical-key") {
      check(
        `fixture-v1:${name}:verbatim-target-key`,
        report?.symbols?.includes("planner.target_binding.node"),
        report?.symbols
      );
    }
    continue;
  }

  const expectation = parseJson(await readFile(join(fixturesV1Dir, name, "expect.json"), "utf8"));
  const report = parseJson(result.stderr);
  check(`fixture-v1:${name}:exit`, result.code === 1 && result.stdout === "", result);
  check(
    `fixture-v1:${name}:reason`,
    report?.reasonCodes?.[0] === expectation?.reasonCodes?.[0],
    { expected: expectation?.reasonCodes?.[0], actual: report?.reasonCodes?.[0], result }
  );
  if (V1_SCHEMA_INVALID_FIXTURES.has(name)) {
    check(`fixture-v1:${name}:schema-invalid`, independent.errors.length > 0, independent);
  } else {
    check(`fixture-v1:${name}:schema-valid-semantic-invalid`, independent.errors.length === 0, independent.errors);
  }
  if (/(?:alias|rename|registry|tombstone)/.test(name)) registryCases += 1;
}

const idTokenCases = [
  "scalar", "enum", "value_object", "entity", "command", "query", "policy",
  "event", "effect", "endpoint", "scenario", "target_binding"
];
for (const semanticId of ["planner.focus_task", ...idTokenCases.map((token) => `planner.${token}.item`)]) {
  semanticIdCases += 1;
  const result = run(["--check-id", semanticId]);
  const report = parseJson(result.stdout);
  check(`check-id:valid:${semanticId}:exit`, result.code === 0 && result.stderr === "", result);
  check(
    `check-id:valid:${semanticId}:verbatim`,
    report?.semanticId === semanticId && report?.canonicalKey === semanticId,
    report
  );
}
const longestContextualId = `${"a".repeat(63)}.target_binding.${"z".repeat(63)}`;
semanticIdCases += 1;
check("check-id:valid:max-contextual", run(["--check-id", longestContextualId]).code === 0, longestContextualId);

// Two distinct length layers: the machine contract's 191 figure is the
// schema-level arithmetic ceiling, while the closed kind namespace imposes
// the stricter accepted contextual maximum of 142 bytes and code points for
// kind-namespaced IDs (127 for plain two-segment IDs). The checker enforces
// segment grammar and the closed token set and has no total-length branch.
const longestPlainId = `${"a".repeat(63)}.${"z".repeat(63)}`;
semanticIdCases += 1;
check("check-id:valid:max-plain", [...longestPlainId].length === 127 && run(["--check-id", longestPlainId]).code === 0, longestPlainId);
const schemaCeilingId = `${"a".repeat(63)}.${"b".repeat(63)}.${"c".repeat(63)}`;
semanticIdCases += 1;
const ceilingResult = run(["--check-id", schemaCeilingId]);
check(
  "check-id:ceiling:schema-max-fails-kind-namespace",
  [...schemaCeilingId].length === 191 && ceilingResult.code === 1 && parseJson(ceilingResult.stderr)?.reasonCodes?.[0] === "semantic-id.kind-namespace",
  ceilingResult.stderr
);
const overContextualId = `${"a".repeat(63)}.target_binding.${"z".repeat(64)}`;
semanticIdCases += 1;
const overContextualResult = run(["--check-id", overContextualId]);
check(
  "check-id:ceiling:contextual-overflow-fails-grammar",
  [...overContextualId].length === 143 && overContextualResult.code === 1 && parseJson(overContextualResult.stderr)?.reasonCodes?.[0] === "semantic-id.grammar",
  overContextualResult.stderr
);
for (const [name, semanticId, expectedReason] of [
  ["uppercase", "Planner.focus", "semantic-id.grammar"],
  ["hyphen", "planner.focus-task", "semantic-id.grammar"],
  ["unicode", "planner.focús", "semantic-id.grammar"],
  ["whitespace", "planner.focus task", "semantic-id.grammar"],
  ["slash", "planner/focus", "semantic-id.grammar"],
  ["colon", "planner:focus", "semantic-id.grammar"],
  ["empty-segment", "planner..focus", "semantic-id.grammar"],
  ["digit-start", "planner.1focus", "semantic-id.grammar"],
  ["one-segment", "planner", "semantic-id.id-class"],
  ["four-segments", "planner.event.focus.changed", "semantic-id.id-class"],
  ["segment-64", `planner.${"a".repeat(64)}`, "semantic-id.grammar"],
  ["unknown-kind-token", "planner.unknown.focus", "semantic-id.kind-namespace"]
]) {
  semanticIdCases += 1;
  const result = run(["--check-id", semanticId]);
  check(`check-id:invalid:${name}:exit`, result.code === 1 && result.stdout === "", result);
  check(`check-id:invalid:${name}:reason`, parseJson(result.stderr)?.reasonCodes?.[0] === expectedReason, result.stderr);
}
for (const forbiddenExample of semanticIds.segment.forbiddenExamples) {
  semanticIdCases += 1;
  const result = run(["--check-id", forbiddenExample]);
  check(
    `check-id:contract-forbidden:${JSON.stringify(forbiddenExample)}`,
    result.code === 1 && parseJson(result.stderr)?.reasonCodes?.[0] === "semantic-id.grammar",
    result
  );
}
for (const [name, args] of [
  ["missing-value", ["--check-id"]],
  ["duplicate", ["--check-id", "planner.focus", "--check-id", "planner.focus"]],
  ["mixed-action", ["--check-id", "planner.focus", "--project", "tests/fixtures/model-v1/valid-minimal"]]
]) {
  const result = run(args);
  check(`check-id:usage:${name}`, result.code === 1 && parseJson(result.stderr)?.reasonCodes?.[0] === "model.usage", result);
  check(`check-id:usage:${name}:no-echo`, result.stderr === MODEL_USAGE_ENVELOPE, { expected: MODEL_USAGE_ENVELOPE, actual: result.stderr });
}

const compatibilityExpectation = parseJson(await readFile(join(compatibilityDir, "expect.json"), "utf8"));
const compatibilitySidePath = (version) => join(compatibilityDir, MODEL_COMPAT_GOLDEN_PAIR.sides.find((side) => side.version === version)?.directory ?? "MISSING-GOLDEN-SIDE");
const compatibilityV0 = compatibilitySidePath("0.1.0");
const compatibilityV1 = compatibilitySidePath("1.0.0");
const compatibilityV0Result = run(["--project", "tests/fixtures/model-compat/0.1.0"]);
const compatibilityV1Result = run(["--project", "tests/fixtures/model-compat/1.0.0"]);
compatibilityCases += 2;
check("compatibility:v0-valid", compatibilityV0Result.code === 0 && compatibilityV0Result.stderr === "", compatibilityV0Result);
check("compatibility:v1-valid", compatibilityV1Result.code === 0 && compatibilityV1Result.stderr === "", compatibilityV1Result);
const compatibilityV0Paths = (await modelDocuments(compatibilityV0)).map((path) => relative(compatibilityV0, path).replaceAll("\\", "/"));
const compatibilityV1Paths = (await modelDocuments(compatibilityV1)).map((path) => relative(compatibilityV1, path).replaceAll("\\", "/"));
check("compatibility:paths", canonical(compatibilityV0Paths) === canonical(compatibilityV1Paths), { compatibilityV0Paths, compatibilityV1Paths });
let compatibilityChangedDocuments = 0;
for (const relativePath of compatibilityV0Paths) {
  const left = parseJson(await readFile(join(compatibilityV0, relativePath), "utf8"));
  const right = parseJson(await readFile(join(compatibilityV1, relativePath), "utf8"));
  if (left?.schema_version !== right?.schema_version) compatibilityChangedDocuments += 1;
  left.schema_version = "VERSION";
  right.schema_version = "VERSION";
  check(`compatibility:schema-version-only:${relativePath}`, canonical(left) === canonical(right), { left, right });
}
check(
  "compatibility:golden-count",
  compatibilityExpectation?.automaticChange === "schema_version-only"
    && compatibilityChangedDocuments === compatibilityExpectation?.changedDocumentCount,
  { compatibilityExpectation, compatibilityChangedDocuments }
);

const mutations = [
  ["unknown-definition-field", "entities.yaml", (doc) => { doc.definitions[0].unexpected = true; }, "model.field-unknown", true],
  ["missing-id", "entities.yaml", (doc) => { delete doc.definitions[0].id; }, "model.field-missing", true],
  ["missing-kind", "entities.yaml", (doc) => { delete doc.definitions[0].kind; }, "model.field-missing", true],
  ["missing-version", "entities.yaml", (doc) => { delete doc.definitions[0].version; }, "model.field-missing", true],
  ["hostile-kind-constructor", "entities.yaml", (doc) => { doc.definitions[0].kind = "constructor"; }, "model.constraint", true],
  ["hostile-kind-toString", "entities.yaml", (doc) => { doc.definitions[0].kind = "toString"; }, "model.constraint", true],
  ["hostile-kind-__proto__", "entities.yaml", (doc) => { doc.definitions[0].kind = "__proto__"; }, "model.constraint", true],
  ["invalid-base", "entities.yaml", (doc) => { doc.definitions[0].base = "bytes"; }, "model.constraint", true],
  ["type-depth-five", "entities.yaml", (doc) => { doc.definitions[5].fields[0].type = { optional: { list: { optional: { list: { ref: "planner.due_date" } } } } }; }, "model.type-expression", true],
  ["duplicate-field", "entities.yaml", (doc) => { doc.definitions[5].fields.push(structuredClone(doc.definitions[5].fields[0])); }, "model.constraint", false],
  ["effect-emits-shape", "commands.yaml", (doc) => { doc.definitions[0].emits = "planner.task_created"; }, "model.constraint", true],
  ["effect-emits-duplicate", "commands.yaml", (doc) => { doc.definitions[0].emits.push(doc.definitions[0].emits[0]); }, "model.constraint", true],
  ["project-empty", "project.yaml", (doc) => { doc.definitions = []; }, "model.shape", true],
  ["project-two-definitions", "project.yaml", (doc) => { doc.definitions.push(structuredClone(doc.definitions[0])); }, "model.shape", true],
  ["document-field", "queries.yaml", (doc) => { doc.extra = true; }, "model.field-unknown", true],
  ["target-required", "bindings.yaml", (doc) => { delete doc.definitions[1].target; }, "model.field-missing", true],
  ["endpoint-path", "bindings.yaml", (doc) => { doc.definitions[0].path = "tasks/focus"; }, "model.constraint", true],
  ["query-reads-empty", "queries.yaml", (doc) => { doc.definitions[0].reads = []; }, "model.constraint", true],
  ["type-two-wrappers", "queries.yaml", (doc) => { doc.definitions[0].returns = { ref: "planner.task", list: { ref: "planner.task" } }; }, "model.type-expression", true],
  ["type-recursion-self", "entities.yaml", (doc) => {
    const task = doc.definitions.find((definition) => definition.id === "planner.task");
    task.fields.find((field) => field.name === "due").type = { ref: "planner.task" };
  }, "model.type-recursion", false],
  ["type-recursion-mutual", "entities.yaml", (doc) => {
    const task = doc.definitions.find((definition) => definition.id === "planner.task");
    const dueWindow = doc.definitions.find((definition) => definition.id === "planner.due_window");
    task.fields.find((field) => field.name === "due").type = { ref: "planner.due_window" };
    dueWindow.fields.find((field) => field.name === "from").type = { ref: "planner.task" };
  }, "model.type-recursion", false],
  ["enum-value-duplicate-semantic", "entities.yaml", (doc) => {
    const enumeration = doc.definitions.find((definition) => definition.id === "planner.task_state");
    enumeration.values.push({ value: enumeration.values[0].value, description: "Duplicate semantic value" });
  }, "model.constraint", false],
  ["description-bmp-2001", "project.yaml", (doc) => { doc.definitions[0].description = "x".repeat(2001); }, "model.constraint", true],
  ["description-astral-2001", "project.yaml", (doc) => { doc.definitions[0].description = "😀".repeat(2001); }, "model.constraint", true]
];

const scratch = await mkdtemp(join(tmpdir(), "lekalo-model-v0.1-"));
try {
  for (let index = 0; index < mutations.length; index += 1) {
    const [name, fileName, mutate, reason, schemaMustReject] = mutations[index];
    const project = join(scratch, String(index));
    await cp(validPlanner, project, { recursive: true });
    const matches = (await modelDocuments(project)).filter((path) => basename(path) === fileName);
    check(`mutation:${name}:file`, matches.length === 1, matches);
    if (matches.length !== 1) {
      continue;
    }
    const document = JSON.parse(await readFile(matches[0], "utf8"));
    mutate(document);
    await writeFile(matches[0], `${JSON.stringify(document)}\n`, "utf8");
    const result = run(["--project", project]);
    mutationCases += 1;
    check(`mutation:${name}:exit`, result.code === 1 && result.stdout === "", result);
    check(`mutation:${name}:reason`, parseJson(result.stderr)?.reasonCodes?.[0] === reason, { expected: reason, actual: result.stderr });
    if (name.startsWith("hostile-kind-")) {
      hostileKindCases += 1;
      const reasonCodes = ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "kind:string"];
      expectFailureEnvelope(`mutation:${name}:envelope`, result, "invalid", reasonCodes, [root, project]);
      const repeated = run(["--project", project]);
      check(
        `mutation:${name}:deterministic`,
        repeated.code === result.code && repeated.stdout === result.stdout && repeated.stderr === result.stderr,
        { result, repeated }
      );
    }
    const independent = await validateFixtureSchema(project, schema);
    if (schemaMustReject) {
      check(`mutation:${name}:schema`, independent.errors.length > 0, independent.errors);
    }
  }

  for (const [name, value] of [
    ["description-bmp-2000", "x".repeat(2000)],
    ["description-astral-2000", "😀".repeat(2000)]
  ]) {
    const project = join(scratch, name);
    await cp(validPlanner, project, { recursive: true });
    const projectFile = join(project, "lekalo", "project.yaml");
    const document = JSON.parse(await readFile(projectFile, "utf8"));
    document.definitions[0].description = value;
    await writeFile(projectFile, `${JSON.stringify(document)}\n`, "utf8");
    const result = run(["--project", project]);
    const independent = await validateFixtureSchema(project, schema);
    unicodeCases += 1;
    check(`unicode:${name}:exit`, result.code === 0 && parseJson(result.stdout)?.status === "valid", result);
    check(`unicode:${name}:schema`, independent.errors.length === 0, independent.errors);
  }
  unicodeCases += 2; // the paired 2001-character mutation cases above

  const missingOptional = join(scratch, "missing-optional");
  await cp(validPlanner, missingOptional, { recursive: true });
  await rm(join(missingOptional, "lekalo", "modules", "planner", "queries.yaml"));
  const missingOptionalResult = run(["--project", missingOptional]);
  readFailureCases += 1;
  check("read:missing-optional:valid", missingOptionalResult.code === 0 && parseJson(missingOptionalResult.stdout)?.status === "valid", missingOptionalResult);

  const missingRequired = join(scratch, "missing-required");
  await cp(validPlanner, missingRequired, { recursive: true });
  await rm(join(missingRequired, "lekalo", "modules", "planner", "module.yaml"));
  const missingRequiredResult = run(["--project", missingRequired]);
  readFailureCases += 1;
  expectFailureEnvelope(
    "read:missing-required",
    missingRequiredResult,
    "invalid",
    ["model.structure-invalid", "structure.document-missing", "lekalo/modules/planner/module.yaml"],
    [root, missingRequired]
  );

  const presentDirectory = join(scratch, "present-directory");
  await cp(validPlanner, presentDirectory, { recursive: true });
  const directoryInsteadOfFile = join(presentDirectory, "lekalo", "modules", "planner", "queries.yaml");
  await rm(directoryInsteadOfFile);
  await mkdir(directoryInsteadOfFile);
  const presentDirectoryResult = run(["--project", presentDirectory]);
  readFailureCases += 1;
  expectFailureEnvelope(
    "read:present-directory",
    presentDirectoryResult,
    "denied",
    ["model.structure-denied", "structure.module-subdirectory", "lekalo/modules/planner/queries.yaml"],
    [root, presentDirectory]
  );

  if (process.platform === "win32") {
    const lockedProject = join(scratch, "locked-existing-file");
    await cp(validPlanner, lockedProject, { recursive: true });
    const lockedFile = join(lockedProject, "lekalo", "modules", "planner", "queries.yaml");
    const lockedResult = await withWindowsExclusiveLock(lockedFile, () => run(["--project", lockedProject]));
    readFailureCases += 1;
    check("read:locked-existing-file:exit", lockedResult.code === 1 && lockedResult.stdout === "", lockedResult);
    check("read:locked-existing-file:reason", parseJson(lockedResult.stderr)?.reasonCodes?.[0] === "model.scan-failed", lockedResult.stderr);
  }

  try {
    const linkType = process.platform === "win32" ? "junction" : "dir";
    async function expectPhysicalDenial(name, project, structureReason, logicalPath) {
      const reasonCodes = ["model.structure-denied", structureReason, logicalPath];
      const first = run(["--project", project]);
      const second = run(["--project", project]);
      physicalContainmentCases += 1;
      expectFailureEnvelope(`physical:${name}`, first, "denied", reasonCodes, [root, project]);
      check(
        `physical:${name}:deterministic`,
        first.code === second.code && first.stdout === second.stdout && first.stderr === second.stderr,
        { first, second }
      );
    }

    const outsideRoot = join(scratch, "outside-root");
    const rootLinkProject = join(scratch, "root-link");
    await cp(validPlanner, outsideRoot, { recursive: true });
    await mkdir(rootLinkProject);
    await symlink(join(outsideRoot, "lekalo"), join(rootLinkProject, "lekalo"), linkType);
    await expectPhysicalDenial("root-link", rootLinkProject, "structure.path-link", "lekalo");

    const moduleLinkProject = join(scratch, "module-link");
    const outsideModule = join(scratch, "outside-module");
    await cp(validPlanner, moduleLinkProject, { recursive: true });
    await cp(join(validPlanner, "lekalo", "modules", "planner"), outsideModule, { recursive: true });
    await rm(join(moduleLinkProject, "lekalo", "modules", "planner"), { recursive: true });
    await symlink(outsideModule, join(moduleLinkProject, "lekalo", "modules", "planner"), linkType);
    await expectPhysicalDenial("module-link", moduleLinkProject, "structure.path-link", "lekalo/modules/planner");

    const fileLinkProject = join(scratch, "file-link");
    const outsideFile = join(scratch, "outside-file");
    const linkedFile = join(fileLinkProject, "lekalo", "modules", "planner", "entities.yaml");
    await cp(validPlanner, fileLinkProject, { recursive: true });
    await mkdir(outsideFile);
    await rm(linkedFile);
    await symlink(outsideFile, linkedFile, linkType);
    await expectPhysicalDenial("file-link", fileLinkProject, "structure.path-link", "lekalo/modules/planner/entities.yaml");

    const targetLinkProject = join(scratch, "target-link");
    const outsideTarget = join(scratch, "outside-target");
    const linkedTarget = join(targetLinkProject, "lekalo", "targets", "node-typescript.yaml");
    await cp(validPlanner, targetLinkProject, { recursive: true });
    await mkdir(outsideTarget);
    await rm(linkedTarget);
    await symlink(outsideTarget, linkedTarget, linkType);
    await expectPhysicalDenial("target-link", targetLinkProject, "structure.path-link", "lekalo/targets/node-typescript.yaml");

    if (process.platform !== "win32") {
      const specialProject = join(scratch, "special-entry");
      const specialPath = join(specialProject, "lekalo", "modules", "planner", "special-entry");
      await cp(validPlanner, specialProject, { recursive: true });
      const server = createServer();
      server.listen(specialPath);
      await once(server, "listening");
      try {
        await expectPhysicalDenial("special-entry", specialProject, "structure.path-special", "lekalo/modules/planner/special-entry");
      } finally {
        await new Promise((resolveClose) => server.close(resolveClose));
      }
    }
  } catch (error) {
    failures.push({ case: "physical-containment-probes", detail: { name: error?.name, code: error?.code, message: error?.message } });
  }
} finally {
  await rm(scratch, { recursive: true, force: true });
}

const schemaMaximumSymbol = `${"a".repeat(63)}.${"b".repeat(63)}.${"c".repeat(63)}`;
const schemaOverlongSymbol = `${"a".repeat(63)}.${"b".repeat(64)}.${"c".repeat(63)}`;
schemaCases += 2;
check(
  "schema:v1-symbol-191-valid",
  [...schemaMaximumSymbol].length === 191 && schemaErrors(schemaMaximumSymbol, schemaV1.$defs.symbolId, schemaV1).length === 0,
  schemaErrors(schemaMaximumSymbol, schemaV1.$defs.symbolId, schemaV1)
);
check(
  "schema:v1-symbol-192-invalid",
  [...schemaOverlongSymbol].length === 192 && schemaErrors(schemaOverlongSymbol, schemaV1.$defs.symbolId, schemaV1).length > 0,
  schemaErrors(schemaOverlongSymbol, schemaV1.$defs.symbolId, schemaV1)
);

const scratchV1 = await mkdtemp(join(tmpdir(), "lekalo-model-v1-"));
try {
  for (const hostileKind of ["constructor", "toString", "__proto__"]) {
    const project = join(scratchV1, `hostile-${hostileKind}`);
    await cp(validPlannerV1, project, { recursive: true });
    const entitiesPath = join(project, "lekalo", "modules", "planner", "entities.yaml");
    const document = JSON.parse(await readFile(entitiesPath, "utf8"));
    document.definitions[0].kind = hostileKind;
    await writeFile(entitiesPath, `${JSON.stringify(document)}\n`, "utf8");
    const result = run(["--project", project]);
    const reasonCodes = ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "kind:string"];
    hostileKindCases += 1;
    mutationCases += 1;
    expectFailureEnvelope(`mutation-v1:hostile-${hostileKind}`, result, "invalid", reasonCodes, [root, project]);
  }

  const typeDepth = join(scratchV1, "type-depth");
  await cp(validPlannerV1, typeDepth, { recursive: true });
  const typeDepthPath = join(typeDepth, "lekalo", "modules", "planner", "entities.yaml");
  const typeDepthDocument = JSON.parse(await readFile(typeDepthPath, "utf8"));
  const typeDepthTask = typeDepthDocument.definitions.find((definition) => definition.id === "planner.task");
  typeDepthTask.fields[0].type = { optional: { list: { optional: { list: { ref: "planner.text" } } } } };
  await writeFile(typeDepthPath, `${JSON.stringify(typeDepthDocument)}\n`, "utf8");
  const typeDepthResult = run(["--project", typeDepth]);
  const typeDepthSchema = await validateFixtureSchema(typeDepth, schemaV1);
  mutationCases += 1;
  check("mutation-v1:type-depth:reason", typeDepthResult.code === 1 && parseJson(typeDepthResult.stderr)?.reasonCodes?.[0] === "model.type-expression", typeDepthResult);
  check("mutation-v1:type-depth:schema", typeDepthSchema.errors.length > 0, typeDepthSchema);

  for (const [name, length, shouldPass] of [
    ["description-astral-2000", 2000, true],
    ["description-astral-2001", 2001, false]
  ]) {
    const project = join(scratchV1, name);
    await cp(validPlannerV1, project, { recursive: true });
    const projectPath = join(project, "lekalo", "project.yaml");
    const document = JSON.parse(await readFile(projectPath, "utf8"));
    document.definitions[0].description = "\u{1F600}".repeat(length);
    await writeFile(projectPath, `${JSON.stringify(document)}\n`, "utf8");
    const result = run(["--project", project]);
    const independent = await validateFixtureSchema(project, schemaV1);
    unicodeCases += 1;
    mutationCases += shouldPass ? 0 : 1;
    if (shouldPass) {
      check(`unicode-v1:${name}:checker`, result.code === 0 && result.stderr === "", result);
      check(`unicode-v1:${name}:schema`, independent.errors.length === 0, independent.errors);
    } else {
      check(`unicode-v1:${name}:checker`, result.code === 1 && parseJson(result.stderr)?.reasonCodes?.[0] === "model.constraint", result);
      check(`unicode-v1:${name}:schema`, independent.errors.length > 0, independent.errors);
    }
  }

  const precedence = join(scratchV1, "reason-precedence");
  await cp(validPlannerV1, precedence, { recursive: true });
  const precedenceModulePath = join(precedence, "lekalo", "modules", "planner", "module.yaml");
  const precedenceModule = JSON.parse(await readFile(precedenceModulePath, "utf8"));
  precedenceModule.schema_version = "0.1.0";
  await writeFile(precedenceModulePath, `${JSON.stringify(precedenceModule)}\n`, "utf8");
  const precedenceEntitiesPath = join(precedence, "lekalo", "modules", "planner", "entities.yaml");
  const precedenceEntities = JSON.parse(await readFile(precedenceEntitiesPath, "utf8"));
  precedenceEntities.definitions.push(structuredClone(precedenceEntities.definitions[0]));
  await writeFile(precedenceEntitiesPath, `${JSON.stringify(precedenceEntities)}\n`, "utf8");
  const precedenceFirst = run(["--project", precedence]);
  const precedenceSecond = run(["--project", precedence]);
  mutationCases += 1;
  check(
    "precedence-v1:schema-before-semantic",
    precedenceFirst.code === 1
      && parseJson(precedenceFirst.stderr)?.reasonCodes?.[0] === "model.schema-version"
      && precedenceFirst.stderr === precedenceSecond.stderr,
    { precedenceFirst, precedenceSecond }
  );

  const reorderedRegistry = join(scratchV1, "registry-source-order");
  await cp(join(fixturesV1Dir, "invalid-alias-ambiguous"), reorderedRegistry, { recursive: true });
  const reorderedProjectPath = join(reorderedRegistry, "lekalo", "project.yaml");
  const reorderedProject = JSON.parse(await readFile(reorderedProjectPath, "utf8"));
  reorderedProject.definitions[0].id_registry.rename_history.reverse();
  await writeFile(reorderedProjectPath, `${JSON.stringify(reorderedProject)}\n`, "utf8");
  const orderedRegistryResult = run(["--project", "tests/fixtures/model-v1/invalid-alias-ambiguous"]);
  const reorderedRegistryResult = run(["--project", reorderedRegistry]);
  registryCases += 1;
  check(
    "precedence-v1:registry-source-order-independent",
    orderedRegistryResult.code === 1
      && orderedRegistryResult.stderr === reorderedRegistryResult.stderr
      && parseJson(reorderedRegistryResult.stderr)?.reasonCodes?.[0] === "semantic-id.alias-ambiguous",
    { orderedRegistryResult, reorderedRegistryResult }
  );

  try {
    const linkType = process.platform === "win32" ? "junction" : "dir";
    const outsideRoot = join(scratchV1, "outside-root");
    const linkedProject = join(scratchV1, "linked-root");
    await cp(validPlannerV1, outsideRoot, { recursive: true });
    await mkdir(linkedProject);
    await symlink(join(outsideRoot, "lekalo"), join(linkedProject, "lekalo"), linkType);
    const linkedResult = run(["--project", linkedProject]);
    physicalContainmentCases += 1;
    expectFailureEnvelope(
      "physical-v1:root-link-before-version",
      linkedResult,
      "denied",
      ["model.structure-denied", "structure.path-link", "lekalo"],
      [root, linkedProject]
    );
  } catch (error) {
    failures.push({ case: "physical-v1-root-link", detail: { name: error?.name, code: error?.code, message: error?.message } });
  }
} finally {
  await rm(scratchV1, { recursive: true, force: true });
}

// Duplicate keys must be rejected by the document parser before JSON object
// materialization. These are raw-source subprocess probes rather than parsed
// fixture mutations, and cover every model object boundary that can carry
// typed input or registry state.
const scratchDuplicateJson = await mkdtemp(join(tmpdir(), "lekalo-model-json-"));
try {
  const duplicateCases = [
    ["top-level-document", "valid-planner/lekalo/project.yaml", '"schema_version":"1.0.0","definitions"', '"schema_version":"1.0.0","schema_version":"1.0.0","definitions"', "lekalo/project.yaml"],
    ["definition-first-hostile-kind", "valid-planner/lekalo/modules/planner/entities.yaml", '"kind":"scalar","version"', '"kind":"constructor","kind":"scalar","version"', "lekalo/modules/planner/entities.yaml"],
    ["definition-last-hostile-kind", "valid-planner/lekalo/modules/planner/entities.yaml", '"kind":"scalar","version"', '"kind":"scalar","kind":"constructor","version"', "lekalo/modules/planner/entities.yaml"],
    ["escaped-duplicate-key", "valid-planner/lekalo/modules/planner/entities.yaml", '"kind":"scalar","version"', '"k\\u0069nd":"constructor","kind":"scalar","version"', "lekalo/modules/planner/entities.yaml"],
    ["nested-field-object", "valid-planner/lekalo/modules/planner/entities.yaml", '{"name":"from","type"', '{"name":"from","name":"from","type"', "lekalo/modules/planner/entities.yaml"],
    ["nested-type-object", "valid-planner/lekalo/modules/planner/entities.yaml", '{"ref":"planner.due_date"}', '{"ref":"planner.due_date","ref":"planner.due_date"}', "lekalo/modules/planner/entities.yaml"],
    ["nested-enum-object", "valid-planner/lekalo/modules/planner/entities.yaml", '{"value":"backlog","description"', '{"value":"backlog","value":"backlog","description"', "lekalo/modules/planner/entities.yaml"],
    ["registry-object", "valid-rename-chain/lekalo/project.yaml", '"rename_history": [', '"rename_history": [],"rename_history": [', "lekalo/project.yaml"],
    ["rename-entry-object", "valid-rename-chain/lekalo/project.yaml", '"from": "planner.legacy_text",', '"from": "planner.legacy_text","from": "planner.legacy_text",', "lekalo/project.yaml"],
    ["tombstone-entry-object", "valid-tombstones/lekalo/project.yaml", '"id": "planner.deleted_text",', '"id": "planner.deleted_text","id": "planner.deleted_text",', "lekalo/project.yaml"]
  ];
  for (const [name, relativePath, needle, replacement, logicalPath] of duplicateCases) {
    const project = join(scratchDuplicateJson, name);
    const sourceProject = relativePath.startsWith("valid-rename-chain")
      ? join(fixturesV1Dir, "valid-rename-chain")
      : relativePath.startsWith("valid-tombstones")
        ? join(fixturesV1Dir, "valid-tombstones")
        : validPlannerV1;
    await cp(sourceProject, project, { recursive: true });
    const filePath = join(project, ...relativePath.split("/").slice(1));
    const source = await readFile(filePath, "utf8");
    check(`duplicate-json:${name}:source`, source.includes(needle), { filePath, needle });
    await writeFile(filePath, source.replace(needle, replacement), "utf8");
    const result = run(["--project", project]);
    duplicateJsonCases += 1;
    expectFailureEnvelope(`duplicate-json:${name}`, result, "invalid", ["model.document-parse", logicalPath], [root, project]);
  }

  const whitespaceProject = join(scratchDuplicateJson, "whitespace-crlf");
  await cp(validPlannerV1, whitespaceProject, { recursive: true });
  const whitespacePath = join(whitespaceProject, "lekalo", "modules", "planner", "entities.yaml");
  const whitespaceSource = await readFile(whitespacePath, "utf8");
  const whitespaceNeedle = '"kind":"scalar","version"';
  check("duplicate-json:whitespace-crlf:source", whitespaceSource.includes(whitespaceNeedle), whitespaceSource.slice(0, 300));
  await writeFile(
    whitespacePath,
    whitespaceSource.replace(whitespaceNeedle, '"kind" : "scalar" ,\r\n\t"kind" : "scalar" ,\r\n\t"version"'),
    "utf8"
  );
  const whitespaceResult = run(["--project", whitespaceProject]);
  duplicateJsonCases += 1;
  expectFailureEnvelope(
    "duplicate-json:whitespace-crlf",
    whitespaceResult,
    "invalid",
    ["model.document-parse", "lekalo/modules/planner/entities.yaml"],
    [root, whitespaceProject]
  );

  // Repeated keys in separate objects and key-shaped text are valid controls.
  const falsePositiveProject = join(scratchDuplicateJson, "false-positive-controls");
  await cp(validPlannerV1, falsePositiveProject, { recursive: true });
  const falsePositivePath = join(falsePositiveProject, "lekalo", "project.yaml");
  const falsePositiveDocument = JSON.parse(await readFile(falsePositivePath, "utf8"));
  falsePositiveDocument.definitions[0].description = 'literal {"kind":"constructor","kind":"scalar"} in a string';
  await writeFile(falsePositivePath, `${JSON.stringify(falsePositiveDocument)}\n`, "utf8");
  const falsePositiveResult = run(["--project", falsePositiveProject]);
  duplicateJsonCases += 1;
  check(
    "duplicate-json:false-positive-controls",
    falsePositiveResult.code === 0 && falsePositiveResult.stderr === "" && parseJson(falsePositiveResult.stdout)?.status === "valid",
    falsePositiveResult
  );
} finally {
  await rm(scratchDuplicateJson, { recursive: true, force: true });
}

// Unknown property names are attacker-controlled input. The checker retains
// only the logical document location and a fixed token, never the key itself.
const scratchUnknownFields = await mkdtemp(join(tmpdir(), "lekalo-model-fields-"));
try {
  const hostileNames = [
    "/tmp/secret-source.txt",
    "/etc/passwd",
    "C:\\secret\\raw-source.txt",
    "\\\\server\\share\\secret.txt",
    "file:///etc/passwd",
    "..\\..\\secret-token",
    "a".repeat(10000),
    "control-\u0000-\u0001-\u001f",
    "секрет🚫-unicode",
    "SOURCE_PAYLOAD_CANARY_TOKEN"
  ];
  for (let index = 0; index < hostileNames.length; index += 1) {
    const name = `unknown-${index}`;
    const project = join(scratchUnknownFields, name);
    await cp(validPlannerV1, project, { recursive: true });
    const entitiesPath = join(project, "lekalo", "modules", "planner", "entities.yaml");
    const document = JSON.parse(await readFile(entitiesPath, "utf8"));
    Object.defineProperty(document.definitions[0], hostileNames[index], { value: "SECRET_SOURCE_PAYLOAD", enumerable: true });
    await writeFile(entitiesPath, `${JSON.stringify(document)}\n`, "utf8");
    const result = run(["--project", project]);
    const stream = `${result.stdout}${result.stderr}`;
    unknownFieldPrivacyCases += 1;
    check(`unknown-field:${name}:exit`, result.code === 1 && result.stdout === "", result);
    check(
      `unknown-field:${name}:safe-envelope`,
      result.stderr === failureEnvelope("invalid", ["model.field-unknown", "lekalo/modules/planner/entities.yaml[0]", "field:unknown"]),
      result.stderr
    );
    check(`unknown-field:${name}:no-key`, !stream.includes(hostileNames[index]), { hostileName: hostileNames[index], stream });
    check(`unknown-field:${name}:no-payload`, !stream.includes("SECRET_SOURCE_PAYLOAD"), stream);
    check(`unknown-field:${name}:no-path-or-stack`, !hasAbsolutePathLeak(stream, [root, project]) && !/(?:TypeError|ReferenceError|\n\s+at\s|check-model\.mjs:\d+)/.test(stream), stream);
  }
} finally {
  await rm(scratchUnknownFields, { recursive: true, force: true });
}

// Every model value that can reach a diagnostic before its grammar/enum check
// is exercised as a subprocess. These probes intentionally include path-like,
// control, Unicode, overlong and canary inputs; only fixed tokens or indexed
// logical locations may cross the checker boundary.
// Remaining interpolations in semantic diagnostics (validated id/ref/target
// tokens and dependency cycles) are safe by construction: their producers
// first pass checkV1SymbolId/checkV0DefinitionId, checkPattern, or enum checks
// before resolveReference, registry, and cycle diagnostics emit them.
const hostileDiagnosticInputs = [
  "/tmp/secret-source.txt",
  "/etc/passwd",
  "C:\\secret\\raw-source.txt",
  "\\\\server\\share\\secret.txt",
  "file:///etc/passwd",
  "..\\..\\secret-token",
  "a".repeat(10000),
  "control-\u0000-\u0001-\u001f",
  "unicode-\u041f\u0440\u0438\u0432\u0435\u0442-\u{1F6A8}",
  "SOURCE_PAYLOAD_CANARY_TOKEN"
];
const hostilePayload = "SECRET_SOURCE_PAYLOAD";
const scratchDiagnosticTaint = await mkdtemp(join(tmpdir(), "lekalo-model-taint-"));
const diagnosticForbiddenPaths = [root, process.cwd(), tmpdir(), scratchDiagnosticTaint];
try {
  // F-001 regressions: nested names/values must not influence the location
  // while unknown properties are being checked.
  const nestedUnknownCases = [
    [
      "nested-field-unknown-property",
      validPlannerV1,
      "entities.yaml",
      (document, hostile) => {
        const entity = document.definitions.find((definition) => definition.kind === "entity");
        entity.fields[0].name = hostile;
        Object.defineProperty(entity.fields[0], hostile, { value: hostilePayload, enumerable: true });
      },
      ["model.field-unknown", "lekalo/modules/planner/entities.yaml[6]/fields[0]", "field:unknown"]
    ],
    [
      "nested-enum-unknown-property",
      validPlannerV1,
      "entities.yaml",
      (document, hostile) => {
        const enumeration = document.definitions.find((definition) => definition.kind === "enum");
        enumeration.values[0].value = hostile;
        Object.defineProperty(enumeration.values[0], hostile, { value: hostilePayload, enumerable: true });
      },
      ["model.field-unknown", "lekalo/modules/planner/entities.yaml[4]/values[0]", "field:unknown"]
    ]
  ];
  for (const [name, sourceProject, fileName, mutate, reasonCodes] of nestedUnknownCases) {
    for (let index = 0; index < hostileDiagnosticInputs.length; index += 1) {
      const hostile = hostileDiagnosticInputs[index];
      const project = join(scratchDiagnosticTaint, `${name}-${index}`);
      await cp(sourceProject, project, { recursive: true });
      const filePath = join(project, "lekalo", "modules", "planner", fileName);
      const document = JSON.parse(await readFile(filePath, "utf8"));
      mutate(document, hostile);
      await writeFile(filePath, `${JSON.stringify(document)}\n`, "utf8");
      const result = run(["--project", project]);
      unknownFieldPrivacyCases += 1;
      const stream = `${result.stdout}${result.stderr}`;
      expectFailureEnvelope(`${name}:${index}`, result, "invalid", reasonCodes, diagnosticForbiddenPaths);
      check(`${name}:${index}:no-hostile-input`, hostileDiagnosticInputs.every((value) => !stream.includes(value)), stream);
      check(`${name}:${index}:no-payload-or-stack`, !stream.includes(hostilePayload) && !/(?:TypeError|ReferenceError|\n\s+at\s|check-model\.mjs:\d+)/.test(stream), stream);
      if (index === 0) {
        const repeated = run(["--project", project]);
        check(`${name}:${index}:deterministic`, repeated.code === result.code && repeated.stdout === result.stdout && repeated.stderr === result.stderr, { result, repeated });
      }
    }
  }

  // Complete reachable pre-validation taint surface. The expected vectors are
  // deliberately exact: the third token is a bounded type/fixed token, while
  // the logical location is fixed or indexed until validation has completed.
  const taintCases = [
    ["unsupported-schema-version", validPlannerV1, "project.yaml", (document, hostile) => { document.schema_version = hostile; }, ["model.schema-version", "lekalo/project.yaml", "value:string"]],
    ["unknown-definition-kind", validPlannerV1, "entities.yaml", (document, hostile) => { document.definitions[0].kind = hostile; }, ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "kind:string"]],
    ["unknown-type-wrapper", validPlannerV1, "entities.yaml", (document, hostile) => {
      const entity = document.definitions.find((definition) => definition.kind === "entity");
      entity.fields[0].type = { [hostile]: { ref: "planner.task_id" } };
    }, ["model.type-expression", "lekalo/modules/planner/entities.yaml[6]/fields:task_id", "wrapper:unknown"]],
    ["enum-visibility", validPlannerV1, "entities.yaml", (document, hostile) => { document.definitions[0].visibility = hostile; }, ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "value:string"]],
    ["enum-portability", validPlannerV1, "entities.yaml", (document, hostile) => { document.definitions[0].portability = hostile; }, ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "value:string"]],
    ["enum-scalar-base", validPlannerV1, "entities.yaml", (document, hostile) => { document.definitions[0].base = hostile; }, ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", "value:string"]],
    ["enum-tombstone-reason", join(fixturesV1Dir, "valid-tombstones"), "project.yaml", (document, hostile) => { document.definitions[0].id_registry.tombstones[0].reason = hostile; }, ["model.constraint", "lekalo/project.yaml[0]/id_registry/tombstones[0]", "value:string"]],
    ["enum-policy-decision", validPlannerV1, "policies.yaml", (document, hostile) => { document.definitions[0].decision = hostile; }, ["model.constraint", "lekalo/modules/planner/policies.yaml[0]", "value:string"]],
    ["enum-effect-operation", validPlannerV1, "commands.yaml", (document, hostile) => { document.definitions[0].operation = hostile; }, ["model.constraint", "lekalo/modules/planner/commands.yaml[0]", "value:string"]],
    ["enum-endpoint-method", validPlannerV1, "bindings.yaml", (document, hostile) => { document.definitions[0].method = hostile; }, ["model.constraint", "lekalo/modules/planner/bindings.yaml[0]", "value:string"]]
  ];
  for (const [name, sourceProject, fileName, mutate, reasonCodes] of taintCases) {
    for (let index = 0; index < hostileDiagnosticInputs.length; index += 1) {
      const hostile = hostileDiagnosticInputs[index];
      const project = join(scratchDiagnosticTaint, `${name}-${index}`);
      await cp(sourceProject, project, { recursive: true });
      const filePath = join(project, "lekalo", "project.yaml");
      const moduleFilePath = join(project, "lekalo", "modules", "planner", fileName);
      const targetPath = fileName === "project.yaml" ? filePath : moduleFilePath;
      const document = JSON.parse(await readFile(targetPath, "utf8"));
      mutate(document, hostile);
      await writeFile(targetPath, `${JSON.stringify(document)}\n`, "utf8");
      const result = run(["--project", project]);
      mutationCases += 1;
      const stream = `${result.stdout}${result.stderr}`;
      expectFailureEnvelope(`${name}:${index}`, result, "invalid", reasonCodes, diagnosticForbiddenPaths);
      check(`${name}:${index}:no-hostile-input`, hostileDiagnosticInputs.every((value) => !stream.includes(value)), stream);
      check(`${name}:${index}:no-payload-or-stack`, !stream.includes(hostilePayload) && !/(?:TypeError|ReferenceError|\n\s+at\s|check-model\.mjs:\d+)/.test(stream), stream);
      if (index === 0) {
        const repeated = run(["--project", project]);
        check(`${name}:${index}:deterministic`, repeated.code === result.code && repeated.stdout === result.stdout && repeated.stderr === result.stderr, { result, repeated });
      }
    }
  }

  // Valid bounded names/values and hostile text in a free-form description are
  // controls: they must not be mistaken for tainted diagnostic input.
  const falsePositiveTaint = join(scratchDiagnosticTaint, "false-positive-controls");
  await cp(validPlannerV1, falsePositiveTaint, { recursive: true });
  const controlEntitiesPath = join(falsePositiveTaint, "lekalo", "modules", "planner", "entities.yaml");
  const controlDocument = JSON.parse(await readFile(controlEntitiesPath, "utf8"));
  controlDocument.definitions.find((definition) => definition.kind === "value-object").fields[0].name = "source_file";
  controlDocument.definitions.find((definition) => definition.kind === "enum").values[0].value = "source_value";
  controlDocument.definitions.find((definition) => definition.kind === "entity").description = [
    hostileDiagnosticInputs[0],
    hostileDiagnosticInputs[2],
    hostileDiagnosticInputs[4],
    hostileDiagnosticInputs[8],
    hostileDiagnosticInputs[9]
  ].join("|");
  await writeFile(controlEntitiesPath, `${JSON.stringify(controlDocument)}\n`, "utf8");
  const controlResult = run(["--project", falsePositiveTaint]);
  mutationCases += 1;
  check("diagnostic-taint:false-positive-controls", controlResult.code === 0 && controlResult.stderr === "" && parseJson(controlResult.stdout)?.status === "valid", controlResult);
  check("diagnostic-taint:false-positive:no-hostile-output", hostileDiagnosticInputs.every((value) => !`${controlResult.stdout}${controlResult.stderr}`.includes(value)), controlResult);
} finally {
  await rm(scratchDiagnosticTaint, { recursive: true, force: true });
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`${JSON.stringify({
    ok: true,
    schemaVersion: "0.1.0",
    schemaVersions: ["0.1.0", "1.0.0"],
    kinds: ALL_KINDS.length,
    fixtureCases,
    subprocessCases,
    schemaCases,
    mutationCases,
    unicodeCases,
    readFailureCases,
    hostileKindCases,
    physicalContainmentCases,
    v1FixtureCases,
    semanticIdCases,
    registryCases,
    compatibilityCases,
    duplicateJsonCases,
    unknownFieldPrivacyCases,
    integrityProbeCases,
    fixtures: { valid: 1, invalid: 12 },
    versionedFixtures: { "1.0.0": { valid: 11, invalid: 36 } }
  }, null, 2)}\n`);
}
