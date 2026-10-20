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
  journal/backup/recovery material), plus the private non-cache commit
  custody pin `.lekalo/change-history-pin.json` (never canonical Model
  input and never reconstructed from ledger claims)

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
`operation-output:set`, `query-effect:add`, `query-effect:remove`,
`requirement:link`, `requirement:unlink`, `migration:apply`, `module:move`,
`binding:add`, and `operation-error:set` — every implemented verb has first-class CLI
sugar that lowers to the same closed request DTO a request file
carries, so sugar and JSON requests are byte-identical at the core.
The remaining cataloged operations are explicit staged refusals
pending their sealed executors (the deferral matrix below). Runtime
decoding mirrors the
published request schema exactly: an
optional member a schema variant admits (for example `symbol:move`'s
`toName`) decodes and reaches the registered unsupported outcome —
never an unknown-member rejection. Declared field defaults and enum
wire values are decoded and refused explicitly at every
field-consuming operation (`change.operation-unsupported`), never
silently dropped from a committed declaration.

### Deferral matrix (P1 coverage scope)

Eighteen of the twenty cataloged operations are implemented and executable.
The remaining two are required before issue #73 is complete. They currently
decode and refuse `change.operation-unsupported` (exit 4); each needs its
contract, sealed executor, and runtime gates:

| Deferred operation | Follow-up increment |
|---|---|
| `ownership:set` | adoption-prerequisite ownership transition vertical (research I5) |
| `symbol:move` | semantic rename history and reference rewrites (research I5) |

Until an increment lands, the deferred verbs are reachable only as
JSON request documents through `change request --request <file>`, and
they refuse with `change.operation-unsupported` before any mutation.
The catalog status pins stay honest either way: an operation is
`implemented` in `contracts/change-catalog.v0.8.0.json` exactly when a
sealed executor exists.

`operation-error:set --operation ID --definition-json JSON` changes one
command/query error union in an existing `lekalo/error-registry.json`:
`{"errors":["planner.store_unavailable"],"registryDigest":"sha256:<64hex>"}`.
The registry is the canonical #62 owner format, bound to the current Model;
absence never installs the embedded seed. The non-empty union is sorted and
unique, every error resolves, commands retain unit output, and queries retain
their exact output type. Adding or removing union members is source-breaking
and needs a reviewed policy or explicit `--allow-breaking`; new bindings use
the owner's additive classification. This standalone operation writes only
that fixed registry home through the shared journal, history and custody
transaction. Replay verifies the later committed endpoint, and recovery
preserves external edits behind an actionable barrier.

Every Change plan pins the registry bytes, including absence. A registry
appearing or changing after preview refuses before writes; Model edits must
preserve every local binding's kind and output. The frozen #62 ID grammar is
still enforced. A module move refuses when it would invalidate a declared
error source location. Transport, OpenAPI/SDK generation, PHP joins, Module
API and Context Budget consume the local owner when present; absent projects
retain the embedded legacy default. A contradictory Module API error list
refuses. Context Budget marks applicable error facts unrepresentable, and
MCP snapshots pin this input even when the Model itself is unchanged.

`binding:add --symbol ID --definition-json JSON` admits two closed payloads:

- `{"kind":"canonical-target-binding","target":"node-typescript"}` adds
  a **new** target-binding definition at the declared semantic ID. An optional
  `description` is accepted. The target must have exactly one decodable local
  declaration. The definition joins its module's physical `bindings.yaml`,
  creating that kind home when necessary. Duplicate IDs and semantic validation
  failures refuse; this declaration makes no claim about native implementation.
- `{"kind":"observed-confirmation","proposal":"prop-<64hex>","candidate":"native-identity","indexDigest":"sha256:<64hex>"}`
  confirms one existing inferred binding through the observed owner. The exact
  proposal, symbol, selected candidate and index digest must agree, and the
  candidate's recorded fingerprint must match its current confined source file.
  Ambiguous proposals require an explicit candidate; missing or stale evidence
  refuses. The operation preserves scan provenance and appends the owner's
  confirmation history without promoting a canonical Model definition.

