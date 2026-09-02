#!/usr/bin/env node

// Reference validator for Lekalo Model v0.1 (issue #5).
// Contract: contracts/model.schema.v0.1.0.json and docs/model.md.
// Exit protocol: 0 valid, 1 invalid/usage. There is no policy-denied class
// in Model v0.1; every model failure is a well-classified invalid verdict.
//
// This validator implements the closed shapes of the published schema
// (type/required/properties/additionalProperties/const/enum/pattern/
// uniqueItems/min/max) plus the semantic pass that JSON Schema deliberately
// does not own: project-wide id uniqueness, module qualification and
// cross-reference resolution with kind checking.
//
// Documents are validated as parsed JSON. Block-YAML parsing, imports and
// source locations are owned by the loader (#7); fixtures therefore use the
// JSON subset of YAML.

import { readFile, readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixturesDir = join(root, "tests", "fixtures", "model");

const SCHEMA_VERSION = "0.1.0";
const KINDS = ["project", "module", "scalar", "enum", "value-object", "entity", "command", "query", "policy", "event", "effect", "endpoint", "scenario", "target-binding"];
const FILE_KINDS = {
  "project.yaml": ["project"],
  "module.yaml": ["module"],
  "entities.yaml": ["scalar", "enum", "value-object", "entity"],
  "commands.yaml": ["command", "effect"],
  "queries.yaml": ["query"],
  "policies.yaml": ["policy"],
  "events.yaml": ["event"],
  "scenarios.yaml": ["scenario"],
  "bindings.yaml": ["endpoint", "target-binding"]
};
const COMMON_FIELDS = new Set(["id", "kind", "version", "description", "derived_from", "visibility", "portability"]);
const TYPE_REF_KINDS = new Set(["scalar", "enum", "value-object", "entity"]);
const DEFINITION_ID = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)?$/;
const MODULE_NAME = /^[a-z][a-z0-9_-]{0,62}$/;
const REQUIREMENT_ID = /^[A-Z0-9][A-Z0-9._-]{0,127}$/;
const FIELD_NAME = /^[a-z][a-z0-9_]{0,63}$/;
const ENDPOINT_PATH = /^(\/[a-z0-9:_{}-]+)+$/;
const SCALAR_BASES = ["string", "number", "boolean", "date", "datetime", "uuid", "uri"];
const EFFECT_OPERATIONS = ["create", "update", "delete"];
const ENDPOINT_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"];
const POLICY_DECISIONS = ["allow", "deny"];
const MAX_TYPE_DEPTH = 4;

