/**
 * Drizzle ORM evidence extraction for the Node/TypeScript target
 * (issue #116).
 *
 * One read-only compiler pass over the scanner's Program that maps
 * statically decidable Drizzle surfaces onto a closed target-evidence
 * document attached as `index.drizzle`:
 *
 *   tables        — pgTable/mysqlTable declarations: columns, physical
 *                   names, nullability, defaults, identity/autoincrement,
 *                   primary/unique/index/foreign constraints with spans;
 *   relations     — `relations(table, ({one, many}) => ...)` endpoints,
 *                   cardinality evidence, join-table candidates;
 *   queries       — select/insert/update/delete builder chains rooted at
 *                   a Drizzle database/transaction receiver: field reads,
 *                   writes, projections, inputs, execution and dynamic
 *                   construction limitations;
 *   transactions  — db.transaction callback boundaries, members, nesting,
 *                   rollback sites, escaped-handle limitations;
 *   migrations    — drizzle.config.* literals plus bounded migration
 *                   folder references (paths + digests, never execution);
 *   scope         — tenant/workspace equality predicates as evidence,
 *                   and explicit missing-scope limitations;
 *   bindings      — owner-supplied entity/table binding input validated
 *                   against extracted tables, plus storage projection
 *                   comparison results (declaration vs projection, never
 *                   live database state).
 *
 * Reliability model (mirrors the issue's requirements):
 *   - TypeScript checker identity against the embedded upstream
 *     declaration closure is the primary evidence; a recognizer only
 *     trusts callees whose declarations resolve into the vendored
 *     drizzle-orm pin (name similarity alone is never evidence);
 *   - raw SQL, `$dynamic()`, dynamic values/set, unresolved receivers
 *     and unknown predicates produce explicit limitations and partial
 *     completeness — never a silently empty complete set;
 *   - a declared schema is not live database state: DB evidence stays
 *     `unknown` here by construction;
 *   - detected effects carry `confidence: "extracted"` and
 *     `canonical: false` — evidence, never canonical declarations;
 *   - a table rename changes the native identity, which invalidates
 *     bindings (rebinding requires confirmed rename history, which this
 *     adapter deliberately does not guess);
 *   - the adapter is read-only: no migrations are executed, no database
 *     connections are made, and no credential material is read.
 *
 * Determinism: every record array is sorted or emitted in a
 * deterministic walk order, every array is bounded, and the document
 * carries a SHA-256 digest over its canonical form so unchanged inputs
 * produce byte-identical evidence.
 */
import { createHash } from "node:crypto";

// ---------------------------------------------------------------------------
// 1. Identity, bounds, closed vocabularies.
// ---------------------------------------------------------------------------

/** The closed evidence document schema identity. */
export const DRIZZLE_EVIDENCE_SCHEMA = "lekalo/drizzle-evidence/v0.1.0";
/** The owner-supplied binding input schema identity. */
export const DRIZZLE_BINDINGS_INPUT_SCHEMA = "lekalo/drizzle-bindings-input/v0.1.0";
/** The owner-supplied storage projection input schema identity. */
export const DRIZZLE_PROJECTION_INPUT_SCHEMA = "lekalo/storage-projection-input/v0.1.0";
/** Version of the extractor (rule set) — part of every document. */
export const DRIZZLE_EXTRACTOR_VERSION = "0.1.0";
/** The rule-set identity stamped on every extracted row. */
export const DRIZZLE_RULESET = "drizzle-static-v1";

/** The domain-separated identity prefix of table native ids. */
const TABLE_IDENTITY_DOMAIN = "lekalo.drizzle.table.v1";

/** Maximum tables one document carries. */
const MAX_TABLES = 256;
/** Maximum columns per table. */
const MAX_COLUMNS = 128;
/** Maximum constraints per table. */
const MAX_CONSTRAINTS = 128;
/** Maximum relations one document carries. */
const MAX_RELATIONS = 256;
/** Maximum query rows one document carries. */
const MAX_QUERIES = 512;
/** Maximum transactions one document carries. */
const MAX_TRANSACTIONS = 128;
/** Maximum migration reference rows. */
const MAX_MIGRATIONS = 256;
/** Maximum migration folder entries per config row. */
const MAX_MIGRATION_ENTRIES = 64;
/** Maximum scope evidence rows. */
const MAX_SCOPE_ROWS = 512;
/** Maximum binding rows. */
const MAX_BINDINGS = 512;
/** Maximum projection comparison rows. */
const MAX_PROJECTION_ROWS = 1024;
/** Maximum limitation rows one document carries. */
const MAX_LIMITATIONS = 512;
/** Maximum contributing source files one document lists. */
const MAX_PROVENANCE_FILES = 1024;
/** Maximum links recorded from one builder chain. */
const MAX_CHAIN_LINKS = 32;
/** Maximum per-query input records. */
const MAX_INPUTS = 32;
/** Maximum predicate field reads per query. */
const MAX_QUERY_FIELDS = 64;
/** Maximum statements walked inside one transaction callback. */
const MAX_TX_STATEMENTS = 256;
/** Maximum alias-resolve hops for one receiver. */
const MAX_ALIAS_HOPS = 4;

/**
 * The bounded public drizzle-orm subpaths the recognizer supports.
 * Must stay in lockstep with the build's DRIZZLE_ENTRY_SUBPATHS (the
 * test suite asserts every mapped file exists in the embedded closure).
 */
export const DRIZZLE_SUPPORTED_SUBPATHS = [
  "index.d.ts",
  "pg-core/index.d.ts",
  "mysql-core/index.d.ts",
  "relations.d.ts",
  "sql/index.d.ts",
  "node-postgres/index.d.ts",
  "mysql2/index.d.ts",
];

/**
 * The closed limitation vocabulary. Every limitation row's code comes
 * from this set — an unknown code is a programming error and refuses.
 */
const LIMITATION_CODES = new Set([
  // attachment / provenance
  "declarations-absent", "pin-absent", "pin-mismatch", "pin-not-exact",
  // schema
  "table-name-dynamic", "column-name-dynamic", "column-type-unknown",
  "sql-default", "client-hook", "unsupported-dialect",
  "constraint-shape-unknown", "constraint-member-unresolved",
  "constraint-predicate-index",
  // relations
  "uniqueness-unproven", "cardinality-unproven", "relation-policy-missing",
  "relation-target-unresolved", "relation-fields-unresolved",
  // queries
  "receiver-unknown", "target-unresolved", "predicate-unresolved",
  "dynamic-values", "dynamic-set", "raw-sql", "dynamic-builder",
  "builder-not-executed", "projection-dynamic", "returning-dynamic",
  "join-target-unresolved", "alias-continuation",
  "input-dynamic", "upsert-alternative",
  // recognized Drizzle surfaces outside the qualified static subset —
  // they are never dropped silently (issue #116 fix round)
  "relational-query-unsupported", "batch-unsupported",
  // a construct-named callee that could not be proven against the
  // closure — never dropped silently (issue #116 fix round 3)
  "callee-unproven",
  // a member call on a module namespace object (import * as ns) — the
  // namespace is provably not a database handle, so the call is never
  // attributed to one; it is an explicit uncertainty instead
  // (issue #116 fix round 4)
  "namespace-receiver-unsupported",
  // transactions
  "query-not-tx-bound", "tx-escaped", "rollback", "nested-transaction",
  "tx-callback-shape-unknown",
  // migrations
  "migration-config-invalid", "migration-layout-unknown",
  "migration-path-escapes-root", "migration-ref-missing",
  // scope
  "scope-predicate-missing", "scope-dynamic",
  // bindings / projection
  "bindings-input-missing", "bindings-input-invalid", "binding-unresolved",
  "binding-ambiguous",
  "binding-stale-table-native-missing", "binding-stale-table-source-changed",
  "projection-input-invalid", "projection-column-missing",
  "projection-column-extra", "projection-type-divergent",
  "projection-nullability-divergent", "projection-key-divergent",
  "projection-table-missing", "projection-table-extra",
  // bounds
  "truncated",
]);

/**
 * The tenant/workspace scope-key vocabulary. Declaration of such a
 * column is evidence of a scope CANDIDATE only — never proof of
 * enforcement.
 */
const SCOPE_KEY_NAMES = new Set([
  "tenantId", "tenant_id", "workspaceId", "workspace_id",
  "organizationId", "organization_id", "orgId", "org_id", "teamId", "team_id",
]);

/** The query-builder head methods. */
const QUERY_HEADS = new Set(["select", "selectDistinct", "insert", "update", "delete"]);
/** Builder methods that continue an aliased builder chain. */
const CONTINUATION_HEADS = new Set(["from", "values", "set"]);
/** The execution/terminal builder methods of the drizzle query builders. */
const TERMINAL_METHODS = new Set(["execute", "all", "run", "get"]);
/** The join builder methods. */
const JOIN_METHODS = new Set(["leftJoin", "rightJoin", "innerJoin", "fullJoin", "join"]);

