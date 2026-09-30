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
  HONO_MAX_CHAIN,
  canonicalHonoText,
  makeRecord,
  mergeRouteEvidence,
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
    const composed = composeChain(route, useEvents);
    // The chain bound is explicit: retained members keep their ordinals,
    // the overflow surfaces as chain-budget uncertainty on the route's
    // registration span — never a silently shortened chain.
    const chain = composed.slice(0, HONO_MAX_CHAIN);
    if (composed.length > HONO_MAX_CHAIN) {
      ctx.addUncertaintyAt(route.event.sourceFile, route.event.node, "chain-budget", String(composed.length));
    }
    const chainLength = chain.length;
    let ordinal = 0;
    // Context records are per route + handler + call site: distinct
    // bindings of the SAME handler by different use events are distinct
    // chain members but ONE fact per context site — without this
    // identity the repeated binding emits byte-identical records and
    // the envelope validator reports duplicate-record (issue #115 fix
    // round 2).
    const contextIdentities = new Map();
    for (const member of chain) {
      emitMiddlewareRecord(ctx, route, member, ordinal, chainLength);
      emitContextRecords(ctx, route, member.handler, contextIdentities);
      ordinal += 1;
    }
    // The terminal handler rides route-handler records (routes phase);
    // its context writes still belong to this route's context story.
    if (route.terminal) {
      emitContextRecords(ctx, route, route.terminal, contextIdentities);
    }
  }
}

/**
 * The ordered chain of one route: applicable `use` middleware of the
 * owning instance, preceded by every mount ancestor's pre-mount
 * middleware (outermost first — execution order), then inline
 * middleware. Parent path filters are matched against the route's full
 * resolved path: a literal disjoint filter is provably excluded, a
 * wildcard/parameterized filter stays conditional, and an unresolvable
 * filter stays conditional with dynamic-path-filter — a path-filtered
 * parent `use` over a mount therefore still yields records (issue #115
 * fix round), never silence. Cross-module pre-mount parent middleware
 * is unproven ordering and stays out of the chain by design.
 */
