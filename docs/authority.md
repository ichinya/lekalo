# Artifact authority and synchronization boundaries

Status: accepted historical baseline `1.2.0`; rejected/yanked candidate
`1.3.0`; reviewed corrective successor `1.3.1`.

The normative accepted-version index is
[`contracts/authority-contracts.manifest.json`](../contracts/authority-contracts.manifest.json).
It binds each contract to an exact `{contractId, version, digest}` and an
exact-byte SHA-256 sidecar. The versioned contracts are
[`1.2.0`](../contracts/authority-matrix.v1.2.0.json) and
[`1.3.1`](../contracts/authority-matrix.v1.3.1.json). The exact `1.3.0` bytes
remain at [`authority-matrix.v1.3.0.json`](../contracts/authority-matrix.v1.3.0.json)
only as a rejected/yanked candidate and are never an accepted authority
reference. The historical
[`authority-matrix.v1.json`](../contracts/authority-matrix.v1.json) file is an
exact byte-for-byte alias of `1.2.0`, not a mutable current pointer.
The rationale is recorded in
[ADR-0001](adr/0001-artifact-authority-boundaries.md). If prose and the
machine-readable contract disagree, adapters must stop with a contract-version
or policy error; they must not guess which form is newer.

The reviewed successor exact identity is:

```text
dev.lekalo.authority-matrix@1.3.1@sha256:5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3
```

The authority contract version is independent from schema and product
versions. This change does not advance the Lekalo product from the `0.0.1`
candidate.

## Authority matrix

`canonical owner` means the only layer allowed to decide the meaning of that
artifact kind. It does not mean every artifact is canonical: `derived`,
`cached`, and `runtime-only` classifications remain non-authoritative even
though one owner governs their format and lifecycle.

| Artifact kind | Canonical owner | Class | Canonical / governed paths | Other layers may |
|---|---|---|---|---|
| Requirements, change intent, delta specs, expected behavior | OpenSpec | canonical | `openspec/specs/**`, `openspec/changes/**` | read and reference stable requirement IDs |
| Execution plans and task state | AI Factory | canonical | `.ai-factory/plans/**` | read status through the workflow contract |
| Runtime state and agent lifecycle | AI Factory | runtime-only | `.ai-factory/state/**` | read exported status; never treat it as requirements |
| Normalized provider evidence envelope | AI Factory | derived | `.ai-factory/qa/**` | write only through the AIFHub adapter contract |
| Semantic model, stable symbols, effects, scenarios, target bindings | Lekalo | canonical | `lekalo/**` | read and reference stable symbols |
| Brownfield import drafts, caches, generated intermediates | Lekalo | derived/cached | `.lekalo/import/**`, `.lekalo/cache/**`, `.lekalo/generated/**` | inspect; explicitly adopt a draft before it becomes canonical |
| Validation results, traceability results, gate diagnostics | HLV | canonical for the HLV result only | `.hlv/**`; confirmed HLV greenfield `project.yaml` | copy a provenance-preserving envelope to AI Factory QA |
| Source code and native test definitions | Source/native toolchain | direct runtime evidence | project-native source and test paths | read, generate a marked draft, or propose a patch |
| Generated summaries and rules | AI Factory | derived | `.ai-factory/rules/generated/**`, derived QA paths | read; never use them to rewrite canonical inputs |
| Generated code | Source/native toolchain | derived until explicit adoption | project-native source paths | generate with provenance; never silently promote it |

The `1.3.1` registry contains 49 stable kinds: all 26 `1.2.0` IDs plus 23
additive kinds needed for the #120 handoff. Every `artifactKinds[]` entry has
exactly one scalar `canonicalOwner` plus non-empty `allowedPaths`,
`allowedReaders`, and `allowedWriters`. For the 26 predecessor kinds,
`allowedReaders` lists every declared actor and therefore preserves the
unrestricted reference behavior of `1.2.0`. Reader declarations are authority
integration roles, not privacy access grants.

Authority lifecycle is only one of `canonical`, `derived`, `cached`,
`runtime-only`, or `direct-evidence`. It is not privacy classification. The
kind registry forbids `dataSensitivity` and `exportDisposition`; #120 owns
those separate fields and must reference these kinds rather than alias them.

### Reviewed successor registry additions