function isObject(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/** Canonical JSON text (sorted keys, compact) — mirrors the scanner. */
function canonicalText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean": return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) return JSON.stringify(null);
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    case "string": return JSON.stringify(value);
    case "object": break;
    default: return JSON.stringify(null);
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalText).join(",")}]`;
  }
  const keys = Object.keys(value).sort((left, right) => {
    const a = Buffer.from(left, "utf8");
    const b = Buffer.from(right, "utf8");
    const length = Math.min(a.length, b.length);
    for (let index = 0; index < length; index += 1) {
      if (a[index] !== b[index]) return a[index] - b[index];
    }
    return a.length - b.length;
  });
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalText(value[key])}`).join(",")}}`;
}

/** Bounded push; flips the passed overflow flag object when exceeded. */
function pushBounded(array, limit, value, overflow) {
  if (array.length >= limit) {
    if (overflow !== undefined) overflow.hit = true;
    return false;
  }
  array.push(value);
  return true;
}

function sortedUnique(values) {
  return [...new Set(values)].sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
}

// ---------------------------------------------------------------------------
// 2. Attachment decision and closure mapping.
// ---------------------------------------------------------------------------

/**
 * Decide whether the embedded drizzle declaration closure may resolve
 * the program's drizzle imports. Policy: every consumer manifest that
 * declares `drizzle-orm` must pin the exact supported release — plain
 * `x.y.z` only. Ranges, missing pins, or mismatched pins leave the
 * declarations unattached so imports degrade to unresolved-import
 * uncertainty instead of acquiring unverified Drizzle identity.
 */
export function resolveDrizzleAttachment(manifest, readBytes, closure) {
  if (!closure) {
    return { attached: false, reason: "declarations-absent" };
  }
  const declared = [];
  let sawAny = false;
  for (const entry of manifest.packageFiles) {
    let document;
    try {
      document = JSON.parse(readBytes(entry.path).toString("utf8"));
    } catch {
      continue;
    }
    if (!isObject(document)) continue;
    for (const section of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) {
      const deps = document[section];
      if (!isObject(deps)) continue;
      const spec = deps["drizzle-orm"];
      if (typeof spec !== "string") continue;
      sawAny = true;
      declared.push(spec);
    }
  }
  if (!sawAny) {
    return { attached: false, reason: "pin-absent" };
  }
  const exact = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;
  const nonExact = declared.filter((spec) => !exact.test(spec));
  if (nonExact.length > 0) {
    return { attached: false, reason: "pin-not-exact", declared: sortedUnique(declared), supported: closure.pin };
  }
  const mismatched = declared.filter((spec) => spec !== closure.pin);
  if (mismatched.length > 0) {
    return { attached: false, reason: "pin-mismatch", declared: sortedUnique(declared), supported: closure.pin };
  }
  return { attached: true, reason: "pin-exact", pin: closure.pin, declared: sortedUnique(declared) };
}

/**
 * The exact compiler `paths` mapping from the supported public subpaths
 * onto the embedded declaration closure. Exact entries only — a
 * wildcard would resolve near-miss subpaths and leak unsupported
 * dialects into the program. Returns null when the closure misses an
 * expected entry file (which must never happen; tests assert it).
 */
export function drizzleClosureMapping(closure) {
  const prefix = `/lekalo/deps/drizzle-orm@${closure.pin}/`;
  const paths = {};
  for (const entry of DRIZZLE_SUPPORTED_SUBPATHS) {
    const specifier = entry === "index.d.ts"
      ? "drizzle-orm"
      : "drizzle-orm/" + entry.replace(/\/index\.d\.ts$/, "").replace(/\.d\.ts$/, "");
    const hostPath = prefix + entry;
    if (!closure.files.has(hostPath)) {
      return null;
    }
    paths[specifier] = [hostPath];
  }
  return paths;
}

// ---------------------------------------------------------------------------
// 3. Small AST helpers.
// ---------------------------------------------------------------------------

/** The 1-based line/column span of one node in its source file. */
function spanOf(sourceFile, node) {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd());
  return {
    start: { line: start.line + 1, column: start.character + 1 },
    end: { line: end.line + 1, column: end.character + 1 },
  };
}

/** The 1-based line of one node start in its source file. */
function lineOf(sourceFile, node) {
  return sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1;
}

/** Statically known string value of one node, or null. */
function staticString(node) {
  if (node === undefined || node === null) return null;
  if (node.kind === 11 /* StringLiteral */ || node.kind === 15 /* NoSubstitutionTemplateLiteral */) {
    return typeof node.text === "string" ? node.text : null;
  }
  return null;
}

/** Statically known primitive (string/number/boolean) of one node. */
function staticPrimitive(node) {
  if (node === undefined || node === null) return null;
  if (node.kind === 11 || node.kind === 15) {
    return { kind: "string", token: node.text };
  }
  if (node.kind === 9 /* NumericLiteral */) {
    return { kind: "number", token: node.getText().slice(0, 64) };
  }
  if (node.kind === 112 /* TrueKeyword */ || node.kind === 97 /* FalseKeyword */) {
    return { kind: "boolean", token: node.kind === 112 ? "true" : "false" };
  }
  if (node.kind === 225 /* PrefixUnaryExpression */ && node.operator === 41 /* MinusToken */) {
    const operand = staticPrimitive(node.operand);
    if (operand?.kind === "number") {
      return { kind: "number", token: `-${operand.token}` };
    }
  }
  return null;
}

/**
 * Flatten a property-access/call chain into its root receiver plus the
 * ordered link list: `db.select().from(users)` → root `db`, links
 * [select(call), from(call)].
 */
function flattenChain(ts, node) {
  const links = [];
  let current = node;
  let guard = 0;
  while (guard++ < 64) {
    if (current === undefined || current === null) break;
    if (current.kind === ts.SyntaxKind.CallExpression) {
      const callee = current.expression;
      if (callee.kind === ts.SyntaxKind.PropertyAccessExpression) {
        links.push({ kind: "call", name: callee.name.getText(), node: current });
        current = callee.expression;
      } else if (callee.kind === ts.SyntaxKind.Identifier) {
        links.push({ kind: "call", name: callee.text, node: current });
        current = callee;
      } else {
        break;
      }
    } else if (current.kind === ts.SyntaxKind.PropertyAccessExpression) {
      links.push({ kind: "access", name: current.name.getText(), node: current });
      current = current.expression;
    } else if (current.kind === ts.SyntaxKind.ParenthesizedExpression
      || current.kind === ts.SyntaxKind.AsExpression
      || current.kind === ts.SyntaxKind.TypeAssertionExpression
      || current.kind === ts.SyntaxKind.NonNullExpression) {
      current = current.expression;
    } else {
      break;
    }
  }
  links.reverse();
  return { root: current, links };
}

/** The VariableDeclaration/PropertyAssignment/BindingElement that declares this symbol. */
function variableDeclarationOf(symbol) {
  for (const declaration of symbol?.declarations ?? []) {
    if (declaration.kind === 261 /* VariableDeclaration */) return declaration;
    if (declaration.kind === 304 /* PropertyAssignment */) return declaration;
    if (declaration.kind === 173 /* PropertyDeclaration */) return declaration;
    // A destructured binding declares its local through a binding
    // element: ignoring it made renamed destructures invisible to both
    // extraction and the unproven-callee net (issue #116 fix round 4).
    if (declaration.kind === 209 /* BindingElement */) return declaration;
  }
  return null;
}

/**
 * Resolve import/export alias symbols to their target. An imported
 * `pgTable` binding is an alias whose declarations are the import
 * specifier; the recognizer needs the resolved Drizzle declaration
 * behind it. Resolution failures return the input unchanged.
 */
function resolveAliasSymbol(checker, symbol) {
  if (symbol === null || symbol === undefined) return symbol;
  if ((symbol.flags & 8388480 /* SymbolFlags.Alias */) === 0) return symbol;
  try {
    return checker.getAliasedSymbol(symbol);
  } catch {
    return symbol;
  }
}

/** Whether any declaration of the symbol lives in the embedded closure. */
function symbolInClosure(symbol, closurePrefix) {
  return (symbol?.declarations ?? [])
    .some((declaration) => declaration.getSourceFile().fileName.startsWith(closurePrefix));
}

/**
 * The construct factory names a callee can be RECOGNIZED by when the
 * checker cannot prove its identity. Recognition by spelling only ever
 * adds an explicit unproven limitation — it never extracts evidence.
 * The sets are disjoint so a call is recognized by exactly one
 * construct walk.
 */
const TABLE_FACTORY_NAMES = new Set([
  "pgTable", "mysqlTable", "sqliteTable", "singlestoreTable",
]);
const RELATIONS_FACTORY_NAMES = new Set(["relations"]);

/** The construct-name spelling of a callee, when one is visible. */
function calleeSpellingName(ts, calleeNode) {
  if (calleeNode?.kind === ts.SyntaxKind.Identifier) return calleeNode.text;
  if (calleeNode?.kind === ts.SyntaxKind.PropertyAccessExpression) {
    return calleeNode.name?.getText() ?? null;
  }
  if (calleeNode?.kind === ts.SyntaxKind.ElementAccessExpression) {
    const argument = calleeNode.argumentExpression;
    return argument?.kind === ts.SyntaxKind.StringLiteral
      || argument?.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral
      ? argument.text
      : null;
  }
  return null;
}

/**
 * The DECLARATION symbol of a callee resolved into the embedded
 * closure — the identity test for factory callees. The local spelling
 * is never identity, and neither is the binding form (issue #116 fix
 * rounds): each of these resolves to the same vendored declaration and
 * must extract exactly like the spelled name —
 *   `import { pgTable as pt } then pt(...)`     (import alias)
 *   `import * as d … then d.relations(…)`       (namespace property)
 *   `const r = relations then r(…)`             (const/let rebinding)
 *   `const { relations: rel2 } = orm then rel2(…)` (renamed destructure)
 * Rebinding follows a bounded chain of variable declarations whose
 * initializer is another reference; anything else (call results,
 * parameters, reassignment through property bags) stays unproven.
 * A BindingElement declaration resolves through the enclosing
 * declaration's initializer: the destructured property name selects
 * the export from the initializer's namespace symbol
 * (issue #116 fix round 4 — previously a renamed destructure escaped
 * both extraction and the unproven flag and was dropped silently).
 * Returns null when the callee does not resolve into the closure.
 * When `probe` is given, `probe.boundHit` marks a resolution that died
 * at the MAX_ALIAS_HOPS bound — recognized, but unprovable within the
 * bound; callers must keep that explicit, never silent.
 */
function closureCalleeSymbol(context, calleeNode, depth = 0, probe = null) {
  let symbol;
  try {
    symbol = context.checker.getSymbolAtLocation(calleeNode);
  } catch {
    return null;
  }
  symbol = resolveAliasSymbol(context.checker, symbol);
  if (symbolInClosure(symbol, context.closurePrefix)) return symbol;
  if (depth >= MAX_ALIAS_HOPS) {
    if (probe !== null) probe.boundHit = true;
    return null;
  }
  const declaration = variableDeclarationOf(symbol);
  if (declaration === null || declaration === undefined) return null;
  const ts = context.ts;
  if (declaration.kind === ts.SyntaxKind.VariableDeclaration) {
    const initializer = declaration.initializer;
    if (initializer?.kind !== ts.SyntaxKind.Identifier
      && initializer?.kind !== ts.SyntaxKind.PropertyAccessExpression) {
      return null;
    }
    return closureCalleeSymbol(context, initializer, depth + 1);
  }
  // Renamed destructuring: `const { relations: rel2 } = orm` (or the
  // shorthand). The local spelling is not identity — the destructured
  // property name selects the export.
  if (declaration.kind === ts.SyntaxKind.BindingElement) {
    const propertyName = declaration.propertyName ?? declaration.name;
    const nameIsSpellable = propertyName?.kind === ts.SyntaxKind.Identifier
      || propertyName?.kind === ts.SyntaxKind.StringLiteral
      || propertyName?.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral;
    if (!nameIsSpellable) return null;
    if (declaration.parent?.kind !== ts.SyntaxKind.ObjectBindingPattern) return null;
    const owner = declaration.parent.parent;
    if (owner?.kind !== ts.SyntaxKind.VariableDeclaration) return null;
    const initializer = owner.initializer;
    if (initializer === undefined || initializer === null) return null;
    let namespaceSymbol;
    try {
      namespaceSymbol = context.checker.getSymbolAtLocation(initializer);
    } catch {
      return null;
    }
    namespaceSymbol = resolveAliasSymbol(context.checker, namespaceSymbol);
    if (namespaceSymbol === null || namespaceSymbol === undefined) return null;
    // Module export surfaces flatten star re-exports only through the
    // checker (`export * from "./relations.js"` never lands in the
    // raw `.exports` table), so resolve through getExportsOfModule and
    // fall back to the direct table (issue #116 fix round 4).
    let target = null;
    try {
      const exports = context.checker.getExportsOfModule(namespaceSymbol)
        ?? context.checker.getExportsOfSymbol(namespaceSymbol) ?? [];
      target = exports.find((entry) => entry?.getName() === propertyName.text) ?? null;
    } catch {
      target = namespaceSymbol.exports?.get(propertyName.text) ?? null;
    }
    if (target !== null && target !== undefined
      && symbolInClosure(target, context.closurePrefix)) return target;
    return null;
  }
  return null;
}

/**
 * Whether a call is RECOGNIZABLE as one of the given Drizzle constructs
 * even though its callee could not be proven against the closure: the
 * callee spelling names a construct factory while the checker refuses
 * identity (an indirect wrapper, an unresolved re-export, an element
 * access). Such a construct is never dropped silently — it is recorded
 * with an explicit `callee-unproven` limitation and the section gap
 * (issue #116 fix round 3, the same honesty contract as the out-of-
 * subset surfaces). Proven callees never enter this path.
 */
function calleeRecognizedButUnproven(context, calleeNode, constructs, family) {
  const probe = { boundHit: false };
  if (closureCalleeSymbol(context, calleeNode, 0, probe) !== null) return false;
  const closureTypeNames = closureTypeSymbolNames(context, calleeNode);
  if (probe.boundHit) {
    // The bound is family-disjoint like every other recognition: the
    // construct is recognized only when the callee's vendored type
    // names THIS construct family (issue #116 fix round 4).
    return closureTypeNames !== null
      && closureTypeNames.some((name) => RECOGNITION_FAMILIES[family].has(name));
  }
  return calleeSpellingNames(context, calleeNode)
    .some((spelling) => constructs.has(spelling));
}

/** Recognition family name → the factory-name set that recognizes it. */
const RECOGNITION_FAMILIES = { table: TABLE_FACTORY_NAMES, relations: RELATIONS_FACTORY_NAMES };

/**
 * The names of the callee's type symbols whose declarations live in
 * the embedded closure: non-null and empty means the type provably
 * resolved entirely OUTSIDE the closure (foreign); null means the type
 * was unresolvable (any/error) and nothing is provable either way.
 */
function closureTypeSymbolNames(context, calleeNode) {
  const { ts, checker } = context;
  let type;
  try {
    type = checker.getTypeAtLocation(calleeNode);
  } catch {
    return null;
  }
  if (type === null || type === undefined) return null;
  const queue = [];
  if (type.symbol) queue.push(type.symbol);
  if (type.aliasSymbol) queue.push(type.aliasSymbol);
  if (typeof type.isUnion === "function" && type.isUnion()) {
    for (const part of type.types) {
      if (part.symbol) queue.push(part.symbol);
      if (part.aliasSymbol) queue.push(part.aliasSymbol);
    }
  }
  if (queue.length === 0) return null;
  const closureNames = [];
  for (const symbol of queue) {
    if (symbolInClosure(symbol, context.closurePrefix)) closureNames.push(symbol.getName());
  }
  return closureNames;
}

/**
 * Every construct spelling a callee is RECOGNIZABLE by: the direct
 * call spelling, plus the destructured property name behind a binding
 * element — `rel2` in `const { relations: rel2 } = orm` spells `rel2`
 * locally, but the construct it destructures is spelled `relations`
 * (issue #116 fix round 4).
 */
function calleeSpellingNames(context, calleeNode) {
  const ts = context.ts;
  const spellings = [];
  const direct = calleeSpellingName(ts, calleeNode);
  if (direct !== null) spellings.push(direct);
  let symbol;
  try {
    symbol = context.checker.getSymbolAtLocation(calleeNode);
  } catch {
    symbol = null;
  }
  for (const declaration of symbol?.declarations ?? []) {
    if (declaration.kind !== ts.SyntaxKind.BindingElement) continue;
    if (declaration.parent?.kind !== ts.SyntaxKind.ObjectBindingPattern) continue;
    const propertyName = declaration.propertyName ?? declaration.name;
    const spellable = propertyName?.kind === ts.SyntaxKind.Identifier
      || propertyName?.kind === ts.SyntaxKind.StringLiteral
      || propertyName?.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral;
    if (spellable) spellings.push(propertyName.text);
  }
  return spellings;
}

/** The first closure declaration path of the symbol (provenance). */
function closureDeclarationPath(symbol, closurePrefix) {
  for (const declaration of symbol?.declarations ?? []) {
    const fileName = declaration.getSourceFile().fileName;
    if (fileName.startsWith(closurePrefix)) {
      return fileName.slice(closurePrefix.length);
    }
  }
  return null;
}

/**
 * Whether the checker type of one node resolves (through its type
 * symbol, alias symbol, or one base-type hop) into the embedded closure
 * — the receiver identity test for database/transaction objects.
 */
function typeInClosure(ts, checker, node, closurePrefix) {
  let type;
  try {
    type = checker.getTypeAtLocation(node);
  } catch {
    return false;
  }
  const seen = new Set();
  const queue = [];
  if (type.symbol) queue.push(type.symbol);
  if (type.aliasSymbol) queue.push(type.aliasSymbol);
  if (typeof type.isUnion === "function" && type.isUnion()) {
    for (const part of type.types) {
      if (part.symbol) queue.push(part.symbol);
      if (part.aliasSymbol) queue.push(part.aliasSymbol);
    }
  }
  while (queue.length > 0) {
    const symbol = queue.shift();
    const declarations = symbol.declarations ?? [];
    const key = `${symbol.getName()}@${declarations[0]
      ? declarations[0].getSourceFile().fileName
      : "?"}`;
    if (seen.has(key)) continue;
    seen.add(key);
    if (symbolInClosure(symbol, closurePrefix)) return true;
    for (const declaration of declarations) {
      if (declaration.heritageClauses === undefined) continue;
      for (const clause of declaration.heritageClauses) {
        for (const heritageType of clause.types) {
          try {
            const base = checker.getTypeAtLocation(heritageType);
            if (base.symbol) queue.push(base.symbol);
            if (base.aliasSymbol) queue.push(base.aliasSymbol);
          } catch {
            // unresolved heritage: skip, never guess
          }
        }
      }
    }
  }
  return false;
}

/**
 * Whether the symbol is a module namespace OBJECT (`import * as ns` or
 * `export * as ns`): its declarations are the module's source file (or
 * the namespace import specifier), never a runtime handle class. A
 * db-spelled binding that resolves to a namespace is provably not a
 * database handle, so typing it as one would fabricate evidence
 * (issue #116 fix round 4 — `orm.select()` on the drizzle-orm
 * namespace is not a query on a client).
 */
function isModuleNamespaceSymbol(ts, checker, symbol, resolved) {
  const namespaceDeclared = (candidate) => (candidate?.declarations ?? [])
    .some((declaration) => declaration.kind === ts.SyntaxKind.SourceFile
      || declaration.kind === ts.SyntaxKind.NamespaceImport
      || declaration.kind === ts.SyntaxKind.ExportSpecifier);
  if (namespaceDeclared(symbol)) return true;
  if (resolved === symbol || resolved === null || resolved === undefined) return false;
  return namespaceDeclared(resolved);
}

function symbolKey(symbol) {
  if (!symbol) return "null";
  const first = (symbol.declarations ?? [])[0];
  const file = first ? first.getSourceFile().fileName : "?";
  const pos = first ? String(first.getStart()) : "-1";
  return `${symbol.getName()}@${file}#${pos}`;
}

// ---------------------------------------------------------------------------
// 4. The extraction context.
// ---------------------------------------------------------------------------

class ExtractorContext {
  constructor({ ts, checker, closurePrefix, limitations }) {
    this.ts = ts;
    this.checker = checker;
    this.closurePrefix = closurePrefix;
    this.limitations = limitations;
    this.overflow = { hit: false };
    /** variable symbol key → table row */
    this.tableBySymbol = new Map();
    /** export name → table row */
    this.tableByExport = new Map();
    /** physical name → [table rows] */
    this.tableByPhysical = new Map();
    this.tables = [];
    this.relations = [];
    this.queries = [];
    this.transactions = [];
    this.migrations = [];
    /** Sections with recognized-but-uncovered Drizzle surfaces — a
     * section may claim complete only with no gaps (issue #116 fix
     * round: a silent drop can never hide behind a zero-row section). */
    this.sectionGaps = new Set();
    this.contributingFiles = new Map();
    this.fileDigestCache = new Map();
  }

  limit(code, modulePath, line, detail) {
    if (!LIMITATION_CODES.has(code)) {
      throw new Error(`unknown limitation code: ${code}`);
    }
    pushBounded(this.limitations, MAX_LIMITATIONS, {
      code,
      module: modulePath ?? null,
      line: typeof line === "number" && Number.isInteger(line) && line > 0 && line < 1000000 ? line : null,
      detail: typeof detail === "string" ? detail.slice(0, 160) : null,
    }, this.overflow);
  }

  noteFile(modulePath, digest) {
    if (this.contributingFiles.size < MAX_PROVENANCE_FILES) {
      this.contributingFiles.set(modulePath, digest);
    }
  }

  tableForNodeIdentifier(node) {
    if (node === undefined || node === null) return null;
    if (node.kind !== this.ts.SyntaxKind.Identifier) return null;
    let symbol;
    try {
      symbol = this.checker.getSymbolAtLocation(node);
    } catch {
      return null;
    }
    const resolved = resolveAliasSymbol(this.checker, symbol);
    return this.tableBySymbol.get(symbolKey(symbol))
      ?? this.tableBySymbol.get(symbolKey(resolved))
      ?? null;
  }
}

/** The stable native identity of one extracted table. */
function tableNativeId(modulePath, exportName, physicalName) {
  return "sha256:" + sha256Hex(canonicalText({
    domain: TABLE_IDENTITY_DOMAIN,
    module: modulePath,
    export: exportName,
    table: physicalName,
  }));
}

// ---------------------------------------------------------------------------
// 5. Schema extraction (tables, columns, constraints).
// ---------------------------------------------------------------------------

/**
 * The bounded column type token of one builder call. The token mirrors
 * the drizzle-level spelling (varchar(36), int, datetime(6));
 * divergences from physical storage stay dialect findings — TS type
 * equivalence never proves storage equivalence.
 */
