# Neutral trace manifest

Issue #22 defines the neutral, HLV/OpenSpec-independent traceability
format: a closed, versioned wire (`lekalo/trace-manifest/v1.0.0`,
identity `dev.lekalo.trace-manifest@1.0.0`, contract
[contracts/trace-manifest.schema.v1.0.0.json](../contracts/trace-manifest.schema.v1.0.0.json))
that expresses the chain

```text
OpenSpec requirement -> Lekalo semantic symbol -> target binding/artifact
  -> scenario/native test -> HLV or native gate evidence
```

and that an AIFHub Extension can consume as a plain JSON schema with no
Rust coupling. Lekalo owns the contract, the typed validator, the
canonical writer, and the derived queries; the persisted `trace.manifest`
itself remains AI Factory-owned direct evidence under the accepted
authority boundary (`.ai-factory/traces/**`), and HLV/OpenSpec/native
formats enter only as verbatim external ids. See
[ADR-0014](adr/0014-trace-manifest.md) for the recorded owner decisions.

## Shape

The top level is closed: `schemaVersion`, `identity`, `manifestId`,
`projectRef`, `completeness` (`full|partial`), `sourceRevision`,
`modelRef` (Model `0.1.0`/`1.0.0`), optional `irRef` (IR `0.1.0`),
optional `graphRef` (Graph `1.0.0`), optional `artifactManifestRef`,
`exportProfile`, `nodes`, `relations`, `gaps`. Seven node kinds —
`requirement`, `symbol`, `artifact`, `scenario`, `native_test`, `gate`,
`diagnostic` — each carry exactly their own identity field; artifacts
add the ownership vocabulary, a privacy-safe logical path, and a content
digest. Every node may preserve foreign original ids verbatim in
`externalRefs` (`openspec|hlv|source-native|aifhub|lekalo`).

## Relations

Eight closed kinds with an explicit endpoint matrix: `implements`
(symbol→requirement), `binds` (symbol→artifact), `covers` (scenario→
symbol|requirement), `verifies` (native_test→scenario|symbol),
`evidences` (gate→native_test|scenario|symbol), `derived_from` and
`supersedes` (same-kind), `references` (gate↔diagnostic, same-kind
pairs). Identity is the tuple (kind, from, to, occurrence) hashed into
`relationId`, so repeated uses of the same pair stay distinct — one
symbol may implement many requirements and vice versa, and both remain
visible. Provenance (`origin`, `sourceSystem`, exact `sourceRevision`/
`sourceDigest`, `recordedBy`), confidence (`exact|high|medium|low|
unknown`), and status (`confirmed|candidate|stale|conflicting|invalid|
unsupported|infrastructure`) are closed vocabularies. A confirmed
relation requires the manifest revision, a non-inferred origin,
exact-or-high confidence, and cited evidence; inferred/candidate
evidence never passes a gate.

## Completeness, gaps, queries

`full` means zero gaps, all relations confirmed, and every sink of the
export profile reachable from its source kind (`requirement-to-gate`,
`requirement-to-test`, `requirement-to-scenario`, `artifact-to-gate`,
`artifact-to-test`). `partial` requires an explicit gap; twelve closed
gap kinds make every missing link visible, and a gap is never confirmed
and never satisfies a gate. Queries answer from the normalized relation
index — `requirements-for:SYMBOL`, `symbols-for:REQUIREMENT`,
`artifacts-for:SYMBOL`, `tests-for:SYMBOL`, `gates-for:TEST`,
`diagnostics-for:GATE`, `gaps` — sorted canonically, occurrence-safe,
and carrying status and confidence.

## CLI

```sh
lekalo trace validate tests/fixtures/trace/full.trace.json
# trace manifest planner-trace-full
#   completeness full (requirement-to-gate)
#   nodes 9; relations 9; gaps 0; uncovered sinks 0

lekalo trace export tests/fixtures/trace/full.trace.json > manifest.json
lekalo --json trace export tests/fixtures/trace/full.trace.json
# {"status":"valid","trace":{...},"manifestDigest":"sha256:..."} (canonical bytes embedded)

lekalo trace query tests/fixtures/trace/full.trace.json requirements-for:planner.focus_task
# requirement PLANNER-REQ-001 implements requirements.focus_task confirmed/exact
# requirement PLANNER-REQ-002 implements requirements.focus_task_archive confirmed/exact

lekalo trace query tests/fixtures/trace/partial.trace.json gaps
# gap missing-gate candidate anchor=symbol:planner.archive_task expected=hlv.gate.archive
# gap stale-revision stale anchor=symbol:planner.archive_task
```

`validate`/`export`/`query` exit 0 on stdout when the document is
accepted; wire violations, semantic violations, unknown query subjects,
and unreadable files exit 1 on stderr through the accepted #11
diagnostics (`graph.input-invalid` `LEK-GRAPH-003`,
`graph.unknown-node` `LEK-GRAPH-007`, `loader.io`) with fixed detail
tags and bounded echoes. Human and JSON are projections of the same
`DomainResult`; nothing is ever written to the filesystem.

## Determinism and gates

Canonical bytes are compact UTF-8 JSON with fixed key order and
canonical array sorts; the manifest digest is `sha256:` over exactly
those bytes. The pinned golden
(`tests/fixtures/trace/golden/planner.trace.json` plus `.sha256`)
is re-derived byte-identically from permuted input, and the
`scripts/test-trace-contracts.mjs` gate recomputes canonical bytes,
digest, and relation identities with a second independent
implementation under exact Ajv 8.17.1 strict on Node 18 and 24,
alongside the 37-fixture invalid matrix.
