/**
 * The Hono framework provider (issue #115) — orchestration and app
 * discovery.
 *
 * Everything here is compiler-derived: instances come from resolved
 * `new Hono()` constructors whose import specifier is a reserved Hono
 * package specifier AND whose class symbol resolves into an inventoried
 * declaration file. Name resemblance alone is never evidence, lookalike
 * locals are rejected, and unresolved constructors become uncertainty —
 * never fabricated applications. No regex claims; no source mutation;
 * no evaluation of project code.
 *
 * The provider is namespaced adapter evidence: records live under
 * `dev.lekalo.hono/` (see hono-evidence.mjs) and are attached to the
 * internal scan index only when the trusted framework policy enables
 * the provider. With the provider disabled or the policy absent, the
 * generic scan index is byte-identical to a Hono-unaware scanner.
 */
import { createHash } from "node:crypto";

import { ADAPTER_VERSION } from "./kernel.mjs";
import { importDeclarationOfSymbol } from "./hono-context.mjs";
import {
  HONO_RULES_REVISION,
  HONO_SPECIFIERS,
  MAX_HONO_RECORDS,
  MAX_HONO_UNCERTAINTY,
  canonicalHonoText,
  makeRecord,
  makeUncertainty,
  sortHonoRecords,
  validateHonoRecords,
} from "./hono-evidence.mjs";
import { collectRegistrations, resolveComposition } from "./hono-routes.mjs";
import { buildMiddlewareChains } from "./hono-middleware.mjs";
import { collectHttpEvidence } from "./hono-http.mjs";
import { collectTestBindings } from "./hono-tests.mjs";
import { joinEndpointContracts, joinServiceCalls, resolveEndpointContracts } from "./hono-bindings.mjs";

export const HONO_PROVIDER_ID = "hono";

/** The one openapi-flavored constructor vocabulary entry. */
const CONSTRUCTOR_VOCABULARY = Object.freeze({
  Hono: { specifier: "hono", kind: "app" },
  OpenAPIHono: { specifier: "@hono/zod-openapi", kind: "openapi" },
});

/** Hard bounds: composition depth, chain length, per-handler work.
 * The canonical constants live in hono-evidence.mjs (shared leaf); these
 * re-exports keep the historical import surface stable. */
export { HONO_MAX_MOUNT_DEPTH, HONO_MAX_CHAIN } from "./hono-evidence.mjs";
export const HONO_MAX_BODY_NODES = 4096;
export const HONO_MAX_HANDLER_DIGEST_BYTES = 8192;

function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

/** Cheap kind aliasing keeps the walk readable without `any` casts. */

/**
 * Resolve the identifier expression of a constructor call to a Hono
 * framework class, or null when it is not one. Requires BOTH: an
 * import whose module specifier is the reserved package specifier for
 * that class name, AND a compiler-resolved class declaration inside an
 * inventoried declaration file. Unknown/lookalike shapes return null.
 */
function resolveFrameworkConstructor(node, sourceFile, ctx) {
  const { ts, checker } = ctx;
  if (node.kind !== ts.SyntaxKind.Identifier) return null;
  const name = node.text;
  const vocabulary = CONSTRUCTOR_VOCABULARY[name];
  if (!vocabulary) return null;
  let symbol = checker.getSymbolAtLocation(node);
  if (!symbol) return null;
  // The use-site binding must be an import (or an immutable alias of
  // one) whose specifier is exactly the reserved package specifier.
  const importSpecifierText = importSpecifierOf(ts, checker, symbol, node, sourceFile);
  if (importSpecifierText !== vocabulary.specifier) {
    if (importSpecifierText !== null && HONO_SPECIFIERS.includes(importSpecifierText)) {
      // A Hono-family specifier, but the wrong one for this name: a
      // real resolution question, not silence.
      ctx.addUncertainty(sourceFile, "unresolved-constructor", `${name}:${importSpecifierText}`);
    }
    return null;
  }
  const root = rootSymbol(ts, checker, symbol);
  if (!root) {
    ctx.addUncertainty(sourceFile, "unresolved-constructor", `${name}:alias`);
    return null;
  }
  const classDeclaration = (root.declarations ?? []).find((declaration) =>
    declaration.kind === ctx.classKind
    && isInventoryDeclarationFile(declaration, ctx));
  if (!classDeclaration) {
    ctx.addUncertainty(sourceFile, "unresolved-constructor", `${name}:unresolved-declaration`);
    return null;
  }
  return { name, kind: vocabulary.kind, symbol: root, classDeclaration };
}