function columnTypeToken(context, factory, callNode) {
  const args = callNode?.arguments ?? [];
  const notes = [];
  let token = factory;
  if (factory === "varchar" || factory === "char" || factory === "varbinary") {
    const options = args[1];
    const length = options !== undefined && options.kind === context.ts.SyntaxKind.ObjectLiteralExpression
      ? propertyScalar(context, options, "length")
      : null;
    token = length !== null ? `${factory}(${length})` : factory;
  } else if (factory === "numeric" || factory === "decimal") {
    const options = args[1];
    if (options !== undefined && options.kind === context.ts.SyntaxKind.ObjectLiteralExpression) {
      const precision = propertyScalar(context, options, "precision");
      const scale = propertyScalar(context, options, "scale");
      token = precision !== null ? `${factory}(${precision}${scale !== null ? `,${scale}` : ""})` : factory;
    }
  } else if (factory === "datetime" || factory === "timestamp" || factory === "time") {
    // Options may sit at args[0] (nameless form) or args[1] (named).
    const options = args[0] !== undefined && args[0].kind === context.ts.SyntaxKind.ObjectLiteralExpression
      ? args[0]
      : args[1];
    if (options !== undefined && options.kind === context.ts.SyntaxKind.ObjectLiteralExpression) {
      const precision = propertyScalar(context, options, "precision");
      if (precision !== null) token = `${factory}(${precision})`;
    }
  } else if (factory === "enum") {
    const values = args[1];
    if (values !== undefined && values.kind === context.ts.SyntaxKind.ArrayLiteralExpression) {
      token = `enum(${values.elements.length})`;
    }
  } else if (factory === "bigint" || factory === "int" || factory === "tinyint" || factory === "mediumint") {
    const options = args[0];
    if (options !== undefined && options.kind === context.ts.SyntaxKind.ObjectLiteralExpression
      && propertyIsTrue(context, options, "unsigned")) {
      notes.push("unsigned");
    }
  }
  const KNOWN = new Set(["boolean", "text", "varchar", "char", "integer", "int", "bigint",
    "smallint", "tinyint", "mediumint", "numeric", "decimal", "real", "float", "double",
    "doublePrecision", "json", "jsonb", "uuid", "date", "time", "datetime", "timestamp",
    "serial", "bigserial", "smallserial", "year", "binary", "varbinary", "enum"]);
  if (!KNOWN.has(factory)) {
    context.limit("column-type-unknown", null, null, factory.slice(0, 64));
    notes.push("type-shape-unknown");
  }
  return { token, notes };
}

function propertyScalar(context, objectNode, name) {
  for (const property of objectNode.properties) {
    if (property.kind !== context.ts.SyntaxKind.PropertyAssignment) continue;
    if (property.name?.getText() !== name) continue;
    const value = staticPrimitive(property.initializer);
    return value ? value.token : null;
  }
  return null;
}

function propertyIsTrue(context, objectNode, name) {
  for (const property of objectNode.properties) {
    if (property.kind !== context.ts.SyntaxKind.PropertyAssignment) continue;
    if (property.name?.getText() !== name) continue;
    return property.initializer?.kind === context.ts.SyntaxKind.TrueKeyword;
  }
  return false;
}

function propertyNode(context, objectNode, name) {
  if (objectNode === undefined || objectNode === null
    || objectNode.properties === undefined) {
    return null;
  }
  for (const property of objectNode.properties) {
    if (property.kind !== context.ts.SyntaxKind.PropertyAssignment) continue;
    if (property.name?.getText() !== name) continue;
    return property.initializer;
  }
  return null;
}

/**
 * The object-literal (or `(helpers) => ({...})` callback) column map of
 * a table constructor call. Returns the object literal node or null.
 */
function columnsObjectOf(ts, node) {
  if (node === undefined || node === null) return null;
  if (node.kind === ts.SyntaxKind.ObjectLiteralExpression) return node;
  if (node.kind === ts.SyntaxKind.ArrowFunction
    || node.kind === ts.SyntaxKind.FunctionExpression) {
    const body = node.body;
    if (body?.kind === ts.SyntaxKind.ObjectLiteralExpression) return body;
    if (body?.kind === ts.SyntaxKind.ParenthesizedExpression
      && body.expression.kind === ts.SyntaxKind.ObjectLiteralExpression) {
      return body.expression;
    }
    if (body?.kind === ts.SyntaxKind.Block) {
      for (const statement of body.statements) {
        if (statement.kind === ts.SyntaxKind.ReturnStatement && statement.expression) {
          const returned = statement.expression;
          if (returned.kind === ts.SyntaxKind.ObjectLiteralExpression) return returned;
          if (returned.kind === ts.SyntaxKind.ParenthesizedExpression
            && returned.expression.kind === ts.SyntaxKind.ObjectLiteralExpression) {
            return returned.expression;
          }
        }
      }
    }
  }
  return null;
}

/**
 * The extra-config array (or deprecated object) of the third table
 * constructor argument.
 */
function extraConfigElementsOf(ts, node) {
  if (node === undefined || node === null) return [];
  let container = null;
  if (node.kind === ts.SyntaxKind.ArrowFunction || node.kind === ts.SyntaxKind.FunctionExpression) {
    const body = node.body;
    if (body?.kind === ts.SyntaxKind.ArrayLiteralExpression) container = body;
    else if (body?.kind === ts.SyntaxKind.ObjectLiteralExpression) container = body;
    else if (body?.kind === ts.SyntaxKind.Block) {
      for (const statement of body.statements) {
        if (statement.kind === ts.SyntaxKind.ReturnStatement && statement.expression) {
          if (statement.expression.kind === ts.SyntaxKind.ArrayLiteralExpression
            || statement.expression.kind === ts.SyntaxKind.ObjectLiteralExpression) {
            container = statement.expression;
          }
        }
      }
    }
  } else if (node.kind === ts.SyntaxKind.ArrayLiteralExpression
    || node.kind === ts.SyntaxKind.ObjectLiteralExpression) {
    container = node;
  }
  if (container === null) return [];
  return container.kind === ts.SyntaxKind.ArrayLiteralExpression
    ? [...container.elements]
    : [...container.properties];
}

function isSqlTaggedTemplate(ts, node) {
  if (node === undefined || node === null) return false;
  if (node.kind === ts.SyntaxKind.TaggedTemplateExpression) return true;
  return node.kind === ts.SyntaxKind.CallExpression
    && node.expression?.kind === ts.SyntaxKind.PropertyAccessExpression
    && node.expression.name?.text === "raw";
}

function neutralKindOf(factory) {
  const stringTypes = new Set(["varchar", "char", "text", "uuid", "varbinary", "binary"]);
  const numberTypes = new Set(["int", "integer", "bigint", "smallint", "tinyint", "mediumint",
    "numeric", "decimal", "real", "float", "double", "doublePrecision", "serial",
    "bigserial", "smallserial", "year"]);
  const datetimeTypes = new Set(["date", "time", "datetime", "timestamp"]);
  const jsonTypes = new Set(["json", "jsonb"]);
  if (stringTypes.has(factory)) return "string";
  if (numberTypes.has(factory)) return "number";
  if (datetimeTypes.has(factory)) return "datetime";
  if (jsonTypes.has(factory)) return "json";
  if (factory === "boolean") return "boolean";
  if (factory === "enum") return "enum";
  return null;
}

/**
 * Walk one column builder chain and fold the decidable facts. The
 * chain looks like `varchar("id", {length: 36}).primaryKey()...`.
 */
function extractColumnChain(context, sourceFile, tsName, callNode, dialect) {
  const { ts } = context;
  const column = {
    tsName,
    physicalName: tsName,
    physicalNameExplicit: false,
    typeToken: null,
    neutralKind: null,
    notNull: false,
    primaryKey: false,
    unique: false,
    autoincrement: false,
    identity: null,
    default: null,
    references: null,
    span: spanOf(sourceFile, callNode),
    limitations: [],
  };
  const { links } = flattenChain(ts, callNode);
  if (links.length === 0 || links[0].kind !== "call") {
    column.limitations.push("column-shape-unknown");
    return column;
  }
  const builder = links[0];
  const shaped = columnTypeToken(context, builder.name, builder.node);
  column.typeToken = shaped.token;
  column.neutralKind = neutralKindOf(builder.name);
  // MySQL `serial` is alias sugar (bigint unsigned autoincrement
  // unique), never a storage type — the divergence is recorded, not
  // folded away.
  if (dialect === "mysql" && builder.name === "serial") {
    column.limitations.push("mysql-serial-alias");
  }
  // First argument may carry the explicit physical name.
  const firstArg = builder.node.arguments?.[0];
  const explicit = staticString(firstArg);
  if (explicit !== null) {
    column.physicalName = explicit;
    column.physicalNameExplicit = true;
  } else if (firstArg === undefined) {
    // Name-implied form (`integer()` with no physical name) is fine.
  } else if (!firstArgGetSelfColumn(firstArg, ts)) {
    column.limitations.push("column-name-dynamic");
  }
  for (const link of links.slice(1)) {
    if (link.kind !== "call") continue;
    switch (link.name) {
      case "primaryKey": column.primaryKey = true; break;
      case "notNull": column.notNull = true; break;
      case "unique": column.unique = true; break;
      case "autoincrement": column.autoincrement = true; break;
      case "default": {
        const arg = link.node.arguments?.[0];
        const primitive = staticPrimitive(arg);
        if (primitive !== null) {
          column.default = { kind: "literal", token: primitive.token };
        } else if (isSqlTaggedTemplate(ts, arg)) {
          column.default = { kind: "sql", token: null };
          context.limit("sql-default", null, null, tsName);
        } else if (arg !== undefined) {
          column.default = { kind: "expression", token: null };
          context.limit("sql-default", null, null, tsName);
        }
        break;
      }
      case "defaultNow": case "defaultRandom": case "defaultNowOnUpdate":
        column.default = { kind: "function", token: link.name };
        break;
      case "$defaultFn": case "$default":
        column.default = { kind: "client-fn", token: null };
        context.limit("client-hook", null, null, tsName);
        break;
      case "$onUpdate":
        column.default = column.default ?? { kind: "client-hook", token: null };
        context.limit("client-hook", null, null, tsName);
        break;
      case "generatedAlwaysAsIdentity":
      case "generatedByDefaultAsIdentity":
        column.identity = link.name === "generatedAlwaysAsIdentity" ? "always" : "byDefault";
        break;
      case "references": {
        const resolved = resolveReferencesTarget(context, link.node.arguments);
        if (resolved === null) {
          column.limitations.push("constraint-member-unresolved");
        } else {
          column.references = resolved;
        }
        break;
      }
      default:
        break;
    }
  }
  // Key-implied nullability: a primary key column is not nullable
  // unless explicitly declared otherwise. A declared `.notNull()` wins.
  if (column.primaryKey) column.notNull = true;
  return column;
}

function firstArgGetSelfColumn(node, ts) {
  return node?.kind === ts.SyntaxKind.PropertyAccessExpression;
}

/**
 * Resolve `.references(() => other.column, config)` to the referenced
 * table, column, and declared actions.
 */
function resolveReferencesTarget(context, args) {
  const { ts } = context;
  const arg = args?.[0];
  if (arg === undefined || arg === null) return null;
  let accessor = arg;
  if (arg.kind === ts.SyntaxKind.ArrowFunction || arg.kind === ts.SyntaxKind.FunctionExpression) {
    accessor = arg.body;
  } else if (arg.kind === ts.SyntaxKind.ParenthesizedExpression) {
    accessor = arg.expression;
  }
  if (accessor?.kind !== ts.SyntaxKind.PropertyAccessExpression) return null;
  const tableRow = context.tableForNodeIdentifier(accessor.expression);
  const columnName = accessor.name.getText();
  if (tableRow === null) return null;
  const config = args?.[1];
  const onDelete = config !== undefined && config.kind === ts.SyntaxKind.ObjectLiteralExpression
    ? staticString(propertyNode(context, config, "onDelete"))
    : null;
  const onUpdate = config !== undefined && config.kind === ts.SyntaxKind.ObjectLiteralExpression
    ? staticString(propertyNode(context, config, "onUpdate"))
    : null;
  const referenced = tableRow.columns.find((column) => column.tsName === columnName) ?? null;
  return {
    table: tableRow.physicalName,
    tableNative: tableRow.native,
    column: referenced ? referenced.physicalName : columnName,
    columnResolved: referenced !== null,
    onDelete,
    onUpdate,
  };
}

/**
 * Extract one table declaration. Returns the table row (also pushed to
 * the context) or null when the constructor is not a recognized
 * dialect factory.
 */
function extractTable(context, sourceFile, node, exportName) {
  const { ts } = context;
  const calleeNode = node.expression;
  if (calleeNode.kind !== ts.SyntaxKind.Identifier
    && calleeNode.kind !== ts.SyntaxKind.PropertyAccessExpression) {
    return null;
  }
  // Factory identity comes from the resolved declaration symbol, never
  // from the local callee spelling — an aliased `pgTable as pt` is the
  // same vendored declaration (issue #116 fix round).
  const symbol = closureCalleeSymbol(context, calleeNode);
  if (symbol === null) return null;
  const factoryName = symbol.getName();
  const dialect = factoryName === "pgTable" ? "postgresql"
    : factoryName === "mysqlTable" ? "mysql"
    : null;
  const modulePath = context.modulePathOf(sourceFile);
  if (dialect === null) {
    if (factoryName === "sqliteTable" || factoryName === "singlestoreTable") {
      context.limit("unsupported-dialect", modulePath, lineOf(sourceFile, node), factoryName);
      context.sectionGaps.add("tables");
    }
    return null;
  }
  const physicalName = staticString(node.arguments?.[0]);
  const row = {
    native: tableNativeId(modulePath, exportName, physicalName ?? `#${node.getStart(sourceFile)}`),
    module: modulePath,
    exportName,
    declarationPath: closureDeclarationPath(symbol, context.closurePrefix),
    physicalName: physicalName ?? null,
    dialect,
    span: spanOf(sourceFile, node),
    fileDigest: context.fileDigestOf(sourceFile),
    columns: [],
    constraints: [],
    joinTableCandidate: false,
    tenantKey: null,
    limitations: [],
    completeness: "complete",
  };
  if (physicalName === null) {
    context.limit("table-name-dynamic", modulePath, lineOf(sourceFile, node));
    row.limitations.push("table-name-dynamic");
    row.completeness = "partial";
  }
  const columnsNode = columnsObjectOf(ts, node.arguments?.[1]);
  if (columnsNode !== null) {
    const columnOverflow = { hit: false };
    for (const property of columnsNode.properties) {
      if (property.kind !== ts.SyntaxKind.PropertyAssignment
        && property.kind !== ts.SyntaxKind.ShorthandPropertyAssignment) {
        continue;
      }
      const tsName = property.name?.getText();
      if (typeof tsName !== "string" || tsName.length === 0) continue;
      const initializer = property.kind === ts.SyntaxKind.PropertyAssignment
        ? property.initializer
        : property.name;
      if (initializer === undefined || initializer.kind !== ts.SyntaxKind.CallExpression) {
        context.limit("column-type-unknown", modulePath, lineOf(sourceFile, property), tsName);
        row.limitations.push("column-type-unknown");
        continue;
      }
      const column = extractColumnChain(context, sourceFile, tsName, initializer, dialect);
      if (!pushBounded(row.columns, MAX_COLUMNS, column, columnOverflow)) break;
      if (column.limitations.includes("column-name-dynamic")) {
        row.limitations.push("column-name-dynamic");
      }
    }
    if (columnOverflow.hit) {
      context.limit("truncated", modulePath, lineOf(sourceFile, node), "columns");
      row.limitations.push("truncated-columns");
      row.completeness = "partial";
    }
  } else {
    context.limit("constraint-shape-unknown", modulePath, lineOf(sourceFile, node), "columns-object");
    row.limitations.push("columns-object-unknown");
    row.completeness = "partial";
  }
  // Extra config: indexes, unique constraints, composite keys, FKs.
  const extra = node.arguments?.[2];
  if (extra !== undefined) {
    const constraintOverflow = { hit: false };
    for (const element of extraConfigElementsOf(ts, extra)) {
      if (element.kind !== ts.SyntaxKind.CallExpression) continue;
      const constraint = extractConstraint(context, sourceFile, element, row);
      if (constraint !== null) {
        if (!pushBounded(row.constraints, MAX_CONSTRAINTS, constraint, constraintOverflow)) break;
      }
    }
    if (constraintOverflow.hit) {
      context.limit("truncated", modulePath, lineOf(sourceFile, node), "constraints");
      row.limitations.push("truncated-constraints");
      row.completeness = "partial";
    }
  }
  // Join-table candidate: every column is a foreign key column and the
  // shape is the composite two-table pair. Evidence only — never a
  // declared join table.
  const fkColumns = row.columns.filter((column) => column.references !== null);
  if (row.columns.length >= 2 && fkColumns.length === row.columns.length
    && new Set(fkColumns.map((column) => column.references?.table)).size === 2) {
    row.joinTableCandidate = true;
  }
  // Scope candidate: a tenant-like column exists (declaration-only —
  // column existence alone is never scoping evidence).
  const scopeColumn = row.columns.find((column) => SCOPE_KEY_NAMES.has(column.tsName)
    || SCOPE_KEY_NAMES.has(column.physicalName));
  if (scopeColumn !== undefined) {
    row.tenantKey = { column: scopeColumn.tsName, evidence: "declaration-only" };
  }
  return row;
}

