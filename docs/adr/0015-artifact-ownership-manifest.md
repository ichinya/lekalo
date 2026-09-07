# ADR-0015: The generated-artifact ownership manifest and drift detection

Date: 2026-09-05
Status: accepted for issue #21

Custody: issue #23 published product 0.1.20 (annotated tag `v0.1.20` on
`eef1863`); issue #22 published product 0.1.19 (annotated tag `v0.1.19`
on `31468e9`); issue #15 published product 0.1.21 (annotated tag
`v0.1.21` on `9ab5b07`); issue #14
published product 0.1.12 (annotated tag `v0.1.12` on `81666da`); this
issue published product 0.1.22 (annotated tag `v0.1.22` on `2dab70e`);
issue #20 published product 0.1.23 (annotated tag `v0.1.23` on
`15be55a`); issue #16 published product 0.1.24 (annotated tag
`v0.1.24` on `b4109e5`); issue #17 published product 0.1.25 (annotated
tag `v0.1.25` on `e627fe5`); issue #18 published product 0.1.26 (annotated tag `v0.1.26` on
`3710179`); issue #24 published product 0.1.27 (annotated tag `v0.1.27` on
`ef7680d`); issue #25 published product 0.1.28 (annotated tag `v0.1.28` on `967bf52`); issue #62 published product 0.1.29 (annotated tag `v0.1.29` on `de6f8a7`); issue #26 now carries the **prospective product candidate 0.1.30** in every accepted path (workspace `Cargo.toml`, both `lekalo`
packages in `Cargo.lock` including the regenerated committed golden lock
and its digests, the `--version` behavior and its pinning tests,
`README.md`, `docs/cli.md`); issue #13 published product 0.1.11
(annotated tag `v0.1.11` on `007c01d`). The artifact-manifest contract
version (`lekalo/artifact-manifest/v1.0.0`, identity
`dev.lekalo.artifact-manifest@1.0.0`) is independent of the product
release, of the Model/IR/graph/lock/protocol contract versions, and of
the diagnostic registry by design.

## Context

Issue #21 requires every generated or checked artifact to record its
semantic owner, adapter/version, content hash, and regeneration policy,
so manual modification of a generated file can never go unnoticed, and
requires read-only `generate --check`, orphan detection, a safe
preview/confirmation-gated clean, and source maps from semantic symbols
to generated ranges, with paths protected against traversal and symlink
escape.

The research briefs (run run_088695f63032: worker_done msg_bbf587370b4b
and briefs msg_24d11dcf9090, msg_2eebd85e8675, msg_412e45557177) defined
the contract shape and the narrow owner decisions this ADR adopts. They
predate the accepted #3–#14 implementations, so every seam was re-pinned
against the exact parent commit `81666da` before implementation.

## Decision

### 1. One independent closed contract at one fixed location

The manifest publishes its own wire contract,
[`contracts/artifact-manifest.schema.v1.0.0.json`](../../contracts/artifact-manifest.schema.v1.0.0.json)
(discriminator `lekalo/artifact-manifest/v1.0.0`, identity
`dev.lekalo.artifact-manifest@1.0.0`), independent of every other
contract family. `contracts/` gains exactly this one new file. Owner
decision: one deterministic global manifest at
`.lekalo/generated/manifests/ownership.json` — target/profile identity
lives in the artifact entries, so there are no per-adapter divergent
files. The document is Lekalo-owned runtime-derived data under the
accepted authority kind `lekalo.generated-intermediate`; it is never a
canonical model file, lockfile, cache database, report bundle, or
release attestation, and never the sole copy of a semantic decision.

### 2. Canonicalization and a non-self-referential integrity digest

The lock discipline carries over verbatim: compact UTF-8 JSON, byte-sorted
object keys, closed sorted arrays, payload plus exactly one trailing LF.
`manifest_digest` is SHA-256 over the canonical payload with the
`manifest_digest` property removed and without the final LF. Artifact
content digests are SHA-256 over the exact observed file bytes
(`canonicalization: exact-file-bytes` — the only v1 canonicalizer);
newline, encoding, and whitespace changes are drift, never normalized
away. A digest mismatch is an integrity denial (exit 3), never a silent
baseline refresh.

### 3. Exact binding domains, no invented identities

`lock_ref` binds the exact `LockDigest`; `inputs` binds the canonical
Model and typed-IR payload pins; source maps bind an `input_revision`
equal to the digest over the recorded inputs; `adapter_ref` deep-equals
the resolved lock component (id, version, package digest, every platform
artifact pin, and the locked protocol version) — a version-only match is
invalid. `semantic_owner` is a #6 semantic symbol identity, never an
authority-matrix canonical owner and never free text. Owner decision: the
`artifact_kind` vocabulary is closed to `source`, `test`, `schema`,
`config`, `docs`, `data`, and `regeneration_policy` is an explicit
per-entry field paired one-to-one with the lifecycle
(`on-input-change`, `once`, `validate-only`, `reference-only`,
`manual-only`).

