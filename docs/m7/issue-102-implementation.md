# Issue #102: privacy-safe public metrics export implementation

Status: **Implemented** local aggregate export, product 0.6.4, branch
`ichinya/m7-issue-102`. Acceptance authority: [issue #102](https://github.com/ichinya/lekalo/issues/102),
refreshed through the authenticated GitHub API on 2026-10-04. Research baseline:
`d26991a0`, [committed research](issue-102-research.md), integration baseline
`a2aa1893766c01434842ca5062188a43898f41c5`. This is a local implementation;
hosted CI, remote publication and production AIFHub import are not acceptance
evidence. No push or other worktree change is part of this delivery.

[Issue #100](https://github.com/ichinya/lekalo/issues/100) remains OPEN at the
verification date and lands in parallel. It owns evaluation execution,
scheduled population approval and producer semantics. This change defines
`evaluation-export-input@0.6.4`, a named typed handoff carrying approved paired
trial membership, arm assignment, run ids and required hard assertions. It
cannot carry measurements, judge scores or arbitrary payload fields. Its
`approved` bit is producer metadata, not proof of issuer authenticity. The
complete frozen privacy authorization binds the exact selected population.
Synthetic acceptance below proves this seam and export behavior; it does not
claim that #100 has produced real Framework Lift results.

## Delivered surfaces and ownership

| Surface | Concrete owner and behavior |
| --- | --- |
| CLI | `crates/lekalo-cli/src/metrics_export.rs::MetricsCommands`; `metrics export --evaluation FILE --scope TOKEN (--dry-run\|--confirm DIGEST) [--authorization FILE] [--recipe FILE] [--destination SPEC] [--project DIR]`; `metrics status EXPORT_ID --scope TOKEN [--authorization FILE] [--project DIR]`. CLI parsing delegates aggregation and refusals to core. |
| Typed #100 seam | `crates/lekalo-core/src/metrics_export/types.rs::{EvaluationInput,Trial,Arm}`; closed `evaluation-export-input.schema.v0.6.4.json`. Distinct live run ids, exactly one baseline/assisted entry per unit, duplicate-key refusal, at most 1,024 entries. Sorting trials and hard assertions normalizes equivalent inputs. |
| Only measurement source | `metrics_export/mod.rs::{evaluation,snapshot,paired}` reads live #121 `run-record@0.4.0` and digest-bound `run-assertions@0.4.0` through `Store::get`; raw observations and loose JSON cannot substitute for stored records. Frozen source refs, scope and local-private/ineligible source constants are checked. No observed index, source tree, prompt or raw tool output is read. |
| Aggregate recipe | `metrics_export/aggregate.rs`; `contracts/metrics-aggregation-definition.v0.6.4.json` and its closed schema. Optional recipe must equal the embedded definition. Formula/rule tokens, units, fields, masking and lifetime release budget are versioned. |
| Public package | `public-metrics.schema.v0.6.4.json`; `metrics-export-manifest.schema.v0.6.4.json` public view; payload and manifest only. Sorted compact UTF-8 JSON plus LF; exact SHA-256 payload, manifest, package and preview digests. Public manifest carries policy semantic ref, authority byte ref, recipe byte ref, exact privacy satellite/schema byte pins and the validity protocol. |
| Private custody | Manifest family local view carries scope, normalized selection, source digests/privacy refs, admitted projection and non-authorizing decision template. It is separate from public payload/manifest. Fixed homes: `.lekalo/privacy/aggregates/<digest>/` and `.lekalo/privacy/decisions/aggregate/<shard>/<digest>.json`. No caller-selected output path. |
| Shipped #119 reuse | `metrics_export/privacy.rs` calls `TrustedContext::embedded`, original `DestinationSpec::resolve`, `authorization_subject_digest`, `validate_decision_input`, `evaluate_decision`, and `redact`. All six destination specs are preserved. The explicit preview/no-confirm flow extends the existing dry-run precedent. |
| Shipped #120 baseline | Privacy/authority 0.3.2 and satellites 0.2.16 remain frozen. A Lekalo-owned admitted `aggregate.decision` projection references original history digests/labels; the derived artifact is `aggregate.artifact`. No history kind is relabeled as HLV-owned evaluation evidence or admitted by changing frozen authority. No sensitivity is silently removed. |
| Shipped #121 lifecycle | `run_history/store.rs::{export_locked,register_metrics_export}` adds an owner-held SQLite writer lock, generation check and same-scope lifetime release budget. Existing delete/clear/prune/retention transaction invalidates `aggregate-input`; final activation and status independently rehash source/assertion bytes. Generic dependent registration keeps its original behavior. |
| Storage | `metrics_export/storage.rs`: Unix descriptor-relative no-follow directories/files, Windows reparse-checked directory handles denying rename/delete, single-link files, create-new immutable writes, exact existing-byte checks, ignore protection and local read-only Git tracking refusal. The manifest is written last. Readers validate digests and current status. |
| Registry successor | Exactly the original 500 entries remain byte-equivalent under the baseline fixture's JSON serialization. Eight additive `LEK-MEXPORT-001..008` rules make 508 entries at product 0.6.4. `gen-suite-coverage.mjs` registers their fixture evidence and refreshes the registry digest. |
| Docs/CI | New owner `docs/metrics-export.md`; top-level command owner and all five families registered in `scripts/lib/docs-maintenance.mjs`. `update-docs-owners.mjs --write` regenerates owners and the `cli.md` index. `ci.yml` invokes a live gate for each family after the CLI build, using exact Ajv 8.17.1. |

## Determinism, uncertainty and public metadata

The recipe uses all selected scheduled pairs, without outcome filtering.
Groups bind arm, pilot/scope state and local core/profile/harness/adapter pins;
each pair must match Git, profile, harness, adapter and pilot metadata plus
required assertions. Git/source/model identities stay local. A comparison
with missing required pins is unknown. Retries, duplicate ingestion and tool
calls do not become additional samples.

The fourteen existing recorder measurement pointers yield sample, checked
u128 sum and exact unreduced `sum/n` rational. Duration uses milliseconds;
tokens/files/tool calls/retries/replans/facts use counts; context bytes and
estimated tokens remain distinct. Context coverage is included-facts sum
divided by candidate-facts sum and is unknown on zero denominator. Cost uses
exact six-decimal micro-units, with no rounding or currency conversion.
Mixed currency/basis or incomplete populations are unknown. Cost per success
is unknown when success count is zero. Required assertions and recorder
outcome/coverage determine verified success; recorder ingestion exit 0 never
counts as task success. Lift is assisted success rate minus baseline rate.

Each cohort needs at least five scheduled units. A nonzero outcome, coverage
or success/complement bucket smaller than five suppresses all cohort numeric
values; k-1 metadata aliases/versions are withheld too. Missing inputs never
become zero: precedence is withheld, then unsupported only when every input is
unsupported, otherwise unknown. Known zero survives. Stack, cached tokens,
fix cycles, first-pass success, human intervention and confidence stay unknown
where recorder/producer semantics are absent. No confidence interval or
quantile implementation is claimed.

The role alias registry is export-local: `consumer-01`, `profile-NN`,
`model-NN`, `harness-NN`. Private provenance controls deterministic ordinal
assignment but is not copied into public bytes. Missing identity remains a
four-state value, not an invented known alias. Only plain numeric release
versions are admitted; arbitrary token-shaped private names cannot enter a
version or alias field. One confirmed release per scope lifetime, including
invalidated/interrupted attempts, prevents a second complementary release
through this command. This is an engineering disclosure rule, not a formal
anonymity proof across independently re-created stores/scopes.

The scanner report explicitly names `typed-public-string-values/1`.
Allowlisted contract constants, content/pinned digests and typed numerals are
recognized before scanning other string leaves with the unchanged #119
engine. This avoids interpreting contract slashes as absolute paths or large
counts as phone numbers. Strict projection and closed schemas exclude raw
text; the scanner is a residual guard, not a native-symbol classifier.
Unexpected text cannot be repaired by changing a metric to a redaction marker.
The redaction diff is empty for the admitted typed projection.

## Acceptance-criterion evidence map

The family gate is `scripts/test-metrics-export-contracts.mjs`; every family
compiles its closed Draft 2020-12 schema with exact Ajv 8.17.1, validates
committed canonical synthetic goldens, and exercises the real offline CLI.
Its fixture provenance is declared in `tests/fixtures/fixture-provenance.json`
under `metrics-export`. No fixture is private-consumer or measured #100 data.

| AC from live issue | Implementation and executable evidence | Boundary |
| --- | --- | --- |
| AC1: only versioned #121 records with #120 policy | `snapshot`, `Store::get`, explicit header/privacy/scope checks, paired trial validation and admitted local projection. Live gate refuses loose observations, duplicate-key/unknown-field inputs, wrong versions, unapproved selections, cross-scope source selection, changed stored bytes and changed source policy even with a recomputed database digest. Actual issued input is validated by independent Node #119 and real Rust `privacy evaluate`. | Evaluation execution and authoritative schedule approval belong to #100; source records stay private/ineligible. |
| AC2: exact payload and redaction diff before publish | `metrics-export-preview` exposes exact payload, manifest, redaction diff, leak report and digests. Byte snapshots verify dry-run does not mutate history or create privacy files. Stop after preview is cancellation. Actual confirmation writes the same candidate bytes. Stale digest, changed history generation, expired evidence and wrong subject binding refuse. | Preview is an operator-local receipt containing decision/source custody metadata; only its payload and public manifest are releasable files. No network delivery. |
| AC3: no source/prompt/secret/PII/private identifier/path by default | Typed projection, closed root/nested schemas, role aliases, numeric version grammar, no input measurement fields. Gate checks forbidden public key names and a token-shaped private profile identity; unknown state carrying a value and arbitrary alias fail schema. Security unit probes test #119 leak refusal, linked homes and hard-linked outputs. Live junction activation leaves outside directory empty. | Scanner heuristics alone are not proof; closed fields and constant/typed construction are the primary boundary. |
| AC4: missing values unknown/withheld rather than zero | Four-state `ValueState`, complete-population recipe and minimum-sample masking. Gate independently checks measured zero, omitted fields, withheld, unsupported, missing pins, zero-success cost, k-1 and mixed-cell suppression. | Missing stack/fix-cycle/confidence and other unavailable producer fields remain unknown. |
| AC5: reproducible definitions/sample sizes | Versioned embedded definition and exact digest, source-id/selection normalization, deterministic grouping/aliases/canonical bytes. Hand-calculated fixture checks sums 50/100, means, 6.250000 USD cost and negative rational lift. Reverse enumeration preserves payload and reviewed digest; k/k+1 and whole-cohort small-cell masks are asserted. | Recipe intentionally covers sums/means/rates/cost/coverage, not additional statistical estimators. Lifetime release budget is fail-closed. |
| AC6: source deletion invalidates aggregates/manifests | `register_metrics_export` binds all runs as one `aggregate-input`; final source rehash and activation share the recorder writer lock. Live gate covers delete, clear, prune, retention-on-append and recovery, and status rejects mismatched scope/tampered package bytes. Writer-lock unit test proves another writer is excluded and refusal releases the lock. | Immutable file retains historical `valid-at-export`; authoritative status becomes `invalidated`. Offline validity is `unverified`; downloaded bytes cannot be recalled. |
| AC7: preserve negative/neutral results | All selected pairs retained. Gate asserts worse assisted success (lift -1), exact neutral success (lift 0), zero verified successes, and separate unsupported/infrastructure buckets with unknown verified success. Hard failures are not overridden by model/judge output. | Synthetic calculations only; no actual superiority result or #100 provider run is claimed. |
| AC8: future AIFHub importable schema | Portable closed JSON, stable discriminators, units, exact recipe/policy refs and state wrappers. Ajv validates actual CLI payload/manifest plus committed canonical goldens; digest and unsafe/unknown-field controls serve as an offline candidate consumer. Manifest carries `live-history-and-current-privacy/1` and `offlineStatus:unverified`. | Candidate import seam, not a delivered AIFHub service adapter or external acceptance. Future importer must consult current status/revocation before treating a historic package as valid. |

Internal source classifications need explicitly requested, fresh verified
declassification approval as well as aggregation approval and transfer
consent. The adapter can plan removal of only `internal`, keeps original
labels in the source projection and puts a null evidence slot in its
non-authorizing template. The frozen evaluator alone validates actual
declassification evidence. A live negative control proves aggregation plus
consent cannot silently remove the internal floor.

## Compatibility correction in the reused evaluator

The new live aggregate vector exposed a pre-existing inverted condition in
`privacy/evaluate.rs::decision_shape`: aggregation erroneously required
`removedSensitivities`, a field belonging to declassification and forbidden
by the frozen aggregation schema. The condition now matches the unchanged
schema and Node evaluator: aggregation validates its approved typed evidence;
declassification requires a nonempty valid sensitivity set. The metrics gate
checks both a valid aggregation input and malformed empty declassification
against both validators. This changes no frozen contract, policy, authority,
consent requirement or source sensitivity floor.

## Local validation record

Local checks during this delivery use the real Windows CLI, synthetic
disposable projects and exact Ajv 8.17.1 outside the checkout. Earlier
failing probes led to the compatibility/path/grammar corrections described
above; they are not counted as passing acceptance.

| Check | Observed result |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | PASS, product 0.6.4. |
| `cargo test --locked -p lekalo-core --lib metrics_export -- --test-threads=1` | PASS, 4 security/lifecycle unit tests: residual disclosure, hard links, junctions and recorder writer-lock/refusal release. |
| `cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings`; `cargo fmt --all --check` | PASS. |
| `test-metrics-export-contracts.mjs --family FAMILY` for all five families; combined final gate | PASS, schema/goldens plus real-binary preview, confirmation, denial, uncertainty and deletion. The manifest lane additionally runs neutral/outage/mixed-cell/source-mutation/generation/clear/prune/retention/recovery probes. |
| `test-privacy-evaluator-parity.mjs`; `test-privacy-runtime-cli.mjs` | PASS, 120 parity vectors and 13 runtime cases; the new metrics corpus separately exercises valid aggregation and malformed empty declassification. |
| `test-run-history-cli.mjs` | PASS, existing #121 offline end-to-end flow. |
| `test-diagnostic-contracts.mjs`; `test-golden-catalog.mjs`; `test-golden-diagnostic-coverage.mjs` | PASS, 508 registry rules; 21 catalog cases; fresh coverage receipts for 20 suite pairs, 131 family-fixture, 344 test-witness and 13 interaction-only rules. |
| `test-fixture-provenance.mjs` | PASS, 78 synthetic families, zero private/evidence-backed fixture families. |
| `update-docs-owners.mjs --write`; `test-docs-ownership.mjs` and `--static`; `test-docs-examples.mjs --static` | PASS, 157 commands, 130 contract artifacts, 56 protocols and 347 owned surfaces; 13 P0 owners. Static documentation example declarations pass; full documentation replay was not rerun. |
| `check-contract-versions.mjs --base d26991a0`; Git diff/index scope checks | PASS, additive product 0.6.4 contracts and original 500 registry entries preserved. |

CI provisions exact Ajv 8.17.1 outside the checkout and invokes each new
family's live gate after `cargo build`. `allowUnionTypes:true` honors the
unchanged #119 scalar-union definition; strict validation remains enabled.
Gate authoring (`--write-goldens`) was used explicitly to create synthetic
fixtures, then normal verification ran without writer mode. CI never writes
goldens, recipes, registry or documentation to repair a failure.

## Remaining external seams and operational limits

The #100 producer must emit this typed selection from its approved schedule
and assertion semantics. Existing #119 evidence carries declared
verification/freshness and exact binding; neither this command nor an
`approved:true` selection independently authenticates an issuer, computes
wall-clock grant expiry, or subscribes to remote revocation. Status needs a
freshly supplied authorization; without one it is unverified.

Local history deletion invalidates the registered dependent immediately;
status does not rely on filesystem cleanup. Immutable package/custody files
remain local historical receipts. An activation interrupted after dependent
registration consumes the lifetime budget; there is no release-recovery
command yet. Manifest-last writes and digest/liveness checks refuse partial
or stale packages. Consumers must perform those checks instead of treating
file presence as continuing publication eligibility.

All destinations currently prepare local files, like shipped privacy export.
Remote upload, acknowledged revocation, importer tombstones and real AIFHub
evaluation execution remain external capabilities. The CI wiring is shipped
in this change; a local test run is not hosted-CI or release acceptance.
