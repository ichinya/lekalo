#!/usr/bin/env node
// Issue #96: the explain catalog generator.
//
// Explicit authoring of the versioned explanation catalog: one stable,
// per-code entry for every diagnostic in the closed 0.8.0 union
// registry. Running without arguments (re)writes the committed contract
// artifact; `--check` is read-only and used by the live gate. Output is
// canonical (deep key-sorted, two-space pretty) so the gate's byte
// comparison proves the committed catalog stays in sync with the
// registry union.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const version = "0.8.0";
const registryPath = "contracts/diagnostic-registry.v0.8.0.json";
const catalogPath = "contracts/explain-catalog.v0.8.0.json";

// Deep key-sorted canonical JSON: identical input data always produces
// identical bytes, so drift is detectable byte-exactly.
export const ordered = (v) =>
  Array.isArray(v) ? v.map(ordered)
    : v && typeof v === "object"
      ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, ordered(v[k])]))
      : v;
export const canonical = (v) => JSON.stringify(ordered(v));
export const pretty = (v) => JSON.stringify(ordered(v), null, 2) + "\n";
export const sha256 = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;

const registry = JSON.parse(readFileSync(resolve(root, registryPath), "utf8"));
assert.equal(registry.schema_version, "lekalo/diagnostic-registry/v0.8.0", "registry pin");
assert.equal(registry.closed, true, "registry closure");

// ---------------------------------------------------------------------------
// Family templates. Each family gets one prose skeleton; every code still
// yields its own distinct entry because rule-specific facts (the registry's
// default message, severity, category, allowed statuses) are injected into
// the wire entry, and the profile carries the family-level detail.
// ---------------------------------------------------------------------------

// Every fix class mentioned by a template must stay within the closed
// registry vocabulary (mirrors fix::Class: safe/unsafe/breaking/manual-only).
const CLASS_PATTERN = /^(safe|unsafe|breaking|manual-only)$/;

