# Issue #34 review round 2 (devin)

Reviewer: dispatched worker `task_fb8619e98b12`. Review-only; no
implementation changes. Branch `ichinya/m7-issue-34`, base
`origin/ichinya/M7` = `9510dd07`, HEAD `a7075590` (fix round 1:
`31d22945` + `ba3586a5` + `a7075590`, reviewed against the post-review
candidate `5e935a47`). Verified against both round-1 reports
(`issue-34-review-devin.md`: 2 major, 3 minor;
`issue-34-review-codex.md`: 4 major, 1 minor) and the fix report
(`issue-34-fix1.md`; no rebuttals claimed).

## Verdict: ISSUES — 0 blocker, 2 major, 1 minor

The fix round addressed every finding's *pin* side correctly — the
write-scope text is now truthful in all three places, `validate` is
repinned to a published describing schema, `drift` is a real negotiated
operation, the prescribed `validate` argv carries `--no-cache`, and the
manifest schema is genuinely tightened (all of Codex's mutation vectors
fail even with recomputed digests; reproduced below). However, **both
newly published describing schemas are wrong for reachable exit-0
receipts** — the exact defect class the original majors reported. Each
was reproduced end-to-end with the real binary and the pinned Ajv
8.17.1 this gate uses.

## Remaining findings

### 1. MAJOR — `validation-report` schema rejects every warning/info-bearing success receipt

`contracts/validation-report.schema.v0.6.3.json` is closed at the top
level (`additionalProperties: false`; only `status`, `modelVersion`,
`validation` declared). But `DomainResult::to_json_string`
(`crates/lekalo-core/src/result.rs:441-457`) appends a top-level
`diagnostics` array and `reasonCodes` array to **any** `Valid` envelope
whose diagnostics are non-empty, and `lekalo validate` returns through
`DomainResult::validation`
(`crates/lekalo-cli/src/main.rs:2714-2716`), which carries the
report's warning/info findings plus the classification-review records.

Reproduced: `lekalo validate --no-cache --project
tests/fixtures/validation/warning/portable-target-reference --json`
exits 0 and emits `status:"valid"` with one `info` diagnostic —
`diagnostics` + `reasonCodes` present on the envelope. Ajv 8.17.1
against the published schema:

```text
warnings-receipt-valid: false
/  additionalProperties  "diagnostics"  must NOT have additional properties
/  additionalProperties  "reasonCodes"   must NOT have additional properties
```

An existing CLI test asserts this wire shape
(`crates/lekalo-cli/tests/validate_semantic.rs:144-172`: default
profile, exit 0, `document["diagnostics"][0]`). The schema's own
description concedes the members exist — "Warning/info validation
findings ride the envelope's `diagnostics` array (closed
`lekalo/diagnostic/v0.2.16` items)" — yet forbids them.

Impact is the same as the original Devin-2/Codex-1: a consumer
performing the documented exact output-schema negotiation rejects a
valid exit-0 receipt whenever the run records any warning/info finding
(a normal default-profile outcome, not an edge case). The new gate
check 9 only exercises the zero-diagnostic `valid/base` fixture, so
the defect passes CI.

Fix direction: declare optional `diagnostics` (bounded array of the
closed diagnostic document or the published diagnostic schema) and
`reasonCodes` (array of rule ids) on the receipt schema, and extend the
gate with a warning-bearing fixture vector — the fixture already
exists.

### 2. MAJOR — `generate-check` schema's `finding` enums are inverted vs the emitted wire

`contracts/generate-check-receipt.schema.v0.6.3.json` declares
`finding.verdict` enum `["orphan","clean","reported"]` and
`finding.lifecycle` enum `["generated","scaffolded"]`. The emitted
non-blocking findings can never carry any of those verdicts and can
carry two lifecycles the enum forbids:

