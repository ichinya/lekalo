#!/usr/bin/env node

// Reference validator for the exact Lekalo Model 0.1.0 and 1.0.0 contracts.
// Contracts: contracts/model.schema.v0.1.0.json,
// contracts/model.schema.v1.0.0.json, and dev.lekalo.semantic-ids@0.1.0.
// Exit protocol: 0 valid, 1 invalid/usage, 3 when the accepted structure
// contract denies physical input. Model validation never downgrades that
// stronger structure verdict.
//
// This validator implements the closed shapes of the published schemas
// (type/required/properties/additionalProperties/const/enum/pattern/
// uniqueItems/min/max) plus the semantic pass that JSON Schema deliberately
// do not own: project-wide id uniqueness, module qualification, rename and
// tombstone registry invariants, and cross-reference resolution with kind
// checking. Historical IDs remain traceability only and are never aliases
// for live model references.
//
// Documents are validated as parsed JSON. Block-YAML parsing, imports and
// source locations are owned by the loader (#7); fixtures therefore use the
// JSON subset of YAML.

import { readFile, readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { validateProject } from "./check-structure.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixturesDir = join(root, "tests", "fixtures", "model");
const fixturesV1Dir = join(root, "tests", "fixtures", "model-v1");
const semanticIdsContract = JSON.parse(await readFile(join(root, "contracts", "semantic-ids.v0.1.0.json"), "utf8"));

const SCHEMA_V0 = "0.1.0";
const SCHEMA_V1 = semanticIdsContract.releaseBinding.modelSchemaSuccessor;
const SUPPORTED_SCHEMA_VERSIONS = new Set([SCHEMA_V0, SCHEMA_V1]);
const KINDS = new Set(["project", "module", "scalar", "enum", "value-object", "entity", "command", "query", "policy", "event", "effect", "endpoint", "scenario", "target-binding"]);
const SYMBOL_KINDS = new Set([...KINDS].filter((kind) => kind !== "project" && kind !== "module"));
const KIND_NAMESPACE_TOKENS = new Set(semanticIdsContract.kindNamespaces.tokens);
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
const SEGMENT_ID = new RegExp(semanticIdsContract.segment.pattern);
const RESERVED_PROJECT_MODULE_IDS = new Set(semanticIdsContract.idClasses.projectId.reserved);
const MODULE_NAME = /^[a-z][a-z0-9_-]{0,62}$/;
const REQUIREMENT_ID = /^[A-Z0-9][A-Z0-9._-]{0,127}$/;
const FIELD_NAME = /^[a-z][a-z0-9_]{0,63}$/;
const ENDPOINT_PATH = /^(\/[a-z0-9:_{}-]+)+$/;
const SCALAR_BASES = ["string", "number", "boolean", "date", "datetime", "uuid", "uri"];
const EFFECT_OPERATIONS = ["create", "update", "delete"];
const ENDPOINT_METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE"];
const POLICY_DECISIONS = ["allow", "deny"];
const MAX_TYPE_DEPTH = 4;
// Model documents are a deliberately bounded JSON subset of YAML. Keep the
// parser bounded before any document object is materialized, and reject
// duplicate keys using their decoded string values (not their source spelling).
const MAX_MODEL_DOCUMENT_BYTES = 4 * 1024 * 1024;
const MAX_MODEL_JSON_DEPTH = 128;
const MAX_MODEL_JSON_OBJECT_KEYS = 4096;
const MAX_MODEL_JSON_ARRAY_ITEMS = 4096;
const UTF8_BYTE_ORDER = (left, right) => Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8"));
const KIND_REQUIRED_FIELDS = new Map([
  ["project", []],
  ["module", []],
  ["scalar", ["base"]],
  ["enum", ["values"]],
  ["value-object", ["fields"]],
  ["entity", ["fields", "identity"]],
  ["command", []],
  ["query", ["reads"]],
  ["policy", ["applies_to", "decision"]],
  ["event", []],
  ["effect", ["operation", "entity"]],
  ["endpoint", ["invokes", "method", "path"]],
  ["scenario", ["summary"]],
  ["target-binding", ["target"]]
]);
const KIND_SPECIFIC_FIELDS = new Map([
  ["project", []],
  ["module", ["imports"]],
  ["scalar", ["base"]],
  ["enum", ["values"]],
  ["value-object", ["fields"]],
  ["entity", ["fields", "identity"]],
  ["command", ["input", "effects"]],
  ["query", ["reads", "returns"]],
  ["policy", ["applies_to", "decision"]],
  ["event", ["payload"]],
  ["effect", ["operation", "entity", "emits"]],
  ["endpoint", ["invokes", "method", "path"]],
  ["scenario", ["summary", "covers"]],
  ["target-binding", ["target"]]
]);

function invalid(reasonCodes) {
  return { outcome: "invalid", reasonCodes };
}

class ModelJsonParseError extends Error {}

// JSON.parse has no duplicate-key rejection mode: a reviver only observes the
// already materialized last-wins object. This small recursive-descent parser
// therefore performs object-key uniqueness at the JSON lexical boundary while
// retaining JSON's string, number, literal, and whitespace rules.
class ModelJsonParser {
  constructor(text) {
    this.text = text;
    this.index = 0;
  }

  fail() {
    throw new ModelJsonParseError();
  }

  skipWhitespace() {
    while (this.index < this.text.length && /[\u0009\u000a\u000d\u0020]/.test(this.text[this.index])) {
      this.index += 1;
    }
  }

  parse() {
    this.skipWhitespace();
    const value = this.parseValue(0);
    this.skipWhitespace();
    if (this.index !== this.text.length) {
      this.fail();
    }
    return value;
  }

  parseValue(depth) {
    if (depth > MAX_MODEL_JSON_DEPTH) {
      this.fail();
    }
    this.skipWhitespace();
    const token = this.text[this.index];
    if (token === "{") return this.parseObject(depth);
    if (token === "[") return this.parseArray(depth);
    if (token === '"') return this.parseString();
    if (token === "t") return this.parseLiteral("true", true);
    if (token === "f") return this.parseLiteral("false", false);
    if (token === "n") return this.parseLiteral("null", null);
    if (token === "-" || (token >= "0" && token <= "9")) return this.parseNumber();
    this.fail();
  }

  parseLiteral(source, value) {
    if (this.text.slice(this.index, this.index + source.length) !== source) {
      this.fail();
    }
    this.index += source.length;
    return value;
  }

  parseString() {
    this.index += 1;
    let value = "";
    while (this.index < this.text.length) {
      const character = this.text[this.index];
      if (character === '"') {
        this.index += 1;
        return value;
      }
      if (character < "\u0020") {
        this.fail();
      }
      if (character === "\\") {
        this.index += 1;
        if (this.index >= this.text.length) {
          this.fail();
        }
        const escape = this.text[this.index];
        const simpleEscapes = { '"': '"', "\\": "\\", "/": "/", b: "\b", f: "\f", n: "\n", r: "\r", t: "\t" };
        if (Object.hasOwn(simpleEscapes, escape)) {
          value += simpleEscapes[escape];
          this.index += 1;
          continue;
        }
        if (escape !== "u" || this.index + 4 >= this.text.length) {
          this.fail();
        }
        const hex = this.text.slice(this.index + 1, this.index + 5);
        if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
          this.fail();
        }
        value += String.fromCharCode(Number.parseInt(hex, 16));
        this.index += 5;
        continue;
      }
      value += character;
      this.index += 1;
    }
    this.fail();
  }

  parseNumber() {
    const match = this.text.slice(this.index).match(/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/);
    if (match === null) {
      this.fail();
    }
    this.index += match[0].length;
    return Number(match[0]);
  }

  parseObject(depth) {
    this.index += 1;
    const object = Object.create(null);
    const keys = new Set();
    this.skipWhitespace();
    if (this.text[this.index] === "}") {
      this.index += 1;
      return object;
    }
    while (true) {
      this.skipWhitespace();
      if (this.text[this.index] !== '"') {
        this.fail();
      }
      const key = this.parseString();
      if (keys.has(key)) {
        this.fail();
      }
      keys.add(key);
      if (keys.size > MAX_MODEL_JSON_OBJECT_KEYS) {
        this.fail();
      }
      this.skipWhitespace();
      if (this.text[this.index] !== ":") {
        this.fail();
      }
      this.index += 1;
      object[key] = this.parseValue(depth + 1);
      this.skipWhitespace();
      if (this.text[this.index] === "}") {
        this.index += 1;
        return object;
      }
      if (this.text[this.index] !== ",") {
        this.fail();
      }
      this.index += 1;
    }
  }

  parseArray(depth) {
    this.index += 1;
    const array = [];
    this.skipWhitespace();
    if (this.text[this.index] === "]") {
      this.index += 1;
      return array;
    }
    while (true) {
      if (array.length >= MAX_MODEL_JSON_ARRAY_ITEMS) {
        this.fail();
      }
      array.push(this.parseValue(depth + 1));
      this.skipWhitespace();
      if (this.text[this.index] === "]") {
        this.index += 1;
        return array;
      }
      if (this.text[this.index] !== ",") {
        this.fail();
      }
      this.index += 1;
    }
  }
}

