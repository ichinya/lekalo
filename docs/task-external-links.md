# Task external links (pilot)

The `tasklinks` pilot (issue #52) is the target-neutral multi-provider
`TaskExternalLink` domain: one local task owns zero or more external
links, and every external identity belongs to at most one task in its
installation scope. File-based commands validate declarations and project
views; they never call a live provider or import credentials. Explicit
`execute` / `--execute-mock` runs the deterministic scenario owner, and
`--execute-native` provisions disposable local state and executes the
maintained Laravel port with synthetic providers; no production state is used.

- Contract family: `dev.lekalo.task-links-integration@0.8.0` (wire
  `lekalo/task-links-integration/v0.8.0`), with the derived
  `task-links-report`, `task-links-evidence`, and
  `task-links-portability` documents at the same version. The export-only
  `task-links-execution.schema.v0.8.0.json` records execution-owner,
  implementation/build, source/input, assertions and result bindings.
  Transaction-concurrency and extended-effects 0.8.0 successors negotiate
  Model/IR 0.8.0; their frozen 0.2.16 predecessors remain unchanged.
- Model/IR ownership (AC1): the typed `Task -> links 0..N` relation,
  the aggregate-child relation, and the exact external-identity
  constraint are declared IN the Model and carried in the canonical IR
  (`contracts/model.schema.v0.8.0.json` + `contracts/ir.schema.v0.8.0.json`,
  `dev.lekalo.ir@0.8.0`). The attachment mirrors the Model; validation
  joins every attachment relation/constraint to the compiled Model and
  refuses any drift - including coherent cardinality mutations - with
  `relation-diverges-from-model`/`constraint-diverges-from-model`.
  Model 0.2.16 sources keep `dev.lekalo.ir@0.2.16` with byte-identical
  canonical output; no alias under 0.2.16 exists.
- Reverse impact: `lekalo task-links impact CONTRACT` derives the
  consumer set (capabilities, bindings, action policies, commands,
  effects, verification scenarios) from the attachment and the
  compilation - never a prewritten list - together with the exact
  member paths: every request/response/error field of the referenced
  schemas carries its own derived reverse chain (request members
  through operation -> policy -> command -> the delivery-surface
  worker -> every declared target binding -> covering verification
  scenarios; response/error members through the delivery surface),
  and bounded static coverage is complete only when member schemas,
  consuming operations, actual outbox-writing workers, module target
  bindings and covering scenarios all resolve. The boundary is
  `declared-schema-consumers`, not verified implementation execution;
  missing witnesses remain explicit gaps.
- Diagnostics: `LEK-TASKLINK-001`..`008` live in the additive
  diagnostic-registry union successor
  (`dev.lekalo.diagnostic-registry@0.8.0`); predecessor producers keep
  their exact registry pins.
- Synthetic fixtures: `tests/fixtures/task-external-links/` — the
  planner/tasklinks model, the integration attachment, mock provider
  snapshots (GitHub-Issues metadata with a reviewed upstream pin plus
  a fully synthetic secondary mock), scenario documents, and one
  admitted mock evidence receipt. Public artifacts use synthetic
  identities only and never name a consumer application.

## Commands

`lekalo task-links validate PATH --project DIR --profile
strict|permissive` joins the closed attachment to the current
compilation and reports the static obligations: relation cardinality
and field existence against the typed IR, the exact five-part external
identity constraint, decision completeness (actor, policy, capability,
revision pins), forbidden planning-field protection, operation binding
joins, and delivery-rule boundaries. Under `strict`, missing or
unsupported required semantics invalidate the run; `permissive`
reports the same gaps without claiming pass. A declaration defect
(such as an unresolved policy reference) invalidates under both
profiles.

`lekalo task-links inspect SYMBOL --project DIR --attachment PATH
[--evidence PATH]` resolves SYMBOL against the typed view (relations,
constraints, decisions, bindings, provider contracts, bound entities)
after the attachment passes the full semantic validation; an unknown
symbol is one typed refusal, a malformed attachment never renders as
an authoritative view. With `--evidence`, an admitted receipt joins
the view (origin, pins, step count); missing evidence stays explicitly
unavailable, and stale or wrong pins fail closed with
`tasklink.evidence-invalid`. Every file-origin `native` claim is refused,
including a genuine exported sidecar or a caller-supplied qualification
boolean: files/rows cannot authenticate a past execution. Actual native
qualification requires the explicit live owner channel below. The contribution
switches are consumed: a disabled `inspect`/`impact`/`scenarios`
contribution refuses the inspect/portability/evidence surfaces with
`contribution-disabled`, and the view renders the full switch row.

Every digest pin on the attachment is recomputed, never trusted: the
Model/IR pin against the canonical IR of the live compilation, the
attachment revision against the canonical attachment-minus-revision
bytes (compact JSON, recursively byte-sorted keys), and every
owner-contract pin against the exact accepted contract file in
`contracts/`. A divergent pin fails closed with
`tasklink.relation-identity-invalid` before anything else runs.
Duplicate JSON members are rejected on every input path (attachment,
evidence, profiles) before serde could silently take the last value. A
same-family different-version discriminator maps to the accepted
`unsupported-version` exit class (exit 5). File operands are opened
no-follow: symlinks and Windows reparse points are refused.

`lekalo task-links portability PATH --project DIR --from-profile PATH
--to-profile PATH [--evidence PATH]... --profile strict|permissive`
joins the resolved profiles, provider contract requirements, and any
admitted evidence into the closed projection: core semantics,
provider adapter facts, and target guarantees. Every row is derived,
never constant — a declared fact without admitted execution evidence
is honestly `unknown`, an undeclared fact is `unsupported` — and the
shared semantic validation runs before any report. A provider-only
change preserves the pinned core fact rows while the adapter rows
move. File-origin evidence is a producer declaration, even when its
steps, digests and SQLite run rows agree: it cannot authenticate
execution. Without live owner execution, core facts and target guarantees
stay `unknown` and are refused under `strict`; `executed` remains empty
in file-admitted summaries.

## Execution-owner channel

`lekalo task-links execute PATH --project DIR` executes all ten compiled
pilot scenarios through the deterministic core engine and independently
asserts state, decisions, receipts, replay, stale resolution, rollback,
partial recipients and failed-only retries. `--execute-mock` on portability
uses the same sealed process-local capability to qualify core facts.
Only the executing module can construct that capability: it has private
fields, no deserializer/import path, and no boolean qualification API.
A serialized receipt is exportable evidence, **not re-importable authority**.
This is local owner authentication, not a signed remote/hosted CI attestation.
The normalized receipt and portability `executionProvenance` bind the actual
implementation/build, canonical source, admitted attachment/recipe inputs,
selected profile and individually asserted result; copied results do not qualify.

`--execute-native` on portability runs the maintained
`App\\Lekalo\\TaskLinksStore` implementation through locked Laravel **13.33.0**
(`tests/fixtures/php-laravel/planner/composer.lock`) and PDO SQLite. Provision
its dependencies explicitly with `composer install --no-interaction
--prefer-dist --no-scripts --working-dir tests/fixtures/php-laravel/planner`.
Missing/mismatched runtime provisioning refuses, never falls back to mocks.
The owner stages embedded worker/implementation bytes in a fresh disposable
DB, synchronizes distinct processes for identity/lease/version races, kills
workers inside an uncommitted four-write transaction and after committed
provider acceptance before acknowledgement, and independently reads the DB
after new-process recovery. It checks the full persisted command/action,
workspace/task/link/generation/version, actor/fields, policy/capability pins,
payload/causation/key, attempts and receipt context; stale authority refuses.
Only the actual `php-laravel-maintained` profile earns the four native target
rows (unique index, compare-and-swap, local transaction, durable outbox).
A real Laravel run selected against Node/Rust leaves those target rows unknown.
Provider behavior is still a synthetic receipt service, not authenticated
GitHub execution or remote exactly-once support.

Raw `effects show --evidence PATH` envelopes remain capped at `extracted`.
Their compact key-sorted content hash (excluding `evidenceDigest`) proves
integrity, not execution; typed sections carry `basis: envelope-declaration`.
For authoritative executed traces, use `effects show OPERATION --project DIR
--task-links-attachment PATH --transactions PATH --extended-effects PATH`.
This executes the sealed mock owner and admits exact same-compilation
transaction/extended-effect documents through their typed owners. All four
canonical task/decision/outbox/audit edges join the exact required local group;
network calls remain outside it. Partial-failure case, scenario digest,
per-provider fault/outcome/receipt and failed-only retry set must match the
actual run. Invented groups/cases, omitted members, stale pins, conflicting
faults and mixed file/executed custody refuse. Verified edges and typed
`basis: owner-admitted-execution` are emitted only through this channel.

## Guarantees and limits

The deterministic engine (identity reservation over typed tuples,
attributed inbound deduplication keyed by the full installation /
provider / external / event scope, revision ordering per the declared
provider contract, decision matrix with explicit partial-capability
subsets, outbox leasing with fencing and time-bound expiry,
acceptance-pending-ack uncertainty, bounded retry with durable
attempt accounting, echo suppression, crash recovery) is exercised by
the Rust battery and the six family gates under
`scripts/test-task-links-*.mjs`, and the qualified native legs run a
pinned bundled-SQLite runner under provisioning, followed by the maintained
Laravel multi-process battery above. The Rust leg covers identity races,
in-process acknowledgement recovery, durable attempts and child-process
transaction interruption; the maintained leg adds synchronized process
races, real acknowledgement-window termination and full-context restart.
Mock/file evidence never satisfies native qualification, and a co-located
receipt alone cannot establish authenticated execution provenance. The external
identity tuple is `(workspace_id, installation_id, provider_id,
object_type, external_id)`, compared as exact text; detach retains
history and the external object; the network call is declared outside
every local transaction; acceptance before acknowledgement surfaces
as the explicit `accepted_pending_ack` state, recovery inside that
window lands in the persistent non-dispatchable `unknown_delivery`
state (only the explicit reconciliation command resolves it), lease
expiry fences every dispatch path, stale link generations cancel
their intents at claim and dispatch, the durable retry budget is
enforced at claim (crash loops cannot bypass it), inbound events are
fenced by the link's full identity/lifecycle/sync mode, and the
closed wire carries the canonical nine-state machine.
