/**
 * Hono HTTP evidence: request validation, responses, errors, OpenAPI
 * routes, and the API/SSR distinction (issue #115).
 *
 * - Validators bind to the routes whose argument position they occupy
 *   (node identity, never shape matching) with their validation target
 *   and resolved schema symbol.
 * - Response sites are compiler-detected calls on the handler's context
 *   parameter; statuses come from literal arguments or a preceding
 *   `c.status(n)` in the same body, else the method default.
 * - Thrown `HTTPException`s (pinned import vocabulary) produce
 *   handles-error records with literal status evidence.
 * - Every route gets one classification record: api/html/ssr/mixed/
 *   unknown. A `.tsx` extension alone is never SSR evidence; JSX
 *   returns, JSX in `c.html`, and renderer `c.render` are.
 */
import {
  makeRecord,
  mergeRouteEvidence,
} from "./hono-evidence.mjs";
import { endpointOf, instanceEndpoint } from "./hono-routes.mjs";
import {
  contextParamSymbolOf,
  functionBodyOf,
  importSpecifierTextAt,
  resolvesToContextSymbol,
} from "./hono-context.mjs";


const RESPONSE_METHODS = new Set(["json", "text", "html", "body", "render"]);
const DEFAULT_STATUS = { json: 200, text: 200, html: 200, body: 200, render: 200 };

const MAX_BODY_NODES = 4096;

/** Validation, response, error, and classification evidence per route. */
export function collectHttpEvidence(ctx, routes, registrations) {
  const { ts } = ctx;
  for (const route of routes) {
    const chainMembers = [...(route.middleware ?? []), route.terminal].filter(Boolean);
    // 1. Validator bindings along the chain.
    for (let index = 0; index < (route.middleware ?? []).length; index += 1) {
      const member = route.middleware[index];
      if (member.kind === "validator") {
        emitValidatorRecord(ctx, route, member, index);
      }
    }
    // 2. Response sites + thrown errors of the terminal handler.
    if (route.terminal) {
      const sites = collectResponseSites(ctx, route.terminal);
      for (const site of sites) {
        emitResponseRecord(ctx, route, route.terminal, site);
      }
      collectThrownErrors(ctx, route, route.terminal);
      // 3. Route classification (api/html/ssr/mixed/unknown).
      emitRouteClassification(ctx, route, sites);
    }
  }
}

