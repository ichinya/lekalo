// Regenerate the pinned taskhub scan document (issue #49, plan S2) by
// driving the committed adapter's real scanner kernel: the same run the
// wire scan would serialize, minus the wire transport. Path-resilient
// (works from any cwd); the output must stay byte-identical to the
// committed document — that identity is the proof this script is the
// true derivation.
//
// Provenance boundary of the produced document (issue #49, fix round 1):
// kernel-derived members — `stableKey`, `location`, `fingerprint`,
// `candidates` (with their confidences and `mappingConfidence`), the
// adapter identity, and the deterministic `revision`;
// authored-to-the-#39-grammar members — the closed `kind`, the
// structural `evidence` (fields/values/identity/references), the
// snake_cased semantic `id`s, and the endpoint rows (the committed
// route table), all transcribed mechanically from the fixture sources
// below and validated against the kernel's own output. The transport
// itself cannot carry this document: each entry detail token is capped
// at 128 bytes and the @taskhub/* scopes tokenize to ~144, and the
// kernel reports module-granular edges whose anchor spelling never
// matches another entry's native id (the core's resolution rule in
// scan_service), so the edge table below resolves each kernel row to
// the semantic ids of the symbols the exact source line binds.
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const repo = path.resolve(fileURLToPath(import.meta.url), "..", "..");
const fixtureDir = path.join(repo, "tests/fixtures/pilot/taskhub");
const outPath = path.join(
  repo,
  "tests/fixtures/pilot/taskhub-scans/taskhub.scan.v0_2_16.json",
);