// Family profile: (kind) explains what the rule family checks; (matters)
// explains the correctness/portability/AI-readability stake; (causes) lists
// common root causes; (targets) carries target-specific notes.
const families = {
  "adapter": {
    title: "Adapter conformance check failed",
    kind: "verifies the conformance evidence an adapter produces for its declared target; the adapter host refuses to trust an adapter whose checks or contracts do not hold",
    matters: "a non-conforming adapter can silently corrupt generated artifacts, misreport target behavior, or break the portability guarantees every downstream consumer relies on",
    causes: [
      "the installed adapter version does not match the contract version the project pins",
      "the adapter's generated artifacts or inventory were edited by hand after generation",
      "the target toolchain or runtime changed underneath the adapter",
      "conformance evidence was produced against a different project revision",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the adapter's documented install/update flow so checks and evidence are regenerated against the pinned contract versions; never edit conformance evidence by hand." },
      { class: "manual-only", recipeId: null, description: "Reproduce the failing check with the adapter's own diagnostics before changing any project source." },
    ],
    before: `lekalo adapter test --target demo`,
    after: `# Fix the mismatch the conformance check reports (for example the\n# pinned contract version), then re-install or re-run the adapter:\nlekalo adapter update --target demo\nlekalo adapter test --target demo`,
    targets: "Target-specific: the exact check names and artifact homes are defined by the target's adapter SDK contract; consult the adapter's manifest for its declared conformance surface.",
    docs: "docs/adapter-conformance.md",
    waiver: "Waivable only for explicitly trusted local development adapters; the underlying conformance gap remains and must be re-checked after any adapter or contract update.",
  },
  "adoption": {
    title: "Admission, evidence or recovery failure in the brownfield adoption service",
    kind: "admits a brownfield adoption flow (inventory, consent, refactor candidate, verification, recovery) and enforces its evidence chain end to end",
    matters: "adoption decisions drive automated source changes; without exact consent and verification evidence the service could modify code no human reviewed or lose the ability to roll back",
    causes: [
      "the adoption plan, evidence or receipt attachment was edited after admission",
      "a refactor candidate references symbols absent from the current baseline run",
      "the verification spec was satisfied partially, and the residue was not reviewed",
      "custody artifacts (rollback refs, digests) were moved or deleted between steps",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the failing adoption artifact with its documented producer command and re-run the admission step; never hand-edit evidence, receipts or custody digests." },
    ],
    before: `lekalo adopt plan --from inventory.json`,
    after: `# Regenerate the artifact the diagnostic names, then re-admit:\nlekalo adopt plan --from inventory.json   # fresh, unedited artifact\n# or restart the flow at the step the diagnostic identifies`,
    targets: "Target-neutral: adoption operates on semantic IDs and Model attachments, never on target source.",
    docs: "docs/adoption.md",
    waiver: "Not waivable: admission and consent failures are custody boundaries; re-run the flow with corrected inputs instead.",
  },
  "ai-lint": {
    title: "AI lint policy or evidence failure",
    kind: "checks that AI-authored code carries the policy-declared evidence and that the evidence satisfies the configured lint policy",
    matters: "lint results about AI changes are only as trustworthy as their evidence; skipping this lets unreviewed machine-written code ship with human-lint credibility",
    causes: [
      "the lint policy references a rule id that no producer emits",
      "evidence attachments are missing, stale, or pinned to a different revision",
      "the revision under review was changed after evidence was collected",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the lint evidence for the current revision and re-run the comparison; align the policy's rule ids with what producers actually emit." },
    ],
    before: `lekalo ai-lint compare --policy lint.json`,
    after: `# Refresh evidence against the current revision, then compare again:\nlekalo ai-lint evidence --out evidence.json\nlekalo ai-lint compare --policy lint.json`,
    targets: "Target-neutral: policies and evidence live beside the Model, not in target source.",
    docs: "docs/ai-lint.md",
    waiver: "Waivers ride the closed waiver contracts (lekalo waivers); a waived finding keeps its recorded rationale in the audit trail.",
  },
  "ambiguity": {
    title: "Ambiguous language construct retained as data",
    kind: "flags a construct whose meaning cannot be proven from the admitted input and records every candidate interpretation as data instead of guessing one",
    matters: "hidden guessing is the failure mode this rule exists to prevent: humans and AI read the same retained candidates rather than an invisible choice",
    causes: [
      "source uses two constructs that map to the same semantic slot",
      "an attachment declares a rule the core grammar admits in more than one way",
      "an external producer emitted overlapping claims without a priority",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Disambiguate the source or attachment explicitly (rename, qualify, or declare the intended meaning) so exactly one interpretation remains." },
    ],
    before: `# two constructs claiming the same slot\nobserved: legacy-call\nobserved: explicit-call`,
    after: `# keep exactly one proven interpretation\nobserved: explicit-call`,
    targets: "Target-neutral: candidates are retained as diagnostic data; no target sees an unflagged guess.",
    docs: "docs/diagnostics.md",
    waiver: "Not applicable: this rule is informational by design and never blocks a result.",
  },
  "architecture-profile": {
    title: "Architecture profile violation",
    kind: "enforces the declared architecture profile: dependency directions, allowed relations and layer boundaries between modules",
    matters: "profiles are the machine-checked form of the agreed architecture; violations left unflagged let the module graph drift until refactors become unbounded",
    causes: [
      "a new dependency crosses a boundary the profile forbids",
      "a module was renamed or moved without updating the profile",
      "the profile was edited without re-deriving its lock from the graph",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Either move the offending dependency back inside the allowed boundary or update the profile intentionally, then re-lock and re-run the comparison." },
    ],
    before: `# planner depends on transport, forbidden by the profile\nlekalo architecture-profile diff --base base.json`,
    after: `# after moving the dependency (or consciously updating the profile):\nlekalo architecture-profile resolve\nlekalo architecture-profile diff --base base.json`,
    targets: "Target-neutral: profiles constrain the semantic module graph, not generated code.",
    docs: "docs/architecture-profile.md",
    waiver: "Profile exceptions belong in the reviewed profile document itself, not in per-run waivers; a diagnostic suppressed elsewhere still fails the profile gate.",
  },
  "authorization": {
    title: "Authorization model or evidence failure",
    kind: "validates the authorization model against its closed grammar and the authority evidence attached to it",
    matters: "authorization errors here mean the project's own access rules cannot be proven; silently passing them would put unproven access claims into generated targets",
    causes: [
      "the authorization document mixes grammar generations or spellings",
      "a subject or action references evidence that is missing or revoked",
      "two rules overlap without an explicit precedence declaration",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Correct the authorization document against the closed grammar and re-attach valid, current evidence before re-running the gate." },
    ],
    before: `lekalo authorization check --file authorization.yaml`,
    after: `# fix the rule the diagnostic names, keep evidence attached:\nlekalo authorization check --file authorization.yaml`,
    targets: "Target-neutral: the authorization model is validated before any target projection consumes it.",
    docs: "docs/authorization.md",
    waiver: "Not waivable: authorization invariants are security boundaries; fix the model instead.",
  },
  "bindings": {
    title: "Observed binding proposal invalid or unconfirmed",
    kind: "checks the binding pipeline that connects native source symbols to semantic IDs: proposal shape, confirmation state and custody",
    matters: "bindings are the trust anchor between generated and native code; an unconfirmed or stale binding silently redirects semantic guarantees to the wrong code",
    causes: [
      "a proposal was edited instead of being re-derived by a fresh scan",
      "the native file changed after the binding was confirmed",
      "two proposals claim the same semantic ID or native symbol",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the scan to regenerate proposals, then confirm the corrected binding explicitly; delete stale custody rather than editing it." },
    ],
    before: `lekalo bindings propose --target node-typescript`,
    after: `lekalo bindings propose --target node-typescript\nlekalo bindings confirm --binding <id>`,
    targets: "Binding homes and custody are target-specific and defined by each adapter's SDK; the semantic IDs they anchor are not.",
    docs: "docs/bindings.md",
    waiver: "Not waivable: binding custody is what makes native evidence referable; refresh instead.",
  },
  "cache": {
    title: "Incremental cache record invalid",
    kind: "guards the content-addressed incremental cache: record shape, dependency keys and reuse eligibility",
    matters: "a cache that reuses more than its keys prove silently serves stale semantic results as fresh",
    causes: [
      "the cache was produced by a different product or contract version",
      "a record's stored dependencies no longer match the current inputs",
      "the SQLite store was modified outside the owned pipeline",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Clear the cache with the owned command (lekalo cache clear --yes) and re-run; the cache is derived data, never evidence." },
    ],
    before: `lekalo load --project .   # uses possibly stale fragments`,
    after: `lekalo cache clear --yes\nlekalo load --project .   # recomputed`,
    targets: "Target-neutral: cache records never encode target-specific state.",
    docs: "docs/cache.md",
    waiver: "Not applicable: clearing is always safe; the cache owns no unique data.",
  },
  "call": {
    title: "Hidden call graph edge",
    kind: "records a call-like relationship that the declared graph cannot prove, keeping the edge visible as data instead of dropping it",
    matters: "consumers who trust the declared graph as complete would miss real runtime calls; the diagnostic makes the gap explicit",
    causes: [
      "dynamic dispatch or reflection creates an edge the static grammar cannot express",
      "a trace attachment claims a relation the Model does not declare",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the relationship explicitly (command effect, scenario coverage or trace relation) so the edge becomes proven graph data." },
    ],
    before: '# implicit string-based dispatch\ncalls: "handler_${name}"',
    after: `# explicit relation the graph can prove\ntriggers: handler_loader`,
    targets: "Target-neutral: the graph and its gaps are Model-level facts.",
    docs: "docs/effect-graph.md",
    waiver: "Not applicable: informational by design; the gap stays recorded even when accepted.",
  },
  "case": {
    title: "Inconsistent identifier casing",
    kind: "checks that semantic identifiers use one consistent casing convention within their scope",
    matters: "casing drift makes equal concepts look different to humans and to the semantic search index, splitting one entity into two apparent ones",
    causes: [
      "modules were authored by different contributors with different conventions",
      "a rename preserved old casing in member names or references",
    ],
    fixes: [
      { class: "unsafe", recipeId: null, description: "Rename to the canonical casing through a semantic change request; renames are semantic operations with impact analysis, not text edits." },
    ],
    before: `id: taskItem    # also task_item elsewhere`,
    after: `id: task_item   # one convention per project`,
    targets: "Generated target code inherits semantic casing; per-target style is the adapter's concern and stays out of the Model.",
    docs: "docs/model.md",
    waiver: "Not waivable per finding: normalize the convention project-wide.",
  },
  "change": {
    title: "Semantic change admission, custody or transaction failure",
    kind: "admits semantic change requests and owns their bounded plans, transactions, receipts and idempotent replay",
    matters: "every durable source mutation flows through this service; a custody or admission gap here would let unreviewed writes land or destroy rollback ability",
    causes: [
      "the change request references symbols absent from the current inputs",
      "the exact preview plan ID was not supplied at apply time, or inputs drifted",
      "a transaction journal from an interrupted run was not recovered",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-preview the change against current inputs and apply with the fresh exact plan ID; recover interrupted transactions with lekalo change recover --transaction <id> before new writes." },
    ],
    before: `lekalo change apply --request r.json --expect-plan <old>`,
    after: `lekalo change preview --request r.json\nlekalo change apply --request r.json --expect-plan <fresh-plan-id>`,
    targets: "Target-neutral: change owns Model/source custody; target regeneration is a separate explicit request.",
    docs: "docs/change.md",
    waiver: "Not waivable: admission and custody are the mutation safety envelope.",
  },
  "ci": {
    title: "CI report projection failure",
    kind: "checks the CI report projections (JSON, SARIF, JUnit, annotations) and their custody when a run writes them",
    matters: "CI consumers act on these reports; a mis-projected report can turn a failed gate green in the hosting system even though the run failed",
    causes: [
      "the report file could not be written (permissions, confined runner)",
      "the requested format is not available for this run kind",
      "report provenance pins do not match the executed inputs",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Fix the report path/permissions or drop the unsupported format; the underlying run verdict is separate and unchanged either way." },
    ],
    before: `lekalo validate --report-file out/missing/sarif.json`,
    after: `lekalo validate --report-file reports/validate.sarif --report-format sarif`,
    targets: "Target-neutral: projections consume the result envelope only.",
    docs: "docs/ci-reports.md",
    waiver: "Not waivable: report integrity failures must be visible in CI.",
  },
  "classification": {
    title: "Data classification or flow violation",
    kind: "enforces the declared data classifications and the classified data-flow policy over the semantic graph",
    matters: "classification rules keep sensitive data out of flows that were never reviewed; bypassing them silently breaks the project's privacy guarantees",
    causes: [
      "a field or effect lacks the classification its consumers assume",
      "a flow crosses a classification boundary without a declared, reviewed transition",
      "the policy was edited without re-deriving dependent evidence",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Classify the offending data explicitly or declare (and review) the flow transition; then re-run the flow report." },
    ],
    before: `lekalo dataflow --project .   # unclassified field on a path`,
    after: `# add the classification to the field or declare the transition,\n# then:\nlekalo dataflow --project .`,
    targets: "Target-neutral: classifications ride the Model; adapters project them per target policy.",
    docs: "docs/classification.md",
    waiver: "Classification exceptions need the classification-policy attachment, not ad-hoc waivers.",
  },
  "cli": {
    title: "CLI usage error",
    kind: "rejects malformed command-line syntax before any domain work starts",
    matters: "usage refusals stay typed: scripts and agents get the stable cli.usage diagnostic instead of parser noise on stdout",
    causes: [
      "a required argument is missing or two flags conflict",
      "an argument value is outside its bounded range or closed vocabulary",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Fix the invocation per lekalo <command> --help; usage errors exit 2 and are owned by the CLI parser, not by DomainResult statuses." },
    ],
    before: `lekalo fix --safe --recipe r.json   # conflicting flags`,
    after: `lekalo fix --recipe r.json --allow-unsafe`,
    targets: "Target-neutral.",
    docs: "docs/cli.md",
    waiver: "Not applicable: there is nothing to waive; correct the command line.",
  },
  "client": {
    title: "Client SDK contract failure",
    kind: "validates the published client SDK surface: envelope shape, capability advertisement and version compatibility",
    matters: "external consumers build on the SDK wire; drift here breaks integrations that were promised stability",
    causes: [
      "the SDK was generated against different contract versions than the binary serves",
      "an envelope field was consumed that the closed schema does not carry",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the SDK with the pinned generator and update consumers to the advertised capabilities; never widen the wire by hand." },
    ],
    before: `node scripts/generate-adapter-sdk.mjs --write   # then edit by hand`,
    after: `node scripts/generate-adapter-sdk.mjs --write\nnode scripts/test-adapter-sdk-contracts.mjs`,
    targets: "Per-SDK: each adapter SDK pins its own contract versions; the wire shapes stay shared.",
    docs: "docs/client-sdk.md",
    waiver: "Not waivable: SDK compatibility is a published contract.",
  },
  "concurrency": {
    title: "Transaction concurrency violation",
    kind: "checks declared transactions for concurrency hazards: overlapping writes, lost updates and isolation assumptions",
    matters: "concurrency bugs are order-dependent and rarely reproduce in review; declaring and checking them is the only scalable defense",
    causes: [
      "two commands write overlapping resources without an ordering declaration",
      "a transaction assumes an isolation level the storage profile does not provide",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Narrow the transaction's write set, declare the ordering, or align the isolation assumptions with the storage profile; then re-run the check." },
    ],
    before: `# tx A writes orders, tx B writes orders without ordering`,
    after: `# declare the ordering or split the write sets, then re-run`,
    targets: "Consult the storage engine profile for the target's actual isolation guarantees; the Model-level check is target-neutral.",
    docs: "docs/transaction-concurrency.md",
    waiver: "Not waivable: unresolved concurrency hazards block release gates.",
  },
  "context": {
    title: "Context capsule boundary violation",
    kind: "validates context capsules and their assembly: bounded projections of semantic facts for AI consumption",
    matters: "capsules are the AI-facing read model; overflow or unproven content here directly degrades every downstream agent answer",
    causes: [
      "a capsule requested more symbols or bytes than the policy allows",
      "assembly included evidence that is not admitted for the requesting consumer",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Narrow the capsule selection or raise the policy consciously; re-assemble after the inputs or evidence change." },
    ],
    before: `lekalo context --symbols <200 ids>`,
    after: `lekalo context --symbols <bounded selection>`,
    targets: "Target-neutral: capsules are derived only from admitted semantic facts.",
    docs: "docs/context.md",
    waiver: "Not waivable: bounds are the product, not an obstacle.",
  },
  "contract": {
    title: "Authority contract failure",
    kind: "enforces the authority matrix: which artifact kinds, homes and writers are admitted for each custody authority",
    matters: "the authority matrix is what stops tools from writing wherever they like; violations mean an unreviewed writer touched governed custody",
    causes: [
      "a tool wrote an artifact kind its authority never admitted",
      "a contract manifest digest no longer matches the artifact bytes",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Remove the unauthorized artifact, re-produce it through the owning tool, and update the contract manifest with reviewed digests." },
    ],
    before: `# hand-edited contracts/authority-matrix.*.json`,
    after: `node scripts/check-authority.mjs   # after regenerating through the owner`,
    targets: "Target-neutral.",
    docs: "docs/authority.md",
    waiver: "Not waivable: authority boundaries are the integrity model.",
  },
  "contracted": {
    title: "Contracted-mode declaration invalid",
    kind: "validates the contracted-mode declarations that pin which Model facts external tools may rely on",
    matters: "contracted mode is a published promise to consumers; an invalid declaration would advertise guarantees the Model does not actually carry",
    causes: [
      "a declaration references a symbol or field that no longer exists",
      "the declaration was edited without re-admitting it through its producer",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate or correct the declaration against the current Model and re-admit it before consumers rely on it." },
    ],
    before: `lekalo contract --attach decl.json   # stale symbol`,
    after: `# refresh the declaration, then:\nlekalo contract --attach decl.json`,
    targets: "Target-neutral: declarations ride semantic IDs.",
    docs: "docs/contracted-mode.md",
    waiver: "Not waivable: published promises must match reality.",
  },
  "core": {
    title: "Capability unavailable in this build",
    kind: "reports a capability that is not implemented in the current binary rather than pretending success",
    matters: "typed unavailability keeps scripts and agents from misreading a no-op as a passing run",
    causes: [
      "the invoked surface is planned but not implemented in this release",
      "a required feature was compiled out or is platform-gated",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Use the documented available alternative for this release, or wait for the capability's own issue to land; the refusal is data, never a crash." },
    ],
    before: `lekalo <unimplemented-surface>`,
    after: `lekalo --help   # pick the implemented surface`,
    targets: "Target-neutral.",
    docs: "docs/roadmap.md",
    waiver: "Not applicable: availability is a release fact, not a policy.",
  },
  "coupling": {
    title: "Coupling or change-radius measurement issue",
    kind: "measures coupling and change-radius metrics with graph evidence and flags measurement conditions that would make the numbers misleading",
    matters: "architecture decisions get made from these numbers; unflagged incomplete evidence would quietly steer refactoring the wrong way",
    causes: [
      "the analyzed scope excludes modules the metrics depend on",
      "cross-module hops are truncated by policy limits",
      "graph evidence is incomplete for the measured slice",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Widen the scope consciously or accept the documented incompleteness; interpret flagged metrics as bounded, not absolute." },
    ],
    before: `lekalo coupling --module planner   # hops truncated`,
    after: `lekalo coupling --project .   # full evidence, or read the flags`,
    targets: "Target-neutral: metrics come from the semantic graph.",
    docs: "docs/coupling.md",
    waiver: "Not applicable: informational flags document measurement conditions.",
  },
  "dataflow": {
    title: "Classified data-flow violation",
    kind: "reports a data flow that crosses a declared classification boundary without an admitted transition",
    matters: "these flows are exactly the paths sensitive data leaks through; each refusal is a reviewed privacy boundary holding",
    causes: [
      "a field's classification changed while its flows stayed unchanged",
      "an effect or query reads classified data the flow report never admitted",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Reclassify the field consciously or insert a declared, reviewed transition; re-run the data-flow report afterwards." },
    ],
    before: `lekalo dataflow --project .   # public query reads confidential`,
    after: `# reclassify or declare the transition, then:\nlekalo dataflow --project .`,
    targets: "Target-neutral: flows are Model-level; target storage policies consume them.",
    docs: "docs/classification.md",
    waiver: "Policy transitions live in the classification policy, not per-run waivers.",
  },
  "diagnostics": {
    title: "Diagnostic infrastructure failure",
    kind: "guards the diagnostic system itself: registry integrity and envelope invariants",
    matters: "when this fires, the reporting layer cannot be trusted; every other diagnostic on the envelope is suspect until the registry is fixed",
    causes: [
      "an embedded registry violates its structural invariants (sort, uniqueness, identity)",
      "a producer tried to emit an unregistered or inactive rule",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Rebuild from a clean checkout; if it persists, the registry contract and its consumers drifted and need a reviewed union fix, never a local override." },
    ],
    before: `# registry edited out of id order`,
    after: `# restore the sorted, closed registry; clean rebuild`,
    targets: "Target-neutral.",
    docs: "docs/diagnostics.md",
    waiver: "Not waivable: registry integrity underpins every result envelope.",
  },
  "diff": {
    title: "Semantic diff failure",
    kind: "compares two typed projections and classifies every change as breaking, non-breaking or policy-only",
    matters: "compatibility verdicts gate releases; a diff that silently under-reports changes would classify breaking API drift as safe",
    causes: [
      "the compared inputs are not same-family attachments or profiles",
      "identity history (renames, moves) was not carried into the comparison",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Compare the correct base/candidate pair and carry rename history; treat text-only diffs as evidence, never as compatibility authority." },
    ],
    before: `lekalo diff a.json b.json   # different families`,
    after: `lekalo diff base.json candidate.json   # same family`,
    targets: "Target-neutral: comparisons run on typed projections.",
    docs: "docs/semantic-diff.md",
    waiver: "Not waivable: classification is the compatibility gate.",
  },
  "error": {
    title: "Error-contract violation",
    kind: "validates operation error contracts: declared failure modes, payload shapes and mapping coverage",
    matters: "error contracts are what let consumers handle failures generically; gaps here push raw internal errors across the boundary",
    causes: [
      "an operation declares no error union where failures are possible",
      "a declared error payload shape does not match its producer",
      "mapping coverage misses a declared error class",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the missing failure modes or fix the payload/mapping so every producer has an admitted error shape." },
    ],
    before: `# command with effects but no error union`,
    after: `# attach the error contract to the operation`,
    targets: "Target adapters project error contracts per transport; the Model contract is the authority.",
    docs: "docs/error-contracts.md",
    waiver: "Not waivable: undeclared failures are the bug class this rule prevents.",
  },
  "evaluation": {
    title: "Framework-lift evaluation failure",
    kind: "guards framework-lift evaluations: baseline/task/campaign/arm invariants and the evidence each result must carry",
    matters: "lift numbers drive adoption decisions; inconsistent arms or missing evidence would present noise as measured improvement",
    causes: [
      "arms were measured on incomparable scopes or profiles",
      "baseline drift: the baseline moved under a running campaign",
      "a metric aggregates inconsistent units across arms",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the affected arms on comparable scopes with fresh evidence; never merge results across drifted baselines." },
    ],
    before: `lekalo metrics run --campaign c1   # baseline drifted`,
    after: `# re-baseline, then re-run the campaign arms`,
    targets: "Target-neutral: evaluation evidence rides admitted run records.",
    docs: "docs/framework-lift.md",
    waiver: "Not waivable: incomparable measurements must not ship as evidence.",
  },
  "event": {
    title: "Event declaration contract violation",
    kind: "checks event declarations against the Model grammar and their declared consumers",
    matters: "events are published facts; a malformed declaration would let producers and consumers disagree about what was published",
    causes: [
      "an event references payload fields that do not exist",
      "a consumer subscribes to an event with an incompatible declared shape",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Align the event declaration and its consumers on one declared payload shape." },
    ],
    before: `# event payload references removed field`,
    after: `# update the event declaration (or its consumers) to the current fields`,
    targets: "Target transports project events per target policy; declarations are Model-level.",
    docs: "docs/model.md",
    waiver: "Not waivable: published shapes are contracts.",
  },
  "expression": {
    title: "Expression failure",
    kind: "parses, types and evaluates the bounded expression language and enforces its closed builtin vocabulary and export limits",
    matters: "expressions compute derived facts; silent mis-evaluation would put wrong data into projections, exports and gates",
    causes: [
      "an expression uses a builtin outside the supported vocabulary",
      "type mismatch between operands or against the bound field",
      "complexity or export-size limits exceeded",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Rewrite the expression within the closed builtin set and type rules, or move the logic behind a declared operation." },
    ],
    before: `when: user.isAdmin() or superuser`,
    after: `when: user.role == "admin"`,
    targets: "Per-target builtin support is published by the support contract; the core grammar is the shared subset.",
    docs: "docs/expressions.md",
    waiver: "Not waivable: an expression that cannot be proven cannot be evaluated.",
  },
  "explain": {
    title: "Diagnostic explanation service failure",
    kind: "guards the explanation service itself: the embedded catalog's integrity, its registry pin, and the readability of caller-supplied reports",
    matters: "explanations are the stable contract humans and AI read instead of per-run text; a broken catalog or an unreadable report must refuse visibly rather than serve an unproven explanation",
    causes: [
      "the requested code or rule id is not in this catalog (distinct unexplained outcome)",
      "the embedded catalog drifted from the registry union it pins",
      "the --diagnostic-file input is malformed, over-limit, or carries no diagnostics",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "For unknown codes, check the spelling against the catalog version in the result; for report refusals, supply a report with a readable diagnostics array. Catalog-integrity failures are developer faults: rebuild from a clean checkout." },
    ],
    before: `lekalo explain LEK-TYPE-014   # illustrative, unregistered code`,
    after: `lekalo explain LEK-HIDDEN-009   # registered code, explained`,
    targets: "Target-neutral: explanations resolve offline from the embedded catalog.",
    docs: "docs/explain.md",
    waiver: "Not applicable: service-integrity refusals are typed outcomes, not policy.",
  },
  "extended": {
    title: "Extended-effect declaration failure",
    kind: "admits extended-effect declarations that go beyond the core effect grammar and enforces their evidence obligations",
    matters: "extended effects extend what the graph claims about the system; unproven claims would propagate into impact, coverage and review surfaces",
    causes: [
      "a declaration exceeds its permitted shape or evidence binding",
      "the underlying effect was removed while the extension remained",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-admit the declaration with current evidence, or remove the extension that lost its underlying effect." },
    ],
    before: `# extension pinned to a removed effect`,
    after: `# re-attach the extension to a live effect or delete it`,
    targets: "Target-neutral: extensions ride the effect graph.",
    docs: "docs/extended-effects.md",
    waiver: "Not waivable: unproven effect claims are not admissible.",
  },
  "fix": {
    title: "Versioned fix recipe failure",
    kind: "governs the versioned recipe service: closed risk classes, scope admission, fingerprints, atomic custody and explicit confirmation",
    matters: "this family is the write path's safety interlock; bypassing it would let unreviewed or mis-scoped patches touch source",
    causes: [
      "a recipe claims a risk class its semantic delta does not admit",
      "the source fingerprint changed between preview and apply",
      "application proceeded without the exact reviewed plan ID",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-preview against current inputs and re-apply with the fresh exact plan ID; for class denials, select a recipe whose class matches the intent (safe needs no flags; unsafe/breaking need their explicit acknowledgements)." },
    ],
    before: `lekalo fix --safe --apply --project .   # no consent`,
    after: `lekalo fix --safe --project .            # review the preview\nlekalo fix --safe --apply --expect-plan <id> --project .`,
    targets: "Adapter recipes must be namespaced (adapter.<target>.*); core admits only the pinned canonical-format recipe.",
    docs: "docs/fixes.md",
    waiver: "Not waivable: the confirmation gates are the consent model.",
  },
  "format": {
    title: "Canonical formatting or formatting-service failure",
    kind: "enforces canonical formatting: pinned render, comment preservation, and Model+IR equality for every admitted edit",
    matters: "formatting diffs are the noisiest review artifacts; the canonical form plus the equality proof keep format-only changes provably semantic-free",
    causes: [
      "a document was edited outside the canonical formatter",
      "a formatter run would lose comments or change the compiled IR",
      "the formatter policy was changed without reformatting",
    ],
    fixes: [
      { class: "safe", recipeId: "core.canonical-format", description: "Apply the admitted safe canonical-format recipe: preview with lekalo fix LEK-FORMAT-001 --dry-run first, then --apply --expect-plan <id>. The recipe refuses comment loss and proves normalized Model and IR equality." },
    ],
    before: `# hand-formatted YAML, drifted from canonical\nschema_version: "0.2.16"\ndefinitions:\n  -    id: tasks.task`,
    after: `schema_version: "0.2.16"\ndefinitions:\n  - id: tasks.task`,
    targets: "Target-neutral: formatting owns Model documents only.",
    docs: "docs/format.md",
    waiver: "Not waivable: canonical formatting is the identity of a document.",
  },
  "graph": {
    title: "Dependency graph failure",
    kind: "builds and validates the dependency graph: node identity, relation kinds and evidence completeness for impact analysis",
    matters: "impact, coverage and change-radius all consume this graph; a wrong edge silently moves every downstream conclusion",
    causes: [
      "a declared relation references a symbol that does not exist",
      "a relation kind is used outside its registered role",
      "graph evidence for a subgraph is missing and cannot be proven complete",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Fix or remove the unprovable relation; accept documented incompleteness only as an explicit flag, never as proven coverage." },
    ],
    before: `# relation -> removed symbol`,
    after: `# re-point the relation to the current symbol`,
    targets: "Target-neutral: the graph is built from the Model and admitted evidence.",
    docs: "docs/effect-graph.md",
    waiver: "Not waivable: unproven edges poison impact analysis.",
  },
  "hidden": {
    title: "Hidden behavior surfaced as data",
    kind: "detects behavior the surface text hides (reflection, string references, undeclared effects) and records it as data with a confidence claim",
    matters: "these constructs are where AI-ported or reviewed code most often misses real behavior; surfacing them keeps the human in command of the invisible",
    causes: [
      "dynamic string-built references or reflective calls",
      "side effects performed outside declared effects",
      "framework conventions that imply behavior without declaring it",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the hidden behavior explicitly (rename to a static reference, declare the effect, bind the call) or accept the finding as recorded uncertainty." },
    ],
    before: `model.getService("user_" + kind).save()`,
    after: `userService.save()   # static reference the graph can prove`,
    targets: "Target-neutral: findings ride the semantic graph; adapters may add their own hidden-behavior evidence.",
    docs: "docs/diagnostics.md",
    waiver: "Waivers ride the closed waiver contracts; the finding and its confidence stay recorded.",
  },
  "history": {
    title: "Run history or audit integrity failure",
    kind: "guards run-history and audit records: append-only integrity, digest chains and bounded retrieval",
    matters: "history is the evidence base for replays, audits and rollbacks; a broken chain invalidates every claim that cites it",
    causes: [
      "history files were edited or truncated outside the owning writer",
      "a record's digest does not match its cited inputs",
      "concurrent writers bypassed the shared lock",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Restore the history from a trusted backup or start a fresh history; never splice, re-order or re-digest records by hand." },
    ],
    before: `# lekalo/change-history.json edited by hand`,
    after: `# restore from backup or archive and let writers append anew`,
    targets: "Target-neutral: history homes are documented canonical paths.",
    docs: "docs/run-history.md",
    waiver: "Not waivable: history integrity is the audit foundation.",
  },
  "impact": {
    title: "Impact analysis refusal or incompleteness",
    kind: "computes affected scope for changed inputs and refuses or flags when the analysis cannot be proven complete",
    matters: "agents and reviewers act on impact reports; unknown scope presented as complete causes missed breakage in exactly the risky changes",
    causes: [
      "changed inputs reference symbols absent from the compiled projection",
      "evidence (tests, artifacts) for affected scope is unknown",
      "revision ranges or working-tree state could not be captured",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Fix or narrow the changed-input selection; treat unknown evidence as unknown and follow the report's flagged follow-ups." },
    ],
    before: `lekalo impact --changed task.save --revision dead..live`,
    after: `# correct the symbol id / revision range, then re-run`,
    targets: "Target-neutral: impact runs on the semantic graph plus admitted evidence.",
    docs: "docs/impact.md",
    waiver: "Not applicable: incompleteness is reported as data, not policy.",
  },
  "implementation": {
    title: "Implementation-mode declaration failure",
    kind: "validates implementation-mode declarations that bind generated implementation artifacts to semantic facts",
    matters: "implementation declarations are how generated code stays attributable; drift here orphans generated artifacts from their Model truth",
    causes: [
      "a declaration cites artifacts that were regenerated or moved",
      "the semantic fact a declaration pinned was renamed or removed",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the declaration with the owning generator and re-check; never edit pinned artifact digests by hand." },
    ],
    before: `# declaration pinned to old artifact digest`,
    after: `# regenerate, then re-run the implementation gate`,
    targets: "Per-adapter: artifact homes are target-specific; the declaration grammar is shared.",
    docs: "docs/implementation.md",
    waiver: "Not waivable: attribution is the point of the mode.",
  },
  "indirection": {
    title: "Unproven indirection retained",
    kind: "records indirection the admitted inputs cannot resolve to a concrete target, keeping the gap visible",
    matters: "indirection through config or strings is invisible to static guarantee systems unless surfaced; this rule makes the ambiguity explicit",
    causes: [
      "behavior is selected through configuration values unknown at analysis time",
      "a registry/lookup pattern maps keys to handlers without declared edges",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Replace runtime string selection with declared references, or record the key set explicitly so the indirection becomes analyzable." },
    ],
    before: `handlers[config.handlerName].run()`,
    after: `# declare the handler set and select with a closed vocabulary`,
    targets: "Target-neutral.",
    docs: "docs/diagnostics.md",
    waiver: "Not applicable: informational; the uncertainty stays recorded.",
  },
  "init": {
    title: "Bootstrap failure",
    kind: "guards project bootstrap: creation, adoption and the first admitted structure of a project",
    matters: "a project created or adopted with an unproven structure carries that debt into every later gate; bootstrap refusals keep the foundation honest",
    causes: [
      "the target directory already contains an incompatible project",
      "adoption inputs reference files outside the admitted boundary",
      "the generated skeleton conflicts with the chosen mode",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Start from an empty directory, or reconcile the existing structure with the documented adoption flow; never force-create over an owned project." },
    ],
    before: `lekalo init --adopt .   # non-empty, unclassified files`,
    after: `# classify or move the strays, then re-run adoption`,
    targets: "Target-neutral: bootstrap writes only the canonical project homes.",
    docs: "docs/bootstrap.md",
    waiver: "Not waivable: the first structure is the root of custody.",
  },
  "inspect": {
    title: "Inspect selection failure",
    kind: "resolves a semantic symbol for inspection and reports selections that cannot be resolved unambiguously",
    matters: "inspect output feeds agents and humans; a wrong-symbol answer is worse than a typed refusal, so ambiguity refuses",
    causes: [
      "the short name matches multiple symbols in the project",
      "the symbol was removed or renamed (check rename history)",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Use the full semantic id, or resolve the ambiguity with search first; consult rename history for moved symbols." },
    ],
    before: `lekalo inspect save   # ambiguous`,
    after: `lekalo inspect planner.save_task`,
    targets: "Target-neutral: inspection reads the compiled projection.",
    docs: "docs/inspect.md",
    waiver: "Not applicable: selection refusals are precise, not policy.",
  },
  "invariant": {
    title: "Invariant transition violation",
    kind: "checks declared state invariants and their transitions for reachability and preservation",
    matters: "broken invariants are the deep bugs: data states no review anticipated; checking transitions is the only systematic coverage",
    causes: [
      "a transition can enter a state the invariant forbids",
      "an invariant references fields a transition legitimately mutates",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Constrain the transition or refine the invariant so every reachable state satisfies it; re-run the transition check afterwards." },
    ],
    before: `# transition sets done=true with empty title`,
    after: `# guard the transition on title non-empty`,
    targets: "Target-neutral: invariants live in the Model.",
    docs: "docs/invariant-transition.md",
    waiver: "Not waivable: reachable violations block release gates.",
  },
  "ir": {
    title: "IR compilation failure",
    kind: "compiles the Model into the typed IR and refuses documents that cannot produce a proven intermediate representation",
    matters: "the IR is the read model every projection builds on; compiling a questionable Model would propagate doubt into all downstream outputs",
    causes: [
      "a document uses a construct outside the admitted grammar generation",
      "references between definitions do not resolve",
      "version pins between Model and IR contracts are incompatible",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Correct the Model against the current grammar (see the loader/semantic rules the envelope also carries) and recompile." },
    ],
    before: `lekalo ir --project .   # undefined reference`,
    after: `# define or re-point the reference, then recompile`,
    targets: "Adapters consume the versioned IR; its version is pinned per adapter contract.",
    docs: "docs/ir.md",
    waiver: "Not waivable: without IR there is nothing downstream to trust.",
  },
  "job": {
    title: "Background job declaration violation",
    kind: "checks background-job declarations for admitted triggers, schedules and effect bindings",
    matters: "jobs run without a caller present; undeclared or mis-declared jobs become unaccountable background behavior",
    causes: [
      "a job trigger references a schedule or event that is not declared",
      "a job writes resources its declared effects do not cover",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the trigger and align the job's effects with what it actually writes." },
    ],
    before: `# job fires on undeclared cron`,
    after: `# declare the schedule in the job definition`,
    targets: "Per-target scheduling semantics are adapter concerns; the declaration is Model-level.",
    docs: "docs/model.md",
    waiver: "Not waivable: background behavior must be declared to be reviewable.",
  },
  "loader": {
    title: "Model loading or grammar refusal",
    kind: "admits project files into the typed Model: canonical homes, encoding, bounded sizes and the closed YAML/JSON grammar",
    matters: "loading is the trust boundary between arbitrary bytes and the semantic system; every later guarantee assumes what the loader admitted",
    causes: [
      "a file sits outside its canonical home or is unreadable/over-limit",
      "syntax outside the closed YAML/JSON subset (anchors, flow style, duplicate keys)",
      "schema fields are unknown, misspelled, or use a foreign generation",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Move the file to its canonical home and correct the syntax/fields the envelope names; run lekalo format afterwards to keep documents canonical." },
    ],
    before: `# definitions: [value: *anchor]`,
    after: `# rewrite without anchors/aliases in the closed subset`,
    targets: "Target-neutral: the loader owns Model documents, never target source.",
    docs: "docs/loader.md",
    waiver: "Not waivable: grammar refusals define the language.",
  },
  "lock": {
    title: "Lock integrity or currency failure",
    kind: "guards the committed lock: reproducible projection identity, currency against sources, and digest binding",
    matters: "the lock is how consumers prove they see the same Model; a stale or edited lock silently forks the project truth",
    causes: [
      "Model sources changed after the last lock/update",
      "the lock was edited or generated by a different product version",
      "checksum sidecars drifted from their artifacts",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the owned update/refresh flow to regenerate the lock from current sources; restore sidecar checksums through the owning tool, never by hand." },
    ],
    before: `lekalo lock --check   # stale`,
    after: `lekalo update --dry-run\nlekalo update          # then commit the refreshed lock`,
    targets: "Target-neutral: the lock pins the Model projection, not target artifacts.",
    docs: "docs/lockfile.md",
    waiver: "Not waivable: currency is provable and cheap; refresh instead.",
  },
  "managed": {
    title: "Managed-mode ownership, custody or generation failure",
    kind: "enforces managed mode: declared ownership, sealed custody evidence, deterministic generation plans and transactional application",
    matters: "managed mode promises fully governed artifacts; any gap would let generated drift or unowned writes into a mode that claims they cannot happen",
    causes: [
      "an artifact changed outside the owned generation flow",
      "a custody seal (authority pin) is missing, stale or unverified",
      "a generation plan references symbols the current Model lacks",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the managed generation/verify flow to regenerate plans and custody from the current Model; recover interrupted transactions before new writes." },
    ],
    before: `# artifact hand-edited under managed ownership`,
    after: `lekalo managed verify --project .   # then regenerate via the plan`,
    targets: "Target-neutral: managed custody rides Model digests and canonical homes.",
    docs: "docs/managed-mode.md",
    waiver: "Not waivable: managed mode has no bypass by definition.",
  },
  "metrics-export": {
    title: "Metrics export failure",
    kind: "validates metrics aggregation definitions and the public export previews built from them",
    matters: "exported metrics are public claims about the system; inconsistent definitions would publish numbers no one can reproduce",
    causes: [
      "an aggregation references metrics that do not exist or disagree in units",
      "the export manifest drifted from the produced previews",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Correct the aggregation definitions and regenerate the previews and manifest through the owned flow." },
    ],
    before: `# aggregation averages incompatible counters`,
    after: `# fix the definition, then re-run the export preview`,
    targets: "Target-neutral: metrics ride admitted evaluation evidence.",
    docs: "docs/metrics-export.md",
    waiver: "Not waivable: public numbers must be reproducible.",
  },
  "module-api": {
    title: "Module public API or boundary failure",
    kind: "guards the versioned module public contract: exported surface, dependency boundaries, locks and compatibility diffs",
    matters: "the public API is what sibling modules and external consumers build against; unflagged drift breaks them at their compile or runtime, not at review time",
    causes: [
      "a change removed or reshaped an exported member without a versioned lock update",
      "a new dependency crosses the module boundary the contract declares",
      "the lock was not refreshed after a legitimate API change",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Restore the exported surface, or make the API change intentionally: refresh the module lock and re-run the diff to classify the delta." },
    ],
    before: `lekalo module diff --lock module-api.lock.json`,
    after: `# decide: restore the surface or accept + re-lock, then re-diff`,
    targets: "Per-adapter: generated public surfaces are projected per target; the contract is Model-level.",
    docs: "docs/module-api.md",
    waiver: "Not waivable: boundary drift must be a reviewed decision, never noise.",
  },
  "native-gate": {
    title: "Native gate admission or refusal",
    kind: "admits native gate plans and runs: bounded declarations, custody preconditions and typed refusals instead of guesses",
    matters: "native gates execute real target code; admitting an unproven plan would run unreviewed code with project trust",
    causes: [
      "a plan references custody or fixtures that are missing or stale",
      "the requested execution exceeds the admitted plan scope",
      "the platform cannot provide the required confinement",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the plan from current custody; run only within the admitted scope, and record the typed refusal honestly when confinement is unavailable." },
    ],
    before: `lekalo native run --plan stale.json`,
    after: `# regenerate custody + plan, then run`,
    targets: "Confinement backends and availability are platform-specific (e.g. bubblewrap on Linux); refusals are data on every OS.",
    docs: "docs/native-gates.md",
    waiver: "Not waivable: native execution outside admitted custody is the harm this family prevents.",
  },
  "nfr": {
    title: "NFR gate failure",
    kind: "checks non-functional requirement gates: bounded evidence, thresholds and the claims each gate is allowed to make",
    matters: "NFR gates turn performance and security budgets into review-blocking facts; unflagged missing evidence would let probably-fine ship",
    causes: [
      "required evidence is missing, expired or pinned to another revision",
      "a threshold is violated or a measurement is outside its admitted envelope",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-measure with fresh evidence at the pinned revision, or consciously renegotiate the threshold through the NFR document." },
    ],
    before: `lekalo nfr --check   # stale evidence`,
    after: `# re-run the measured scenario, attach fresh evidence, re-check`,
    targets: "Thresholds may be target-specific; evidence identity and staleness rules are shared.",
    docs: "docs/nfr.md",
    waiver: "Waivers ride the closed waiver contracts with recorded rationale.",
  },
  "observed": {
    title: "Observed-mode index or scan failure",
    kind: "guards observed mode: native evidence scans, index records, freshness and candidate completeness",
    matters: "observed mode is how native reality enters the semantic world; stale or partial evidence here quietly misrepresents the target",
    causes: [
      "the index was built from a scan older than the current native files",
      "a scan claims coverage the backend could not actually observe",
      "records were edited outside the scan service",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the scan to refresh the index; treat unobservable domains as unknown, never as covered." },
    ],
    before: `lekalo scan --project .   # native files changed since`,
    after: `lekalo scan --project .   # fresh, then re-observe`,
    targets: "Scanner backends are per-target; freshness rules and index grammar are shared.",
    docs: "docs/observed-mode.md",
    waiver: "Not applicable: freshness is recorded as data.",
  },
  "openapi": {
    title: "OpenAPI projection failure",
    kind: "projects the Model into the published OpenAPI surface and validates it against the OpenAPI schema",
    matters: "the OpenAPI document is a public contract for external HTTP consumers; drift here breaks clients that were promised the schema",
    causes: [
      "a declared operation cannot be projected (unsupported shape)",
      "the projection drifted from the validated OpenAPI version",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Align the Model declarations with what the projection admits, and regenerate the document through the owned flow." },
    ],
    before: `# hand-edited openapi.json`,
    after: `# regenerate from the Model, then validate`,
    targets: "Transport-specific details ride the transport contract; the projection grammar is shared.",
    docs: "docs/openapi.md",
    waiver: "Not waivable: public API documents must match the Model.",
  },
  "php-operations": {
    title: "PHP operations map failure",
    kind: "admits the PHP operations-generation input and validates the resulting operations map",
    matters: "operations maps drive PHP code generation; a wrong map generates code that silently diverges from the Model",
    causes: [
      "the operations input references operations the Model does not declare",
      "the map was edited outside the generator",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the map from the admitted input through the generator; fix the input when the Model and it disagree." },
    ],
    before: `# hand-edited operations map`,
    after: `# regenerate via the generator, then re-run the gate`,
    targets: "PHP-specific: the map feeds the PHP/Laravel adapter's operation surface.",
    docs: "docs/php-laravel-operations.md",
    waiver: "Not waivable: generated code must trace to admitted input.",
  },
  "php-routes": {
    title: "PHP routes map failure",
    kind: "admits the PHP routes-generation input and validates the resulting routes map",
    matters: "routes maps decide what HTTP surface the PHP target exposes; drift here exposes endpoints the Model never declared",
    causes: [
      "the routes input declares routes with no Model operation",
      "the map drifted from the generator output",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the routes map via the generator; remove routes that no Model operation backs." },
    ],
    before: `# hand-added route`,
    after: `# declare the operation in the Model or drop the route`,
    targets: "PHP-specific: feeds the PHP/Laravel adapter transport surface.",
    docs: "docs/php-laravel-operations.md",
    waiver: "Not waivable: undeclared HTTP surface is a security boundary issue.",
  },
  "portability": {
    title: "Portability evidence or receipt failure",
    kind: "admits portability recipes, evidence and receipts that prove a Model ported to a target faithfully",
    matters: "portability is the product's core promise; unproven port claims would ship adapters that diverge from the semantic truth",
    causes: [
      "evidence is pinned to an older Model or adapter revision",
      "a receipt references execution that cannot be reproduced",
      "the recipe's declared family does not match its evidence",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-run the port against the current revision to regenerate evidence and receipts; never re-badge old evidence with new digests." },
    ],
    before: `lekalo portability --receipt stale.json`,
    after: `# re-run the port flow, then re-admit fresh evidence`,
    targets: "Per-target: evidence binds target identity and adapter SDK versions.",
    docs: "docs/portability.md",
    waiver: "Not waivable: port claims without evidence are the failure mode.",
  },
  "publication": {
    title: "Publication boundary violation",
    kind: "guards what may be published: exported artifacts, their classification and their custody at the boundary",
    matters: "publication is irreversible in practice; boundary checks are the last chance to keep unreviewed or classified material internal",
    causes: [
      "an artifact lacks the classification its destination requires",
      "a published digest does not match the reviewed bytes",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Classify the artifact or publish through the owning flow so the boundary check sees reviewed bytes and declarations." },
    ],
    before: `# unclassified artifact exported`,
    after: `# classify, then export via the owning flow`,
    targets: "Target-neutral: publication rules ride the Model and authority matrix.",
    docs: "docs/security.md",
    waiver: "Not waivable: boundary decisions are explicit, not per-run.",
  },
  "query": {
    title: "Query model violation",
    kind: "validates the query model: declared reads, returns and their consistency with the entity model",
    matters: "queries are the read-side contract; inconsistent declarations would return data the write side never admitted",
    causes: [
      "a query returns fields no entity or aggregate declares",
      "declared reads do not cover the fields the returns project",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Align the query's reads and returns with the entity model, or extend the entity consciously." },
    ],
    before: `# query returns user.email; entity lacks email`,
    after: `# add the field to the entity or drop it from returns`,
    targets: "Per-target: query translation is adapter work; the model-level contract is the authority.",
    docs: "docs/query-model.md",
    waiver: "Not waivable: read/write divergence is a correctness bug.",
  },
  "requirements": {
    title: "Requirements attachment or coverage failure",
    kind: "admits requirements attachments and reports their coverage over the semantic surface",
    matters: "requirements traceability is how humans verify the system still does what it claims; broken attachments or silent gaps break that verification",
    causes: [
      "an attachment references symbols that no longer exist",
      "coverage cannot resolve a requirement to any declared behavior",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Update the attachment's references to current symbols; treat unresolved coverage as an explicit gap, never as satisfied." },
    ],
    before: `lekalo requirements --attach req.json   # stale ids`,
    after: `# re-point the ids, then re-run the report`,
    targets: "Target-neutral: requirements ride semantic IDs.",
    docs: "docs/requirements.md",
    waiver: "Not applicable: gaps are reported as data for triage.",
  },
  "scenario": {
    title: "Scenario declaration or execution failure",
    kind: "validates scenario declarations and admits their bounded execution as native gates",
    matters: "scenarios are executable product truth; mis-declared scenarios either fail honestly or, worse, pass while testing the wrong thing",
    causes: [
      "a scenario covers symbols that were renamed or removed",
      "the scenario IR references operations with mismatched shapes",
      "execution was requested outside admitted custody",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-point the scenario to current symbols and re-derive its IR; execute only within the admitted plan and custody." },
    ],
    before: `# scenario covers removed handler`,
    after: `# update the scenario to the current symbols`,
    targets: "Per-target execution is the native-gate family's concern; declarations are Model-level.",
    docs: "docs/native-gates.md",
    waiver: "Not waivable: scenarios must test declared behavior.",
  },
  "search": {
    title: "Semantic search admission or index failure",
    kind: "admits search queries, owns the disposable local index and its coverage claims, and returns bounded selection cards",
    matters: "search is how agents discover the project; unproven or stale results would anchor every later decision on wrong facts",
    causes: [
      "the index is stale relative to current sources",
      "the query is malformed or outside the admitted grammar",
      "coverage for a domain is missing and cannot be claimed",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Rebuild the index when stale; correct the query grammar; treat missing coverage as explicit unknown and consult the flagged domains." },
    ],
    before: `lekalo search "task"   # index stale`,
    after: `lekalo search-index rebuild\nlekalo search "task"`,
    targets: "Target-neutral: search reads semantic data and admitted evidence only; no repository text scan.",
    docs: "docs/semantic-search.md",
    waiver: "Not applicable: retrieval gaps are recorded, not waivable.",
  },
  "semantic": {
    title: "Semantic validation failure",
    kind: "runs the default semantic validator over the compiled Model: references, declarations and cross-definition consistency",
    matters: "these are the correctness rules that make the Model mean what it says; a blocking finding here means downstream projections would be wrong",
    causes: [
      "references to undefined or removed definitions or fields",
      "duplicate identities or conflicting declarations",
      "profile-gated rules violated under the selected validation profile",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Fix the Model where the finding points; semantic edits should go through typed change requests so impact and history are captured." },
    ],
    before: `lekalo validate   # field references undefined scalar`,
    after: `# define the scalar or fix the reference, then re-validate`,
    targets: "Target-neutral: validation runs before any target projection.",
    docs: "docs/validation.md",
    waiver: "Blocking semantic findings are not waivable; strict-profile deltas are profile decisions.",
  },
  "storage": {
    title: "Storage migration or projection failure",
    kind: "plans and validates storage migrations and projections from the Model to target storage",
    matters: "storage mistakes lose data; migration planning refuses rather than guessing when evidence or mappings are incomplete",
    causes: [
      "a migration step references entities the current Model lacks",
      "projection mappings disagree with declared fields or types",
      "transaction or rename history is missing for an incremental step",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-plan the migration from the current Model with complete rename history; apply only through the transactional migration writer with reviewed evidence." },
    ],
    before: `lekalo storage laravel-plan --project .   # missing rename history`,
    after: `# restore history or re-plan as a fresh baseline, then apply`,
    targets: "Per-storage-engine: engine profiles define real capabilities; Model-level planning stays engine-neutral.",
    docs: "docs/storage-projection.md",
    waiver: "Not waivable: data-loss risks need migration evidence, never flags.",
  },
  "storage-engine": {
    title: "Storage engine capability mismatch",
    kind: "checks declared storage behavior against the engine profile's actual capabilities",
    matters: "assuming capabilities the engine lacks fails at runtime under load, exactly where debugging is most expensive",
    causes: [
      "the Model declares constraints the target engine cannot enforce",
      "the engine profile is stale or targets the wrong engine or version",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Align the declarations with the engine profile, or select an engine that provides the required capabilities." },
    ],
    before: `# partial index on engine without partial-index support`,
    after: `# drop the construct or choose a capable engine`,
    targets: "Engine-specific by definition: consult the storage engine profile for the target engine.",
    docs: "docs/storage-engine.md",
    waiver: "Not waivable: capability assumptions must match engine reality.",
  },
  "structure": {
    title: "Canonical structure violation",
    kind: "enforces the canonical project layout: file homes, portable paths and physical containment",
    matters: "canonical homes are what make projects portable and tools predictable; a misplaced file is invisible to every home-based consumer",
    causes: [
      "files created by hand or by foreign tooling in non-canonical homes",
      "path separators or traversal outside the project root",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Move the file to its canonical home per docs/canonical-structure.md; fix separators and remove traversal." },
    ],
    before: `# entities defined in lekalo/planner.yaml`,
    after: `lekalo/planner/entities.yaml   # canonical home`,
    targets: "Target-neutral: structure rules apply before any target sees the project.",
    docs: "docs/canonical-structure.md",
    waiver: "Not waivable: homes are the addressing scheme.",
  },
  "target-profile": {
    title: "Target profile failure",
    kind: "validates target profiles: declared target identity, capability claims and their evidence",
    matters: "profiles are how targets promise what they can do; unproven capability claims produce generated code the target cannot run",
    causes: [
      "a profile declares capabilities the target SDK does not provide",
      "the profile was edited without re-admitting its evidence",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-derive the profile from the target's actual SDK capabilities and re-admit it with fresh evidence." },
    ],
    before: `# profile claims mysql window functions on sqlite`,
    after: `# correct capability claims, re-admit`,
    targets: "Target-specific by definition.",
    docs: "docs/target-profile.md",
    waiver: "Not waivable: capability claims must be true.",
  },
  "target": {
    title: "Target protocol failure",
    kind: "governs the target protocol: request/response admission, version negotiation and bounded transport for adapter processes",
    matters: "the protocol is the only channel adapters get; violations would let malformed or oversized traffic reach the host or hide adapter failures",
    causes: [
      "the adapter speaks a different protocol version than the host",
      "a response exceeds output bounds or arrives after the deadline",
      "request files reference paths outside the admitted boundary",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Align adapter and host protocol versions; respect output bounds and deadlines; regenerate request files through the owning flow." },
    ],
    before: `# adapter targets protocol v2, host speaks v1`,
    after: `# pin matching versions on both sides`,
    targets: "Per-target: each adapter documents its protocol pin and transport limits.",
    docs: "docs/target-protocol.md",
    waiver: "Not waivable: protocol discipline is the containment boundary.",
  },
  "tasklink": {
    title: "Task-links pilot failure",
    kind: "admits external task links and their integration, portability and execution evidence in the task-links pilot",
    matters: "task links bind semantic work to external trackers; unproven links would let claims of done-ness escape the evidence system",
    causes: [
      "a link references an external task id that cannot be resolved",
      "integration evidence is stale relative to the linked revision",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Refresh the link's evidence against the current revision; remove links whose external task no longer resolves." },
    ],
    before: `# task link to deleted tracker item`,
    after: `# re-validate or remove the link`,
    targets: "Pilot-scoped: execution stays read-only; portability evidence is per-target.",
    docs: "docs/task-external-links.md",
    waiver: "Not waivable: pilot evidence rules are the pilot.",
  },
  "trace": {
    title: "Neutral trace failure",
    kind: "admits the neutral trace manifest and validates its relations, revisions and digest pins",
    matters: "traces are neutral evidence linking requirements to behavior; broken pins would validate today's claims with yesterday's artifacts",
    causes: [
      "the manifest references relations or symbols absent from the current revision",
      "digest pins no longer match the cited attachments",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Re-export the trace from its producer against the current revision; fix relation references instead of loosening pins." },
    ],
    before: `lekalo trace validate --manifest t.json   # stale pin`,
    after: `# re-export, then validate`,
    targets: "Target-neutral: traces bridge Model facts and external evidence.",
    docs: "docs/trace-manifest.md",
    waiver: "Not applicable: staleness is recorded as data.",
  },
  "transaction": {
    title: "Storage transaction violation",
    kind: "checks declared transactions against storage semantics: write sets, isolation and recovery behavior",
    matters: "transactions are the difference between a failed operation and a corrupted state; undeclared behavior here is unrecoverable in production",
    causes: [
      "a transaction writes outside its declared resource set",
      "isolation assumptions exceed what the engine provides",
      "recovery behavior is undeclared for a mid-transaction failure",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the exact write set and recovery semantics; align isolation with the engine profile." },
    ],
    before: `# tx also touches audit_log, undeclared`,
    after: `# declare the write set fully`,
    targets: "Engine semantics come from the storage engine profile; declarations stay Model-level.",
    docs: "docs/transaction-concurrency.md",
    waiver: "Not waivable: undeclared transaction behavior blocks release.",
  },
  "transport": {
    title: "HTTP transport contract failure",
    kind: "validates the HTTP transport contract: declared routes, payload shapes and transport-level guarantees",
    matters: "transport is the system's actual attack and integration surface; undeclared routes or shapes are reachable behavior no review covered",
    causes: [
      "a handler serves a route the contract never declared",
      "payload shapes disagree between declaration and implementation",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Declare the route and payload in the Model, or remove the undeclared surface; regenerate transports from declarations." },
    ],
    before: `# ad-hoc route added in target code`,
    after: `# declare the operation in the Model and regenerate`,
    targets: "Per-target transports differ; the declared contract is the shared authority.",
    docs: "docs/transport-http.md",
    waiver: "Not waivable: reachable undeclared surface is a security boundary.",
  },
  "validate": {
    title: "Validation run refusal",
    kind: "admits validation runs: profile selection, scope and the bounded report the run produces",
    matters: "validation is the entry gate every other guarantee cites; refusing malformed runs keeps validated a meaningful claim",
    causes: [
      "the selected validation profile does not exist or is unsupported",
      "scope or module selections reference unknown modules",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Select an admitted profile and a known module scope, then re-run." },
    ],
    before: `lekalo validate --profile strictest`,
    after: `lekalo validate --profile strict`,
    targets: "Target-neutral: validation precedes projection.",
    docs: "docs/validation.md",
    waiver: "Not applicable: run admission is precise, not policy.",
  },
  "versioning": {
    title: "Versioning or migration failure",
    kind: "owns version transitions: compatibility classification, migration planning and the sealed migration catalog",
    matters: "version transitions are where data is lost or contracts break; refusing unproven transitions is the whole point of the service",
    causes: [
      "a migration step is unknown to the sealed catalog",
      "the transition's compatibility class disagrees with the actual delta",
      "apply was requested without the transaction's exact evidence",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Plan migrations through the owning service from the current state; never hand-write migration steps or re-classify a delta to dodge the gate." },
    ],
    before: `lekalo migrate --apply --plan hand-made.json`,
    after: `lekalo migrate --plan --from <current> --to <target>\nlekalo migrate --apply --transaction <id>`,
    targets: "Per-target storage migrations add engine steps; classification semantics are shared.",
    docs: "docs/versioning.md",
    waiver: "Not waivable: transitions need reviewed evidence, not flags.",
  },
  "watch": {
    title: "Watch loop failure",
    kind: "runs the incremental watch loop: snapshot reconciliation, bounded scheduling and complete diagnostic replacement per settled revision",
    matters: "watch is what editors and agents consume live; missed or partial updates would show stale errors as current, or current errors as gone",
    causes: [
      "filesystem events were lost or coalesced past the quiet window",
      "the reconciler found inputs it cannot snapshot within bounds",
      "a terminal condition (cancelled run) was mistaken for success",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Let the reconciler rescan (or restart watch); treat each terminal event as its own state; never infer settled semantics from a missed stream." },
    ],
    before: `# editor saved during the debounce window`,
    after: `# wait for the settled-revision event; the reconciler rescans otherwise`,
    targets: "Backends are platform-specific (native vs polling); semantics are reconciled from content, not events.",
    docs: "docs/cli.md",
    waiver: "Not applicable: loop health is data, not policy.",
  },
  "worklog-changelog": {
    title: "Worklog changelog pilot failure",
    kind: "admits worklog-changelog pilot attachments and validates their consumer oracles, ports and refusals",
    matters: "the pilot's value is honest evidence: attachments that claim consumer-visible change without admission would poison the pilot's conclusions",
    causes: [
      "an attachment cites worklogs or receipts that do not resolve",
      "the consumer oracle disagrees with the current attachment family",
    ],
    fixes: [
      { class: "manual-only", recipeId: null, description: "Regenerate the attachment through the pilot's documented flow and re-admit; fix the oracle and attachment disagreement explicitly." },
    ],
    before: `# hand-built changelog attachment`,
    after: `# regenerate via the pilot flow, then re-admit`,
    targets: "Pilot-scoped: ports must record their own evidence per target.",
    docs: "docs/m8/issue-51-implementation.md",
    waiver: "Not waivable: pilot honesty is the deliverable.",
  },
};