/** One validates-request record bound to the route occurrence. */
function emitValidatorRecord(ctx, route, member, ordinal) {
  const validator = member.validator;
  const schemaNode = validator.schemaNode;
  let to = null;
  let status = "complete";
  let reasons = [];
  let confidence = "exact";
  if (schemaNode) {
    const { ts, checker } = ctx;
    const target = schemaNode.kind === ts.SyntaxKind.PropertyAccessExpression ? schemaNode.name : schemaNode;
    let symbol = target.kind === ts.SyntaxKind.Identifier ? checker.getSymbolAtLocation(target) : null;
    if (symbol) {
      // Imported schema bindings alias to the declaring symbol.
      for (let depth = 0; depth < 8; depth += 1) {
        if (symbol.flags & ts.SymbolFlags.Alias) {
          try {
            symbol = checker.getAliasedSymbol(symbol);
          } catch {
            break;
          }
        } else {
          break;
        }
      }
    }
    if (symbol) {
      const row = ctx.symbolRowOf(symbol);
      if (row) {
        to = {
          module: row.module,
          native: row.native,
          name: row.qualifiedName,
          indexed: true,
          signature: row.signature,
          digest: ctx.digestOfNode(symbol.declarations?.[0] ?? schemaNode, schemaNode.getSourceFile()),
        };
      } else {
        to = {
          module: ctx.normalizeModulePath(schemaNode.getSourceFile().fileName),
          native: ctx.inlineNative(schemaNode.getSourceFile(), schemaNode, "schema"),
          name: symbol.name ?? "schema",
          indexed: false,
          digest: ctx.digestOfNode(schemaNode, schemaNode.getSourceFile()),
        };
        confidence = "high";
      }
    } else if (schemaNode.kind === ts.SyntaxKind.CallExpression
      || schemaNode.kind === ts.SyntaxKind.ArrowFunction
      || schemaNode.kind === ts.SyntaxKind.FunctionExpression) {
      // Inline schema/handler constructions: compiler-identified by
      // occurrence and content digest, not by name.
      to = {
        module: ctx.normalizeModulePath(schemaNode.getSourceFile().fileName),
        native: ctx.inlineNative(schemaNode.getSourceFile(), schemaNode, "schema"),
        name: "inline-schema",
        indexed: false,
        digest: ctx.digestOfNode(schemaNode, schemaNode.getSourceFile()),
      };
      confidence = "high";
    } else {
      status = "incomplete";
      reasons = ["unresolved-schema"];
      confidence = "low";
      ctx.addUncertaintyAt(validator.node.getSourceFile(), validator.node, "unresolved-schema", validator.kind);
    }
  } else {
    status = "incomplete";
    reasons = ["unresolved-schema"];
    confidence = "low";
  }
  // A validation claim under a conditional/deferred/unresolved mount or
  // registration inherits that scope — never a complete fact (issue
  // #115 fix round 2).
  const evidence = mergeRouteEvidence(route, status, reasons);
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/validates-request",
    from: instanceEndpoint(route.instance),
    to,
    method: route.methods[0] ?? null,
    path: route.path,
    ordinal,
    note: `${validator.kind}:${validator.target ?? "target-unknown"}`,
    provenance: "detected",
    confidence,
    status: evidence.status,
    reasons: evidence.reasons,
    span: ctx.spanOf(validator.node, validator.node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

/** Response sites of one handler in source order. */
function collectResponseSites(ctx, handler) {
  const body = functionBodyOf(ctx, handler);
  if (!body) return [];
  const contextParameter = contextParamSymbolOf(ctx, handler);
  if (!contextParameter) return [];
  const { ts } = ctx;
  const sites = [];
  let currentStatus = null;
  let visited = 0;
  const visit = (node) => {
    if (visited > MAX_BODY_NODES) return;
    visited += 1;
    if (node.kind === ts.SyntaxKind.CallExpression && node.expression.kind === ts.SyntaxKind.PropertyAccessExpression) {
      const method = node.expression.name?.text;
      const receiver = node.expression.expression;
      if (resolvesToContextSymbol(ctx, receiver, contextParameter)) {
        if (method === "status") {
          const literal = literalNumber(node.arguments?.[0] ?? null, ts);
          if (literal !== null) currentStatus = literal;
          else ctx.addUncertaintyAt(node.getSourceFile(), node, "dynamic-status", "c.status");
        } else if (RESPONSE_METHODS.has(method)) {
          const statusArg = method === "render" ? null : literalNumber(node.arguments?.[1] ?? null, ts);
          sites.push({
            node,
            method,
            status: statusArg ?? currentStatus ?? DEFAULT_STATUS[method],
            statusIsDefault: statusArg === null && currentStatus === null,
            jsx: method === "html" && isJsxNode(ctx, node.arguments?.[0] ?? null)
              || method === "render",
            jsxReturn: false,
          });
        }
      }
    } else if (node.kind === ts.SyntaxKind.ReturnStatement && isJsxNode(ctx, node.expression ?? null)) {
      sites.push({
        node,
        method: "jsx-return",
        status: 200,
        statusIsDefault: true,
        jsx: true,
        jsxReturn: true,
      });
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
  return sites;
}

function literalNumber(node, ts) {
  return node && node.kind === ts.SyntaxKind.NumericLiteral ? Number(node.text) : null;
}

function isJsxNode(ctx, node) {
  if (!node) return false;
  const { ts } = ctx;
  return node.kind === ts.SyntaxKind.JsxElement
    || node.kind === ts.SyntaxKind.JsxSelfClosingElement
    || node.kind === ts.SyntaxKind.JsxFragment;
}

function facetOfSite(site) {
  if (site.method === "json" || site.method === "text" || site.method === "body") return "api";
  if (site.method === "html") return site.jsx ? "ssr" : "html";
  if (site.method === "render") return "ssr";
  if (site.method === "jsx-return") return "ssr";
  return "unknown";
}

function emitResponseRecord(ctx, route, handler, site) {
  const facet = facetOfSite(site);
  // The response rides the route's scope: a response site inside a
  // handler only reachable through an incomplete mount stays
  // incomplete (issue #115 fix round 2).
  const evidence = mergeRouteEvidence(route, "complete", []);
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/returns-response",
    from: instanceEndpoint(route.instance),
    to: endpointOf(handler),
    method: route.methods[0] ?? null,
    path: route.path,
    facet,
    httpStatus: site.status,
    note: `${site.method}${site.statusIsDefault ? ":default-status" : ""}`,
    provenance: "detected",
    confidence: site.statusIsDefault ? "high" : "exact",
    status: evidence.status,
    reasons: evidence.reasons,
    span: ctx.spanOf(site.node, site.node.getSourceFile()),
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

/** The route-level API/SSR classification from all observed sites. */
function emitRouteClassification(ctx, route, sites) {
  const facets = new Set(sites.map(facetOfSite).filter((facet) => facet !== "unknown"));
  let facet;
  let status = "complete";
  const reasons = [];
  if (facets.size === 0) {
    facet = "unknown";
    status = "incomplete";
    reasons.push("no-response-evidence");
  } else if (facets.size === 1) {
    facet = [...facets][0];
  } else if (facets.has("api") && (facets.has("ssr") || facets.has("html"))) {
    facet = "mixed";
  } else {
    facet = "mixed";
  }
  route.facet = facet;
  const evidence = mergeRouteEvidence(route, status, reasons);
  ctx.addRecord(makeRecord({
    relation: "dev.lekalo.hono/returns-response",
    from: instanceEndpoint(route.instance),
    to: endpointOf(route.terminal),
    method: route.methods[0] ?? null,
    path: route.path,
    facet,
    note: "route-classification",
    provenance: "detected",
    confidence: facet === "unknown" ? "low" : "high",
    status: evidence.status,
    reasons: evidence.reasons,
    span: route.span,
    revision: ctx.revision,
    adapterVersion: ctx.adapterVersion,
    frameworkVersion: ctx.frameworkVersion,
  }));
}

/** Thrown HTTPException sites (pinned import vocabulary). */
function collectThrownErrors(ctx, route, handler) {
  const body = functionBodyOf(ctx, handler);
  if (!body) return;
  const { ts, checker } = ctx;
  let visited = 0;
  const visit = (node) => {
    if (visited > MAX_BODY_NODES) return;
    visited += 1;
    if (node.kind === ts.SyntaxKind.ThrowStatement) {
      const expression = node.expression;
      const target = expression?.kind === ts.SyntaxKind.NewExpression
        ? expression.expression
        : expression;
      if (target && (target.kind === ts.SyntaxKind.Identifier || target.kind === ts.SyntaxKind.PropertyAccessExpression)) {
        const symbolNode = target.kind === ts.SyntaxKind.PropertyAccessExpression ? target.name : target;
        const symbol = checker.getSymbolAtLocation(symbolNode);
        const importedFrom = symbol ? importSpecifierTextAt(ctx, target) : null;
        if (importedFrom === "hono/http-exception") {
          const statusArgument = expression?.kind === ts.SyntaxKind.NewExpression
            ? (expression.arguments?.[0]?.kind === ts.SyntaxKind.NumericLiteral
              ? Number(expression.arguments[0].text)
              : null)
            : null;
          const thrownStatus = statusArgument !== null ? "complete" : "incomplete";
          const thrownReasons = statusArgument !== null ? [] : ["dynamic-status"];
          const evidence = mergeRouteEvidence(route, thrownStatus, thrownReasons);
          ctx.addRecord(makeRecord({
            relation: "dev.lekalo.hono/handles-error",
            from: instanceEndpoint(route.instance),
            to: {
              module: null,
              native: null,
              name: "HTTPException",
              indexed: false,
            },
            method: route.methods[0] ?? null,
            path: route.path,
            httpStatus: statusArgument,
            note: "throw",
            provenance: "detected",
            confidence: statusArgument !== null ? "exact" : "medium",
            status: evidence.status,
            reasons: evidence.reasons,
            span: ctx.spanOf(node, node.getSourceFile()),
            revision: ctx.revision,
            adapterVersion: ctx.adapterVersion,
            frameworkVersion: ctx.frameworkVersion,
          }));
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(body);
}

