#!/usr/bin/env node
// Issue #93 live CLI gate: the real binary over a disposable copy of the
// committed planner fixture. Proves the seven acceptance behaviors end
// to end: writers/readers without grep (AC1), exact-ID precedence (AC2),
// requirements/scenarios/bindings joined with provider qualification
// (AC3), card metadata completeness (AC4), ambiguity honesty (AC5),
// offline operation with the durable index lifecycle and cache custody
// (AC6), and the negative/refusal vectors (closed grammar, cursor
// binding, stale rows).
//
// Requires `cargo build --workspace --locked` first (CI builds before
// this gate). Runs fully offline in an empty environment; no adapter,
// network, or target-source access happens, and the gate fails if any
// expected artifact is missing instead of skipping. Every successful
// search/find response is validated live with pinned Ajv 8.20.0 (same
// NODE_PATH / LEKALO_AJV_NODE_PATH provisioning as the contract gate).

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { dirname as dirnamePath } from "node:path";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { realpathSync } from "node:fs";

const root = resolve(dirnamePath(fileURLToPath(import.meta.url)), "..");
const binary =
  process.env.LEKALO_BIN ??
  join(root, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const require = createRequire(
  process.env.LEKALO_AJV_NODE_PATH
    ? `${process.env.LEKALO_AJV_NODE_PATH}/`
    : import.meta.url,
);
let validateResult;
try {
  const version = require("ajv/package.json").version;
  if (version !== "8.20.0") fail("ajv-version", version);
  const Ajv2020 = require("ajv/dist/2020").default;
  validateResult = new Ajv2020({ strict: true, allErrors: true }).compile(
    JSON.parse(readFileSync(join(root, "contracts/search-result.schema.v0.8.0.json"), "utf8")),
  );
} catch (error) {
  fail("live-result-schema-unavailable", error?.message);
}
let liveSchemas = 0;
const validateLiveResult = (document, args) => {
  // Only CLI envelope fields are stripped: all payload members remain
  // subject to the closed result schema, including future additions.
  const { status, diagnostics, reasonCodes, ...payload } = document;
  if (status !== "valid" || !validateResult(payload)) {
    fail("live-result-schema-invalid", { args, status, errors: validateResult.errors });
  }
  liveSchemas += 1;
};

let binaryOk = false;
try {
  binaryOk = statSync(binary).isFile();
} catch {
  binaryOk = false;
}
if (!binaryOk) fail("binary-missing", `run: cargo build --workspace --locked (${binary})`);

const fixture = resolve(root, "tests/fixtures/search/planner");
if (!existsSync(fixture)) fail("fixture-missing", fixture);
const inputsRelative = "evidence/search-inputs.json";
if (!existsSync(join(fixture, inputsRelative))) fail("inputs-missing", inputsRelative);

// The empty offline environment: only what the process needs to start.
const offlineEnv = { PATH: process.env.PATH ?? "" };
if (process.platform === "win32") {
  offlineEnv.SystemRoot = process.env.SystemRoot ?? "";
}

// Disposable project copy; the CLI owns the cache home inside it.
let project = mkdtempSync(join(tmpdir(), "lekalo-search-cli-"));
cpSync(fixture, project, { recursive: true });
rmSync(join(project, ".lekalo"), { recursive: true, force: true });
// GitHub's Windows runners spell %TEMP% with the 8.3 profile alias and
// the selection policy denies alias spellings before any command runs.
project = realpathSync.native(project);

const run = (args) => {
  const result = spawnSync(binary, args, {
    cwd: project,
    encoding: "utf8",
    env: offlineEnv,
  });
  const text = (result.stdout ?? "").trim().length > 0
    ? result.stdout
    : result.stderr ?? "";
  if (result.status === 0 && (args.includes("find") || args.includes("search"))) {
    validateLiveResult(JSON.parse(text), args);
  }
  return {
    code: result.status ?? 1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    json() {
      return JSON.parse(text);
    },
  };
};

let checks = 0;
const ok = (name, condition, detail) => {
  if (!condition) fail(name, detail);
  checks += 1;
};

// Cold canonical-only assembly: no registration or producer can mask
// the coverage vocabulary emitted by the minimal live response (r11).
const coldCanonical = run(["--json", "--no-cache", "find", "--id", "planner.focus_task", "--project", "."]);
ok("r11-cold-result-schema", coldCanonical.code === 0 && coldCanonical.json().coverage.length === 9, coldCanonical.stderr);

// ---------------------------------------------------------------------------
// AC1: writers/readers of a field without grep — exact scope, overlapping
// scope, and the declared/detected trust split.
// ---------------------------------------------------------------------------
const exactWriters = run([
  "--json", "find",
  "--writes", "planner.user_task_planning.focused_at",
  "--project", ".",
  "--inputs", inputsRelative,
]);
ok("ac1-exact-writers-exit", exactWriters.code === 0, exactWriters.stderr);
const exactWritersDoc = exactWriters.json();
ok("ac1-exact-writers-selection", exactWritersDoc.selection === "multiple", exactWritersDoc.selection);
const exactWriterKeys = exactWritersDoc.items.map((item) => item.card.key).sort();
ok(
  "ac1-exact-writers-members",
  JSON.stringify(exactWriterKeys) ===
    JSON.stringify(["def:planner.focus_task", "def:planner.refocus_task"]),
  exactWriterKeys,
);
for (const item of exactWritersDoc.items) {
  const witness = item.card.effects.find((row) => row.relation === "writes:planner.user_task_planning:focused_at");
  ok("ac1-exact-witness", Boolean(witness), item.card.key);
  ok("ac1-witness-trust", witness && witness.trust === "detected", item.card.key);
}

const overlapping = run([
  "--json", "find",
  "--writes", "planner.user_task_planning.focused_at",
  "--effect-scope", "overlapping",
  "--project", ".",
  "--inputs", inputsRelative,
]);
ok("ac1-overlapping-exit", overlapping.code === 0, overlapping.stderr);
const overlappingKeys = overlapping.json().items.map((item) => item.card.key).sort();
ok(
  "ac1-overlapping-superset",
  overlappingKeys.length > exactWriterKeys.length &&
    exactWriterKeys.every((key) => overlappingKeys.includes(key)),
  overlappingKeys,
);

const readers = run([
  "--json", "find",
  "--reads", "planner.user_task_planning.focused_at",
  "--project", ".",
  "--inputs", inputsRelative,
]);
ok("ac1-readers-exit", readers.code === 0, readers.stderr);
const readersDoc = readers.json();
ok("ac1-readers-unique", readersDoc.selection === "unique", readersDoc.selection);
ok(
  "ac1-reader-member",
  readersDoc.items[0]?.card.key === "def:planner.get_focused_task",
  readersDoc.items[0]?.card.key,
);

// ---------------------------------------------------------------------------
// AC2: the exact full ID outranks every lexical/fuzzy result, in both
// modes; fuzzy without the flag answers none.
// ---------------------------------------------------------------------------
const exactSearch = run([
  "--json", "search", "planner.focus_task",
  "--project", ".", "--inputs", inputsRelative,
]);
ok("ac2-exact-exit", exactSearch.code === 0, exactSearch.stderr);
const exactDoc = exactSearch.json();
// The exact identity may be carried by more than one card kind (the
// definition and its recorded binding); every top candidate is exact-id
// and the planner focus command is among them, ahead of all lexical or
// fuzzy results.
const topTier = exactDoc.items[0].match.tier;
ok("ac2-exact-tier", topTier === "exact-id", topTier);
let topGroupEnd = 0;
while (
  topGroupEnd < exactDoc.items.length &&
  exactDoc.items[topGroupEnd].match.tier === "exact-id"
) {
  topGroupEnd += 1;
}
const topKeys = exactDoc.items.slice(0, topGroupEnd).map((item) => item.card.key);
ok(
  "ac2-exact-first",
  topKeys.includes("def:planner.focus_task") &&
    exactDoc.items.slice(topGroupEnd).every((item) => item.match.tier !== "exact-id"),
  topKeys,
);
ok("ac2-exact-edits", exactDoc.items[0].match.edits === 0, exactDoc.items[0].match.edits);

const fuzzyOff = run(["--json", "search", "fokus", "--project", ".", "--inputs", inputsRelative]);
ok("ac2-fuzzy-off-none", fuzzyOff.json().selection === "none", fuzzyOff.json().selection);
const fuzzyOn = run([
  "--json", "search", "fokus", "--fuzzy", "--project", ".", "--inputs", inputsRelative,
]);
const fuzzyDoc = fuzzyOn.json();
ok("ac2-fuzzy-on-hits", fuzzyDoc.items.length >= 2, fuzzyDoc.items.length);
ok(
  "ac2-fuzzy-labeled",
  fuzzyDoc.items.every((item) => item.match.tier === "fuzzy" && item.match.label === "lexical-fuzzy"),
  fuzzyDoc.items[0]?.match,
);
ok("ac2-fuzzy-ambiguous", fuzzyDoc.selection === "ambiguous", fuzzyDoc.selection);
ok(
  "ac2-fuzzy-diagnostic",
  (fuzzyDoc.reasonCodes ?? []).includes("search.ambiguous"),
  fuzzyDoc.reasonCodes,
);

// ---------------------------------------------------------------------------
// AC3/AC5: the requirement chain joins the trace producers; the same
// external ID across two providers is ambiguous with both qualified
// candidates visible; `--requirement-source` resolves determinately.
// The same-name query pair is ambiguous; both candidates stay visible.
// ---------------------------------------------------------------------------
const unqualified = run([
  "--json", "find",
  "--requirement", "PLANNER-REQ-001",
  "--project", ".", "--inputs", inputsRelative,
]);
const unqualifiedDoc = unqualified.json();
ok("ac3-unqualified-ambiguous", unqualifiedDoc.selection === "ambiguous", unqualifiedDoc.selection);
const requirementKeys = unqualifiedDoc.items.map((item) => item.card.key);
ok(
  "ac3-both-providers-visible",
  requirementKeys.includes("req:openspec:PLANNER-REQ-001") &&
    requirementKeys.includes("req:aifhub:PLANNER-REQ-001"),
  requirementKeys,
);
const implementing = unqualifiedDoc.items
  .filter((item) => item.card.kind === "command")
  .map((item) => item.card.key);
ok(
  "ac3-symbols-joined",
  implementing.includes("def:planner.focus_task") &&
    implementing.includes("def:planner.refocus_task"),
  implementing,
);

const qualified = run([
  "--json", "find",
  "--requirement", "PLANNER-REQ-001",
  "--requirement-source", "openspec",
  "--project", ".", "--inputs", inputsRelative,
]);
const qualifiedDoc = qualified.json();
// AC3: the requirement selector resolves the whole recorded chain —
// the provider-qualified requirement card and every card joined to it
// (definitions, scenario, gate) — with the trace's admitted partial
// coverage disclosed.
const qualifiedKeys = qualifiedDoc.items.map((item) => item.card.key);
ok(
  "ac3-qualified-chain",
  qualifiedKeys.includes("req:openspec:PLANNER-REQ-001") &&
    qualifiedKeys.includes("def:planner.focus_task") &&
    qualifiedKeys.includes("def:planner.focus_flow"),
  qualifiedKeys,
);
ok(
  "ac3-qualified-selection",
  qualifiedDoc.selection === "multiple",
  qualifiedDoc.selection,
);
ok("ac3-qualified-trace-partial", qualifiedDoc.coverage.some((row) => row.domain === "trace" && row.state === "partial"), qualifiedDoc.coverage);

const sameName = run(["--json", "search", "get_status", "--project", ".", "--inputs", inputsRelative]);
const sameNameDoc = sameName.json();
ok("ac5-same-name-ambiguous", sameNameDoc.selection === "ambiguous", sameNameDoc.selection);
const sameNameKeys = sameNameDoc.items.map((item) => item.card.key);
ok(
  "ac5-same-name-candidates",
  sameNameKeys.includes("def:notify.get_status") && sameNameKeys.includes("def:planner.get_status"),
  sameNameKeys,
);

// ---------------------------------------------------------------------------
// AC4: the card answers without opening a file: kind/module, signature,
// description, requirement, relation witness, provenance, freshness.
// The exact identity ties across the definition and its recorded
// binding card; assert on the definition card of the tied top group.
// ---------------------------------------------------------------------------
const exactItems = exactDoc.items;
const definitionCard =
  exactItems.find((item) => item.card.key === "def:planner.focus_task").card;
const card = definitionCard;
ok("ac4-kind", card.kind === "command", card.kind);
ok("ac4-module", card.module === "planner", card.module);
ok("ac4-signature", card.signatures.some((row) => row.id === "planner.task_id" && row.role === "input"), card.signatures);
ok("ac4-description", typeof card.description === "string" && card.description.length > 0 && card.description.length <= 512, card.description);
ok("ac4-requirement", card.requirements.includes("PLANNER-REQ-001"), card.requirements);
ok(
  "ac4-relations",
  card.relations.some((row) => row.relation === "command-effect" && row.other === "def:planner.mark_task_focused"),
  card.relations,
);
ok("ac4-provenance", card.provenance === "ir", card.provenance);
ok("ac4-freshness", card.freshness === "current", card.freshness);
ok("ac4-completeness", card.completeness === "complete", card.completeness);

// Native bindings: the observed producer's paths resolve through typed
// identities, prefix matching never crosses component boundaries.
const bindings = run([
  "--json", "find",
  "--binding-path", "apps/api/src/planner",
  "--project", ".", "--inputs", inputsRelative,
]);
ok("ac4-bindings-exit", bindings.code === 0, bindings.stderr);
const bindingKeys = bindings.json().items.map((item) => item.card.key);
ok("ac4-bindings-hit", bindingKeys.length >= 3, bindingKeys);
const noCrossing = run([
  "--json", "find",
  "--binding-path", "apps/api/src/planner-old",
  "--project", ".", "--inputs", inputsRelative,
]);
ok("ac4-prefix-no-crossing", noCrossing.json().selection === "none", noCrossing.json().selection);
const nativeSymbol = run([
  "--json", "find",
  "--native-symbol", "apps/api/src/planner/focus-task.ts#focusTaskById",
  "--project", ".", "--inputs", inputsRelative,
]);
const nativeDoc = nativeSymbol.json();
// The second candidate answers the exact native-symbol filter on BOTH
// its cards (the binding card and the definition card), each carrying
// the complete admitted candidate set.
const nativeKeys = nativeDoc.items.map((item) => item.card.key);
ok(
  "ac4-native-symbol",
  nativeDoc.selection === "multiple" &&
    nativeKeys.includes("bind:planner.focus_task") &&
    nativeKeys.includes("def:planner.focus_task"),
  nativeKeys,
);

// ---------------------------------------------------------------------------
// AC5: stale rows stay out until --include-stale; zero matches is valid.
// ---------------------------------------------------------------------------
const revoked = run([
  "--json", "find", "--waiver", "lint-focus-debt-revoked",
  "--project", ".", "--inputs", inputsRelative,
]);
ok("ac5-revoked-excluded", revoked.json().selection === "none", revoked.json().selection);
const included = run([
  "--json", "find", "--waiver", "lint-focus-debt-revoked", "--include-stale",
  "--project", ".", "--inputs", inputsRelative,
]);
const includedDoc = included.json();
ok("ac5-include-stale", includedDoc.selection === "unique", includedDoc.selection);
ok("ac5-stale-freshness", includedDoc.items[0].card.freshness === "stale", includedDoc.items[0].card.freshness);

const noMatch = run(["--json", "search", "xyzzynomatch", "--project", "."]);
const noMatchDoc = noMatch.json();
ok("ac5-zero-valid", noMatch.code === 0 && noMatchDoc.status === "valid" && noMatchDoc.selection === "none", noMatchDoc.selection);

// ---------------------------------------------------------------------------
// AC6: offline index lifecycle — build, status, cursor pages bound to
// the generation, --no-cache transient equality, cache-clear custody.
// No adapter/network access happens anywhere in this gate (empty env).
// ---------------------------------------------------------------------------
const build = run([
  "search-index", "build", "--project", ".", "--inputs", inputsRelative,
]);
ok("ac6-build-exit", build.code === 0, build.stderr);
// Either honest receipt is correct: `built` when a new generation was
// published, `current` when the identical generation is already the
// published registration (a canonical-only query must never clobber a
// manifest registration, so the admitted generation can legitimately
// already be current here).
ok(
  "ac6-build-receipt",
  build.stdout.includes("search index built") || build.stdout.includes("search index current"),
  build.stdout,
);

const status = run(["--json", "search-index", "status", "--project", "."]);
const statusDoc = status.json();
ok("ac6-status-ready", statusDoc.searchIndex.state === "ready", statusDoc.searchIndex);
ok("ac6-status-cards", statusDoc.searchIndex.cards >= 20, statusDoc.searchIndex.cards);

// No explicit manifest: this response is served from the registered
// generation, not the cold assembly; run() schema-validates it too.
const registeredWarm = run(["--json", "find", "--id", "planner.focus_task", "--project", "."]);
ok("r11-registered-warm-result-schema", registeredWarm.code === 0 && registeredWarm.json().coverage.length === 9, registeredWarm.stderr);

// Warm pagination walks the whole ranked set with no repeats/skips.
const warmSeen = new Set();
let cursor;
for (let page = 0; page < 40; page += 1) {
  const args = [
    "--json", "find", "--kind", "command", "--limit", "3", "--project", ".",
    "--inputs", inputsRelative,
  ];
  if (cursor) args.push("--cursor", cursor);
  const pageRun = run(args);
  ok(`ac6-page-${page}-exit`, pageRun.code === 0, pageRun.stderr);
  const doc = pageRun.json();
  for (const item of doc.items) {
    ok(`ac6-page-${page}-repeat`, !warmSeen.has(item.card.key), item.card.key);
    warmSeen.add(item.card.key);
  }
  if (!doc.page.next) break;
  cursor = doc.page.next;
}
ok("ac6-paged-rows", warmSeen.size >= 4, warmSeen.size);

// The cursor rejects a foreign query and garbage, both fail-closed.
const foreign = run([
  "--json", "find", "--kind", "query", "--limit", "3",
  "--cursor", cursor, "--project", ".", "--inputs", inputsRelative,
]);
ok("ac6-cursor-foreign", foreign.code === 1 && foreign.json().diagnostics[0].id === "search.cursor-invalid", foreign.code);
const garbage = run([
  "--json", "find", "--kind", "command", "--limit", "3",
  "--cursor", "AAAA:BBBB", "--project", ".", "--inputs", inputsRelative,
]);
ok("ac6-cursor-garbage", garbage.code === 1 && garbage.json().diagnostics[0].data.detail === "malformed", garbage.code);

// Cold transient (--no-cache) answers match the warm envelope when the
// same inputs are admitted: the cache is a pure acceleration whose
// output is byte-identical. (--no-cache never reads the cache home;
// evidence files are project inputs, not cache.)
const warm = run([
  "--json", "find", "--writes", "planner.user_task_planning.focused_at",
  "--project", ".", "--inputs", inputsRelative,
]);
const cold = run([
  "--no-cache", "--json", "find", "--writes", "planner.user_task_planning.focused_at",
  "--project", ".", "--inputs", inputsRelative,
]);
ok("ac6-cold-equals-warm", cold.stdout === warm.stdout, {
  warm: warm.stdout.slice(0, 120),
  cold: cold.stdout.slice(0, 120),
});

// Without --inputs the optional producer domains report unknown, never
// fabricated completeness (this is a --no-cache run).
const withoutInputs = run([
  "--no-cache", "--json", "find", "--kind", "command", "--project", ".",
]);
const coldCoverage = withoutInputs.json().coverage;
ok(
  "ac6-cold-coverage-unknown",
  coldCoverage.some((row) => row.domain === "trace" && row.state === "unknown"),
  coldCoverage,
);

// Cache custody: cache clear removes the search generations and keeps
// the migration home byte-for-byte.
const migrations = join(project, ".lekalo/cache/migrations");
const { mkdirSync, writeFileSync } = await import("node:fs");
mkdirSync(migrations, { recursive: true });
writeFileSync(join(migrations, "keep.bin"), "custody");
const clear = run(["cache", "clear", "--yes", "--project", "."]);
ok("ac6-clear-exit", clear.code === 0, clear.stderr);
ok("ac6-migration-custody", existsSync(join(migrations, "keep.bin")), "keep.bin survives");
ok("ac6-generation-cleared", !existsSync(join(project, ".lekalo/cache/search/current.json")), "pointer removed");

// A build under --no-cache is a usage error: the bypass never writes.
const bypass = run(["--json", "--no-cache", "search-index", "build", "--project", "."]);
ok("ac6-bypass-refused", bypass.code === 1 && bypass.json().diagnostics[0].id === "cli.usage", bypass.code);

// Closed grammar refusals: bare find, an unknown kind, an out-of-range
// limit, and effect-scope without reads/writes are all search.query-
// invalid at exit 1.
const bare = run(["--json", "find", "--project", "."]);
ok("refuse-bare-find", bare.code === 1 && bare.json().diagnostics[0].id === "search.query-invalid", bare.code);
const unknownKind = run(["--json", "find", "--kind", "who", "--project", "."]);
ok("refuse-unknown-kind", unknownKind.code === 1 && unknownKind.json().diagnostics[0].id === "search.query-invalid", unknownKind.code);
const badLimit = run(["--json", "find", "--kind", "command", "--limit", "101", "--project", "."]);
ok("refuse-limit", badLimit.code === 1 && badLimit.json().diagnostics[0].id === "search.query-invalid", badLimit.code);
const orphanScope = run([
  "--json", "find", "--kind", "command", "--effect-scope", "overlapping", "--project", ".",
]);
ok("refuse-orphan-scope", orphanScope.code === 1 && orphanScope.json().diagnostics[0].id === "search.query-invalid", orphanScope.code);

// Privacy canaries: a refused traversal input never echoes the path and
// no cursor carries raw path/query bytes.
const { createHash } = await import("node:crypto");
const canary = "planner-dotdot-canary";
const traversal = run([
  "--json", "find", "--kind", "command",
  "--inputs", `../${canary}.json`, "--project", ".",
]);
ok("refuse-traversal", traversal.code === 1, traversal.code);
ok("privacy-no-canary-echo", !traversal.stdout.includes(canary) && !traversal.stderr.includes(canary), "canary absent");
const pageOne = run([
  "--json", "find", "--kind", "command", "--limit", "2", "--project", ".",
  "--inputs", inputsRelative,
]);
const next = pageOne.json().page.next ?? "";
ok("privacy-cursor-opaque", next.length > 0 && !next.includes("planner") && !next.includes("focus"), "cursor carries no subjects");


// ---------------------------------------------------------------------------
// F11: the mandatory producer families answer with positive and negative
// probes, and their closed decoders refuse hostile documents.
// ---------------------------------------------------------------------------
const errorBound = run([
  "--json", "find", "--error", "planner.store_unavailable",
  "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "f11-error-signatures",
  errorBound.json().selection === "multiple" &&
    errorBound.json().items.map((item) => item.card.key).includes("def:planner.focus_task"),
  errorBound.json().items.map((item) => item.card.key),
);
const errorNegative = run([
  "--json", "find", "--error", "planner.nonexistent_error",
  "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "f11-error-negative",
  errorNegative.json().status === "valid" && errorNegative.json().selection === "none",
  errorNegative.json().selection,
);
const diagnostics = run([
  "--json", "find", "--kind", "diagnostic", "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "f11-diagnostic-cards",
  diagnostics.json().selection === "multiple" &&
    diagnostics.json().items.every((item) => item.card.kind === "diagnostic") &&
    diagnostics.json().items.length >= 2,
  diagnostics.json().items.map((item) => item.card.key),
);
const diagnosticRule = run([
  "--json", "find", "--diagnostic", "planner.focus-concurrency",
  "--project", ".", "--inputs", inputsRelative,
]);
const diagnosticRuleKeys = diagnosticRule.json().items.map((item) => item.card.key);
ok(
  "f11-diagnostic-rule",
  // The reverse join answers the finding card AND its subject beside
  // it — the rule lookup is multiple, never a lone finding cut off
  // from what it evidences.
  diagnosticRule.json().selection === "multiple" &&
    diagnosticRuleKeys.includes("diag:focus-concurrency-001") &&
    diagnosticRuleKeys.includes("def:planner.focus_task"),
  diagnosticRuleKeys,
);
const profile = run([
  "--json", "find", "--profile", "focus-critical",
  "--project", ".", "--inputs", inputsRelative,
]);
const profileKeys = profile.json().items.map((item) => item.card.key);
ok(
  "f11-profile-membership",
  profile.json().selection === "multiple" &&
    profileKeys.includes("def:planner.focus_task") &&
    profileKeys.includes("def:planner.refocus_task"),
  profileKeys,
);
const profileNegative = run([
  "--json", "find", "--profile", "no-such-profile",
  "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "f11-profile-negative",
  profileNegative.json().status === "valid" && profileNegative.json().selection === "none",
  profileNegative.json().selection,
);
const ownership = run([
  "--json", "find", "--ownership", "project",
  "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "f11-ownership-facts",
  ownership.json().selection !== "none" &&
    ownership.json().items.map((item) => item.card.key).includes("def:planner.focus_task"),
  ownership.json().items.map((item) => item.card.key),
);

// Closed decoder refusals for the search-scoped diagnostic-set wire: an
// unknown member and a forged count both refuse with registered rules.
const baseManifest = JSON.parse(readFileSync(join(project, inputsRelative), "utf8"));
const diagDoc = JSON.parse(readFileSync(join(project, "evidence/diagnostics.json"), "utf8"));
const writeManifestFor = (doc, name) => {
  writeFileSync(join(project, "evidence", name), JSON.stringify(doc));
  const manifest = JSON.parse(JSON.stringify(baseManifest));
  manifest.inputs = [{ kind: "diagnostics", path: `evidence/${name}`, identity: "dev.lekalo.search-diagnostics@0.8.0" }];
  writeFileSync(join(project, "evidence/probe-manifest.json"), JSON.stringify(manifest));
  return "evidence/probe-manifest.json";
};
const unknownMember = JSON.parse(JSON.stringify(diagDoc));
unknownMember.findings[0].unfamiliar = true;
let hostileManifest = writeManifestFor(unknownMember, "diag-hostile.json");
const refusedUnknown = run([
  "--json", "find", "--kind", "command", "--project", ".", "--inputs", hostileManifest,
]);
ok(
  "f11-diagnostics-closed-decode",
  refusedUnknown.code === 1 && refusedUnknown.json().diagnostics[0].id === "search.input-invalid",
  refusedUnknown.json().diagnostics[0]?.data?.detail,
);
const forgedCount = JSON.parse(JSON.stringify(diagDoc));
forgedCount.metadata.findingCount = 99;
hostileManifest = writeManifestFor(forgedCount, "diag-hostile.json");
const refusedForged = run([
  "--json", "find", "--kind", "command", "--project", ".", "--inputs", hostileManifest,
]);
ok(
  "f11-diagnostics-forged-count",
  refusedForged.code === 1 && refusedForged.json().diagnostics[0].id === "search.input-invalid",
  refusedForged.json().diagnostics[0]?.data?.detail,
);
rmSync(join(project, "evidence/probe-manifest.json"), { force: true });
rmSync(join(project, "evidence/diag-hostile.json"), { force: true });

// ---------------------------------------------------------------------------
// Issue #93 r4 probes: schema/runtime parity for the effects producer,
// the typed observed-ownership property, missing-domain coverage
// disclosure, and evidence-custody refusals for ghost/conflicting
// diagnostic identities.
// ---------------------------------------------------------------------------
// (a) The effects producer's modelVersion is the published closed enum
// (0.2.16), not a semver pattern: a syntactically valid foreign
// version refuses exactly like Ajv does.
const foreignEffects = JSON.parse(readFileSync(join(project, "evidence/effects.detected.json"), "utf8"));
foreignEffects.modelVersion = "99.0.0";
writeFileSync(join(project, "evidence/effects.detected.json"), JSON.stringify(foreignEffects));
const refusedVersion = run([
  "--json", "find", "--writes", "planner.user_task_planning.focused_at",
  "--project", ".", "--inputs", inputsRelative,
]);
ok(
  "r4-effects-model-version-parity",
  refusedVersion.code === 1 && refusedVersion.json().diagnostics[0].id === "search.input-invalid",
  refusedVersion.json().diagnostics[0]?.data?.detail,
);
// Restore the committed producer bytes.
const effectsPath = join(project, "evidence/effects.detected.json");
const originalEffects = readFileSync(join(fixture, "evidence/effects.detected.json"), "utf8");
writeFileSync(effectsPath, originalEffects);

// (b) `--ownership observed` resolves the typed observed-ownership
// property (never Model visibility).
const observed = run([
  "--json", "find", "--ownership", "observed",
  "--project", ".", "--inputs", inputsRelative,
]);
const observedKeys = observed.json().items.map((item) => item.card.key);
ok(
  "r4-ownership-observed",
  observed.json().selection === "multiple" &&
    observedKeys.includes("def:planner.user_task_planning") &&
    observedKeys.every((key) => key.includes("user_task_planning")),
  observedKeys,
);

// (c) A needed-but-missing producer domain is disclosed, never a
// confident zero: under the cold bypass (no registration, no manifest)
// --error answers over unknown coverage with the coverage warning and
// no unique verdict. (A warm query without a manifest legitimately
// reuses the saved registration, whose error coverage is complete.)
const errorNoManifest = run([
  "--json", "--no-cache", "find", "--error", "planner.task_not_found", "--project", ".",
]);
ok(
  "r4-missing-domain-disclosure",
  errorNoManifest.code === 0 &&
    (errorNoManifest.json().reasonCodes ?? []).includes("search.coverage-incomplete") &&
    errorNoManifest.json().selection !== "unique",
  { selection: errorNoManifest.json().selection, codes: errorNoManifest.json().reasonCodes },
);

// (d) Evidence custody: a ghost diagnostic subject and a conflicting
// diagnostic identity are typed refusals, never silently merged cards.
const writeManifestFor2 = (document, name) => {
  const manifest = { schemaVersion: "lekalo/search-inputs/v0.8.0", identity: "dev.lekalo.search-inputs@0.8.0", projectId: "planner", inputs: [{ kind: "diagnostics", path: `evidence/${name}`, identity: "dev.lekalo.search-diagnostics@0.8.0" }] };
  writeFileSync(join(project, `evidence/${name}`), JSON.stringify(document));
  writeFileSync(join(project, "evidence/probe-manifest.json"), JSON.stringify(manifest));
  return "evidence/probe-manifest.json";
};
const ghost = JSON.parse(readFileSync(join(project, "evidence/diagnostics.json"), "utf8"));
ghost.findings[0].subject = "planner.does_not_exist";
let hostile2 = writeManifestFor2(ghost, "diag-ghost.json");
const refusedGhost = run([
  "--json", "find", "--kind", "command", "--project", ".", "--inputs", hostile2,
]);
ok(
  "r4-ghost-diagnostic-subject",
  refusedGhost.code === 1 && refusedGhost.json().diagnostics[0].data.detail === "subject-unknown",
  refusedGhost.json().diagnostics[0]?.data?.detail,
);
const conflicting = JSON.parse(readFileSync(join(project, "evidence/diagnostics.json"), "utf8"));
conflicting.metadata.findingCount = 3;
conflicting.findings.push({
  id: "focus-concurrency-001",
  rule: "planner.different-rule",
  subject: "planner.focus_task",
  severity: "warning",
  disposition: "active",
});
hostile2 = writeManifestFor2(conflicting, "diag-conflict.json");
const refusedConflict = run([
  "--json", "find", "--kind", "command", "--project", ".", "--inputs", hostile2,
]);
ok(
  "r4-conflicting-diagnostic-identity",
  refusedConflict.code === 1 && refusedConflict.json().diagnostics[0].data.detail === "identity-conflict",
  refusedConflict.json().diagnostics[0]?.data?.detail,
);
rmSync(join(project, "evidence/probe-manifest.json"), { force: true });
rmSync(join(project, "evidence/diag-ghost.json"), { force: true });
rmSync(join(project, "evidence/diag-conflict.json"), { force: true });

// ---------------------------------------------------------------------------
// Scale envelope (issue #93 r6/r8): the r5 review measured the
// warm/query paths at ~47 s per cached query and ~45 s `status` at
// ~4k cards (superlinear derivation), invisible at the 36-card fixture
// scale. This gate synthesizes a ~1.7k-card project and enforces a
// generous per-invocation deadline on the warm paths. Mechanism since
// r7 (memo removed): linear duplicate-key lexer + always-decisive full
// derivation proof, ~2-3 s at this scale in debug builds; the
// quadratic/pre-lexer mechanism measured ~21 s warm and ~13 s status
// on the same machine. No cache sidecar is trusted — warm queries
// always re-derive. Measured seconds are recorded in the summary so
// the envelope stays auditable, not gamed.
// ---------------------------------------------------------------------------
let scaleProject = mkdtempSync(join(tmpdir(), "lekalo-search-cli-scale-"));
cpSync(fixture, scaleProject, { recursive: true });
rmSync(join(scaleProject, ".lekalo"), { recursive: true, force: true });
scaleProject = realpathSync.native(scaleProject);
for (let m = 0; m < 150; m++) {
  const dir = join(scaleProject, "lekalo", "modules", `scale${m}`);
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    join(dir, "module.yaml"),
    `schema_version: "0.2.16"\ndefinitions:\n  - id: scale${m}\n    kind: module\n    version: 1\n    description: "Scale module ${m}"\n    imports:\n      - notify\n`,
  );
  let yaml = 'schema_version: "0.2.16"\ndefinitions:\n';
  for (let e = 0; e < 10; e++) {
    yaml += `  - id: scale${m}.entity${e}\n    kind: entity\n    version: 1\n    description: "Scale entity ${m} ${e} umlaut aeoeue"\n    fields:\n      - name: label\n        type: "notify.text"\n        required: true\n    identity:\n      - label\n`;
  }
  writeFileSync(join(dir, "entities.yaml"), yaml);
}
const runScale = (args) => {
  const result = spawnSync(binary, args, { cwd: scaleProject, encoding: "utf8", env: offlineEnv });
  if (result.status === 0 && (args.includes("find") || args.includes("search"))) {
    validateLiveResult(JSON.parse(result.stdout), args);
  }
  return result;
};
const scaleBuild = runScale([
  "--json", "search-index", "build", "--project", ".", "--inputs", inputsRelative,
]);
ok("r6-scale-build-exit", scaleBuild.status === 0, scaleBuild.stderr);
const scaleCards = JSON.parse((scaleBuild.stdout ?? "").trim().length > 0 ? scaleBuild.stdout : scaleBuild.stderr).cards;
ok("r6-scale-cards", scaleCards > 1_500, scaleCards);
const SCALE_DEADLINE_MS = 10_000;
const timedScale = (name, args, verify) => {
  const started = Date.now();
  const result = runScale(args);
  const seconds = (Date.now() - started) / 1000;
  ok(`${name}-exit`, result.status === 0, result.stderr);
  verify(result);
  ok(`${name}-deadline`, Date.now() - started < SCALE_DEADLINE_MS, `${seconds.toFixed(2)} s`);
  return seconds;
};
const scaleWarm = timedScale("r6-scale-warm-find", ["--json", "find", "--id", "planner.focus_task", "--project", "."], (result) => {
  ok("r6-scale-warm-exact", JSON.parse(result.stdout).items[0].card.id === "planner.focus_task", result.stdout);
});
const scaleStatus = timedScale("r6-scale-status", ["--json", "search-index", "status", "--project", "."], (result) => {
  ok("r6-scale-status-ready", JSON.parse(result.stdout).searchIndex.state === "ready", result.stdout);
});
const scaleUpdate = timedScale("r6-scale-noop-update", ["--json", "search-index", "update", "--project", "."], (result) => {
  ok("r6-scale-update-reused", JSON.parse(result.stdout).reused === true, result.stdout);
});
rmSync(scaleProject, { recursive: true, force: true });

process.stdout.write(
  `${JSON.stringify({ ok: true, checks, liveSchemas, project: "disposable temp copy", scale: { cards: scaleCards, warmFindSeconds: +scaleWarm.toFixed(2), statusSeconds: +scaleStatus.toFixed(2), noopUpdateSeconds: +scaleUpdate.toFixed(2), deadlineSeconds: SCALE_DEADLINE_MS / 1000 } }, null, 2)}\n`,
);
