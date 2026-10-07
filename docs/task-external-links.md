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
under the qualified-environment flag (`LEKALO_TASKLINKS_NATIVE=1`) and
always requires the wall-clock-native clock policy.

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
move; target guarantees without native evidence stay explicitly
`unknown` and are refused under `strict` instead of being claimed.

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
pinned transactional DB runner with process-restart failpoints under
provisioning. Mock evidence never satisfies the durable/race
assertions: native coverage stays unclaimed until a qualified
environment runs the native gate with provisioning. The external
identity tuple is `(workspace_id, installation_id, provider_id,
object_type, external_id)`, compared as exact text; detach retains
history and the external object; the network call is declared outside
every local transaction; acceptance before acknowledgement surfaces
as the explicit `accepted_pending_ack` state and recovery inside that
window reports unknown delivery for reconciliation instead of a
guessed success.
