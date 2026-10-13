# Task external links (pilot)

The `tasklinks` pilot (issue #52) is the target-neutral multi-provider
`TaskExternalLink` domain: one local task owns zero or more external
links, and every external identity belongs to at most one task in its
installation scope. The surface is read-only: it validates
declarations, projects typed views, and compares portability — it
never executes a provider, never writes outbox state, imports tokens,
or stores reports.

- Contract family: `dev.lekalo.task-links-integration@0.8.0` (wire
  `lekalo/task-links-integration/v0.8.0`), with the derived
  `task-links-report`, `task-links-evidence`, and
  `task-links-portability` documents at the same version.
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
`tasklink.evidence-invalid`. A `native` origin claim is admitted only
when the environment actually stands behind it: the provisioning flag
(`LEKALO_TASKLINKS_NATIVE=1`) plus the executed native battery's
qualification receipt beside the provisioned DB (every required leg,
the exact identity, the named database). A flag without an executed
battery refuses with `tasklink.evidence-invalid`. The contribution
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
execution. Core execution facts and target guarantees therefore stay
`unknown` and are refused under `strict`; `executed` remains empty in
file-admitted summaries. The native gate records its observed battery
execution separately and never re-labels mock steps as native runs.

Detected effects are capped at `extracted` confidence. Their content
hash covers compact recursively key-sorted envelope JSON excluding
`evidenceDigest` itself; it detects tampering, not producer authenticity.
Typed local groups must join real canonical writes, network calls stay
outside them, and partial-failure records require mixed recipient
results with exactly the failed-only retry set. These sections explicitly
carry `basis: envelope-declaration`: authoritative transaction/case owner
admission and executed atomic/partial traces remain pending.

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
pinned bundled-SQLite runner under provisioning. Its four legs cover
identity races, in-process acknowledgement recovery, durable attempts
and a real child-process kill during an uncommitted transaction. This
is not maintained Laravel execution or full worker-context recovery;
those native acceptance obligations remain pending. Mock/file evidence
never satisfies durable/race qualification, and a co-located receipt
alone cannot establish authenticated execution provenance. The external
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
