# Issue #34 review round 4 (codex)

Reviewer: `codex`. Independent, review-only verification of FIX ROUND 3 on
branch `ichinya/m7-issue-34`. Reviewed delta: `c3a8d3fa..7fcce15a`, comprising
implementation `d1b605d74924add966673dae2999df7f94b2729e` and fix report
`7fcce15ad7dde4f2fcb094176d115cfccd244e23`. Actual tested HEAD:
`a612afdbf59a494c11f896783af5db3e27f83271`; its only additional change is
`docs/m7/issue-34-review4-devin.md`, so the implementation and fixtures are
identical to the requested candidate. That round-4 review was not read or
used to establish this verdict.

Read first: `issue-34-fix3.md`, `issue-34-review3-devin.md`, and
`issue-34-review3-cline.md`. Re-derived the dispositions from the actual
delta, governing contracts, CLI emitters, CI ordering, rebuilt binary, and
Ajv 8.17.1. Windows, Node 24.13.0; Ajv resolved from
`C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules` and its exact
version asserted. Review probes and raw evidence remain under ignored
`target/` inside this worktree. No implementation, test, fixture, schema,
workflow, or existing report was edited.

## Verdict: ACCEPT — 0 blockers, 0 major, 0 minor

All three round-3 findings are fixed in the reviewed delta. The provider
gate succeeds with the formerly required capture directory absent, both
reachable error-severity success receipts validate, and the diagnostic
projection and finding bounds agree with their governing contracts. No
regression or gate weakening was found in this delta.

## Findings

None reproduced within the requested FIX ROUND 3 scope.

## Round-3 findings: disposition check

| Finding in both reviews | Claimed | Independently verified |
| --- | --- | --- |
| 1. BLOCKER: gate needs git-ignored `target/provider-receipts` before CI produces it | fixed | **FIXED** — built the binary, asserted the capture directory was absent, and ran the gate before any Rust test invocation in this review: exit 0, 15 checks. It remained absent afterward and after the Rust suites. The consumer now reads a tracked golden; the mandatory provider test compares the real child-process receipt to that golden. |
| 2. MAJOR: validation-report rejects reachable exit-0 `error` diagnostics | fixed | **FIXED** — both named classification fixtures emitted exit 0, `status=valid`, and severity `error`; Ajv 8.17.1 accepted both against the published validation-report schema. Clean and info-bearing fixtures also passed. Registry severities were not changed. Both classification fixtures are now permanent live gate vectors. |
| 3. MINOR: embedded diagnostic item and finding bounds diverge from wire grammar | fixed | **FIXED** — structural comparison of the embedded item and its 15 reused definitions matched the governing diagnostic schema, with the intentional receipt registry pin and redundant closedness keyword accounted for. Finding bounds and required fields match the emitters. Co-presence and contradictory verdict/findings vectors now reject. |

### Gate self-sufficiency and live golden fidelity

`scripts/test-provider-contracts.mjs:350-365` reads
`tests/fixtures/provider/drift-reported.golden.json`, checks the reported
shape and host-path guard, and validates it against the generate-check
schema. `git ls-files --error-unmatch` confirms the golden is committed.
There is no remaining `provider-receipts` or `drift-captured` reference in
the gate or provider test.

`crates/lekalo-cli/tests/provider.rs:520-655` still authors a manifest through
`GenerateService::inputs`, changes the custom-lifecycle artifact, and runs
the real `generate --check` child process. All previous exit, envelope,
lifecycle, and finding-verdict assertions remain. Its new JSON-value
equality assertion at line 653 pins every receipt field to the committed
golden; it replaces the old capture write. This proves semantic JSON
equality, not whitespace equality of the pretty-printed files.

The unchanged CI workflow still builds at line 181, runs the provider gate
at line 193, and runs workspace tests at line 228. That ordering is now
valid: the gate consumes committed data and the later mandatory Rust test
proves its live fidelity. The gate-before-tests reproduction used the
actual absence of `target/provider-receipts`, without deleting or moving
any pre-existing artifact. The existing no-binary path was also exercised
through `LEKALO_BIN` pointing to a nonexistent path: explicit live skip,
exit 0, seven static checks.

### Live error-severity receipts

Each command below used the rebuilt `target/debug/lekalo.exe`, with the
global `--json` position requested by the task:

```text
lekalo --json validate --no-cache --project tests/fixtures/classification/invalid/unclosed-policy
exit=0; stderr empty; status=valid
diagnostics[0]: classification.kind-rule-missing / LEK-CLS-010 / error
reasonCodes: [classification.kind-rule-missing]
Ajv 8.17.1 validation-report.schema.v0.6.3.json: VALID

lekalo --json validate --no-cache --project tests/fixtures/classification/invalid/expired-public-grant
exit=0; stderr empty; status=valid
diagnostics[0]: classification.expired-declassification / LEK-CLS-007 / error
reasonCodes: [classification.expired-declassification]
Ajv 8.17.1 validation-report.schema.v0.6.3.json: VALID
```

The implementation path remains `main.rs:2714-2716` (append recorded
classification diagnostics to model diagnostics), `main.rs:2772-2786`
(only strict invalidates these classification findings), and
`result.rs:448-457` (serialize diagnostics and derive reason codes in the
same order). The schema admits `info|warning|error`; the provider-contract
and `DomainResult` descriptions now describe the error-bearing success
case. The diagnostic registry and governing diagnostic schema have no
changes in the reviewed delta.

### Diagnostic projection and schema closedness

