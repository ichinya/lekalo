# Issue #34 review round 3 (devin)

Reviewer: dispatched worker `task_5d84af6e13ec`. Focused verification of the
fix-2 delta only. Branch `ichinya/m7-issue-34`, base `origin/ichinya/M7` =
`9510dd07`, HEAD `b4aea3e2` (fix round 2: `4b8a6994` + `a906e7c8` +
`b4aea3e2`). Verified against the round-2 reports
(`issue-34-review2-devin.md`, `issue-34-review2-cline.md`) and the fix
report (`issue-34-fix2.md`). All reproductions used the real binary built
at this HEAD (`cargo build -p lekalo-cli`) and the pinned Ajv 8.17.1 this
gate uses (`LEKALO_AJV_NODE_PATH` convention; Node 24.13.0, Windows).

A concurrent codex re-review of this same candidate exists as uncommitted
worktree content (`docs/m7/issue-34-review-codex.md`, not committed at
HEAD). Its conclusions were not used to establish findings; every defect
below was independently reproduced from the code, the workflow file, the
binary, and Ajv. Where the verdicts converge I note it.

## Verdict: ISSUES — 1 blocker, 1 major, 1 minor

Fix round 2 lands real corrections: both published schemas now describe
the shapes the round-2 reviews pinned (the warning/info-bearing validate
receipt and the `verdict:"reported"` drift receipt both validate; the
finding enums match `check.rs`/`types.rs` exactly), the minors landed,
and every claimed gate passes locally. But two defects of the same class
remain — one breaks clean CI outright, and the validation-report schema
still rejects a second reachable exit-0 receipt class that neither new
gate vector exercises.

## Findings

### 1. BLOCKER — the new gate consumes a receipt no CI step produces before it runs

`scripts/test-provider-contracts.mjs:300-317`: whenever
`target/debug/lekalo` exists, the gate now **requires**
`target/provider-receipts/drift-reported.json` and fails with
`drift-captured-missing` when it is absent. The file's only producer is
the new Rust test `drift_reported_receipt_is_captured_for_the_schema_gate`
(`crates/lekalo-cli/tests/provider.rs`, writes the capture at the test's
end). `.gitignore:1` (`/target/`) — confirmed via `git check-ignore` — so
the capture is never committed and never present on a clean checkout.

CI ordering in `.github/workflows/ci.yml`, job `build-test` (matrix
ubuntu/windows/macos, no cargo or `target/` caching — only
`package-manager-cache: false`):

```text
:181  cargo build --workspace --locked        -> target/debug/lekalo exists
:193  node scripts/test-provider-contracts.mjs -> binaryAvailable=true,
      captured receipt ABSENT -> fail("drift-captured-missing") -> exit 1
:228  cargo test --workspace --locked          -> the producer test runs
      here, after the gate already failed
```

So every clean `build-test` run on all three OSes fails at :193; the
producer first executes at :228. (The `schemas` job at :80 is unaffected
only because it never builds — the gate skips the live block.) The gate
passes locally in this worktree solely because a leftover capture exists
from an earlier `cargo test` — exactly the masking the design creates.
Established by reading the gate script, the workflow, and `.gitignore`;
consistent with the local observation that deleting the capture and
re-running the gate without running the test first reproduces
`drift-captured-missing`.

Fix direction: produce the receipt inside the gate (it already spawns
`lock` + `generate --check` for the clean vector) or run the producer
before the consumer in CI; the captured file must not be a
leftover-dependent precondition.

### 2. MAJOR — `validation-report` schema still rejects a reachable exit-0 receipt class: `error`-severity findings under the default profile

`contracts/validation-report.schema.v0.6.3.json:59` restricts diagnostic
`severity` to `["info","warning"]`. But the default-profile `validate`
run deliberately records classification-review findings into the success
envelope **without invalidating**:

- `crates/lekalo-cli/src/main.rs:2772-2785` — `if strict &&
  outcome.invalid { invalid }`; otherwise `recorded =
  findings_set(&outcome)` rides the success diagnostics
  ("the findings ride the success diagnostics").
- `main.rs:2714-2716` — `diagnostics.extend(recorded)` then
  `DomainResult::validation(...)`, whose `result.rs:224` doc comment
  claims "warning/info only" — unenforced, and false on this path.
