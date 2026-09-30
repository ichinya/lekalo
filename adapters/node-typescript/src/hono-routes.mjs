/**
 * Hono route registrations, nested routers, and base paths (issue #115).
 *
 * Registration events are collected per call site with compiler-resolved
 * receivers and handlers (never regex), then composed deterministically:
 *
 * - every instance's own routes resolve under its standalone base;
 * - `route(path, child)` mounts resolve with Hono's documented
 *   mount-time snapshot semantics: child registrations that precede the
 *   mount occurrence in the SAME module are fully included; child
 *   registrations after the mount, or in other modules (where static
 *   analysis cannot prove initialization order), stay resolved records
 *   marked `incomplete` with a machine reason — never silently guessed;
 * - shared children compose at every mount occurrence; cycles and depth
 *   overruns are detected and recorded.
 *
 * Paths resolve without evaluation: literals, substitution-free
 * templates, bounded concatenations, and immutable const aliases the
 * checker reduces to strings. Anything else is a `dynamic-path`
 * unknown record with a span.
 */
import {
  HONO_MAX_MOUNT_DEPTH,
  makeRecord,
  makeUncertainty,
} from "./hono-evidence.mjs";
import { joinPaths } from "./hono-scanner.mjs";
import { importSpecifierTextAt } from "./hono-context.mjs";


/** The route-method vocabulary of the supported Hono surface. */
const ROUTE_METHODS = new Set(["get", "post", "put", "patch", "delete", "options", "head", "all", "on"]);
/** App-shaping methods (registration affecting, non-route). */
const APP_METHODS = new Set(["route", "use", "basePath", "onError", "notFound", "openapi"]);

/** Node kinds whose bodies defer execution (registrations inside them
 * are not proven to run at module initialization). */
const DEFERRING_KINDS = [
  "FunctionDeclaration",
  "FunctionExpression",
  "ArrowFunction",
  "MethodDeclaration",
  "Constructor",
  "GetAccessor",
  "SetAccessor",
];

/** Node kinds that gate execution of their children at runtime. */
const CONDITIONAL_KINDS = [
  "IfStatement",
  "SwitchStatement",
  "ConditionalExpression",
  "TryStatement",
  "CatchClause",
  "ForStatement",
  "ForOfStatement",
  "ForInStatement",
  "WhileStatement",
  "DoStatement",
];
/**
 * Walk every inventoried source file and collect raw registration
 * events on discovered instances. Returns the globally ordered event
 * list (module path bytes, then source position).
 */
export function collectRegistrations(ctx) {
  const { ts, checker, program } = ctx;
  const events = [];
  // Pass 1: the validator vocabulary across all files first, so route
  // events in any module can resolve const-bound validators by node
  // identity regardless of module enumeration order.
  precollectValidators(ctx);
  // Pass 2: registration events on discovered instances.
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile) continue;
    const fromModule = ctx.normalizeModulePath(sourceFile.fileName);
    if (fromModule === null) continue;
    const visit = (node) => {
      if (node.kind !== ts.SyntaxKind.CallExpression) {
        ts.forEachChild(node, visit);
        return;
      }
      const expression = node.expression;
      if (expression.kind !== ts.SyntaxKind.PropertyAccessExpression) {
        // ts.SyntaxKind.Identifier callees: the standalone vocabulary (createRoute,
        // validator, zValidator are imported directly).
        const calleeName = expression.kind === ts.SyntaxKind.Identifier ? expression.text : null;
        if (calleeName === "createRoute") {
          collectCreateRoute(ctx, node, sourceFile, fromModule, events);
        } else if (calleeName === "validator" || calleeName === "zValidator") {
          collectValidator(ctx, node, expression, sourceFile, fromModule, events);
        }
        ts.forEachChild(node, visit);
        return;
      }
      const methodName = expression.name?.text ?? null;
      if (methodName === null) {
        ts.forEachChild(node, visit);
        return;
      }
      const instance = ctx.instanceOfExpression(expression.expression, sourceFile);
      if (!instance) {
        reportNearMissReceiver(ctx, expression, sourceFile, fromModule);
        ts.forEachChild(node, visit);
        return;
      }
      if (!ROUTE_METHODS.has(methodName) && !APP_METHODS.has(methodName)) {
        ts.forEachChild(node, visit);
        return;
      }
      events.push(makeEvent(ctx, {
        kind: classifyMethod(methodName),
        node,
        instance,
        methodName,
        sourceFile,
        module: fromModule,
      }));
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
  events.sort((left, right) => {
    const byModule = compareUtf8(left.module, right.module);
    if (byModule !== 0) return byModule;
    return left.order - right.order;
  });
  for (let index = 0; index < events.length; index += 1) {
    events[index].order = index;
  }
  return events;
}

function classifyMethod(methodName) {
  if (ROUTE_METHODS.has(methodName)) return "route";
  if (methodName === "route") return "mount";
  if (methodName === "use") return "use";
  if (methodName === "onError" || methodName === "notFound") return "error";
  if (methodName === "openapi") return "openapi";
  return "other";
}