An observed confirmation is a standalone request. Its index and native evidence
join the plan's input pins; apply rechecks them under the writer lock. Only
`.lekalo/import/observed/index.json` is writable, in the same journal, rollback,
history and custody transaction as the Change receipt. Native files remain
evidence. Post-validation rechecks their fingerprints, and recovery preserves
later edits until the conflict is repaired. Model IR is unchanged; native impact
remains explicitly incomplete. Replay follows subsequent committed index changes
through the history chain, with the same local custody requirement.

Query effect operations edit the query's declared `reads` dependencies. They
resolve the query and referenced symbol through the typed Model; removing its
required final read refuses validation. Requirement operations edit the
symbol's `derived_from` list using the frozen uppercase requirement ID grammar.
They do not write external requirement stores. Removing the final link removes
the optional YAML member, preserving the accepted block-only YAML surface.

The registered application-model migration recipe
`model.entity-field-rename.v0.8.0` takes exactly `entity`, `name`, and `to`
parameters. It invokes the same sealed field-rename executor, including identity
reference updates, policy/evidence checks, file diff, rollback, and idempotent
history. The original migration request is hashed, and its versioned step ID is
retained in the plan and committed receipt's operation `detail`. Unknown recipes
refuse before mutation; user scripts are never executed. This is separate from
the Model schema-version migration command.

`module:move --module tasks --directory task_sources` relocates the physical
module directory and preserves every source byte and semantic ID. A move is one
standalone request operation; mixing it with other operations currently refuses.
The preview displays both old and new file coordinates and their digests. Apply
uses an atomic no-replace directory rename under the shared writer lock and
commits the history/custody records in the same recovery lifecycle. The exact
module inventory is checked before the move, after it, and before recovery. An
occupied destination, link, extra file, or changed byte refuses. Recovery keeps
later user changes and its reader barrier until the conflict is resolved. Receipts
distinguish `moved-from` and `moved-to`; replay follows the latest committed file
outcome, including subsequent relocations. Unix renames use a no-follow parent
directory capability; Windows holds parent handles against deletion/renaming.

```sh
lekalo change entity-field:add --project sample --entity tasks.task \
  --name priority --type tasks.priority --dry-run --json
lekalo change enum-case:change --project sample --enum tasks.priority \
  --name low --description "Lowest tier" --dry-run --json
lekalo change definition:add --project sample --kind command \
  --module tasks --name complete \
  --input-json '[{"name":"id","type":{"ref":"tasks.text"},"required":true}]' \
  --dry-run --json
lekalo change command-effect:add --project sample --command tasks.complete \
  --effect tasks.events.completed --dry-run --json
lekalo change operation-input:set --project sample --operation tasks.complete \
  --input-json '[{"name":"id","type":{"ref":"tasks.text"},"required":true}]' \
  --dry-run --json
lekalo change operation-output:set --project sample --operation tasks.find_done \
  --type 'list<tasks.task>' --dry-run --json
lekalo change query-effect:add --project sample --query tasks.find_done \
  --effect tasks.task --dry-run --json
lekalo change requirement:link --project sample --symbol tasks.task \
  --requirement TASK-REQ-001 --dry-run --json
lekalo change migration:apply --project sample \
  --step model.entity-field-rename.v0.8.0 \
  --parameters-json '{"entity":"tasks.task","name":"title","to":"label"}' \
  --dry-run --json
lekalo change request --project sample --request change-request.json \
  --apply --idempotency-key task-priority-v1 --json
lekalo change receipt --project sample --idempotency-key task-priority-v1 --json
lekalo change recover --project sample --transaction change-ab12cd34-1 --json
```

