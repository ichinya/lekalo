# Issue #87 — Fix Round 3 report

Branch `ichinya/m4-issue-87`, worktree `m4-issue-87`. Base: r2 head
`2de7afde`; the convergent major (devin F-1 / cline R2-1, grant validity
discarded on report/exposure/consent/inspect) was fixed first in
`92bdcf86` + `ad309be5` (shared `grant_is_valid` predicate threaded into
`run_report`/`exposure_findings`/the consent gate/`classification
inspect`; validation findings fold into the report; adversarial
`expired-public-grant` fixture denies end-to-end).

Round 3 covers the remaining minors, one commit per finding. Reviews:
`.m4/issue-87-review-devin-r2.md`, `.m4/issue-87-review-cline-r2.md`.

## Process note (honesty)

Early in the round three commits were briefly reset away and recreated;
the branch tip history below is the final, verified state. Two of the
recreated commits initially landed incomplete/broken (a required-flag
mistake on `--as-of`; a test written against non-existent APIs) and were
**fixed forward with dedicated fix-forward commits** — never re-reset:

| Commit | Content |
|---|---|
| `88d46ed2` | R2-2 part 1: `--as-of` args on classification validate/inspect |
| `504aa321` | R2-3/F-6 tenant heuristic docs + R2-4 registry severity flip |
| `a88c46fc` | F-3 first attempt (superseded, see F-3 below) |
| `c6be3359` | F-4/R2-5 dead gate arms (completed by `e27c9d8`) |
| `e27c9d8` | **fix-forward**: completes R2-2 (optional `--as-of` + `LEKALO_AS_OF` env, deterministic default preserved; dataflow report wired) and finishes the F-4 removal (parse/as_str arms, orphaned `confidence` param, dead `inputs_declared_complete`) |
| `923e93b` | R2-4 tail: contract-gate row for DFL-009 aligned (error, `valid` dropped) |
| `3f295dfa` | F-5: dedicated custody ids LEK-CLS-013..015 |
| `6f2905b` | F-3 fix-forward (analyzer-level gated-gate tests) + F-7 emit-event boundary decision + docs |
| `21510f2` | **fix-forward**: drops the superseded non-compiling detected-edge F-3 draft from the integration binary (HEAD must compile) and rustfmts the inspect payload call |

## Per-finding table

