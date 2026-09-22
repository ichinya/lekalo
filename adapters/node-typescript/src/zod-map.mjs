/**
 * Pure IR → schema-AST mapping for the Zod generator (issue #45, plan §3.2).
 *
 * Inputs are one parsed canonical IR document (`dev.lekalo.ir@0.2.16`) and
 * one resolved codegen policy; the output is a closed schema AST plus typed
 * findings. No filesystem, clock, or environment access happens here, so
 * the same inputs always map to the same AST — the determinism contract the
 * byte-stable emitter and the ownership manifest both depend on.
 *
 * Mapping rules (plan §4):
 * - `required` governs key presence (`.optional()`); the `optional` type
 *   wrapper governs value nullability (`.nullable()`); the four presence ×
 *   nullability combinations stay distinct and all four are exercised by
 *   the committed fixture vectors.
 * - Every schema-bearing definition kind maps: scalars, enums,
 *   value-objects, entities, command inputs, query returns, event
 *   payloads. Effects, endpoints, policies, scenarios, and target bindings
 *   are not schema-bearing and are skipped by design.
 * - Constructs the mapper cannot express (unknown scalar bases, refs to
 *   non-schema-bearing definitions, unknown type shapes, missing query
 *   returns) are classified as `zod.unsupported-construct` findings with
 *   `symbol:<id>` details and the whole definition is skipped — never a
 *   silent drop.
 * - Object shapes are closed (`.strict()`) to mirror the closed model;
 *   the `unknown-keys` policy relaxes this to `.strip()` when resolved.
 * - Scalars named by any entity `identity` member (directly or through
 *   an `optional` wrapper) are emitted branded over their own declared
 *   base schema, so opaque ids cannot be constructed from raw values
 *   without `.parse` — and branding never tightens the base validation.
 */

/** The sidecar micro-contract token (adapter-owned, plan §3.6). */
export const MAP_CONTRACT = "lekalo/zod-map/v0.3.2";

/** The sole IR contract this mapper accepts. */
export const IR_IDENTITY = "dev.lekalo.ir@0.2.16";

/** The logical output directory of the generated Zod modules. */
export const ZOD_DIR = "src/generated/node-typescript/zod";

/** The closed finding code for constructs outside the mappable subset. */
export const UNSUPPORTED = "zod.unsupported-construct";

/** The documented default codegen policy (plan §4, R4). */
export const DEFAULT_POLICY = deepFreeze({
  date: "date-string",
  unknownKeys: "strict",
});

/** The closed set of definition kinds that carry schemas. */
export const SCHEMA_KINDS = deepFreeze([
  "scalar",
  "enum",
  "value-object",
  "entity",
  "command",
  "query",
  "event",
]);

/** The closed date policies. */
export const DATE_POLICIES = deepFreeze(["date-string", "date-native"]);

/** The closed unknown-keys policies. */
export const UNKNOWN_KEY_POLICIES = deepFreeze(["strict", "strip"]);

/** The maximum field-path flattening depth of one sidecar map. */
const MAX_FLATTEN_DEPTH = 8;

function deepFreeze(value) {
  if (value !== null && typeof value === "object") {
    for (const key of Object.keys(value)) deepFreeze(value[key]);
    Object.freeze(value);
  }
  return value;
}

/** `task_id` → `TaskId`; empty underscore segments contribute nothing. */
export function pascal(text) {
  return text
    .split("_")
    .filter((part) => part.length > 0)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
}

const KIND_SUFFIX = deepFreeze({
  command: "Input",
  query: "Result",
  event: "Payload",
});

/**
 * The internal control signal for one construct outside the mappable
 * subset. Caught at the declaration boundary and classified as a finding
 * with the `symbol:<id>` detail token — never swallowed.
 */
class Unsupported extends Error {
  constructor(detail) {
    super(detail);
    this.name = "Unsupported";
  }
}

/**
 * Map one project IR document to per-module schema ASTs.
 *
 * `ir` is the parsed canonical JSON (definitions + modules arrays);
 * `policy` is a resolved `{date, unknownKeys}` policy. Returns
 * `{ modules, findings }` where modules are sorted by id, declarations are
 * sorted by semantic id, and every declaration carries its export name.
 */
export function mapProject(ir, policy = DEFAULT_POLICY) {
  const definitions = indexDefinitions(ir);
  const names = allocateNames(ir);
  const branded = collectBrandedScalars(ir);
  const context = { policy, definitions, names, branded, findings: [] };

  // Pass 1: map every schema-bearing definition; unsupported constructs
  // classify as findings and skip exactly one definition.
  const modules = new Map();
  for (const definition of ir.definitions ?? []) {
    if (!SCHEMA_KINDS.includes(definition.kind)) continue;
    const mapped = mapDefinition(definition, context);
    if (!mapped) continue;
    const moduleId = definition.id.split(".")[0];
    let module = modules.get(moduleId);
    if (!module) {
      module = { id: moduleId, declarations: [], imports: [], fields: {} };
      modules.set(moduleId, module);
    }
    module.declarations.push(mapped);
  }

  // Pass 2: export-name registry (for cross-module imports) and per-module
  // sidecar field paths (flattening needs every declaration mapped).
  const byExport = new Map();
  for (const module of modules.values()) {
    for (const declaration of module.declarations) {
      byExport.set(`${declaration.module}/${declaration.exportName}`, declaration);
    }
  }
  const result = [];
  for (const moduleId of [...modules.keys()].sort()) {
    const module = modules.get(moduleId);
    module.declarations.sort(bySemanticId);
    module.imports = collectImports(module);
    module.fields = {};
    for (const declaration of module.declarations) {
      recordFieldPaths(module, declaration, byExport);
    }
    result.push(module);
  }
  context.findings.sort(compareFindings);
  return { modules: result, findings: context.findings };
}