function composeChain(route, useEvents) {
  const chain = [];
  const seen = new Set();
  const consider = (event, provenance) => {
    if (seen.has(event)) return;
    seen.add(event);
    const applicability = applicabilityOf(event, route);
    if (applicability === "not-applicable") return;
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
  // Mount ancestors: each ancestor's pre-mount `use` events (proven by
  // same-module registration order to precede the mount occurrence).
  const mountChain = route.mountChain ?? (route.mount ? [route.mount] : []);
  for (const mount of mountChain) {
    const parentUses = useEvents
      .filter((event) => event.instance.key === mount.instance.key
        && event.module === mount.module
        && event.order < mount.order)
      .sort((left, right) => left.order - right.order);
    for (const event of parentUses) {
      consider(event, "parent-use");
    }
  }
  // Same-instance `use` events in registration order.
  const own = useEvents
    .filter((event) => event.instance.key === route.instance.key)
    .sort((left, right) => left.order - right.order);
  for (const event of own) {
    consider(event, "use");
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
  // A filter that failed to resolve proves no overlap and no disjointness:
  // conditional evidence, never a guessed match (issue #115 fix round).
  if (event.pathFilterKind === "unknown") return "conditional";
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
  // `next()` evidence is compiler-symbol based: the call must resolve
  // to the middleware's OWN declared second parameter. A same-named
  // symbol from any other scope is not pass-through evidence (issue
  // #115 fix round), and middleware without a next parameter have
  // nothing to detect.
  const nextSymbol = nextParamSymbolOf(ctx, handler);
  const callsNext = body ? containsNextCall(ctx, body, nextSymbol) : false;
  const role = explicitRoleOf(ctx, handler);
  const reasons = [];
  let status = "complete";
  if (member.applicability === "conditional") {
    reasons.push("conditional-applicability");
    status = "incomplete";
  }
  if (member.useEvent?.pathFilterKind === "unknown") {
    reasons.push("dynamic-path-filter");
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
  // Chain membership rides the route's scope: middleware composed onto
  // a route under a conditional/deferred/unresolved mount inherits that
  // scope — presence under a phantom route is never a complete fact
  // (issue #115 fix round 2).
  const evidence = mergeRouteEvidence(route, status, reasons);
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
    status: evidence.status,
    reasons: evidence.reasons,
    span: ctx.spanOf(handler.node, handler.node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

/**
 * The symbol of the middleware's declared second parameter — the Hono
 * `next` continuation — resolved for inline and referenced handlers.
 */
function nextParamSymbolOf(ctx, handler) {
  const { ts, checker } = ctx;
  let declaration = null;
  if (handler.node
    && (handler.node.kind === ts.SyntaxKind.ArrowFunction || handler.node.kind === ts.SyntaxKind.FunctionExpression)) {
    declaration = handler.node;
  } else if (handler.indexed && handler.symbol) {
    declaration = handler.symbol.declarations?.find((candidate) => candidate.body
      && (candidate.kind === ts.SyntaxKind.FunctionDeclaration
        || candidate.kind === ts.SyntaxKind.MethodDeclaration
        || candidate.kind === ts.SyntaxKind.ArrowFunction
        || candidate.kind === ts.SyntaxKind.FunctionExpression)) ?? null;
  }
  const second = declaration?.parameters?.[1]?.name;
  if (!second || second.kind !== ts.SyntaxKind.Identifier) return null;
  return checker.getSymbolAtLocation(second) ?? null;
}

/** Compiler-walk detection of a call to the middleware's own `next`
 * parameter symbol inside its body. */
function containsNextCall(ctx, body, nextSymbol) {
  if (!nextSymbol) return false;
  const { ts, checker } = ctx;
  let found = false;
  let visited = 0;
  const visit = (node) => {
    if (found || visited > MAX_BODY_NODES) return;
    visited += 1;
    if (node.kind === ts.SyntaxKind.CallExpression && node.expression.kind === ts.SyntaxKind.Identifier
      && checker.getSymbolAtLocation(node.expression) === nextSymbol) {
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
function emitContextRecords(ctx, route, handler, contextIdentities) {
  const body = functionBodyOf(ctx, handler);
  if (!body) return;
  // Distinct use events binding the same handler resolve distinct
  // endpoint OBJECTS with identical CONTENT: the dedupe key is the
  // canonical endpoint, not object identity (issue #115 fix round 2).
  const handlerKey = canonicalHonoText(endpointOf(handler));
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
          methodName === "set" ? "context-write" : "context-read", key, handlerKey, contextIdentities);
      }
    } else if ((node.kind === ts.SyntaxKind.PropertyAccessExpression || node.kind === ts.SyntaxKind.ElementAccessExpression)
      && node.expression?.kind === ts.SyntaxKind.PropertyAccessExpression
      && node.expression.name?.text === "var"
      && resolvesToContextSymbol(ctx, node.expression.expression, contextParameter)) {
      const key = node.kind === ts.SyntaxKind.PropertyAccessExpression
        ? (node.name?.kind === ts.SyntaxKind.Identifier ? node.name.text : null)
        : (node.argument?.kind === ts.SyntaxKind.StringLiteral ? node.argument.text : null);
      emitContextKeyRecord(ctx, route, handler, node, "context-read", key, handlerKey, contextIdentities);
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
}

function emitContextKeyRecord(ctx, route, handler, node, relation, key, handlerKey, contextIdentities) {
  if (key === null || key === undefined) {
    ctx.addUncertaintyAt(node.getSourceFile(), node, "dynamic-context-key", relation);
    return;
  }
  // One context site is one fact per route: the same handler bound by
  // several use events must not re-emit the identical record (issue
  // #115 fix round 2). Sites stay distinct — span identity — so two
  // reads of one key in one handler keep both records.
  const span = ctx.spanOf(node, node.getSourceFile());
  const identity = `${relation}|${key}|${span.path}:${span.startLine}:${span.startColumn}:${span.endLine}:${span.endColumn}`;
  let seen = contextIdentities.get(handlerKey);
  if (seen === undefined) {
    seen = new Set();
    contextIdentities.set(handlerKey, seen);
  }
  if (seen.has(identity)) return;
  seen.add(identity);
  // The context read/write rides the route's scope: a key touched by a
  // handler only reachable through an incomplete mount stays incomplete
  // (issue #115 fix round 2).
  const evidence = mergeRouteEvidence(route, "complete", []);
  ctx.addRecord(makeRecord({
    relation: `dev.lekalo.hono/${relation}`,
    from: endpointOf(handler),
    to: null,
    path: route.path,
    method: route.methods[0] ?? null,
    note: key.slice(0, 128),
    provenance: "detected",
    confidence: "exact",
    status: evidence.status,
    reasons: evidence.reasons,
    span: ctx.spanOf(node, node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}