/** Root of an alias chain (import aliases, bounded). */
function rootSymbol(ts, checker, symbol) {
  let current = symbol;
  for (let depth = 0; depth < 8; depth += 1) {
    if (current.flags & ts.SymbolFlags.Alias) {
      try {
        current = checker.getAliasedSymbol(current);
      } catch {
        return current;
      }
    } else {
      return current;
    }
  }
  return current;
}

/**
 * The import specifier that binds this identifier at the use site, or
 * null when the identifier is not imported here. Immutable one-step
 * const aliases (`const App = Hono`) are followed; deeper or mutable
 * aliasing is reported, never guessed.
 */
function importSpecifierOf(ts, checker, symbol, node, sourceFile) {
  const importDeclaration = importDeclarationOfSymbol(ts, symbol);
  if (importDeclaration !== null) {
    return importDeclaration.moduleSpecifier.text;
  }
  const declarations = symbol.declarations ?? [];
  // One-step immutable local alias of an import.
  for (const declaration of declarations) {
    if (declaration.kind !== ts.SyntaxKind.VariableDeclaration) continue;
    const declared = declaredConstName(ts, declaration);
    if (declared !== null && declaration.initializer?.kind === ts.SyntaxKind.Identifier) {
      const initializerSymbol = checker.getSymbolAtLocation(declaration.initializer);
      if (initializerSymbol && initializerSymbol !== symbol) {
        return importSpecifierOf(ts, checker, initializerSymbol, declaration.initializer, sourceFile);
      }
    }
  }
  return null;
}

/** `const NAME = ...` — the exact declared name, else null. */
function declaredConstName(ts, declaration) {
  const list = declaration.parent;
  if (list?.kind !== ts.SyntaxKind.VariableDeclarationList) return null;
  if (!(list.flags & ts.NodeFlags.Const)) return null;
  const nameNode = declaration.name;
  if (nameNode.kind !== ts.SyntaxKind.Identifier) return null;
  return nameNode.text;
}

function isInventoryDeclarationFile(declaration, ctx) {
  const fileName = declaration.getSourceFile?.()?.fileName;
  if (typeof fileName !== "string") return false;
  if (!/\.d\.[cm]?ts$/.test(fileName)) return false;
  return ctx.normalizeModulePath(fileName) !== null;
}

/** The 1-based span of one node in its source file. */
export function spanOf(node, sourceFile) {
  const start = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const end = sourceFile.getLineAndCharacterOfPosition(node.getEnd());
  return {
    path: null, // bound by the orchestrator to the logical module path
    startLine: start.line + 1,
    startColumn: start.character + 1,
    endLine: end.line + 1,
    endColumn: end.character + 1,
  };
}

/**
 * Scan the project once with the Hono provider enabled. Returns the
 * provider envelope: provider state, closed evidence records, and
 * uncertainty. Throws nothing out of bounds — every anomaly is a
 * record or an uncertainty row, never silence.
 */
