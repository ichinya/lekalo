---
title: "ADR-0001: Artifact authority and synchronization boundaries"
status: accepted
date: 2026-08-30
contract-version: 1.3.1
predecessor-contract-version: 1.3.0-rejected-yanked
---

# ADR-0001: Artifact authority and synchronization boundaries

## Context

OpenSpec, AI Factory/AIFHub Extension, Lekalo, HLV, and the native source/test
toolchain answer different questions. Without an explicit boundary contract,
an integration could create competing requirements, semantic models, or gate
records and resolve drift through silent synchronization.

Lekalo does not yet have its Rust workspace. The boundary therefore has to be
portable, machine-readable, and testable before implementation details can
accidentally become the contract.

## Decision

We assign exactly one canonical owner to every artifact kind in the accepted
versioned contracts indexed by `contracts/authority-contracts.manifest.json`:

- OpenSpec owns requirements and change intent.
- AI Factory owns execution workflow and runtime lifecycle.
- Lekalo owns the adopted semantic application model and target bindings.
- HLV owns the meaning of its own validation/traceability results and diagnostic
  codes, but those results never replace requirements.
- The source/native toolchain owns implementation and native tests as direct
  runtime evidence.

Canonical Lekalo files use `lekalo/**`, outside all OpenSpec and AI Factory
trees. Non-canonical Lekalo drafts/cache/generated data use `.lekalo/**`.
Generated artifacts remain derived until an explicit reviewed adoption by the
target owner. Cross-owner synchronization is one-way into derived envelopes.
Bidirectional reconciliation is report-producing and proposal-only; it never
performs automatic canonical writes.

Generic one-way sync accepts only canonical/direct evidence as input and only
derived/cached/runtime-only output. Generated-code promotion and brownfield
model adoption are distinct strict actions that require explicit adoption,
review, and provenance. Same ownership never permits derived-to-canonical sync.

`claim` is universally reference-only and requires `substitute: false`.
Substitution has no allowed source/target pair in v1; proof-bearing transitions
remain exclusive to `promote` and `adopt`.

Root `project.yaml` is not an unconditional HLV boundary. It is HLV-owned only
when the operation classifies it as `hlv.project-contract` and carries confirmed
HLV layout context; an unrelated root `project.yaml` remains available to its
native owner.

Conflicts are resolved at the owner of the disputed fact. Consumers preserve
both facts and block the dependent operation instead of applying a global
last-writer-wins rule.

The dependency-free checker is part of the decision. It strictly validates each
action shape before policy evaluation, distinguishes malformed input (exit `1`)
from policy denial (exit `3`), and checks ownership, logical paths,
substitution, promotion/adoption, and synchronization against allowed,
forbidden, and malformed fixtures. The JSON contract is the integration
surface; the checker is a reference policy evaluator, not the future Lekalo
CLI.

Published versions are immutable and `closed-exact`. Contract identity is the
exact `{contractId, version, digest}`; the digest is a non-self-referential
SHA-256 of exact contract bytes carried by the manifest and per-version
sidecar. The checker independently trusts only reviewed manifest/contract
digests. `--contract` therefore requires an exact accepted `--authority-ref`;
a path, reformatted copy, recomputed caller digest, runtime kind extension,
local alias, or undeclared successor is not authority.

The historical `1.2.0` bytes remain accepted and unchanged. The exact `1.3.0`
bytes are preserved as a rejected/yanked candidate because owner-only path
boundaries permitted same-owner kind and writer relabeling. Corrective successor
`1.3.1` preserves the 49 stable IDs: all 26 baseline kind IDs plus 23 kinds
needed for the #120 handoff. Those kinds cover the
authority and privacy policy/schema artifacts, source maps and native identity,
raw diagnostics, context/trace/run evidence, fixtures and repository identity,
consumer projections, and export/redaction/aggregate artifacts and decisions.
Authority lifecycle remains separate from privacy classification.

In `1.3.1`, each protected boundary binds an exact set of kinds, readers, and
writers. Evaluation collects every matching boundary and chooses the
deterministic most-specific policy by literal prefix segments, literal segment
count, literal character count, segment depth, then fewer wildcards. A tied
overlap must have an identical owner/kind/reader/writer policy or contract load
fails closed. Each reference must satisfy both boundary and kind permissions,
using action-specific read/write roles.

Source maps and native symbol identities are direct source/native evidence and
only source/native may write them. A future Lekalo-derived map needs a distinct
reviewed derived kind. HLV metrics are HLV-owned direct evidence and only HLV
may write `.hlv/evidence/metrics/**`; AI Factory and AIFHub may read it or write
their own envelopes elsewhere, never inside `.hlv/**`.

Every later successor needs a new versioned file and digest, exact predecessor
reference, compatibility/migration metadata, stable-kind diff, manifest entry,
checker trust profile, and mutation evidence before review can select it. This
procedure is machine-readable in `1.3.1` and documented in
`docs/authority-contract-migration-1.2-to-1.3.md` and
`docs/authority-contract-migration-1.3.0-to-1.3.1.md`.

## Consequences

- AIFHub Extension can compose OpenSpec, HLV, and future Lekalo providers without
  taking ownership of provider artifacts.
- Traceability uses stable references and provenance rather than duplicated
  canonical bodies.
- Brownfield discovery cannot silently turn observed or generated material into
  canonical requirements or source.
- Consumers must reject unsupported contract major versions and ambiguous paths.
- Broader predecessor wildcards cannot relabel a more-specific protected path;
  ambiguity and policy ties fail before operation evaluation.
- Consumers must verify the exact accepted authority triple and reject stale
  references, undeclared kinds, local aliases, and unknown successors.
- Logical path checks reject common Windows aliases, but adapters remain
  responsible for safe physical resolution and containment before I/O.
- Fixing a conflict may require separate owner-scoped changes followed by fresh
  validation/evidence; this is deliberate and auditable.

## Rejected alternatives

### OpenSpec stores the Lekalo model

Rejected because semantic application state has a different lifecycle from a
single OpenSpec change and would violate the canonical-owner boundary.

### HLV evidence can fill missing requirements

Rejected because observed validation evidence answers whether something was
proved, not what behavior is intended.

### Last writer wins or silent two-way mirroring

Rejected because it hides lossy mappings and owner conflicts. Reconciliation
must report loss and conflict and leave canonical writes to each owner.

### Generated output becomes source automatically

Rejected because generator bugs or stale inputs could silently change the
canonical implementation. Adoption requires explicit review and provenance.