All 23 additions have the same explicit reader set: `openspec`, `ai-factory`,
`lekalo`, `hlv`, `source-native`, and `aifhub-adapter`. This permits
authority-level referencing only; privacy/export permission remains outside
this contract.

| Stable kind ID | Owner | Lifecycle | Logical paths | Writers |
|---|---|---|---|---|
| `authority.contract` | Lekalo | canonical | `contracts/authority-matrix.v1.json`, `contracts/authority-matrix.v*.json` | Lekalo |
| `authority.contract-manifest` | Lekalo | canonical | authority manifest and authority SHA-256 sidecars | Lekalo |
| `privacy.policy` | Lekalo | canonical | versioned privacy policy JSON and sidecars | Lekalo |
| `privacy.export-schema` | Lekalo | canonical | versioned privacy export schema JSON | Lekalo |
| `diagnostics.raw-tool-output` | AI Factory | direct-evidence | `.ai-factory/evidence/raw/**` | AI Factory, HLV, AIFHub adapter |
| `context.capsule` | AI Factory | runtime-only | `.ai-factory/context/**` | AI Factory, AIFHub adapter |
| `trace.manifest` | AI Factory | direct-evidence | `.ai-factory/traces/**` | AI Factory, HLV, AIFHub adapter |
| `ai.prompt` | AI Factory | direct-evidence | `.ai-factory/runs/**/prompts/**` | AI Factory, AIFHub adapter |
| `ai.response` | AI Factory | direct-evidence | `.ai-factory/runs/**/responses/**` | AI Factory, AIFHub adapter |
| `ai.tool-transcript` | AI Factory | direct-evidence | `.ai-factory/runs/**/tools/**` | AI Factory, AIFHub adapter |
| `metrics.evaluation-evidence` | HLV | direct-evidence | `.hlv/evidence/metrics/**` | HLV |
| `fixture` | source/native | direct-evidence | `tests/fixtures/**`, `fixtures/**` | source/native |
| `repository.identity` | source/native | direct-evidence | `.source-native/repository/**` | source/native |
| `native.symbol-identity` | source/native | direct-evidence | `.source-native/symbols/**` | source/native |
| `native.source-map` | source/native | direct-evidence | `**/*.map` | source/native |
| `consumer.model` | Lekalo | derived | `.lekalo/consumer/model/**` | Lekalo |
| `consumer.target-bindings` | Lekalo | derived | `.lekalo/consumer/bindings/**` | Lekalo |
| `export.artifact` | Lekalo | derived | `.lekalo/privacy/exports/**` | Lekalo |
| `export.decision` | Lekalo | direct-evidence | `.lekalo/privacy/decisions/export/**` | Lekalo |
| `redaction.artifact` | Lekalo | derived | `.lekalo/privacy/redacted/**` | Lekalo |
| `redaction.decision` | Lekalo | direct-evidence | `.lekalo/privacy/decisions/redaction/**` | Lekalo |
| `aggregate.artifact` | Lekalo | derived | `.lekalo/privacy/aggregates/**` | Lekalo |
| `aggregate.decision` | Lekalo | direct-evidence | `.lekalo/privacy/decisions/aggregate/**` | Lekalo |

The exact machine entries, including lower-case owner IDs and complete reader
arrays, are authoritative in `authority-matrix.v1.3.1.json`. This table is a
review aid and does not create alternative aliases.

## Path boundaries

Canonical Lekalo files live under `lekalo/**`. No Lekalo semantic model,
symbol, effect, scenario, or target binding may be stored under
`openspec/changes/**`, `openspec/specs/**`, `.ai-factory/**`, or `.hlv/**`.
`.lekalo/**` is reserved for non-canonical Lekalo drafts, cache, runtime data,
and generated intermediates.

Protected trees are owner-governed:

| Path | Owner | Write boundary |
|---|---|---|
| `openspec/specs/**` | OpenSpec | OpenSpec only |
| `openspec/changes/**` | OpenSpec | OpenSpec only |
| `.ai-factory/plans/**` | AI Factory | AI Factory only |
| `.ai-factory/state/**` | AI Factory | AI Factory only |
| `.ai-factory/qa/**` | AI Factory | AI Factory, or AIFHub adapter for normalized derived evidence |
| `.ai-factory/rules/generated/**` | AI Factory | AI Factory generated output only |
| `lekalo/**` | Lekalo | Lekalo only |
| `.lekalo/**` | Lekalo | Lekalo non-canonical output only |
| `.hlv/**` | HLV | HLV only |
| versioned authority/privacy contracts and SHA-256 sidecars listed by `1.3.1` | Lekalo | Lekalo contract owner only |
| `.ai-factory/evidence/**`, `.ai-factory/context/**`, `.ai-factory/traces/**`, `.ai-factory/runs/**` | AI Factory | declared producer writers only; owner remains AI Factory |
| `.source-native/**`, `tests/fixtures/**`, `fixtures/**` | source/native | source/native evidence writers only |
| root `project.yaml` with `context.hlvLayoutConfirmed: true` and `hlv.project-contract` kind | HLV | conditional; otherwise it is not an HLV boundary |

Every `1.3.1` boundary declares an exact `artifactKinds` set and explicit
`readers` and `writers`. A reference is allowed only when its selected kind is
in the boundary set and its actor is permitted by both the boundary and the
kind. `read` source references use reader sets; `write` target references use
writer sets. One-way sync uses read/write, proposal-only reconciliation and
reference-only claim use read/read, and promote/adopt use read/write.

The evaluator considers all matching patterns. It orders them by the following
tuple, from most to least specific: literal prefix segments, literal segment
count, literal character count, segment depth, then fewer wildcard tokens.
The most-specific policy wins. Equal-specificity matches are valid only when
their owner, kind, reader, and writer policies are identical; conflicting or
ambiguous ties are rejected at contract load. The checker also rejects unknown
kinds, owner disagreements, boundary reader/writer sets broader than their
kinds, case-alias duplicates, and tied overlapping policies. Thus broad
source/native patterns such as `**` and `**/*.map` cannot relabel a more
specific protected subtree.

`native.source-map` and `native.symbol-identity` are direct source/native
evidence. Only `source-native` may write them; Lekalo and AIFHub may consume
them as readers. A future Lekalo-produced derived mapping requires a new,
reviewed derived kind and path—it cannot reuse either direct-evidence label.
Likewise, `.hlv/evidence/metrics/**` has an exact
`metrics.evaluation-evidence` boundary and only HLV may write it. AI Factory
and the AIFHub adapter may read that evidence or submit their own envelopes in
their governed paths, but may not write inside `.hlv/**`.

Paths in the contract are project-relative POSIX paths. Absolute paths,
backslashes, empty segments, `.` and `..` are rejected, so an adapter cannot
escape a boundary through alternate path spelling. Inputs must already be
decoded; encoded separators/traversal and control characters are rejected.
Protected-boundary comparison is conservatively case-insensitive so a path such
as `OpenSpec/changes/**` cannot bypass policy on a case-insensitive filesystem.
Every segment ending in a dot or space is rejected, as are colons (including
NTFS alternate-data-stream syntax), DOS device names such as `CON`/`NUL`/
`COM1`, and short-name-like `~<number>` segments.

These are lexical checks, not proof of physical containment. The checker cannot
inspect 8.3 aliases, symlinks, junctions, reparse points, or filesystem races.
Before filesystem I/O, an adapter must resolve the existing parent and final
target with platform-safe APIs, reject alias/reparse escapes, and verify the
physical path remains inside the intended project and owner root. Rejection of
short-name-like spelling here reduces ambiguity but does not replace physical
resolution.

Source, native-test, and generated-code kinds use a language-neutral `**` path
pattern because target layouts differ. A target adapter must first classify the
path as source/test/generated material; the protected owner boundaries above
still win, so this wildcard cannot be used to write an OpenSpec, AI Factory,
Lekalo, or HLV tree under a source-native label.

## Direction of data flow

Generic one-way `sync` is limited to canonical/direct-evidence input and a
derived/cached/runtime-only target. It cannot write a canonical or
direct-evidence target, even when both kinds have the same owner. Promotion and
brownfield adoption are separate actions with exact kind pairs and mandatory
explicit-adoption, review, and provenance evidence. Provenance is a strict
object containing a non-empty source revision, a `sha256:<64 hex>` source
digest, and the non-empty reviewer/recorder identity.

`claim` is reference-only in contract v1. Its operation must carry
`substitute: false`; every well-formed `substitute: true` is a policy denial for
every owner, artifact kind, and classification pair. It cannot replace
`promote`, `adopt`, or any owner-scoped write.

