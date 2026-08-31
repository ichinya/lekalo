# ADR-0002: Exact-custody privacy decision contract

Status: accepted privacy-contract successor for issue #120; #120 passed cold
review and was closed on 2026-08-30, and the accepted product release is
`0.0.2`

Date: 2026-08-30

## Context

Lekalo needs one fail-closed privacy/export seam before downstream issues read,
transform, retain or publish evidence. The authority dependency is now the
closed-exact `dev.lekalo.authority-matrix@1.3.1` registry with 49 stable kind
IDs. The earlier privacy `1.0.0` WIP used older authority references, a copied
taxonomy and caller-recomputable policy custody; it was reviewed but never
accepted.

Product, policy, schema, decision and authority lifecycles are independent.
The accepted M0 product is `0.0.1`; acceptance of #120 would produce product
`0.0.2`. Those product numbers must not identify policy or schema semantics.

## Decision

Adopt privacy policy `dev.lekalo.privacy-export-policy@1.0.6`, input schema
`2.5.0`, output schema `1.5.0`, decision contract
`dev.lekalo.privacy-export-decision@1.5.0`, exact classification contract
`dev.lekalo.privacy-classification-decision@1.0.0`, bound exclusively to
authority `1.3.1` and its exact digest, and exact authorizing-evidence registry
`dev.lekalo.privacy-authorizing-evidence@1.1.0`, and authorization subject
profile `dev.lekalo.privacy-authorization-subject-profile@1.0.0`.

Output schema `1.5.0` corrects the frozen `1.4.0` schema's swapped version
constraints: emitted classification refs are exactly contract `1.0.0`, while
emitted authorizing-evidence refs are exactly registry `1.1.0`. Input `2.5.0`
has the same closed shape and Authorization Subject Profile semantics as
`2.4.0`; only its exact policy/decision refs advance. Pre-evaluation startup
and custody failures use the separate closed CLI error schema `1.0.0` and are
not represented as `ExportDecisionOutput`.

Policy identity uses a deterministic, non-self-referential canonical projection
with `policyRef.digest` omitted. Exact raw policy, schema and authority bytes
are pinned by a versioned manifest. The checker contains the accepted
manifest hash; matching a caller-recomputed policy digest is never sufficient.
Missing/replaced sidecars, mutated same-ID/version content, stale references,
alternate taxonomies, aliases and extensions fail before evaluation.

The decision input and output are strict closed objects. Sensitivity labels are
orthogonal to one exact disposition. Multi-label permissions intersect and any
applicable deny dominates. Local constraints may only narrow. Broader policy
requires a reviewed versioned grant embedded in the trusted policy; this
accepted policy embeds none.

Derivation never mutates or transfers a source merely because transformation
is required. Repository contexts use opaque `repo-sha256:` refs and enforce
role/ref/relation/boundary coherence. New artifacts keep opaque `source-sha256:`
identity plus exact source custody/classification refs. Duplicate/conflicting
source identities fail closed and source-set evaluation is permutation-invariant.
Derived and synthetic provenance modes are mutually exclusive. Non-synthetic
public fixtures require permission, license and consent for every source-transfer
or storage operation. Consumer-repository-only classification requires an
originating consumer repository for every operation, not only storage.
Transform evidence and any declassification/aggregation decision are retained,
then the new artifact undergoes a new evaluation. Path values use normalized project-relative representation
or an explicit state, and measured zero is a known value rather than a missing
state.

Generic audit references never authorize. Each authorizing field has its own
closed, custody-anchored evidence kind and purpose. Evidence declares an exact
contract triple, outcome, opaque identity, verified/current state, and exact
canonical `subject-sha256:` binding. The versioned subject profile clones the
complete strict input and excludes only authorizing evidence objects plus the
non-authorizing constraint trace ref; declared set ordering makes semantic
sets permutation-invariant. Wrong shapes or purposes are malformed; missing,
stale, expired, unverified, wrong-outcome, reused-identity or context-mismatched
evidence denies. Conflict resolution, declassification and aggregation use the
same exact mechanism with distinct purposes.

The evaluator is dependency-free, offline and metadata-only. Opaque repository
token equality proves declared coherence only. Trusted #119 runtime envelopes
and integration adapters such as #89 must mint, bind and freshness-check tokens
from physically resolved repository context; missing, stale or unverified
binding fails closed before invocation or decision acceptance. #120 accepts no
caller-supplied binding attestation, caller-fabricated equality is not physical
identity proof, and #120 claims no such proof.
Those trusted envelopes/adapters likewise mint and verify authorizing evidence;
#120 checks declared custody and coherence only, not cryptographic authenticity.
#87, #119, #121 and #102 must consume this exact seam without local vocabulary
overrides.

## Consequences

- Authority/default coverage is mechanically closed-exact at 49 kinds.
- A cosmetic policy edit intentionally changes raw custody, while a semantic
  policy edit also changes the canonical policy identity.
- The rejected `1.0.0` WIP, frozen rejected `1.0.1` candidate and frozen/yanked
  accepted `1.0.2`, `1.0.3`, `1.0.4` and `1.0.5` remain auditable at exact old
  policy/manifest/schema bytes. Accepted `1.0.6` is an explicit successor
  rather than a silent replacement.
- Input/output schema versions advance because nested exact-ref validation,
  conflict-state coupling, duplicate-set closure and effective current refs
  change wire bytes.
- Win32 compatibility-normalized DOS/console aliases are lexically denied;
  physical reparse/link/8.3/TOCTOU checks remain downstream.
- Fully offline local use remains possible, including a withheld resource path,
  without granting repository/network export rights.
- `transform-required` is operationally safe: it cannot be interpreted as
  permission to transfer the evaluated source.

## Rejected alternatives

### Use product `0.0.2` as policy version

Rejected because product acceptance and policy/schema semantics have different
lifecycles.

### Trust a digest supplied or recomputed by the caller

Rejected because an attacker could weaken same-ID/version policy content and
recompute its digest. A separately pinned manifest is required.

### Copy authority kind names into privacy code and add extensions

Rejected because local taxonomies drift and can shadow ownership. Defaults must
match the exact authority registry and order.

### Inspect or redact payloads in #120

Rejected as a scope and evidence-boundary violation. Runtime scanning,
redaction, transfer, propagation, history, publication, private-repository
access and physical containment are downstream responsibilities.
