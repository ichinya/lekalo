# Issue #34 review round 3 (cline)

Reviewer: dispatched worker `cline`, dispatch `ctx_b66e67d3f229`. Review-only;
no implementation, test, fixture, or contract file was modified. Branch
`ichinya/m7-issue-34`, base `origin/ichinya/M7` = `9510dd07`, HEAD `c517582a`
(fix round 2: `4b8a6994` + `a906e7c8` + `b4aea3e2`, plus devin's round-3
report `c517582a`). Verified against both round-2 reports
(`issue-34-review2-devin.md`, `issue-34-review2-cline.md`) and the fix report
(`issue-34-fix2.md`).

Method: every reproduction below used the real binary built at this HEAD
(`cargo build -p lekalo-cli`) and the pinned Ajv 8.17.1 at the repo-convention
path (`%TEMP%\lekalo-ajv-8.17.1\node_modules`, Windows, Node 24.13.0). Probe
scripts live outside the checkout (`C:\Users\User\r3probe\`); nothing in the
worktree was touched. I read `issue-34-review3-devin.md` only after fixing my
own findings; the two of us converged independently, and I re-derived each
finding from the code, the schema, the binary, and Ajv rather than adopting it.

## Verdict: ISSUES — 1 blocker, 1 major, 1 minor

Fix round 2 lands the *shape* corrections the round-2 reviews asked for. Both
published schemas still describe only a subset of the receipts the real binary
emits, and the new gate acquires a hard dependency on an artifact no CI step
produces before the gate runs. Everything else in the delta verifies clean.

## Findings

### 1. BLOCKER — the gate requires a receipt that no CI step has produced when it runs

`scripts/test-provider-contracts.mjs:300-317` — when `target/debug/lekalo`
exists, the gate now **requires** `target/provider-receipts/drift-reported.json`
and hard-fails:

```text
{ "ok": false,
  "reason": "drift-captured-missing",
  "detail": "run: cargo test -p lekalo-cli --test provider (captures ...)" }
```

The file's only producer is the new Rust test
`drift_reported_receipt_is_captured_for_the_schema_gate`
(`crates/lekalo-cli/tests/provider.rs:639-643`), which writes it at the *end*
of the test body. `git check-ignore -v` confirms `.gitignore:1 (/target/)`
ignores it, so it is never committed and never present on a clean checkout.

Reproduced on this HEAD by deleting the capture (a clean-checkout
approximation) and re-running the gate with the binary present:

```text
gate with leftover capture present:  {"ok":true,"gate":"provider-capabilities","checks":12}
gate after removing the capture:    {"ok":false,"reason":"drift-captured-missing"}   exit 1
```

CI ordering confirms no producer runs first. In `build-test`
(`.github/workflows/ci.yml`, matrix ubuntu/windows/macos, no `target/` caching):

```text
:181  cargo build --workspace --locked          -> target/debug/lekalo now exists
:193  node scripts/test-provider-contracts.mjs   -> binaryAvailable=true, capture ABSENT -> exit 1
:228  cargo test --workspace --locked            -> the producer test first runs HERE
```

The only `cargo test` invocations between the build and the gate are
`lekalo-core` suites (`:217`, `:221`, `:227`) — none of which produce the file.
So every clean `build-test` run on all three OSes fails at `:193`, and the
producer executes afterwards at `:228`. The `schemas` job (`:80`) is unaffected
only because it never builds, so the gate skips the live block — which also
means no earlier job can catch the failure. The gate passes locally here *only*
because a leftover capture from an earlier `cargo test` exists; I confirmed that
by removing it and watching the gate fail.

Note the test itself is sound and does pass (`13/13`, `0 ignored`) — the defect
is the CI ordering of its artifact, not the test.

Fix direction: produce the receipt inside the gate (it already spawns `lock` +
`generate --check` for the clean vector, so the reported vector is reachable
the same way), or commit a pre-authored receipt under
`tests/fixtures/provider/`, or move the producer ahead of the consumer in the
workflow. A gitignored leftover must not be a hard gate precondition.

### 2. MAJOR — `validation-report` still rejects a reachable exit-0 receipt class: `error`-severity findings under the default profile

`contracts/validation-report.schema.v0.6.3.json:59` restricts the embedded
diagnostic `severity` to `["info","warning"]`. But the default profile records
classification-review findings onto the *success* envelope, and every
`classification.*` rule is registered `default_severity: "error"`.

The path, confirmed by reading rather than assuming:

- `crates/lekalo-cli/src/main.rs:2772-2785` — `if strict && outcome.invalid`
  returns `invalid`; otherwise `findings_set(&outcome)` becomes `recorded`,
  documented in-source as "Recorded, never silently skipped: the findings ride
  the success diagnostics".
- `main.rs:2714-2716` — `diagnostics.extend(recorded)` then
  `DomainResult::validation(...)`.
- `crates/lekalo-core/src/classification/mod.rs:74-91` — `findings_set` builds
  each row via `diagnostics::normalize::build`, which takes
  `entry.default_severity()` unconditionally
  (`crates/lekalo-core/src/diagnostics/normalize.rs:67`).
- `contracts/diagnostic-registry.v0.4.0.json` — all 16 `classification.*`
  entries are `default_severity: "error"` (429 of 449 entries are error).
- `crates/lekalo-core/src/validator/profile.rs:149` — a rule whose default is
  `Severity::Error` can never be downgraded (`return Err(ProfileError::Invariant)`),
  so the emitted severity is unconditionally `error`, not a profile artifact.

`DomainResult::validation`'s own doc comment claims "warning/info only"
(`crates/lekalo-core/src/result.rs:224`); that is unenforced and false on this
path.

Reproduced with the real binary at HEAD, prescribed argv, default profile:

```text
$ lekalo validate --no-cache --json --project tests/fixtures/classification/invalid/unclosed-policy
exit=0 status=valid diagnostics=1 reasonCodes=1
diag[0]: id=classification.kind-rule-missing code=LEK-CLS-010 severity=error
SCHEMA INVALID  /diagnostics/0/severity  enum  allowedValues ["info","warning"]

$ ... --project tests/fixtures/classification/invalid/expired-public-grant
exit=0 status=valid diag[0]: id=classification.expired-declassification code=LEK-CLS-007 severity=error
SCHEMA INVALID  /diagnostics/0/severity  enum
```

Severity enum is the *only* violation on both. Sweeping all seven shipped
`tests/fixtures/classification/invalid/` fixtures, two are reachable exit-0
error-bearing vectors and both are rejected; the other five are either exit 1
or carry no diagnostics:

```text
credential-declassified        exit=1 reasonCodes=["classification.invalid-declassification"]
expired-public-grant           exit=0 diag=[error:classification.expired-declassification] INVALID
public-endpoint-private-field  exit=0 diag=[]                     VALID
secret-in-sink                 exit=0 diag=[]                     VALID
tenant-crossing                exit=0 diag=[]                     VALID
unclosed-policy                exit=0 diag=[error:classification.kind-rule-missing] INVALID
unknown-subject                exit=1 reasonCodes=["classification.unknown-subject"]
```

This is the same defect class as round-2 finding 1: an ordinary,
prescribed-argv, exit-0 receipt that the documented exact output-schema
negotiation rejects. The two new gate vectors cover the zero-diagnostic and
info-bearing shapes only — `scripts/test-provider-contracts.mjs` never mentions
`classification` (confirmed by grep and by enumerating the fixtures it spawns:
`validation/valid/base`, `validation/warning/portable-target-reference`,
`orchestration/project`) — so the defect survives the gate exactly as its
predecessor did.

The prose is wrong in step with the enum, which is how I confirmed the intended
scope was narrower than the wire:

- `contracts/validation-report.schema.v0.6.3.json:5` — "Error findings make the
  run `invalid` (exit 1, stderr)". True for model validation; false for the
  recorded classification review under the default profile.
- `docs/provider-contract.md:159-167` — claims the schema "covers both reachable
  shapes". There are three: clean, info/warning-bearing, and default-profile
  error-finding-bearing.

Fix direction: admit the success-reachable severities the implementation
actually emits on this envelope (do **not** re-severity the registered
`classification.*` rows to satisfy the schema — that would misrepresent the
wire), correct both descriptions to state the three shapes, and add a
classification-bearing fixture receipt as a gate vector. The fixtures already
exist; the gate already copies fixture trees into a scratch dir.

### 3. MINOR — the embedded diagnostic item and the finding bounds diverge from the wire grammar they claim to describe

The receipt schema describes its `diagnostics` items as "the closed
`lekalo/diagnostic/v0.2.16` item schema" (`:26`), but the item is neither a
`$ref` to nor a faithful projection of the governing
`contracts/diagnostic.schema.v0.2.16.json`. Divergence in **both** directions:

**Looser than the governing contract** (admits non-wire shapes, so the
closed-item claim is false): `source`, `data`, and `metadata` are bare
`{"type":"object"}` and `related_locations`/`causes`/`fixes` items are entirely
unchecked, whereas the governing schema closes `source` (logicalPath + range,
`additionalProperties:false`), bounds `data` to a named `dataValue` union,
constrains `metadata`, and closes the three array item shapes.

**Tighter than the wire grammar** (can reject reachable items): the `id` and
`reasonCodes` pattern `^[a-z][a-z0-9]*(\.[a-z][a-z0-9-]*)+$` forbids
first-segment hyphens that the governing `ruleId`
(`^[a-z][a-z0-9-]*(\.[a-z][a-z0-9-]*)+$`) admits. Measured against the
registry: **36 of 449 registered rule ids are rejected** by the receipt
schema's own pattern — `native-gate.*`, `storage-engine.*`, `target-profile.*`,
etc. None is emitted on the validate path today, so nothing is broken yet, but
the schema advertises the closed diagnostic grammar while rejecting a third of
it. Also: `id`/`message_id`/`reasonCodes` items lack the governing
`maxLength: 128`; `symbol` is a bare string vs `symbolId` (≤256); `message`
maxLength 512 vs `boundedText` 256; `related_locations` maxItems 16 < contract
32; `causes` maxItems 16 > contract 8.

`generate-check-receipt` has the same precision gaps, verified against code:

- `findings` maxItems 1024 < the manifest's ≤65536 artifact entries
  (`crates/lekalo-core/src/artifacts/canonical.rs:408`) — a `reported` receipt
  can legally exceed it.
- `finding.owner` maxLength 128 < the symbol-id bound 191
  (`crates/lekalo-core/src/ir/grammar.rs:69`); `finding.path` maxLength 512 <
  `ArtifactPath`, which bounds per segment (≤64) with no total bound
  (`artifacts/types.rs:71-99`).
- `finding.kind` is a free ≤64 string where the wire emits the closed 6-value
  `ArtifactKind` enum (`source|test|schema|config|docs|data`,
  `artifacts/types.rs`); `owner`/`kind`/`lifecycle` and top-level
  `manifestDigest` are optional although `DriftFinding::entry`/`CheckReceipt`
  always emit them.

Two co-presence/conditional statements are documented in prose but not enforced
by the schema (I confirmed these are *accepted*, i.e. the gap is missing
enforcement, not looseness that admits hostile input):

- `validation-report` `reasonCodes` ↔ `diagnostics` "present exactly
  together" — a receipt with `diagnostics` but no `reasonCodes` validates, and
  so does `reasonCodes` with no `diagnostics`.
- `generate-check` `findings` "empty exactly when verdict is clean" — a
  receipt with `verdict: "clean"` and a non-empty `findings` array validates.

None of these rejects today's ordinary receipts — finding 2 is the one that
does. Same "schema ≠ wire truth" class as the majors, in the lax/bound-
mismatch direction.

## Round-2 findings: disposition check

| Round-2 finding | Claimed | This review |
| --- | --- | --- |
| M1 (devin-1 + cline-1) validation-report rejects warning/info receipts | fixed | **PARTIAL** — the warning/info shape now validates (reproduced). But the schema still rejects the error-severity shape reachable under the default profile (finding 2); the item/bounds precision is finding 3. |
| M2 (devin-2 + cline-2) generate-check finding enums inverted | fixed | **FIXED** — verified against the emitters: `finding.verdict` is `["stale","manual-drift","missing"]` and `finding.lifecycle` is `["scaffolded","checked","external","custom"]`, matching `check.rs:351-366` (`push_finding` only on stale/manual-drift/missing, orphans always `blocking` via `:378-380`, generated always blocking via `:400-404`) and `types.rs:191-199`/`types.rs:587-594` (`generated|scaffolded|checked|external|custom`; `clean|stale|manual-drift|missing|orphan`). The captured `verdict:"reported"` receipt validates. Orphan/generated blocking is now stated correctly in both the schema description and `provider-contract.md:172-179`. Bounds/precision residuals in finding 3. |
| Devin minor (dead `RuntimeMetadata`, stale `read-or-check` doc) | fixed | **FIXED** — `operations.rs` `EffectClass` is back to `ReadOnly`/`GeneratedArtifacts`, the module doc now describes the `drift` variant under read-only, and the `provider-capabilities` `effectClass` enum shrank to `["read-only","generated-artifacts"]`. No stray references remain in `crates/`, `contracts/`, `scripts/`, or `docs/`. |
| Cline-3 (minor) `drift` lock requirement understated | fixed | **FIXED** — reproduced: with the lock present and **no** `--locked`, `generate --check` exits 0 `verdict=clean`; delete `lekalo.lock` and the same argv exits 1 with `reasonCodes=["lock.missing"]`. `provider-contract.md:108` now states `project + lock (unconditional: a missing lekalo.lock fails with lock.missing; --locked only adds the freshness check)`, which matches `check.rs`'s unconditional `LockState::Absent -> LockFailure::Missing`. |

## Reviewer-verified gates (this worktree, Windows, Node 24.13.0, Ajv 8.17.1)

```text
cargo build -p lekalo-cli --locked                                PASS (already current)
cargo test -p lekalo-cli --test provider --locked                 13/13 PASS, 0 ignored
  (incl. drift_reported_receipt_is_captured_for_the_schema_gate)
cargo test -p lekalo-cli --test generate --locked                 10/10 PASS
cargo test -p lekalo-cli --test validate_semantic --locked         7/7 PASS
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
node scripts/test-provider-contracts.mjs                          PASS, 12 checks
  (only because the leftover capture exists — finding 1; deleting it
   reproduces drift-captured-missing)
node scripts/check-contract-versions.mjs --base HEAD              PASS (98 artifacts)
node scripts/check-contract-versions.mjs --base origin/ichinya/M7  PASS (98 artifacts, 0.6.3)
node scripts/test-contract-versions.mjs                           PASS (6 cases)
node scripts/test-fixture-provenance.mjs                          PASS (64 families)
git diff --check origin/ichinya/M7..HEAD                          PASS
git status --porcelain                                            clean
```

Every gate output the fix report cites reproduces. The fix report's own evidence
is accurate on everything I checked; its claim that the 12-check gate "added …
captured reported drift receipt" is true locally but is precisely what finding 1
makes non-reproducible on a clean runner.

### Receipt validation against the published schemas (real binary + pinned Ajv)

```text
validate-clean       (validation/valid/base)                        VALID
validate-warning     (validation/warning/portable-target-reference)  VALID
captured drift-reported receipt (verdict:"reported", findings non-empty) VALID
validate --project tests/fixtures/classification/invalid/unclosed-policy       INVALID
validate --project tests/fixtures/classification/invalid/expired-public-grant INVALID
```

### Schema closedness (28 negative vectors, 25 rejected)

The schemas were **corrected, not loosened** — the substance of the round-2 fix
is genuine. Extra top-level members, wrong `status`/`verdict`/`lifecycle`/
`severity`/`mode` enums, `finding.verdict` of `clean`/`orphan`/`reported`,
`finding.lifecycle=generated`, missing `path`/`verdict`, malformed
`lockDigest`/`id`/`code`/`reasonCodes`, removed `counts` members, empty
`diagnostics`, and missing/extra diagnostic item members are all rejected. The
three accepted vectors are the unenforced co-presence/conditional statements
recorded in finding 3 — a missing-constraint gap, not an admission of hostile
input.

## Regressions / gate-weakening

None found in the fix-2 delta. No `#[ignore]` was added (the `provider` suite
reports `0 ignored`), no existing assertion was removed or relaxed, no check was
weakened, and no schema was loosened. The gate grew 11 → 12 checks and now
executes three live receipt validations (it performed two before). Test count
grew 12 → 13. `git status` is clean and the probe scripts were kept outside the
checkout.

The blocker is the opposite of gate-weakening — it is a gate made *stricter*
than its inputs can satisfy on a clean runner. That is a real defect, not a
conservative false alarm: I reproduced the exact `drift-captured-missing`
failure and traced the producer/consumer ordering through the workflow file.

## Bottom line

Not landable yet. Both round-2 majors were understood correctly and the enum
correction (M2) is exact. What remains is two instances of the same pattern this
branch has now surfaced twice: a published describing schema that is correct for
the vector its author tested and wrong for the next reachable receipt class,
plus a gate that pins the contract without pinning the *inputs*. The fixes are
small and well-localized — admit the success-reachable severities and correct
the two descriptions, add a classification fixture receipt as a gate vector,
produce-or-commit the captured drift receipt so the gate is self-sufficient, and
tighten the embedded item/bounds toward the governing diagnostic contract.
Everything else in fix round 2 verifies clean.
