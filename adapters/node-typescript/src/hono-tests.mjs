/**
 * Hono test bindings (issue #115).
 *
 * In test modules the provider binds `app.request(path, init)` and
 * `testClient(app)` flows back to the resolved routes of the SAME
 * application identity. A linked test is claimed flow evidence with
 * provenance — never a passing run, never conformance. Dynamic URLs,
 * unknown apps, and ambiguous route matches lower completeness with
 * machine reasons instead of guessed links.
 */
import {
  makeRecord,
} from "./hono-evidence.mjs";
import { instanceEndpoint } from "./hono-routes.mjs";
import { importSpecifierTextAt } from "./hono-context.mjs";


/** The static describe/it/test vocabulary (mirrors the generic rule). */
const TEST_CALLEES = new Set(["describe", "it", "test"]);

/** Test modules by path shape; anything else is not scanned for tests. */
function isTestModule(path) {
  return /(^|\/)(test|spec)\.[cm]?[jt]sx?$/.test(path)
    || /\.(test|spec)\.[cm]?[jt]sx?$/.test(path)
    || /(^|\/)__tests__\//.test(path);
}

/**
 * Bind test flows to resolved routes. `routes` is the resolved route
 * list of this scan (all instances).
 */
export function collectTestBindings(ctx, routes) {
  const { ts, checker, program } = ctx;
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile) continue;
    const fromModule = ctx.normalizeModulePath(sourceFile.fileName);
    if (fromModule === null || !isTestModule(fromModule)) continue;
    const testScopes = []; // stack of describe/it literals
    const clientVariables = new Map(); // symbol -> instance record
    const visit = (node) => {
      if (node.kind === ts.SyntaxKind.CallExpression) {
        const expression = node.expression;
        const calleeName = expression.kind === ts.SyntaxKind.Identifier
          ? expression.text
          : expression.kind === ts.SyntaxKind.PropertyAccessExpression ? expression.name?.text : null;
        // describe/it/test scope tracking (static vocabulary only).
        if (calleeName && TEST_CALLEES.has(calleeName)) {
          const nameNode = node.arguments?.[0];
          const name = nameNode && (nameNode.kind === ts.SyntaxKind.StringLiteral) ? nameNode.text : null;
          testScopes.push(name);
          const callback = node.arguments?.find((argument) =>
            argument.kind === ts.SyntaxKind.ArrowFunction || argument.kind === ts.SyntaxKind.FunctionExpression);
          if (callback) {
            ts.forEachChild(callback, visit);
          }
          testScopes.pop();
          return;
        }
        if (calleeName === "testClient") {
          collectTestClient(ctx, node, sourceFile, fromModule, routes, testScopes, clientVariables);
        } else if (calleeName === "request"
          && expression.kind === ts.SyntaxKind.PropertyAccessExpression) {
          // Only `app.request(...)` on a resolvable receiver is Hono
          // evidence; a bare `request(...)` helper is not a Hono-shaped
          // call and produces no uncertainty (issue #115 fix round).
          collectAppRequest(ctx, node, expression, sourceFile, fromModule, routes, testScopes);
        } else if (calleeName && expression.kind === ts.SyntaxKind.PropertyAccessExpression
          && ["get", "post", "put", "patch", "delete", "options", "head"].includes(calleeName)) {
          collectClientVerb(ctx, node, expression, calleeName, sourceFile, fromModule,
            routes, testScopes, clientVariables);
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(sourceFile);
  }
}

/** `const client = testClient(app)` + the client→app identity record. */
function collectTestClient(ctx, node, sourceFile, fromModule, routes, testScopes, clientVariables) {
  const { ts, checker } = ctx;
  const specifier = importSpecifierTextAt(ctx, node.expression);
  if (specifier !== "hono/testing") return;
  const appExpression = node.arguments?.[0] ?? null;
  const instance = appExpression ? ctx.instanceOfExpression(appExpression, sourceFile) : null;
  if (!instance) {
    ctx.addUncertaintyAt(sourceFile, node, "unknown-test-app", "testClient");
    return;
  }
  emitRouteTest(ctx, {
    module: fromModule,
    sourceFile,
    node,
    testScopes,
    instance,
    route: null,
    note: "test-client",
    terminal: null,
    method: null,
    path: null,
    status: "complete",
    reasons: [],
    confidence: "exact",
  });
  // Bind `const client = testClient(app)` for later verb calls.
  const declaration = node.parent;
  if (declaration?.kind === ts.SyntaxKind.VariableDeclaration
    && declaration.name?.kind === ts.SyntaxKind.Identifier) {
    const symbol = checker.getSymbolAtLocation(declaration.name);
    if (symbol) clientVariables.set(symbol, instance);
  }
}

/** `app.request('/path', {method: 'POST'})` → route-test record. */
function collectAppRequest(ctx, node, expression, sourceFile, fromModule, routes, testScopes) {
  const { ts } = ctx;
  const instance = ctx.instanceOfExpression(expression.expression, sourceFile);
  if (!instance) {
    ctx.addUncertaintyAt(sourceFile, node, "unknown-test-app", "request");
    return;
  }
  const pathNode = node.arguments?.[0] ?? null;
  const path = pathNode ? ctx.resolveLiteralString(pathNode, sourceFile) : null;
  const init = node.arguments?.[1] ?? null;
  const method = init && init.kind === ts.SyntaxKind.ObjectLiteralExpression
    ? literalMethodOf(ctx, init)
    : null;
  if (path === null) {
    ctx.addUncertaintyAt(sourceFile, node, "dynamic-test-target", "request-path");
    return;
  }
  emitMatchedRouteTest(ctx, {
    module: fromModule,
    sourceFile,
    node,
    testScopes,
    instance,
    method: method ?? "GET",
    path: path.value,
    // An absent init is the documented default (GET); only a present
    // init without a literal method is dynamic.
    methodResolved: init ? method !== null : true,
    pathResolved: true,
  });
}

/** `client.get('/path')` on a bound testClient variable. */
function collectClientVerb(ctx, node, expression, verb, sourceFile, fromModule, routes, testScopes, clientVariables) {
  const { ts, checker } = ctx;
  const receiver = expression.expression;
  if (receiver.kind !== ts.SyntaxKind.Identifier) return;
  const symbol = checker.getSymbolAtLocation(receiver);
  const instance = symbol ? clientVariables.get(symbol) : null;
  if (!instance) return;
  const pathNode = node.arguments?.[0] ?? null;
  const path = pathNode ? ctx.resolveLiteralString(pathNode, sourceFile) : null;
  if (path === null) {
    ctx.addUncertaintyAt(sourceFile, node, "dynamic-test-target", "client-path");
    return;
  }
  emitMatchedRouteTest(ctx, {
    module: fromModule,
    sourceFile,
    node,
    testScopes,
    instance,
    method: verb.toUpperCase(),
    path: path.value,
    methodResolved: true,
    pathResolved: true,
  });
}

/**
 * Match one static request against the resolved routes rooted at the
 * same app identity: standalone routes of the instance AND routes that
 * reached it through mount ancestry (rootInstance). Test flows on a
 * mounting app can therefore bind mounted routes; a unique match binds
 * the terminal handler and inherits the route's completeness (a bound
 * but incomplete route never claims a complete flow). Ambiguous or
 * missing matches stay reasoned incomplete records, never guesses.
 */
function emitMatchedRouteTest(ctx, { module, sourceFile, node, testScopes, instance, method, path, methodResolved, pathResolved }) {
  const candidates = [];
  for (const route of ctx.routes ?? []) {
    if (route.rootInstance?.key !== instance.key) continue;
    for (const routeMethod of route.methods) {
      if (routeMethod === method && route.path === path) candidates.push(route);
    }
  }
  const unique = candidates.length === 1 ? candidates[0] : null;
  const bound = unique !== null;
  const complete = bound && unique.status === "complete";
  emitRouteTest(ctx, {
    module,
    sourceFile,
    node,
    testScopes,
    instance,
    route: unique,
    terminal: unique?.terminal ?? null,
    method,
    path,
    status: complete ? "complete" : bound ? unique.status : "incomplete",
    reasons: bound
      ? [...(unique.reasons ?? [])]
      : candidates.length === 0
        ? (methodResolved && pathResolved ? ["missing-endpoint-join"] : ["dynamic-test-target"])
        : ["ambiguous-endpoint-join"],
    confidence: complete ? "exact" : bound ? "medium" : "low",
    note: bound ? "app-request" : `app-request:${candidates.length}-matches`,
  });
}

function emitRouteTest(ctx, { module, sourceFile, node, testScopes, instance, route, terminal, method, path, status, reasons, confidence, note }) {
  const testName = [...testScopes].filter(Boolean).join(">");
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/route-test",
    from: {
      module,
      native: ctx.inlineNative(sourceFile, node, "test"),
      name: testName || "test",
      indexed: false,
    },
    to: terminal
      ? {
        module: terminal.module,
        native: terminal.native,
        name: terminal.name,
        indexed: terminal.indexed === true,
        signature: terminal.signature ?? null,
        digest: terminal.digest ?? null,
      }
      : instanceEndpoint(instance),
    method,
    path,
    note,
    provenance: "detected",
    confidence,
    status,
    reasons,
    span: ctx.spanOf(node, sourceFile),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

function literalMethodOf(ctx, objectLiteral) {
  const { ts } = ctx;
  for (const property of objectLiteral.properties ?? []) {
    if (property.kind === ts.SyntaxKind.PropertyAssignment
      && property.name?.kind === ts.SyntaxKind.Identifier
      && property.name.text === "method"
      && property.initializer?.kind === ts.SyntaxKind.StringLiteral) {
      return property.initializer.text.toUpperCase();
    }
  }
  return null;
}

