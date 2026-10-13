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
unsupported 4); the implemented increments are `entity-field:add`,
`entity-field:remove`, `entity-field:rename`, `enum-case:add`,
`enum-case:change`, `definition:add`, `command-effect:add`,
`command-effect:remove`, `operation-input:set`, and
`operation-output:set` — the remaining cataloged operations are staged
refusals pending their sealed executors. Runtime decoding mirrors the
published request schema exactly: an
optional member a schema variant admits (for example `symbol:move`'s
`toName`) decodes and reaches the registered unsupported outcome —
never an unknown-member rejection. Declared field defaults and enum
wire values are decoded and refused explicitly at every
field-consuming operation (`change.operation-unsupported`), never
silently dropped from a committed declaration.

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
`sha256:<64hex>` digests. Every CLI JSON ingress validates before it
transforms: the shared `--expect-model` flag merges into a request file
through the same duplicate-key-refusing parser as the request surface,
so a malformed raw request stays malformed on the mutation path — the
flag can never launder it into an accepted mutation — and the
`--default-json`/`--value-json` literal flags parse strictly too.

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
`change.source-preservation-unavailable`, denied 3). Setting an
UNCHANGED signature plans no byte edit at all — comments, quoting, and
formatting stay verbatim; a changed signature performs proven bounded
per-item edits only, and member reorders refuse explicitly instead of
reformatting the block. A field rename rewrites the entity's own
governed `identity` references with it. `definition:add` can create a
module's first definition of a kind: the kind-home document is
synthesized whole, committed as a creation inside the same transaction,
and rolled back by removing exactly the bytes the transaction wrote.
The impact projection reports every reverse-reachable consumer of the
typed diff seeds (the wire cap equals the closure bound); a larger
closure marks the projection incomplete instead of dropping known
consumers. The generation projection validates its evidence through
the owners: the project lock is parsed by the lockfile owner's strict
canonical parser, each target document's closed identity is decoded
with the duplicate-key-refusing parser, and a target is planned only
when the lock actually locks it — invalid or absent locks, undecodable
or unlocked targets, and the enumeration cap propagate as
`complete: false`, never a complete-looking inventory over invalid
evidence. The request digest
pins the request's canonical JSON: pretty and compact presentations of
the same operation batch canonicalize to one digest (the digest pins
the intent, not its bytes), so CLI sugar and an equivalent request
file produce the identical plan and idempotency binding.

## Application, idempotency, recovery

`change::apply_prepared` is the only writer. Under a project-scoped
writer lock it rechecks the durable ledger first (a repeated key with
the same canonical request replays the original committed receipt
without another mutation; a different intent conflicts,
`change.idempotency-conflict`, denied 3), revalidates every before
digest plus the sealed generation evidence (every declared target
document and the project lock — additions, removals, and edits after
preparation all refuse), stages a durable journal plus verified
backups, applies same-directory replacements, verifies every after
digest, finalizes the canonical history in the same transaction, and
removes the journal as the commit point. Ordinary failure restores all
originals; a crash leaves a recoverable journal that blocks every
project reader until explicit `lekalo change recover` succeeds
(`change.recovery-required`, denied 3).

Journal provenance (round 5): a journal is recovery material only when
a durable pending record — written under `.lekalo/pending-transactions.json`
BEFORE the journal exists and removed with it at completion — pins the
exact same facts (transaction id, plan id, the whole file pin set, the
history flag; the state may be at most one step behind the journal's,
the crash window between the two publications). A fully self-consistent
journal planted into `.lekalo/cache/change/` alone — by clone, checkout,
or an archive preserving the tree — is refused with the barrier
retained (`change.commit-failed`, `journal-provenance`); the whole
`.lekalo/` runtime home is gitignored so neither the journal nor the
registry propagates through version control. A durably committed
recovery additionally requires the canonical history to name the
transaction. This is tamper evidence, not authentication: an attacker
with write access to the entire project tree can forge both files —
equivalent to editing the canonical sources directly.

Every journaled path is validated component-wise against the confined
project root (no-follow, including Windows reparse points) AND against
the canonical homes the structure owner admits before any recovery
write — an existing-file entry must be an admitted Model document home
or the history sidecar, a creation entry must be the history sidecar or
an admitted module kind home, and duplicate or aliased entries refuse —
so a forged journal can never claim an unrelated project file
(`change.write-boundary-denied`, denied 3). The pending registry,
journal directory/file, and backup directory/files are also checked as
no-follow regular files or directories; duplicate registry transaction
IDs and duplicate journal JSON members refuse. Every backup is
digest-verified against the `beforeDigest` pin written into the journal
at staging time, and every target is preflighted against the states the
transaction can explain (its pinned before bytes, its pinned after
bytes, or a restoration-pending absence): edits made after an
interruption are preserved with the reader barrier retained
(`change.precondition-failed`, `recovery-drift`). A durably committed
recovery verifies every target against its pinned after bytes and
completes its cleanup BEFORE reporting committed, so a committed
receipt never leaves its own reader barrier in place. The receipt
surface verdicts EVERY pending journal with the closed decoder — a
lexically-first committed-looking journal without exact history custody
cannot mask a later unresolved one. Recovery reclaims a stale
writer lock whose recorded holder is provably dead; a live holder still
denies (`change.precondition-failed`). The rollback reports receipt
state `rolled-back` — never `committed` — as a flat change-receipt
document.

The canonical history is a tamper-evident chain: each record carries a
digest over the previous record's digest, the project identity, the
record's exact execution-intent digest, and its own pinned fields plus
the ENTIRE embedded receipt projection, recomputed and verified on
every decode; replay, lookup, and committed recovery additionally bind
the ledger's project identity to the selected project's owner document.
A valid ledger copied from a different project ID refuses rather than
authorizing replay. The stored intent is additionally cross-checked against
the receipt's own constituent facts (policy digest, expected plan,
acknowledgement flag); the policy-bytes digest also binds its allowed
compatibility classes. Forged records, field tampering — including a
lone `intentDigest` edit — insertion, deletion, reordering, and
zero-byte ledger files all fail closed (`change.history-invalid`,
invalid 1); replay never guesses from current source bytes. The chain
is tamper evidence against partial edits, not authentication against
an attacker who can recompute every hash.

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