function invalid(reasonCodes) {
  return { outcome: "invalid", reasonCodes };
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringLength(value) {
  return [...value].length;
}

function isNonEmptyString(value, max = 2000) {
  return typeof value === "string" && stringLength(value) >= 1 && stringLength(value) <= max;
}

function checkPattern(value, pattern, where, code = "model.constraint") {
  if (typeof value !== "string" || !pattern.test(value)) {
    return invalid([code, where]);
  }
  return null;
}

function checkDefinitionId(value, where) {
  if (typeof value !== "string" || value.length < 3 || value.length > 129 || !DEFINITION_ID.test(value)) {
    return invalid(["model.id-grammar", where]);
  }
  return null;
}

function checkClosedObject(definition, allowed, where) {
  if (!isObject(definition)) {
    return invalid(["model.shape", where]);
  }
  for (const key of Object.keys(definition)) {
    if (!allowed.has(key)) {
      return invalid(["model.field-unknown", where, `field:${key}`]);
    }
  }
  return null;
}

function checkEnum(value, allowed, where) {
  if (!allowed.includes(value)) {
    return invalid(["model.constraint", where, `value:${String(value)}`]);
  }
  return null;
}

function checkUniqueStrings(list, where) {
  const seen = new Set();
  for (const item of list) {
    if (seen.has(item)) {
      return invalid(["model.constraint", where, `duplicate:${item}`]);
    }
    seen.add(item);
  }
  return null;
}

function checkTypeExpression(node, depth, where) {
  if (depth > MAX_TYPE_DEPTH) {
    return invalid(["model.type-expression", where, "depth"]);
  }
  if (!isObject(node)) {
    return invalid(["model.type-expression", where]);
  }
  const keys = Object.keys(node);
  if (keys.length !== 1) {
    return invalid(["model.type-expression", where, "exactly-one-wrapper"]);
  }
  const key = keys[0];
  if (key === "ref") {
    return checkDefinitionId(node.ref, where);
  }
  if (key === "list" || key === "optional") {
    return checkTypeExpression(node[key], depth + 1, where);
  }
  return invalid(["model.type-expression", where, `wrapper:${key}`]);
}

function checkFieldList(fields, where) {
  if (!Array.isArray(fields) || fields.length < 1) {
    return invalid(["model.constraint", where, "fields"]);
  }
  const names = new Set();
  for (const field of fields) {
    const fieldWhere = `${where}/fields:${field?.name ?? "?"}`;
    const failure = checkClosedObject(field, new Set(["name", "type", "required", "description"]), fieldWhere)
      ?? checkPattern(field.name, FIELD_NAME, fieldWhere)
      ?? checkTypeExpression(field.type, 1, fieldWhere);
    if (failure) {
      return failure;
    }
    if (field.required !== undefined && field.required !== true) {
      return invalid(["model.constraint", fieldWhere, "required-must-be-true"]);
    }
    if (field.description !== undefined && !isNonEmptyString(field.description)) {
      return invalid(["model.constraint", fieldWhere, "description"]);
    }
    if (names.has(field.name)) {
      return invalid(["model.constraint", fieldWhere, `duplicate-field:${field.name}`]);
    }
    names.add(field.name);
  }
  return null;
}

function checkIdList(list, where, minItems = 1) {
  if (!Array.isArray(list) || list.length < minItems) {
    return invalid(["model.constraint", where, "list"]);
  }
  for (const item of list) {
    const failure = checkDefinitionId(item, where);
    if (failure) {
      return failure;
    }
  }
  return checkUniqueStrings(list, where);
}

function kindRequiredFields(kind) {
  const required = {
    project: [],
    module: [],
    scalar: ["base"],
    enum: ["values"],
    "value-object": ["fields"],
    entity: ["fields", "identity"],
    command: [],
    query: ["reads"],
    policy: ["applies_to", "decision"],
    event: [],
    effect: ["operation", "entity"],
    endpoint: ["invokes", "method", "path"],
    scenario: ["summary"],
    "target-binding": ["target"]
  };
  return required[kind] ?? [];
}

function checkDefinition(definition, where, allowedKinds) {
  if (!isObject(definition)) {
    return invalid(["model.shape", where]);
  }
  for (const key of ["id", "kind", "version"]) {
    if (definition[key] === undefined) {
      return invalid(["model.field-missing", where, `field:${key}`]);
    }
  }
  const failure = checkClosedObject(definition, new Set([...COMMON_FIELDS, ...kindSpecificFields(definition.kind)]), where);
  if (failure) {
    return failure;
  }
  if (!KINDS.includes(definition.kind)) {
    return invalid(["model.constraint", where, `kind:${String(definition.kind)}`]);
  }
  if (!allowedKinds.includes(definition.kind)) {
    return invalid(["model.kind-not-allowed-in-file", where, `kind:${definition.kind}`]);
  }
  for (const key of kindRequiredFields(definition.kind)) {
    if (definition[key] === undefined) {
      return invalid(["model.field-missing", where, `field:${key}`]);
    }
  }
  const idFailure = checkDefinitionId(definition.id, where);
  if (idFailure) {
    return idFailure;
  }
  if (!Number.isInteger(definition.version) || definition.version < 1) {
    return invalid(["model.constraint", where, "version"]);
  }
  if (definition.description !== undefined && !isNonEmptyString(definition.description)) {
    return invalid(["model.constraint", where, "description"]);
  }
  if (definition.derived_from !== undefined) {
    if (!Array.isArray(definition.derived_from) || definition.derived_from.some((item) => typeof item !== "string")) {
      return invalid(["model.constraint", where, "derived_from"]);
    }
    for (const item of definition.derived_from) {
      const itemFailure = checkPattern(item, REQUIREMENT_ID, where, "model.constraint");
      if (itemFailure) {
        return itemFailure;
      }
    }
    const duplicate = checkUniqueStrings(definition.derived_from, where);
    if (duplicate) {
      return duplicate;
    }
  }
  if (definition.visibility !== undefined) {
    const visibility = checkEnum(definition.visibility, ["module", "project"], where);
    if (visibility) {
      return visibility;
    }
  }
  if (definition.portability !== undefined) {
    const portability = checkEnum(definition.portability, ["portable", "target-specific"], where);
    if (portability) {
      return portability;
    }
  }
  return checkKindSpecificFields(definition, where);
}

function kindSpecificFields(kind) {
  const fields = {
    project: [],
    module: ["imports"],
    scalar: ["base"],
    enum: ["values"],
    "value-object": ["fields"],
    entity: ["fields", "identity"],
    command: ["input", "effects"],
    query: ["reads", "returns"],
    policy: ["applies_to", "decision"],
    event: ["payload"],
    effect: ["operation", "entity", "emits"],
    endpoint: ["invokes", "method", "path"],
    scenario: ["summary", "covers"],
    "target-binding": ["target"]
  };
  return fields[kind] ?? [];
}

function checkKindSpecificFields(definition, where) {
  switch (definition.kind) {
    case "project":
      return null;
    case "module": {
      if (definition.imports === undefined) {
        return null;
      }
      if (!Array.isArray(definition.imports)) {
        return invalid(["model.constraint", where, "imports"]);
      }
      for (const item of definition.imports) {
        const failure = checkPattern(item, MODULE_NAME, where, "model.constraint");
        if (failure) {
          return failure;
        }
      }
      return checkUniqueStrings(definition.imports, where);
    }
    case "scalar":
      return checkEnum(definition.base, SCALAR_BASES, where);
    case "enum": {
      const values = definition.values;
      if (!Array.isArray(values) || values.length < 1) {
        return invalid(["model.constraint", where, "values"]);
      }
      const seen = new Set();
      for (const entry of values) {
        const entryWhere = `${where}/values:${entry?.value ?? "?"}`;
        const failure = checkClosedObject(entry, new Set(["value", "description"]), entryWhere)
          ?? checkPattern(entry.value, FIELD_NAME, entryWhere);
        if (failure) {
          return failure;
        }
        if (entry.description !== undefined && !isNonEmptyString(entry.description)) {
          return invalid(["model.constraint", entryWhere, "description"]);
        }
        if (seen.has(entry.value)) {
          return invalid(["model.constraint", entryWhere, `duplicate:${entry.value}`]);
        }
        seen.add(entry.value);
      }
      return null;
    }
    case "value-object":
      return checkFieldList(definition.fields, where);
    case "entity": {
      const fields = checkFieldList(definition.fields, where);
      if (fields) {
        return fields;
      }
      const identity = definition.identity;
      if (!Array.isArray(identity) || identity.length < 1) {
        return invalid(["model.constraint", where, "identity"]);
      }
      for (const item of identity) {
        const failure = checkPattern(item, FIELD_NAME, where, "model.constraint");
        if (failure) {
          return failure;
        }
      }
      const duplicates = checkUniqueStrings(identity, where);
      if (duplicates) {
        return duplicates;
      }
      const fieldNames = new Set(definition.fields.map((field) => field.name));
      for (const item of identity) {
        if (!fieldNames.has(item)) {
          return invalid(["model.identity-field-missing", where, `field:${item}`]);
        }
      }
      return null;
    }
    case "command": {
      if (definition.input !== undefined) {
        const input = checkFieldList(definition.input, where);
        if (input) {
          return input;
        }
      }
      if (definition.effects !== undefined) {
        return checkIdList(definition.effects, where, 0);
      }
      return null;
    }
    case "query": {
      const reads = checkIdList(definition.reads, where);
      if (reads) {
        return reads;
      }
      if (definition.returns !== undefined) {
        return checkTypeExpression(definition.returns, 1, where);
      }
      return null;
    }
    case "policy": {
      const applies = checkIdList(definition.applies_to, where);
      if (applies) {
        return applies;
      }
      return checkEnum(definition.decision, POLICY_DECISIONS, where);
    }
    case "event":
      if (definition.payload === undefined) {
        return null;
      }
      return checkFieldList(definition.payload, where);
    case "effect": {
      const operation = checkEnum(definition.operation, EFFECT_OPERATIONS, where);
      if (operation) {
        return operation;
      }
      const entity = checkDefinitionId(definition.entity, where);
      if (entity) {
        return entity;
      }
      if (definition.emits !== undefined) {
        return checkIdList(definition.emits, where, 0);
      }
      return null;
    }
    case "endpoint": {
      const invokes = checkDefinitionId(definition.invokes, where);
      if (invokes) {
        return invokes;
      }
      const method = checkEnum(definition.method, ENDPOINT_METHODS, where);
      if (method) {
        return method;
      }
      if (typeof definition.path !== "string" || definition.path.length > 512 || !ENDPOINT_PATH.test(definition.path)) {
        return invalid(["model.constraint", where, "path"]);
      }
      return null;
    }
    case "scenario": {
      if (!isNonEmptyString(definition.summary)) {
        return invalid(["model.constraint", where, "summary"]);
      }
      if (definition.covers !== undefined) {
        return checkIdList(definition.covers, where, 0);
      }
      return null;
    }
    case "target-binding":
      return checkPattern(definition.target, MODULE_NAME, where, "model.constraint");
    default:
      return invalid(["model.constraint", where, `kind:${String(definition.kind)}`]);
  }
}

async function readModelDocument(path, where) {
  let text;
  try {
    text = await readFile(path, "utf8");
  } catch (error) {
    if (error?.code === "ENOENT") {
      return { missing: true };
    }
    return { error: invalid(["model.scan-failed", where]) };
  }
  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return { error: invalid(["model.document-parse", where]) };
  }
  return { document };
}