### 4. Adapter identity enters only with a published protocol

The lock forbids executable components while the target protocol is
unpublished, so in the current world no lock can contain an adapter and
an adapter ref would be unverifiable. Owner decision, mirroring the
lock's own rule: `adapter_ref` must be absent while the bound lock pins
no published target protocol and required once it does; a manifest that
disagrees with the lock's protocol state is stale, never a silent pass.
Generator identity is supplied by that locked adapter, never invented.

### 5. Lifecycle severities and verdict policy

`generated` staleness, drift, and absence — and any orphan — block the
check (exit 1). `scaffolded`, `checked`, `external`, and `custom` entries
are never overwritten and never repaired: their findings are reported in
the receipt without blocking, because those artifacts belong to the
implementation or are reference-only. `external`/`custom` unavailability
becomes blocking only when a selected profile requires the capability, a
selection that cannot arise before profile resolution lands (#29), at
which point the `core.capability-unavailable` seam already exists.

### 6. Explicit orphan scope and a confirmation-gated clean

v1 declares exactly one managed root, `.lekalo/generated/` — the
reserved runtime home. The scan never classifies arbitrary project
files; it refuses links, reparse points, and special entries fail-closed,
and refuses (rather than reports) unclaimed files whose names violate
the path policy. The manifest's own bookkeeping under
`.lekalo/generated/manifests/` is never output and can never be orphaned
or cleaned. The clean plan is pure and deterministic — orphan paths,
exact content digests, sizes, plus the bound manifest and lock digests,
hashed into a `planId` — and application revalidates the whole plan,
then deletes one file at a time with an immediate re-check before each
removal; any refusal aborts with zero further deletions. There is no
plain `--yes`: a mutating clean without a bound plan identity is
`lock.preview-required`, and a changed plan or project is
`lock.source-changed` with zero deletes. Residual TOCTOU between the
final re-check and the unlink is bounded by immediate pre-delete
revalidation of file identity and content; unix additionally refuses
multi-link files. Handle-relative deletion remains a platform seam for
the adapter-process work (#27/#91).

### 7. Source maps are traceability, not ownership transfer

Each source map binds one artifact key and the exact inputs revision to
sorted half-open byte ranges per semantic id. Ranges must lie inside the
observed bytes; the same semantic id's ranges may not overlap; a map on a
never-generated lifecycle carries no ranges. This surface serializes and
validates maps; it does not replace the accepted #8 SourceMap producer.

### 8. Diagnostics through the accepted #11 seam, registry unchanged

The registry stays at 1.2.0 and gains no rules: verdicts are typed result
data, and failures reuse the closest registered rules with bounded tokens
(`lock.noncanonical`, `lock.schema-invalid`,
`lock.unsupported-schema-version`, `lock.digest-mismatch`, `lock.stale`,
`lock.source-changed`, `lock.missing`, `lock.preview-required`,
`lock.reference-invalid`, `structure.document-missing`,
`structure.runtime-unexpected-entry`, `structure.path-link`,
`structure.path-special`, `loader.io`) — one diagnostic per finding, with
the artifact's logical path as the symbol and bounded owner/kind/lifecycle
tokens in the data object. Exit classes follow the accepted envelope: 0
valid, 1 invalid, 3 denied integrity and path refusals, 5 for a future
manifest discriminator; severity never computes an exit.

### 9. Ownership boundary

This surface supplies the pure manifest service, the check, and the
clean planner/applier. It does not spawn adapters, resolve profiles,
discover capabilities, own the cache, invent transaction or privacy
semantics, or add report formats: `generate`/`verify` orchestration
stays with #91, the adapter protocol and typed write plans with
#27/#28/#29, the cache with #20, authorization with #25, and reports
with #103. The `GenerateService::inputs` receipt is the typed seam #91
consumes for its preflight, and `LockRequirement::Required` semantics
are honored by requiring the lock for every generate mode.

## Consequences

- #91 can call the check before any adapter spawn and after writes, and
  consume `inputs`, `check`, and the clean plan as typed seams.
- #27/#28/#29 extended managed roots plug into the same orphan scan and
  adapter deep-equality without a shape change; adapter refs become
  mandatory the moment locks pin a published protocol.
- #20/#103 may cache and render from the same verdicts keyed by exact
  inputs; the cache can never become the canonical manifest authority.
- The wire fixtures (`tests/fixtures/artifacts/wire/`) are gate-checked
  with exact Ajv 8.17.1 on Node 18 and 24, including the canonical-byte,
  self-digest, and cross-field invariants the schema cannot express.

## References

- [docs/artifact-manifest.md](../artifact-manifest.md) — the manifest surface and guarantees.
- [ADR-0009](0009-lockfile.md) — the lock whose digest and canonical discipline this binds to.
- [ADR-0005](0005-semantic-ids.md) — the semantic owner identities.
- [ADR-0003](0003-canonical-structure-and-path-safety.md) — the path policy and runtime home.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and registry.
