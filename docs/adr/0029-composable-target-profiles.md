# ADR-0029: Composable target profiles

Status: accepted for issue #29.

## Context

Targets were framed around whole frameworks: an adapter spoke for a
runtime, its storage, transport, testing, analysis, and deployment in
one identity. Moving a project from Node.js to Go then meant
re-qualifying everything at once, and "supports Laravel" said nothing
about which guarantees came from PHP, which from Postgres, and which
from the transport. The M1/M2 baseline already fixed the harder halves:
adapters are separate executables behind a versioned process protocol
(ADR-0025), and the lock freezes profiles as versioned identities with
declared and resolved digests (ADR-0009) — but nothing defined what a
profile *is*, how components combine, or what guarantees survive a
switch of one axis.

Rejected alternatives:

- **Monolithic adapters with richer metadata.** The framework stays the
  unit of reuse; a Go migration re-qualifies storage and transport that
  never changed, and guarantees cannot be attributed to components.
- **Free-form YAML passed through to adapters.** Arbitrary input means
  arbitrary meaning: two tools read two profiles of the same project
  differently, nothing is digestable, and compatibility becomes a
  runtime surprise instead of a resolved fact.
- **Runtime plug-in composition in core.** Core would have to know
  frameworks; the component vocabulary would move with the product
  instead of a reviewed registry.

## Decision

1. A profile composes exactly one component per closed axis —
   `runtime`, `storage`, `transport`, `testing`, `analysis`,
   `deployment`. Components live in an embedded, closed, versioned
   registry (`dev.lekalo.target-components@1.0.0`); a declared id
   without a definition refuses resolution. Adding a component is a
   reviewed registry increment.
2. Component capability contracts are declared, not inferred: provided
   capability ids with the closed states `full`/`partial`, exact
   sibling requirements, minimum capability requirements, and
   conflicts. Runtime independence is a property of the component, so
   `postgres-sql`, `http-json`, and `container` are reusable across
   Node, PHP, and Go unchanged.
3. Profiles may extend one base within a document. The nearest
   declaration of an axis wins (explicit precedence); the composed
   capability map takes the weakest provided state, so a profile never
   claims more than every contributing component guarantees.
4. Compatibility is evaluated in one fixed order — chain, components,
   identity constraints, capability requirements, inheritance
   guarantees — and every violation carries a sorted stable reason.
   Inheritance may not silently weaken the base: a weaker resolved
   state needs an override acknowledging exactly that state, and
   removal of a base capability is never overridable.
5. Resolution produces an immutable, machine-readable snapshot per
   profile with two digest domains: canonical declared bytes
   (`profiles.source_digest`) and canonical resolved bytes
   (`profiles.digest`, `{id, version, components, capabilities}`).
   Both land in the committed lock through the existing #10 seam, so a
   monorepo may carry several profiles each with its own digest.
6. Portability is a per-axis report over two resolved snapshots: reused
   or changed per component, with the capability deltas of each change.
7. The adapter protocol transports the resolved snapshot, not YAML: on
   a 1.2.0 session an operation with a profile token may carry the
   snapshot digest plus id-sorted capability pairs; older sessions
   refuse rather than drop the resolution.
8. Six registered `target-profile.*` rules (`LEK-TPF-001..006`) close
   the taxonomy as diagnostic registry 1.14.0, additive over the
   accepted frozen 1.13.0 (the integrated chain 1.12.0 → 1.13.0 →
   1.14.0; 1.13.0 carries the five `init.*` adoption rules of #38 and
   ships byte-identical to the accepted artifact).

## Consequences

- The version registry moves to 1.2.0 (protocol family publishes the
  additive 1.2.0 request extension; frozen 1.0.0/1.1.0 documents keep
  their exact meanings), so committed contract-only locks are
  regenerated in the published world, as in #27/#28.
- The candidate product version carries 0.2.4; product, protocol,
  profile, and diagnostic registry versions remain independent lines.
- Profiles have no CLI surface in this issue: the loader seam, catalog
  supply, and `generate --locked` integration arrive with their owning
  issues; this issue ships the contract, the resolution seam, the
  portability report, the wire projection, and the gates that exercise
  them.
- Component ids are a closed vocabulary today; widening it is a
  registry change with the same review weight as a protocol version.

## References

- [Target profiles](../target-profile.md)
- [Target protocol](../target-protocol.md)
- [Lockfile](../lockfile.md)
- [Diagnostics](../diagnostics.md)
- ADR-0009 (lockfile), ADR-0025 (target protocol); consumer issues #91
  (generation execution) and the catalog supply seam.