const CONSTRAINT_NAMES = new Set([
  "index", "uniqueIndex", "primaryKey", "foreignKey", "check", "unique",
  "fulltextIndex", "spatialIndex",
]);

/**
 * One extra-config constraint builder call with `.on(...)` members,
 * spans, and dialect notes.
 */
function extractConstraint(context, sourceFile, node, tableRow) {
  const { ts } = context;
  if (node.kind !== ts.SyntaxKind.CallExpression) return null;
  // The chain's FIRST call is the constraint builder; trailing calls
  // are `.on(...)`, `.where(...)`, `.references(...)`, and actions.
  const chain = flattenChain(ts, node);
  const firstCall = chain.links.find((link) => link.kind === "call");
  const name = firstCall?.name ?? null;
  if (name === null || !CONSTRAINT_NAMES.has(name)) return null;
  const kind = name === "uniqueIndex" ? "unique-index"
    : name === "index" ? "index"
    : name === "primaryKey" ? "primary-key"
    : name === "unique" ? "unique"
    : name === "fulltextIndex" ? "fulltext-index"
    : name === "spatialIndex" ? "spatial-index"
    : name === "foreignKey" ? "foreign-key"
    : "check";
  const constraintName = staticString(firstCall.node.arguments?.[0]);
  const first = firstCall.node.arguments?.[0];
  // Object-form builders (`primaryKey({ name, columns })`) carry their
  // members in the object, not in a chained `.on(...)`.
  const objectColumns = first !== undefined && first.kind === ts.SyntaxKind.ObjectLiteralExpression
    ? propertyNode(context, first, "columns")
    : null;
  const objectName = first !== undefined && first.kind === ts.SyntaxKind.ObjectLiteralExpression
    ? staticString(propertyNode(context, first, "name"))
    : null;
  const members = [];
  const memberOverflow = { hit: false };
  const onCall = [...chain.links].reverse().find((link) => link.kind === "call" && link.name === "on");
  if (onCall !== undefined) {
    for (const arg of onCall.node.arguments ?? []) {
      const member = constraintMemberOf(context, arg, tableRow);
      if (member !== null) {
        if (!pushBounded(members, MAX_COLUMNS, member, memberOverflow)) break;
      }
    }
  } else if (objectColumns?.kind === ts.SyntaxKind.ArrayLiteralExpression) {
    for (const element of objectColumns.elements) {
      const member = constraintMemberOf(context, element, tableRow);
      if (member !== null) {
        if (!pushBounded(members, MAX_COLUMNS, member, memberOverflow)) break;
      }
    }
  }
  const referencesCall = [...chain.links].reverse()
    .find((link) => link.kind === "call" && link.name === "references");
  const referenced = referencesCall !== undefined
    ? resolveReferencesTarget(context, referencesCall.node.arguments)
    : null;
  const actions = {};
  for (const link of chain.links) {
    if (link.kind === "call" && (link.name === "onDelete" || link.name === "onUpdate")) {
      const value = staticString(link.node.arguments?.[0]);
      if (value !== null) actions[link.name] = value;
    }
  }
  const whereCall = [...chain.links].reverse().find((link) => link.kind === "call" && link.name === "where");
  const limitations = [];
  // A `check(name, sql`...`)` body is raw SQL by construction: the
  // expression is recorded as an explicit unknown, never silently
  // absent (issue #116 fix round).
  const checkBody = kind === "check" ? firstCall.node.arguments?.[1] : undefined;
  const sqlPredicate = kind === "check"
    && (whereCall !== undefined || checkBody !== undefined);
  if (sqlPredicate) {
    context.limit("raw-sql", tableRow.module, lineOf(sourceFile, node), constraintName ?? "check");
    limitations.push("raw-sql");
  }
  if (members.length === 0 && kind !== "check") {
    context.limit("constraint-member-unresolved", tableRow.module, lineOf(sourceFile, node), kind);
    limitations.push("constraint-member-unresolved");
  }
  if (kind === "index" && whereCall !== undefined) {
    limitations.push("constraint-predicate-index");
  }
  return {
    kind,
    name: constraintName ?? objectName ?? null,
    nameExplicit: constraintName !== null || objectName !== null,
    unique: kind === "unique-index" || kind === "unique",
    members: members.map((member) => member.physicalName),
    membersResolved: members.map((member) => member.resolved),
    referenced,
    actions: Object.keys(actions).length > 0 ? actions : null,
    sqlPredicate,
    span: spanOf(sourceFile, node),
    limitations,
  };
}

/** One `.on(...)` member: the callback param's column property access. */
function constraintMemberOf(context, node, tableRow) {
  const { ts } = context;
  let accessor = node;
  if (node?.kind === ts.SyntaxKind.ArrowFunction) accessor = node.body;
  if (accessor?.kind !== ts.SyntaxKind.PropertyAccessExpression) return null;
  const tsName = accessor.name.getText();
  const column = tableRow.columns.find((candidate) => candidate.tsName === tsName);
  return {
    physicalName: column ? column.physicalName : tsName,
    resolved: column !== null,
  };
}

// ---------------------------------------------------------------------------
// 6. Relations extraction.
// ---------------------------------------------------------------------------

/**
 * Extract one `relations(table, ({one, many}) => ({...}))` declaration.
 * Soft application-level relations are recorded as evidence with their
 * own limitations: they never prove database enforcement, required
 * participation, or complete neutral relation fields.
 */
function extractRelations(context, sourceFile, node) {
  const { ts, checker } = context;
  // The same identity test the walker used: import aliases, namespace
  // property access, and const/let rebinding all resolve (issue #116
  // fix round 3); anything else was never a recognized surface.
  if (closureCalleeSymbol(context, node.expression) === null) return null;
  // From here the surface is a RECOGNIZED relations declaration: any
  // failure to extract it is a coverage gap for the relations section,
  // never a silent drop (issue #116 fix round). Each failure path
  // below records the gap before returning.
  const modulePath = context.modulePathOf(sourceFile);
  const sourceTableRow = context.tableForNodeIdentifier(node.arguments?.[0]);
  const callback = node.arguments?.[1];
  if (sourceTableRow === null) {
    context.sectionGaps.add("relations");
    context.limit("relation-target-unresolved", modulePath, lineOf(sourceFile, node), "source-table");
    return null;
  }
  if (callback === undefined
    || (callback.kind !== ts.SyntaxKind.ArrowFunction && callback.kind !== ts.SyntaxKind.FunctionExpression)) {
    context.sectionGaps.add("relations");
    context.limit("relation-policy-missing", modulePath, lineOf(sourceFile, node), "callback");
    return null;
  }
  const param = callback.parameters[0];
  const helperNames = { one: "one", many: "many" };
  if (param?.name?.kind === ts.SyntaxKind.ObjectBindingPattern) {
    for (const element of param.name.elements) {
      if (element.kind !== ts.SyntaxKind.BindingElement) continue;
      const propertyName = element.propertyName?.getText();
      const bound = element.name?.getText();
      if ((propertyName === "one" || propertyName === "many") && bound !== undefined) {
        helperNames[propertyName] = bound;
      }
    }
  }
  let body = callback.body;
  if (body?.kind === ts.SyntaxKind.ParenthesizedExpression) {
    body = body.expression;
  }
  if (body?.kind !== ts.SyntaxKind.ObjectLiteralExpression) {
    context.sectionGaps.add("relations");
    context.limit("relation-policy-missing", modulePath, lineOf(sourceFile, node), "config-object");
    return null;
  }
  const rows = [];
  for (const property of body.properties) {
    if (property.kind !== ts.SyntaxKind.PropertyAssignment
      && property.kind !== ts.SyntaxKind.ShorthandPropertyAssignment) continue;
    const relationName = property.name?.getText();
    const initializer = property.kind === ts.SyntaxKind.PropertyAssignment
      ? property.initializer
      : property.name;
    if (initializer?.kind !== ts.SyntaxKind.CallExpression) continue;
    const callName = initializer.expression?.getText();
    const kind = callName === helperNames.one ? "one"
      : callName === helperNames.many ? "many"
      : null;
    if (kind === null) continue;
    const targetRow = context.tableForNodeIdentifier(initializer.arguments?.[0]);
    const config = initializer.arguments?.[1];
    const fields = [];
    const references = [];
    let relationNameExplicit = null;
    if (config !== undefined && config.kind === ts.SyntaxKind.ObjectLiteralExpression) {
      const fieldsNode = propertyNode(context, config, "fields");
      const referencesNode = propertyNode(context, config, "references");
      relationNameExplicit = staticString(propertyNode(context, config, "relationName"));
      for (const [sink, listNode] of [[fields, fieldsNode], [references, referencesNode]]) {
        if (listNode === null) continue;
        const arrayNode = listNode.kind === ts.SyntaxKind.ArrayLiteralExpression
          ? listNode
          : listNode.kind === ts.SyntaxKind.ArrowFunction ? listNode.body : null;
        if (arrayNode?.kind !== ts.SyntaxKind.ArrayLiteralExpression) continue;
        for (const element of arrayNode.elements) {
          let accessor = element;
          if (element.kind === ts.SyntaxKind.ArrowFunction) accessor = element.body;
          if (accessor?.kind !== ts.SyntaxKind.PropertyAccessExpression) continue;
          const ownerRow = context.tableForNodeIdentifier(accessor.expression);
          sink.push({
            table: ownerRow ? ownerRow.exportName : null,
            column: accessor.name.getText(),
            resolved: ownerRow !== null,
          });
        }
      }
    }
    const limitations = [];
    if (targetRow === null) {
      context.limit("relation-target-unresolved", modulePath, lineOf(sourceFile, property), relationName);
      limitations.push("relation-target-unresolved");
    }
    let cardinality;
    if (kind === "many") {
      cardinality = "one-to-many";
      limitations.push("cardinality-unproven");
    } else {
      // `one` is a proven one-to-one only with a uniqueness fact (a
      // unique column or unique single-member index behind the pair);
      // otherwise the database-level cardinality stays unproven.
      const joinUnique = relationJoinUnique(sourceTableRow, targetRow, fields, references);
      cardinality = joinUnique ? "one-to-one" : "one-to-many-or-one-to-one";
      if (!joinUnique) limitations.push("uniqueness-unproven");
      if (fields.length === 0 || references.length === 0) limitations.push("relation-fields-unresolved");
    }
    limitations.push("relation-policy-missing");
    rows.push({
      module: modulePath,
      name: relationName ?? null,
      sourceTable: sourceTableRow.exportName,
      sourceTableNative: sourceTableRow.native,
      targetTable: targetRow ? targetRow.exportName : null,
      targetTableNative: targetRow ? targetRow.native : null,
      kind,
      cardinality,
      relationName: relationNameExplicit,
      fields,
      references,
      span: spanOf(sourceFile, property),
      limitations,
      confidence: "inferred",
    });
  }
  if (rows.length === 0) {
    // A recognized relations declaration with zero extractable
    // endpoints is a coverage gap, never a clean empty set.
    context.sectionGaps.add("relations");
  }
  void checker;
  return rows;
}

/**
 * Whether a `one` relation has database-level uniqueness evidence: one
 * side's fields line up with a unique column or single-member unique
 * index. Application-level `one` alone never proves it.
 */
function relationJoinUnique(sourceRow, targetRow, fields, references) {
  if (sourceRow === null || targetRow === null) return false;
  const uniqueColumns = new Set();
  for (const column of sourceRow.columns) {
    if (column.unique || column.primaryKey) uniqueColumns.add(column.tsName);
  }
  for (const constraint of sourceRow.constraints) {
    if ((constraint.kind === "unique-index" || constraint.kind === "unique")
      && constraint.members.length === 1) {
      uniqueColumns.add(constraint.members[0]);
    }
  }
  if (fields.length === 1 && uniqueColumns.has(fields[0].column)) return true;
  const targetUnique = new Set();
  for (const column of targetRow.columns) {
    if (column.unique || column.primaryKey) targetUnique.add(column.tsName);
  }
  for (const constraint of targetRow.constraints) {
    if ((constraint.kind === "unique-index" || constraint.kind === "unique")
      && constraint.members.length === 1) {
      targetUnique.add(constraint.members[0]);
    }
  }
  if (references.length === 1 && targetUnique.has(references[0].column)) return true;
  return false;
}

// ---------------------------------------------------------------------------
// 7. Query and scope extraction.
// ---------------------------------------------------------------------------

/**
 * Classify one flattened chain whose root receiver is a drizzle
 * database/transaction into a query row. Unresolvable pieces become
 * explicit limitations — never silent drops, never invented fields.
 */