The default flow is canonical input to derived output:

```text
OpenSpec requirement (read)
  -> Lekalo stable symbol (Lekalo-owned write)
  -> target binding (Lekalo-owned write)
  -> source/native test reference (read)
  -> HLV gate result (HLV-owned write)
  -> AI Factory provider evidence envelope (derived one-way copy)
```

Each edge stores stable IDs rather than copying the upstream artifact body. A
trace link contains a requirement reference, semantic symbol ID, target binding
and revision, scenario/test reference, then gate evidence with exact tool and
protocol versions, change/revision, timestamp, and provenance.

There is no silent bidirectional synchronization. Cross-owner reconciliation is
a proposal-only operation: it performs no canonical writes, identifies each
direction explicitly, emits loss, conflict, and provenance reports, and requires
approval from every affected canonical owner. The owners then apply changes in
their own trees.

### Allowed examples

1. Lekalo reads an OpenSpec requirement and writes a stable symbol to
   `lekalo/model/application.json`.
2. HLV writes its result under `.hlv/evidence/**`; the AIFHub adapter copies an
   exact, provenance-preserving envelope one way to
   `.ai-factory/qa/<change-id>/providers/hlv.json`.
3. Lekalo emits marked generated code with provenance. The source owner reviews
   and invokes `promote` before it is reclassified as source.
4. Lekalo imports observed brownfield facts into `.lekalo/import/**`; after
   review, the Lekalo owner invokes `adopt` with provenance to write `lekalo/**`.
5. A reconciliation tool produces loss/conflict/provenance reports and owner
   proposals without writing either canonical tree.

### Forbidden examples

1. Lekalo writes a model into `openspec/changes/<change-id>/**`.
2. HLV or AI Factory edits OpenSpec requirements because a gate disagrees.
3. AIFHub mirrors OpenSpec and Lekalo in both directions automatically, even if
   the files appear lossless. Without reports it is invalid; with reports it is
   still proposal-only until each owner acts.
4. HLV evidence is marked as satisfying or replacing a missing OpenSpec
   requirement. Evidence can support a requirement, never create one.
5. Generated code, a summary, a rule, or a provider export is silently promoted
   to canonical source.
6. `.lekalo/import/**` is generically synced into `lekalo/**`, or
   `.ai-factory/rules/generated/**` into `.ai-factory/plans/**`; same-owner
   status does not bypass classification direction.
7. Any `claim` uses `substitute: true`, including generated code -> source,
   brownfield draft -> semantic model, or generated rule -> execution plan.

## Conflict resolution and precedence

Precedence is scoped by question, not a global last-writer-wins order:

| Question in conflict | Authority | Required action |
|---|---|---|
| What and why should change? | OpenSpec | fix OpenSpec through its owner, or keep the conflict blocking |
| How is work planned/executed? | AI Factory | fix the workflow artifact through AI Factory |
| What semantic element or binding exists? | Lekalo | fix the Lekalo model through Lekalo |
| What did HLV validate and diagnose? | HLV | preserve the raw HLV result and diagnostic codes |
| What implementation/test actually exists or runs? | Source/native toolchain | preserve direct evidence; do not reinterpret it as expected behavior |

When authorities disagree, neither silently rewrites the other. The consumer
records both references plus loss/conflict/provenance information and returns a
blocking conflict for any operation that depends on the disputed link. A change
to the intended behavior belongs in OpenSpec; a change to semantic identity
belongs in Lekalo; an implementation correction belongs in source/tests. HLV
then produces a new result rather than editing earlier evidence.

## Incomplete and contradictory models

- A missing link is explicit `unresolved`; tools must not invent a requirement,
  symbol, binding, test, or gate.
- An incomplete optional provider produces `warn/degraded`. A phase configured
  to require that link returns `fail/blocking`.
- An internally contradictory Lekalo model blocks Lekalo validate/generate/
  verify operations that depend on it. Read-only inspection may continue with
  diagnostics.
- A stale revision never counts as current evidence. The tool must regenerate
  derived evidence from the canonical owners; it must not patch canonical input.
- Absence of OpenSpec, Lekalo, or HLV does not transfer its authority to another
  layer. The corresponding artifact kind is simply unavailable.

## Greenfield and brownfield rules