- Verdicts: `run_check`/`push_finding`
  (`crates/lekalo-core/src/artifacts/check.rs:351-366,393-405`) put a
  finding in the success receipt's `findings` only for **non-`generated`
  lifecycle** entries with verdict `stale`, `manual-drift`, or
  `missing` (`DriftVerdict::as_str`, `artifacts/types.rs:586-596`).
  `orphan` findings always go to `blocking` → `ArtifactFailure::Drift`
  → exit 1 (`check.rs:302,379,414-416`); `clean` entries are only
  counted; `reported` is a receipt-level verdict, never a finding
  verdict.
- Lifecycles: `generated` findings always block, so receipt findings
  are exactly `scaffolded`/`checked`/`external`/`custom`
  (`Lifecycle::as_str`, `artifacts/types.rs:190-199`).

Reproduced end-to-end: copied `tests/fixtures/orchestration/project`,
ran real `lekalo lock -- node adapters/node-typescript/node-adapter.mjs`
and `generate --target node-typescript -- node ...` (which also
re-confirmed the corrected write-scope claim — the run created
`src/generated/node-typescript/planner.ts` outside `.lekalo/**`), then
re-authored `ownership.json` with the entry's lifecycle flipped to
`custom` (+ `manual-only` policy, canonical digest recomputed), drifted
the file, and ran `lekalo generate --check --json`: exit 0,
`verdict:"reported"`, `findings[0] = {path, owner:"planner.generated",
kind:"source", lifecycle:"custom", verdict:"manual-drift"}`. Ajv
8.17.1:

```text
reported-receipt-valid: false
/findings/0/lifecycle  enum  allowedValues ["generated","scaffolded"]
/findings/0/verdict    enum  allowedValues ["orphan","clean","reported"]
```

An existing CLI test pins the same wire values
(`crates/lekalo-cli/tests/generate.rs:484-488`: `verdict:"reported"`,
`counts.reported:1`, `findings[0].lifecycle:"custom"`,
`findings[0].verdict:"manual-drift"`).

