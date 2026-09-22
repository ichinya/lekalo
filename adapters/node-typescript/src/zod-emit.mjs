/**
 * Deterministic TypeScript text emitter for the Zod generator (issue #45,
 * plan §3.2). Input is the pure schema AST of `zod-map.mjs`; output is one
 * TypeScript module per Lekalo module, the shared `runtime.ts`, the
 * `index.ts` barrel, and one canonical `.map.json` sidecar per module.
 *
 * Byte stability is the contract (acceptance criterion 6):
 * - fixed header comment (adapter id/version, contract versions, input
 *   digest — no timestamps, no paths, no host data);
 * - fixed import block (zod, runtime, cross-module — each sorted);
 * - declarations emitted in topological order over intra-module type
 *   dependencies, ties broken by semantic-id byte order;
 * - 2-space indent, LF endings, no trailing whitespace, exactly one
 *   final newline;
 * - string literals emitted via JSON.stringify (no escaping drift);
 * - the sidecar is canonical JSON (keys sorted by UTF-8 byte order,
 *   compact) with the same final-newline rule.
 *
 * Each rendered declaration records its half-open UTF-8 byte range so the
 * core ownership manifest can ingest the ranges as source-map bindings.
 */

import { createHash } from "node:crypto";
import { MAP_CONTRACT, ZOD_DIR } from "./zod-map.mjs";

/** The emitting adapter identity; kept in lockstep with the kernel. */
export const ADAPTER_ID = "lekalo-target-node-typescript";

/**
 * SHA-256 of text as `sha256:<hex>` — the header input digest and the
 * sidecar/plan content digests all share this one spelling.
 */
export function sha256(text) {
  return "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");
}

/**
 * Canonical compact JSON: object keys sorted by unsigned UTF-8 byte order,
 * no whitespace, arrays keep explicit identity order. Mirrors the core
 * serializer byte for byte on the closed sidecar shapes.
 */
export function canonicalJson(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) throw new TypeError("non-finite number");
      return Number.isInteger(value) && Math.abs(value) < 1e15
        ? String(value)
        : JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object": {
      if (Array.isArray(value)) {
        return `[${value.map(canonicalJson).join(",")}]`;
      }
      const keys = Object.keys(value).sort((left, right) => {
        const leftBytes = Buffer.from(left, "utf8");
        const rightBytes = Buffer.from(right, "utf8");
        const length = Math.min(leftBytes.length, rightBytes.length);
        for (let index = 0; index < length; index += 1) {
          if (leftBytes[index] !== rightBytes[index]) {
            return leftBytes[index] - rightBytes[index];
          }
        }
        return leftBytes.length - rightBytes.length;
      });
      return `{${keys
        .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
        .join(",")}}`;
    }
    default:
      throw new TypeError("unserializable value");
  }
}

/** One rendered file: logical path, exact text, and the sidecar map. */
function file(path, text, map) {
  return { path, text, map };
}

/**
 * Emit every generated file of one mapped project.
 *
 * `input` is `{ modules, inputDigest, adapterVersion, irIdentity }`:
 * `modules` is the mapper output, `inputDigest` is the sha256 of the exact
 * canonical IR bytes generation consumed, `adapterVersion` and
 * `irIdentity` are the header contract pins. Returns sorted `{path, text,
 * map}` records: `runtime.ts`, `index.ts`, one `<module>.ts` and one
 * `<module>.map.json` per emission group. The `map` member carries
 * `{ owner, fields, declarations }` and is non-null only on sidecars.
 */
export function emitFiles({ modules, inputDigest, adapterVersion, irIdentity }) {
  const files = [file(`${ZOD_DIR}/runtime.ts`, runtimeText(adapterVersion))];
  const groups = emissionGroups(modules);
  for (const group of groups) {
    const others = groups.filter((candidate) => candidate !== group);
    const outbound = groupImports(group.modules, new Set(group.modules.map((module) => module.id)), others);
    const emitGroup = { ...group, imports: outbound };
    const emitted = emitModule(emitGroup, {
      inputDigest,
      adapterVersion,
      irIdentity,
    });
    files.push(file(`${ZOD_DIR}/${group.id}.ts`, emitted.text));
    files.push(
      file(
        `${ZOD_DIR}/${group.id}.map.json`,
        `${canonicalJson(emitted.map)}\n`,
        emitted.map,
      ),
    );
  }
  files.push(file(`${ZOD_DIR}/index.ts`, barrelText(groups)));
  files.sort((left, right) =>
    left.path < right.path ? -1 : left.path > right.path ? 1 : 0,
  );
  return files;
}