function checkDocumentShape(document, where, allowedKinds) {
  const failure = checkClosedObject(document, new Set(["schema_version", "definitions"]), where);
  if (failure) {
    return failure;
  }
  if (document.schema_version !== SCHEMA_VERSION) {
    return invalid(["model.schema-version", where, `value:${String(document.schema_version)}`]);
  }
  if (!Array.isArray(document.definitions) || document.definitions.length < 1) {
    return invalid(["model.shape", where, "definitions"]);
  }
  if ((where.endsWith("project.yaml") || where.endsWith("module.yaml")) && document.definitions.length !== 1) {
    return invalid(["model.shape", where, "single-definition"]);
  }
  for (let index = 0; index < document.definitions.length; index += 1) {
    const definitionFailure = checkDefinition(document.definitions[index], `${where}[${index}]`, allowedKinds);
    if (definitionFailure) {
      return definitionFailure;
    }
  }
  return null;
}

function resolveReference(ref, byId, expectedKinds, where) {
  const target = byId.get(ref);
  if (target === undefined) {
    return invalid(["model.ref-unresolved", where, `ref:${ref}`]);
  }
  if (!expectedKinds.has(target.definition.kind)) {
    return invalid(["model.ref-kind-mismatch", where, `ref:${ref}`, `kind:${target.definition.kind}`]);
  }
  return null;
}

