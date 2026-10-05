# Issue #34 review round 4 (devin)

Reviewer: independent dispatch (Devin), verification of the fix-round-3
delta only. Branch `ichinya/m7-issue-34`, base `origin/ichinya/M7` =
`9510dd07`, HEAD `7fcce15a` (fix round 3: `d1b605d7` + report `7fcce15a`;
fix-3 delta `c3a8d3fa..7fcce15a`: 7 files, +546/−121). Verified against
the round-3 reports (`issue-34-review3-devin.md` @ `c517582a`,
`issue-34-review3-cline.md` @ `c3a8d3fa`) and the fix report
(`docs/m7/issue-34-fix3.md`). All reproductions used the real binary
rebuilt at this HEAD (`cargo build -p lekalo-cli --locked`, fresh) and
the pinned Ajv 8.17.1 (`%TEMP%\lekalo-ajv-8.17.1\node_modules`; Node
24.13.0, Windows). Probe scripts live outside the checkout
(`C:\Users\User\r4probe\`); nothing in the worktree was modified.

## Verdict: ACCEPT

All three round-3 findings — on which both reviewers independently
converged (1 blocker, 1 major, 1 minor) — are fixed and verified live.
The blocker is resolved by removing the artifact-ordering precondition
entirely (committed golden + a Rust live==golden equality test), not by
reordering CI; the major is resolved by admitting the reachable
error-severity shape without re-severitying the registry; the minor's
schema/wire divergences are corrected in both directions. No gate or
schema was weakened: the gate grew 12 → 15 checks and every expansion
of an accept-set corresponds to a receipt class the binary provably
emits.

## Finding disposition

### 1. BLOCKER (devin-1 / cline-1) — gate required a git-ignored receipt no CI step produced before it ran — FIXED, verified

- `scripts/test-provider-contracts.mjs` no longer references
  `target/provider-receipts/` — confirmed by grep over the gate,
  `.github/workflows/ci.yml`, and `crates/lekalo-cli/tests/provider.rs`
  (zero hits). The `drift-captured-missing` precondition is gone because
  the dependency is gone.
- The reported-receipt vector is now the committed
  `tests/fixtures/provider/drift-reported.golden.json` (26 lines,
  `verdict:"reported"`, one `manual-drift`/`custom` finding). The gate
  validates it against `generate-check-receipt.schema.v0.6.3.json` and
  adds a host-path guard (`/\\/u` rejection on the serialized golden).
- Fidelity is proven, not assumed:
  `drift_reported_receipt_matches_the_published_golden`
  (`crates/lekalo-cli/tests/provider.rs:520`) authors the ownership
  manifest through `GenerateService::inputs` pins, drifts the
  custom-lifecycle file, runs the read-only check, and ends with
  `assert_eq!(receipt, golden)` — the live wire equals the committed
  golden as a JSON value. So the Ajv verdict on the golden is the
  verdict on the wire class, on every clean runner, with no ordering
  constraint between `ci.yml:193` (gate) and `ci.yml:228` (tests).
- Reproduced the reviewer's clean-checkout approximation directly:
  `target/provider-receipts/` does not exist in this worktree, the gate
  still passes all 15 checks (below). The blocker condition — gate
  failing on a clean runner — cannot recur by construction.

### 2. MAJOR (devin-2 / cline-2) — validation-report schema rejected reachable exit-0 error-severity receipts — FIXED, verified live

`contracts/validation-report.schema.v0.6.3.json`: the embedded
diagnostic `severity` is now `enum: ["info","warning","error"]`; the
`diagnostics`/`reasonCodes` bounds are the wire-exact 512 (two
`DiagnosticSet`-bounded sets); `status` stays `const:"valid"`, so
exit-1/3/5 failure envelopes still cannot validate — the accept-set
grew only over reachable success shapes.

Reproduced with the real binary at HEAD, prescribed argv, default
profile, then Ajv 8.17.1 against the published schema:

```text
lekalo validate --no-cache --json --project
  tests/fixtures/classification/invalid/unclosed-policy
  exit=0 status="valid" diag[0].id=classification.kind-rule-missing
  diag[0].severity="error"   -> schema VALID (was INVALID pre-fix)
lekalo validate --no-cache --json --project
  tests/fixtures/classification/invalid/expired-public-grant
  exit=0 status="valid" diag[0].id=classification.expired-declassification
  diag[0].severity="error"   -> schema VALID (was INVALID pre-fix)
```

Both fixtures are now permanent gate vectors: gate section 9 copies
them into its scratch dir, requires exit 0, requires
`diagnostics[0].severity === "error"`, and requires schema validity —
the exact defect class that slipped two rounds can no longer pass
unseen. Prose corrected in step: the schema header and member
descriptions state the three reachable shapes with the strict-profile
contrast; `docs/provider-contract.md` now says "all three reachable
shapes" instead of "both"; the false "(warning/info only)" claims on
`DomainResult::validation`/`Valid` (`crates/lekalo-core/src/result.rs`)
are replaced with the recorded-diagnostics truth. No registry row was
re-severitied — `contracts/diagnostic-registry.v0.4.0.json` is not in
the delta.

### 3. MINOR (devin-3 / cline-3) — embedded diagnostic item and finding bounds diverged from the wire grammar — FIXED, verified

- The embedded `$defs.diagnosticItem` is a faithful projection of
  `contracts/diagnostic.schema.v0.2.16.json` (verified by structural
  diff, not by reading the claim): identical property set (15 keys) and
  identical `required` list; every inner def (`range`, `source`,
  `dataObject`, `location`, `relatedLocations`, `causes`, `fixes`,
  `position`, `logicalPath`, `ruleId`, `diagnosticCode`, `boundedText`,
  `symbolId`, `dataValue`, `providerMetadata`) is carried verbatim under
  a `diagnostic/` `$ref` prefix — the only rewrite is the ref path, and
  the only omitted def is `registryVersion`, unused because the receipt
  pins `registry_version` to `const:"0.4.0"`. Closed `source`
  (logicalPath + range, `minProperties`1, `additionalProperties:false`),
  bounded `data` (16-field named `dataValue` union), closed
  `related_locations` (≤32) / `causes` (≤8) / `fixes` (≤16) items — all
  match the governing grammar, both directions.
- `generate-check-receipt.schema.v0.6.3.json`: `findings.maxItems`
  65536 (the manifest artifact-entry cap); `finding.owner` is the exact
  Model 0.2.16 symbol-id grammar — cross-checked against
  `ir/grammar.rs:11-21,66-74` (`is_segment` ≤63 chars, 2–3 `.`-separated
  segments, total 3–191); `finding.path` drops the 512 cap (the wire
  bounds `ArtifactPath` per segment); `finding.kind` is the closed
  6-value `ArtifactKind` enum matching `artifacts/types.rs:108-115`;
  `path/owner/kind/lifecycle/verdict` and top-level `manifestDigest`
  are required; two `allOf` if/then rules enforce `clean → findings`
  empty and `reported → findings` non-empty.

Ajv vector probe (24 vectors, real receipts as base — `probe-vectors.mjs`):

| Vector | Result |
| --- | --- |
| receipt with `diagnostics` but no `reasonCodes` (and the mirror) | both reject — co-presence enforced |
| diagnostic `id`/`reasonCodes` = `native-gate.policy-missing` (first-segment hyphen) | accepted — governing `ruleId` grammar |
| extra `source` member / `message` 257 chars / `symbol` `Bad-Symbol` | reject / reject / reject |
| `symbol` `planner.focus_task`; severities `info`, `warning` | accepted |
| `related_locations` ×33 / `causes` ×9 | reject (governing bounds 32/8) |
| `severity:"fatal"` | rejects — enum stays closed at the 3 reachable |
| committed golden `reported` receipt | valid |
| `verdict:"clean"` + non-empty findings; `reported` + empty findings | both reject; `clean` + empty findings accepts |
| finding missing `owner`/`kind`/`lifecycle`; `manifestDigest` absent | all reject |
| `kind:"bogus-kind"`; `owner:"A.bad"` | reject (closed enum, grammar) |
| `path` 600 chars | accepted (no total cap, as the wire) |

## Gate weakening check

The only removed gate lines are the captured-receipt block
(`target/provider-receipts` existence/shape/schema checks), replaced by
the committed-golden vector plus a host-path guard; `checks` accounting
went 12 → 15 (+2 classification live vectors, +1 net on the reported
block). No `#[ignore]`, no relaxed assertion, no removed required field
in any schema — expansions are exactly the reachable wire shapes the
round-3 reviews reproduced. `git status` in this worktree is clean
after all runs above; this review commits only its own report.

## Reviewer-run gates (this worktree, Windows, Node 24.13.0, Ajv 8.17.1)

```text
cargo build -p lekalo-cli --locked                                PASS (fresh)
cargo test -p lekalo-cli --test provider --locked                 13/13 PASS
  (drift_reported_receipt_matches_the_published_golden included)
cargo test -p lekalo-cli --test generate --locked                 10/10 PASS
cargo test -p lekalo-cli --test validate_semantic --locked         7/7 PASS
node scripts/test-provider-contracts.mjs                          PASS, 15 checks
  (with target/provider-receipts/ absent — the clean-checkout case)
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 PASS (98 artifacts, 0.6.3)
node scripts/test-contract-versions.mjs                           PASS (6 cases)
node scripts/test-fixture-provenance.mjs                          PASS (64 families)
node scripts/test-diagnostic-contracts.mjs                        PASS (governing schema
  has zero diff vs origin/ichinya/M7 — the projection is embedded, the
  source of truth untouched)
git diff --check origin/ichinya/M7..HEAD                          PASS
```

## Bottom line

Landable. Fix round 3 closes the blocker structurally (the gate no
longer depends on a leftover at all), admits the third reachable
success shape with the registry severities intact, and brings both
receipt schemas to wire-faithful precision — verified against the live
binary, the governing contracts, and a 24-vector closedness probe.