export function scanHonoProvider({ ts, checker, program, context, index, revision, readDataFile }) {
  if (typeof revision !== "string" || revision === "") {
    revision = "unknown-revision";
  }
  const classKind = ts.SyntaxKind.ClassDeclaration;
  const apps = [];
  const records = [];
  const uncertainty = [];
  let budgetExceeded = false;

  const addUncertaintyAt = (sourceFile, node, kind, detail) => {
    if (uncertainty.length >= MAX_HONO_UNCERTAINTY) {
      budgetExceeded = true;
      return;
    }
    const fromModule = context.normalizeModulePath(sourceFile.fileName);
    uncertainty.push(makeUncertainty(
      fromModule ?? "unknown",
      kind,
      detail,
      node === null
        ? null
        : sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile)).line + 1,
    ));
  };

  const addRecord = (record) => {
    if (records.length >= MAX_HONO_RECORDS) {
      budgetExceeded = true;
      return;
    }
    records.push(record);
  };

  const ctx = {
    ts,
    checker,
    program,
    context,
    index,
    revision,
    adapterVersion: ADAPTER_VERSION,
    frameworkVersion: "unknown",
    classKind,
    apps,
    records,
    uncertainty,
    instanceBySymbol: new Map(),
    instanceByNode: new Map(),
    validatorByNode: new Map(),
    addRecord,
    addUncertainty: (sourceFile, kind, detail) => addUncertaintyAt(sourceFile, null, kind, detail),
    addUncertaintyAt,
    spanOf: (node, file) => {
      const sourceFile = file ?? node.getSourceFile();
      return {
        ...spanOf(node, sourceFile),
        path: context.normalizeModulePath(sourceFile.fileName) ?? "unknown",
      };
    },
    normalizeModulePath: (fileName) => context.normalizeModulePath(fileName),
    symbolRowNative: (symbol) => context.symbolRows?.get(symbol)?.native ?? null,
    symbolRowOf: (symbol) => context.symbolRows?.get(symbol) ?? null,
    resolveLiteralString: (node, sourceFile) => resolveLiteralString(ctx, node, sourceFile, 0),
    instanceOfExpression: (expression, sourceFile) => instanceOfExpression(ctx, expression, sourceFile, 0),
    instanceSymbolOfExpression: (expression, sourceFile) =>
      instanceSymbolOfExpression(ctx, expression, sourceFile, 0),
    markBudgetExceeded: () => {
      budgetExceeded = true;
    },
    digestOfNode: (node, sourceFile) => {
      const text = node.getText(sourceFile ?? node.getSourceFile());
      return "sha256:" + sha256Hex(
        String(text).slice(0, HONO_MAX_HANDLER_DIGEST_BYTES)
        + `:${String(text).length}`,
      );
    },
    inlineNative: (sourceFile, node, role) => {
      const module = context.normalizeModulePath(sourceFile.fileName) ?? "unknown";
      const start = node.getStart(sourceFile);
      return `hono-inline-${sha256Hex([module, role ?? "inline", start, node.getEnd()].join("\0"))}`;
    },
    endpointContracts: resolveEndpointContracts(readDataFile, context.endpointContractPaths ?? []),
  };

  discoverInstances(ctx);
  const registrations = collectRegistrations(ctx);
  const { routes } = resolveComposition(ctx, registrations);
  ctx.routes = routes;
  buildMiddlewareChains(ctx, routes, registrations);
  collectHttpEvidence(ctx, routes, registrations);
  joinServiceCalls(ctx, routes);
  collectTestBindings(ctx, routes);
  joinEndpointContracts(ctx, routes);

  const violations = validateHonoRecords(records);
  for (const violation of violations.slice(0, 16)) {
    uncertainty.push(makeUncertainty(
      "hono", "invalid-record", violation.code ?? "invalid", null,
    ));
  }
  if (budgetExceeded) {
    uncertainty.push(makeUncertainty("hono", "record-budget", "records-or-uncertainty", null));
  }
  sortHonoRecords(records);
  uncertainty.sort((left, right) => {
    const a = Buffer.from(canonicalHonoText(left), "utf8");
    const b = Buffer.from(canonicalHonoText(right), "utf8");
    return a.compare(b);
  });

  const counts = {
    apps: apps.length,
    routes: routes.length,
    records: records.length,
    uncertainty: uncertainty.length,
  };
  const digest = "sha256:" + sha256Hex(canonicalHonoText({ records, uncertainty }));
  const state = violations.length > 0 || budgetExceeded
    ? "partial"
    : uncertainty.length > 0
      ? "partial"
      : apps.length === 0
        ? "empty"
        : "complete";
  return {
    provider: {
      id: HONO_PROVIDER_ID,
      rulesRevision: HONO_RULES_REVISION,
      state,
      frameworkVersion: ctx.frameworkVersion,
      counts,
      digest,
    },
    records,
    uncertainty,
  };
}

/**
 * Discover Hono instances: bound `new Hono()`/`new OpenAPIHono()`
 * constructors plus immutable const aliases and `basePath` views.
 * Every discovery is a full app-discovered record with a span.
 */