/**
 * Reachability classification of one registration site (issue #115
 * fix round): only module top-level straight-line code — or code in a
 * function the same module provably calls at top level — is proven to
 * run at initialization. Everything else is recorded with an explicit
 * reason and never emitted as a complete fact:
 *
 * - `top-level`: direct module body statement (proven);
 * - `called`: inside a named local function the module calls straight-
 *   line at top level (hoisted declarations, or const functions called
 *   after their declaration); arguments are irrelevant for closed-over
 *   registrations, but unproven calls never qualify;
 * - `conditional`: under `if`/`switch`/ternary/loop/try/`&&`/`||` —
 *   may or may not run (reason `conditional-registration`);
 * - `deferred`: inside a function body with no proven top-level call
 *   (reason `deferred-registration`);
 * - `unreachable`: preceded at the same block level by `return`/`throw`
 *   (reason `unreachable-registration`).
 */
function reachabilityOf(ctx, node, sourceFile) {
  const { ts } = ctx;
  let conditional = false;
  let current = node.parent;
  for (let depth = 0; current && current !== sourceFile && depth < 512; depth += 1) {
    if (DEFERRING_KINDS.includes(ts.SyntaxKind[current.kind])) {
      if (unreachableSiblingBefore(ctx, node, sourceFile)) return "unreachable";
      return functionIsProvenCalled(ctx, current, sourceFile) ? "called" : "deferred";
    }
    const kindName = ts.SyntaxKind[current.kind];
    if (CONDITIONAL_KINDS.includes(kindName)) conditional = true;
    if (kindName === "BinaryExpression"
      && (current.operatorToken?.kind === ts.SyntaxKind.AmpersandAmpersandToken
        || current.operatorToken?.kind === ts.SyntaxKind.BarBarToken
        || current.operatorToken?.kind === ts.SyntaxKind.QuestionQuestionToken)) {
      conditional = true;
    }
    current = current.parent;
  }
  if (unreachableSiblingBefore(ctx, node, sourceFile)) return "unreachable";
  return conditional ? "conditional" : "top-level";
}

/** Any earlier sibling of the enclosing statement that returns or throws. */
function unreachableSiblingBefore(ctx, node, sourceFile) {
  const { ts } = ctx;
  let current = node;
  while (current && current !== sourceFile) {
    const parent = current.parent;
    if (parent
      && (parent.kind === ts.SyntaxKind.Block || parent.kind === ts.SyntaxKind.SourceFile)
      && Array.isArray(parent.statements)) {
      for (const statement of parent.statements) {
        if (statement === current) break;
        if (statement.kind === ts.SyntaxKind.ReturnStatement || statement.kind === ts.SyntaxKind.ThrowStatement) {
          return true;
        }
      }
    }
    current = parent;
  }
  return false;
}

/**
 * Is this function-like ancestor a NAMED local function the same module
 * invokes straight-line at top level? Hoisted declarations qualify from
 * any top-level call; const-bound arrow/function expressions only from
 * calls that follow their declaration (no TDZ guessing).
 */
function functionIsProvenCalled(ctx, functionNode, sourceFile) {
  const { ts } = ctx;
  let name = null;
  let minCallStart = 0;
  if (functionNode.kind === ts.SyntaxKind.FunctionDeclaration && functionNode.name?.kind === ts.SyntaxKind.Identifier) {
    name = functionNode.name.text;
  } else if (functionNode.kind === ts.SyntaxKind.VariableDeclaration) {
    name = declaredConstIdentifierText(ts, functionNode);
  } else {
    const declaration = functionNode.parent;
    if (declaration?.kind === ts.SyntaxKind.VariableDeclaration) {
      const list = declaration.parent;
      if (list?.kind === ts.SyntaxKind.VariableDeclarationList && (list.flags & ts.NodeFlags.Const)) {
        name = declaration.name?.kind === ts.SyntaxKind.Identifier ? declaration.name.text : null;
        minCallStart = declaration.getStart(sourceFile);
      }
    }
  }
  if (name === null) return false;
  const counts = topLevelStraightLineCallsOf(ctx, sourceFile);
  const callStarts = counts.get(name);
  if (!callStarts || callStarts.length === 0) return false;
  return callStarts.some((start) => start > minCallStart);
}

function declaredConstIdentifierText(ts, declaration) {
  const list = declaration.parent;
  if (list?.kind !== ts.SyntaxKind.VariableDeclarationList) return null;
  if (!(list.flags & ts.NodeFlags.Const)) return null;
  return declaration.name?.kind === ts.SyntaxKind.Identifier ? declaration.name.text : null;
}

/**
 * Call expressions in the module's straight-line top-level body, per
 * callee name (position list). Function-like bodies and conditional
 * wrappers are skipped: a call under `if` proves nothing. Cached per
 * source file on the scan context.
 */
function topLevelStraightLineCallsOf(ctx, sourceFile) {
  const cached = ctx.topLevelCallCache ??= new WeakMap();
  if (cached.has(sourceFile)) return cached.get(sourceFile);
  const { ts } = ctx;
  const callsByName = new Map();
  const record = (name, start) => {
    if (!callsByName.has(name)) callsByName.set(name, []);
    callsByName.get(name).push(start);
  };
  const visit = (node) => {
    const kindName = ts.SyntaxKind[node.kind];
    if (DEFERRING_KINDS.includes(kindName) || CONDITIONAL_KINDS.includes(kindName)) return;
    if (node.kind === ts.SyntaxKind.CallExpression && node.expression?.kind === ts.SyntaxKind.Identifier) {
      record(node.expression.text, node.getStart(sourceFile));
      for (const argument of node.arguments ?? []) visit(argument);
      return;
    }
    ts.forEachChild(node, visit);
  };
  for (const statement of sourceFile.statements ?? []) visit(statement);
  cached.set(sourceFile, callsByName);
  return callsByName;
}