function typeExpressionRefs(node, out) {
  if (node.ref !== undefined) {
    out.push(node.ref);
    return;
  }
  if (node.list !== undefined) {
    typeExpressionRefs(node.list, out);
    return;
  }
  if (node.optional !== undefined) {
    typeExpressionRefs(node.optional, out);
  }
}

function findTypeRecursion(graph) {
  const state = new Map();
  const stack = [];
  const positions = new Map();

  function visit(id) {
    state.set(id, "visiting");
    positions.set(id, stack.length);
    stack.push(id);

    for (const dependency of graph.get(id) ?? []) {
      if (state.get(dependency) === "visiting") {
        return [...stack.slice(positions.get(dependency)), dependency];
      }
      if (state.get(dependency) !== "visited") {
        const cycle = visit(dependency);
        if (cycle) {
          return cycle;
        }
      }
    }

    stack.pop();
    positions.delete(id);
    state.set(id, "visited");
    return null;
  }

  for (const id of [...graph.keys()].sort()) {
    if (state.has(id)) {
      continue;
    }
    const cycle = visit(id);
    if (cycle) {
      return cycle;
    }
  }
  return null;
}

function checkSemantics(project) {
  // Project and module identities live in their own namespaces. Every other
  // definition shares one project-wide symbol space and is qualified by its
  // containing module directory. Stable path-independent IDs are owned by #6.
  const byId = new Map();
  for (const { document, where, moduleName } of project.documents) {
    for (const definition of document.definitions) {
      if (definition.kind === "project") {
        continue;
      }
      if (definition.kind === "module") {
        if (definition.id !== moduleName) {
          return invalid(["model.module-mismatch", `${where}:${definition.id}`, `module:${moduleName}`]);
        }
        continue;
      }
      if (byId.has(definition.id)) {
        return invalid(["model.duplicate-id", where, `id:${definition.id}`]);
      }
      byId.set(definition.id, { definition, where });
    }
  }
  const targets = new Set(project.targets);
  const typeGraph = new Map();

  for (const { document, where, moduleName } of project.documents) {
    for (const definition of document.definitions) {
      const definitionWhere = `${where}:${definition.id}`;
      if (definition.kind === "project" || definition.kind === "module") {
        continue;
      }
      const segments = definition.id.split(".");
      if (segments.length !== 2 || segments[0] !== moduleName) {
        return invalid(["model.module-mismatch", definitionWhere, `module:${moduleName}`]);
      }
      const refs = [];
      switch (definition.kind) {
        case "scalar":
        case "enum":
          break;
        case "value-object":
          for (const field of definition.fields) {
            typeExpressionRefs(field.type, refs);
          }
          break;
        case "entity":
          for (const field of definition.fields) {
            typeExpressionRefs(field.type, refs);
          }
          break;
        case "command":
          if (definition.input !== undefined) {
            for (const field of definition.input) {
              typeExpressionRefs(field.type, refs);
            }
          }
          if (definition.effects !== undefined) {
            for (const effectId of definition.effects) {
              const failure = resolveReference(effectId, byId, new Set(["effect"]), definitionWhere);
              if (failure) {
                return failure;
              }
            }
          }
          break;
        case "query":
          for (const entityId of definition.reads) {
            const failure = resolveReference(entityId, byId, new Set(["entity"]), definitionWhere);
            if (failure) {
              return failure;
            }
          }
          if (definition.returns !== undefined) {
            typeExpressionRefs(definition.returns, refs);
          }
          break;
        case "policy":
          for (const commandId of definition.applies_to) {
            const failure = resolveReference(commandId, byId, new Set(["command"]), definitionWhere);
            if (failure) {
              return failure;
            }
          }
          break;
        case "event":
          if (definition.payload !== undefined) {
            for (const field of definition.payload) {
              typeExpressionRefs(field.type, refs);
            }
          }
          break;
        case "effect": {
          const failure = resolveReference(definition.entity, byId, new Set(["entity"]), definitionWhere);
          if (failure) {
            return failure;
          }
          if (definition.emits !== undefined) {
            for (const eventId of definition.emits) {
              const eventFailure = resolveReference(eventId, byId, new Set(["event"]), definitionWhere);
              if (eventFailure) {
                return eventFailure;
              }
            }
          }
          break;
        }
        case "endpoint": {
          const failure = resolveReference(definition.invokes, byId, new Set(["command", "query"]), definitionWhere);
          if (failure) {
            return failure;
          }
          break;
        }
        case "scenario":
          if (definition.covers !== undefined) {
            for (const coveredId of definition.covers) {
              if (!byId.has(coveredId)) {
                return invalid(["model.ref-unresolved", definitionWhere, `ref:${coveredId}`]);
              }
            }
          }
          break;
        case "target-binding":
          if (!targets.has(definition.target)) {
            return invalid(["model.target-unresolved", definitionWhere, `target:${definition.target}`]);
          }
          break;
        default:
          break;
      }
      for (const ref of refs) {
        const failure = resolveReference(ref, byId, TYPE_REF_KINDS, definitionWhere);
        if (failure) {
          return failure;
        }
      }
      if (definition.kind === "value-object" || definition.kind === "entity") {
        const dependencies = refs.filter((ref) => {
          const targetKind = byId.get(ref).definition.kind;
          return targetKind === "value-object" || targetKind === "entity";
        });
        typeGraph.set(definition.id, [...new Set(dependencies)].sort());
      }
    }
  }

  const recursion = findTypeRecursion(typeGraph);
  if (recursion) {
    return invalid(["model.type-recursion", `cycle:${recursion.join("->")}`]);
  }

  return null;
}