function extractQueryChain(context, sourceFile, { root, links }, receiverKind, txId) {
  const { ts, checker } = context;
  const head = links[0];
  const headName = head.name;
  let kind;
  if (headName === "select" || headName === "selectDistinct") kind = "select";
  else if (headName === "insert") kind = "insert";
  else if (headName === "update") kind = "update";
  else if (headName === "delete") kind = "delete";
  else if (headName === "from") kind = "select";
  else if (headName === "values") kind = "insert";
  else if (headName === "set") kind = "update";
  else return null;
  const modulePath = context.modulePathOf(sourceFile);
  const query = {
    id: null,
    kind,
    module: modulePath,
    span: spanOf(sourceFile, links[links.length - 1].node),
    headSpan: spanOf(sourceFile, head.node),
    receiver: receiverKind,
    target: null,
    awaited: isAwaitedChain(ts, links[links.length - 1].node),
    terminal: [...links].reverse().some((link) => link.kind === "call" && TERMINAL_METHODS.has(link.name)),
    chain: links.slice(0, MAX_CHAIN_LINKS).map((link) => link.name),
    reads: [],
    writes: [],
    projection: null,
    inputs: [],
    joins: [],
    transactionId: txId,
    conditional: false,
    effects: [],
    limitations: headName === "from" || headName === "values" || headName === "set"
      ? ["alias-continuation"]
      : [],
    confidence: "extracted",
    canonical: false,
  };
  if (query.limitations.length > 0) {
    context.limit("alias-continuation", modulePath, lineOf(sourceFile, head.node));
  }
  query.id = "q:" + sha256Hex(canonicalText({
    module: query.module,
    span: query.span,
    kind: query.kind,
    chain: query.chain,
  })).slice(0, 24);
  // Target table: the select builder names it at `.from(t)`; insert,
  // update, and delete name it in the head call arguments.
  if (kind === "select") {
    const fromLink = links[1];
    if (fromLink !== undefined && fromLink.kind === "call" && fromLink.name === "from") {
      const tableRow = context.tableForNodeIdentifier(fromLink.node.arguments?.[0]);
      if (tableRow !== null) {
        query.target = {
          native: tableRow.native,
          exportName: tableRow.exportName,
          physicalName: tableRow.physicalName,
          dialect: tableRow.dialect,
        };
      } else {
        context.limit("target-unresolved", modulePath, lineOf(sourceFile, fromLink.node), "from");
        query.limitations.push("target-unresolved");
      }
    } else {
      context.limit("target-unresolved", modulePath, lineOf(sourceFile, head.node), "from");
      query.limitations.push("target-unresolved");
    }
  } else {
    const targetArg = head.node.arguments?.[0];
    const tableRow = context.tableForNodeIdentifier(targetArg);
    if (tableRow !== null) {
      query.target = {
        native: tableRow.native,
        exportName: tableRow.exportName,
        physicalName: tableRow.physicalName,
        dialect: tableRow.dialect,
      };
    } else {
      context.limit("target-unresolved", modulePath, lineOf(sourceFile, head.node), kind);
      query.limitations.push("target-unresolved");
    }
  }
  for (let index = 1; index < links.length; index += 1) {
    const link = links[index];
    if (link.kind !== "call") continue;
    const args = link.node.arguments ?? [];
    switch (link.name) {
      case "where": case "having": {
        if (args[0] !== undefined) extractPredicate(context, sourceFile, args[0], query, "where");
        break;
      }
      case "values": {
        if (kind === "insert") extractValuesOrSet(context, sourceFile, args[0], query, "values");
        break;
      }
      case "set": {
        if (kind === "update") extractValuesOrSet(context, sourceFile, args[0], query, "set");
        break;
      }
      case "returning": {
        extractReturning(context, sourceFile, args, query);
        break;
      }
      case "$returningId": {
        query.projection = query.projection ?? { kind: "primary-keys", columns: [] };
        break;
      }
      case "$dynamic": {
        query.limitations.push("dynamic-builder");
        break;
      }
      case "orderBy": {
        for (const arg of args) recordFieldAccess(context, sourceFile, arg, query, "order");
        break;
      }
      case "groupBy": {
        for (const arg of args) recordFieldAccess(context, sourceFile, arg, query, "group");
        break;
      }
      case "onConflictDoUpdate": case "onConflictDoNothing": case "onDuplicateKeyUpdate": {
        // Upsert: the static effect is create-OR-update. Record the
        // alternatives honestly instead of claiming both happened.
        query.limitations.push("upsert-alternative");
        let setArg = null;
        if (link.name === "onDuplicateKeyUpdate") {
          // MySQL single-object form: { set: {...} } (or the bare set
          // object in legacy call shapes).
          setArg = args.length >= 2
            ? args[1] ?? null
            : args[0] !== undefined && args[0].kind === ts.SyntaxKind.ObjectLiteralExpression
              ? (propertyNode(context, args[0], "set") ?? args[0])
              : args[0] ?? null;
        } else if (args.length >= 2) {
          setArg = args[1] ?? null;
        } else if (args[0] !== undefined && args[0].kind === ts.SyntaxKind.ObjectLiteralExpression) {
          setArg = propertyNode(context, args[0], "set");
        }
        if (setArg !== null) {
          extractValuesOrSet(context, sourceFile, setArg, query, "upsert-set");
        }
        break;
      }
      default: {
        if (JOIN_METHODS.has(link.name)) {
          const joinTarget = context.tableForNodeIdentifier(args[0]);
          if (joinTarget !== null) {
            query.joins.push({
              kind: link.name,
              table: joinTarget.exportName,
              tableNative: joinTarget.native,
              span: spanOf(sourceFile, link.node),
            });
            if (args[1] !== undefined) extractPredicate(context, sourceFile, args[1], query, "join");
          } else {
            context.limit("join-target-unresolved", modulePath, lineOf(sourceFile, link.node));
            query.limitations.push("join-target-unresolved");
          }
        }
        break;
      }
    }
  }
  // The select projection: bare `select()` reads every declared column
  // of the target; `select({...})` names them.
  const selectArgs = head.node.arguments ?? [];
  if (kind === "select" && headName !== "from") {
    if (selectArgs.length === 0) {
      query.projection = { kind: "all", columns: [] };
      // The exact targeted row — re-found by native id, never the
      // export-name map whose last writer can be a same-named table
      // from another module (issue #116 fix round 3).
      const targetRow = tableForQueryTarget(context, query);
      for (const column of targetRow?.columns ?? []) {
        if (query.reads.length < MAX_QUERY_FIELDS) {
          query.reads.push({ table: query.target.exportName, column: column.tsName, role: "projection" });
        }
      }
    } else {
      extractProjection(context, sourceFile, selectArgs[0], query);
    }
  }
  // Effects (evidence, never canonical declarations).
  const readFields = dedupeFields(query.reads);
  const writeFields = dedupeFields(query.writes);
  if (kind === "select") {
    query.effects = [{ action: "read", level: "entity", fields: readFields }];
  } else if (kind === "insert") {
    query.effects = [{ action: "create", level: "entity", fields: writeFields }];
  } else if (kind === "update") {
    query.effects = [{ action: "update", level: "entity", fields: writeFields }];
  } else {
    query.effects = [{ action: "delete", level: "entity", fields: [] }];
  }
  // Builder-not-executed honesty: an unawaited, unterminated chain
  // describes a possible query only — never a runtime-verified effect.
  // An explicit `return` hands the builder to the caller on purpose,
  // so it does not add execution noise (issue #116 fix round).
  if (!query.awaited && !query.terminal
    && !isReturnedChain(ts, links[links.length - 1].node)) {
    query.limitations.push("builder-not-executed");
  }
  void root;
  void checker;
  return query;
}

function dedupeFields(fields) {
  const seen = new Map();
  for (const field of fields) {
    const key = `${field.table}.${field.column}`;
    if (!seen.has(key)) seen.set(key, field);
  }
  return [...seen.values()].map((field) => ({ table: field.table, column: field.column }));
}

/** Whether the outermost chain call sits under an await. */
function isAwaitedChain(ts, node) {
  let current = node.parent;
  let guard = 0;
  while (current !== undefined && current !== null && guard++ < 12) {
    if (current.kind === ts.SyntaxKind.AwaitExpression) return true;
    if (current.kind === ts.SyntaxKind.PropertyAccessExpression
      || current.kind === ts.SyntaxKind.CallExpression
      || current.kind === ts.SyntaxKind.ParenthesizedExpression
      || current.kind === ts.SyntaxKind.NonNullExpression
      || current.kind === ts.SyntaxKind.AsExpression) {
      current = current.parent;
      continue;
    }
    return false;
  }
  return false;
}

/** Whether the chain is handed off by an explicit `return`. */
function isReturnedChain(ts, node) {
  let current = node.parent;
  let guard = 0;
  while (current !== undefined && current !== null && guard++ < 12) {
    if (current.kind === ts.SyntaxKind.ReturnStatement) return true;
    if (current.kind === ts.SyntaxKind.PropertyAccessExpression
      || current.kind === ts.SyntaxKind.CallExpression
      || current.kind === ts.SyntaxKind.ParenthesizedExpression
      || current.kind === ts.SyntaxKind.NonNullExpression
      || current.kind === ts.SyntaxKind.AsExpression) {
      current = current.parent;
      continue;
    }
    return false;
  }
  return false;
}

const COMPARISON_PREDICATES = new Set(["eq", "ne", "gt", "gte", "lt", "lte", "like", "ilike",
  "inArray", "notInArray", "between", "notBetween", "arrayContains", "arrayContained",
  "arrayOverlaps"]);

/**
 * Bounded predicate walk: `eq`/`and`/`or`/`inArray`/... field reads and
 * query inputs. Unresolved or raw-SQL predicates become limitations.
 */
function extractPredicate(context, sourceFile, node, query, role = "where") {
  const { ts } = context;
  if (node === undefined || node === null) {
    context.limit("predicate-unresolved", query.module, null, role);
    query.limitations.push("predicate-unresolved");
    return;
  }
  if (isSqlTaggedTemplate(ts, node)) {
    context.limit("raw-sql", query.module, lineOf(sourceFile, node));
    query.limitations.push("raw-sql");
    return;
  }
  if (node.kind !== ts.SyntaxKind.CallExpression) {
    context.limit("predicate-unresolved", query.module, lineOf(sourceFile, node));
    query.limitations.push("predicate-unresolved");
    return;
  }
  const { links } = flattenChain(ts, node);
  const calleeName = links.length > 0 && links[0].kind === "call" && links.length === 1
    ? links[0].name
    : null;
  if (calleeName === "and" || calleeName === "or" || calleeName === "not") {
    for (const arg of node.arguments ?? []) {
      extractPredicate(context, sourceFile, arg, query, role);
    }
    return;
  }
  if (calleeName === "isNull" || calleeName === "isNotNull") {
    recordFieldAccess(context, sourceFile, node.arguments?.[0], query, role);
    return;
  }
  if (calleeName !== null && COMPARISON_PREDICATES.has(calleeName)) {
    recordFieldAccess(context, sourceFile, node.arguments?.[0], query, role);
    const rest = calleeName === "between" || calleeName === "notBetween"
      ? (node.arguments ?? []).slice(1)
      : (node.arguments ?? []).slice(1, 2);
    for (const arg of rest) recordInput(context, sourceFile, arg, query, role);
    return;
  }
  context.limit("predicate-unresolved", query.module, lineOf(sourceFile, node), calleeName ?? "?");
  query.limitations.push("predicate-unresolved");
}

/**
 * Record `table.column` reads from a field access expression; the base
 * identifier must resolve to an extracted table variable.
 */
function recordFieldAccess(context, sourceFile, node, query, role) {
  const { ts } = context;
  if (node === undefined || node === null) return;
  if (node.kind === ts.SyntaxKind.PropertyAccessExpression) {
    const tableRow = context.tableForNodeIdentifier(node.expression);
    const column = node.name.getText();
    if (tableRow !== null) {
      if (query.reads.length < MAX_QUERY_FIELDS) {
        query.reads.push({ table: tableRow.exportName, column, role });
      } else {
        context.limit("truncated", query.module, lineOf(sourceFile, node), "reads");
      }
    } else {
      context.limit("target-unresolved", query.module, lineOf(sourceFile, node), column);
      query.limitations.push("target-unresolved");
    }
    return;
  }
  if (node.kind === ts.SyntaxKind.CallExpression) {
    // `asc(users.id)` / `desc(users.id)` wrappers and SQL fragments.
    const { links } = flattenChain(ts, node);
    const calleeName = links.length > 0 && links[0].kind === "call" ? links[0].name : null;
    if (calleeName === "asc" || calleeName === "desc") {
      recordFieldAccess(context, sourceFile, node.arguments?.[0], query, role);
      return;
    }
    context.limit("raw-sql", query.module, lineOf(sourceFile, node));
    query.limitations.push("raw-sql");
    return;
  }
  context.limit("predicate-unresolved", query.module, lineOf(sourceFile, node));
  query.limitations.push("predicate-unresolved");
}

/** Record one query input with its bounded compiler type spelling. */
function recordInput(context, sourceFile, node, query, role) {
  const { ts, checker } = context;
  if (node === undefined || node === null) return;
  if (query.inputs.length >= MAX_INPUTS) {
    context.limit("truncated", query.module, lineOf(sourceFile, node), "inputs");
    return;
  }
  const primitive = staticPrimitive(node);
  if (primitive !== null) {
    query.inputs.push({ role, expr: "literal", typeToken: primitive.kind, value: primitive.token });
    return;
  }
  if (node.kind === ts.SyntaxKind.PropertyAccessExpression) {
    const tableRow = context.tableForNodeIdentifier(node.expression);
    if (tableRow !== null) {
      // A column reference on an extracted table is a field read of
      // that table, not an external input — join RHS equality columns
      // were previously misrecorded as reference inputs (issue #116
      // fix round).
      if (query.reads.length < MAX_QUERY_FIELDS) {
        query.reads.push({ table: tableRow.exportName, column: node.name.getText(), role });
      } else {
        context.limit("truncated", query.module, lineOf(sourceFile, node), "reads");
      }
      return;
    }
  }
  if (node.kind === ts.SyntaxKind.Identifier
    || node.kind === ts.SyntaxKind.PropertyAccessExpression) {
    let typeToken = null;
    try {
      typeToken = checker.typeToString(checker.getTypeAtLocation(node), undefined,
        ts.TypeFormatFlags.NoTruncation).slice(0, 64);
    } catch {
      typeToken = null;
    }
    query.inputs.push({ role, expr: "reference", typeToken, value: null });
    return;
  }
  if (node.kind === ts.SyntaxKind.ObjectLiteralExpression
    || node.kind === ts.SyntaxKind.ArrayLiteralExpression) {
    query.inputs.push({ role, expr: "static-composite", typeToken: null, value: null });
    return;
  }
  context.limit("input-dynamic", query.module, lineOf(sourceFile, node));
  query.inputs.push({ role, expr: "dynamic", typeToken: null, value: null });
}

/**
 * The `.values({...})` / `.set({...})` field writes. Dynamic shapes
 * (spreads, identifiers, computed keys) become explicit limitations —
 * the entity-level effect stays, the field set is never invented.
 */
function extractValuesOrSet(context, sourceFile, node, query, role) {
  const { ts } = context;
  if (node === undefined || node === null) return;
  if (node.kind !== ts.SyntaxKind.ObjectLiteralExpression) {
    const code = role === "set" || role === "upsert-set" ? "dynamic-set" : "dynamic-values";
    context.limit(code, query.module, lineOf(sourceFile, node));
    query.limitations.push(code);
    return;
  }
  for (const property of node.properties) {
    if (property.kind === ts.SyntaxKind.PropertyAssignment
      || property.kind === ts.SyntaxKind.ShorthandPropertyAssignment) {
      if (property.name?.kind === ts.SyntaxKind.ComputedPropertyName) {
        context.limit("dynamic-values", query.module, lineOf(sourceFile, property));
        query.limitations.push("dynamic-values");
        continue;
      }
      const columnName = property.name?.getText();
      if (typeof columnName !== "string" || columnName.length === 0) continue;
      if (query.writes.length < MAX_QUERY_FIELDS) {
        query.writes.push({ table: query.target?.exportName ?? null, column: columnName, role });
      } else {
        context.limit("truncated", query.module, lineOf(sourceFile, property), "writes");
      }
      recordInput(context, sourceFile, property.initializer ?? property.name, query, role);
      continue;
    }
    if (property.kind === ts.SyntaxKind.SpreadAssignment) {
      const code = role === "set" || role === "upsert-set" ? "dynamic-set" : "dynamic-values";
      context.limit(code, query.module, lineOf(sourceFile, property));
      query.limitations.push(code);
    }
  }
}

/**
 * The `.returning({...})` / `.returning()` projection (PostgreSQL) — a
 * field read of returned columns.
 */
function extractReturning(context, sourceFile, args, query) {
  const arg = args?.[0];
  if (arg === undefined) {
    query.projection = { kind: "returning-all", columns: [] };
    return;
  }
  if (arg.kind !== context.ts.SyntaxKind.ObjectLiteralExpression) {
    context.limit("returning-dynamic", query.module, lineOf(sourceFile, arg));
    query.limitations.push("returning-dynamic");
    return;
  }
  const columns = [];
  for (const property of arg.properties) {
    if (property.kind !== context.ts.SyntaxKind.PropertyAssignment
      && property.kind !== context.ts.SyntaxKind.ShorthandPropertyAssignment) continue;
    columns.push(property.name?.getText() ?? null);
  }
  query.projection = { kind: "returning", columns };
  for (const column of columns) {
    if (query.reads.length < MAX_QUERY_FIELDS) {
      query.reads.push({ table: query.target?.exportName ?? null, column, role: "returning" });
    }
  }
}