/**
 * The status/reason penalty of one event's reachability, or null when
 * the registration site is proven to run at initialization.
 */
export function reachabilityPenaltyOf(event) {
  switch (event.reachability) {
    case "conditional": return { reason: "conditional-registration", status: "incomplete" };
    case "deferred": return { reason: "deferred-registration", status: "incomplete" };
    case "unreachable": return { reason: "unreachable-registration", status: "unknown" };
    default: return null;
  }
}

function compareUtf8(left, right) {
  const a = Buffer.from(left, "utf8");
  const b = Buffer.from(right, "utf8");
  return a.compare(b);
}

/** Build one normalized registration event with resolved handlers. */
function makeEvent(ctx, { kind, node, instance, methodName, sourceFile, module }) {
  const { ts } = ctx;
  const args = node.arguments ?? [];
  const event = {
    kind,
    node,
    instance,
    methodName,
    sourceFile,
    module,
    order: node.getStart(sourceFile),
    path: null,
    pathKind: null,
    pathNode: null,
    methods: null,
    pathFilter: null,
    childInstance: null,
    handlers: [],
    inlineMiddleware: [],
    routeDefinition: null,
    status: "complete",
    reasons: [],
  };
  if (kind === "route") {
    if (methodName === "on") {
      const methodsArgument = args[0];
      const literals = literalStringArray(methodsArgument, ts);
      if (literals === null) {
        event.status = "unknown";
        event.reasons.push("dynamic-method");
        ctx.addUncertaintyAt(sourceFile, node, "dynamic-method", "on");
      } else {
        event.methods = literals;
      }
      event.pathNode = args[1] ?? null;
    } else {
      event.pathNode = args[0] ?? null;
      event.methods = [methodName.toUpperCase()];
    }
    resolvePathAndHandlers(ctx, event, args, methodName === "on" ? 2 : 1);
  } else if (kind === "mount") {
    event.pathNode = args[0] ?? null;
    const path = ctx.resolveLiteralString(event.pathNode, sourceFile);
    if (path === null) {
      event.status = "unknown";
      event.reasons.push("dynamic-path");
    } else {
      event.path = path.value;
      event.pathKind = path.kind;
    }
    const childExpression = args[1] ?? null;
    if (childExpression) {
      const child = ctx.instanceOfExpression(childExpression, sourceFile);
      if (child) {
        event.childInstance = child;
      } else {
        event.status = "unknown";
        event.reasons.push("unknown-handler");
        ctx.addUncertaintyAt(sourceFile, node, "unknown-handler", "mount-target");
      }
    } else {
      event.status = "unknown";
      event.reasons.push("unknown-handler");
    }
  } else if (kind === "use") {
    const first = args[0] ?? null;
    const firstIsPath = first !== null
      && (first.kind === ts.SyntaxKind.StringLiteral || first.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral);
    let handlerStart = 0;
    if (firstIsPath) {
      const filter = ctx.resolveLiteralString(first, sourceFile);
      event.pathFilter = filter === null ? null : filter.value;
      handlerStart = 1;
    }
    event.handlers = resolveHandlerChain(ctx, args, handlerStart, sourceFile);
  } else if (kind === "error" || kind === "openapi") {
    if (kind === "openapi") {
      event.routeDefinition = args[0] ?? null;
      event.handlers = resolveHandlerChain(ctx, args, 1, sourceFile);
    } else {
      event.handlers = resolveHandlerChain(ctx, args, 0, sourceFile);
    }
  }
  // Reachability: a registration site that is not proven to run at
  // module initialization is never a complete fact (issue #115 fix).
  event.reachability = reachabilityOf(ctx, node, sourceFile);
  const penalty = reachabilityPenaltyOf(event);
  if (penalty) {
    event.reasons.push(penalty.reason);
    if (event.status === "complete") event.status = penalty.status;
  }
  return event;
}

function resolvePathAndHandlers(ctx, event, args, handlerStart) {
  const path = ctx.resolveLiteralString(event.pathNode, event.sourceFile);
  if (path === null) {
    event.status = "unknown";
    event.reasons.push("dynamic-path");
    ctx.addUncertaintyAt(event.sourceFile, event.node, "dynamic-path", event.methodName);
  } else {
    event.path = path.value;
    event.pathKind = path.kind;
  }
  event.handlers = resolveHandlerChain(ctx, args, handlerStart, event.sourceFile);
}

/**
 * Resolve handler arguments (starting at `handlerStart`) to closed
 * endpoints. Referenced handlers bind to their indexed native symbols;
 * inline functions get stable occurrence keys; unknown shapes become
 * uncertainty, never a guessed symbol.
 */
export function resolveHandlerChain(ctx, args, handlerStart, sourceFile) {
  const chain = [];
  for (let index = handlerStart; index < (args?.length ?? 0); index += 1) {
    const endpoint = resolveHandlerEndpoint(ctx, args[index], sourceFile);
    if (endpoint !== null) chain.push(endpoint);
  }
  return chain;
}

/**
 * One handler argument → closed endpoint record. Call-expression
 * handlers are kept as validator references when the call is a known
 * validator construction; other factories are unknown.
 */