async function loadProject(projectRoot) {
  const lekaloDir = join(projectRoot, "lekalo");
  const documents = [];
  const targets = [];
  const projectWhere = "lekalo/project.yaml";
  const projectDoc = await readModelDocument(join(lekaloDir, "project.yaml"), projectWhere);
  if (projectDoc.missing || projectDoc.error) {
    return { error: projectDoc.error ?? invalid(["model.scan-failed", projectWhere]) };
  }
  documents.push({ document: projectDoc.document, where: projectWhere, moduleName: null });
  let moduleNames = [];
  try {
    moduleNames = (await readdir(join(lekaloDir, "modules"), { withFileTypes: true }))
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort();
  } catch (error) {
    if (error?.code !== "ENOENT") {
      return { error: invalid(["model.scan-failed", "lekalo/modules"]) };
    }
  }
  for (const moduleName of moduleNames) {
    const moduleDir = join(lekaloDir, "modules", moduleName);
    for (const [fileName, allowedKinds] of Object.entries(FILE_KINDS)) {
      if (fileName === "project.yaml") {
        continue;
      }
      const where = `lekalo/modules/${moduleName}/${fileName}`;
      const read = await readModelDocument(join(moduleDir, fileName), where);
      if (read.missing) {
        if (fileName === "module.yaml") {
          return { error: invalid(["model.scan-failed", where]) };
        }
        continue; // optional kind file
      }
      if (read.error) {
        return { error: read.error };
      }
      documents.push({ document: read.document, where, moduleName });
    }
  }
  try {
    const targetEntries = await readdir(join(lekaloDir, "targets"), { withFileTypes: true });
    for (const entry of targetEntries) {
      if (entry.isFile() && entry.name.endsWith(".yaml")) {
        targets.push(entry.name.slice(0, -5));
      }
    }
  } catch (error) {
    if (error?.code !== "ENOENT") {
      return { error: invalid(["model.scan-failed", "lekalo/targets"]) };
    }
    // targets/ is optional at the model layer when it is absent
  }
  targets.sort();
  return { documents, targets };
}

