/**
 * Hono middleware chains, order, and context evidence (issue #115).
 *
 * For every resolved route the provider builds the ordered chain:
 * registered `use` middleware (global or path-filtered, in
 * registration order) followed by the route's inline middleware and
 * terminal handler. Order is expressed as entry ordinals; reverse
 * unwinding of post-`next()` work is annotated only when a `next()`
 * call is compiler-detected in the middleware body — presence of
 * middleware never proves authorization correctness, never mints an
 * `authorizes` relation, and never turns context keys into canonical
 * domain fields.
 *
 * Applicability of a `use` path filter is decided conservatively:
 * global filters apply; a literal disjoint filter demonstrably does not
 * match; parameterized/wildcard filters stay `conditional` with an
 * uncertainty reason instead of a guessed match.
 */
import {
  makeRecord,
} from "./hono-evidence.mjs";
import { endpointOf, instanceEndpoint, reachabilityPenaltyOf } from "./hono-routes.mjs";
import {
  contextParamSymbolOf,
  functionBodyOf,
  resolvesToContextSymbol,
} from "./hono-context.mjs";


/** Explicit JSDoc role tags (annotations, never name inference). */
const ROLE_TAGS = [
  ["@lekalo-auth", "auth"],
  ["@lekalo-tenant", "tenant"],
  ["@lekalo-context", "context"],
  ["@lekalo-logging", "logging"],
  ["@lekalo-custom", "custom"],
];

const MAX_BODY_NODES = 4096;

/**
 * Build middleware chain records and context read/write evidence for
 * every resolved route.
 */
export function buildMiddlewareChains(ctx, routes, registrations) {
  const useEvents = registrations.filter((event) => event.kind === "use");
  for (const route of routes) {
    const chain = composeChain(route, useEvents);
    const chainLength = chain.length;
    let ordinal = 0;
    for (const member of chain) {
      emitMiddlewareRecord(ctx, route, member, ordinal, chainLength);
      emitContextRecords(ctx, route, member.handler);
      ordinal += 1;
    }
    // The terminal handler rides route-handler records (routes phase);
    // its context writes still belong to this route's context story.
    if (route.terminal) {
      emitContextRecords(ctx, route, route.terminal);
    }
  }
}

/**
 * The ordered chain of one route: applicable `use` middleware of the
 * owning instance (and, for mounted routes, the parent's pre-mount
 * global middleware), then inline middleware.
 */
function composeChain(route, useEvents) {
  const chain = [];
  const seen = new Set();
  const consider = (event, provenance, applicability) => {
    if (seen.has(event)) return;
    seen.add(event);
    for (const handler of event.handlers) {
      chain.push({
        handler,
        kind: handler.kind === "validator" ? "validator" : "middleware",
        useEvent: event,
        provenance,
        applicability,
      });
    }
  };
  // Same-instance `use` events in registration order.
  const own = useEvents
    .filter((event) => event.instance.key === route.instance.key)
    .sort((left, right) => left.order - right.order);
  for (const event of own) {
    const applicability = applicabilityOf(event, route);
    if (applicability === "not-applicable") continue;
    consider(event, "use", applicability);
  }
  // Pre-mount global middleware of the mounting parent.
  if (route.mount && route.mount.instance) {
    const parentUses = useEvents
      .filter((event) => event.instance.key === route.mount.instance.key
        && event.module === route.mount.module
        && event.order < route.mount.order
        && (event.pathFilter === null || event.pathFilter === undefined))
      .sort((left, right) => left.order - right.order);
    for (const event of parentUses) {
      consider(event, "parent-use", "conditional");
    }
  }
  // Inline middleware args of the registration itself.
  for (const handler of route.middleware ?? []) {
    chain.push({
      handler,
      kind: handler.kind === "validator" ? "validator" : "middleware",
      useEvent: null,
      provenance: "inline",
      applicability: "applicable",
    });
  }
  return chain;
}

/** Applicability of one use event's path filter to one route. */
function applicabilityOf(event, route) {
  const filter = event.pathFilter;
  if (filter === null || filter === undefined) return "applicable";
  if (filter.includes("*") || filter.includes(":") || filter.includes("?")) {
    return "conditional";
  }
  const target = route.path ?? "/";
  if (target === filter || target.startsWith(filter.endsWith("/") ? filter : `${filter}/`)) {
    return "applicable";
  }
  return "not-applicable";
}