Every implemented verb shares the same execution flags
(`--dry-run`/`--apply`/`--idempotency-key`/`--policy`/
`--allow-breaking`/`--expect-model`). Field-list members that are
themselves declarations (`--input-json`, `--payload-json`) take one
typed JSON array parsed by the same duplicate-key-refusing parser as
the request surface; type spellings (`--type`, `--returns`) take the
Model sugar grammar (`ref`, `list<ref>`, `ref?`). The emission
spelling `command-effect:add --emit <event>` decodes and refuses
`change.operation-unsupported` with detail `emission-representation`
pending the reviewed extended-effect successor — the refusal is the
documented increment boundary, and the `--effect` reference spelling
is the implemented path.

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
formatting stay verbatim. Every requested/resolved operation still keeps
its audit row in original order, including no-op rows in mixed batches;
a changed signature performs proven bounded
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
evidence. The per-file `fileDiff` entries degrade the same way instead
of refusing or truncating silently: when either side of one file's diff
exceeds 16 KiB (`change.file-diff-limit`), or the line-level patch
exceeds 512 rows, that file's diff entry is omitted and the plan
envelope carries `complete: false` with `completeReason:
"change.file-diff-limit"` — the plan stays schema-valid, the semantic
diff and every digest pin stay exact, and only the human-readable
patch projection is bounded. The request digest
pins the request's canonical JSON: pretty and compact presentations of
the same operation batch canonicalize to one digest (the digest pins
the intent, not its bytes), so CLI sugar and an equivalent request
file produce the identical plan and idempotency binding.

## Application, idempotency, recovery