export async function validateModel(projectRoot) {
  const loaded = await loadProject(projectRoot);
  if (loaded.error) {
    return loaded.error;
  }
  for (const { document, where } of loaded.documents) {
    const fileName = where.split("/").pop();
    const allowedKinds = FILE_KINDS[fileName];
    const failure = checkDocumentShape(document, where, allowedKinds);
    if (failure) {
      return failure;
    }
  }
  const semantic = checkSemantics(loaded);
  if (semantic) {
    return semantic;
  }
  const kinds = {};
  for (const { document } of loaded.documents) {
    for (const definition of document.definitions) {
      kinds[definition.kind] = (kinds[definition.kind] ?? 0) + 1;
    }
  }
  const modules = loaded.documents
    .map((entry) => entry.moduleName)
    .filter((name) => name !== null)
    .filter((name, index, all) => all.indexOf(name) === index)
    .sort();
  return {
    outcome: "valid",
    report: {
      status: "valid",
      modules,
      targets: loaded.targets,
      kinds
    }
  };
}

async function runFixtureConformance() {
  const entries = await readdir(fixturesDir, { withFileTypes: true });
  const results = { valid: 0, invalid: 0 };
  const mismatches = [];
  for (const entry of entries.sort((a, b) => (a.name < b.name ? -1 : 1))) {
    if (!entry.isDirectory() || (!entry.name.startsWith("valid-") && !entry.name.startsWith("invalid-"))) {
      continue;
    }
    const fixtureDir = join(fixturesDir, entry.name);
    let expectation = { outcome: "valid" };
    if (entry.name.startsWith("invalid-")) {
      try {
        expectation = JSON.parse(await readFile(join(fixtureDir, "expect.json"), "utf8"));
      } catch {
        expectation = null;
      }
    }
    if (expectation === null || (expectation.outcome !== "valid" && typeof expectation.reasonCodes?.[0] !== "string")) {
      mismatches.push({ fixture: entry.name, expectedReason: "model.expect-missing", actualReason: null });
      continue;
    }
    const result = await validateModel(fixtureDir);
    if (result.outcome !== expectation.outcome) {
      mismatches.push({ fixture: entry.name, expected: expectation.outcome, actual: result.outcome, actualReason: result.reasonCodes?.[0] ?? null });
      continue;
    }
    if (expectation.outcome === "invalid" && result.reasonCodes[0] !== expectation.reasonCodes[0]) {
      mismatches.push({ fixture: entry.name, expectedReason: expectation.reasonCodes[0], actualReason: result.reasonCodes[0] });
      continue;
    }
    results[result.outcome === "valid" ? "valid" : "invalid"] += 1;
  }
  if (mismatches.length > 0) {
    return { outcome: "invalid", report: { status: "invalid", reasonCodes: ["model.fixture-mismatch"], mismatches } };
  }
  return { outcome: "valid", report: { status: "valid", fixtures: results } };
}

export async function main(argv) {
  const args = argv ?? process.argv.slice(2);
  let project = null;
  for (let i = 0; i < args.length; i += 1) {
    if (args[i] === "--project" && args[i + 1] !== undefined && !args[i + 1].startsWith("--") && project === null) {
      project = args[i + 1];
      i += 1;
    } else {
      process.stderr.write(JSON.stringify({ status: "invalid", reasonCodes: ["model.usage", args[i]] }, null, 2) + "\n");
      return 1;
    }
  }
  if (project !== null) {
    const result = await validateModel(resolve(project));
    if (result.outcome === "valid") {
      process.stdout.write(JSON.stringify(result.report, null, 2) + "\n");
      return 0;
    }
    process.stderr.write(JSON.stringify({ status: "invalid", reasonCodes: result.reasonCodes }, null, 2) + "\n");
    return 1;
  }
  const conformance = await runFixtureConformance();
  if (conformance.outcome === "valid") {
    process.stdout.write(JSON.stringify(conformance.report, null, 2) + "\n");
    return 0;
  }
  process.stderr.write(JSON.stringify(conformance.report, null, 2) + "\n");
  return 1;
}

const invokedDirectly = process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  process.exitCode = await main();
}