function indexDefinitions(ir) {
  const index = new Map();
  for (const definition of ir.definitions ?? []) {
    index.set(definition.id, definition);
  }
  return index;
}

/**
 * Scalars named by any entity `identity` member's type are emitted
 * branded — through the closed wrapper chain: a direct ref, or an
 * `optional`-wrapper ref (a nullable identity value is legal IR). The
 * scalar keeps its declared base; branding (`z.BRAND<"semantic.id">`)
 * never tightens validation beyond that base, so raw values cannot
 * masquerade as opaque ids without `.parse`.
 */
function collectBrandedScalars(ir) {
  const branded = new Set();
  const visit = (type) => {
    if (type === null || typeof type !== "object" || Array.isArray(type)) {
      return;
    }
    if (typeof type.ref === "string") {
      branded.add(type.ref);
      return;
    }
    // Only the optional wrapper composes (the nullability axis); list
    // elements are collection members, never identity values.
    if (type.optional !== undefined) {
      visit(type.optional);
    }
  };
  for (const definition of ir.definitions ?? []) {
    if (definition.kind !== "entity") continue;
    const byName = new Map(
      (definition.fields ?? []).map((field) => [field.name, field]),
    );
    for (const name of definition.identity ?? []) {
      visit(byName.get(name)?.type);
    }
  }
  return branded;
}

/** Allocate every schema-bearing export name before any body is mapped. */
function allocateNames(ir) {
  const names = new Map();
  const used = new Map();
  for (const definition of ir.definitions ?? []) {
    if (!SCHEMA_KINDS.includes(definition.kind)) continue;
    const moduleId = definition.id.split(".")[0];
    const local = definition.id.slice(moduleId.length + 1);
    let candidate =
      pascal(moduleId) + pascal(local) + (KIND_SUFFIX[definition.kind] ?? "");
    const seen = used.get(moduleId) ?? new Set();
    used.set(moduleId, seen);
    let suffix = 1;
    while (seen.has(candidate)) {
      suffix += 1;
      candidate = `${candidate.replace(/\d+$/, "")}${suffix}`;
    }
    seen.add(candidate);
    names.set(definition.id, {
      exportName: `${candidate}Schema`,
      typeName: candidate,
    });
  }
  return names;
}

function mapDefinition(definition, context) {
  const moduleId = definition.id.split(".")[0];
  const naming = context.names.get(definition.id);
  try {
    switch (definition.kind) {
      case "scalar":
        return mapScalar(definition, naming, context, moduleId);
      case "enum":
        return mapEnum(definition, naming, moduleId);
      case "value-object":
      case "entity":
      case "command":
      case "event":
        return mapObject(definition, naming, context, moduleId);
      case "query":
        return mapQuery(definition, naming, context, moduleId);
      default:
        return undefined;
    }
  } catch (error) {
    if (error instanceof Unsupported) {
      context.findings.push({
        path: `${ZOD_DIR}/${moduleId}.ts`,
        code: UNSUPPORTED,
        detail: `symbol:${definition.id}`,
      });
      return undefined;
    }
    throw error;
  }
}

function mapScalar(definition, naming, context, moduleId) {
  const expr = scalarExpr(definition.base, context.policy);
  const branded = context.branded.has(definition.id);
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "scalar",
    exportName: naming.exportName,
    typeName: naming.typeName,
    branded,
    expr: branded ? { k: "brand", inner: expr, brand: definition.id } : expr,
  };
}

function mapEnum(definition, naming, moduleId) {
  const values = (definition.values ?? []).map((value) => value?.value);
  if (
    values.length === 0 ||
    values.some((value) => typeof value !== "string")
  ) {
    throw new Unsupported(definition.id);
  }
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "enum",
    exportName: naming.exportName,
    typeName: naming.typeName,
    // Declared order is semantic; never sort enum members.
    values,
    expr: { k: "enum", values },
  };
}

function mapObject(definition, naming, context, moduleId) {
  const strict = context.policy.unknownKeys === "strict";
  const members =
    definition.kind === "command"
      ? definition.input
      : definition.kind === "event"
        ? definition.payload
        : definition.fields;
  const fields = (members ?? []).map((field) => {
    const inner = mapType(field.type, context);
    // Presence axis: an absent `required` member makes the key optional.
    const expr = field.required === true ? inner : { k: "optional", inner };
    return { name: field.name, required: field.required === true, expr };
  });
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "object",
    objectKind: definition.kind,
    exportName: naming.exportName,
    typeName: naming.typeName,
    strict,
    fields,
    expr: {
      k: "object",
      strict,
      fields: fields.map((field) => ({ name: field.name, expr: field.expr })),
    },
  };
}

