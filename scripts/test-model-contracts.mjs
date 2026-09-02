#!/usr/bin/env node

// Independent conformance suite for Lekalo Model v0.1 (issue #5).
// It exercises the reference checker as a real subprocess and separately
// interprets every JSON Schema keyword used by the shipped contract.

import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const checker = join(root, "scripts", "check-model.mjs");
const fixturesDir = join(root, "tests", "fixtures", "model");
const validPlanner = join(fixturesDir, "valid-planner");
const schemaPath = join(root, "contracts", "model.schema.v0.1.0.json");

const ALL_KINDS = ["project", "module", "scalar", "enum", "value-object", "entity", "command", "query", "policy", "event", "effect", "endpoint", "scenario", "target-binding"];
const EXACT_FIXTURES = [
  "invalid-constraint",
  "invalid-document-parse",
  "invalid-duplicate-id",
  "invalid-field-unknown",
  "invalid-id-grammar",
  "invalid-identity-field-missing",
  "invalid-kind-not-allowed-in-file",
  "invalid-module-mismatch",
  "invalid-ref-kind-mismatch",
  "invalid-ref-unresolved",
  "invalid-schema-version",
  "invalid-target-unresolved",
  "valid-planner"
];
const SCHEMA_INVALID_FIXTURES = new Set([
  "invalid-constraint",
  "invalid-document-parse",
  "invalid-field-unknown",
  "invalid-id-grammar",
  "invalid-kind-not-allowed-in-file",
  "invalid-schema-version"
]);
const DOCUMENT_SCHEMA = {
  "project.yaml": "projectDocument",
  "module.yaml": "moduleDocument",
  "entities.yaml": "entitiesDocument",
  "commands.yaml": "commandsDocument",
  "queries.yaml": "queriesDocument",
  "policies.yaml": "policiesDocument",
  "events.yaml": "eventsDocument",
  "scenarios.yaml": "scenariosDocument",
  "bindings.yaml": "bindingsDocument"
};

const failures = [];
let fixtureCases = 0;
let subprocessCases = 0;
let schemaCases = 0;
let mutationCases = 0;
let unicodeCases = 0;
let readFailureCases = 0;
let hostileKindCases = 0;
let physicalContainmentCases = 0;

function run(args) {
  const result = spawnSync(process.execPath, [checker, ...args], { cwd: root, encoding: "utf8" });
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
}

const first = run(["--project", "tests/fixtures/model/valid-planner"]);
const second = run(["--project", "tests/fixtures/model/valid-planner"]);
check("determinism:project", first.code === second.code && first.stdout === second.stdout && first.stderr === second.stderr, { first, second });

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
      const kind = document.definitions[0].kind;
      const reasonCodes = ["model.constraint", "lekalo/modules/planner/entities.yaml[0]", `kind:${kind}`];
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

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, failures }, null, 2)}\n`);
  process.exitCode = 1;
} else {
  process.stdout.write(`${JSON.stringify({
    ok: true,
    schemaVersion: "0.1.0",
    kinds: ALL_KINDS.length,
    fixtureCases,
    subprocessCases,
    schemaCases,
    mutationCases,
    unicodeCases,
    readFailureCases,
    hostileKindCases,
    physicalContainmentCases,
    fixtures: { valid: 1, invalid: 12 }
  }, null, 2)}\n`);
}