- `classification/mod.rs:74-91` `findings_set` builds each row through
  `diagnostics::normalize::build`, which takes
  `entry.default_severity()` unconditionally
  (`diagnostics/normalize.rs:67`). Every `classification.*` finding rule
  is registered `default_severity: "error"`
  (`contracts/diagnostic-registry.v0.4.0.json`: LEK-CLS-006/007/008/010/
  011/012, e.g. `classification.kind-rule-missing` at `:1019-1041`,
  `classification.expired-declassification` at `:970-993`).

Reproduced end-to-end with the real binary at HEAD (prescribed argv,
default profile):

```text
target\debug\lekalo.exe validate --no-cache --json --project
  tests/fixtures/classification/invalid/unclosed-policy
EXIT=0  status="valid"
diagnostics[0]: id=classification.kind-rule-missing  code=LEK-CLS-010
                severity="error"   reasonCodes=[classification.kind-rule-missing]

target\debug\lekalo.exe validate --no-cache --json --project
  tests/fixtures/classification/invalid/expired-public-grant
EXIT=0  status="valid"  diagnostics[0].severity="error" (LEK-CLS-007)
```

Ajv 8.17.1 against the published schema — the **only** violation is the
severity enum:

```text
unclosed-policy:      valid=false  /diagnostics/0/severity enum
                      allowedValues ["info","warning"]
expired-public-grant: valid=false  /diagnostics/0/severity enum
```

This is the same defect class as round-2 finding 1: an ordinary,
prescribed-argv, exit-0 receipt that the documented exact output-schema
negotiation rejects. The two new gate vectors cover the zero-diagnostic
and info-bearing shapes only — neither exercises a classification
finding, so the defect passes the gate exactly like its predecessor did.

The prose is wrong in step with the enum: the schema description (line
5) asserts "Error findings make the run `invalid` (exit 1, stderr)",
and `docs/provider-contract.md` now claims the schema "covers both
reachable shapes" — there are three: clean, info/warning-bearing, and
default-profile error-finding-bearing.

Fix direction: admit the success-reachable severities the implementation
actually emits on this envelope (the default profile's recorded
classification findings keep their registered `error` severity — do not
re-severity them to satisfy the schema), correct both descriptions, and
add a classification-bearing fixture receipt as a gate vector (the
fixtures already exist in `tests/fixtures/classification/invalid/`).

### 3. MINOR — the embedded diagnostic item and the finding bounds diverge from the wire grammar they advertise (both directions)

The receipt schema's diagnostic item claims to be "the closed
`lekalo/diagnostic/v0.2.16` item" but is neither a reuse nor a faithful
projection of `contracts/diagnostic.schema.v0.2.16.json`:

- **Looser than the governing contract** (admits non-wire shapes):
  `source`/`data`/`metadata` are bare `{"type":"object"}` and
  `related_locations`/`causes`/`fixes` items are unchecked, while the
  governing schema closes `source` (logicalPath + range,
  `additionalProperties:false`), bounds `data` to named typed values
  (16 properties, `dataValue` union), constrains `metadata`
  (providerMetadata) and closes the three array item shapes. `symbol`
  is a bare string vs the `symbolId` pattern (≤256); `message`
  maxLength 512 vs `boundedText` 256; `id`/`message_id`/`reasonCodes`
  items lack the `ruleId` maxLength 128.
- **Tighter than the wire grammar** (risks rejecting reachable items):
  `id`/`reasonCodes` pattern `^[a-z][a-z0-9]*(\.[a-z][a-z0-9-]*)+$`
  forbids first-segment hyphens the governing `ruleId` admits
  (`^[a-z][a-z0-9-]*...`) — registered ids like `native-gate.*`,
  `storage-engine.*`, `target-profile.*` exist (not currently emitted on
  the validate path, but the schema advertises the closed diagnostic
  grammar); `related_locations` maxItems 16 < contract 32; `causes`
  maxItems 16 > contract 8.

The `generate-check` schema has the same precision gaps:

- `findings` maxItems 1024 < the manifest's ≤65536 artifact entries
  (`crates/lekalo-core/src/artifacts/canonical.rs:408`) — a `reported`
  receipt can legally exceed it.
- `finding.owner` maxLength 128 < the symbol-id bound 191
  (`crates/lekalo-core/src/ir/grammar.rs:69`); `finding.path` maxLength
  512 < `ArtifactPath` (per-segment ≤64, no total bound,
  `artifacts/types.rs:71-99`).
