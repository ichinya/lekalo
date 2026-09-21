/**
 * The production HTTP/JSON transport generator extension (issue #70,
 * plan S11).
 *
 * One launch-seam extension that turns a validated `generate` request
 * into a deterministic route-layer write plan derived from two
 * evidence homes: the single transport evidence file
 * (`.lekalo/cache/transport/<project>.json`) joined with the
 * compiled-IR evidence (`.lekalo/cache/ir/<project>.json`) that
 * carries the Model endpoint symbols — `method`, `path`, and
 * `invokes` stay single-sourced in the Model, so an unresolvable
 * join refuses, it never emits a null-bearing route. The plan is
 * pure data: one route module per model module, the canonical error
 * envelope, the declared headers, and explicit `unsupported` notes
 * for every declared streaming/upload/download capability this
 * runtime does not implement — never a silent downgrade.
 *
 * The extension reads through the kernel's validated read view only
 * and writes only inside the permitted project root under the
 * declared write root `src/routes/**`. Dry runs never write; applies
 * publish exactly the declared plan.
 */
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

/** The operation this extension serves. */
export const TRANSPORT_OPERATION = "generate";
/** The named capability the extension claims (partial: no OpenAPI). */
export const TRANSPORT_CAPABILITY = "generate.transport-http";
/** The OpenAPI capability stays with its own owner (issue #46). */
export const OPENAPI_CAPABILITY = "generate.openapi";
/** Canonical version of the generator implementation. */
export const EXTENSION_VERSION = "0.4.0";
/** The runtime home of the canonical transport evidence. */
export const TRANSPORT_EVIDENCE_DIR = ".lekalo/cache/transport";
/** The runtime home of the canonical compiled-IR evidence the route
 * layer joins with (the Model endpoint symbols carry `method`, `path`,
 * and `invokes` — single-sourced in the Model, never in the
 * attachment). */
export const IR_EVIDENCE_DIR = ".lekalo/cache/ir";
/** The declared write root of the route layer. */
export const ROUTE_WRITE_ROOT = "src/routes/**";
/** The read trees the deployment profile must cover for the transport
 * generator to enable (the two evidence homes). */
export const TRANSPORT_READ_ROOT = ".lekalo/cache/transport";
export const IR_READ_ROOT = ".lekalo/cache/ir";

/** The exact embedded IR contract identity the join accepts. */
const IR_IDENTITY = "dev.lekalo.ir@0.2.16";

const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

const isObject = (value) => typeof value === "object" && value !== null && !Array.isArray(value);

/** Compact JSON with byte-sorted keys — the canonical JS form. */
const canonicalJson = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (value !== null && typeof value === "object") {
    const body = Object.keys(value)
      .filter((key) => value[key] !== undefined)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
};

/**
 * The bounded evidence decoder: mirrors the closed transport-http
 * wire surface at the level the route layer needs (identity,
 * defaults, endpoints with operation ids, error maps, security, and
 * capabilities). Anything else refuses.
 */
function decodeEvidence(bytes) {
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch {
    return { error: "transport-evidence-invalid" };
  }
  if (!isObject(document)) return { error: "transport-evidence-invalid" };
  if (document.schemaVersion !== "lekalo/transport-http/v0.4.0") {
    return { error: "transport-evidence-version" };
  }
  if (!Array.isArray(document.endpoints) || document.endpoints.length === 0) {
    return { error: "transport-evidence-empty" };
  }
  const endpoints = [];
  for (const endpoint of document.endpoints) {
    if (!isObject(endpoint) || typeof endpoint.endpoint !== "string") {
      return { error: "transport-evidence-invalid" };
    }
    endpoints.push(endpoint);
  }
  endpoints.sort((left, right) => (left.endpoint < right.endpoint ? -1 : 1));
  return {
    value: {
      projectId: document.projectId,
      wire: document.wire,
      defaults: document.defaults,
      securitySchemes: Array.isArray(document.securitySchemes)
        ? document.securitySchemes
        : [],
      endpoints,
    },
  };
}

/**
 * The bounded decoder of the canonical compiled-IR evidence: the one
 * join source for the Model endpoint symbols (`method`, `path`,
 * `invokes`). Anything else refuses.
 */