function mapQuery(definition, naming, context, moduleId) {
  if (!definition.returns) {
    throw new Unsupported(definition.id);
  }
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "alias",
    exportName: naming.exportName,
    typeName: naming.typeName,
    expr: mapType(definition.returns, context),
  };
}

/** Compose one type expression bottom-up (wrapper rules, plan §4). */
function mapType(type, context) {
  if (type === null || typeof type !== "object" || Array.isArray(type)) {
    throw new Unsupported("type-shape");
  }
  if (typeof type.ref === "string") {
    return mapRef(type.ref, context);
  }
  if (type.list !== undefined) {
    return { k: "array", item: mapType(type.list, context) };
  }
  if (type.optional !== undefined) {
    // Nullability axis: the optional wrapper maps to `.nullable()`.
    return { k: "nullable", inner: mapType(type.optional, context) };
  }
  throw new Unsupported("type-shape");
}

function mapRef(id, context) {
  const target = context.definitions.get(id);
  if (!target || !SCHEMA_KINDS.includes(target.kind)) {
    throw new Unsupported(`ref:${id}`);
  }
  const naming = context.names.get(id);
  return {
    k: "ref",
    name: naming.exportName,
    module: id.split(".")[0],
  };
}

function scalarExpr(base, policy) {
  switch (base) {
    case "string":
      return { k: "string" };
    case "number":
      return { k: "number" };
    case "boolean":
      return { k: "boolean" };
    case "date":
      return policy.date === "date-native"
        ? { k: "dateNative" }
        : { k: "dateString" };
    case "datetime":
      return { k: "datetime" };
    case "uuid":
      return { k: "uuid" };
    case "uri":
      return { k: "uri" };
    default:
      throw new Unsupported(`base:${base}`);
  }
}

function collectImports(module) {
  const byModule = new Map();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "ref") {
      if (expr.module === module.id) return;
      let entry = byModule.get(expr.module);
      if (!entry) {
        entry = new Set();
        byModule.set(expr.module, entry);
      }
      entry.add(expr.name);
      return;
    }
    if (expr.k === "array") {
      visit(expr.item);
      return;
    }
    if (expr.k === "object") {
      for (const field of expr.fields) visit(field.expr);
      return;
    }
    if (expr.k === "nullable" || expr.k === "optional" || expr.k === "brand") {
      visit(expr.inner);
    }
  };
  for (const declaration of module.declarations) visit(declaration.expr);
  return [...byModule.keys()].sort().map((id) => ({
    module: id,
    names: [...byModule.get(id)].sort(),
  }));
}

/**
 * Record the field-path → semantic-id entries of one module's sidecar map
 * (plan §3.6): every declaration export plus every declared field,
 * flattened through same-module object refs (array elements contribute an
 * index segment) so runtime zod issue paths resolve to owning symbols.
 * Flattening depth is bounded; deeper nesting falls back to the enclosing
 * entry at runtime.
 */
function recordFieldPaths(module, mapped, byExport) {
  const fields = module.fields;
  fields[mapped.exportName] = mapped.semanticId;
  if (mapped.kind !== "object") return;
  fields[""] = mapped.semanticId;
  for (const field of mapped.fields) {
    fields[field.name] = mapped.semanticId;
    flattenFieldPath(fields, field.expr, field.name, mapped, byExport, 0);
  }
}

function flattenFieldPath(fields, expr, prefix, owner, byExport, depth) {
  if (depth >= MAX_FLATTEN_DEPTH) return;
  if (!expr || typeof expr !== "object") return;
  if (expr.k === "nullable" || expr.k === "optional") {
    flattenFieldPath(fields, expr.inner, prefix, owner, byExport, depth + 1);
    return;
  }
  if (expr.k === "array") {
    flattenFieldPath(
      fields,
      expr.item,
      `${prefix}.0`,
      owner,
      byExport,
      depth + 1,
    );
    return;
  }
  if (expr.k !== "ref" || expr.module !== owner.module) return;
  const declaration = byExport.get(`${expr.module}/${expr.name}`);
  if (!declaration || declaration.kind !== "object") return;
  for (const field of declaration.fields) {
    const path = `${prefix}.${field.name}`;
    fields[path] = declaration.semanticId;
    flattenFieldPath(fields, field.expr, path, declaration, byExport, depth + 1);
  }
}

function bySemanticId(left, right) {
  return left.semanticId < right.semanticId
    ? -1
    : left.semanticId > right.semanticId
      ? 1
      : 0;
}

function compareFindings(left, right) {
  const key = (finding) =>
    `${finding.path}\u0000${finding.code}\u0000${finding.detail ?? ""}`;
  const leftKey = key(left);
  const rightKey = key(right);
  return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0;
}