Compiled both receipt schemas and the governing diagnostic schema with
`new Ajv2020({strict: true, allErrors: true})`. Deep comparison normalized
the embedded local reference prefix and ignored descriptions. All 15
reused diagnostic definitions matched. The root item matched after:

- Accounting for `registry_version: {const: "0.4.0"}`, already pinned by
  this receipt, in place of the governing generic registry-version grammar.
  Its now-unused `registryVersion` definition is consequently not copied.
- Accounting for omitted `unevaluatedProperties:false`: the item's
  `additionalProperties:false` and full property declaration already close
  the same object.

Thus source/range/position, named data values, provider metadata,
related-location items, cause items, fix items, symbols, messages, rule
ids, required members, and their bounds have matching constraints. All
449 registered rule ids satisfy the embedded grammar, including all 36
ids with a hyphen in the first segment.

Independent mutation probes: **108 negative vectors rejected, 14 positive
vectors accepted, 0 unexpected outcomes**. These are synthetic schema
vectors based on live receipts and the committed reported golden; boundary
acceptance is not a claim that a 65536-artifact project was generated live.
Coverage includes:

- `diagnostics` without `reasonCodes`, the inverse, and empty arrays;
  `clean` plus non-empty findings, and `reported` plus empty findings.
- Extra envelope/report/count/diagnostic/finding members; missing required
  diagnostic and finding fields; wrong pins, enums, ids, codes, and digests.
- Empty/extra source members, absolute/parent source paths, malformed ranges
  and positions, invalid data keys/types/depth/list sizes, invalid provider
  metadata, and unchecked or extra related/cause/fix members.
- Rejection above message/symbol/data/list/related/cause/fix/envelope bounds;
  acceptance at message 256, diagnostic symbol 256, data fields 16, list 64,
  related locations 32, causes 8, fixes 16, and diagnostics/reasonCodes 512.
- Generate-check rejects unknown kinds, generated lifecycle, non-finding
  verdicts, malformed owners, and missing entry identity or manifest digest.
  It accepts the 191-character owner and a 639-character logical path,
  accepts 65536 findings, and rejects 65537.

The generate-check bounds were also checked against `artifacts/canonical.rs:408`
(65536 entries), `ir/grammar.rs:69-80` (two or three 1-63-character symbol
segments, total 3-191), `artifacts/types.rs:71-99` (artifact paths bounded
per segment, without a total cap), and `artifacts/check.rs:300-431`
(one finding per manifest entry; generated and orphan findings block).
The 512-item validation-envelope cap matches the append of two separately
256-bounded diagnostic sets, rather than imposing a single-set cap:
`validator/report.rs:29` stores a `DiagnosticSet`, classification findings
use the same constructor (`classification/mod.rs:89`), and
`diagnostics/normalize.rs:174-175` rejects a set above the limit defined at
`diagnostics/types.rs:15`.

## Reviewer-verified gates

```text
cargo build -p lekalo-cli --locked                                  PASS
cargo test -p lekalo-cli --test provider --test generate
  --test validate_semantic --locked                                 PASS
  provider: 13/13; generate: 10/10; validate_semantic: 7/7
  all suites: 0 failed, 0 ignored, 0 filtered out
node scripts/test-provider-contracts.mjs                            PASS, 15 checks
  capture directory absent; executed before Rust tests
node scripts/test-provider-contracts.mjs (nonexistent LEKALO_BIN)     PASS, 7 checks + explicit live skip
live clean/info/error receipts + pinned Ajv                         PASS, 4/4
diagnostic projection comparison                                   PASS
schema mutation/boundary probes                                    PASS, 108 negative + 14 positive
cargo fmt --all -- --check                                          PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                           PASS
node scripts/test-diagnostic-contracts.mjs                          PASS
node scripts/check-contract-versions.mjs --base origin/ichinya/M7    PASS, 98 artifacts, product 0.6.3
node scripts/test-contract-versions.mjs                             PASS, 6 cases
node scripts/test-fixture-provenance.mjs                            PASS, 64 families
git diff --check c3a8d3fa..HEAD                                      PASS
git diff --check origin/ichinya/M7..HEAD                            PASS
git status --porcelain=v1 (before authoring this report)             clean
```

For the two checks naming `origin/ichinya/M7`, the local ref at review time
was `87c27d86d3fd2936586f32d95f9d0f9e1bcc7817`, rather than the older
`9510dd07` recorded in the fix report. The requested review delta remains
explicitly bounded by `c3a8d3fa` and `7fcce15a`; no fetch or branch change
was performed.

## Regressions / gate-weakening

None found. The implementation delta changes two describing schemas, one
provider test, one gate, documentation/comments, and adds one golden.
CI, the diagnostic registry, the governing diagnostic schema, and the
runtime validation/generation behavior are unchanged. No `#[ignore]` or
relaxed existing assertion was introduced. The gate retains discovery,
schema-pin/digest, clean/info receipt, no-cache, and clean-drift checks,
adds the two live error-bearing vectors, and replaces the ignored capture
with a tracked golden backed by mandatory live JSON equality. Its reported
check count increases from 12 to 15.

Schema acceptance expands only to the severities, grammars, and bounds the
governing wire permits; nested closedness, required entry identity, envelope
bounds, and co-presence become stricter. The negative vectors confirm the
round-3 missing constraints now reject without relaxing the existing closed
envelopes.

Acceptance applies to this FIX ROUND 3 delta and the reproduced local
evidence. Remote CI and Linux/macOS execution were not run in this review.
The fix report's claimed 213-project sweep was not repeated; the four live
validation fixtures and the independent schema probes above are the evidence
for this verdict. Only this review report is committed; no push is made.
