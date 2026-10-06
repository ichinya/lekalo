# Semantic change API

Issue #73 replaces direct YAML/code edits with semantic operations: a
closed request surface, validated candidates, bounded plans, and one
durable, idempotent transaction writer. The implementation is a pure
preparation core in `lekalo-core` (`crate::change`) plus a separately
invoked CLI handoff (`lekalo change`). A future MCP tool consumes the
same request, plan, and receipt types; it never receives a raw
filesystem writer or a policy bypass.

- Contracts (all at product 0.8.0):
  `contracts/change-request.schema.v0.8.0.json`,
  `contracts/change-catalog.v0.8.0.json` (+ schema),
  `contracts/change-policy.schema.v0.8.0.json`,
  `contracts/change-plan.schema.v0.8.0.json`,
  `contracts/change-receipt.schema.v0.8.0.json`,
  `contracts/change-history.schema.v0.8.0.json`
- Wire discriminators: `lekalo/change-{request,catalog,policy,plan,
  receipt,history}/v0.8.0`; identities `dev.lekalo.change-*@0.8.0`
- Diagnostics: the `change.*` rule family (`LEK-CHANGE-001..012`)
  through the #11 registry union successor 0.8.0; predecessor
  producers keep their exact pins
- Canonical homes: `lekalo/change-history.json` (durable audit and
  idempotency ledger, admitted by the structure checker; never model
  input) and `.lekalo/cache/change/<transaction-id>/` (temporary
  journal/backup/recovery material)

## Request surface

The request is the single operation declaration surface shared by CLI
sugar, JSON request files, and future tools. Decoding is closed and
bounded: duplicate JSON keys, unknown members, unknown operations,
over-limit batches, and malformed digests fail closed
(`change.request-invalid`, invalid 1). A cataloged operation without a
sealed executor refuses explicitly (`change.operation-unsupported`,
unsupported 4); the first increments implement `entity-field:add`,
`entity-field:remove`, `entity-field:rename`, and `enum-case:add`.

```sh
lekalo change entity-field:add --project sample --entity tasks.task \
  --name priority --type tasks.priority --dry-run --json
lekalo change request --project sample --request change-request.json \
  --apply --idempotency-key task-priority-v1 --json
lekalo change receipt --project sample --idempotency-key task-priority-v1 --json
lekalo change recover --project sample --transaction change-ab12cd34-1 --json
```

`--apply` requires `--idempotency-key`; without it the invocation is a
read-only preview (`--dry-run` is an explicit synonym and conflicts
with `--apply`). Preview never writes: no history, no cache, no locks,
no source bytes. `--allow-breaking` records explicit acceptance of the
reported compatibility classes and never overrides malformed requests,
invalid Models, path-safety refusals, ownership denials, or stale
preconditions. `--expect-model`/`--expect-plan` pin exact
`sha256:<64hex>` digests.

## Preparation pipeline

`change::prepare` is pure and deterministic: it resolves every
operation against typed semantic identities (never text search), builds
one candidate project with typed tree edits, re-decodes the planned
byte edits, asserts compiled-candidate equality, revalidates across
project boundaries, computes the owner-produced semantic diff, the
compatibility classification, the impact projection from diff seeds on
the matching graphs, and the plan-only generation projection, and pins
the complete input set by digest. Invalid candidates always refuse; a
valid breaking candidate requires an allowed policy class or
`--allow-breaking` (`change.compatibility-denied`, denied 3).

The sealed plan carries both the semantic diff and the exact bounded
file diff (byte-identity outside declared edit ranges, refusal instead
of guessing for comment-adjacent or ambiguous layouts,
`change.source-preservation-unavailable`, denied 3). The request digest
pins the canonical re-serialization of the request, so pretty file JSON
and compact CLI-sugar bytes of one request carry the same identity.

## Application, idempotency, recovery

`change::apply_prepared` is the only writer. Under a project-scoped
writer lock it rechecks the durable ledger first (a repeated key with
the same canonical request replays the original committed receipt
without another mutation; a different intent conflicts,
`change.idempotency-conflict`, denied 3), revalidates every before
digest, stages a durable journal plus verified backups, applies
same-directory replacements, verifies every after digest, finalizes the
canonical history in the same transaction, and removes the journal as
the commit point. Ordinary failure restores all originals; a crash
leaves a recoverable journal that blocks every project reader until
explicit `lekalo change recover` succeeds
(`change.recovery-required`, denied 3).

The raw idempotency key never enters any wire surface: only its
`sha256:` digest is stored. Receipt states (`committed`, `replayed`,
`no-op`, `rejected`, `rolled-back`, `recovery-required`) are Change
receipt states, never envelope statuses.

## Non-goals

No automatic generated-target writes (run `lekalo generate --locked`
separately), no arbitrary textual patching, no semantic module-ID
renames, no distributed transactions, and no MCP server shipping claim
in this increment. The remaining cataloged operations land in later
increments and refuse with the registered unsupported outcome until
then.