| Id | Severity | Resolution commit(s) | Evidence |
|---|---|---|---|
| devin F-1 / cline R2-1 (major) | major | `92bdcf86`, `ad309be5` (pre-round) | Shared `grant_is_valid` predicate (roles + strict lowering + not self-approved + not expired at as-of) used by `run_report`, `exposure_findings`, the consent gate, and inspect; `ValidationOutcome` rows fold into the report and deny the verdict; `expired-public-grant` fixture: `classification validate` exit 1, `dataflow report --endpoint planner.api_focus:public` denied with both findings. |
| cline R2-2 | minor | `88d46ed2`, `e27c9d8` | `--as-of DATE` is an **optional** flag on `classification validate`, `classification inspect`, and `dataflow report` (deterministic default `DEFAULT_AS_OF = 2026-01-01T00:00:00Z` preserved for derived artifacts; env override `LEKALO_AS_OF`). `validate_policy_and_grants_as_of` is now the only validate entry the CLI uses. Docs state the fixed date + semantics. Probe on `expired-public-grant`: `--as-of 2019-01-01T00:00:00Z` → exit 0 "valid: 11 subject(s), 0 findings"; default → exit 1 LEK-CLS-007. |
| devin F-2 | minor | `ad309be5` (pre-round) | Inspect lowers kinds only through `grant_is_valid`; e2e §6d'' asserts the unlowered kind on the expired fixture. |
| cline R2-3 / devin F-6 | minor | `504aa321` | `docs/classification.md` now states the convention loudly: `tenantRelation: same` is **declared-partitioned, field-NAME heuristic** (entity declares `tenant`, `tenant_id`, or `*_tenant_id`), not an enforced tenancy declaration; absence still fails to `unknown` → crossing. IR has no first-class tenancy declaration; binding to one is deferred to the query-model tenancy shape. |
| cline R2-4 | minor | `504aa321`, `923e93b` | Registry row `dataflow.observed-incomplete` (LEK-DFL-009) is `default_severity: "error"` with `allowed_statuses: [invalid, denied]` — matching the producer's `Severity::Error` findings; the unreachable `valid` status is dropped (the denial also rides `inputsComplete: false`). Contract gate (`test-classification-contracts.mjs`) row + comment updated; gate green: `{ok:true, registryEntries:358, classificationRules:15, dataflowRules:9}`. |
| devin F-3 | minor | `a88c46fc` (superseded), `6f2905b` | First attempt injected a detected edge via `attach_detected`; that cannot reach the gate (see seam note below) and used non-existent APIs. Fix-forward adds `dataflow::analyze::gated_sink_tests` — four analyzer-level cases driving `gate_outcome`'s gated branch over a real policy/resolution: consent-without-valid-grant → `missing-approval`; undeclared destination → `destination-forbidden`; declared destination → satisfied `destination-declared`; missing policy row → `unknown-flow` (fail-closed). **Seam note (documented in the test header):** ADR-0013 §3 keeps gated kinds out of the declared Model, and detected gated edges carry `ResourceKind::ExternalService/Cache/Job/Output` subjects, which are namespaced-only — `SubjectPath::parse` rejects them, so the detected loop skips such edges before gate evaluation. The gated branch is therefore only unit-reachable today; end-to-end reachability stays on the documented observed-evidence ingestion seam. |
| devin F-4 / cline R2-5 | nit | `c6be3359`, `e27c9d8` | Dead `GateReason::LowConfidence`/`InputsIncomplete` arms removed from the vocabulary (`parse`/`as_str` included), from `gate_outcome`, from `gate_rule_of`, and from `gate_rules_cover_every_failure_reason`; the always-`true` `inputs_declared_complete()` and the now-unused `confidence` param on `gate_outcome` are gone. Detected edges keep emitting `low-confidence-sensitive` + `observed-incomplete` findings directly, so no coverage was lost — the vocabulary no longer implies coverage that never existed. |
| devin F-5 | nit | `3f295dfa` | Dedicated custody refusal ids registered additively: `classification.custody-project` (LEK-CLS-013), `classification.custody-model` (LEK-CLS-014), `classification.custody-ir` (LEK-CLS-015) — error/invalid/semantic, byte-canonical rows, id-sorted (the registry gate enforces `entries-not-sorted-by-id` and pinned entry counts). `validate_custody` now refuses under the dedicated ids; builders + registry-finalization tests added; new regression `custody_mismatch_refuses_under_the_dedicated_id` asserts the project-pin refusal id (and that it is *not* `classification.unknown-kind`). Probe: corrupting `modelRef.digest` → exit 1, `LEK-CLS-014 classification.custody-model`. ADR-0043 range updated to `LEK-CLS-001..015`. |
| devin F-7 | minor | `6f2905b` | Decision: `EventPublish` stays **out** of `is_gated` — documented, not silent. `emit-event` edges are intra-model domain events consumed inside the model boundary; external publication is the detected `publish-output` kind (`publication`, gated). For `crossTenant: forbidden` kinds the tenant rule still catches event flows (underivable relation → `unknown` → crossing). Documented in `docs/classification.md` ("Gated sinks and the emit-event boundary") and on `is_gated` itself; pinned by `event_publish_stays_outside_the_gated_surface_set` (EventPublish ungated; ExternalCall/Publication/CacheWrite/Export gated). Gating `event-publish` itself waits for sink-actor bindings on the event bus. |

## Verification

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | clean (0 warnings) |
| `cargo test -p lekalo-core --lib` | 555 passed; 2 failed — the pre-existing Windows sandbox pair (`target_protocol` confinement/conformance, `TransportFailed { detail: "spawn" }`, error 1450), reproduced at merge-base by both r2 reviews |
| `cargo test -p lekalo-core --test dataflow` | 3/3 |
| `node scripts/test-classification-cli.mjs` | `{ok:true, fixtures:{valid:1, declassified:1, invalid:7}, sentinelScanned:true}` |
| `NODE_PATH=%TEMP%\lekalo-ajv-8.17.1\node_modules node scripts/test-classification-contracts.mjs` | `{ok:true, ajv:"8.17.1", registryEntries:358, predecessorEntries:321, classificationRules:15, dataflowRules:9}` |
| `cargo test -p lekalo-core --test dataflow` (against committed HEAD) | 3/3 |
| R2-2 probe (`expired-public-grant`) | `--as-of 2019-01-01…` → exit 0; default → exit 1 LEK-CLS-007 |
| F-5 probe (corrupt `modelRef.digest`) | exit 1, `LEK-CLS-014 classification.custody-model` |

## Residual risks / carried seams

1. Gated-sink evaluation over detected edges remains unreachable
   end-to-end until the observed-evidence ingestion seam carries
   resolvable subjects (F-3 seam note); the gate machinery itself is now
   unit-proven.
2. `emit-event` flows stay ungated by the documented F-7 boundary;
   `reviewed` kinds emitting to the bus pass without destination/consent
   evaluation until sink-actor bindings land.
3. `tenantRelation: same` remains the documented declared-partitioned
   name heuristic until a first-class tenancy declaration exists in the
   IR (R2-3).
4. Registry grew additively within the frozen `0.4.0` successor
   (+3 rows, byte-canonical, id-sorted); all contract gates stay green.

No merges, tags, or pushes; no other branches touched.