export function resolveHandlerEndpoint(ctx, argNode, sourceFile) {
  if (!argNode) return null;
  const { ts, checker } = ctx;
  if (argNode.kind === ts.SyntaxKind.ArrowFunction || argNode.kind === ts.SyntaxKind.FunctionExpression) {
    return {
      kind: "inline",
      node: argNode,
      module: ctx.normalizeModulePath(sourceFile.fileName),
      native: ctx.inlineNative(sourceFile, argNode, "handler"),
      name: "inline",
      indexed: false,
      signature: null,
      digest: ctx.digestOfNode(argNode, sourceFile),
    };
  }
  if (argNode.kind === ts.SyntaxKind.Identifier || argNode.kind === ts.SyntaxKind.PropertyAccessExpression) {
    const symbol = rootSymbolOf(ctx, argNode);
    if (symbol) {
      // A const-bound validator (`const v = zValidator(...)` used by
      // reference) keeps its validator identity through the variable.
      for (const declaration of symbol.declarations ?? []) {
        if (declaration.kind === ts.SyntaxKind.VariableDeclaration
          && declaration.initializer
          && ctx.validatorByNode.has(declaration.initializer)) {
          const validator = ctx.validatorByNode.get(declaration.initializer);
          return {
            kind: "validator",
            node: argNode,
            validator: { ...validator, node: declaration.initializer },
          };
        }
      }
      const row = ctx.symbolRowOf(symbol);
      if (row) {
        const declarationNode = symbol.declarations?.[0] ?? argNode;
        return {
          kind: "reference",
          node: argNode,
          symbol,
          module: row.module,
          native: row.native,
          name: row.qualifiedName,
          indexed: true,
          signature: row.signature,
          digest: ctx.digestOfNode(declarationNode, declarationNode.getSourceFile()),
        };
      }
      return {
        kind: "local",
        node: argNode,
        symbol,
        module: ctx.normalizeModulePath(sourceFile.fileName),
        native: ctx.inlineNative(sourceFile, argNode, "local-handler"),
        name: symbol.name ?? "local",
        indexed: false,
        signature: null,
        digest: ctx.digestOfNode(symbol.declarations?.[0] ?? argNode, sourceFile),
        reason: "local-handler",
      };
    }
  }
  if (argNode.kind === ts.SyntaxKind.CallExpression) {
    // A constructed handler (validator middleware or a factory).
    const validator = ctx.validatorByNode.get(argNode);
    if (validator) {
      return { kind: "validator", node: argNode, validator };
    }
    ctx.addUncertaintyAt(sourceFile, argNode, "unknown-handler", "factory");
    return {
      kind: "unknown",
      node: argNode,
      module: ctx.normalizeModulePath(sourceFile.fileName),
      native: ctx.inlineNative(sourceFile, argNode, "unknown-handler"),
      name: "unknown",
      indexed: false,
      signature: null,
      digest: ctx.digestOfNode(argNode, sourceFile),
      reason: "unknown-handler",
    };
  }
  ctx.addUncertaintyAt(sourceFile, argNode, "unknown-handler", String(argNode.kind));
  return null;
}

function rootSymbolOf(ctx, node) {
  const { ts, checker } = ctx;
  const target = node.kind === ts.SyntaxKind.PropertyAccessExpression ? node.name : node;
  let symbol = checker.getSymbolAtLocation(target);
  if (!symbol) return null;
  for (let depth = 0; depth < 8; depth += 1) {
    if (symbol.flags & ts.SymbolFlags.Alias) {
      try {
        symbol = checker.getAliasedSymbol(symbol);
      } catch {
        return symbol;
      }
    } else {
      return symbol;
    }
  }
  return symbol;
}

/** `['GET', 'POST']` / `'GET'` → literal string array, else null. */
function literalStringArray(node, ts) {
  if (!node) return null;
  if (node.kind === ts.SyntaxKind.StringLiteral || node.kind === ts.SyntaxKind.NoSubstitutionTemplateLiteral) {
    return [node.text.toUpperCase()];
  }
  if (node.kind !== ts.SyntaxKind.ArrayLiteralExpression) return null;
  const values = [];
  for (const element of node.elements ?? []) {
    if (element.kind !== ts.SyntaxKind.StringLiteral && element.kind !== ts.SyntaxKind.NoSubstitutionTemplateLiteral) {
      return null;
    }
    values.push(element.text.toUpperCase());
  }
  return values;
}

/**
 * A non-instance receiver on a Hono-shaped method: only worth an
 * uncertainty when the receiver's compiler TYPE is a Hono class (a
 * real lost instance, e.g. produced by an untracked factory). Plain
 * non-Hono `.get()`/`.post()` calls on other types stay silent.
 */
function reportNearMissReceiver(ctx, expression, sourceFile, fromModule) {
  const methodName = expression.name?.text;
  if (!ROUTE_METHODS.has(methodName) && !APP_METHODS.has(methodName)) return;
  const { checker } = ctx;
  const receiverType = checker.getTypeAtLocation(expression.expression);
  const typeName = receiverType?.symbol?.name;
  if (typeName === "Hono" || typeName === "OpenAPIHono") {
    ctx.addUncertaintyAt(sourceFile, expression.parent ?? expression, "unsupported-receiver", methodName);
  }
}

/**
 * `createRoute({method, path, operationId, ...})` from the pinned
 * OpenAPI vocabulary: literal properties only, one bounded record.
 */