function parseModelDocument(text) {
  if (Buffer.byteLength(text, "utf8") > MAX_MODEL_DOCUMENT_BYTES) {
    throw new ModelJsonParseError();
  }
  return new ModelJsonParser(text).parse();
}

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function stringLength(value) {
  return [...value].length;
}

function reasonValue(value) {
  // Diagnostic values are often emitted before their grammar is known. Keep
  // the classification bounded and never reflect attacker-controlled text.
  if (typeof value === "string") return "string";
  if (value === null) return "null";
  if (["number", "boolean"].includes(typeof value)) return String(value);
  if (Array.isArray(value)) return "array";
  if (typeof value === "object") return "object";
  return typeof value;
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

function checkV0DefinitionId(value, where) {
  if (typeof value !== "string" || value.length < 3 || value.length > 129 || !DEFINITION_ID.test(value)) {
    return invalid(["model.id-grammar", where]);
  }
  return null;
}

function splitV1Id(value, where) {
  if (typeof value !== "string") {
    return { failure: invalid(["semantic-id.grammar", where]) };
  }
  const segments = value.split(".");
  if (segments.some((segment) => !SEGMENT_ID.test(segment))) {
    return { failure: invalid(["semantic-id.grammar", where]) };
  }
  return { segments };
}

function checkV1ProjectOrModuleId(value, where) {
  const parsed = splitV1Id(value, where);
  if (parsed.failure) {
    return parsed.failure;
  }
  if (parsed.segments.length !== 1) {
    return invalid(["semantic-id.id-class", where, "single-segment-required"]);
  }
  if (RESERVED_PROJECT_MODULE_IDS.has(value)) {
    return invalid(["semantic-id.reserved", where, `id:${value}`]);
  }
  return null;
}

function checkV1SymbolId(value, where) {
  const parsed = splitV1Id(value, where);
  if (parsed.failure) {
    return parsed.failure;
  }
  if (parsed.segments.length < 2 || parsed.segments.length > 3) {
    return invalid(["semantic-id.id-class", where, "two-or-three-segments-required"]);
  }
  if (parsed.segments.length === 3 && !KIND_NAMESPACE_TOKENS.has(parsed.segments[1])) {
    return invalid(["semantic-id.kind-namespace", where, `value:${parsed.segments[1]}`]);
  }
  // No total-length branch exists here: the segment grammar bounds every
  // segment at 63 bytes and the closed kind namespace bounds the middle
  // token, so accepted kind-namespaced IDs can never exceed 142 bytes and
  // code points (127 for plain two-segment IDs). The contract figure of 191
  // is the schema-level arithmetic ceiling of three 63-byte segments plus
  // two dots and stays enforced by the JSON schema, not by this checker.
  return null;
}

function checkReferenceId(value, where, schemaVersion) {
  return schemaVersion === SCHEMA_V1
    ? checkV1SymbolId(value, where)
    : checkV0DefinitionId(value, where);
}

function checkClosedObject(definition, allowed, where) {
  if (!isObject(definition)) {
    return invalid(["model.shape", where]);
  }
  for (const key of Object.keys(definition)) {
    if (!allowed.has(key)) {
      return invalid(["model.field-unknown", where, "field:unknown"]);
    }
  }
  return null;
}

function checkEnum(value, allowed, where) {
  if (!allowed.includes(value)) {
    return invalid(["model.constraint", where, `value:${reasonValue(value)}`]);
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

function checkTypeExpression(node, depth, where, schemaVersion) {
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
    return checkReferenceId(node.ref, where, schemaVersion);
  }
  if (key === "list" || key === "optional") {
    return checkTypeExpression(node[key], depth + 1, where, schemaVersion);
  }
  return invalid(["model.type-expression", where, "wrapper:unknown"]);
}

function checkFieldList(fields, where, schemaVersion) {
  if (!Array.isArray(fields) || fields.length < 1) {
    return invalid(["model.constraint", where, "fields"]);
  }
  const names = new Set();
  for (let index = 0; index < fields.length; index += 1) {
    const field = fields[index];
    // The nested name is untrusted until the closed-shape and grammar checks
    // below pass. Use only an indexed logical location for those failures.
    const indexedFieldWhere = `${where}/fields[${index}]`;
    const shapeFailure = checkClosedObject(field, new Set(["name", "type", "required", "description"]), indexedFieldWhere);
    if (shapeFailure) {
      return shapeFailure;
    }
    const nameFailure = checkPattern(field.name, FIELD_NAME, indexedFieldWhere);
    if (nameFailure) {
      return nameFailure;
    }
    const fieldWhere = `${where}/fields:${field.name}`;
    const failure = checkTypeExpression(field.type, 1, fieldWhere, schemaVersion);
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

function checkIdList(list, where, schemaVersion, minItems = 1) {
  if (!Array.isArray(list) || list.length < minItems) {
    return invalid(["model.constraint", where, "list"]);
  }
  for (const item of list) {
    const failure = checkReferenceId(item, where, schemaVersion);
    if (failure) {
      return failure;
    }
  }
  return checkUniqueStrings(list, where);
}

function kindRequiredFields(kind) {
  return KIND_REQUIRED_FIELDS.get(kind) ?? [];
}

function checkDefinition(definition, where, allowedKinds, schemaVersion) {
  if (!isObject(definition)) {
    return invalid(["model.shape", where]);
  }
  for (const key of ["id", "kind", "version"]) {
    if (definition[key] === undefined) {
      return invalid(["model.field-missing", where, `field:${key}`]);
    }
  }
  if (!KINDS.has(definition.kind)) {
    return invalid(["model.constraint", where, `kind:${reasonValue(definition.kind)}`]);
  }
  const symbolHistoryFields = schemaVersion === SCHEMA_V1 && SYMBOL_KINDS.has(definition.kind) ? ["renamed_from"] : [];
  const failure = checkClosedObject(
    definition,
    new Set([...COMMON_FIELDS, ...symbolHistoryFields, ...kindSpecificFields(definition.kind, schemaVersion)]),
    where
  );
  if (failure) {
    return failure;
  }
  if (!allowedKinds.includes(definition.kind)) {
    return invalid(["model.kind-not-allowed-in-file", where, `kind:${definition.kind}`]);
  }
  for (const key of kindRequiredFields(definition.kind)) {
    if (definition[key] === undefined) {
      return invalid(["model.field-missing", where, `field:${key}`]);
    }
  }
  const idFailure = schemaVersion === SCHEMA_V1
    ? (SYMBOL_KINDS.has(definition.kind)
        ? checkV1SymbolId(definition.id, where)
        : checkV1ProjectOrModuleId(definition.id, where))
    : checkV0DefinitionId(definition.id, where);
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
  if (schemaVersion === SCHEMA_V1 && SYMBOL_KINDS.has(definition.kind) && definition.renamed_from !== undefined) {
    if (!Array.isArray(definition.renamed_from) || definition.renamed_from.length < 1) {
      return invalid(["model.constraint", where, "renamed_from"]);
    }
    for (const historicalId of definition.renamed_from) {
      const historicalFailure = checkV1SymbolId(historicalId, where);
      if (historicalFailure) {
        return historicalFailure;
      }
    }
    const duplicate = checkUniqueStrings(definition.renamed_from, where);
    if (duplicate) {
      return duplicate;
    }
  }
  return checkKindSpecificFields(definition, where, schemaVersion);
}

function kindSpecificFields(kind, schemaVersion) {
  const fields = KIND_SPECIFIC_FIELDS.get(kind) ?? [];
  return schemaVersion === SCHEMA_V1 && kind === "project" ? [...fields, "id_registry"] : fields;
}

function checkIdRegistry(registry, where) {
  const failure = checkClosedObject(registry, new Set(["rename_history", "tombstones"]), where);
  if (failure) {
    return failure;
  }
  const keys = Object.keys(registry);
  if (keys.length < 1) {
    return invalid(["model.constraint", where, "non-empty-registry"]);
  }

  for (const key of ["rename_history", "tombstones"]) {
    if (!Object.hasOwn(registry, key)) {
      continue;
    }
    const entries = registry[key];
    if (!Array.isArray(entries) || entries.length < 1) {
      return invalid(["model.constraint", where, key]);
    }
    for (let index = 0; index < entries.length; index += 1) {
      const entry = entries[index];
      const entryWhere = `${where}/${key}[${index}]`;
      if (key === "rename_history") {
        const entryFailure = checkClosedObject(entry, new Set(["from", "to", "definition_version", "same_identity", "note"]), entryWhere);
        if (entryFailure) {
          return entryFailure;
        }
        for (const required of ["from", "to", "definition_version"]) {
          if (entry[required] === undefined) {
            return invalid(["model.field-missing", entryWhere, `field:${required}`]);
          }
        }
        const idFailure = checkV1SymbolId(entry.from, entryWhere) ?? checkV1SymbolId(entry.to, entryWhere);
        if (idFailure) {
          return idFailure;
        }
        if (!Number.isInteger(entry.definition_version) || entry.definition_version < 1) {
          return invalid(["model.constraint", entryWhere, "definition_version"]);
        }
        if (entry.same_identity !== undefined && entry.same_identity !== true) {
          return invalid(["model.constraint", entryWhere, "same_identity"]);
        }
        if (entry.note !== undefined && !isNonEmptyString(entry.note)) {
          return invalid(["model.constraint", entryWhere, "note"]);
        }
        continue;
      }

      const entryFailure = checkClosedObject(entry, new Set(["id", "reason", "replaced_by", "since"]), entryWhere);
      if (entryFailure) {
        return entryFailure;
      }
      for (const required of ["id", "reason", "since"]) {
        if (entry[required] === undefined) {
          return invalid(["model.field-missing", entryWhere, `field:${required}`]);
        }
      }
      const idFailure = checkV1SymbolId(entry.id, entryWhere);
      if (idFailure) {
        return idFailure;
      }
      const reasonFailure = checkEnum(entry.reason, ["replaced", "deleted"], entryWhere);
      if (reasonFailure) {
        return reasonFailure;
      }
      if (!Number.isInteger(entry.since) || entry.since < 1) {
        return invalid(["model.constraint", entryWhere, "since"]);
      }
      if (entry.reason === "replaced") {
        if (entry.replaced_by === undefined) {
          return invalid(["model.field-missing", entryWhere, "field:replaced_by"]);
        }
        const targetFailure = checkV1SymbolId(entry.replaced_by, entryWhere);
        if (targetFailure) {
          return targetFailure;
        }
      } else if (entry.replaced_by !== undefined) {
        return invalid(["semantic-id.tombstone-replaced-by-forbidden", entryWhere, `id:${entry.id}`]);
      }
    }
  }
  return null;
}

function checkKindSpecificFields(definition, where, schemaVersion) {
  switch (definition.kind) {
    case "project":
      if (schemaVersion === SCHEMA_V1 && definition.id_registry !== undefined) {
        return checkIdRegistry(definition.id_registry, `${where}/id_registry`);
      }
      return null;
    case "module": {
      if (definition.imports === undefined) {
        return null;
      }
      if (!Array.isArray(definition.imports)) {
        return invalid(["model.constraint", where, "imports"]);
      }
      for (const item of definition.imports) {
        const failure = schemaVersion === SCHEMA_V1
          ? checkV1ProjectOrModuleId(item, where)
          : checkPattern(item, MODULE_NAME, where, "model.constraint");
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
      for (let index = 0; index < values.length; index += 1) {
        const entry = values[index];
        // As with fields, do not put the unvalidated enum value in a
        // diagnostic location. The indexed location is stable and bounded.
        const indexedEntryWhere = `${where}/values[${index}]`;
        const shapeFailure = checkClosedObject(entry, new Set(["value", "description"]), indexedEntryWhere);
        if (shapeFailure) {
          return shapeFailure;
        }
        const valueFailure = checkPattern(entry.value, FIELD_NAME, indexedEntryWhere);
        if (valueFailure) {
          return valueFailure;
        }
        const entryWhere = `${where}/values:${entry.value}`;
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
      return checkFieldList(definition.fields, where, schemaVersion);
    case "entity": {
      const fields = checkFieldList(definition.fields, where, schemaVersion);
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
        const input = checkFieldList(definition.input, where, schemaVersion);
        if (input) {
          return input;
        }
      }
      if (definition.effects !== undefined) {
        return checkIdList(definition.effects, where, schemaVersion, 0);
      }
      return null;
    }
    case "query": {
      const reads = checkIdList(definition.reads, where, schemaVersion);
      if (reads) {
        return reads;
      }
      if (definition.returns !== undefined) {
        return checkTypeExpression(definition.returns, 1, where, schemaVersion);
      }
      return null;
    }
    case "policy": {
      const applies = checkIdList(definition.applies_to, where, schemaVersion);
      if (applies) {
        return applies;
      }
      return checkEnum(definition.decision, POLICY_DECISIONS, where);
    }
    case "event":
      if (definition.payload === undefined) {
        return null;
      }
      return checkFieldList(definition.payload, where, schemaVersion);
    case "effect": {
      const operation = checkEnum(definition.operation, EFFECT_OPERATIONS, where);
      if (operation) {
        return operation;
      }
      const entity = checkReferenceId(definition.entity, where, schemaVersion);
      if (entity) {
        return entity;
      }
      if (definition.emits !== undefined) {
        return checkIdList(definition.emits, where, schemaVersion, 0);
      }
      return null;
    }
    case "endpoint": {
      const invokes = checkReferenceId(definition.invokes, where, schemaVersion);
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
        return checkIdList(definition.covers, where, schemaVersion, 0);
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
    document = parseModelDocument(text);
  } catch {
    return { error: invalid(["model.document-parse", where]) };
  }
  return { document };
}

function checkDocumentShape(document, where, allowedKinds, expectedVersion = null) {
  const failure = checkClosedObject(document, new Set(["schema_version", "definitions"]), where);
  if (failure) {
    return failure;
  }
  if (!SUPPORTED_SCHEMA_VERSIONS.has(document.schema_version)
      || (expectedVersion !== null && document.schema_version !== expectedVersion)) {
    return invalid(["model.schema-version", where, `value:${reasonValue(document.schema_version)}`]);
  }
  if (!Array.isArray(document.definitions) || document.definitions.length < 1) {
    return invalid(["model.shape", where, "definitions"]);
  }
  if ((where.endsWith("project.yaml") || where.endsWith("module.yaml")) && document.definitions.length !== 1) {
    return invalid(["model.shape", where, "single-definition"]);
  }
  for (let index = 0; index < document.definitions.length; index += 1) {
    const definitionFailure = checkDefinition(document.definitions[index], `${where}[${index}]`, allowedKinds, document.schema_version);
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

function checkSemanticsV0(project) {
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

function findRenameCycle(renameByFrom) {
  const state = new Map();
  const stack = [];
  const positions = new Map();

  function visit(id) {
    state.set(id, "visiting");
    positions.set(id, stack.length);
    stack.push(id);
    const next = renameByFrom.get(id)?.to;
    if (next !== undefined && renameByFrom.has(next)) {
      if (state.get(next) === "visiting") {
        return [...stack.slice(positions.get(next)), next];
      }
      if (state.get(next) !== "visited") {
        const cycle = visit(next);
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

  for (const id of [...renameByFrom.keys()].sort(UTF8_BYTE_ORDER)) {
    if (!state.has(id)) {
      const cycle = visit(id);
      if (cycle) {
        return cycle;
      }
    }
  }
  return null;
}

function checkV1Registry(projectDefinition, projectWhere, byId) {
  const registry = projectDefinition.id_registry;
  if (registry === undefined) {
    for (const [id, record] of [...byId.entries()].sort(([left], [right]) => UTF8_BYTE_ORDER(left, right))) {
      if ((record.definition.renamed_from ?? []).length > 0) {
        return invalid(["semantic-id.rename-metadata-mismatch", `${record.where}:${id}`, "registry-missing"]);
      }
    }
    return null;
  }

  const registryWhere = `${projectWhere}:${projectDefinition.id}/id_registry`;
  if (projectDefinition.version < 2) {
    return invalid(["semantic-id.registry-project-version", registryWhere, `version:${projectDefinition.version}`]);
  }

  const renameEntries = [...(registry.rename_history ?? [])].sort((left, right) =>
    UTF8_BYTE_ORDER(left.from, right.from)
      || UTF8_BYTE_ORDER(left.to, right.to)
      || left.definition_version - right.definition_version);
  const tombstones = [...(registry.tombstones ?? [])].sort((left, right) =>
    UTF8_BYTE_ORDER(left.id, right.id) || left.since - right.since);
  const renameByFrom = new Map();
  const incoming = new Map();
  for (const entry of renameEntries) {
    if (renameByFrom.has(entry.from)) {
      return invalid(["semantic-id.alias-ambiguous", registryWhere, `from:${entry.from}`]);
    }
    renameByFrom.set(entry.from, entry);
    const entries = incoming.get(entry.to) ?? [];
    entries.push(entry);
    incoming.set(entry.to, entries);
  }

  const tombstoneById = new Map();
  for (const entry of tombstones) {
    if (tombstoneById.has(entry.id)) {
      return invalid(["semantic-id.tombstone-duplicate", registryWhere, `id:${entry.id}`]);
    }
    tombstoneById.set(entry.id, entry);
  }

  const renameGraphIds = new Set();
  for (const entry of renameEntries) {
    renameGraphIds.add(entry.from);
    renameGraphIds.add(entry.to);
  }
  for (const id of [...tombstoneById.keys()].sort(UTF8_BYTE_ORDER)) {
    if (renameGraphIds.has(id)) {
      return invalid(["semantic-id.registry-overlap", registryWhere, `id:${id}`]);
    }
    if (byId.has(id)) {
      return invalid(["semantic-id.tombstone-reuse", registryWhere, `id:${id}`]);
    }
  }

  for (const entry of renameEntries) {
    if (byId.has(entry.from)) {
      return invalid(["semantic-id.rename-source-live", registryWhere, `from:${entry.from}`]);
    }
    if (!renameByFrom.has(entry.to) && !byId.has(entry.to)) {
      return invalid(["semantic-id.rename-target-missing", registryWhere, `to:${entry.to}`]);
    }
  }

  const cycle = findRenameCycle(renameByFrom);
  if (cycle) {
    return invalid(["semantic-id.rename-cycle", `cycle:${cycle.join("->")}`]);
  }

  for (const [target, entries] of [...incoming.entries()].sort(([left], [right]) => UTF8_BYTE_ORDER(left, right))) {
    if (entries.length > 1 && entries.some((entry) => entry.same_identity !== true)) {
      return invalid(["semantic-id.alias-convergence-unasserted", registryWhere, `to:${target}`]);
    }
  }

  for (const [id, record] of [...byId.entries()].sort(([left], [right]) => UTF8_BYTE_ORDER(left, right))) {
    const directSources = (incoming.get(id) ?? []).map((entry) => entry.from).sort(UTF8_BYTE_ORDER);
    const declaredSources = [...(record.definition.renamed_from ?? [])].sort(UTF8_BYTE_ORDER);
    if (directSources.length !== declaredSources.length
        || directSources.some((source, index) => source !== declaredSources[index])) {
      return invalid(["semantic-id.rename-metadata-mismatch", `${record.where}:${id}`, `expected:${directSources.join(",") || "none"}`]);
    }
  }

  for (const entry of renameEntries) {
    const next = renameByFrom.get(entry.to);
    if (next !== undefined && entry.definition_version >= next.definition_version) {
      return invalid(["semantic-id.rename-version", registryWhere, `from:${entry.from}`]);
    }
    let terminalId = entry.to;
    while (renameByFrom.has(terminalId)) {
      terminalId = renameByFrom.get(terminalId).to;
    }
    const terminal = byId.get(terminalId).definition;
    if (entry.definition_version > terminal.version) {
      return invalid(["semantic-id.rename-version", registryWhere, `from:${entry.from}`]);
    }
  }

  for (const tombstone of tombstones) {
    if (tombstone.since > projectDefinition.version) {
      return invalid(["semantic-id.tombstone-version", registryWhere, `id:${tombstone.id}`]);
    }
    if (tombstone.reason === "replaced" && !byId.has(tombstone.replaced_by)) {
      return invalid(["semantic-id.tombstone-target-missing", registryWhere, `id:${tombstone.id}`]);
    }
  }
  return null;
}

function checkV1References(project, byId) {
  const typeGraph = new Map();
  const targets = new Set(project.targets);

  for (const { document, where } of project.documents) {
    for (const definition of document.definitions) {
      if (!SYMBOL_KINDS.has(definition.kind)) {
        continue;
      }
      const definitionWhere = `${where}:${definition.id}`;
      const refs = [];
      switch (definition.kind) {
        case "scalar":
        case "enum":
          break;
        case "value-object":
        case "entity":
          for (const field of definition.fields) {
            typeExpressionRefs(field.type, refs);
          }
          break;
        case "command":
          for (const field of definition.input ?? []) {
            typeExpressionRefs(field.type, refs);
          }
          for (const effectId of definition.effects ?? []) {
            const failure = resolveReference(effectId, byId, new Set(["effect"]), definitionWhere);
            if (failure) return failure;
          }
          break;
        case "query":
          for (const entityId of definition.reads) {
            const failure = resolveReference(entityId, byId, new Set(["entity"]), definitionWhere);
            if (failure) return failure;
          }
          if (definition.returns !== undefined) typeExpressionRefs(definition.returns, refs);
          break;
        case "policy":
          for (const commandId of definition.applies_to) {
            const failure = resolveReference(commandId, byId, new Set(["command"]), definitionWhere);
            if (failure) return failure;
          }
          break;
        case "event":
          for (const field of definition.payload ?? []) {
            typeExpressionRefs(field.type, refs);
          }
          break;
        case "effect": {
          const entityFailure = resolveReference(definition.entity, byId, new Set(["entity"]), definitionWhere);
          if (entityFailure) return entityFailure;
          for (const eventId of definition.emits ?? []) {
            const eventFailure = resolveReference(eventId, byId, new Set(["event"]), definitionWhere);
            if (eventFailure) return eventFailure;
          }
          break;
        }
        case "endpoint": {
          const failure = resolveReference(definition.invokes, byId, new Set(["command", "query"]), definitionWhere);
          if (failure) return failure;
          break;
        }
        case "scenario":
          for (const coveredId of definition.covers ?? []) {
            if (!byId.has(coveredId)) {
              return invalid(["model.ref-unresolved", definitionWhere, `ref:${coveredId}`]);
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
        if (failure) return failure;
      }
      if (definition.kind === "value-object" || definition.kind === "entity") {
        typeGraph.set(definition.id, [...new Set(refs.filter((ref) => {
          const targetKind = byId.get(ref).definition.kind;
          return targetKind === "value-object" || targetKind === "entity";
        }))].sort(UTF8_BYTE_ORDER));
      }
    }
  }

  const recursion = findTypeRecursion(typeGraph);
  return recursion ? invalid(["model.type-recursion", `cycle:${recursion.join("->")}`]) : null;
}

function checkSemanticsV1(project) {
  const declaredModuleId = new Map();
  const moduleById = new Map();
  let projectDefinition = null;
  let projectWhere = "lekalo/project.yaml";

  for (const { document, where, moduleName } of project.documents) {
    for (const definition of document.definitions) {
      if (definition.kind === "project") {
        projectDefinition = definition;
        projectWhere = where;
      } else if (definition.kind === "module") {
        if (moduleById.has(definition.id)) {
          return invalid(["semantic-id.module-duplicate", where, `id:${definition.id}`]);
        }
        moduleById.set(definition.id, { definition, where, moduleName });
        declaredModuleId.set(moduleName, definition.id);
      }
    }
  }

  const byId = new Map();
  for (const { document, where, moduleName } of project.documents) {
    for (const definition of document.definitions) {
      if (!SYMBOL_KINDS.has(definition.kind)) {
        continue;
      }
      const definitionWhere = `${where}:${definition.id}`;
      if (byId.has(definition.id)) {
        return invalid(["model.duplicate-id", where, `id:${definition.id}`]);
      }
      const segments = definition.id.split(".");
      const declared = declaredModuleId.get(moduleName);
      if (declared === undefined || segments[0] !== declared) {
        return invalid(["model.module-mismatch", definitionWhere, `module:${declared ?? moduleName}`]);
      }
      if (segments.length === 3 && segments[1] !== definition.kind.replaceAll("-", "_")) {
        return invalid(["semantic-id.kind-namespace", definitionWhere, `expected:${definition.kind.replaceAll("-", "_")}`]);
      }
      byId.set(definition.id, { definition, where });
    }
  }

  const registryFailure = checkV1Registry(projectDefinition, projectWhere, byId);
  if (registryFailure) {
    return registryFailure;
  }
  return checkV1References(project, byId);
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
  const structure = await validateProject(projectRoot);
  if (structure.outcome !== "valid") {
    const modelReason = structure.outcome === "denied" ? "model.structure-denied" : "model.structure-invalid";
    return { outcome: structure.outcome, reasonCodes: [modelReason, ...structure.reasonCodes] };
  }
  const loaded = await loadProject(projectRoot);
  if (loaded.error) {
    return loaded.error;
  }
  let schemaVersion = null;
  for (const { document, where } of loaded.documents) {
    const fileName = where.split("/").pop();
    const allowedKinds = FILE_KINDS[fileName];
    const failure = checkDocumentShape(document, where, allowedKinds, schemaVersion);
    if (failure) {
      return failure;
    }
    schemaVersion ??= document.schema_version;
  }
  const semantic = schemaVersion === SCHEMA_V1 ? checkSemanticsV1(loaded) : checkSemanticsV0(loaded);
  if (semantic) {
    return semantic;
  }
  const kinds = {};
  for (const { document } of loaded.documents) {
    for (const definition of document.definitions) {
      kinds[definition.kind] = (kinds[definition.kind] ?? 0) + 1;
    }
  }
  let modules;
  let symbols;
  if (schemaVersion === SCHEMA_V1) {
    modules = loaded.documents.flatMap(({ document }) => document.definitions
      .filter((definition) => definition.kind === "module")
      .map((definition) => definition.id)).sort(UTF8_BYTE_ORDER);
    symbols = loaded.documents.flatMap(({ document }) => document.definitions
      .filter((definition) => SYMBOL_KINDS.has(definition.kind))
      .map((definition) => definition.id)).sort(UTF8_BYTE_ORDER);
  } else {
    modules = loaded.documents
      .map((entry) => entry.moduleName)
      .filter((name) => name !== null)
      .filter((name, index, all) => all.indexOf(name) === index)
      .sort();
  }
  const report = {
    status: "valid",
    modules,
    targets: loaded.targets,
    kinds
  };
  if (schemaVersion === SCHEMA_V1) {
    report.schemaVersion = schemaVersion;
    report.symbols = symbols;
  }
  return {
    outcome: "valid",
    report
  };
}

async function runFixtureSet(directory, label) {
  const entries = await readdir(directory, { withFileTypes: true });
  const results = { valid: 0, invalid: 0 };
  const mismatches = [];
  for (const entry of entries.sort((a, b) => (a.name < b.name ? -1 : 1))) {
    if (!entry.isDirectory() || (!entry.name.startsWith("valid-") && !entry.name.startsWith("invalid-"))) {
      continue;
    }
    const fixtureDir = join(directory, entry.name);
    let expectation = { outcome: "valid" };
    if (entry.name.startsWith("invalid-")) {
      try {
        expectation = JSON.parse(await readFile(join(fixtureDir, "expect.json"), "utf8"));
      } catch {
        expectation = null;
      }
    }
    if (expectation === null || (expectation.outcome !== "valid" && typeof expectation.reasonCodes?.[0] !== "string")) {
      mismatches.push({ fixture: `${label}/${entry.name}`, expectedReason: "model.expect-missing", actualReason: null });
      continue;
    }
    const result = await validateModel(fixtureDir);
    if (result.outcome !== expectation.outcome) {
      mismatches.push({ fixture: `${label}/${entry.name}`, expected: expectation.outcome, actual: result.outcome, actualReason: result.reasonCodes?.[0] ?? null });
      continue;
    }
    if (expectation.outcome === "invalid" && result.reasonCodes[0] !== expectation.reasonCodes[0]) {
      mismatches.push({ fixture: `${label}/${entry.name}`, expectedReason: expectation.reasonCodes[0], actualReason: result.reasonCodes[0] });
      continue;
    }
    results[result.outcome === "valid" ? "valid" : "invalid"] += 1;
  }
  return { results, mismatches };
}

async function runFixtureConformance() {
  const v0 = await runFixtureSet(fixturesDir, SCHEMA_V0);
  const v1 = await runFixtureSet(fixturesV1Dir, SCHEMA_V1);
  const mismatches = [...v0.mismatches, ...v1.mismatches];
  if (mismatches.length > 0) {
    return { outcome: "invalid", report: { status: "invalid", reasonCodes: ["model.fixture-mismatch"], mismatches } };
  }
  // Keep the published v0.1 `fixtures` member byte-for-byte compatible for
  // existing consumers; the successor count is additive and versioned.
  return {
    outcome: "valid",
    report: {
      status: "valid",
      fixtures: v0.results,
      versionedFixtures: { [SCHEMA_V1]: v1.results }
    }
  };
}

function checkStandaloneSymbolId(value) {
  const failure = checkV1SymbolId(value, "semantic-id");
  if (failure) {
    return failure;
  }
  return null;
}

export async function main(argv) {
  const args = argv ?? process.argv.slice(2);
  let project = null;
  let checkId = null;
  for (let i = 0; i < args.length; i += 1) {
    if (args[i] === "--project" && args[i + 1] !== undefined && !args[i + 1].startsWith("--") && project === null && checkId === null) {
      project = args[i + 1];
      i += 1;
    } else if (args[i] === "--check-id" && args[i + 1] !== undefined && !args[i + 1].startsWith("--") && checkId === null && project === null) {
      checkId = args[i + 1];
      i += 1;
    } else {
      // Operator-controlled argv is never echoed: the usage envelope is a
      // stable, typed, static failure shape with no argument content.
      process.stderr.write(JSON.stringify({ status: "invalid", reasonCodes: ["model.usage"] }, null, 2) + "\n");
      return 1;
    }
  }
  if (checkId !== null) {
    const failure = checkStandaloneSymbolId(checkId);
    if (failure) {
      process.stderr.write(JSON.stringify({ status: "invalid", reasonCodes: failure.reasonCodes }, null, 2) + "\n");
      return 1;
    }
    process.stdout.write(JSON.stringify({ status: "valid", semanticId: checkId, canonicalKey: checkId }, null, 2) + "\n");
    return 0;
  }
  if (project !== null) {
    const result = await validateModel(resolve(project));
    if (result.outcome === "valid") {
      process.stdout.write(JSON.stringify(result.report, null, 2) + "\n");
      return 0;
    }
    const envelope = JSON.stringify({ status: result.outcome, reasonCodes: result.reasonCodes }, null, 2) + "\n";
    if (result.outcome === "denied") {
      process.stdout.write(envelope);
      return 3;
    }
    process.stderr.write(envelope);
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
