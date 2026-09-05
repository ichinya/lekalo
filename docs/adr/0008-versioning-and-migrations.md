# ADR-0008: Versioning and migrations for Model, IR, and protocol

Date: 2026-09-04
Status: accepted for issue #9

## Context

Issues #6/#7/#8 shipped two exact Model contract versions, a loader with
an exact-version dispatch, and a typed IR. Three contracts now evolve on
independent schedules: the user-facing Model schema, the compiled IR, and
the future target/provider protocol. Nothing governed their versioning,
support policy, or how a project moves between Model versions. Issue #9
owns that surface without revising any published #3–#8 behavior.

Issue #9 published product 0.1.7 at `f0b3784`; issue #10 published
product 0.1.8 at `8ddbbf0`; issue #11 carried prospective product 0.1.9;
issue #12 published product 0.1.10 at `fdfbcb5`; issue #13 now carries prospective product 0.1.11.
Issue #8 published product 0.1.6 at `5580b83`. Product versions, Model schema versions, the IR
contract version, and the protocol version are independent by design.

## Decision

### Three sealed families over strict canonical SemVer

`ContractVersion<F>` wraps the `semver` crate with private fields and a
sealed family marker (`model`, `ir`, `protocol`, plus `registry` for the
versioning artifacts). Parsing accepts exactly `MAJOR.MINOR.PATCH` with an
optional prerelease; build metadata is rejected because SemVer precedence
ignores it and two spellings for one version would break deterministic
registry bytes. A round-trip check closes every residual spelling gap.
Rejected: `VersionReq`-style intervals — support is exact-set membership,
so `0.2.0` (between registered versions) is unsupported.

### One embedded registry, validated at construction

The registry lives as a canonical JSON artifact embedded in `lekalo-core`
(`include_bytes!`), parsed once per process, and fully validated before
any use: strict ascending versions, one current per published family,
lifecycle thresholds that ascend, alias grammar (`v<major>`), edge
endpoints registered and strictly ascending, and at most one simple
migration path per ordered pair. A broken registry is a developer fault:
`versioning.registry-invalid`, never guessed policy. The registry artifact
and the compiled step catalog are bound one-to-one; an edge without an
implementation or an implementation without an edge is invalid, and so is
any family that declares edges without a catalog.

Note: the brief anticipated an IR 1.0.0; the accepted #8 shipped
`dev.lekalo.ir@0.1.0`. The registry records the accepted reality, and the
Node gate cross-checks the artifact against the compiled constant so the
two cannot drift.

### Lifecycle: supported, deprecated, retired

Deprecated versions stay fully usable with a deprecation window expressed
in contract versions, not wall clocks. Retirement requires a reviewed
registry change, cannot take effect before `retirement_not_before`, and
fails closed with exit 5. One shared support-policy check serves `load`,
`ir`, and `migrate`; there is no second IR-only gate.

### Explicit one-step migrations, never collapsed chains

Migration steps are sealed, Model-only, and catalog-registered. Planning
resolves the unique declared chain and executes each step in order,
re-validating every intermediate output with its target version's decoder;
a chain is never collapsed into one rewrite. The shipped catalog has
exactly `model-0.1.0-to-1.0.0@1`, which reuses the #6 preconditions: every
ID must already be legal under 1.0.0, or the planner refuses with
`versioning.migration-precondition` and zero writes — tooling never
case-folds, translates, truncates, or invents rename history.

### Semantic diff and deterministic plan identity

Before and after projects compile through the accepted #8 IR; a typed
differ compares project/module/definition payloads excluding only contract
envelope versions and spans. `planId` is SHA-256 over the registry
version, ordered edge ids, and sorted logical paths with before/after
digests — never absolute paths or timestamps — making plans byte-stable
and runs idempotent.

### Capability-safe transaction with honest guarantees

The writer derives from the accepted project root, rejects symlink/
junction/reparse components on every operation, and pins the original
bytes as the compare-and-swap base. Immutable backups plus a digest
manifest precede the first replacement; replacements use same-directory
atomic renames with a durable journal and exclusive runtime lock. Because
no portable filesystem offers multi-file atomicity, the documented
guarantee is: preflight failures write nothing, write failures restore
from verified backups, readers fail closed (`versioning.recovery-required`)
while a journal exists, and the next explicit migrate recovers before new
work. Backups avoid spelling a `lekalo` directory inside `.lekalo` so the
accepted #4 runtime scan keeps denying nested roots.

Rollback is recovery from recorded backups, not a reverse migration edge.
It refuses with zero writes on any user edit (`rollback-conflict`) or
backup mismatch, and is itself journaled and verified.

### Consequences

- Provenance is limited to digests, logical paths, edge ids, plan ids, and
  declared losses; no absolute paths, host data, or timestamps.
- The protocol family ships unpublished; the preflight refuses every
  external adapter with `versioning.protocol-unpublished` until a
  protocol version exists, and required IR extensions can never be
  satisfied by the extension-free #8 IR.
- Adding a Model version is a breaking registry change: it must extend the
  finite `ModelVersion` dispatch, the exhaustive conversion, the registry
  artifact, and (if migratable) the compiled catalog together.
- #10 still owns the lockfile and adapter digests; #27 owns adapter
  processes and manifest wiring; #11 owns the rich diagnostic schema.