- `finding.kind` is a free ≤64 string where the wire emits the closed
  6-value `ArtifactKind` enum; `owner`/`kind`/`lifecycle` and top-level
  `manifestDigest` are optional though `DriftFinding::entry`/`CheckReceipt`
  always emit them; the `diagnostics`↔`reasonCodes` co-presence the
  description states is unenforced.

None of these reject today's ordinary receipts (finding 2 is the one
that does); they are the same "schema ≠ wire truth" defect class in the
lax/bound-mismatch direction and contradict the closed-item claim the
schema itself makes.

## Round-2 findings: disposition check

| Finding | Claimed | Verified |
| --- | --- | --- |
| M1 validation-report rejects warning/info receipts | fixed | PARTIAL — the warned shape now validates (reproduced), but the schema still rejects the error-severity shape reachable under the default profile (finding 2). The nested/bounds precision is finding 3. |
| M2 generate-check finding enums inverted | fixed | FIXED for the enums — `verdict:["stale","manual-drift","missing"]`, `lifecycle:["scaffolded","checked","external","custom"]` match `check.rs:351-405` + `types.rs:586-596,190-199` exactly; captured `verdict:"reported"` receipt validates; orphan/generated blocking now stated correctly in schema and `provider-contract.md:172-179`. Bounds/precision residuals in finding 3. |
| Devin minor (dead `RuntimeMetadata`, stale `read-or-check` doc) | fixed | FIXED — `operations.rs` enum is back to `ReadOnly`/`GeneratedArtifacts`, module doc corrected, `provider-capabilities` `effectClass` enum shrunk to `["read-only","generated-artifacts"]`. |
| Cline minor (`drift` lock requirement understated) | fixed | FIXED — `provider-contract.md:108` now states `project + lock (unconditional...)`; `check.rs:57-60` confirms `LockState::Absent -> LockFailure::Missing` unconditionally. |

## Reviewer-verified gates (this worktree, Windows, Node 24.13.0, Ajv 8.17.1)

```text
cargo build -p lekalo-cli                                     PASS (HEAD)
cargo test -p lekalo-cli --test provider --locked             13/13 PASS
  (incl. drift_reported_receipt_is_captured_for_the_schema_gate —
   the blocker is the CI ordering of its artifact, not the test)
cargo test -p lekalo-cli --test generate --locked             10/10 PASS
cargo test -p lekalo-cli --test validate_semantic --locked     7/7 PASS
node scripts/test-provider-contracts.mjs                      PASS, 12 checks
  (locally; passes only because the leftover capture exists — finding 1)
cargo fmt --all -- --check                                    PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                     PASS
node scripts/check-contract-versions.mjs --base origin/ichinya/M7
                                                              PASS (98 artifacts, 0.6.3)
git diff --check origin/ichinya/M7..HEAD                      PASS
```

Receipt validations (real binary, pinned Ajv): warning receipt valid,
clean receipt valid, captured `verdict:"reported"` drift receipt valid,
`unclosed-policy`/`expired-public-grant` `valid` receipts REJECTED at
`/diagnostics/0/severity` (finding 2). Schema closedness: 12 negative
vectors all reject (extra members, wrong status/verdict/lifecycle/severity
enums, missing required fields, empty `diagnostics`) — the schemas were
corrected, not loosened.

## Regressions / gate-weakening

No `#[ignore]`, no relaxed assertion, no weakened existing check in the
fix-2 delta: the gate grew 11 -> 12 checks and the two describing schemas
only changed where the wire required it. The blocker is a new artifact
dependency the gate introduced without ordering its producer, not a
weakening of an existing gate.

`git status`: clean except the parallel reviewer's uncommitted
`docs/m7/issue-34-review-codex.md` (pre-existing worktree content, not
part of the reviewed delta); this review commits only its own report.

## Bottom line

Not landable yet. The enum corrections both verify, but the fix ships a
hard CI-ordering defect (the gate's captured-receipt precondition is
produced only by a test that runs later — or never — on every clean
runner) and the validation-report schema still fails a second reachable
exit-0 class: default-profile runs that record `error`-severity
classification findings, reproduced on two shipped fixtures with the
prescribed argv. The minor item/bounds precision rides along. The fixes
are small: admit the reachable severities and re-point the descriptions,
make the captured receipt produced-or-ordered before its consumer, and
tighten the embedded item/bounds toward the governing diagnostic
contract.