// Every fix class used by templates must stay in the closed vocabulary.
for (const [family, profile] of Object.entries(families)) {
  assert.ok(profile.title && profile.kind && Array.isArray(profile.causes) && profile.causes.length > 0, `family ${family}: template incomplete`);
  for (const fix of profile.fixes) {
    assert.match(fix.class, CLASS_PATTERN, `family ${family}: closed fix class`);
  }
  assert.ok(profile.docs.startsWith("docs/"), `family ${family}: docs owner`);
}

/// One explain entry for one registry entry. Deterministic per code:
/// the same registry facts always produce the same catalog entry bytes.
function entryFor(registryEntry) {
  const [family] = registryEntry.id.split(".");
  const profile = families[family];
  assert.ok(profile, `no family template for ${registryEntry.id}`);
  const statuses = [...registryEntry.allowed_statuses].sort();
  const blocking = statuses.includes("invalid");
  const related = registry.entries
    .filter((other) => other.id !== registryEntry.id && other.id.split(".")[0] === family)
    .map((other) => other.code)
    .sort();
  const executable = profile.fixes.find((fix) => fix.recipeId !== null) ?? null;
  return {
    code: registryEntry.code,
    ruleId: registryEntry.id,
    title: profile.title,
    category: registryEntry.category,
    checks: `The rule ${profile.kind}.`,
    rationale: `It matters because ${profile.matters}.`,
    rootCauses: profile.causes,
    fixOptions: profile.fixes.map((fix) => ({
      class: fix.class,
      ...(fix.recipeId !== null ? { recipeId: fix.recipeId } : {}),
      description: fix.description,
    })),
    executableRecipeId: executable ? executable.recipeId : null,
    examples: [{ before: profile.before, after: profile.after }],
    relatedCodes: related,
    severityProfile: {
      defaultSeverity: registryEntry.default_severity,
      severities: [registryEntry.default_severity],
      allowedStatuses: statuses,
      blocking,
      waiverPolicy: profile.waiver,
    },
    targetNotes: [{ scope: "general", note: profile.targets }],
    docsLinks: [{ version: version, url: `https://dev.lekalo/docs/${version}/${profile.docs.slice(5)}` }],
  };
}

