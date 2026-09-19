/**
 * The production HTTP/JSON transport generator extension (issue #70,
 * plan S11).
 *
 * One launch-seam extension that turns a validated `generate` request
 * into a deterministic route-layer write plan derived from the single
 * transport evidence file (`​.lekalo/cache/transport/<project>.json`,
 * the only transport input an adapter may read). The plan is pure
 * data: one route module per model module, the canonical error
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
/** The declared write root of the route layer. */
export const ROUTE_WRITE_ROOT = "src/routes/**";
/** The read tree the deployment profile must cover for the transport
 * generator to enable (the evidence home). */
export const TRANSPORT_READ_ROOT = ".lekalo/cache/transport";

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

/** Render one deterministic route module for one model module. */
function routeModuleText(moduleId, endpoints, defaults) {
  const routes = endpoints.map((endpoint) => ({
    endpoint: endpoint.endpoint,
    operationId: effectiveOperationId(endpoint),
    method: endpoint.method ?? null,
    path: endpoint.path ?? null,
    invokes: endpoint.invokes ?? null,
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
  }));
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
      "// Route declarations derived from the canonical transport evidence.\n" +
      `export const routes = ${canonicalJson(payload)};\n`
  );
}

/**
 * Build the route-layer write plan from the transport evidence.
 * Returns the plan plus the explicit unsupported notes.
 */
export function planRouteLayer(evidence, projectId) {
  const byModule = new Map();
  for (const endpoint of evidence.endpoints) {
    const moduleId = endpoint.endpoint.split(".")[0] ?? "default";
    const bucket = byModule.get(moduleId) ?? [];
    bucket.push(endpoint);
    byModule.set(moduleId, bucket);
  }
  const writes = [];
  for (const moduleId of [...byModule.keys()].sort()) {
    const text = routeModuleText(moduleId, byModule.get(moduleId), evidence.defaults);
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
function evidencePathFor(request, readView) {
  const candidates = [];
  const irName = request.ir_path?.split("/").pop();
  if (irName?.endsWith(".json")) {
    candidates.push(`${TRANSPORT_EVIDENCE_DIR}/${irName}`);
  }
  for (const root of readView.roots ?? []) {
    if (root.kind === "file" && root.path.startsWith(`${TRANSPORT_EVIDENCE_DIR}/`)) {
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
  const evidencePath = evidencePathFor(request, readView);
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
  const plan = planRouteLayer(evidence, evidence.projectId ?? "project");
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
    readRoots: [TRANSPORT_READ_ROOT],
    invoke: (context) => transportGenerateOperation(context),
  };
}