function decodeIrEvidence(bytes) {
  let document;
  try {
    document = JSON.parse(bytes.toString("utf8"));
  } catch {
    return { error: "ir-evidence-invalid" };
  }
  if (!isObject(document) || document.contract !== IR_IDENTITY) {
    return { error: "ir-evidence-version" };
  }
  if (!Array.isArray(document.definitions)) {
    return { error: "ir-evidence-invalid" };
  }
  const endpoints = new Map();
  for (const definition of document.definitions) {
    if (!isObject(definition)) return { error: "ir-evidence-invalid" };
    if (definition.kind !== "endpoint") continue;
    if (
      typeof definition.id !== "string"
      || typeof definition.method !== "string"
      || typeof definition.path !== "string"
      || typeof definition.invokes !== "string"
    ) {
      return { error: "ir-evidence-invalid" };
    }
    endpoints.set(definition.id, {
      method: definition.method,
      path: definition.path,
      invokes: definition.invokes,
    });
  }
  const projectId = isObject(document.project) && typeof document.project.id === "string"
    ? document.project.id
    : null;
  return { value: { endpoints, projectId } };
}

/** The deterministic operation id (the camel-case derivation). */
function effectiveOperationId(endpoint) {
  if (typeof endpoint.operationId === "string" && endpoint.operationId.length > 0) {
    return endpoint.operationId;
  }
  return endpoint.endpoint
    .split(".")
    .map((segment, index) =>
      index === 0
        ? segment
        : segment
            .split("_")
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
            .join(""),
    )
    .join("");
}

/**
 * The explicit unsupported notes: every declared capability this
 * runtime does not implement is reported, never silently dropped.
 * This generator emits route declarations only — no streaming,
 * upload, or download channel — so every declared capability note is
 * `unsupported`.
 */
function unsupportedNotes(endpoints) {
  const notes = [];
  const seen = new Set();
  for (const endpoint of endpoints) {
    for (const declaration of endpoint.capabilities ?? []) {
      const key = `${declaration.capability}/${declaration.detail ?? "unspecified"}`;
      if (seen.has(key)) continue;
      seen.add(key);
      notes.push({
        capability: declaration.capability,
        detail: declaration.detail ?? "unspecified",
        minimumSupport: declaration.minimumSupport ?? "partial",
        state: "unsupported",
      });
    }
  }
  notes.sort(
    (left, right) =>
      (left.capability + left.detail).localeCompare(right.capability + right.detail),
  );
  return notes;
}

/** Render one deterministic route module for one model module. The
 * Model join (`method`, `path`, `invokes`) is required per route:
 * an unjoined endpoint is a refused plan, never a null-bearing one. */
function routeModuleText(moduleId, endpoints, defaults, joins) {
  const routes = endpoints.map((endpoint) => {
    const joined = joins.get(endpoint.endpoint);
    if (!joined) {
      throw new TypeError("transport-endpoint-unjoined");
    }
    return {
      endpoint: endpoint.endpoint,
      operationId: effectiveOperationId(endpoint),
      method: joined.method,
      path: joined.path,
      invokes: joined.invokes,
      params: endpoint.params ?? [],
      body: endpoint.body ?? null,
      success: endpoint.success ?? null,
      errors: endpoint.errors ?? [],
      errorDefaults: endpoint.errorDefaults ?? null,
      auth: endpoint.auth ?? null,
      headers: endpoint.idempotency || endpoint.correlation
        ? {
            idempotency: endpoint.idempotency ?? null,
            correlation: endpoint.correlation ?? null,
          }
        : null,
    };
  });
  const payload = {
    module: moduleId,
    wire: "lekalo-http-wire/v1",
    errorEnvelope: defaults?.errorEnvelope ?? "canonical-v1",
    // The canonical error envelope: the #62 identity quadruple with
    // public payload fields only; statuses are projections.
    envelope: { error: ["category", "code", "id", "payload"], ok: false },
    routes: [...routes].sort((left, right) =>
      left.operationId.localeCompare(right.operationId),
    ),
  };
  return (
    "// Generated by lekalo-target-node-typescript transport extension " +
      `${EXTENSION_VERSION} — never edit.\n` +
      "// Route declarations derived from the canonical transport evidence\n" +
      "// joined with the compiled-IR evidence (method, path, invokes).\n" +
      `export const routes = ${canonicalJson(payload)};\n`
  );
}

/**
 * Build the route-layer write plan from the transport evidence joined
 * with the compiled-IR endpoint surface (`joins`: one Map from the
 * endpoint symbol to its `{method, path, invokes}`). An endpoint the
 * join cannot resolve throws — the caller refuses the plan, never a
 * null-bearing route.
 */
export function planRouteLayer(evidence, projectId, joins) {
  const byModule = new Map();
  for (const endpoint of evidence.endpoints) {
    if (!joins.has(endpoint.endpoint)) {
      throw new TypeError("transport-endpoint-unjoined");
    }
    const moduleId = endpoint.endpoint.split(".")[0] ?? "default";
    const bucket = byModule.get(moduleId) ?? [];
    bucket.push(endpoint);
    byModule.set(moduleId, bucket);
  }
  const writes = [];
  for (const moduleId of [...byModule.keys()].sort()) {
    const text = routeModuleText(moduleId, byModule.get(moduleId), evidence.defaults, joins);
    writes.push({
      path: `src/routes/${moduleId}.routes.ts`,
      action: "create",
      sha256: sha256Text(text),
      bytes: text,
    });
  }
  return {
    writes: writes.map(({ path, action, sha256 }) => ({ path, action, sha256 })),
    bodies: writes,
    notes: unsupportedNotes(evidence.endpoints),
    projectId,
  };
}