/** The `select({...})` projection of a select query. */
function extractProjection(context, sourceFile, node, query) {
  const { ts } = context;
  if (node === undefined || node === null) return;
  if (node.kind !== ts.SyntaxKind.ObjectLiteralExpression) {
    context.limit("projection-dynamic", query.module, lineOf(sourceFile, node));
    query.limitations.push("projection-dynamic");
    return;
  }
  const columns = [];
  for (const property of node.properties) {
    if (property.kind !== ts.SyntaxKind.PropertyAssignment
      && property.kind !== ts.SyntaxKind.ShorthandPropertyAssignment) {
      context.limit("projection-dynamic", query.module, lineOf(sourceFile, property));
      query.limitations.push("projection-dynamic");
      continue;
    }
    const alias = property.name?.getText();
    const value = property.kind === ts.SyntaxKind.PropertyAssignment
      ? property.initializer
      : property.name;
    if (value?.kind === ts.SyntaxKind.PropertyAccessExpression) {
      const tableRow = context.tableForNodeIdentifier(value.expression);
      columns.push({ alias, table: tableRow ? tableRow.exportName : null, column: value.name.getText() });
      if (query.reads.length < MAX_QUERY_FIELDS) {
        query.reads.push({
          table: tableRow ? tableRow.exportName : null,
          column: value.name.getText(),
          role: "projection",
        });
      }
    } else {
      // Computed/SQL projections are bounded unknowns.
      context.limit("raw-sql", query.module, lineOf(sourceFile, property));
      query.limitations.push("raw-sql");
      columns.push({ alias, table: null, column: null });
    }
  }
  query.projection = { kind: "columns", columns };
}

// ---------------------------------------------------------------------------
// 8. Transactions.
// ---------------------------------------------------------------------------

/**
 * One `db.transaction(async (tx) => {...})` boundary with its member
 * queries, nested groups, rollback site, and escape limitations. The
 * callback proves a boundary in source; atomicity/isolation remain
 * runtime facts this static evidence never asserts.
 */
function extractTransaction(context, sourceFile, callNode, receiverKind, parentTxId, walkState) {
  const { ts } = context;
  const callback = callNode.arguments?.[0];
  const modulePath = context.modulePathOf(sourceFile);
  const row = {
    id: `tx${walkState.txCounter.next++}`,
    module: modulePath,
    span: spanOf(sourceFile, callNode),
    receiver: receiverKind,
    parent: parentTxId,
    nested: [],
    members: [],
    rollback: null,
    limitations: [],
    confidence: "extracted",
  };
  if (callback === undefined
    || (callback.kind !== ts.SyntaxKind.ArrowFunction && callback.kind !== ts.SyntaxKind.FunctionExpression)) {
    context.limit("tx-callback-shape-unknown", modulePath, lineOf(sourceFile, callNode));
    row.limitations.push("tx-callback-shape-unknown");
    return row;
  }
  const param = callback.parameters[0];
  const txParamName = param?.name?.kind === ts.SyntaxKind.Identifier ? param.name.text : null;
  if (txParamName === null) {
    context.limit("tx-callback-shape-unknown", modulePath, lineOf(sourceFile, callNode));
    row.limitations.push("tx-callback-shape-unknown");
  }
  const body = callback.body;
  if (body === undefined) return row;
  const statements = body.kind === ts.SyntaxKind.Block ? [...body.statements] : [body];
  // Scoped aliases inside the callback: `const t2 = tx` keeps identity;
  // assigning the handle outward (`outer = tx`) is an escape.
  const aliases = new Map();
  let escaped = false;
  if (txParamName !== null) aliases.set(txParamName, row.id);
  const nestedState = { ...walkState, txScopes: new Map(walkState.txScopes) };
  if (txParamName !== null) nestedState.txScopes.set(txParamName, row.id);
  let statementsWalked = 0;

  const receiverTxOf = (root, localAliases) => {
    if (root?.kind !== ts.SyntaxKind.Identifier) return { tx: null, identity: "unknown" };
    const name = root.text;
    const local = localAliases.get(name);
    if (local !== undefined) return { tx: local, identity: local === row.id ? "tx" : "tx-alias" };
    const scoped = nestedState.txScopes.get(name);
    if (scoped !== undefined) return { tx: scoped, identity: "tx" };
    if (nestedState.dbAliases.has(name)) return { tx: null, identity: "db-alias" };
    // Receiver identity is symbol-resolved (the name-keyed memo is
    // gone — issue #116 fix round 3).
    return { tx: null, identity: rootIdentityKind(context, root, nestedState) };
  };

  const walkExpression = (node, conditional, localAliases) => {
    if (node === undefined || node === null) return;
    if (node.kind === ts.SyntaxKind.CallExpression) {
      const { root, links } = flattenChain(ts, node);
      const tail = links[links.length - 1];
      const tailName = tail?.kind === "call" ? tail.name : null;
      const { tx, identity } = receiverTxOf(root, localAliases);
      if (tx !== null && tailName === "rollback") {
        row.rollback = spanOf(sourceFile, node);
        context.limit("rollback", modulePath, lineOf(sourceFile, node));
        return;
      }
      if (tx !== null && tailName === "transaction") {
        const nested = extractTransaction(context, sourceFile, node, "tx", row.id, {
          ...nestedState,
          txScopes: new Map(nestedState.txScopes),
        });
        row.nested.push(nested.id);
        context.limit("nested-transaction", modulePath, lineOf(sourceFile, node));
        context.transactions.push(nested);
        return;
      }
      if (links.length > 0 && links[0].kind === "call"
        && (QUERY_HEADS.has(links[0].name) || CONTINUATION_HEADS.has(links[0].name))) {
        if (identity === "tx" || identity === "tx-alias" || identity === "db"
          || identity === "db-alias") {
          // transactionId is bound ONLY for tx-receiver chains; a db
          // query inside the callback is not a member of the group —
          // the two axes stay separate (issue #116 fix round).
          const query = extractQueryChain(context, sourceFile, { root, links },
            identity, tx);
          if (query !== null) {
            if (tx === null) {
              context.limit("query-not-tx-bound", modulePath, lineOf(sourceFile, node));
              query.limitations.push("query-not-tx-bound");
            } else {
              row.members.push(query.id);
            }
            if (conditional) {
              query.conditional = true;
              query.limitations.push("conditional-flow");
            }
            context.queries.push(query);
          }
          return;
        }
      }
      // Recognized out-of-subset surfaces inside the callback bound to
      // a drizzle receiver: explicit limitations, never drops (issue
      // #116 fix round).
      if (links.length > 0
        && (identity === "tx" || identity === "tx-alias" || identity === "db"
          || identity === "db-alias")) {
        if (links[0].kind === "access" && links[0].name === "query") {
          context.limit("relational-query-unsupported", modulePath, lineOf(sourceFile, node),
            "relational-query-api");
          context.sectionGaps.add("queries");
          return;
        }
        if (links[0].kind === "call" && links[0].name === "batch") {
          context.limit("batch-unsupported", modulePath, lineOf(sourceFile, node), "batch-api");
          context.sectionGaps.add("queries");
          return;
        }
      }
      // Raw executes on the db handle inside the transaction callback.
      if (tx === null && (identity === "db" || identity === "db-alias")
        && tailName !== undefined && TERMINAL_METHODS.has(tailName)) {
        context.limit("query-not-tx-bound", modulePath, lineOf(sourceFile, node), tailName);
      }
      return;
    }
    if (node.kind === ts.SyntaxKind.AwaitExpression
      || node.kind === ts.SyntaxKind.ParenthesizedExpression
      || node.kind === ts.SyntaxKind.PropertyAccessExpression
      || node.kind === ts.SyntaxKind.ConditionalExpression) {
      ts.forEachChild(node, (child) => walkExpression(child, conditional, localAliases));
      return;
    }
    if (node.kind === ts.SyntaxKind.BinaryExpression
      && node.operatorToken?.kind === ts.SyntaxKind.EqualsToken) {
      // Escape detection: the tx handle stored into anything but a
      // fresh local alias degrades the boundary evidence.
      if (node.right?.kind === ts.SyntaxKind.Identifier && localAliases.has(node.right.text)
        && !node.left?.kind === ts.SyntaxKind.Identifier) {
        escaped = true;
        return;
      }
      if (node.right?.kind === ts.SyntaxKind.Identifier && localAliases.has(node.right.text)
        && (node.left?.kind === ts.SyntaxKind.PropertyAccessExpression
          || node.left?.kind === ts.SyntaxKind.ElementAccessExpression)) {
        escaped = true;
        return;
      }
      walkExpression(node.left, conditional, localAliases);
      walkExpression(node.right, conditional, localAliases);
      return;
    }
  };

  const walkStatement = (node, conditional) => {
    if (node === undefined || node === null) return;
    if (statementsWalked++ >= MAX_TX_STATEMENTS) {
      context.limit("truncated", modulePath, lineOf(sourceFile, node), "transaction-body");
      row.limitations.push("truncated-transaction-body");
      return;
    }
    if (node.kind === ts.SyntaxKind.VariableDeclarationList) {
      for (const declaration of node.declarations) {
        const name = declaration.name?.kind === ts.SyntaxKind.Identifier
          ? declaration.name.text
          : null;
        const initializer = declaration.initializer;
        if (name !== null && initializer !== undefined) {
          // Local alias capture: `const t2 = tx`.
          if (initializer.kind === ts.SyntaxKind.Identifier) {
            const aliased = aliases.get(initializer.text)
              ?? nestedState.txScopes.get(initializer.text);
            if (aliased !== undefined) {
              aliases.set(name, aliased);
              continue;
            }
          }
          walkExpression(initializer, conditional, aliases);
        }
      }
      return;
    }
    if (node.kind === ts.SyntaxKind.ExpressionStatement) {
      walkExpression(node.expression, conditional, aliases);
      return;
    }
    if (node.kind === ts.SyntaxKind.ReturnStatement) {
      walkExpression(node.expression, conditional, aliases);
      return;
    }
    if (node.kind === ts.SyntaxKind.IfStatement) {
      walkExpression(node.expression, conditional, aliases);
      walkStatement(node.thenStatement, true);
      if (node.elseStatement !== undefined) walkStatement(node.elseStatement, true);
      return;
    }
    if (node.kind === ts.SyntaxKind.Block) {
      for (const statement of node.statements) walkStatement(statement, conditional);
      return;
    }
    if (node.kind === ts.SyntaxKind.TryStatement) {
      walkStatement(node.tryBlock, conditional);
      if (node.catchClause?.block !== undefined) walkStatement(node.catchClause.block, true);
      if (node.finallyBlock !== undefined) walkStatement(node.finallyBlock, true);
      return;
    }
    if (node.kind === ts.SyntaxKind.ForOfStatement || node.kind === ts.SyntaxKind.ForInStatement
      || node.kind === ts.SyntaxKind.ForStatement || node.kind === ts.SyntaxKind.WhileStatement
      || node.kind === ts.SyntaxKind.DoStatement) {
      walkStatement(node.statement, true);
      return;
    }
    ts.forEachChild(node, (child) => walkStatement(child, conditional));
  };

  for (const statement of statements) {
    walkStatement(statement, false);
  }
  if (escaped) {
    context.limit("tx-escaped", modulePath, lineOf(sourceFile, callback));
    row.limitations.push("tx-escaped");
  }
  return row;
}

// ---------------------------------------------------------------------------
// 9. Receiver identity and migrations.
// ---------------------------------------------------------------------------

/**
 * The receiver kind of a chain root: "db" (factory-created database or
 * closure-typed parameter), "tx" (transaction callback parameter via
 * the walk state), "db-alias" (variable initialized from a db chain),
 * "namespace" (a module namespace object — provably NOT a handle),
 * or "unknown".
 */
function rootIdentityKind(context, root, state) {
  const { ts, checker } = context;
  if (root === undefined || root === null) return "unknown";
  if (root.kind === ts.SyntaxKind.Identifier) {
    if (state.txScopes.has(root.text)) return "tx";
    if (state.dbAliases.has(root.text)) return "db-alias";
    let symbol;
    try {
      symbol = checker.getSymbolAtLocation(root);
    } catch {
      return "unknown";
    }
    // Import aliases resolve through their target declaration: an
    // imported database handle is a variable in another module, and
    // the alias type can be `any` when annotations do not survive.
    const resolved = resolveAliasSymbol(checker, symbol);
    // A module namespace object (`import * as orm`) is never a
    // database handle: its type is the module's export surface, which
    // RESOLVES INTO THE EMBEDDED CLOSURE for vendored modules — the
    // closure test alone would type the namespace as `db` and
    // fabricate evidence from a provably-wrong receiver identity
    // (issue #116 fix round 4).
    if (isModuleNamespaceSymbol(ts, checker, symbol, resolved)) return "namespace";
    // The proven-receiver memo is keyed by the RESOLVED DECLARATION
    // SYMBOL, never the bare name: two same-named bindings are two
    // different receivers, so a non-Drizzle local named `db` can never
    // inherit the identity proven for the Drizzle client handle — and
    // a real handle can never be masked by a same-named local (issue
    // #116 fix round 3, the db.*/db.batch receiver short-circuit).
    const memoKey = resolved === undefined || resolved === null
      ? null
      : symbolKey(resolved);
    if (memoKey !== null) {
      const memo = state.dbSymbols.get(memoKey);
      if (memo !== undefined) return memo;
    }
    const declaration = variableDeclarationOf(resolved);
    const depth = state.depth ?? 0;
    if (declaration !== null && declaration !== undefined
      && declaration.initializer !== undefined && depth < MAX_ALIAS_HOPS) {
      const innerRoot = flattenChain(ts, declaration.initializer).root;
      const kind = rootIdentityKind(context, innerRoot, {
        txScopes: state.txScopes,
        dbAliases: state.dbAliases,
        dbSymbols: state.dbSymbols,
        depth: depth + 1,
      });
      if (kind === "db" || kind === "db-alias") {
        if (memoKey !== null) state.dbSymbols.set(memoKey, kind);
        return kind;
      }
      if (kind !== "unknown") return kind;
    }
    if (typeInClosure(ts, checker, root, context.closurePrefix)) {
      if (memoKey !== null) state.dbSymbols.set(memoKey, "db");
      return "db";
    }
    return "unknown";
  }
  if (typeInClosure(ts, checker, root, context.closurePrefix)) return "db";
  return "unknown";
}

/**
 * Migration/schema references: `drizzle.config.*` literals and the
 * bounded migration folder contents (paths + digests, never execution,
 * never applied-state claims).
 */
