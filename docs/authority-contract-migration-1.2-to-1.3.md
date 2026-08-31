# Authority contract migration: 1.2.0 to the 1.3 line

Status: `1.3.0` rejected/yanked; use reviewed corrective successor `1.3.1`.

## Exact identities

| Lifecycle | Contract ID | Version | Exact-byte SHA-256 |
|---|---|---:|---|
| Accepted immutable baseline | `dev.lekalo.authority-matrix` | `1.2.0` | `3446ce25ce33397f8c49426144a4c4dbf8c8ea23e4c759ceca70557606ffeda2` |
| Rejected/yanked candidate, preserved only for custody | `dev.lekalo.authority-matrix` | `1.3.0` | `50a4b9c8533644bd61841e695fc042e4ac52307952dcccb37bf6f2d3ab3a3211` |
| Accepted corrective successor | `dev.lekalo.authority-matrix` | `1.3.1` | `5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3` |

The digest covers exact file bytes and is intentionally not stored inside the
contract it identifies. The non-self-referential manifest and versioned
sidecars carry it. The checker has independent trust anchors for the manifest
and lifecycle profiles, so editing bytes and recomputing a caller-controlled
digest cannot admit a change.

The historical `contracts/authority-matrix.v1.json` remains an exact-byte alias
of `1.2.0`. A custom path is authoritative only with an exact accepted triple.
The preserved `1.3.0` bytes and digest identify one rejected candidate; no
second accepted digest exists for that version.

## Compatibility and custody

The 1.3 registry is additive relative to `1.2.0`: all 26 predecessor kind IDs
remain stable and 23 kinds are added for the #120 handoff. Authority lifecycle
remains separate from privacy sensitivity and disposition.

The first candidate, `1.3.0`, is not accepted because its protected boundaries
were owner-only. A caller could relabel a protected path with a broader kind
having the same owner or broader writers. It is therefore yanked, even though
its exact bytes remain immutable and verifiable for audit.

The accepted `1.3.1` successor preserves the 49 stable IDs and former operation
guarantees while strengthening boundary custody. Each boundary binds an exact
kind set plus readers and writers; all matching boundaries are evaluated and
the deterministic most-specific policy wins. Three direct-evidence writer sets
are narrowed: `metrics.evaluation-evidence` to HLV only, and
`native.symbol-identity`/`native.source-map` to source/native only. These are
documented corrective changes, not silent mutations of `1.3.0`.

## Consumer migration

A consumer pinned to exact `1.2.0` remains valid for its original 26 kinds and
operation semantics. It must reject the 23 unknown 1.3-line IDs.

A consumer requiring the expanded registry must:

1. Use the full exact `1.3.1` triple, never `1.3.0`.
2. Resolve its versioned path through `authority-contracts.manifest.json`.
3. Verify manifest trust, sidecar bytes, exact-byte digest, `contractId`, and
   `version` before policy evaluation.
4. Use stable IDs directly; do not create aliases or runtime extensions.
5. Enforce the contract's exact kind-bound boundary and specificity semantics.
6. Reject missing/unknown kinds, stale or yanked references, byte mutations,
   undeclared successors, and local policy weakening with exit `1`.

A stale `1.2.0` reference paired with `1.3.1` bytes fails identity. The exact
`1.3.0` reference fails lifecycle admission. A reformatted or semantically
mutated `1.3.1`, even with a recomputed supplied digest, is unsupported.

## Reviewed successor admission procedure

For any version after `1.3.1`:

1. Create a new explicitly versioned file; never edit published or yanked
   predecessor bytes.
2. Record the exact predecessor triple and lifecycle.
3. Declare compatibility, kind additions/changes/removals, changed fields, and
   the consumer migration action.
4. Give every kind one owner and non-empty paths/readers/writers plus one
   authority lifecycle classification.
5. Preserve stable IDs and semantics or classify and review the incompatibility.
6. Publish an exact-byte SHA-256 sidecar and manifest lifecycle entry.
7. Add a closed-exact checker profile and conformance/mutation evidence for
   identity, registry, boundaries, ambiguity, stale refs, and operation rules.
8. Change `currentAuthorityRef` only after successor review passes.

The manifest is a closed accepted-version index, not an extension mechanism.
Unknown entries, versions, and recomputed digests remain invalid until reviewed
code and evidence deliberately admit them.

## #120 handoff

#120 must bind to:

```text
dev.lekalo.authority-matrix@1.3.1@sha256:5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3
```

It should remove its local kind list and reference the exact 49 stable IDs.
Existing #120 work that still names `1.3.0` is expected to fail closed until it
is migrated in that separate ticket. Its policy/schema version and the Lekalo
product version remain separate; this authority migration does not edit #120
or advance the product from the `0.0.1` candidate.

The corrective details are in
[Authority contract migration 1.3.0 to 1.3.1](authority-contract-migration-1.3.0-to-1.3.1.md).