export function buildCatalog() {
  const entries = registry.entries.map(entryFor);
  // Closed grammar, sorted by diagnostic code; unknown codes are outside
  // the catalog by construction (closed lookups).
  entries.sort((a, b) => (a.code < b.code ? -1 : a.code > b.code ? 1 : 0));
  assert.equal(new Set(entries.map((entry) => entry.code)).size, entries.length, "code collision");
  return {
    schemaVersion: `lekalo/explain-catalog/v${version}`,
    identity: `dev.lekalo.explain-catalog@${version}`,
    catalogVersion: version,
    registryIdentity: registry.identity,
    registryDigest: sha256(readFileSync(resolve(root, registryPath))),
    closed: true,
    entries,
  };
}

const check = process.argv.includes("--check");
const catalog = buildCatalog();
const bytes = Buffer.from(pretty(catalog), "utf8");
if (check) {
  let current = null;
  try {
    current = readFileSync(resolve(root, catalogPath));
  } catch {
    console.error(JSON.stringify({ ok: false, reason: "catalog-missing", path: catalogPath }));
    process.exit(1);
  }
  if (!current.equals(bytes)) {
    console.error(JSON.stringify({ ok: false, reason: "catalog-drift", path: catalogPath, expectedDigest: sha256(bytes), actualDigest: sha256(current) }));
    process.exit(1);
  }
  console.log(JSON.stringify({ ok: true, gate: "explain-catalog-check", entries: catalog.entries.length }));
} else {
  writeFileSync(resolve(root, catalogPath), bytes);
  console.log(JSON.stringify({ ok: true, wrote: catalogPath, entries: catalog.entries.length, digest: sha256(bytes) }));
}