function extractMigrations(extract, ts, manifest, readBytes) {
  const allFiles = [
    ...manifest.sourceFiles, ...manifest.configFiles,
    ...manifest.packageFiles, ...manifest.otherFiles,
  ];
  const configEntries = manifest.sourceFiles
    .filter((entry) => /drizzle\.config\.[cm]?tsx?$/.test(entry.path))
    .slice(0, 8);
  for (const entry of configEntries) {
    let text;
    try {
      text = readBytes(entry.path).toString("utf8");
    } catch {
      continue;
    }
    extract.noteFile(entry.path, entry.digest);
    const sourceFile = ts.createSourceFile(entry.path, text, ts.ScriptTarget.Latest, true);
    const row = {
      kind: "config",
      module: entry.path,
      digest: entry.digest,
      dialect: null,
      schemaRefs: [],
      out: null,
      layout: null,
      entries: [],
      span: null,
      limitations: [],
    };
    const visit = (node) => {
      if (node.kind === ts.SyntaxKind.PropertyAssignment) {
        const name = node.name?.getText();
        if (name === "dialect") {
          row.dialect = staticString(node.initializer);
          row.span = spanOf(sourceFile, node);
        } else if (name === "schema") {
          const value = staticString(node.initializer);
          if (value !== null) {
            row.schemaRefs.push(normalizeRelativeRef(value));
          } else if (node.initializer?.kind === ts.SyntaxKind.ArrayLiteralExpression) {
            for (const element of node.initializer.elements) {
              const member = staticString(element);
              if (member !== null) row.schemaRefs.push(normalizeRelativeRef(member));
            }
          } else if (node.initializer !== undefined) {
            context2Limit(extract, "migration-config-invalid", entry.path, lineOf(sourceFile, node), "schema");
          }
        } else if (name === "out") {
          const value = staticString(node.initializer);
          row.out = value === null ? null : normalizeRelativeRef(value);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
    if (row.out !== null) {
      if (row.out.includes("..") || row.out.startsWith("/")) {
        extract.limit("migration-path-escapes-root", entry.path, row.span?.start?.line ?? null, row.out);
        row.limitations.push("migration-path-escapes-root");
      } else {
        const prefix = `${row.out.replace(/\/$/, "")}/`;
        const folderEntries = [...manifest.sourceFiles, ...manifest.otherFiles]
          .filter((file) => file.path.startsWith(prefix));
        if (folderEntries.length === 0) {
          extract.limit("migration-ref-missing", entry.path, row.span?.start?.line ?? null, row.out);
          row.limitations.push("migration-ref-missing");
        } else {
          const journalCandidates = [
            `${prefix}meta/_journal.json`,
            `${prefix}meta/journal.json`,
          ];
          const journal = folderEntries.find((file) => journalCandidates.includes(file.path));
          row.layout = journal !== undefined ? "folders-journal" : "sql-only";
          if (journal === undefined) {
            extract.limit("migration-layout-unknown", entry.path, null, row.out);
            row.limitations.push("migration-layout-unknown");
          }
          const entryOverflow = { hit: false };
          for (const file of folderEntries.slice(0, MAX_MIGRATION_ENTRIES)) {
            // Migration folder inputs are part of the evidence's file
            // provenance (issue #116 fix round): their digests feed
            // both the entry rows and contributingFiles.
            extract.noteFile(file.path, "sha256:" + file.digest);
            pushBounded(row.entries, MAX_MIGRATION_ENTRIES, {
              path: file.path,
              digest: "sha256:" + file.digest,
              kind: file.path.endsWith(".sql") ? "sql"
                : file.path.endsWith("journal.json") ? "journal"
                : file.path.endsWith("snapshot.json") ? "snapshot" : "meta",
            }, entryOverflow);
          }
          if (entryOverflow.hit) {
            extract.limit("truncated", entry.path, null, "migration-entries");
            row.limitations.push("truncated-migration-entries");
          }
        }
      }
    }
    for (const schemaRef of row.schemaRefs) {
      if (schemaRef.includes("..") || schemaRef.startsWith("/")) {
        extract.limit("migration-path-escapes-root", entry.path, null, schemaRef);
        row.limitations.push("migration-path-escapes-root");
        continue;
      }
      // A schema reference must exist in the granted inventory — exact
      // file, directory, or bounded glob prefix. A dangling reference
      // is an explicit limitation, never silently accepted (issue #116
      // fix round).
      if (!inventoryRefKnown(allFiles, schemaRef)) {
        extract.limit("migration-ref-missing", entry.path, row.span?.start?.line ?? null, schemaRef);
        row.limitations.push("migration-ref-missing");
      }
    }
    pushBounded(extract.migrations, MAX_MIGRATIONS, row, extract.overflow);
  }
}

function context2Limit(extract, code, path, line, detail) {
  extract.limit(code, path, line, detail);
}

/**
 * Whether a migration schema reference names something the granted
 * inventory contains: an exact file, a directory prefix, or a bounded
 * glob prefix (dir slash star, dir slash star-star). Never a
 * filesystem probe — the inventory is the read view's
 * already-enumerated truth.
 */
function inventoryRefKnown(allFiles, schemaRef) {
  const paths = allFiles.map((file) => file.path);
  if (paths.includes(schemaRef)) return true;
  const base = schemaRef.replace(/\/\*\*?\/?(?:\.[a-z]+)?$/i, "");
  const prefix = base !== schemaRef && base.length > 0 ? base : schemaRef;
  return paths.some((path) => path.startsWith(`${prefix}/`));
}

/** Normalize a config literal (`./drizzle`) to an inventory path. */
function normalizeRelativeRef(value) {
  return value.replace(/^\.\//, "").replace(/\/$/, "");
}

// ---------------------------------------------------------------------------
// 10. Scope evidence, bindings input, storage projection comparison.
// ---------------------------------------------------------------------------

/**
 * The exact table row a resolved query targeted. Query targets resolve
 * by symbol identity, but the export-name map holds only the LAST
 * writer per name — with two modules exporting the same name it can
 * return another module's table. The row is therefore re-found by its
 * unique native id (module + export + physical name), which pins the
 * precise declaration the query was built against (issue #116 fix
 * round 3).
 */
function tableForQueryTarget(context, query) {
  const native = query.target?.native ?? null;
  if (native === null) return null;
  return context.tables.find((table) => table.native === native) ?? null;
}

/**
 * Tenant/workspace scope evidence rows derived from extracted queries.
 * An equality predicate on a tenant-like column is scope evidence; its
 * absence on a table that declares such a column is a recorded
 * limitation — never an authorization verdict.
 */
function extractScopeRows(context) {
  const rows = [];
  const overflow = { hit: false };
  for (const query of context.queries) {
    const targetExport = query.target?.exportName ?? null;
    // The precise targeted row (native-id re-resolution) — the
    // export-name map can hold another module's same-named table, whose
    // tenantKey would decide scope evidence wrongly (issue #116 fix
    // round 3).
    const targetRow = tableForQueryTarget(context, query);
    for (const read of query.reads) {
      if (read.role !== "where" || read.table !== targetExport) continue;
      if (!SCOPE_KEY_NAMES.has(read.column)) continue;
      const input = query.inputs.find((candidate) => candidate.role === "where"
        && candidate.expr !== "dynamic");
      pushBounded(rows, MAX_SCOPE_ROWS, {
        queryId: query.id,
        kind: "predicate",
        table: targetExport,
        column: read.column,
        predicate: "equality",
        span: query.headSpan,
        confidence: input !== undefined ? "extracted" : "unknown",
      }, overflow);
      break;
    }
    if (query.kind === "insert" && targetExport !== null) {
      const scopeWrite = query.writes.find((write) => SCOPE_KEY_NAMES.has(write.column));
      if (scopeWrite !== undefined) {
        pushBounded(rows, MAX_SCOPE_ROWS, {
          queryId: query.id,
          kind: "create-scope",
          table: targetExport,
          column: scopeWrite.column,
          predicate: "value",
          span: query.headSpan,
          confidence: "extracted",
        }, overflow);
      }
    }
    if (targetRow !== null && targetRow.tenantKey !== null && query.kind !== "insert") {
      const scoped = rows.some((row) => row.queryId === query.id);
      if (!scoped && !query.limitations.includes("raw-sql")) {
        context.limit("scope-predicate-missing", query.module, query.span.start.line, query.id);
        query.limitations.push("scope-predicate-missing");
      }
    }
    if (overflow.hit) break;
  }
  return rows;
}

/**
 * Decode the closed owner-supplied binding input. Malformed input is a
 * recorded limitation — never a crash, never a guessed mapping.
 */
export function decodeBindingsInput(text) {
  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return { ok: false, reason: "bindings-input-invalid" };
  }
  if (!isObject(document) || document.schema !== DRIZZLE_BINDINGS_INPUT_SCHEMA) {
    return { ok: false, reason: "bindings-input-invalid" };
  }
  if (!Array.isArray(document.entities)) {
    return { ok: false, reason: "bindings-input-invalid" };
  }
  const entities = [];
  for (const entity of document.entities.slice(0, MAX_BINDINGS)) {
    if (!isObject(entity) || typeof entity.entity !== "string" || typeof entity.table !== "string") {
      return { ok: false, reason: "bindings-input-invalid" };
    }
    entities.push({ entity: entity.entity, table: entity.table });
  }
  return { ok: true, entities };
}

/**
 * Compare extracted tables against the owner-supplied storage
 * projection attachment. This is a declaration-vs-projection check:
 * the live database stays unknown by construction.
 */
function compareProjection(context, projection) {
  const rows = [];
  const overflow = { hit: false };
  if (!isObject(projection) || projection.schema !== DRIZZLE_PROJECTION_INPUT_SCHEMA
    || !Array.isArray(projection.tables)) {
    context.limit("projection-input-invalid", null, null);
    return { rows, state: "invalid" };
  }
  const projectedByName = new Map(projection.tables
    .filter((table) => isObject(table) && typeof table.table === "string")
    .map((table) => [table.table, table]));
  for (const table of context.tables) {
    if (table.physicalName === null) continue;
    const projected = projectedByName.get(table.physicalName);
    if (projected === undefined) {
      context.limit("projection-table-missing", table.module, table.span.start.line, table.physicalName);
      pushBounded(rows, MAX_PROJECTION_ROWS, { table: table.physicalName, status: "missing-in-projection" }, overflow);
      continue;
    }
    const projectedColumns = new Map((projected.columns ?? [])
      .filter((column) => isObject(column) && typeof column.name === "string")
      .map((column) => [column.name, column]));
    for (const column of table.columns) {
      const expected = projectedColumns.get(column.physicalName);
      if (expected === undefined) {
        context.limit("projection-column-missing", table.module, column.span.start.line, column.physicalName);
        pushBounded(rows, MAX_PROJECTION_ROWS, {
          table: table.physicalName,
          column: column.physicalName,
          status: "missing-in-projection",
        }, overflow);
        continue;
      }
      const divergences = [];
      if (typeof expected.type === "string" && expected.type !== column.typeToken) {
        // A divergent type spelling is exactly the honest finding this
        // comparison exists for (e.g. varchar(36) declared vs binary(16)
        // projected): TS equivalence never proves storage equivalence.
        context.limit("projection-type-divergent", table.module, column.span.start.line,
          `${table.physicalName}.${column.physicalName}`);
        divergences.push("type-divergent");
      }
      if (typeof expected.nullable === "boolean" && expected.nullable === column.notNull) {
        context.limit("projection-nullability-divergent", table.module, column.span.start.line,
          `${table.physicalName}.${column.physicalName}`);
        divergences.push("nullability-divergent");
      }
      if (typeof expected.primaryKey === "boolean" && expected.primaryKey !== column.primaryKey) {
        context.limit("projection-key-divergent", table.module, column.span.start.line,
          `${table.physicalName}.${column.physicalName}`);
        divergences.push("key-divergent");
      }
      pushBounded(rows, MAX_PROJECTION_ROWS, {
        table: table.physicalName,
        column: column.physicalName,
        status: divergences.length === 0 ? "matched" : divergences.join("+"),
        declared: column.typeToken,
        projected: typeof expected.type === "string" ? expected.type : null,
      }, overflow);
      if (overflow.hit) break;
    }
    for (const [name] of projectedColumns) {
      if (!table.columns.some((column) => column.physicalName === name)) {
        context.limit("projection-column-extra", table.module, null, `${table.physicalName}.${name}`);
        pushBounded(rows, MAX_PROJECTION_ROWS, {
          table: table.physicalName,
          column: name,
          status: "missing-in-declaration",
        }, overflow);
      }
    }
    if (overflow.hit) break;
  }
  if (!overflow.hit) {
    for (const [name] of projectedByName) {
      if (!context.tables.some((table) => table.physicalName === name)) {
        context.limit("projection-table-extra", null, null, name);
        pushBounded(rows, MAX_PROJECTION_ROWS, { table: name, status: "missing-in-declaration" }, overflow);
      }
    }
  }
  return { rows, state: "checked" };
}

/**
 * The pure staleness audit of previous bindings against a current
 * document (issue #116 AC6). A table rename changes the native id, so
 * old bindings invalidate; rebinding requires confirmed rename history,
 * which this adapter does not synthesize.
 */
export function auditStaleBindings(previous, current) {
  const rows = [];
  if (!isObject(previous) || !Array.isArray(previous.bindings)) return rows;
  const tablesNow = new Map((current?.tables ?? []).map((table) => [table.native, table]));
  for (const binding of previous.bindings) {
    if (binding?.status !== "confirmed") continue;
    const tableNow = tablesNow.get(binding.table?.native ?? "");
    if (tableNow === undefined) {
      rows.push({ entity: binding.entity, status: "stale", reason: "binding-stale-table-native-missing" });
      continue;
    }
    if (tableNow.fileDigest !== binding.table?.fileDigest) {
      rows.push({ entity: binding.entity, status: "stale", reason: "binding-stale-table-source-changed" });
      continue;
    }
    rows.push({ entity: binding.entity, status: "current", reason: null });
  }
  return rows;
}

// ---------------------------------------------------------------------------
// 11. Module walk orchestration and document assembly.
// ---------------------------------------------------------------------------

function newWalkState() {
  return {
    txScopes: new Map(),
    dbAliases: new Map(),
    /** resolved receiver symbol key → proven identity ("db"/"db-alias") */
    dbSymbols: new Map(),
    txCounter: { next: 1 },
    depth: 0,
  };
}

/**
 * The full pass: collect tables, relations, queries, transactions,
 * migrations and assemble the closed document.
 */
export function attachDrizzleEvidence({
  ts, checker, program, context, index,
  drizzleAttachment, drizzleClosure,
  manifest, readBytes,
  drizzleBindingsInput, drizzleProjectionInput,
}) {
  if (!drizzleAttachment?.attached || !drizzleClosure) {
    index.drizzle = null;
    // A project that DECLARES drizzle-orm but fails the pin policy is
    // not a non-drizzle project: record the rejection reason so hosts
    // can distinguish declared-but-unsupported from absent (issue #116
    // fix round). Plain absence stays unrecorded.
    if (drizzleAttachment && !drizzleAttachment.attached
      && (drizzleAttachment.reason === "pin-not-exact"
        || drizzleAttachment.reason === "pin-mismatch")) {
      index.drizzleAttachment = {
        attached: false,
        reason: drizzleAttachment.reason,
        declared: drizzleAttachment.declared ?? [],
        supported: drizzleAttachment.supported ?? null,
      };
    }
    return null;
  }
  const closurePrefix = `/lekalo/deps/drizzle-orm@${drizzleClosure.pin}/`;
  const limitations = [];
  const extract = new ExtractorContext({ ts, checker, closurePrefix, limitations });
  extract.modulePathOf = (sourceFile) => normalizePathForIndex(sourceFile, context) ?? sourceFile.fileName;
  extract.fileDigestOf = (sourceFile) => {
    const cached = extract.fileDigestCache.get(sourceFile.fileName);
    if (cached !== undefined) return cached;
    const digest = "sha256:" + sha256Hex(sourceFile.text);
    extract.fileDigestCache.set(sourceFile.fileName, digest);
    return digest;
  };
  const bindingsInput = drizzleBindingsInput?.value ?? null;
  const projectionRecord = drizzleProjectionInput ?? null;
  const projectionInput = projectionRecord?.value ?? null;
  if (bindingsInput !== null && drizzleBindingsInput.path !== null) {
    extract.noteFile(drizzleBindingsInput.path, "sha256:" + sha256Hex(bindingsInput));
  }
  if (projectionInput !== null && projectionRecord.path !== null) {
    extract.noteFile(projectionRecord.path, "sha256:" + sha256Hex(canonicalText(projectionInput)));
  }

  // Pass A: tables in every inventory module first, then relations —
  // a relations module may precede its schema module in program order,
  // so the table identity map must be complete before relations walk.
  for (const sourceFile of program.getSourceFiles()) {
    const modulePath = normalizePathForIndex(sourceFile, context);
    if (modulePath === null) continue;
    extract.noteFile(modulePath, extract.fileDigestOf(sourceFile));
    const walkTables = (node) => {
      if (node === undefined || node === null) return;
      if (node.kind === ts.SyntaxKind.VariableDeclaration
        && node.name?.kind === ts.SyntaxKind.Identifier
        && node.initializer?.kind === ts.SyntaxKind.CallExpression) {
        const exportName = node.name.text;
        const table = extractTable(extract, sourceFile, node.initializer, exportName);
        if (table !== null) {
          if (extract.tables.length < MAX_TABLES) {
            extract.tables.push(table);
            let symbol;
            try {
              symbol = checker.getSymbolAtLocation(node.name);
            } catch {
              symbol = null;
            }
            if (symbol !== null) extract.tableBySymbol.set(symbolKey(symbol), table);
            extract.tableByExport.set(exportName, table);
            if (table.physicalName !== null) {
              const bucket = extract.tableByPhysical.get(table.physicalName) ?? [];
              bucket.push(table);
              extract.tableByPhysical.set(table.physicalName, bucket);
            }
          } else {
            extract.limit("truncated", modulePath, lineOf(sourceFile, node), "tables");
            extract.overflow.hit = true;
          }
        } else if (calleeRecognizedButUnproven(extract, node.initializer.expression, TABLE_FACTORY_NAMES, "table")) {
          // A construct-named callee that could not be proven is an
          // explicit limitation, never a silent drop (issue #116 fix
          // round 3); the tables section cannot claim complete over it.
          extract.limit("callee-unproven", modulePath, lineOf(sourceFile, node), "table");
          extract.sectionGaps.add("tables");
        }
      }
      ts.forEachChild(node, walkTables);
    };
    walkTables(sourceFile);
  }
  for (const sourceFile of program.getSourceFiles()) {
    if (normalizePathForIndex(sourceFile, context) === null) continue;
    const walkRelations = (node) => {
      if (node === undefined || node === null) return;
      if (node.kind === ts.SyntaxKind.CallExpression) {
        // Callee identity decides, never the local callee shape or
        // spelling: identifier, namespace property access, and
        // const/let rebinding all resolve to the vendored declaration
        // (issue #116 fix round 3 — the relations path now has the
        // PropertyAccess treatment the table path already had).
        const symbol = node.expression?.kind === ts.SyntaxKind.Identifier
          || node.expression?.kind === ts.SyntaxKind.PropertyAccessExpression
          ? closureCalleeSymbol(extract, node.expression)
          : null;
        if (symbol !== null && symbol.getName() === "relations") {
          const rows = extractRelations(extract, sourceFile, node);
          if (rows !== null) {
            for (const row of rows) {
              if (!pushBounded(extract.relations, MAX_RELATIONS, row, extract.overflow)) break;
            }
          }
        } else if (calleeRecognizedButUnproven(extract, node.expression, RELATIONS_FACTORY_NAMES, "relations")) {
          // A construct-named callee that could not be proven is an
          // explicit limitation, never a silent drop (issue #116 fix
          // round 3).
          extract.limit("callee-unproven", extract.modulePathOf(sourceFile),
            lineOf(sourceFile, node), "relations");
          extract.sectionGaps.add("relations");
        }
      }
      ts.forEachChild(node, walkRelations);
    };
    walkRelations(sourceFile);
  }

  // Pass B: queries and transactions per module (stable file order).
  for (const sourceFile of program.getSourceFiles()) {
    const modulePath = normalizePathForIndex(sourceFile, context);
    if (modulePath === null) continue;
    const state = newWalkState();
    const visit = (node) => {
      if (node === undefined || node === null) return;
      if (node.kind === ts.SyntaxKind.CallExpression) {
        const { root, links } = flattenChain(ts, node);
        const tail = links[links.length - 1];
        const tailName = tail?.kind === "call" ? tail.name : null;
        if (tailName === "transaction") {
          const identity = rootIdentityKind(extract, root, state);
          if (identity === "namespace") {
            // A module namespace is provably not a transaction owner:
            // the member call stays an explicit uncertainty instead of
            // a silent drop (issue #116 fix round 4).
            extract.limit("namespace-receiver-unsupported", modulePath,
              lineOf(sourceFile, node), "transaction");
            extract.sectionGaps.add("transactions");
            return;
          }
          if (identity === "db" || identity === "db-alias") {
            const tx = extractTransaction(extract, sourceFile, node, "db", null, state);
            if (!pushBounded(extract.transactions, MAX_TRANSACTIONS, tx, extract.overflow)) {
              extract.limit("truncated", modulePath, lineOf(sourceFile, node), "transactions");
            }
            return; // members walked inside extractTransaction
          }
        }
        if (links.length > 0 && links[0].kind === "call"
          && (QUERY_HEADS.has(links[0].name) || CONTINUATION_HEADS.has(links[0].name))) {
          const identity = rootIdentityKind(extract, root, state);
          if (identity === "namespace") {
            // A db-spelled chain root that resolves to a module
            // namespace is provably not a handle: the query-shaped
            // chain is an explicit uncertainty, never a fabricated row
            // (issue #116 fix round 4).
            const shapePair = links.some((link) => link.kind === "call"
              && (link.name === "from" || link.name === "values" || link.name === "set"
                || link.name === "where"));
            if (shapePair) {
              extract.limit("namespace-receiver-unsupported", modulePath,
                lineOf(sourceFile, node), "query");
              extract.sectionGaps.add("queries");
            }
            return;
          }
          if (identity === "db" || identity === "db-alias" || identity === "tx") {
            const query = extractQueryChain(extract, sourceFile, { root, links }, identity,
              root.kind === ts.SyntaxKind.Identifier ? state.txScopes.get(root.text) ?? null : null);
            if (query !== null) {
              if (!pushBounded(extract.queries, MAX_QUERIES, query, extract.overflow)) {
                extract.limit("truncated", modulePath, lineOf(sourceFile, node), "queries");
              }
            }
            // Alias capture: `const q = db.select()...` — a later chain
            // rooted at `q` continues this builder.
            if (node.parent?.kind === ts.SyntaxKind.VariableDeclaration
              && node.parent.initializer === node
              && node.parent.name?.kind === ts.SyntaxKind.Identifier) {
              if (identity === "db") state.dbAliases.set(node.parent.name.text, true);
              const txId = root.kind === ts.SyntaxKind.Identifier
                ? state.txScopes.get(root.text) ?? null
                : null;
              if (identity === "tx" && txId !== null) {
                state.txScopes.set(node.parent.name.text, txId);
              }
            }
            return;
          }
          // Drizzle-shaped chain on an unknown receiver: explicit
          // incompleteness, never a silent drop.
          const shapePair = links.some((link) => link.kind === "call"
            && (link.name === "from" || link.name === "values" || link.name === "set"
              || link.name === "where"));
          if (shapePair) {
            extract.limit("receiver-unknown", modulePath, lineOf(sourceFile, node));
            extract.sectionGaps.add("queries");
          }
        }
        // Recognized Drizzle surfaces outside the qualified static
        // subset: the relational query API (`db.query.*`) and the batch
        // API (`db.batch`). They emit an explicit limitation with a
        // reason code and force the queries section partial — never a
        // silent drop (issue #116 fix round).
        if (links.length > 0
          && (links[0].kind === "access" && links[0].name === "query"
            || links[0].kind === "call" && links[0].name === "batch")) {
          const identity = rootIdentityKind(extract, root, state);
          if (identity === "namespace") {
            // `ns.batch(...)` / `ns.query.*` on a module namespace is
            // provably not a drizzle client surface: an explicit
            // namespace limitation, never a batch/relational
            // misattribution (issue #116 fix round 4).
            extract.limit("namespace-receiver-unsupported", modulePath,
              lineOf(sourceFile, node),
              links[0].name === "query" ? "relational-query-api" : "batch-api");
            extract.sectionGaps.add("queries");
            if (links[0].kind === "access") return;
            // A batch array may carry genuine drizzle chains: keep
            // walking so its member statements still extract.
          } else if (identity === "db" || identity === "db-alias" || identity === "tx") {
            const code = links[0].name === "query"
              ? "relational-query-unsupported"
              : "batch-unsupported";
            extract.limit(code, modulePath, lineOf(sourceFile, node),
              links[0].name === "query" ? "relational-query-api" : "batch-api");
            extract.sectionGaps.add("queries");
            if (links[0].kind === "access") return;
            // A batch array carries genuine drizzle chains: keep
            // walking so its member statements still extract.
          }
        }
        // Raw execution on a drizzle database handle: the statement is
        // opaque, so only the explicit warning exists — never a
        // fabricated query shape.
        if (tailName === "execute") {
          const identity = rootIdentityKind(extract, root, state);
          if (identity === "db" || identity === "db-alias") {
            extract.limit("raw-sql", modulePath, lineOf(sourceFile, node));
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }

  // Pass C: migrations, scope, bindings, projection comparison.
  extractMigrations(extract, ts, manifest, readBytes);
  const scopeRows = extractScopeRows(extract);

  let bindings = [];
  let bindingsState = "missing";
  if (bindingsInput !== null) {
    const decoded = decodeBindingsInput(bindingsInput);
    if (!decoded.ok) {
      extract.limit("bindings-input-invalid", null, null, decoded.reason);
      bindingsState = "invalid";
    } else {
      bindingsState = "decoded";
      const overflow = { hit: false };
      for (const entity of decoded.entities) {
        // Resolve by precise evidence, never by guessing a name
        // collision: several modules may export the same name (or
        // declare the same physical table), and confirming one of them
        // would be a guessed pick marked `confirmed` (issue #116 fix
        // round). A unique export match is exact; the physical-name
        // spelling only resolves when it is unambiguous too.
        const exportMatches = extract.tables.filter((table) => table.exportName === entity.table);
        const physicalMatches = extract.tableByPhysical.get(entity.table) ?? [];
        let tableRow = null;
        let ambiguous = false;
        if (exportMatches.length === 1) {
          tableRow = exportMatches[0];
        } else if (exportMatches.length > 1) {
          ambiguous = true;
        } else if (physicalMatches.length === 1) {
          tableRow = physicalMatches[0];
        } else if (physicalMatches.length > 1) {
          ambiguous = true;
        }
        if (ambiguous) {
          extract.limit("binding-ambiguous", null, null, `${entity.entity}->${entity.table}`);
          pushBounded(bindings, MAX_BINDINGS, {
            entity: entity.entity,
            table: { exportName: entity.table, native: null, physicalName: null, fileDigest: null },
            status: "ambiguous",
            basis: "explicit-owner-input",
            limitations: ["binding-ambiguous"],
          }, overflow);
          continue;
        }
        if (tableRow === null) {
          extract.limit("binding-unresolved", null, null, `${entity.entity}->${entity.table}`);
          pushBounded(bindings, MAX_BINDINGS, {
            entity: entity.entity,
            table: { exportName: entity.table, native: null, physicalName: null, fileDigest: null },
            status: "unresolved",
            basis: "explicit-owner-input",
            limitations: ["binding-unresolved"],
          }, overflow);
          continue;
        }
        pushBounded(bindings, MAX_BINDINGS, {
          entity: entity.entity,
          table: {
            exportName: tableRow.exportName,
            native: tableRow.native,
            physicalName: tableRow.physicalName,
            fileDigest: tableRow.fileDigest,
          },
          status: "confirmed",
          basis: "explicit-owner-input",
          limitations: [],
        }, overflow);
      }
      if (overflow.hit) extract.limit("truncated", null, null, "bindings");
    }
  } else if (drizzleBindingsInput && drizzleBindingsInput.reason !== "absent") {
    // The input exists but could not be read as text: an invalid input
    // is recorded as invalid, never mislabeled as missing (issue #116
    // fix round).
    extract.limit("bindings-input-invalid", null, null, drizzleBindingsInput.reason);
    bindingsState = "invalid";
  } else {
    extract.limit("bindings-input-missing", null, null);
  }

  let projectionRows = [];
  let projectionState = "absent";
  if (projectionInput !== null) {
    const comparison = compareProjection(extract, projectionInput);
    projectionRows = comparison.rows;
    projectionState = comparison.state;
  } else if (projectionRecord && projectionRecord.reason !== "absent") {
    // Malformed JSON / unreadable projection input: explicit invalid
    // state instead of a silent absence (issue #116 fix round).
    extract.limit("projection-input-invalid", null, null, projectionRecord.reason);
    projectionState = "invalid";
  }

  // Completeness per section: any row-level limitation, overflow,
  // unresolved input, or recognized-but-uncovered surface degrades the
  // section — completeness is never claimed over unknowns, and a
  // zero-row section can never hide a silent drop (issue #116 fix
  // round: the dead `partial:partial` ternaries are gone; sections
  // with all-clean rows are honestly complete).
  const partialFromRows = (rows) => rows.some((row) => (row.limitations?.length ?? 0) > 0);
  const sections = {
    tables: !extract.sectionGaps.has("tables")
      && extract.tables.every((table) => table.completeness === "complete")
      && !extract.overflow.hit
      ? "complete"
      : "partial",
    relations: extract.sectionGaps.has("relations") || partialFromRows(extract.relations)
      ? "partial"
      : "complete",
    queries: extract.sectionGaps.has("queries") || partialFromRows(extract.queries)
      ? "partial"
      : "complete",
    transactions: extract.sectionGaps.has("transactions") || partialFromRows(extract.transactions)
      ? "partial"
      : "complete",
    migrations: extract.migrations.every((row) => row.limitations.length === 0) ? "complete" : "partial",
    scope: "partial",
    bindings: bindingsState === "decoded" ? "complete" : "partial",
    projection: projectionState === "checked"
      ? (projectionRows.some((row) => row.status !== "matched") ? "partial" : "complete")
      : "partial",
  };

  const document = {
    schema: DRIZZLE_EVIDENCE_SCHEMA,
    identity: {
      adapter: "lekalo-target-node-typescript",
      extractor: DRIZZLE_EXTRACTOR_VERSION,
      ruleSet: DRIZZLE_RULESET,
      readOnly: true,
      orm: {
        name: "drizzle-orm",
        pin: drizzleClosure.pin,
        attached: true,
        declarationsDigest: drizzleClosure.digest,
        attachmentReason: drizzleAttachment.reason,
        declared: drizzleAttachment.declared ?? [],
      },
    },
    provenance: {
      // Mirrors the scanner's input manifest key: the full input
      // revision this evidence is fresh against. otherFiles are inputs
      // too — migration SQL/journal edits (same byte length or not)
      // must change the revision (issue #116 fix round, research §2).
      inputRevision: sha256Hex(canonicalText({
        sourceFiles: manifest.sourceFiles.length,
        configFiles: manifest.configFiles.length,
        packageFiles: manifest.packageFiles.length,
        otherFiles: manifest.otherFiles.length,
        totalBytes: manifest.totalBytes,
        files: [
          ...manifest.sourceFiles, ...manifest.configFiles,
          ...manifest.packageFiles, ...manifest.otherFiles,
        ],
      })),
      files: [...extract.contributingFiles.entries()]
        .map(([path, digest]) => ({ path, digest }))
        .sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0))
        .slice(0, MAX_PROVENANCE_FILES),
    },
    dialects: sortedUnique(extract.tables.map((table) => table.dialect)),
    tables: extract.tables,
    relations: extract.relations,
    queries: extract.queries,
    transactions: extract.transactions,
    migrations: extract.migrations,
    scope: scopeRows,
    bindings,
    projection: { state: projectionState, rows: projectionRows },
    completeness: {
      state: Object.values(sections).every((value) => value === "complete") && !extract.overflow.hit
        ? "complete"
        : "partial",
      sections,
      // A declared schema never proves live database state.
      databaseState: "unknown",
    },
    limitations: limitations
      .slice()
      .sort((left, right) => (canonicalText(left) < canonicalText(right) ? -1 : 1))
      .slice(0, MAX_LIMITATIONS),
  };
  document.digest = "sha256:" + sha256Hex(canonicalText(document));
  index.drizzle = document;
  return document;
}

function normalizePathForIndex(sourceFile, context) {
  // Mirror the scanner's inventory normalization: only files the read
  // view enumerates are project modules; embedded deps never are.
  const fileName = sourceFile.fileName.split("\\").join("/");
  const inventorySet = context.inventorySet;
  if (inventorySet.has(fileName)) return inventorySet.get(fileName);
  const lowered = fileName.toLowerCase();
  if (inventorySet.has(lowered)) return inventorySet.get(lowered);
  return null;
}

/** The evidence summary the scan operation's internal envelope carries. */
export function drizzleEvidenceSummary(document) {
  if (document === null || document === undefined) return null;
  return {
    digest: document.digest,
    completeness: document.completeness.state,
    counts: {
      tables: document.tables.length,
      relations: document.relations.length,
      queries: document.queries.length,
      transactions: document.transactions.length,
      limitations: document.limitations.length,
    },
  };
}