/** One uses-middleware record (order, unwind, next evidence, role). */
function emitMiddlewareRecord(ctx, route, member, ordinal, chainLength) {
  const handler = member.handler;
  if (handler.kind === "validator") return; // validated in the http phase
  const body = functionBodyOf(ctx, handler);
  const callsNext = body ? containsNextCall(ctx, body) : false;
  const role = explicitRoleOf(ctx, handler);
  const reasons = [];
  let status = "complete";
  if (member.applicability === "conditional") {
    reasons.push("conditional-applicability");
    status = "incomplete";
  }
  // A `use` site that is not proven to run at initialization cannot
  // make a complete middleware claim (issue #115 fix round).
  const reach = member.useEvent ? reachabilityPenaltyOf(member.useEvent) : null;
  if (reach) {
    reasons.push(reach.reason);
    status = reach.status === "unknown" ? "unknown" : "incomplete";
  }
  if (!callsNext) {
    reasons.push("no-next-call-detected");
    status = status === "complete" ? "incomplete" : status;
  }
  if (handler.reason) {
    reasons.push(handler.reason);
    status = "incomplete";
  }
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/uses-middleware",
    from: instanceEndpoint(route.instance),
    to: endpointOf(handler),
    method: route.methods[0] ?? null,
    path: route.path,
    ordinal,
    unwindOrdinal: chainLength - ordinal,
    role,
    note: `${member.provenance};next=${callsNext ? "detected" : "absent"}`,
    provenance: role !== null ? "explicit" : "detected",
    confidence: member.applicability === "conditional" ? "medium" : "exact",
    status,
    reasons,
    span: ctx.spanOf(handler.node, handler.node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

/** Compiler-walk detection of a `next()` call inside a body. */
function containsNextCall(ctx, body) {
  const { ts } = ctx;
  let found = false;
  let visited = 0;
  const visit = (node) => {
    if (found || visited > MAX_BODY_NODES) return;
    visited += 1;
    if (node.kind === ts.SyntaxKind.CallExpression && node.expression.kind === ts.SyntaxKind.Identifier
      && node.expression.text === "next") {
      found = true;
      return;
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
  return found;
}

/**
 * Explicit middleware roles from JSDoc annotations on the resolved
 * declaration (indexed rows carry JSDoc; inline nodes carry attached
 * comments). Absent annotation → no role claim, never name inference.
 */
function explicitRoleOf(ctx, handler) {
  const pieces = [];
  // Index rows carry the display comment (known tags only), so custom
  // @lekalo-* tags are read from the declaration's JSDoc syntax too.
  if (handler.indexed && handler.symbol) {
    const display = ctx.symbolRowOf(handler.symbol)?.jsdoc;
    if (typeof display === "string") pieces.push(display);
  }
  const declaration = handler.symbol?.declarations?.[0]
    ?? (handler.node && (handler.node.kind === ts.SyntaxKind.ArrowFunction || handler.node.kind === ts.SyntaxKind.FunctionExpression)
      ? handler.node
      : null);
  if (declaration && ctx.ts.getJSDocCommentsAndTags) {
    const comments = ctx.ts.getJSDocCommentsAndTags(declaration);
    pieces.push(...comments.map((piece) => piece.getText?.() ?? ""));
  }
  const text = pieces.join("\n");
  if (text === "") return null;
  for (const [tag, role] of ROLE_TAGS) {
    if (text.includes(tag)) return role;
  }
  return null;
}

/**
 * Context read/write relations for literal keys (`c.set('k', …)`,
 * `c.get('k')`, `c.var.k`). Keys stay namespaced implementation
 * evidence; dynamic keys become uncertainty, never canonical fields.
 */
function emitContextRecords(ctx, route, handler) {
  const body = functionBodyOf(ctx, handler);
  if (!body) return;
  const contextParameter = contextParamSymbolOf(ctx, handler);
  if (!contextParameter) return;
  const { ts } = ctx;
  let visited = 0;
  const visit = (node) => {
    if (visited > MAX_BODY_NODES) return;
    visited += 1;
    if (node.kind === ts.SyntaxKind.CallExpression && node.expression.kind === ts.SyntaxKind.PropertyAccessExpression) {
      const methodName = node.expression.name?.text;
      const receiver = node.expression.expression;
      if ((methodName === "set" || methodName === "get")
        && resolvesToContextSymbol(ctx, receiver, contextParameter)) {
        const keyNode = node.arguments?.[0] ?? null;
        const key = keyNode && keyNode.kind === ts.SyntaxKind.StringLiteral ? keyNode.text : null;
        emitContextKeyRecord(ctx, route, handler, node,
          methodName === "set" ? "context-write" : "context-read", key);
      }
    } else if ((node.kind === ts.SyntaxKind.PropertyAccessExpression || node.kind === ts.SyntaxKind.ElementAccessExpression)
      && node.expression?.kind === ts.SyntaxKind.PropertyAccessExpression
      && node.expression.name?.text === "var"
      && resolvesToContextSymbol(ctx, node.expression.expression, contextParameter)) {
      const key = node.kind === ts.SyntaxKind.PropertyAccessExpression
        ? (node.name?.kind === ts.SyntaxKind.Identifier ? node.name.text : null)
        : (node.argument?.kind === ts.SyntaxKind.StringLiteral ? node.argument.text : null);
      emitContextKeyRecord(ctx, route, handler, node, "context-read", key);
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
}

function emitContextKeyRecord(ctx, route, handler, node, relation, key) {
  if (key === null || key === undefined) {
    ctx.addUncertaintyAt(node.getSourceFile(), node, "dynamic-context-key", relation);
    return;
  }
  ctx.addRecord(makeRecord({
    relation: `dev.lekalo.hono/${relation}`,
    from: endpointOf(handler),
    to: null,
    path: route.path,
    method: route.methods[0] ?? null,
    note: key.slice(0, 128),
    provenance: "detected",
    confidence: "exact",
    status: "complete",
    reasons: [],
    span: ctx.spanOf(node, node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}
