/**
 * Hono binding joins (issue #115): handler → service calls through the
 * TypeScript semantic graph, and route → endpoint-contract joins.
 *
 * - Handler calls: every compiler-resolved call inside a terminal
 *   handler to an indexed project symbol becomes a `handler-call`
 *   record with exact native identities on both ends. Unresolved
 *   dispatch stays in the generic reference family, never a guess.
 * - Endpoint contracts: route facts join DECLARED contract data
 *   (`lekalo/endpoints.json` read from the inventory) only through
 *   unique compatible method+path matches; ambiguity and absence stay
 *   `incomplete` with reasons, and an SSR-classified route never joins
 *   an API contract. The Model/IR-verified join is a successor-contract
 *   step; shape matches are `inferred` candidates, never confirmations.
 */
import {
  makeRecord,
} from "./hono-evidence.mjs";
import { endpointOf, instanceEndpoint } from "./hono-routes.mjs";


const MAX_CALLS_PER_HANDLER = 32;
const MAX_BODY_NODES = 8192;

/**
 * Parse declared endpoint-contract data files: bounded, strict JSON,
 * closed shape. Anything malformed contributes nothing (the join then
 * reports missing evidence, never a partial guess).
 */
export function resolveEndpointContracts(readDataFile, paths) {
  const contracts = [];
  if (typeof readDataFile !== "function") return contracts;
  for (const path of (paths ?? []).slice(0, 4)) {
    let document;
    try {
      document = JSON.parse(readDataFile(path, 64 * 1024));
    } catch {
      continue;
    }
    if (!Array.isArray(document?.endpoints)) continue;
    for (const endpoint of document.endpoints.slice(0, 256)) {
      if (typeof endpoint?.id !== "string" || typeof endpoint?.method !== "string"
        || typeof endpoint?.path !== "string") continue;
      contracts.push({
        id: endpoint.id.slice(0, 128),
        method: endpoint.method.toUpperCase(),
        path: endpoint.path,
      });
    }
  }
  return contracts;
}

/** handler-call records: terminal handlers → resolved project callees. */
export function joinServiceCalls(ctx, routes) {
  const { ts, checker } = ctx;
  for (const route of routes) {
    const handler = route.terminal;
    if (!handler) continue;
    const body = handlerBodyOf(ctx, handler);
    if (!body) continue;
    let visited = 0;
    let recorded = 0;
    const visit = (node) => {
      if (visited > MAX_BODY_NODES || recorded >= MAX_CALLS_PER_HANDLER) return;
      visited += 1;
      if (node.kind === ts.SyntaxKind.CallExpression) {
        const signature = checker.getResolvedSignature(node);
        const declaration = signature?.declaration;
        if (declaration) {
          const declarationFile = declaration.getSourceFile?.();
          const module = declarationFile ? ctx.normalizeModulePath(declarationFile.fileName) : null;
          if (module !== null && !declarationFile.isDeclarationFile) {
            // Local project callee: bind exact native identities.
            const symbol = declaration.name
              ? checker.getSymbolAtLocation(declaration.name)
              : checker.getSymbolAtLocation(declaration);
            const row = symbol ? ctx.symbolRowOf(symbol) : null;
            if (row) {
              ctx.addRecord(makeRecord({
                relation: "dev.lekalo.hono/handler-call",
                from: endpointOf(handler),
                to: {
                  module: row.module,
                  native: row.native,
                  name: row.qualifiedName,
                  indexed: true,
                  signature: row.signature,
                },
                method: route.methods[0] ?? null,
                path: route.path,
                provenance: "detected",
                confidence: "exact",
                status: "complete",
                reasons: [],
                span: ctx.spanOf(node, node.getSourceFile()),
                revision: ctx.revision,
                adapterVersion: ctx.adapterVersion,
                frameworkVersion: ctx.frameworkVersion,
              }));
              recorded += 1;
            }
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(body);
  }
}

function handlerBodyOf(ctx, handler) {
  const { ts } = ctx;
  const node = handler.node;
  if (!node) return null;
  if (node.kind === ts.SyntaxKind.ArrowFunction || node.kind === ts.SyntaxKind.FunctionExpression) {
    return node.body ?? null;
  }
  const declaration = handler.symbol?.declarations?.find((candidate) => candidate.body);
  return declaration?.body ?? null;
}

/**
 * Join resolved routes to declared endpoint contracts. Runs only when
 * contract data exists; every non-join is a reasoned incomplete record.
 */
export function joinEndpointContracts(ctx, routes) {
  const contracts = ctx.endpointContracts ?? [];
  if (contracts.length === 0) return;
  for (const route of routes) {
    if (!route.terminal) continue;
    const methods = route.methods.length > 0 ? route.methods : [null];
    const candidates = contracts.filter((contract) =>
      methods.some((method) => method === contract.method)
      && contract.path === route.path);
    const ssrBlocked = route.facet === "ssr";
    if (candidates.length === 1) {
      const contract = candidates[0];
      const conflict = ssrBlocked || route.facet === "html";
      ctx.addRecord(makeRecord({
        relation: "dev.lekalo.hono/endpoint-contract",
        from: instanceEndpoint(route.instance),
        to: endpointOf(route.terminal),
        method: methods[0],
        path: route.path,
        note: conflict ? `${contract.id}:ssr-api-conflict` : contract.id,
        provenance: "inferred",
        confidence: "medium",
        status: conflict ? "incomplete" : "complete",
        reasons: conflict ? ["ssr-api-conflict"] : [],
        span: route.span,
        revision: ctx.revision,
        adapterVersion: ctx.adapterVersion,
        frameworkVersion: ctx.frameworkVersion,
      }));
    } else if (candidates.length > 1) {
      ctx.addRecord(makeRecord({
        relation: "dev.lekalo.hono/endpoint-contract",
        from: instanceEndpoint(route.instance),
        to: endpointOf(route.terminal),
        method: methods[0],
        path: route.path,
        note: `${candidates.length}-candidates`,
        provenance: "inferred",
        confidence: "low",
        status: "incomplete",
        reasons: ["ambiguous-endpoint-join"],
        span: route.span,
        revision: ctx.revision,
        adapterVersion: ctx.adapterVersion,
        frameworkVersion: ctx.frameworkVersion,
      }));
    } else {
      ctx.addRecord(makeRecord({
        relation: "dev.lekalo.hono/endpoint-contract",
        from: instanceEndpoint(route.instance),
        to: endpointOf(route.terminal),
        method: methods[0],
        path: route.path,
        note: "no-contract",
        provenance: "inferred",
        confidence: "low",
        status: "incomplete",
        reasons: ["missing-endpoint-join"],
        span: route.span,
        revision: ctx.revision,
        adapterVersion: ctx.adapterVersion,
        frameworkVersion: ctx.frameworkVersion,
      }));
    }
  }
}