// ---------------------------------------------------------------------------
// Shared runtime.
// ---------------------------------------------------------------------------

function runtimeText(adapterVersion) {
  // The runtime is written in the JS-strict subset of TypeScript (JSDoc
  // types only): the file is simultaneously valid TS for typechecking
  // consumers and directly executable ESM JavaScript for runtime probes.
  return `// Generated by ${ADAPTER_ID}@${adapterVersion} (zod-schema-generator).
// Adapter-owned runtime helpers shared by every generated module. Content
// depends only on the adapter version, so this file is itself a
// determinism probe: any byte drift means a stale artifact. Do not edit;
// regenerate with \`lekalo generate\`.
import * as z from "zod";

/** A calendar date string (YYYY-MM-DD): the JSON-faithful date form. */
export const LekaloDateString = z.string().regex(/^\\d{4}-\\d{2}-\\d{2}$/);

/**
 * Brand one schema with its Lekalo semantic id: \`z.infer\` yields
 * \`<base> & z.BRAND<"module.name">\`, so raw values cannot masquerade as
 * opaque ids — they must pass \`.parse\`. The brand applies over the
 * scalar's own declared base schema (uuid, string, number, date, …);
 * branding never tightens validation beyond the base. The two-argument
 * form keeps the emitted files plain-JS executable.
 *
 * @template {{ safeParse: Function }} T
 * @param {T} schema
 * @param {string} semanticId
 * @returns {T}
 */
export function lekaloBrand(schema, semanticId) {
  return schema.brand(semanticId);
}

/**
 * Map raw zod issues to Lekalo semantic ids through the field map of the
 * sibling \`<module>.map.json\` sidecar. Exact field paths win; otherwise
 * the closest enclosing path wins; otherwise the module owner. Unknown
 * input paths are attributed, never dropped. The issue parameter is the
 * structural shape every zod issue satisfies, so plain JavaScript
 * consumers can call this helper without importing zod.
 *
 * @param {{ path: (string | number)[], code: string }[]} issues
 * @param {Record<string, string>} fields
 * @param {string} owner
 * @returns {{ path: string, semanticId: string, code: string }[]}
 */
export function normalizeIssues(issues, fields, owner) {
  return issues.map((issue) => {
    const path = issue.path.join(".");
    return {
      path,
      semanticId: resolveFieldOwner(fields, path, owner),
      code: issue.code,
    };
  });
}

/**
 * Exact path first, then the closest enclosing mapped path, then the
 * mapped root symbol (the \`""\` entry), and only then the owner
 * argument — one fallback chain, coherent with the sidecar bytes.
 *
 * @param {Record<string, string>} fields
 * @param {string} path
 * @param {string} owner
 * @returns {string}
 */
function resolveFieldOwner(fields, path, owner) {
  if (Object.prototype.hasOwnProperty.call(fields, path)) {
    return fields[path];
  }
  let prefix = path;
  for (;;) {
    const cut = prefix.lastIndexOf(".");
    if (cut <= 0) break;
    prefix = prefix.slice(0, cut);
    if (Object.prototype.hasOwnProperty.call(fields, prefix)) {
      return fields[prefix];
    }
  }
  if (Object.prototype.hasOwnProperty.call(fields, "")) {
    return fields[""];
  }
  return owner;
}
`;
}

function barrelText(groups) {
  const lines = [
    `// Generated by barrel emission (issue #45). One stable import root for`,
    `// every generated Zod group; entries are sorted and the set changes`,
    `// only when the set of schema-bearing modules changes.`,
  ];
  const entries = ["runtime", ...groups.map((group) => group.id)].sort();
  for (const entry of entries) {
    lines.push(`export * from "./${entry}";`);
  }
  return `${lines.join("\n")}\n`;
}

// ---------------------------------------------------------------------------
// Per-module rendering.
// ---------------------------------------------------------------------------

/**
 * Partition the mapped modules into emission groups: the union of weakly
 * connected components of the cross-module reference graph. A group of one
 * module keeps the plan's one-file-per-module layout; modules that mutually
 * reference each other merge into one deterministic file (the head module's
 * id names it), because separate ESM modules cannot express mutual
 * top-level schema constants. Group ids are sorted and deterministic.
 */
