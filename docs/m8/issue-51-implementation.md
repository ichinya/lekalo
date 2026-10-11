# Issue #51 implementation: Worklog and opt-in Changelog pilot

Implemented on branch `ichinya/m8-issue-51` at the M8 product version
`0.8.0`. The authoritative plan is `docs/m8/issue-51-research.md`; every
proposed artifact below was built, validated, and committed locally
(no push, no GitHub mutation). The live issue's eight acceptance rows are
mapped at the end.

## Contract artifacts (all new, all at product 0.8.0)

- `contracts/worklog-changelog-pilot.schema.v0.8.0.json` — the closed
  composition contract `dev.lekalo.worklog-changelog-pilot@0.8.0`,
  discriminator `lekalo/worklog-changelog-pilot/v0.8.0`: twelve fixed
  operation roles with declared effect boundaries and commit boundaries,
  independence rules, correlations, approval/snapshot field bindings,
  the public field allowlist with the shared rendering policy, replay
  scopes, and capability requirements. Draft 2020-12, closed
  (`additionalProperties` + `unevaluatedProperties` false), bounds per
  the research (128 operation roles, 128 consumers, 64 scenarios, 256
  assertions, 32 capabilities per target).
- `contracts/worklog-changelog-evidence.schema.v0.8.0.json` — the closed
  evidence report `dev.lekalo.worklog-changelog-evidence@0.8.0`:
  composition pin, toolchain, per-target backend state/execution,
  capability rows (`supported|partial|unsupported|unknown` + blocking),
  per-scenario assertions, consumer witnesses, completeness, and
  verdicts. Unsupported publication semantics stay visible rows.
- `contracts/diagnostic-registry.v0.8.0.json` (+ its schema successor) —
  the strict union successor: all 522 current entries byte-for-byte plus
  exactly `worklog-changelog.input-invalid` `LEK-WC-001` through
  `worklog-changelog.evidence-incomplete` `LEK-WC-007`. Sorted ids,
  unique codes, `message_id == id`, closed data fields. The wire stays
  `lekalo/diagnostic/v0.2.16`; the grammar is unchanged.
- `scripts/gen-worklog-changelog-contracts.mjs` authors all four and
  re-verifies union/sort/uniqueness on every run (`--check` is the
  read-only live mode).

## Pure core owner

`crates/lekalo-core/src/worklog_changelog/` validates the composition
document end to end with no hidden side effects:

- `wire.rs` — bounded parse from exact bytes with duplicate-key
  rejection before any map insertion, closed member sets, digest pins,
  closed vocabularies (roles, effect kinds, commit boundaries, consumed
  families), canonical ordering, and hard refusal of duplicate roles.
- `types.rs` — the typed attachment and the `assemble` gate (bounds,
  adjacent-order invariants, replay-scope uniqueness).
- `validate.rs` — the pure join against the pinned compilation:
  custody (project id + exact IR digest), role resolution to declared
  commands/queries, effect-graph agreement with the canonical declared
  projection (count and set equality; anonymous query reads matched by
  count and kind), independence rules refusing shared effect ids and
  shared infrastructure effects, approval bindings split between
  publish-time inputs and approval-record fields, append-only snapshot
  field pins, public allowlist checked against every private entity
  field, payload-bound replay scopes, and capability coverage that keeps
  `publish.approval`, `publish.revisioned_snapshot`,
  `security.review_gate`, `transaction.atomic_group`,
  `idempotency.durable_key` and an explicit partial provider-idempotency
  row mandatory.
- `evidence.rs` — the honest admission: strict refuses (`unsupported`)
  when required rows are unsupported/unknown/not executed; permissive
  degrades; both retain every row and blocker.
- `diagnostic.rs` — the seven rules through the registry-backed
  constructor; the 0.8.0 registry is the third resolution tier
  (`embedded` → `successor` → `pilot`), so predecessor producers keep
  their exact pins and existing goldens are untouched.