const adapter = await import(
  new URL(`file://${path.join(repo, "adapters/node-typescript/adapter.mjs")}`).href
);
const kernel = adapter.__lekaloKernel;
const scanner = adapter.__lekaloScanner;
const sha256Hex = (bytes) => createHash("sha256").update(bytes).digest("hex");
const canonicalText = (value) => {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(canonicalText).join(",")}]`;
  if (typeof value === "object") {
    return `{${Object.keys(value).sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalText(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
};

// The declared launch profile: every source tree, the workspace root
// manifests; `.env` is outside every read root and is never read.
const readRoots = [
  { kind: "tree", path: "packages/tasks" },
  { kind: "tree", path: "packages/events" },
  { kind: "tree", path: "packages/integrations" },
  { kind: "tree", path: "packages/sync" },
  { kind: "tree", path: "apps/api" },
  { kind: "file", path: "package.json" },
  { kind: "file", path: "pnpm-workspace.yaml" },
  { kind: "file", path: "tsconfig.base.json" },
];
const profile = kernel.validateResolvedProjectProfile({
  id: "taskhub-pilot",
  mode: "observed",
  target: "node-typescript",
  readRoots,
  exclusions: [],
  provenance: {
    origin: "declared",
    revision: "taskhub-pilot-0001",
    disposition: "public-fixture",
  },
});
const roots = readRoots.map((entry) => ({
  ...entry,
  scope: entry.kind === "tree" ? `${entry.path}/**` : entry.path,
}));
const readView = kernel.createReadView(fixtureDir, roots, profile);
readView.permittedProjectRoot = fixtureDir;
const index = scanner.runScan({
  profile,
  readView,
  permittedProjectRoot: fixtureDir,
  limits: {},
});
if (index.state !== "complete") throw new Error(`scan state ${index.state}`);
if (index.anyUncertainty.length > 0 || index.diagnostics.length > 0) {
  throw new Error("uncertainty/diagnostics present");
}
const adapterIdentity = adapter.__lekaloAdapterIdentity;

// The deterministic revision binding: the adapter identity plus the
// kernel's own content identity of the run (the profile digest and the
// input-manifest key, both computed by the kernel).
const manifestKey =
  "sha256:" + sha256Hex(Buffer.from(canonicalText(index.inputManifest), "utf8"));
const revision =
  "sha256:" +
  sha256Hex(
    Buffer.from(
      canonicalText([
        adapterIdentity.id,
        adapterIdentity.version,
        adapterIdentity.digest,
        index.profileDigest,
        manifestKey,
      ]),
      "utf8",
    ),
  );

const fileFingerprint = (relative) =>
  "sha256:" + sha256Hex(readFileSync(path.join(fixtureDir, relative)));

// The adapter's scope rule (semanticProposalFor), applied to the
// discovered packages verbatim; the symbol name is the reference
// scanner's snake_case normalization, the spelling the canonical
// symbol grammar the promotion seam requires.
function scopeOf(modulePath) {
  const pkg = index.packages.find(
    (candidate) =>
      modulePath === candidate.root || modulePath.startsWith(`${candidate.root}/`),
  );
  return (
    (pkg?.name ?? "project")
      .replace(/^@/, "")
      .split(/[\\/._-]+/)
      .filter((part) => /^[a-z0-9]+$/i.test(part))
      .join("_")
      .toLowerCase() || "project"
  );
}

// The edge-resolution table: one entry per kernel reference row, keyed
// by (from-file, to-module, line, role), resolved to the adapter's OWN
// proposed semantic ids of the symbols that exact source line binds
// (import statements, call sites). Transcribed mechanically from the
// fixture sources; the generator fails loudly if the kernel carries a
// row this table does not transcribe, or a target no scan entry
// proposes.
const T = {
  tasks: "taskhub_tasks.task",
  tasksState: "taskhub_tasks.task_state",
  tasksCreate: "taskhub_tasks.create_task",
  tasksTransition: "taskhub_tasks.transition_task",
  tasksNewInput: "taskhub_tasks.new_task_input",
  events: "taskhub_events.task_event",
  eventsEncode: "taskhub_events.encode_task_event",
  intCal: "taskhub_integrations.calendar_sync_payload",
  intClient: "taskhub_integrations.integration_client",
  intPayload: "taskhub_integrations.sync_payload",
  intKind: "taskhub_integrations.sync_target_kind",
  intWebhook: "taskhub_integrations.webhook_sync_payload",
  syncToPayload: "taskhub_sync.to_payload",
  apiRoutes: "taskhub_api.routes",
  apiParseTask: "taskhub_api.parse_task",
};
const edgeTable = {
  "packages/sync/src/index.ts": {
    "packages/tasks/src/index.ts:7:reference": [T.tasks],
    "packages/events/src/index.ts:8:reference": [T.events],
    "packages/integrations/src/index.ts:9:reference": [
      T.intCal,
      T.intClient,
      T.intPayload,
      T.intKind,
      T.intWebhook,
    ],
    "packages/integrations/src/index.ts:50:call": [T.intClient],
    "packages/sync/src/index.ts:49:call": [T.syncToPayload],
  },
  "apps/api/src/index.ts": {
    "packages/tasks/src/index.ts:6:reference": [
      T.tasksNewInput,
      T.tasks,
      T.tasksState,
      T.tasksCreate,
      T.tasksTransition,
    ],
    "packages/tasks/src/index.ts:71:call": [T.tasksCreate],
    "packages/tasks/src/index.ts:96:call": [T.tasksTransition],
    "packages/events/src/index.ts:13:reference": [T.events, T.eventsEncode],
    "apps/api/src/index.ts:76:call": [T.apiRoutes],
    "apps/api/src/index.ts:81:call": [T.apiParseTask],
  },
};
for (const row of index.references) {
  const key = `${row.to}:${row.line}:${row.role}`;
  if (edgeTable[row.from]?.[key] === undefined) {
    throw new Error(`untranscribed kernel row ${row.from} -> ${key}`);
  }
}

// The semantic projection of the domain (the #39 evidence grammar, the
// task-domain-scan discipline): the closed kind, structural evidence,
// and the domain-truth edges the sources carry. Symbols absent here
// keep the adapter default (kind `entity`, no structural evidence) and
// never promote — exactly the union/shapeless surfaces. Primitive-typed
// attributes stay unwired fields (recorded as the domain's own scalars
// only where the domain defines one, `task_id`). These tables are
// transcriptions of the fixture sources, nothing invented.
const field = (name, type, required = true) => ({ name, type, required });
const ref = (target, role, confidence = "exact") => ({ target, role, confidence });
const semanticProjection = {
  // packages/tasks - the domain the pilot promotes end to end.
  "taskhub_tasks.task_id": { kind: "scalar", evidence: { base: "string" } },
  "taskhub_tasks.task_state": {
    kind: "enum",
    evidence: {
      values: [
        { value: "backlog", description: "Not started" },
        { value: "focused", description: "In progress" },
        { value: "done", description: "Completed" },
      ],
    },
  },
  "taskhub_tasks.task": {
    kind: "entity",
    evidence: {
      fields: [
        field("task_id", "taskhub_tasks.task_id"),
        field("state", "taskhub_tasks.task_state"),
      ],
      identityFields: ["task_id"],
    },
  },
  "taskhub_tasks.new_task_input": {
    kind: "value-object",
    evidence: { fields: [field("task_id", "taskhub_tasks.task_id")] },
  },
  "taskhub_tasks.create_task": {
    kind: "command",
    semanticRefs: [ref("taskhub_tasks.task", "create")],
  },
  "taskhub_tasks.list_tasks": {
    kind: "query",
    semanticRefs: [ref("taskhub_tasks.task", "read")],
  },
  "taskhub_tasks.transition_task": {
    kind: "command",
    semanticRefs: [ref("taskhub_tasks.task", "update")],
  },
  // packages/events - the external-task envelope surface. The envelope
  // entity's field types carry the source's own member spellings; the
  // payload union (task_event_payload) and task_event_kind keep the
  // adapter default: unions have no renderable canonical form.
  "taskhub_events.task_event": {
    kind: "entity",
    evidence: {
      fields: [
        field("kind", "taskhub_events.task_eventKind"),
        field("payload", "taskhub_events.task_eventPayload"),
      ],
    },
  },
  "taskhub_events.encode_task_event": {
    kind: "command",
    semanticRefs: [ref("taskhub_events.task_event", "read")],
  },
  "taskhub_events.decode_task_event": {
    kind: "query",
    semanticRefs: [ref("taskhub_events.task_event", "read")],
  },
  // packages/integrations - the integration contracts. The webhook
  // payload, DeliveryAck, and the client interface keep the adapter
  // default (string-typed members stay unwired).
  "taskhub_integrations.sync_target_kind": {
    kind: "enum",
    evidence: { values: [{ value: "calendar" }, { value: "webhook" }] },
  },
  "taskhub_integrations.calendar_sync_payload": {
    kind: "value-object",
    evidence: {
      fields: [field("calendar_id", "taskhub_integrations.sync_target_kind")],
    },
  },
  // packages/sync - the worker flow.
  "taskhub_sync.to_payload": {
    kind: "command",
    semanticRefs: [ref("taskhub_integrations.sync_payload", "create")],
  },
  "taskhub_sync.sync_event": {
    kind: "effect",
    semanticRefs: [ref("taskhub_integrations.sync_payload", "create")],
  },
};

const semanticIds = new Set();
const symbols = [];
const topLevel = index.symbols.filter((symbol) => symbol.memberOf === null);
for (const symbol of topLevel) {
  const name = symbol.qualifiedName
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/_+/g, "_")
    .toLowerCase();
  const id = `${scopeOf(symbol.module)}.${name}`;
  if (id.length > 192) throw new Error(`id over bound: ${id}`);
  semanticIds.add(id);
  const projection = semanticProjection[id];
  const kind = projection?.kind ?? "entity";
  const evidence = {};
  if (projection?.evidence?.base !== undefined) evidence.base = projection.evidence.base;
  if (projection?.evidence?.values !== undefined) evidence.values = projection.evidence.values;
  if (projection?.evidence?.fields !== undefined) evidence.fields = projection.evidence.fields;
  if (projection?.evidence?.identityFields !== undefined) {
    evidence.identityFields = projection.evidence.identityFields;
  }
  const derived = edgeTable[symbol.module];
  const references = [];
  if (derived) {
    for (const row of index.references.filter(
      (candidate) => candidate.from === symbol.module,
    )) {
      const key = `${row.to}:${row.line}:${row.role}`;
      const targets = derived[key];
      if (targets === undefined) {
        throw new Error(`untranscribed kernel row ${symbol.module} -> ${key}`);
      }
      for (const target of targets) {
        references.push({ target, role: row.role, confidence: row.confidence });
      }
    }
  }
  for (const semanticRef of projection?.semanticRefs ?? []) {
    references.push({ ...semanticRef });
  }
  references.sort((left, right) =>
    left.target < right.target ? -1 : left.target > right.target ? 1 : 0,
  );
  if (references.length > 0) evidence.references = references;
  const fingerprint = fileFingerprint(symbol.module);
  const record = {
    id,
    kind,
    stableKey: symbol.native,
    location: { path: symbol.module, line: symbol.line },
    fingerprint,
    mappingConfidence: symbol.declarationOnly ? "low" : "medium",
    candidates: [
      {
        native: symbol.native,
        path: symbol.module,
        line: symbol.line,
        fingerprint,
        confidence: symbol.declarationOnly ? "low" : "medium",
      },
    ],
  };
  if (Object.keys(evidence).length > 0) record.evidence = evidence;
  symbols.push(record);
}
for (const symbol of symbols) {
  for (const reference of symbol.evidence?.references ?? []) {
    if (!semanticIds.has(reference.target)) {
      throw new Error(`unresolved target ${reference.target}`);
    }
  }
}
symbols.sort((left, right) => (left.id < right.id ? -1 : 1));

// Endpoints: the committed apps/api route table (ROUTES + openapi.json),
// projected onto the handle dispatch symbol (task-domain-scan precedent).
const endpoints = [
  {
    id: "taskhub_api.tasks_list_route",
    method: "GET",
    path: "/tasks",
    symbol: "taskhub_api.handle",
  },
  {
    id: "taskhub_api.tasks_create_route",
    method: "POST",
    path: "/tasks",
    symbol: "taskhub_api.handle",
  },
  {
    id: "taskhub_api.tasks_transition_route",
    method: "POST",
    path: "/tasks/{task_id}/transition",
    symbol: "taskhub_api.handle",
  },
];

const document = {
  schemaVersion: "lekalo/observed-scan/v0.2.16",
  adapter: {
    id: adapterIdentity.id,
    version: adapterIdentity.version,
    digest: adapterIdentity.digest,
  },
  project: "taskhub",
  revision,
  symbols,
  endpoints,
  target: "node-typescript",
};

writeFileSync(outPath, `${JSON.stringify(document, null, 2)}\n`);
process.stdout.write(
  `taskhub scan regenerated: ${symbols.length} symbols, ${endpoints.length} endpoints, revision ${revision}\n`,
);