export function emissionGroups(modules) {
  const ids = modules.map((module) => module.id);
  const byId = new Map(modules.map((module) => [module.id, module]));
  // Undirected edges from each module's declared cross-module imports.
  const neighbors = new Map(ids.map((id) => [id, new Set()]));
  for (const module of modules) {
    for (const entry of module.imports) {
      if (!byId.has(entry.module)) continue;
      neighbors.get(module.id).add(entry.module);
      neighbors.get(entry.module).add(module.id);
    }
  }
  const visited = new Set();
  const groups = [];
  for (const id of ids) {
    if (visited.has(id)) continue;
    const component = [];
    const queue = [id];
    visited.add(id);
    while (queue.length > 0) {
      const current = queue.shift();
      component.push(current);
      for (const next of [...neighbors.get(current)].sort()) {
        if (!visited.has(next)) {
          visited.add(next);
          queue.push(next);
        }
      }
    }
    component.sort();
    const members = component.map((member) => byId.get(member));
    groups.push({
      id: component[0],
      modules: members,
      declarations: members.flatMap((member) => member.declarations),
      imports: members.flatMap((member) => member.imports),
      // First-wins merge in the members' (sorted) order: a colliding
      // field path keeps the first deterministic owner instead of
      // silently moving to the last writer (issue #45 review F-2).
      fields: members.reduce((merged, member) => {
        for (const key of Object.keys(member.fields)) {
          if (!Object.hasOwn(merged, key)) {
            merged[key] = member.fields[key];
          }
        }
        return merged;
      }, {}),
    });
  }
  return groups;
}

function emitModule(module, context) {
  const header = [
    `// Generated by ${ADAPTER_ID}@${context.adapterVersion}`,
    `// (zod-schema-generator) from ${context.irIdentity} input ${context.inputDigest}.`,
    `// Do not edit: regenerate with \`lekalo generate\`. Presence (required)`,
    `// and nullability (optional wrapper) are orthogonal axes here:`,
    `// \`required\` governs key presence, the optional wrapper emits`,
    `// \`.nullable()\`. Closed objects mirror the closed model (.strict()).`,
  ];
  const imports = [
    `import * as z from "zod";`,
    ...collectRuntimeImports(module),
    ...module.imports.map(
      (entry) =>
        `import { ${entry.names.join(", ")} } from "./${entry.module}";`,
    ),
  ];
  const body = [];
  const declarations = [];
  let cursor = byteLength(`${header.join("\n")}\n\n${imports.join("\n")}\n\n`);
  for (const declaration of orderDeclarations(module.declarations)) {
    const text = renderDeclaration(declaration);
    const start = cursor;
    const end = start + byteLength(text);
    declarations.push({
      id: declaration.semanticId,
      export: declaration.exportName,
      start,
      end,
    });
    body.push(text);
    cursor = end + 1; // the blank line between declarations
  }
  const text = `${[...header, "", ...imports, "", ...body].join("\n")}\n`;
  // The owner fallback agrees with the flat map: the root path entry wins
  // when present (a merged group's head module need not own the root
  // symbol), and the head module id is only the last-resort fallback.
  const owner = Object.hasOwn(module.fields, "")
    ? module.fields[""]
    : module.modules[0].id;
  return {
    text,
    map: {
      contract: MAP_CONTRACT,
      adapter: { id: ADAPTER_ID, version: context.adapterVersion },
      owner,
      fields: module.fields,
      declarations,
    },
  };
}

/**
 * Cross-group imports of one emission group: references reaching outside
 * the group, one sorted statement per target group, names sorted.
 */
function collectGroupImports(group) {
  const groupIds = new Set(group.modules.map((module) => module.id));
  const localExports = new Set(group.declarations.map((decl) => decl.exportName));
  const byModule = new Map();
  for (const entry of group.imports) {
    if (groupIds.has(entry.module)) continue;
    const names = entry.names.filter((name) => !localExports.has(name));
    if (names.length === 0) continue;
    let bucket = byModule.get(entry.module);
    if (!bucket) {
      bucket = new Set();
      byModule.set(entry.module, bucket);
    }
    for (const name of names) bucket.add(name);
  }
  return [...byModule.keys()].sort().map((moduleId) => {
    const names = [...byModule.get(moduleId)].sort();
    return `import { ${names.join(", ")} } from "./${moduleId}";`;
  });
}

/**
 * Cross-group imports of one mapped module (pre-merge), filtered to a
 * target set of group ids; mutual references inside one group contribute
 * no imports because the referenced declarations are emitted locally.
 */