Impact: identical to the original Codex-3 — the negotiated contract
fails on the very response class the contract itself documents ("a
clean **or findings-only** check is exit 0"). Every real drift
observation a consumer actually needs (the non-blocking
staleness/manual-drift/missing reports) is rejected by the published
schema; only the vacuous clean case validates. The gate checks only the
clean case.

The schema description is also internally wrong: "only `orphan` may
appear as reported-only; stale/manual-drift/missing block" — the code
is the reverse (orphans always block; the other three are reported when
the entry lifecycle is not `generated`). `docs/provider-contract.md:172-175`
repeats the same over-broad blocking claim ("any blocking finding —
stale, manual drift, missing artifact, orphan — fails the run exit 1").

Fix direction: `finding.verdict` → `["stale","manual-drift","missing"]`;
`finding.lifecycle` → `["scaffolded","checked","external","custom"]`;
correct both descriptions; add a `verdict:"reported"` vector to the
gate (a scaffolded/custom drifted fixture entry — the pattern is
already exercised by `drifted_custom_files_are_reported_without_blocking_and_never_rewritten`).

### 3. MINOR — stale effect-class doc and a dead enum variant introduced by the fix

`crates/lekalo-core/src/provider/operations.rs:22-23` module doc
describes a "`read-or-check`" effect class for the drift variant — no
such `EffectClass` exists; `drift` carries `read-only` (line 123).
Conversely the enum now declares `RuntimeMetadata`
("runtime-metadata", admitted by the schema's `effectClass` enum at
`contracts/provider-capabilities.schema.v0.6.3.json:55`) which no
operation uses and `generation_is_the_only_mutating_operation`
prevents any current operation from using. Neither existed before the
fix (the pre-fix enum was `ReadOnly | GeneratedArtifacts`). The wire
contract is still correct — per-operation consts pin the effect — but
the module doc names a non-existent class and the dead variant/admitted
enum member has no described semantics. Drop or document it
consistently.

## Round-1 findings: disposition check

| Finding | Claimed | Verified |
| --- | --- | --- |
| Devin-1 / Codex-4 (write scope `.lekalo/**` overclaim) | fixed | FIXED — `operations.rs:16-21`, schema `effectClass` description (`:54`), `provider-contract.md` positions 6 + effect section now state adapter-declared scopes verified against the ownership plan + `.lekalo/generated/**` metadata + protected-home refusal; re-verified live (real generate wrote `src/generated/node-typescript/planner.ts`). |
| Devin-2 / Codex-1 (validate pin is the profile input schema) | fixed | PARTIAL — repin to `lekalo/validation-report/v0.6.3` is real and the profile pin correctly remains as configuration metadata, but the published schema is wrong for the diagnostics-carrying case (finding 1). |
| Devin-3 (manifest command strings) | fixed | FIXED — position 3 now says the manifest carries no command strings; `Operation`'s `command` field removed; golden confirms 5 fields. |
| Devin-4 (frozen `productVersion`) | fixed | FIXED — truthed in `version.rs:52-58`, `manifest.rs:81-84`, schema `:409-412`, contract doc `:83`; freezing rationale consistent with `docs/versioning.md`. |
| Devin-5 (argv subset / readiness `model` / drift adapter) | fixed | FIXED — position 4 marks the recipes a provider-relevant subset; readiness rule notes `model`; `drift` is a separate `requiresAdapter:false` operation so the naive-consumer conflict is gone. |
| Codex-2 (read-only `validate` writes cache) | fixed | FIXED — prescribed argv carries `--no-cache` (contract doc `:107`, effect section `:116-121`); new test `prescribed_validate_argv_with_no_cache_writes_nothing` and gate check 9 enforce the zero-write guarantee; re-ran it — PASS. |
| Codex-3 (drift-check has no negotiated contract) | fixed | PARTIAL — `drift` operation + `lekalo/generate-check/v0.6.3` exist, read-only, no adapter, tested at the boundary, but the schema rejects every `verdict:"reported"` receipt and misstates blocking semantics (finding 2). |
| Codex-5 (schema doesn't enforce declared invariants) | fixed | FIXED — per-operation const tuples at canonical positions (`prefixItems` + `items:false`), exact sorted 9-pin tuple; reproduced the attack vectors below. |

## Reviewer-verified gates (this worktree, Windows, Node 24.13, Ajv 8.17.1)

```text
cargo test -p lekalo-cli --test provider --locked            12/12 PASS
cargo test -p lekalo-core provider:: --locked                21/21 PASS (incl. diagnostics::provider)
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                    PASS
cargo fmt --all -- --check                                   PASS
node scripts/check-contract-versions.mjs --base origin/ichinya/M7
                                                           PASS (98 artifacts, product 0.6.3)
node scripts/test-provider-contracts.mjs                     PASS, 11 checks
git diff --check origin/ichinya/M7..HEAD                     PASS
git status --porcelain                                     clean (probe dirs under gitignored target/)
```

Codex-5 negative vectors, rerun with recomputed self-digests
(Ajv 8.17.1): golden valid; `generate` marked read-only → rejected;
alien `lekalo/alien/v99.0.0` pin → rejected; swapped operation order →
rejected; `validate` repinned to the profile schema → rejected;
eleventh `init` operation → rejected.

## Regressions / gate-weakening

None found. The diff's removals are superseded assertions replaced by
stronger ones (wrong `validation-profile` pin → `validation-report`;
`seven`→`nine` pins; `nine`→`ten` operations; added field-level const
constraints). Test count grew (provider CLI 10→12, core unit tests
cover the new constants), the Node gate grew 9→11 checks and now
executes two live receipt validations it never performed before. No
assertion was relaxed and no schema was loosened — the manifest schema
is strictly tighter. `provider`/`validation` fixture families are
already declared in `tests/fixtures/fixture-provenance.json`; CI wiring
(`ci.yml:80` schema job, `:188-193` build job after `cargo build`) is
intact. Scope: additive docs/contract/gate/test changes only, no
unreviewed artifacts.

## Bottom line

Close, but not landable: the two fixes that were the core of both
reviews (truthful negotiated output contracts for `validate` and
`generate --check`) publish describing schemas that fail on the
non-empty cases — reachable via the prescribed argv, proven against the
real binary and the pinned Ajv. Both need a small schema correction plus
one gate vector each; the minor doc/enum nit rides along. Everything
else in the fix round verifies clean.