- `crates/lekalo-core/tests/worklog_changelog.rs` — the pinned join of
  the committed fixture, the scenario corpus canonicalization through
  the production Scenario IR module, one negative vector per rule (exact
  rule id + detail token), duplicate-key rejection, and honest evidence
  admission for both profiles.

## CLI

`lekalo pilot validate <document> --project DIR` and
`lekalo pilot admit <report> --profile strict|permissive --project DIR`
in `crates/lekalo-cli/src/main.rs`, following the `requirements`/
`nfr` pattern: the core owns every decision, the binary selects,
renders, and maps exits via `DomainResult` (valid 0, invalid 1, denied
3, unsupported 4); both commands honor the global `--no-cache`. A
strict admission whose derived verdict is `unsupported` returns the
evidence-carrying unsupported class (`DomainResult::unsupported_json`,
the mirror of `denied_json`): the receipt stays visible in the
envelope's `payload` while the process exit is the documented 4.
Admission also enforces the closed evidence schema's required-member
sets top-level and per-target (`missing-member` refusal), so a
structurally incomplete report never reaches verdict derivation.

## Fixtures (`tests/fixtures/pilot/worklog-changelog/`)

- `model/` — the neutral synthetic `taskhub` project: `worklog` module
  (completion, private worklog, provider outbox and delivery as separate
  commands with separate declared effects), `changelog` module (opt-in
  draft, preview, approval binding, gated publication with a separate
  publish-output effect, withdrawal, public DTO queries), the
  authorization attachment pinning the exact compiled IR digest with
  member/author/service-worker/publishing-authority/public-reader
  actors, and the OpenSpec requirement tree bound through the
  requirements attachment with exact canonical revision digests.
- `attachments/` — the accepted #23–#26 families instantiated against
  the compiled Model (transaction-concurrency with independent commit
  boundaries and the declared publication atomic group; extended-effects
  with the outbox job, conditional provider call, consented review-gated
  revisioned publication and four partial-failure schedules; the error
  registry; invariant transitions), plus `pilot.json`, the composition
  document pinning exact Model/IR and attachment digests.
- `scenarios/` — seventeen canonical Scenario IR documents: the ten
  issue scenarios, the approval-required, stale-preview, and
  publication-partial-failure guards, and the four catalog-parity
  vectors (`worklog_partial_failure`, `declared_atomic_rollback`,
  `republish_revision`, `publish_withdraw_ordering`) that complete the
  declared `scenarios.yaml` catalog 1:1. The contracts gate and the
  corpus test both verify the catalog↔document parity: every scenario
  id declared in the model catalog has a committed, production-
  canonical scenario document.
- `evidence/` — the honest evidence report (php unknown/unknown-capable,
  go unsupported with backend absent, all publication rows blocking) and
  the consumer oracle fixing the reverse closure of the public DTO.
- `public/` — the closed application DTO schema (payload version 0.8.0).
- `invalid/` — the duplicate-key and unknown-version raw vectors.
- `php/`, `go/` — the maintained port manifest and the Go refusal
  record; native runtime evidence stays target-confined.

## Gates and CI

- `scripts/test-worklog-changelog-contracts.mjs` — Ajv 8.17.1 schema
  gate: both pilot schemas, the registry union successor, fixture
  instances, canonical compact byte-sorted bytes (the duplicate-key
  proof), and the Model/IR engine pins of the composition and corpus.
  The binary resolves per host (`LEKALO_BIN`, then
  `target/debug/lekalo.exe`, then `target/debug/lekalo`); without a
  binary the pin leg degrades loudly (`enginePins:
  "skipped-binary-unavailable"` plus a stderr warning), never silently.
- `scripts/test-worklog-changelog-pilot.mjs` — the closed
  `--target php|go --profile strict|permissive --case all|<id>` harness
  with `--expect-denied` as the negative gate: composition validation
  through the binary, static forbidden-effect evaluation against the
  canonical effect projection (denial-path assertions retained as
  runtime evidence, never counted as passes), admission re-derived
  through `lekalo pilot admit`, schema-valid report on stdout, bounded
  diagnostics on stderr. The harness also enforces the documented exit
  contract of the admission: a derived `unsupported` verdict must exit
  4 with the receipt still on stdout, every other verdict must exit 0;
  an exit-4 receipt is read from the failure envelope's embedded
  payload.
