# ADR-0014: The neutral trace manifest

Date: 2026-09-05
Status: accepted for issue #22

Custody: this issue published product 0.1.19 (annotated tag `v0.1.19` on
`31468e9`); issue #23 published product 0.1.20 (annotated tag `v0.1.20` on
`eef1863`); issue #15 published product 0.1.21 (annotated tag `v0.1.21` on
`9ab5b07`); issue #21 published product 0.1.22 (annotated tag `v0.1.22`
on `2dab70e`); issue #20 published product 0.1.23 (annotated tag
`v0.1.23` on `15be55a`); issue #16 published product 0.1.24 (annotated
tag `v0.1.24` on `b4109e5`); issue #17 now carries the **prospective product
candidate 0.1.25** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden lock
and its digests, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #14 published product 0.1.12
(annotated tag `v0.1.12` on `81666da`). The trace contract version
(`lekalo/trace-manifest/v1.0.0`, identity
`dev.lekalo.trace-manifest@1.0.0`) is independent of the product release,
of the Model/IR/graph/effects contract versions, of the
adapter/protocol/profile versions, and of the diagnostic registry by
design.

## Context

Issue #22 asks for a neutral traceability format that expresses the
authority chain `openspec.requirement -> lekalo.stable-symbol ->
lekalo.target-binding -> lekalo.scenario-or-source-native.native-test ->
hlv.gate-diagnostic` without binding AIFHub consumers to Rust and without
embedding any foreign wire. The research briefs (run run_088695f63032:
briefs msg_c5cd0deef1c1, msg_4dbfadb4c8d4, msg_8ba9145d1525, precision
msg_54fa022d38ed, worker_done msg_7790e71252dd) recorded the owner
decisions this ADR adopts.

## Decision

### 1. Ownership split: neutral contract vs persisted evidence

The accepted authority-matrix 1.3.1 entry fixes `trace.manifest` as
AI Factory-owned direct evidence (`canonicalOwner ai-factory`,
`classification direct-evidence`, `allowedPaths [.ai-factory/traces/**]`,
readers openspec/ai-factory/lekalo/hlv/source-native/aifhub-adapter,
writers ai-factory/hlv/aifhub-adapter). Issue #22 does not change that:
**Lekalo owns the neutral Trace contract/schema, the typed validator, the
canonical writer, and the derived queries; AI Factory ownership of
persisted `trace.manifest` instances is untouched.** HLV/OpenSpec/native
formats are referenced through verbatim external ids and never enter
core; AIFHub consumption stays process/JSON-based with no Rust coupling
(the closed schema is the only artifact a consumer needs). Any change to
registry/path/writer semantics is a separately reviewed #2 successor.

### 2. Independent closed contract

[`contracts/trace-manifest.schema.v1.0.0.json`](../../contracts/trace-manifest.schema.v1.0.0.json)
is the single new contract file (discriminator
`lekalo/trace-manifest/v1.0.0`, identity
`dev.lekalo.trace-manifest@1.0.0`). The diagnostic registry file is
unchanged: trace failures reuse the graph-family infrastructure rules
(`graph.input-invalid`, `graph.unknown-node`, `graph.traversal-limit`,
`graph.export-limit`) plus `loader.io` for unreadable documents, exactly
like the accepted #14 effect graph. Gap/status/completeness semantics are
closed result data, not diagnostics. `contracts/` gains no sidecar; the
schema's own custody follows the repository rule.

### 3. Closed shape and node union

The top level is closed: `schemaVersion`, `identity`, `manifestId`
(stable logical export id), `projectRef` (opaque project identity),
`completeness` (`full|partial`), exact `sourceRevision` (40 or 64 hex),
`modelRef` (pinned to Model `0.1.0`/`1.0.0`), optional `irRef` (pinned to
the accepted IR `0.1.0`), optional `graphRef` (pinned to Graph `1.0.0`),
optional `artifactManifestRef` (generic semver: the #21 seam is
referenced, not imported), `exportProfile`, `nodes`, `relations`, `gaps`.
Seven node kinds cover the chain: `requirement`, `symbol`, `artifact`,
`scenario`, `native_test`, `gate`, `diagnostic`. Each carries exactly its
own identity field (exclusivity is validator-enforced, like the graph
subkind rule); artifact nodes require the #21 ownership words
(`generated|scaffolded|checked|external|custom`), a privacy-safe logical
path, and a content digest; scenario/test/gate/diagnostic nodes carry
opaque exact ids plus optional contract version and evidence digest.
External refs preserve foreign original ids verbatim
(`openspec|hlv|source-native|aifhub|lekalo` + `originalId` + optional
contract version/revision/digest) — never display text, paths, or
rendered prose.

### 4. Relations, occurrence, provenance, confidence, status

