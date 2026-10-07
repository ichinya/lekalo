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
[--evidence PATH]` projects the typed view: sorted relations with
cardinality and detach semantics, the identity constraints, the
decision rows, the provider contracts and operation bindings, and the
delivery rule block. With `--evidence`, an admitted receipt joins the
view (origin, pins, step count); missing evidence stays explicitly
unavailable, and stale or wrong pins fail closed with
`tasklink.evidence-invalid`.

`lekalo task-links portability PATH --project DIR --from-profile PATH
--to-profile PATH --profile strict|permissive` separates unchanged
core domain semantics from provider adapter facts and target
guarantees. A provider-only change preserves every core row byte while
the pinned adapter rows move; target guarantees without native
evidence stay explicitly `unknown` and are refused under `strict`
instead of being claimed.

## Guarantees and limits

The deterministic engine (identity reservation, attributed inbound
deduplication, decision matrix, outbox leasing with fencing, bounded
retry, echo suppression, crash recovery) is exercised by the Rust
battery and the six family gates under `scripts/test-task-links-*.mjs`.
Mock evidence never satisfies the durable/race assertions: native
coverage stays unclaimed until a qualified environment runs the native
gate with provisioning. The external identity tuple is
`(workspace_id, installation_id, provider_id, object_type,
external_id)`, compared as exact text; detach retains history and the
external object; the network call is declared outside every local
transaction; acceptance before acknowledgement reports unknown
delivery for reconciliation instead of a guessed success.