/**
 * The kernel dispatch entry: one generate exchange over the read
 * view. Dry runs never write; applies publish exactly the declared
 * plan inside the permitted root.
 */
/**
 * Resolve the evidence path: the ir filename carries the project id
 * (`<project>.json`), and a file-shaped read root under the evidence
 * home names it explicitly. Anything else is absent — never guessed.
 */
function evidencePathFor(request, readView, home) {
  const candidates = [];
  const irName = request.ir_path?.split("/").pop();
  if (irName?.endsWith(".json")) {
    candidates.push(`${home}/${irName}`);
  }
  for (const root of readView.roots ?? []) {
    if (root.kind === "file" && root.path.startsWith(`${home}/`)) {
      candidates.push(root.path);
    }
  }
  return candidates.find((candidate) => readView.canRead(candidate));
}

export function transportGenerateOperation(context) {
  const { request, readView } = context;
  if (!readView) {
    return { state: "unsupported", diagnostics: [{ reason: "profile-absent" }] };
  }
  const permittedProjectRoot = readView.permittedProjectRoot;
  const evidencePath = evidencePathFor(request, readView, TRANSPORT_EVIDENCE_DIR);
  if (!evidencePath) {
    return {
      state: "failed",
      diagnostics: [{ reason: "transport-evidence-absent" }],
    };
  }
  const decoded = decodeEvidence(readView.readFile(evidencePath));
  if (decoded.error) {
    return { state: "failed", diagnostics: [{ reason: decoded.error }] };
  }
  const evidence = decoded.value;
  // The Model join: the compiled-IR evidence is the one authoritative
  // source of the endpoint method/path/invokes; an absent or unreadable
  // IR refuses instead of planning a null-bearing route layer.
  const irPath = evidencePathFor(request, readView, IR_EVIDENCE_DIR)
    ?? (request.ir_path !== undefined && readView.canRead(request.ir_path)
      ? request.ir_path
      : undefined);
  if (!irPath) {
    return {
      state: "failed",
      diagnostics: [{ reason: "ir-evidence-absent" }],
    };
  }
  const ir = decodeIrEvidence(readView.readFile(irPath));
  if (ir.error) {
    return { state: "failed", diagnostics: [{ reason: ir.error }] };
  }
  // Custody rebind: the transport evidence belongs to the project the
  // IR evidence was compiled from; a mismatched pair refuses instead
  // of generating another project's routes.
  if (!ir.value.projectId || ir.value.projectId !== evidence.projectId) {
    return {
      state: "failed",
      diagnostics: [{ reason: "transport-project-mismatch" }],
    };
  }
  const joins = new Map();
  for (const endpoint of evidence.endpoints) {
    const joined = ir.value.endpoints.get(endpoint.endpoint);
    if (!joined) {
      return {
        state: "failed",
        diagnostics: [{ reason: "transport-endpoint-unjoined" }],
      };
    }
    joins.set(endpoint.endpoint, joined);
  }
  const plan = planRouteLayer(evidence, evidence.projectId ?? "project", joins);
  if (request.dry_run === false) {
    const absolute = resolve(permittedProjectRoot);
    for (const write of plan.bodies) {
      const target = join(absolute, ...write.path.split("/"));
      mkdirSync(join(target, ".."), { recursive: true });
      writeFileSync(target, write.bytes, "utf8");
    }
  }
  return {
    state: "complete",
    data: {
      writes: plan.writes,
      unsupported: plan.notes,
    },
    evidence: {
      transportEvidence: evidencePath,
      projectId: plan.projectId,
      unsupportedCount: plan.notes.length,
    },
  };
}

/** The extension descriptor the bundle entry registers. */
export function transportExtensionDescriptor() {
  return {
    id: "http-transport-generator",
    version: EXTENSION_VERSION,
    operations: [TRANSPORT_OPERATION],
    namedCapabilities: {
      [TRANSPORT_CAPABILITY]: "partial",
      [OPENAPI_CAPABILITY]: "unsupported",
    },
    acceptedIrVersions: ["0.2.16"],
    writeRoots: [ROUTE_WRITE_ROOT],
    readRoots: [TRANSPORT_READ_ROOT, IR_READ_ROOT],
    invoke: (context) => transportGenerateOperation(context),
  };
}