Eight relation kinds with an explicit endpoint matrix, validated before
indexing: `implements` (symbol→requirement), `binds` (symbol→artifact),
`covers` (scenario→symbol|requirement), `verifies`
(native_test→scenario|symbol), `evidences`
(gate→native_test|scenario|symbol), `derived_from` and `supersedes`
(same-kind lineage), `references` (gate↔diagnostic and same-kind pairs).
Every relation carries a canonical `relationId` = `sha256:` over the
fixed domain tag plus (kind, from, to, occurrence) — the tuple fully
determines the digest, repeated endpoint pairs stay distinct, and nothing
is ever collapsed by endpoint pair. `provenance` is closed
(`origin declared|observed|inferred|imported`, `sourceSystem`,
exact `sourceRevision`+`sourceDigest`, `recordedBy` adapter id, optional
safe logical path). `confidence` is the closed word vocabulary
`exact|high|medium|low|unknown` — numeric confidence would need a
decimal canonicalization rule this contract deliberately does not carry.
`status` is closed (`confirmed|candidate|stale|conflicting|invalid|
unsupported|infrastructure`) and never inferred from missing fields. A
confirmed relation requires a matching manifest revision, non-inferred
origin, exact-or-high confidence, and at least one cited evidence
reference — inferred/candidate evidence never counts as a passing gate.

### 5. Completeness, gaps, dangling semantics

`full` requires zero gaps, every relation confirmed, and complete sink
coverage for the export profile: every sink node (requirement or
artifact) reachable from at least one source node (gate, native test, or
scenario) following relation directions — the five closed profiles are
`requirement-to-gate`, `requirement-to-test`,
`requirement-to-scenario`, `artifact-to-gate`, `artifact-to-test`.
`partial` requires at least one explicit gap. Ordinary relations to
absent nodes are fatal (`dangling-endpoint`); only an explicit typed gap
(`missing-requirement` … `privacy-redacted`, twelve closed kinds) may
represent a dangling/unresolved external reference, a gap can never be
`confirmed`, and it never satisfies a gate. Uncovered sinks in partial
manifests are reported explicitly in the result — never silently. The
authority's missing-link state maps to gaps; timestamps are excluded from
the wire entirely (evidence metadata stays with the evidence owner), so
re-export of the same revision is byte-stable.

### 6. Determinism, canonical export, digest

Canonical bytes are compact UTF-8 JSON with object keys in fixed contract
order, nodes sorted by (kind, id), relations by (kind, from, to,
occurrence), gaps by (kind, anchor, expected), and external/evidence
references sorted as sets; no BOM, no trailing LF in the digest payload.
Input insertion order, whitespace, and key order never leak: the pinned
golden (`tests/fixtures/trace/golden/planner.trace.json`) is regenerated
byte-identically from permuted input, and the manifest digest is
`sha256:` over exactly those canonical bytes, pinned in the committed
sidecar. The Node gate recomputes canonical bytes, digest, and relation
identities with a second independent implementation (exact Ajv 8.17.1
strict on Node 18 and 24).

### 7. Privacy-safe logical paths

Path fields are project-relative POSIX logical paths validated lexically
through the accepted structure grammar (`project_fs::path_violation`):
absolute spellings, drives/UNC/devices/ADS, backslashes, percent
escapes, `.`/`..`, empty/control segments, trailing dot/space, DOS device
names, short-name-like aliases, and non-NFC spellings are all rejected
(`structure.path-*` codes as the stable diagnostic detail). No physical
root, URL, username, source snippet, transcript, or volatile host data
may appear anywhere in the wire; lexical acceptance never claims physical
containment.

### 8. CLI handoff

`lekalo trace validate PATH`, `lekalo trace export PATH`, and
`lekalo trace query PATH SELECTOR` only read the document bytes, select,
render, and map exits onto the accepted 0/1 envelope. The core owns every
decision. Queries are the closed forward/reverse set
(`requirements-for:ID`, `symbols-for:ID`, `artifacts-for:ID`,
`tests-for:ID`, `gates-for:ID`, `diagnostics-for:ID`, `gaps`) answered
from the precomputed adjacency; unknown subjects are explicit
`graph.unknown-node` failures, known-but-unmatched subjects are empty
successes. No command writes anything, executes a test or gate, spawns a
provider, or claims runtime equivalence (#27/#28/#29/#31/#47/#56/#91/
#103 own those seams).

## Consequences

- #21 artifact manifests and #23 scenarios plug in as typed opaque
  references without a shape change here; when their contracts are
  accepted, adapters bind their exact versions into `artifactManifestRef`
  and the evidence digests.
- AIFHub Extension consumes the closed schema directly (process/JSON, no
  Rust coupling) and derives its provider-specific evidence from the
  normalized relation array; reverse indexes are derived, rebuildable
  data, never a second source of truth.
- #16 impact checks can join the trace chain against the #13 graph by
  symbol id; #103 reports can project gap/status rows without parsing
  foreign formats.
- Registry/path/writer changes remain a #2 successor; this contract
  neither writes traces nor claims their custody.

## References

- [docs/trace-manifest.md](../trace-manifest.md) — the trace surface and guarantees.
- [ADR-0005](0005-semantic-ids.md) — the semantic-ID grammar the symbol nodes reuse.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
- [ADR-0012](0012-dependency-graph.md) — the graph whose relation/confidence vocabulary and adapter grammar are reused.
- [ADR-0013](0013-effect-graph.md) — the precedent for routing failures through the unchanged #11 registry.