For greenfield projects, initialization of each provider tree is an explicit
user action. Providers may be active in any supported combination, and no tool
is auto-installed or initialized by discovery. The HLV `project.yaml` path is
treated as HLV-owned only after read-only HLV layout/capability discovery
confirms it; the adapter then asserts `context.hlvLayoutConfirmed: true` in the
operation. The logical checker validates that assertion is present and typed,
but it does not itself perform or prove discovery.

For brownfield projects, discovery and import are read-only by default. Observed
source facts are written as a derived draft under `.lekalo/import/**`; they only
become the canonical semantic model through the strict `adopt` action after
explicit adoption, review, and provenance evidence. Generic `sync` cannot write
that canonical target. Existing requirements are not fabricated from code,
native tests, HLV results, or generated summaries.

## Automatic enforcement

Run `node scripts/check-authority.mjs` for the current reviewed successor or
`node scripts/check-authority.mjs --contract-version 1.2.0` for the immutable
historical baseline. Both execute the same allowed, forbidden, and malformed
operation fixtures. Run `node scripts/test-authority-cli.mjs` for real
subprocess exit-code coverage across both versions. Run
`node scripts/test-authority-contracts.mjs` for exact-byte custody, registry
conformance, migration compatibility, contract mutations, unknown successors,
local aliases, runtime extensions, and successor-boundary inheritance. Run
`node scripts/test-authority-boundaries.mjs` for the exhaustive successor
matrix across every protected boundary, kind, actor, and read/write role.

Every operation is a strict object discriminated by `action`; unknown fields,
missing fields, wrong enums/types, and malformed nested refs/reports/evidence/
context are structural errors. Allowed decisions exit `0`; well-formed policy
denials exit `3`; malformed contracts or operations exit `1`. Output for a
well-formed single operation is deterministic JSON.

### Exact references and closed-exact profiles

An authority reference has the literal form
`contractId@version@sha256:<64 lowercase hex>`. The checker verifies:

1. the exact-byte manifest against its embedded trust anchor;
2. the selected manifest entry against the accepted checker profile;
3. the sidecar bytes and filename;
4. the selected contract's exact bytes, `contractId`, `version`, and semantic
   invariants before operation evaluation.

`--contract <path>` alone is invalid. A caller supplying a path must also pass
the exact accepted `--authority-ref`; this prevents a convenience/current path
from becoming authority by itself. `--contract-version <accepted-version>`
resolves the canonical versioned path through the trusted manifest.

Both accepted versions (`1.2.0` and `1.3.1`) are `closed-exact`. The exact
`1.3.0` triple is explicitly rejected/yanked and exits `1` before evaluation.
A byte reformat, semantic change,
recomputed caller digest, undeclared kind, local alias, boundary extension, or
unknown successor fails with exit `1`. Even the historical `1.2.0` field that
once described semantically identical copies is now wrapped by exact-reference
custody; only its published bytes satisfy the accepted triple.

For `1.3.1`, the checker additionally verifies the complete kind-bound
protected-boundary registry, exact specificity policy, kind/owner agreement,
reader/writer subset constraints, and absence of ambiguous equal-specificity
overlaps. It verifies the complete protected-boundary baseline, the exact
conditional HLV tuple including required boolean `true`, all artifact ownership
and classification data, sync/claim/promotion/adoption policies, path safety,
HLV non-substitution, conflict policy, project rules, and operation exit
semantics before evaluating an operation. Missing, altered, duplicate,
case-aliased, conflicting, or shadowing entries are contract errors and exit
`1` without an operation decision.

### Reviewed successor procedure

Published bytes are immutable. Evolution requires a new versioned JSON file,
an exact-byte SHA-256 sidecar, a manifest entry, the exact predecessor triple,
machine-readable added/changed/removed kind IDs, compatibility classification,
and a reviewed migration note. The checker must add a new trust profile and
mutation fixtures before the manifest may select it. A successor cannot delete
or change a stable kind silently. Unknown versions and stale references fail
closed.

The original registry handoff and its yanked status are documented in
[Authority contract migration 1.2.0 to the 1.3 line](authority-contract-migration-1.2-to-1.3.md).
Consumers must use the corrective
[1.3.0 to 1.3.1 migration](authority-contract-migration-1.3.0-to-1.3.1.md).