- `scripts/test-worklog-changelog-impact.mjs` — binary-driven: the
  complete depth-256 reverse closure of `changelog.public_entry_dto`
  and the mutation probes against the committed oracle, compared as
  exact set equality in both directions — a consumer missing from the
  oracle or an unlisted consumer in the closure both fail the gate.
- `scripts/test-worklog-changelog-neutrality.mjs` — the full public
  artifact inventory scanned for framework vocabulary, identity
  leakage, host paths, wall clocks, credentials, and non-synthetic URLs.
- CI (`.github/workflows/ci.yml`): the contracts gate joins the pinned
  Ajv schema job; a dedicated `worklog-changelog-pilot` job builds the
  locked CLI and runs the pilot harness (both strict denials asserted,
  the permissive degraded pass), the impact gate, and the neutrality
  gate. The generator scripts are the sanctioned regeneration path.

## Governance

`check-structure`, `check-privacy`, `check-contract-versions --base
HEAD^` (140 contract artifacts), `test-fixture-provenance` (the pilot
fixtures live under the already-synthetic `pilot` family),
`test-diagnostic-contracts`, `test-golden-diagnostic-coverage`, and
`test-architecture-profile-contracts` (the predecessor 0.6.5 consumer)
all pass unchanged.

## Acceptance mapping

| Issue acceptance criterion | Proof |
| --- | --- |
| Independent effects представлены отдельными declarations. | The Model declares one effect per concern; the composition roles carry one declared effect set each; `validate.rs` refuses shared effect ids and shared infrastructure effects between independence-rule targets; scenarios `completion_without_worklog`, `completion_with_worklog`, `no_provider_comment`, `no_opt_in` statically prove the absence (pilot gate, 14 static proofs). |
| Publication contract требует preview/approval. | The approval binding covers preview revision/digest/tenant at publish time plus destination/authority/policy revision on the approval record; `changelog.scenario.approval_required` and `stale_preview` guard documents; `publish.approval` capability row mandatory (WC-003/WC-007). |
| Partial failures не откатывают successful local completion без declared transaction. | The transaction-concurrency attachment declares separate commit boundaries (completion, worklog, outbox, delivery, publication) and non-atomic external boundaries with `recovery_required`; the provider/publication/worklog partial-failure scenarios assert the committed completion intact; `declared_atomic_rollback` is the contrasting declared-group rollback vector, committed as a scenario document. |
| Snapshot semantics переносимы между targets. | The snapshot declaration pins append-only revision/digest/schema-version fields of the declared entity; the `source_edit_after_publish` and `republish_revision` scenario documents; per-target capability rows keep revisioned-snapshot support visible per backend. |
| Scenarios проверяют privacy/security/idempotency. | `public_projection` (allowlist-only DTO), `unsafe_content` (shared rendering/safe-URL policy), `repeated_correlation_key` (replay + payload-drift conflict), cross-tenant exclusion; the validator enforces the excluded-field closure over every private entity; replay scopes are payload-bound and collision-free (WC-005/WC-006). |
| Impact показывает все consumers changelog schema. | `test-worklog-changelog-impact.mjs` validates every oracle witness family: the five graph consumers and three transitive subjects as exact set equality both directions, the public schema projected against the composition allowlist (both wire directions, required coverage, no unserved private field), and the composition through the production custody join; the additive and breaking mutation probes execute on isolated copies and must reproduce exactly the declared deltas; unknown external consumers stay explicitly possible. |
| PHP/Go portability report не скрывает unsupported publication semantics. | The native PHP runtime lane (`php/run.php`) executes all seventeen scenario assertions in-process and the harness binds the live results before admission: php rows are `executed`/`supported` with the honest `call.idempotent_write` partial row, go rows stay `unsupported` (backend absent); strict admission re-derives every row from the composition and the compilation, so a report author cannot erase a guarantee, and the go strict `--expect-denied` leg certifies the aggregate refusal in CI. |
| Public artifacts не раскрывают название consumer application. | `test-worklog-changelog-neutrality.mjs` scans the complete inventory for framework vocabulary, identity leakage (project-id closure, package identities, branded compounds), host paths, wall clocks, credentials, and non-synthetic URLs; any inventory file without a scannable text extension fails the gate (`unscannable-inventory`), so an opaque blob can never evade the scan; the fixtures use only the synthetic `taskhub`/`worklog`/`changelog` vocabulary. |