`change::apply_prepared` is the only writer. Under the shared
project-wide mutation lock (`.lekalo/cache/writer.lock`, held across
the change, migration (apply and rollback), format, generate, and
adoption (apply, confirm, rollback, and recovery) writers — every
mutating entry of every family) it rechecks the
durable ledger first (a repeated key with
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
(`change.recovery-required`, denied 3). The same shared lock carries
the sibling barriers: a change writer refuses while a migration,
format, or adoption journal is stranded (`change.precondition-failed`,
denied 3, detail `migration-journal`/`format-journal`/
`adoption-journal`) — the adoption barrier's stranded predicate ignores
completed work, which deliberately retains its journals (a strictly
owner-decoded, digest- and plan-bound completed receipt with the exact
write set, or recovery progress covering **every** original restoration
with matching plan/path/snapshot/before/after metadata, disambiguates).
A restored prefix of a multi-file recovery, malformed/empty receipt,
or enumeration/decode error preserves the barrier, and adoption's own
mutating entries stay reachable while a journal is stranded because
they are its resolvers.

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
cannot mask a later unresolved one. Recovery takes over the shared
writer lock when its OS-held handle is released (a dead owner); a live
holder still denies (`change.precondition-failed`). The rollback
reports receipt state `rolled-back` — never `committed` — as a flat
change-receipt document.

History is bounded independently by **4,194,304 encoded bytes** and
**4,096 records**. Before beginning a transaction the writer checks both
limits and round-trips the entire prospective history through the same
decoder used by receipt/replay. Capacity refuses with
`change.precondition-failed` (denied 3), detail `history-byte-limit` or
`history-record-limit`; all original sources, history rows, and custody
remain untouched, with no journal/pending record created. Existing
receipts/replays remain usable. There is no implicit rotation, deletion,
or truncation of audit rows; an archival/continuation contract is not
implemented in this increment.

The canonical history is a tamper-evident chain: each record carries a
digest over the previous record's digest, the project identity, the
record's exact execution-intent digest, and its own pinned fields plus
the ENTIRE embedded receipt projection, recomputed and verified on
every decode. A top-level `tip` independently pins the accepted record
count (`sequence`) and last `recordDigest`: dropping the tail and changing
`nextSequence` alone fails decode. Stored receipts admit only committed
mutations with a nonempty, duplicate-free source set or no-op with no
source files; malformed or empty mutation projections fail closed.
Replay, lookup, and committed recovery additionally bind
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

Replay authority therefore also requires independent **local commit
custody** (`.lekalo/change-history-pin.json`, outside the disposable
cache). The sealed writer journals and publishes this closed pin in the
same transaction as sources/history; ordinary failure restores the old
pin and committed recovery verifies it. It binds the exact accepted
history bytes, project, record count, chain tip, and the first locally
authorized sequence of its trust epoch. A ledger-only editor
cannot mint it by substituting a different in-project file, rewriting
after-digests to current bytes, fabricating a later successor, or
emptying the source set (even relabeling the claim as no-op). Missing,
linked, malformed, or mismatched custody refuses `change.history-invalid`
(`commit-custody`), including when a new key attempts to extend the edited
history. A removed ledger with surviving custody is not first-use.
`cache clear` preserves this pin and the persistent `writer.lock` leaf;
no-op mutating entries may create that lock without changing sources.

### New checkout: initialize a local trust epoch

The canonical audit/receipt store remains the history, not the custody
pin: read-only lookup remains available after source drift or a checkout
without local custody, but it never authorizes replay. A normal source
checkout intentionally omits the ignored `.lekalo/` pin. To perform NEW
semantic work in that checkout, first review its canonical Model and
history, compute the SHA-256 of the **exact history file bytes**, then run:

```sh
sha256sum lekalo/change-history.json
lekalo change custody-init --project . --expect-history sha256:<64hex> --json
# Now apply a new operation under a fresh idempotency key as usual.
```

`custody-init` is an explicit runtime-only initialization, not a change
record or a claim that old transactions ran here. Under the shared writer
lock and all sibling recovery barriers it validates the complete Model,
closed history, project binding, and expected byte digest; it atomically
creates (never replaces) the no-follow custody pin. Sources and history
remain byte-identical. The schema-valid no-op receipt has no request/key,
operations, files, history anchor, or transaction; no audit row is added.
No history means ordinary first-use apply instead; it needs no init.

History admission also enforces the unchanged closed Draft 2020-12 schema,
not just the public hash chain. Every record and embedded receipt requires
1–100 operation rows, with the schema's discriminator grammar, required
members, closed objects, enums, and string bounds. In particular, genuine
pre-r11 no-op records with `operations: []` are invalid legacy history:
init, apply (including new-key no-op), replay, and receipt lookup refuse
`change.history-invalid` (invalid 1, detail `history-schema`). No missing
rows are guessed, no original source/history/custody bytes are rewritten,
and no transaction is started. Restore a valid complete backup or await
an explicit fidelity-preserving migration; init is not such a repair.
Ordinary read-only Model loading and preview remain available, as does
audit lookup of schema-valid histories without local custody or after
source drift.

All existing sequences become an immutable **audit-only prefix**. Their
receipts stay readable, but they cannot authorize replay
(`change.history-invalid`, detail `commit-custody-epoch`) or committed
recovery (the recovery barrier remains), even if a forged prefix was
fully rechained to match current file bytes.
Subsequent locally committed rows alone acquire replay authority. Every
append preserves the original epoch boundary, so a legitimate new commit
cannot launder an imported claim. Restoring the original trusted pin
with a complete matching project backup is the alternative when old
local replay authority must be preserved; never synthesize that pin.

Missing/malformed history, stale expectations, invalid Models, foreign
project IDs, linked homes, held locks, and pending recovery refuse init.
An existing pin (valid, stale, or malformed) is never overwritten or
rebound by this command; investigate or restore it instead. A source
checkout of a different revision should start without another revision's
runtime state and explicitly initialize its own epoch, not copy a donor's
pin. Valid legacy local pins remain accepted and upgrade on the next
commit with the original first-local boundary of sequence 1.

Editing/copying both the entire runtime custody and the ledger is outside
the ledger-only trust boundary, like editing canonical sources directly.
This is local provenance, not cryptographic authentication against a
full-project writer.

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
