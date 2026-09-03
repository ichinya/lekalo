# Versioning and migrations

Status: normative for issue #9. This document defines the three contract
version families, the support policy, the migration machine, and the
compatibility preflight. The design decision is
[ADR-0008](adr/0008-versioning-and-migrations.md).

## The three contract families

Model, IR, and protocol versions evolve independently of each other and of
the product release:

| Family | Wire name | Current | Versions |
| --- | --- | --- | --- |
| user-facing Lekalo Model schema | `model` | 1.0.0 | 0.1.0 (deprecated), 1.0.0 |
| normalized Lekalo IR | `ir` | 0.1.0 | 0.1.0 |
| target/provider process protocol | `protocol` | — | unpublished |

The compiled IR is never migrated: an IR transition is always produced by
rebuilding from migrated Model source, and the receipt proves the semantic
payload is unchanged apart from the contract version.

## Strict canonical SemVer

Every contract version is spelled exactly `MAJOR.MINOR.PATCH`, optionally
with a SemVer prerelease. Build metadata, whitespace, a leading `v`,
partial components, leading zeroes, overflow, and non-ASCII digits are
rejected; the accepted spelling is exactly the canonical rendering.
Prerelease spellings parse but are supported only when that exact version
is registered.

## Support policy

Support is exact-set membership, never a numeric interval: `0.2.0` lies
between registered Model versions and is still unsupported. Lifecycle
states:

- **supported** — accepted for load, IR, migration.
- **deprecated** — still fully usable; the deprecation window is recorded
  in contract versions (`deprecated_since`, `retirement_not_before`).
- **retired** — fails closed with exit 5. Retirement is never automatic:
  it requires a reviewed registry change, cannot take effect before
  `retirement_not_before`, and names the replacement when one exists.

The registry is embedded in `lekalo-core`
(`versioning/contracts/version-registry.v1.0.0.json`), parsed once, and
fully validated before use. Violating invariants is a developer fault
rendered as `versioning.registry-invalid`, never guessed policy. Selectors
accept `model/<canonical-semver>` or a declared alias (`model/v1`);
`model/1`, `model/v1.0.0`, and repeated `v` are malformed, and a
well-formed but absent target such as `model/v2` is unsupported.

## Shared gate and exit codes

One support-policy check serves `load`, `ir`, and `migrate`: unsupported
and retired versions fail closed with exit 5
(`versioning.unsupported-version`) before any canonicalization or write;
malformed versions and selectors are exit 1
(`versioning.invalid-version`); mixed versions within one project remain
exit 1 (`versioning.mixed-versions`).

## Change classification

Every version entry and migration edge carries a written classification:
`breaking`, `additive`, or `behavioral`. Behavioral changes are never
hidden in a patch. Edges also declare formatting/comment loss (the shipped
step declares none: only the version token bytes change) and the
regeneration impact on unrelated artifacts.

## Migration machine

`lekalo migrate --to model/1.0.0 [--project DIR]` plans, then applies:

1. The accepted loader pipeline validates the project (structure,
   capability-safe reads, exact-version gate, shared support gate).
2. The unique declared migration chain is resolved from the registry.
   Zero paths is exit 5 (`versioning.no-migration-path`); more than one
   path is a registry fault; downgrades do not exist — rollback is
   recovery from a recorded backup, not a reverse edge.
3. The chain's compiled steps execute one at a time in memory. The shipped
   catalog has exactly one step, `model-0.1.0-to-1.0.0@1`.
4. Each step's output is re-validated with its target version's decoder,
   and the final project is compiled through the IR. A typed semantic diff
   compares the before/after projects excluding only contract envelope
   versions; the planned diff is proven again after the write.
5. The plan identity (`planId`) is SHA-256 over the registry version, the
   ordered edge ids, and the sorted logical paths with their before/after
   digests — never over absolute paths or timestamps — so repeated
   identical snapshots produce byte-identical plans and idempotent runs.

### The 0.1.0 to 1.0.0 step

Automatic migration is legal exactly when every existing ID already
satisfies the Model 1.0.0 grammar, reservation, and qualification rules
(see [model-migration-0.1.0-to-1.0.0.md](model-migration-0.1.0-to-1.0.0.md)).
Otherwise the planner refuses with
`versioning.migration-precondition` and zero writes; the owner authors the
semantic edits. When the preconditions hold, only the parsed
`schema_version` token bytes change in each document: comments, quoting,
CRLF/LF, multibyte content, and the final-newline state are preserved
byte for byte, and the declared loss list is empty.

### Compatibility preflight

`lekalo compatibility` prints the embedded registry. Adapters declare
compatibility through a typed manifest (`irMin`/`irMax`,
`protocolMin`/`protocolMax`, required extensions). The preflight decides
in one fixed order — manifest schema/version, current IR support, current
protocol support, inclusive IR range, inclusive protocol range, required
extensions — and no generation may start on a non-compatible verdict.
While the protocol family is unpublished, no external adapter can be
compatible (`versioning.protocol-unpublished`).

## Transaction, backups, rollback, recovery

The writer is derived from the accepted project root after an explicit
migrate operation and exposes no path escape hatch:

1. Every preflight happens before the first source write.
2. A durable journal plus an exclusive runtime lock
   (`.lekalo/cache/migrations/active.lock`, distinct from `lekalo.lock`)
   guard the transaction.
3. Immutable backups (`<planId>/before/…`) and a digest manifest are
   written and flushed before any replacement; existing material is
   reused only after exact validation and never overwritten.
4. Files are replaced in sorted logical order with same-directory atomic
   renames, flushed, and journaled.
5. After the write, the tree is reloaded through the real read path, the
   target versions and after digests are verified, and the planned
   semantic diff is proven. Only then is the transaction committed and
   the journal removed.

No portable filesystem gives one atomic transaction across files. The
guaranteed contract is: preflight failures write nothing; ordinary write
failures restore replaced files in reverse order from verified backups
(`versioning.commit-failed`); while a journal exists, every reader fails
closed with `versioning.recovery-required`; and the next explicit migrate
operation recovers — restoring every manifest file to its verified before
digest — before doing new work. External tools may observe intermediate
state only after a process or power crash.

`lekalo migrate --rollback <planId>` restores the recorded before bytes.
It refuses with zero writes when the migrated state was edited
(`versioning.rollback-conflict`), when a plan is unknown
(`versioning.rollback-failed`), or when any backup digest disagrees.
Rollback is itself journaled and verified. Repeated dry-run, apply retry,
recovery, and rollback are idempotent; a second migration to the current
target is a deterministic `changed: false` success.

Backups are local recovery material under `.lekalo/cache/`. They are
never canonical source, never published, and never promoted.

## Stability rules

- No raw source lines, absolute paths, host/user/OS data, timestamps,
  environment values, or raw operating-system errors enter any envelope.
- The provenance surface is exactly: registry version, edge ids, plan id,
  logical paths, digests, semantic changes, declared losses, transaction
  state, and backup identity. Downstream owners may wrap the receipt in
  their own attestations; migrate never writes their governed homes.