## Verification record

- `cargo fmt --all -- --check`
- `cargo clippy -p lekalo-core -p lekalo-cli -- -D warnings`
- `cargo test --workspace --locked` (42 targets, 0 failures)
- `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-worklog-changelog-contracts.mjs`
- `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-worklog-changelog-pilot.mjs --target php --profile strict --case all` (the native PHP lane executes all seventeen scenario assertions; a missing interpreter or any failure fails the gate)
- `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-worklog-changelog-pilot.mjs --target go --profile strict --case all --expect-denied`
- `NODE_PATH="$LEKALO_AJV_NODE_PATH" node scripts/test-worklog-changelog-pilot.mjs --target go --profile permissive --case all`
- `node scripts/test-worklog-changelog-impact.mjs`
- `node scripts/test-worklog-changelog-neutrality.mjs`
- `node scripts/check-contract-versions.mjs --base HEAD^`

## Review round 2 resolutions

The devin and codex round-2 reviews drove this follow-up work: the two
embedded Model/IR pins that the r1 digest shift had gone stale carry the
recomputed digests; the contracts gate gained the fixture-parity leg
that runs the documented review commands against the committed fixture
and fails closed on a present-but-broken CLI; evidence admission is a
bound join (the report pins the exact composition bytes, and every
required row, scenario, backend binding, blocking entry, closure claim,
and verdict claim is re-derived from the composition and the pinned
compilation) over a complete closed wire decode; `pilot validate`
loads and hashes every consumed document and runs its production
validator, refusing duplicate family pins; read-only eligibility is
derived from the compiled operation and every claimed read resolves 1:1;
replay and correlation keys resolve against operation inputs; the
capability guarantees are bound to their exact operation scope and
minimum; the impact gate validates every witness family and executes
both mutation probes on isolated copies; and the native PHP runtime
lane above supplies the required native acceptance. Latent fixture
defects the new joins exposed (error-registry retry consistency and
canonical order, extended-effects effect-ref shapes and case integrity,
invariant-transition subject spaces and transition error outcomes,
transaction capability coverage) were repaired through the sanctioned
generators with every pin recomputed.

## Review round 5 resolutions

The devin and codex round-4 reviews (both CHANGES-REQUESTED) drove this
round; every numbered finding is fixed and probe-verified:

1. **Admission executes the backend.** `lekalo pilot admit` runs every
   declared native lane itself and hands the executed rows to the pure
   core as a trusted receipt. A report row claiming execution without
   a matching receipt is refused (`execution:unverified`): a static
   hash of the backend file is provenance, not an execution receipt,
   so an `exit(77)` backend with a copied digest can no longer pass.
   The receipt join also verifies the exact assertion rows, the
   capability tuples, and the full scenario corpus, and the consumer
   witness join verifies every reported `(consumer, relation,
   witness)` triple against the committed consumer oracle cross-checked
   with the compilation's derived graph inventory, refuses unknown
   consumers and unknown assertion ids, and enforces scenario target
   membership.
2. **Report-level blockers refuse the targets they apply to.** Core
   admission attributes every report-level obligation (failed or
   contradictory scenario rows, missing or invented consumer
   witnesses, verdict-claim conflicts, closure gaps) to the targets
   they bind, so the per-target receipt verdict the pilot gate reads
   can never stay green for a document admission refuses for that
   target; an honestly absent *other* target stays independently
   reported. A pruned php-only report (a deleted go row) is an invalid
   document outright (`targets-inventory`): the portability report can
   never hide the unsupported lane.