function collectCreateRoute(ctx, node, sourceFile, fromModule, events) {
  const { ts } = ctx;
  const specifier = importSpecifierTextAt(ctx, node.expression);
  if (specifier !== "@hono/zod-openapi") return;
  const config = node.arguments?.[0];
  if (!config || config.kind !== ts.SyntaxKind.ObjectLiteralExpression) {
    ctx.addUncertaintyAt(sourceFile, node, "dynamic-path", "createRoute-config");
    return;
  }
  const read = (propertyName) => {
    for (const property of config.properties ?? []) {
      if (property.kind === ts.SyntaxKind.PropertyAssignment
        && property.name?.kind === ts.SyntaxKind.Identifier
        && property.name.text === propertyName) {
        return property.initializer ?? null;
      }
    }
    return null;
  };
  const methodNode = read("method");
  const pathNode = read("path");
  const operationIdNode = read("operationId");
  const definition = {
    node,
    module: fromModule,
    method: methodNode && (methodNode.kind === ts.SyntaxKind.StringLiteral) ? methodNode.text.toUpperCase() : null,
    path: pathNode ? (ctx.resolveLiteralString(pathNode, sourceFile)?.value ?? null) : null,
    operationId: operationIdNode && operationIdNode.kind === ts.SyntaxKind.StringLiteral ? operationIdNode.text : null,
    request: read("request"),
    responses: read("responses"),
  };
  if (definition.method === null || definition.path === null) {
    ctx.addUncertaintyAt(sourceFile, node, "dynamic-path", "createRoute");
  }
  events.push({
    kind: "route-definition",
    node,
    instance: null,
    methodName: "createRoute",
    sourceFile,
    module: fromModule,
    order: node.getStart(sourceFile),
    definition,
    handlers: [],
    status: "complete",
    reasons: [],
  });
}

/**
 * The validator pre-pass: every recognized validator construction is
 * remembered by node identity before route events are collected.
 */