function discoverInstances(ctx) {
  const { ts, checker, program } = ctx;
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile) continue;
    const fromModule = ctx.normalizeModulePath(sourceFile.fileName);
    if (fromModule === null) continue;
    const visit = (node) => {
      if (node.kind === ts.SyntaxKind.NewExpression && node.expression?.kind === ts.SyntaxKind.Identifier) {
        const constructor = resolveFrameworkConstructor(node.expression, sourceFile, ctx);
        if (constructor) {
          registerInstance(ctx, constructor, node, node, sourceFile, fromModule);
        }
      } else if (node.kind === ts.SyntaxKind.VariableDeclaration) {
        // const app = new Hono() | const view = app.basePath('/x') | const alias = app
        const name = declaredConstName(ts, node);
        if (name === null) return;
        const initializer = node.initializer;
        if (!initializer) return;
        if (initializer.kind === ts.SyntaxKind.NewExpression && initializer.expression?.kind === ts.SyntaxKind.Identifier) {
          const constructor = resolveFrameworkConstructor(initializer.expression, sourceFile, ctx);
          if (constructor) {
            registerInstance(ctx, constructor, initializer, node, sourceFile, fromModule, node.name);
          }
        } else if (initializer.kind === ts.SyntaxKind.CallExpression
          && initializer.expression.kind === ts.SyntaxKind.PropertyAccessExpression) {
          const method = initializer.expression.name?.text;
          const receiverSymbol = instanceSymbolOfExpression(ctx, initializer.expression.expression, sourceFile, 0);
          if (method === "basePath" && receiverSymbol) {
            const base = ctx.resolveLiteralString(initializer.arguments?.[0], sourceFile);
            if (base === null) {
              ctx.addUncertaintyAt(sourceFile, initializer, "dynamic-path", "basePath");
              return;
            }
            bindDerivedView(ctx, node.name, sourceFile, fromModule, {
              kind: "view",
              ownerSymbol: receiverSymbol,
              basePath: base.value,
            }, initializer);
          }
        } else if (initializer.kind === ts.SyntaxKind.Identifier) {
          const target = instanceSymbolOfExpression(ctx, initializer, sourceFile, 0);
          if (target) {
            bindDerivedView(ctx, node.name, sourceFile, fromModule, {
              kind: "alias",
              ownerSymbol: target,
            }, initializer);
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
}

/**
 * Register one discovered instance. Bound variable instances carry
 * their indexed symbol; anonymous ones (`export default new Hono()`,
 * bare `new Hono().get(...)`) carry a node-derived stable key.
 */
function registerInstance(ctx, constructor, newNode, occurrence, sourceFile, fromModule, nameNode) {
  const { ts, checker } = ctx;
  // One `new Hono()` is one instance even when both the NewExpression
  // and its enclosing VariableDeclaration visit reach it.
  const registered = ctx.instanceByNode.get(newNode);
  if (registered) return registered;
  const span = ctx.spanOf(occurrence, sourceFile);
  let symbol = null;
  let name = constructor.name.toLowerCase();
  if (nameNode && nameNode.kind === ts.SyntaxKind.Identifier) {
    symbol = checker.getSymbolAtLocation(nameNode);
    name = nameNode.text;
    // A variable with more than one declaration (or any reassignment)
    // is mutable: aliases through it would guess. Keep the instance
    // discoverable but mark the binding incomplete.
    const declarations = symbol?.declarations ?? [];
    if (declarations.length > 1) {
      ctx.addUncertaintyAt(sourceFile, occurrence, "mutable-alias", name);
    } else if (hasReassignment(ts, sourceFile, name)) {
      ctx.addUncertaintyAt(sourceFile, occurrence, "mutable-alias", name);
    }
  }
  const instance = {
    key: symbol
      ? `sym:${name}@${symbol.declarations?.[0]?.getStart?.() ?? 0}`
      : `node:${fromModule}:${newNode.getStart(sourceFile)}`,
    kind: constructor.kind,
    symbol,
    module: fromModule,
    name,
    span,
    native: symbol ? (ctx.symbolRowNative(symbol) ?? ctx.inlineNative(sourceFile, newNode, "app")) : ctx.inlineNative(sourceFile, newNode, "app"),
    indexed: symbol ? ctx.symbolRowNative(symbol) !== null : false,
    declarationSpan: spanOf(constructor.classDeclaration, constructor.classDeclaration.getSourceFile()),
  };
  if (symbol && !ctx.instanceBySymbol.has(symbol)) {
    ctx.instanceBySymbol.set(symbol, instance);
  }
  if (!ctx.instanceByNode.has(newNode)) {
    ctx.instanceByNode.set(newNode, instance);
  }
  ctx.apps.push(instance);
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/app-discovered",
    from: { module: instance.module, native: instance.native, name: instance.name, indexed: instance.indexed },
    to: {
      module: instance.declarationSpan.path ?? instance.module,
      native: null,
      name: constructor.name,
      indexed: false,
      signature: null,
    },
    note: constructor.kind,
    provenance: "detected",
    confidence: symbol ? "exact" : "high",
    status: "complete",
    reasons: [],
    span,
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
  return instance;
}

/** Any `name = ...` write to this variable in the file → mutable. */
function hasReassignment(ts, sourceFile, name) {
  let found = false;
  const visit = (node) => {
    if (found) return;
    if ((node.kind === ts.SyntaxKind.BinaryExpression)
      && node.operatorToken?.kind === ts.SyntaxKind.EqualsToken
      && node.left?.kind === ts.SyntaxKind.Identifier
      && node.left.text === name) {
      found = true;
      return;
    }
    if (node.kind === ts.SyntaxKind.PostfixUnaryExpression
      || node.kind === ts.SyntaxKind.PrefixUnaryExpression) {
      if (node.operand?.kind === ts.SyntaxKind.Identifier && node.operand.text === name) found = true;
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return found;
}

function bindDerivedView(ctx, nameNode, sourceFile, fromModule, shape, occurrence) {
  const { ts, checker } = ctx;
  if (!nameNode || nameNode.kind !== ts.SyntaxKind.Identifier) return;
  const symbol = checker.getSymbolAtLocation(nameNode);
  if (!symbol) return;
  const span = ctx.spanOf(occurrence, sourceFile);
  const native = ctx.symbolRowNative(symbol) ?? ctx.inlineNative(sourceFile, occurrence, "view");
  const instance = {
    key: `sym:${nameNode.text}@${symbol.declarations?.[0]?.getStart?.() ?? 0}`,
    kind: shape.kind,
    symbol,
    module: fromModule,
    name: nameNode.text,
    span,
    native,
    indexed: ctx.symbolRowNative(symbol) !== null,
    ownerSymbol: shape.ownerSymbol,
    basePath: shape.kind === "view" ? shape.basePath : "",
  };
  if (!ctx.instanceBySymbol.has(symbol)) {
    ctx.instanceBySymbol.set(symbol, instance);
    ctx.apps.push(instance);
  }
}

/**
 * Resolve an expression to an instance symbol: direct identifiers and
 * one-hop immutable alias chains. Property receivers and call chains
 * that do not resolve are reported by the caller, never guessed.
 */
function instanceSymbolOfExpression(ctx, expression, sourceFile, depth) {
  const { ts, checker } = ctx;
  if (!expression || expression.kind !== ts.SyntaxKind.Identifier) return null;
  if (depth > 2) return null;
  const symbol = checker.getSymbolAtLocation(expression);
  if (!symbol) return null;
  const current = rootSymbol(ts, checker, symbol);
  if (ctx.instanceBySymbol.has(current)) return current;
  // Immutable const alias of an instance variable.
  for (const declaration of current.declarations ?? []) {
    if (declaration.kind === ts.SyntaxKind.VariableDeclaration && declaration.initializer?.kind === ts.SyntaxKind.Identifier) {
      if (declaredConstName(ts, declaration) === null) return null;
      const target = checker.getSymbolAtLocation(declaration.initializer);
      if (target) {
        const targetRoot = rootSymbol(ts, checker, target);
        if (ctx.instanceBySymbol.has(targetRoot)) return targetRoot;
      }
    }
  }
  return null;
}

/**
 * Resolve a receiver expression to a discovered instance record:
 * identifiers (and immutable aliases), inline `app.basePath('/x')`
 * views, and bare `new Hono()` receivers. Anything else is null.
 */
function instanceOfExpression(ctx, expression, sourceFile, depth) {
  const { ts } = ctx;
  if (!expression || depth > 2) return null;
  if (expression.kind === ts.SyntaxKind.Identifier) {
    const symbol = instanceSymbolOfExpression(ctx, expression, sourceFile, depth);
    return symbol ? ctx.instanceBySymbol.get(symbol) ?? null : null;
  }
  if (expression.kind === ts.SyntaxKind.NewExpression) {
    return ctx.instanceByNode.get(expression) ?? null;
  }
  if (expression.kind === ts.SyntaxKind.CallExpression
    && expression.expression.kind === ts.SyntaxKind.PropertyAccessExpression) {
    const method = expression.expression.name?.text;
    if (method !== "basePath") return null;
    const receiver = instanceOfExpression(ctx, expression.expression.expression, sourceFile, depth + 1);
    if (!receiver) return null;
    const base = ctx.resolveLiteralString(expression.arguments?.[0], sourceFile);
    if (base === null) {
      ctx.addUncertaintyAt(sourceFile, expression, "dynamic-path", "basePath");
      return null;
    }
    return {
      key: `${receiver.key}|basePath:${base.value}`,
      kind: "view",
      symbol: null,
      module: receiver.module,
      name: receiver.name,
      span: ctx.spanOf(expression, sourceFile),
      native: receiver.native,
      indexed: receiver.indexed,
      ownerSymbol: receiver.symbol,
      basePath: joinPaths(receiver.basePath ?? "", base.value),
    };
  }
  return null;
}

/** Join two literal path segments the way Hono concatenates them. */
export function joinPaths(prefix, suffix) {
  const left = prefix === "/" ? "" : prefix;
  const right = suffix === "/" ? "" : suffix;
  const joined = `${left ?? ""}${right ?? ""}`;
  return joined === "" ? "/" : joined;
}

/**
 * Statically resolve a string-valued expression WITHOUT evaluation:
 * literals, template literals without substitution holes, bounded
 * concatenations of known leaves, and immutable const aliases the
 * checker resolves to a literal. Returns `{ value, kind }` or null.
 */
function resolveLiteralString(ctx, node, sourceFile, depth) {
  if (!node || depth > 4) return null;
  const { ts, checker } = ctx;
  if (node.kind === ts.SyntaxKind.StringLiteral || node.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral) {
    return { value: node.text, kind: "literal" };
  }
  if (node.kind === ts.SyntaxKind.TemplateExpression) {
    let value = node.head?.text ?? "";
    for (const span of node.templateSpans ?? []) {
      const part = resolveLiteralString(ctx, span.expression, sourceFile, depth + 1);
      if (part === null) return null;
      value += part.value + (span.literal?.text ?? "");
    }
    return { value, kind: "template" };
  }
  if (node.kind === ts.SyntaxKind.BinaryExpression
    && node.operatorToken?.kind === ts.SyntaxKind.PlusToken) {
    const left = resolveLiteralString(ctx, node.left, sourceFile, depth + 1);
    const right = resolveLiteralString(ctx, node.right, sourceFile, depth + 1);
    if (left === null || right === null) return null;
    return { value: left.value + right.value, kind: "concat" };
  }
  if (node.kind === ts.SyntaxKind.Identifier) {
    const symbol = checker.getSymbolAtLocation(node);
    if (!symbol) return null;
    for (const declaration of symbol.declarations ?? []) {
      if (declaration.kind !== ts.SyntaxKind.VariableDeclaration) continue;
      if (declaredConstName(ts, declaration) === null) continue;
      const initializer = declaration.initializer;
      const resolved = resolveLiteralString(ctx, initializer, sourceFile, depth + 1);
      if (resolved !== null) return { value: resolved.value, kind: "const-alias" };
    }
    return null;
  }
  if (node.kind === ts.SyntaxKind.PropertyAccessExpression) {
    // Enum members and const objects the checker reduces to literals.
    const constant = checker.getConstantValue?.(node);
    if (typeof constant === "string") return { value: constant, kind: "const-alias" };
    const symbol = checker.getSymbolAtLocation(node);
    if (symbol) {
      for (const declaration of symbol.declarations ?? []) {
        if (declaration.kind === ts.SyntaxKind.EnumMember && declaration.initializer) {
          const resolved = resolveLiteralString(ctx, declaration.initializer, sourceFile, depth + 1);
          if (resolved !== null) return { value: resolved.value, kind: "const-alias" };
        }
        if (declaration.kind === ts.SyntaxKind.PropertyAssignment) {
          const resolved = resolveLiteralString(ctx, declaration.initializer, sourceFile, depth + 1);
          if (resolved !== null) return { value: resolved.value, kind: "const-alias" };
        }
      }
    }
    return null;
  }
  return null;
}