3. **Inner Model pins verified on every consumed family.** The
   transaction-concurrency, extended-effects, invariant-transition, and
   requirements attachments must carry the exact canonical Model
   digest of the resolved compilation; the error-registry identity pin
   is exact equality (no prefix fallback); the authorization document's
   declared identity must equal its consumed pin.
4. **Custody is physical.** The composition document, every consumed
   attachment, and the native lane file resolve under the fixture root
   without traversing symlinks, junctions, or reparse points, and
   never carry a second hard link — byte equality alone cannot
   establish custody.
5. **Creation is insert-only within the tenant scope.** `create_draft`
   refuses cross-tenant hijacks, refuses recreation over published or
   withdrawn entries (no unaudited unpublish, no resurrection), keeps
   the revision monotonic on a same-tenant recreate, and invalidates
   every outstanding approval; withdrawal also invalidates approvals
   and dominates every later binding state.
6. **The gate binds the committed Scenario IR, not labels.** The PHP
   lane emits the exact ordered invocation log (operation + input map)
   and structured per-step facts (observed entity rows with field
   values, typed error payloads, replay classes with duplicate counts,
   served projection fields); the gate matches every `when` step as an
   ordered subsequence of the invocation log and evaluates every
   `then` assertion (selectors, expected fields, payload members,
   replay detail, projection against the composition's allowlist)
   against those facts.
7. **Replay scopes carry actor identity and canonical payload
   binding.** The scope composite is (tenant, actor-or-service,
   operation) + correlation key; payload digests are canonical
   byte-sorted-key JSON of the submitted request members, so a changed
   request under a recorded key conflicts instead of replaying, and a
   missing requestor id is refused.
8. **Durable claims are backed by a write-through journal.** Replay
   records and outbox rows/outcomes append to a journal file as they
   commit; the committed scenarios exercise recovery by rebuilding
   fresh store instances from the journal bytes (restart replay,
   delivery after restore) — the claim notes describe exactly that
   in-process recovery model.
9. **The URL policy refuses encoded forms.** Decoding runs to a
   fixpoint over numeric entities, a closed named-entity table, and
   percent escapes; any decode-changing spelling refuses
   (`rejectEncoded`), controls are re-checked on the decoded text, and
   the scheme allowlist applies to every `scheme:` token — `mailto:`,
   `tel:`, `ftp:`, encoded delimiters, and encoded allowed schemes all
   refuse, while plain prose colons stay safe.
10. **Decoder/schema parity is exact and gated.** The semantic-id
    grammar, the field-name and pin patterns, the control-character
    exclusion, every `uniqueItems` array, the per-target assertion
    bound (`maxItems: 256`), and the consumed identity/version pins are
    ported verbatim from the published closed schemas into the shared
    decoder grammar, and the contracts gate now runs differential
    wire probes — Ajv 8.17.1 verdict versus production CLI verdict —
    over curated mutation vectors in both directions so the two
    grammars cannot drift apart again.

## Boundary

Real provider integration, live publication, private requirement
sources, and native Go runtime acceptance remain out of scope by
design; the reports keep those gaps visible instead of filling them.
The PHP lane runs in the pilot job on the pinned 8.3 interpreter; a
real Go backend still supplies its bound evidence in a separate
toolchain-pinned job.

Deferred by review (recorded, not silently dropped): the seven
`LEK-WC-001`–`LEK-WC-007` rules are not registered in the suite
coverage index (`tests/fixtures/suite/v1` stays pinned to
`diagnostic-registry.v0.6.4`). The golden-catalog gates pass unchanged
because the pilot family resolves through the 0.8.0 successor tier, not
the pinned suite catalog; cross-registering the pilot rules into the
suite coverage index is its own follow-up with owner approval, exactly
like the PHP/Go runtime deferral.
