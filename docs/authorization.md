# Lekalo authorization

Issue #25 makes authorization an explicit part of the semantic model: an
operation cannot be generated or implemented without a declared actor,
scope, and policy. The contract is an independent version family —
discriminator `lekalo/authorization/v1.0.0`, identity
`dev.lekalo.authorization@1.0.0` — independent of the product release,
the Model/IR/graph/effect/protocol contract versions, the diagnostic
registry, and the strict review profile (`dev.lekalo.authorization-profile@1.0.0`)
by design.

- [`contracts/authorization.schema.v1.0.0.json`](../contracts/authorization.schema.v1.0.0.json) — the closed wire schema,
- [`docs/adr/0021-authorization.md`](adr/0021-authorization.md) — the owner decisions,

## Canonical source

One project-level source per project, exactly
`lekalo/authorization.yaml` (the accepted #4 successor path; never
per-module, never inside `.lekalo/**`, never inside target code). The
document is closed: `schema_version`, `identity`, `model_ref`,
`profile_ref`, `policies`, and the optional `capabilities`, `roles`,
`compositions`, and `mappings` sections. Every unknown field, version,
ref, or id is invalid. The source carries semantic ids, declared field
names, and bounded symbolic values only — no runtime principals, tokens,
claims, provider names, middleware names, or secrets.

`model_ref` pins the exact Model schema version and the canonical IR
payload digest the policies were written against; strict review refuses
a stale pin.

## Closed vocabulary

- **Actors.** `public` (no authentication; only explicitly public
  policies/effects), `identity.user` (authentication required),
  `identity.service` (workload/service identity required), `system.job`
  (service/scheduler identity plus a named `job`; never an ambient
  global bypass), `internal` (trusted runtime identity at a declared
  boundary; never a public bypass). Unknown actor/auth combinations
  reject. Every non-public actor requires an explicit authenticated
  principal at evaluation; missing authentication is denied.
- **Scope.** The closed tenant/workspace/user dimensions. Each protected
  operation declares at most one unambiguous binding per dimension from
  typed sources (`actor.id`, `actor.tenant_id`, `actor.workspace_id`,
  `input.<field>`, `resource.<field>`, `scope.<dimension>`) with an
  explicit equality/membership/ownership relation against a typed
  anchor. Policy literals are not sources; cross-tenant and mismatched
  workspace/user are denied.
- **Capabilities and roles.** Semantic symbols with explicit, acyclic
  `implies` graphs. Inheritance is never hidden in target roles.
- **Ownership.** An explicit resource-field to actor-field equality over
  declared IR fields (for example `resource.owner_id` equals
  `actor.id`).
- **Conditions.** A finite, non-executable language: typed operands,
  `equals`/`not_equals`/`in`, and fixed-depth `all`/`any`/`not`
  composition (depth ≤ 4, ≤ 64 nodes). No calls, scripts, loops,
  arithmetic, regex, filesystem/network/time/random access, target
  syntax, or user-defined functions.
- **Fields.** Sorted, unique read/write permission sets; no wildcards.
  Unknown or dynamic fields are denied, never assumed safe.

## Policies and composition

A policy is the closed combination of actor, decision
(`allow`/`deny`), scope bindings, required capabilities/roles, ownership
predicates, typed conditions, field permissions, an optional composition
reference, and a required `error_ref` to an existing error-contract
symbol (the #62 identity surface; #25 validates grammar and reference
only — status codes, payload schemas, and transport projection stay with
their owners). Composition is closed `all_of`/`any_of` over policy and
composition references, acyclic and deterministic: a matching deny
dominates, a matching allow is required, and no match, missing policy,
unresolved reference, or ambiguous composition is deny/block in strict.
Duplicate declarations are rejected, never merged.

## Strict review, deny by default

`lekalo validate --strict --json` runs the authorization review after
semantic validation:

- reference integrity (unknown operations, fields, capabilities, roles,
  compositions, mappings; implication/composition cycles) is **invalid**
  (exit 1) in both profiles;
- an unprotected protected effect, a stale `model_ref`, or adapter
  mapping evidence below `full` is **denied** (exit 3) under strict;
- the default profile is advisory: it never silently weakens strict, it
  simply does not block.

A project with protected effects and no authorization document at all
blocks under strict (`authorization.effect-unprotected`). Separation
from validation and business preconditions is preserved: #12 keeps
semantic validity, #25 only decides access.

## Protected effects

Declared effect coverage consumes the #14 effect declarations (it never
detects effects): entity create/update/delete, field writes, event/job
enqueue, external/destructive calls, cache writes/invalidations,
publication/output exposure, and audit writes require an allow
declaration; query reads and public outputs require field-read grants.
Effect detection and sensitive classification stay with #14/#26/#87/#119.

## Adapter mapping evidence

The neutral `mappings` section carries exactly `{target,
authorization_contract_ref, semantic_policy_ref, mapping_state
(full|partial|unsupported|unknown), evidence_ref, observed_revision,
reason_refs}`. Names, middleware/guard wiring, execution, and transport
projection stay with the adapter owners (#27/#28/#29). In strict,
partial/unsupported/unknown evidence cannot pass; an adapter reports
unsupported rather than optimistically passing.

## Graph, impact, and diff contributions

#25 supplies typed facts, not seams: sorted `authorizes` facts (policy ×
operation × actor × decision × error_ref) for the #13 graph, impact item
ids for #16, and a closed change classification (`policy-added`,
`actor-broadened`, `scope-weakened`, `deny-removed`, `field-permission-changed`,
`mapping-state-changed`, …) with dominant severity (`security-breaking`,
`behavioral`, `cosmetic`) for #18-style consumers. New allow surface,
public actors, weaker scope, and deny removal are security-breaking;
mandatory authentication and narrower writes are behavioral.

## Deterministic canonicalization

Canonical bytes are compact UTF-8 JSON with byte-sorted keys; every
repeated collection is stored sorted and duplicate-free (duplicates are
rejected, not merged), so the canonical form is independent of source
order and byte-identical across runs. Digests are SHA-256 over the
canonical bytes. Bounds are owner-approved and tested at N and N+1: 256
policies/symbol declarations/compositions/mappings per document, 256
referenced operations per policy, 64 implication targets, condition
depth 4 with 64 nodes, composition depth 8, and a 1 MiB source cap.

## Fixtures and gates

The hermetic fixtures live under
[`tests/fixtures/authorization`](../tests/fixtures/authorization): the
multi-tenant planner project plus `uncovered`, `mapping-partial`,
`stale`, `malformed`, and `noauth` variants, the golden canonical
documents under `golden/`, and the evaluation vector matrix in
`vectors.json` (same tenant/owner allowed; wrong user, cross-tenant,
missing auth, missing scope, missing capability, ownership mismatch,
private-field read, wrong job, deny-overrides-allow, and public
exposure denied).

The Node release gate is
`node scripts/test-authorization-contracts.mjs` (exact Ajv 8.17.1
against the schema plus canonical-order and closed-vocabulary
invariants), run in CI on Node 18 and 24. The Rust core tests
byte-compare the goldens, drive the vector matrix through the evaluator,
and pin every review outcome; the CLI tests pin the denied (exit 3) and
invalid (exit 1) envelopes through the alias-free temp spelling.
