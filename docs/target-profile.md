# Target profiles

Issue #29 replaces monolithic framework adapters with composable target
profiles. A profile composes exactly one component per axis —
`runtime`, `storage`, `transport`, `testing`, `analysis`, and
`deployment` — so moving from Node.js to PHP or Go reuses the storage,
transport, and deployment components and changes only the runtime-bound
ones. The design decision is [ADR-0029](adr/0029-composable-target-profiles.md).

The contract (`dev.lekalo.target-profile@1.0.0`,
`lekalo/target-profile/v1.0.0`) is independent of the product release,
the Model/IR/protocol versions, the diagnostic registry, and the adapter
process protocol. It is published as
[`contracts/target-profile.schema.v1.0.0.json`](../contracts/target-profile.schema.v1.0.0.json)
and executed by `target_profile` in `lekalo-core`.

## Components and capability contracts

Every component that may appear on an axis carries exactly one
definition in the embedded, closed registry
(`dev.lekalo.target-components@1.0.0`):

- `provides` — the capability contract: stable dotted capability ids
  with the closed support states `full` and `partial`. A component that
  does not list a capability does not provide it; absence is never
  optimistically upgraded.
- `requires_components` — exact sibling components on other axes
  (`laratesto` requires runtime `php-laravel`).
- `requires_capabilities` — minimum support the composed profile must
  resolve to (`grpc-proto` requires `runtime.async` at `full`).
- `conflicts` — sibling components it can never share a profile with
  (`serverless` conflicts with storage `sqlite-file`).

A new component or a semantic change to a definition is a reviewed
registry increment, never an optimistic guess. A declared component
unknown to the registry refuses resolution
(`target-profile.component-unknown`).

## Profiles, inheritance, and explicit precedence

A profile declaration names an id and exact canonical version (a
fabricated `0.0.0` never exists), optionally one base profile it
`extends` (same document), the component per axis, and explicit
override acknowledgments. Without a base, all six axes are declared;
with a base, the declared map overrides per axis and missing axes
inherit. The nearest declaration of an axis wins — the precedence is
explicit, never source-order or version guessing. Inheritance chains
are depth-bounded; unknown bases and cycles refuse with
`target-profile.reference-invalid`.

## Resolution

Resolution is deterministic and fail-closed, in one fixed order per
profile:

1. the inheritance chain (unknown base, cycle, depth);
2. component lookup against the registry;
3. capability composition — the union of the resolved components'
   provided capabilities, each at the **weakest** provided state, so a
   profile only ever claims what every contributing component
   guarantees;
4. identity constraints — exact requirements and conflicts, every
   violation reported as a sorted stable reason token
   (`target-profile.combination-incompatible`);
5. capability requirements against the composed support
   (`target-profile.capability-unsatisfied`);
6. the inheritance guarantee check: the composed map may not weaken the
   base's map unless the profile carries an override acknowledging
   exactly the weaker resolved state
   (`target-profile.inheritance-weakening`). Removing a base capability
   entirely is never overridable — a new base profile is the honest
   spelling.

The output is one immutable, machine-readable
`ResolvedProfile` per declaration: id, version, the resolved component
per axis with its definition version, the composed capability map, the
canonical snapshot digest, the canonical declared-input digest, per-axis
provenance (`declared` or `inherited from <base>`), and the override
evidence. Two runs over the same document are byte-identical. A
monorepo document carries several profiles; each resolves independently
and takes its own digest.

## Digests in the lock and evidence

- `profiles.source_digest` — canonical JSON of the closed declaration
  (id, version, extends, sorted component map, sorted overrides).
- `profiles.digest` — canonical JSON of
  `{id, version, components, capabilities}`, the effective semantics a
  run binds to. Provenance and override evidence are outside the
  digest: identical effective semantics produce one digest whatever
  declared route produced them.

## Portability

`portability(source, target)` reports, for every axis separately,
whether the component is reused or changes and — for changed axes —
exactly which provided capabilities the change gains, loses,
strengthens, or weakens, sorted by id. It consumes two already-resolved
profiles, so its verdicts describe semantics a run actually binds to,
and it is a plain serializable value with byte-stable output.

## Adapter protocol integration

Adapters never receive profile YAML. An operation that carries a
profile token may carry, on a protocol 1.2.0 session, the resolved
snapshot digest plus the capability pairs projected by
`ResolvedProfile::wire_resolution` (see
[target protocol](target-protocol.md)). Older sessions refuse the
resolution instead of silently dropping it. The digest binds the plan
context, so planned and applied exchanges are bound to the exact
profile snapshot they were planned against.

## Failure taxonomy

Every refusal maps onto one registered rule via the shared envelope:

| Rule | Code | Refusal |
| --- | --- | --- |
| `target-profile.document-invalid` | `LEK-TPF-004` | closed shape, grammar, duplicate ids, meaningless overrides |
| `target-profile.component-unknown` | `LEK-TPF-003` | declared component not in the registry |
| `target-profile.reference-invalid` | `LEK-TPF-006` | unknown base, cycle, over-deep chain |
| `target-profile.combination-incompatible` | `LEK-TPF-002` | identity constraints violated, with reasons |
| `target-profile.capability-unsatisfied` | `LEK-TPF-001` | composed support below a requirement |
| `target-profile.inheritance-weakening` | `LEK-TPF-005` | unacknowledged weaker resolution or removal |

These rules ship in diagnostic registry 1.14.0, additive over the
accepted frozen 1.13.0 — the direct predecessor of this line, which
carries the five `init.adopt-*` adoption rules of #38. The integrated
registry chain is 1.12.0 (the #28 `target.ir-unsupported` rule) →
1.13.0 (the `init.*` rules) → 1.14.0 (the `target-profile.*` rules),
with every predecessor preserved verbatim and custody-checked.