function groupImports(modules, ownIds, otherGroups) {
  const otherIds = new Set(otherGroups.flatMap((group) => group.modules.map((module) => module.id)));
  const otherExports = new Set(otherGroups.flatMap((group) => group.declarations.map((decl) => decl.exportName)));
  const byModule = new Map();
  for (const module of modules) {
    for (const entry of module.imports) {
      if (ownIds.has(entry.module) || !otherIds.has(entry.module)) continue;
      const names = entry.names.filter((name) => otherExports.has(name));
      if (names.length === 0) continue;
      let bucket = byModule.get(entry.module);
      if (!bucket) {
        bucket = new Set();
        byModule.set(entry.module, bucket);
      }
      for (const name of names) bucket.add(name);
    }
  }
  return [...byModule.keys()].sort().map((moduleId) => {
    const names = [...byModule.get(moduleId)].sort();
    return { module: moduleId, names };
  });
}

function collectRuntimeImports(module) {
  const used = new Set();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "dateString") {
      used.add("LekaloDateString");
      return;
    }
    if (expr.k === "brand") {
      used.add("lekaloBrand");
      visit(expr.inner);
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
  return used.size > 0
    ? [`import { ${[...used].sort().join(", ")} } from "./runtime";`]
    : [];
}

/**
 * Topological order over intra-module type dependencies, ties broken by
 * semantic-id byte order (plan §3.2). Reference cycles are unreachable —
 * the model rejects recursive types — and a defensive visit marker turns a
 * would-be cycle into a deterministic emission order instead of a hang.
 */
export function orderDeclarations(declarations) {
  const byExport = new Map(
    declarations.map((declaration) => [declaration.exportName, declaration]),
  );
  const dependencies = new Map(
    declarations.map((declaration) => [
      declaration.exportName,
      intraModuleDeps(declaration, byExport),
    ]),
  );
  const ordered = [];
  const emitted = new Set();
  const visiting = new Set();
  const visit = (declaration) => {
    if (emitted.has(declaration.exportName)) return;
    if (visiting.has(declaration.exportName)) return; // defensive; unreachable
    visiting.add(declaration.exportName);
    for (const dependency of dependencies.get(declaration.exportName)) {
      visit(byExport.get(dependency));
    }
    visiting.delete(declaration.exportName);
    emitted.add(declaration.exportName);
    ordered.push(declaration);
  };
  for (const declaration of declarations) visit(declaration);
  return ordered;
}

function intraModuleDeps(declaration, byExport) {
  const deps = new Set();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "ref") {
      // Export names are unique across the whole project, so a name hit in
      // this file's declaration map is an intra-file dependency — mutual
      // merged modules included.
      if (byExport.has(expr.name)) {
        deps.add(expr.name);
      }
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
  visit(declaration.expr);
  return [...deps].sort();
}

// ---------------------------------------------------------------------------
// Declaration rendering.
// ---------------------------------------------------------------------------

function renderDeclaration(declaration) {
  const schema = renderExpr(declaration.expr, "");
  const lines = [
    `export const ${declaration.exportName} = ${schema};`,
    `export type ${declaration.typeName} = z.infer<typeof ${declaration.exportName}>;`,
  ];
  return lines.join("\n");
}

function renderExpr(expr, indent) {
  const inner = indentUnit(indent);
  switch (expr.k) {
    case "string":
      return `z.string()`;
    case "number":
      return `z.number().finite()`;
    case "boolean":
      return `z.boolean()`;
    case "dateString":
      return `LekaloDateString`;
    case "dateNative":
      return `z.date()`;
    case "datetime":
      return `z.string().datetime({ offset: true })`;
    case "uuid":
      return `z.string().uuid()`;
    case "uri":
      return `z.string().url()`;
    case "enum":
      return `z.enum([${expr.values.map((value) => JSON.stringify(value)).join(", ")}])`;
    case "brand":
      return `lekaloBrand(${renderExpr(expr.inner, indent)}, ${JSON.stringify(expr.brand)})`;
    case "ref":
      return expr.name;
    case "array":
      return `z.array(${renderExpr(expr.item, indent)})`;
    case "nullable":
      return `${renderExpr(expr.inner, indent)}.nullable()`;
    case "optional":
      return `${renderExpr(expr.inner, indent)}.optional()`;
    case "object": {
      if (expr.fields.length === 0) {
        return expr.strict ? `z.object({}).strict()` : `z.object({}).strip()`;
      }
      const body = expr.fields
        .map(
          (field) =>
            `${inner}  ${JSON.stringify(field.name)}: ${renderExpr(field.expr, `${inner}  `)},`,
        )
        .join("\n");
      return `z.object({\n${body}\n${inner}})${expr.strict ? ".strict()" : ".strip()"}`;
    }
    default:
      throw new TypeError(`unrenderable expression kind ${expr?.k}`);
  }
}

function indentUnit(indent) {
  return indent;
}

function byteLength(text) {
  return Buffer.byteLength(text, "utf8");
}
