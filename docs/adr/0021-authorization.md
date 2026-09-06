# ADR-0021: Authorization actors, scopes, and policy contracts

Date: 2026-09-06
Status: accepted for issue #25

Custody: this issue carries the **prospective product candidate 0.1.28** in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock` including the regenerated committed golden lock and its
digests, the `--version` behavior and its pinning tests, `README.md`,
`docs/cli.md`); issue #24 published product 0.1.27 (annotated tag
`v0.1.27` on `ef7680d`); issue #18 published product 0.1.26 (annotated tag
`v0.1.26` on `3710179`). The authorization contract version
(`lekalo/authorization/v1.0.0`, identity `dev.lekalo.authorization@1.0.0`)
and the strict review profile identity
(`dev.lekalo.authorization-profile@1.0.0`) are independent of the product
release, of the Model/IR/graph/effect/protocol contract versions, of the
diagnostic registry, and of each other by design.

## Context

Issues #5–#8 shipped the Model, the stable semantic ids, the loader, and
the typed IR; #11–#23 shipped diagnostics, semantic validation, the
graphs, scenarios, and the diff. Everything downstream of a model still
treated access control as an implementation detail: Model policies carry
only `applies_to` plus a binary `allow`/`deny` decision, no actor, no
scope, no fields, no composition. Issue #25 owns that surface: actor
types and authentication requirements, tenant/workspace/user scope
bindings, policy symbols, capabilities/roles/ownership conditions,
field-level read/write constraints, protected-effect coverage, adapter
mapping evidence, and Scenario-IR `allowed`/`denied` assertion
semantics.

The research briefs (run run_088695f63032: briefs msg_95d5e0604fdb,
msg_6fc0f504fb47, the owner-boundary acceptance msg_bed55d938ee0, the
integration brief msg_5ecb6f645ff2, and worker_done msg_920e01e3639d)
recorded the owner decisions this ADR adopts.

## Decision

### 1. Independent closed contract; Model stays immutable

Authorization publishes its own wire contract,
[`contracts/authorization.schema.v1.0.0.json`](../../contracts/authorization.schema.v1.0.0.json)
(discriminator `lekalo/authorization/v1.0.0`, identity
`dev.lekalo.authorization@1.0.0`), independent of every other contract
family. The accepted Model v0.1.0 and the current Model 1.0.0 stay
untouched: authorization metadata never enters `model.schema.*.json` or
the per-module `policies.yaml`. `model_ref` pins the Model schema
version plus the canonical IR payload digest, so a document is
provably written against one accepted compilation.

### 2. Canonical source path: the accepted #4 successor

Per-project authorization semantics need a canonical Lekalo source, not
a cache or evidence artifact. The accepted #4 structure was closed to
`project.yaml`, `modules/**`, and `targets/**`, so this issue carries
the reviewed #4 successor widening: exactly one root-level
`lekalo/authorization.yaml` per project, owned by Lekalo, committed,
subject to the #4 path-safety rules. It is never per-module
(`modules/*/authorization.yaml` is unexpected), never in
`.lekalo/consumer/**` (derived), never in target code, and never inside
`policies.yaml` (Model rejects it). The closed root-entry sets in
`project_fs.rs` and `scripts/check-structure.mjs` widened additively;
no other path changed.

### 3. Closed actors, scopes, and conditions; no executable anything

The actor vocabulary is closed — `public`, `identity.user`,
`identity.service`, `system.job`, `internal` — with fixed
authentication requirements; `system.job` is the only actor with a
named `job` and never an ambient bypass. Scope is the closed
tenant/workspace/user set with at most one typed, explicit binding per
dimension; binding relations are equality/membership/ownership, and
policy literals are not binding sources. Capabilities and roles are
semantic symbols with explicit acyclic `implies` graphs. Conditions are
typed operands plus `equals`/`not_equals`/`in` and fixed-depth
`all`/`any`/`not` (depth 4, 64 nodes) — no calls, scripts, arithmetic,
regex, time, or target syntax, keeping the language far from #66's
expression surface.

### 4. Deny-by-default evaluation; deny dominates

The evaluator is pure: it takes an explicit typed principal, request,
resource view, and runtime scope, and never reads files, calls identity
providers, invokes middleware, runs adapters, or touches networks or
secrets. A satisfied deny decides first; otherwise a satisfied allow is
required; everything else is denied. Field requests are granted only by
a matching allow's explicit field set — unknown or uncovered fields
deny, so private-field exposure is checked at evaluation even before
#87 classification exists. Scenario assertions (`#23`'s `authorization`
kind) are satisfied exactly when the asserted outcome equals the
evaluated outcome of the named policy for the request context; #23
keeps parsing and execution.

### 5. Strict review blocks; validation stays separate

`lekalo validate --strict` runs the review after #12's semantic
validation. Reference integrity is invalid (exit 1) in every profile —
a dangling reference is malformed contract data, not an access
decision. Strict then blocks (exit 3, denied) on: protected effects
without allow coverage (including a missing document where protected
effects exist), a stale `model_ref` digest, and adapter mapping
evidence below `full`. The default profile is advisory and never blocks;
strict is the only built-in that gates, and optional behavior cannot
silently weaken it. #12 keeps semantic validity and business
preconditions; #25 only decides access.

### 6. Adapter mapping evidence, not adapters

The document's optional `mappings` section carries the neutral evidence
record `{target, authorization_contract_ref, semantic_policy_ref,
mapping_state, evidence_ref, observed_revision, reason_refs}` — no
framework middleware names, no provider details, no execution. In
strict, only `full` passes; `partial`/`unsupported`/`unknown` block.
The process protocol (#27), capability discovery (#28), profiles (#29),
and native conformance (#31) consume this record later; #25 executes
nothing.

### 7. Graph, impact, and diff facts; renames via #6

#25 contributes typed facts — sorted `authorizes` tuples for the #13
graph (whose `authorizes` relation and policy nodes pre-exist), impact
item ids for #16, and a closed change classification for diff-style
consumers: policy add/remove, actor broadening/narrowing, scope
weakening/tightening, capability/role/ownership changes, field
permission changes, deny add/remove, `error_ref` changes, and
mapping-state changes. New allow surface, public actors, weaker scope,
and deny removal are security-breaking; mandatory authentication and
narrower writes are behavioral; cosmetic ordering is empty. Policy,
actor, capability, and role ids follow #6 grammar and rename/tombstone
rules, so retired ids are never reused.

### 8. Diagnostics through the #11 seam: registry minor 1.5.0 → 1.6.0

The registry takes its next wire-shape-preserving minor increment to
[`diagnostic-registry.v1.6.0.json`](../../contracts/diagnostic-registry.v1.6.0.json)
with twelve `authorization.*` rules (LEK-AUTH-001..012, category
`security`): `document-invalid`, `actor-invalid`, `scope-invalid`,
`policy-invalid`, `ref-unresolved`, `error-ref-unresolved`,
`composition-invalid`, `field-uncovered`, `effect-unprotected`,
`mapping-stale`, `limit-exceeded`, and `profile-invalid`, assembled
through the shared registry-backed constructor with bounded fixed
tokens only — no attacker-controlled echo. `field-uncovered` is
registered without a declared-IR emitter in v1 (declared effects are
entity-scoped; exact-field evidence arrives with the adapter owners),
exactly like `diff.change-limit`. The validation profiles pin
`diagnostic_registry_version` 1.6.0 and move together with the embed.

### 9. Limits and determinism (v1, owner-approved)

256 policies, symbol declarations, compositions, and mappings per
document; 256 referenced operations per policy; 64 implications and
ownership predicates; condition depth 4 and 64 nodes; composition depth
8; 16 reason refs; 1 MiB source bytes. Every bound rejects at N+1 with
no partial result, before allocation where possible. Canonical bytes
are compact UTF-8 JSON with byte-sorted keys; collections are stored
sorted and duplicate-free so source order is never semantic; digests
are SHA-256 over the canonical bytes.

### 10. Owner decisions adopted

- contract family `dev.lekalo.authorization@1.0.0` and profile identity
  `dev.lekalo.authorization-profile@1.0.0` (independent families);
- canonical path `lekalo/authorization.yaml`, project-wide (one
  document; not per-module);
- `error_ref` is required on every policy and grammar-validated; with
  no accepted #62 error surface yet, #25 validates reference grammar
  and leaves reachability to the #62-informed successor;
- policy `applies_to` references accepted operations; the coarse Model
  `policies.yaml` stays untouched and composes downstream (a Model deny
  informs generation, an authorization deny decides access);
- `internal` means a trusted runtime principal at a declared boundary —
  distinct from `identity.service` (a workload identity) and
  `system.job` (a named scheduled job); global/system scope requires an
  explicit named service actor grant and is never inferred;
- partial/unsupported/unknown mapping evidence cannot pass strict;
  missing evidence marks impact incomplete in #16's dimension;
- exact bounds as in Decision 9; diff labels as in Decision 7.

## Consequences

- Commands with tenant writes cannot pass strict without an actor, a
  tenant scope binding, and a covering policy; denied paths carry a
  stable error contract; scenarios assert `allowed`/`denied` for both
  authorized and unauthorized actors; adapters report partial or
  unsupported policy semantics instead of optimistic passes; policy
  changes surface in the impact and diff facts; authorization metadata
  contains no runtime secrets. These close every acceptance criterion
  of issue #25 that is reachable without the downstream owners.
- #13/#16/#18/#23 and #27/#28/#29/#31 consume the typed facts and
  evidence records through their own successors; #87 private-field
  classification plugs into the field sets; #62 plugs into `error_ref`
  reachability; #66's expression surface stays untouched.
- The pinned goldens under `tests/fixtures/authorization/golden` are
  gate-checked with exact Ajv 8.17.1 on Node 18 and 24 by
  `scripts/test-authorization-contracts.mjs`, byte-compared by the Rust
  fixture test, and the evaluation vector matrix drives the evaluator
  through the required planner matrix.

## References

- [docs/authorization.md](../authorization.md) — the authorization
  surface, vocabulary, review outcomes, and guarantees.
- [ADR-0003](0003-canonical-structure-and-path-safety.md) — the closed
  structure this successor widens by exactly one root entry.
- [ADR-0010](0010-diagnostics.md) — the diagnostic contract and the
  registry the `authorization.*` family joins.
- [ADR-0011](0011-semantic-validation.md) — the profile seam strict
  review rides on.
- [ADR-0013](0013-effect-graph.md) — the effect declarations coverage
  consumes.
- [ADR-0019](0019-semantic-diff.md) — the classification and evidence
  conventions the change facts follow.