function precollectValidators(ctx) {
  const { ts, checker, program } = ctx;
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile) continue;
    const fromModule = ctx.normalizeModulePath(sourceFile.fileName);
    if (fromModule === null) continue;
    const visit = (node) => {
      if (node.kind === ts.SyntaxKind.CallExpression) {
        const expression = node.expression;
        const calleeName = expression.kind === ts.SyntaxKind.Identifier ? expression.text : null;
        if (calleeName === "validator" || calleeName === "zValidator") {
          collectValidator(ctx, node, expression, sourceFile, fromModule, null);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
}

/**
 * `zValidator(target, schema)` / `validator(target, schema)`: the
 * constructed middleware is remembered by node so chains can bind it.
 */
function collectValidator(ctx, node, expression, sourceFile, fromModule, events) {
  const { ts } = ctx;
  const methodName = expression.kind === ts.SyntaxKind.Identifier
    ? expression.text
    : expression.name?.text ?? null;
  const specifier = importSpecifierTextAt(ctx, expression);
  const expected = methodName === "zValidator" ? "@hono/zod-validator" : "hono/validator";
  if (specifier !== expected) return;
  const args = node.arguments ?? [];
  const target = args[0]?.kind === ts.SyntaxKind.StringLiteral ? args[0].text : null;
  if (target === null) {
    ctx.addUncertaintyAt(sourceFile, node, "unresolved-validator-target", methodName);
  }
  const schemaNode = args[1] ?? null;
  const validator = {
    node,
    module: fromModule,
    target,
    schemaNode,
    kind: methodName,
  };
  ctx.validatorByNode.set(node, validator);
  if (events !== null) {
    events.push({
      kind: "validator",
      node,
      instance: null,
      methodName,
      sourceFile,
      module: fromModule,
      order: node.getStart(sourceFile),
      validator,
      handlers: [],
      status: "complete",
      reasons: [],
    });
  }
}


// ---------------------------------------------------------------------------
// Composition: standalone resolution, mounts, base paths.
// ---------------------------------------------------------------------------

/**
 * Resolve all registrations into full route records. Returns
 * `{ resolved, routes }` where `resolved` is the flat list of resolved
 * route facts consumed by later phases and `routes` mirrors it.
 */
export function resolveComposition(ctx, events) {
  const { ts } = ctx;
  const routeEvents = events.filter((event) => event.kind === "route");
  const mountEvents = events.filter((event) => event.kind === "mount");
  const useEvents = events.filter((event) => event.kind === "use");
  const errorEvents = events.filter((event) => event.kind === "error");
  const openapiEvents = events.filter((event) => event.kind === "openapi");
  const definitions = events.filter((event) => event.kind === "route-definition");

  const mountedChildren = new Set(mountEvents.map((event) => event.childInstance?.key).filter(Boolean));
  const routes = [];

  // 1. Error-handler and openapi registrations resolve app-level.
  // Reachability of the registration site carries into the record:
  // a conditional onError is bounded evidence, never a complete fact.
  for (const event of errorEvents) {
    const penalty = reachabilityPenaltyOf(event);
    for (const handler of event.handlers) {
      const handlerReason = handler.reason ? [handler.reason] : [];
      const complete = penalty === null && handlerReason.length === 0;
      ctx.addRecord(makeRecord({
        relation: "dev.lekalo.hono/handles-error",
        from: instanceEndpoint(event.instance),
        to: endpointOf(handler),
        note: event.methodName,
        provenance: "detected",
        confidence: handler.reason === undefined ? "exact" : "high",
        status: complete ? "complete" : penalty?.status === "unknown" ? "unknown" : "incomplete",
        reasons: [...(penalty ? [penalty.reason] : []), ...handlerReason],
        span: ctx.spanOf(event.node, event.sourceFile),
        revision: ctx.revision,
        adapterVersion: ctx.adapterVersion,
        frameworkVersion: ctx.frameworkVersion,
      }));
    }
  }

  // 2. Base-path views.
  for (const app of ctx.apps) {
    if (app.kind !== "view" || !app.basePath) continue;
    const owner = app.ownerSymbol ? ctx.instanceBySymbol.get(app.ownerSymbol) : null;
    ctx.addRecord(makeRecord({
      relation: "dev.lekalo.hono/base-path",
      from: owner ? instanceEndpoint(owner) : null,
      to: instanceEndpoint(app),
      path: app.basePath,
      provenance: "detected",
      confidence: "exact",
      status: "complete",
      reasons: [],
      span: app.span,
      revision: ctx.revision,
      adapterVersion: ctx.adapterVersion,
      frameworkVersion: ctx.frameworkVersion,
    }));
  }

  // 3. Standalone routes for roots never mounted into anything.
  const standaloneOwners = new Map();
  for (const event of routeEvents) {
    const key = event.instance.key;
    if (mountedChildren.has(key)) continue;
    if (!standaloneOwners.has(key)) standaloneOwners.set(key, event.instance);
  }
  for (const [key, instance] of standaloneOwners) {
    resolveInto(ctx, routeEvents, instance, "/", {
      base: standaloneBaseOf(ctx, instance),
      status: "complete",
      reasons: [],
      origin: "standalone",
    }, routes, 0);
  }

  // 4. Mount composition with proven-order snapshot semantics.
  const closureOf = importClosureOf(ctx);
  const rootMounts = mountEvents.filter((event) => {
    // A mount is a root mount when its parent is not itself the child
    // of another mount in the same module (nested mounts are reached
    // recursively from the outermost parent's resolution).
    return !isNestedMount(mountEvents, event);
  });
  for (const mount of rootMounts) {
    resolveMount(ctx, mount, mountEvents, routeEvents, routes, 0, [], closureOf, "/");
  }
  // 5. Mounted children keep their own deeper mounts resolved through
  // step 4's recursion; nothing standalone remains here.

  // 6. OpenAPI route definitions and app registrations.
  emitOpenApiRecords(ctx, openapiEvents, definitions);

  return { routes, events };
}

/**
 * The transitive import closure of one module (bounded, cycle-safe):
 * the set of modules whose top-level body provably evaluates before
 * this module's body in straight-line ESM. Built once per scan from
 * the compiler-resolved import reference rows.
 */
function importClosureOf(ctx) {
  const cached = new Map();
  const adjacency = new Map();
  for (const row of ctx.index?.references ?? []) {
    if (row.role !== "reference" || !row.from || !row.to) continue;
    if (!adjacency.has(row.from)) adjacency.set(row.from, new Set());
    adjacency.get(row.from).add(row.to);
  }
  const closureOf = (module) => {
    if (cached.has(module)) return cached.get(module);
    const closure = new Set();
    const stack = [[module, 0]];
    while (stack.length > 0) {
      const [current, depth] = stack.pop();
      if (depth > 16 || closure.has(current)) continue;
      if (current !== module) closure.add(current);
      for (const next of adjacency.get(current) ?? []) {
        stack.push([next, depth + 1]);
      }
    }
    cached.set(module, closure);
    return closure;
  };
  return closureOf;
}


function isNestedMount(mountEvents, event) {
  // An event whose parent instance is a mounted child is reached
  // through that child's mount resolution, not as a root.
  return event.instance.kind === "view" || event.instance.kind === "alias"
    ? false
    : mountEvents.some((other) =>
      other !== event && other.childInstance?.key === event.instance.key);
}

/** The standalone base prefix of an instance (views compose). */
function standaloneBaseOf(ctx, instance, seen = new Set()) {
  if (!instance || seen.has(instance.key)) return "/";
  seen.add(instance.key);
  if (instance.kind === "view") {
    const owner = instance.ownerSymbol ? ctx.instanceBySymbol.get(instance.ownerSymbol) : null;
    return joinPaths(standaloneBaseOf(ctx, owner, seen), instance.basePath ?? "/");
  }
  if (instance.kind === "alias") {
    const owner = instance.ownerSymbol ? ctx.instanceBySymbol.get(instance.ownerSymbol) : null;
    return standaloneBaseOf(ctx, owner, seen);
  }
  return "/";
}

/**
 * Resolve one mount: emit the mounts-router record and the child's
 * route events under the mount prefix with snapshot semantics, then
 * recurse into the child's own mounts. `basePrefix` carries the full
 * ancestor base chain: a mount nested two levels deep composes
 * root prefix + parent standalone base + every mount path, so
 * `api2.route('/v1', v1)` + `app.route('/api2', api2)` resolves
 * `/api2/v1/...` — never the child's prefix alone.
 */
function resolveMount(ctx, mount, mountEvents, routeEvents, routes, depth, stack, closureOf, basePrefix) {
  if (depth >= HONO_MAX_MOUNT_DEPTH) {
    ctx.addUncertaintyAt(mount.sourceFile, mount.node, "composition-depth", String(depth));
    return;
  }
  if (stack.some((key) => key === mount.childInstance?.key)) {
    ctx.addUncertaintyAt(mount.sourceFile, mount.node, "composition-cycle", mount.path ?? "");
    return;
  }
  const parentBase = joinPaths(
    basePrefix ?? "/",
    mount.instance.kind === "view" || mount.instance.kind === "alias"
      ? standaloneBaseOf(ctx, mount.instance)
      : "/",
  );
  const mountBase = joinPaths(parentBase, mount.path ?? "/");
  if (mount.childInstance) {
    ctx.addRecord(makeRecord({
      relation: "dev.lekalo.hono/mounts-router",
      from: instanceEndpoint(mount.instance),
      to: instanceEndpoint(mount.childInstance),
      path: mount.path,
      provenance: "detected",
      confidence: "exact",
      status: mount.status === "unknown" ? "unknown" : mount.status === "complete" ? "complete" : "incomplete",
      reasons: mount.reasons,
      span: ctx.spanOf(mount.node, mount.sourceFile),
      revision: ctx.revision,
      adapterVersion: ctx.adapterVersion,
      frameworkVersion: ctx.frameworkVersion,
    }));
    const child = mount.childInstance;
    const stackNext = [...stack, child.key];
    // Snapshot: child route events before the mount occurrence in the
    // same module are fully included; later/other-module events stay
    // resolved but incomplete.
    for (const event of routeEvents) {
      if (event.instance.key !== child.key) continue;
      const included = classifyChildEvent(mount, event, closureOf);
      if (included === null) continue;
      // Per-event scope: each child registration carries its own
      // ordering classification under this mount occurrence.
      resolveOneRoute(ctx, event, {
        base: mountBase,
        status: included.status,
        reasons: included.reasons,
        origin: `mount:${mount.module}:${mount.order}`,
        mount,
      }, routes);
    }
    for (const nested of mountEvents) {
      if (nested.instance.key !== child.key) continue;
      resolveMount(ctx, nested, mountEvents, routeEvents, routes, depth + 1, stackNext, closureOf, mountBase);
    }
  } else {
    ctx.addRecord(makeRecord({
      relation: "dev.lekalo.hono/mounts-router",
      from: instanceEndpoint(mount.instance),
      to: null,
      path: mount.path,
      provenance: "detected",
      confidence: "low",
      status: "unknown",
      reasons: mount.reasons,
      span: ctx.spanOf(mount.node, mount.sourceFile),
      revision: ctx.revision,
      adapterVersion: ctx.adapterVersion,
      frameworkVersion: ctx.frameworkVersion,
    }));
  }
}

/**
 * Ordering classification of one child event under one mount, using
 * proven ESM initialization order where the import graph supports it:
 * same-module registrations follow the positional snapshot rule; an
 * event module that provably evaluates before the mount module is
 * complete; one that provably evaluates after is post-mount; anything
 * not provable stays incomplete with the cross-module reason. A cycle
 * between the two modules is reported as such.
 */
function classifyChildEvent(mount, event, closureOf) {
  if (event.module === mount.module) {
    return event.order < mount.order
      ? { status: "complete", reasons: [] }
      : { status: "incomplete", reasons: ["post-mount-registration"] };
  }
  const mountClosure = closureOf(mount.module);
  const eventClosure = closureOf(event.module);
  const mountBeforeEvent = mountClosure.has(event.module);
  const eventBeforeMount = eventClosure.has(mount.module);
  if (mountBeforeEvent && eventBeforeMount) {
    // Import cycle: initialization order between the bodies is unknown.
    return { status: "incomplete", reasons: ["composition-cycle"] };
  }
  if (mountBeforeEvent) {
    return { status: "complete", reasons: [] };
  }
  if (eventBeforeMount) {
    return { status: "incomplete", reasons: ["post-mount-registration"] };
  }
  return { status: "incomplete", reasons: ["cross-module-registration-order"] };
}

/**
 * Resolve the route/use events of one instance under one base prefix.
 * `scope` distinguishes standalone from per-mount resolution so shared
 * children produce one resolved set per mount occurrence.
 */
function resolveInto(ctx, routeEvents, instance, _instanceBase, scope, routes, depth) {
  if (depth >= HONO_MAX_MOUNT_DEPTH) return;
  for (const event of routeEvents) {
    if (event.instance.key !== instance.key) continue;
    if (scope.origin.startsWith("mount:") && event.resolvedScopes?.has(scope.origin)) continue;
    resolveOneRoute(ctx, event, scope, routes);
  }
}

/** Resolve exactly one route event into its route-handler records. */
function resolveOneRoute(ctx, event, scope, routes) {
  const methods = event.methods ?? [null];
  const fullPath = scope.origin === "standalone"
    ? joinPaths(scope.base, event.path ?? "/")
    : joinPaths(scope.base, event.path ?? "/");
  const reasons = [...(event.reasons ?? []), ...(scope.reasons ?? [])];
  const status = event.status === "unknown"
    ? "unknown"
    : scope.status === "complete" && reasons.length === 0
      ? "complete"
      : event.status === "complete"
        ? "incomplete"
        : event.status;
  const terminal = event.handlers.length > 0 ? event.handlers[event.handlers.length - 1] : null;
  const resolved = {
    event,
    instance: event.instance,
    mount: scope.mount ?? null,
    path: fullPath,
    methods: methods.filter(Boolean),
    terminal,
    middleware: event.handlers.slice(0, Math.max(0, event.handlers.length - 1)),
    status,
    reasons,
    span: ctx.spanOf(event.node, event.sourceFile),
    module: event.module,
    order: event.order,
    origin: scope.origin,
  };
  routes.push(resolved);
  if (scope.origin.startsWith("mount:")) {
    event.resolvedScopes ??= new Set();
    event.resolvedScopes.add(scope.origin);
  }
  if (!terminal) {
    ctx.addUncertaintyAt(event.sourceFile, event.node, "unknown-handler", "missing-handler");
    return;
  }
  for (const method of methods.length > 0 ? methods : [null]) {
    ctx.addRecord(makeRecord({
      relation: "dev.lekalo.hono/route-handler",
      from: instanceEndpoint(event.instance),
      to: endpointOf(terminal),
      method,
      path: fullPath,
      ordinal: event.handlers.length - 1,
      provenance: "detected",
      confidence: confidenceOf(event, terminal, status),
      status,
      reasons: dedupe(reasons),
      span: resolved.span,
      revision: ctx.revision,
      adapterVersion: ctx.adapterVersion,
      frameworkVersion: ctx.frameworkVersion,
    }));
  }
}

function confidenceOf(event, terminal, status) {
  if (status === "unknown") return "unknown";
  if (event.pathKind === "literal" && terminal.kind === "reference" && terminal.indexed) return "exact";
  if (event.pathKind === "const-alias" || terminal.kind === "local") return "high";
  return terminal.kind === "inline" ? "exact" : "medium";
}

function dedupe(values) {
  return [...new Set(values)];
}

export function instanceEndpoint(instance) {
  if (!instance) return null;
  return {
    module: instance.module,
    native: instance.native,
    name: instance.name,
    indexed: instance.indexed,
    signature: null,
  };
}

export function endpointOf(handler) {
  if (!handler) return null;
  return {
    module: handler.module,
    native: handler.native,
    name: handler.name,
    indexed: handler.indexed === true,
    signature: handler.signature ?? null,
    digest: handler.digest ?? null,
  };
}

/** OpenAPI route definitions + `.openapi(definition, handler)` joins. */
function emitOpenApiRecords(ctx, openapiEvents, definitions) {
  const definitionByNode = new Map(definitions.map((event) => [event.definition.node, event.definition]));
  for (const event of definitions) {
    const definition = event.definition;
    const definitionSource = event.sourceFile;
    if (definition.method === null || definition.path === null) continue;
    ctx.addRecord(makeRecord({
      relation: "dev.lekalo.hono/openapi-operation",
      from: {
        module: definition.module,
        native: ctx.inlineNative(definitionSource, definition.node, "openapi"),
        name: "createRoute",
        indexed: false,
      },
      to: null,
      method: definition.method,
      path: definition.path,
      note: definition.operationId ?? "operationid-unknown",
      provenance: "detected",
      confidence: definition.operationId ? "exact" : "medium",
      status: definition.operationId ? "complete" : "incomplete",
      reasons: definition.operationId ? [] : ["dynamic-path"],
      span: ctx.spanOf(definition.node, definitionSource),
      revision: ctx.revision,
      adapterVersion: ctx.adapterVersion,
      frameworkVersion: ctx.frameworkVersion,
    }));
  }
  for (const event of openapiEvents) {
    const definitionNode = event.routeDefinition;
    const resolved = definitionNode
      ? definitionByNode.get(definitionCallNode(ctx, definitionNode)) ?? null
      : null;
    // A definition only joins when its method and path resolved; the
    // operationId may still be missing (incomplete, never guessed).
    const definition = resolved && resolved.method !== null && resolved.path !== null
      ? resolved
      : null;
    const definitionSeen = resolved !== null;
    for (const handler of event.handlers) {
      ctx.addRecord(makeRecord({
        relation: "dev.lekalo.hono/openapi-operation",
        from: instanceEndpoint(event.instance),
        to: endpointOf(handler),
        method: definition?.method ?? null,
        path: definition?.path ?? null,
        note: definition?.operationId ?? (definitionSeen ? "definition-incomplete" : "definition-unresolved"),
        provenance: "detected",
        confidence: definition ? "exact" : "low",
        status: definition && handler.reason === undefined
          ? "complete"
          : "incomplete",
        reasons: definition
          ? (handler.reason ? [handler.reason] : [])
          : (definitionSeen ? ["dynamic-path"] : ["unknown-handler"]),
        span: ctx.spanOf(event.node, event.sourceFile),
        revision: ctx.revision,
        adapterVersion: ctx.adapterVersion,
        frameworkVersion: ctx.frameworkVersion,
      }));
    }
  }
}

/**
 * Resolve a route-definition argument to the createRoute call node:
 * the exact call expression, or the const variable initialized by one.
 */
function definitionCallNode(ctx, node) {
  const { ts } = ctx;
  if (!node) return null;
  if (node.kind === ts.SyntaxKind.CallExpression) return node;
  if (node.kind === ts.SyntaxKind.Identifier) {
    const { checker } = ctx;
    const symbol = checker.getSymbolAtLocation(node);
    for (const declaration of symbol?.declarations ?? []) {
      if (declaration.kind === ts.SyntaxKind.VariableDeclaration
        && declaration.initializer?.kind === ts.SyntaxKind.CallExpression) {
        return declaration.initializer;
      }
    }
  }
  return null;
}
