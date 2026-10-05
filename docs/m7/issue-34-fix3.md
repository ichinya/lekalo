# Issue #34 — fix round 3

Implementation worker: dispatched worker `task_6fc766d02e6b`. Fixes the
round-3 reviews (`issue-34-review3-devin.md` @ `c517582a`,
`issue-34-review3-cline.md` @ `c3a8d3fa`), which independently converged on
**1 blocker + 1 major + 1 minor**. Fix commit: `d1b605d7` (single commit; no
CI workflow change is needed). Branch `ichinya/m7-issue-34`, base
`origin/ichinya/M7` = `9510dd07`.

## Fix strategy in one paragraph

The gate's reported-receipt vector moves from a git-ignored capture to a
committed golden fixture whose live equivalence a Rust test proves on every
run, the validation-report schema admits all three reachable exit-0 shapes
(clean, warning/info-bearing, and default-profile error-finding-bearing) with
the prose corrected in step, and the embedded diagnostic item becomes a
faithful projection of the governing `lekalo/diagnostic/v0.2.16` schema while
the generate-check bounds align with the wire grammars they describe. No
registry row was re-severitied, no gate or test was weakened: the schemas
changed only where the reachable wire required it, and the gate grew
12 → 15 checks.

## Per-finding disposition — `issue-34-review3-devin.md`

### D1. BLOCKER — the gate consumes a receipt no CI step produces before it runs — FIXED

Both reviewers offered three fix directions: produce the receipt inside the
gate, commit a pre-authored receipt, or reorder the producer ahead of the
consumer in `ci.yml`. The committed-golden direction (b) is the one taken,
hardened so it does not lose the live-binary guarantee the capture provided:

- `tests/fixtures/provider/drift-reported.golden.json` is the exact receipt
  the real binary emits for the findings-bearing drift scenario (captured
  once from the producer test at this HEAD, then committed). The gate reads
  it as a committed fixture — the `drift-captured-missing` failure and the
  entire `target/provider-receipts/` dependency are deleted
  (`scripts/test-provider-contracts.mjs`).
- Fidelity is not assumed, it is asserted:
  `drift_reported_receipt_matches_the_published_golden`
  (`crates/lekalo-cli/tests/provider.rs`, renamed from
  `drift_reported_receipt_is_captured_for_the_schema_gate`) still authors the
  manifest through the `GenerateService::inputs` pins, drifts the
  custom-lifecycle file, runs the read-only check, and now requires the live
  receipt to equal the committed golden as a JSON value, field for field. The
  receipt carries no host data (logical paths, content-bound digests, fixed
  serde field order), so the check is deterministic on every runner and the
  golden cannot go stale silently — a contract/product change that moves the
  receipt fails the Rust test loudly instead of passing a frozen vector. The
  capture write is removed; nothing consumes `target/provider-receipts/`
  anymore.
- Chain: Rust test proves the binary emits exactly the committed bytes; the
  gate Ajv-validates exactly those bytes against the published
  `lekalo/generate-check/v0.6.3` schema. Together: the wire class validates,
  on every clean checkout, with no ordering constraint between
  `ci.yml:193` (gate) and `ci.yml:228` (tests) — which is why no workflow
  change is required.
- Proven: the gate passes 15 checks with a stale leftover present, with the
  leftover tree deleted, and (unchanged behavior) on the no-binary path.
  Verified locally by deleting `target/provider-receipts/` entirely and
  re-running — the reviewer's exact clean-checkout approximation.

### D2. MAJOR — the validation-report schema rejects reachable exit-0 `error`-severity receipts — FIXED

Adopted the reviewer's fix direction verbatim — admit the emitted severities,
do **not** re-severity the registry:

- `contracts/validation-report.schema.v0.6.3.json`: the embedded diagnostic
  `severity` enum is now `["info","warning","error"]`. The success envelope's
  bound is tightened to the wire-exact 512 (the model-validation set and the
  recorded classification set are each `DiagnosticSet`-bounded at 256;
  `reasonCodes` is the same-length id projection of `diagnostics`).
- Descriptions corrected in step: the schema header now states the three
  reachable shapes and that default-profile classification rows keep their
  registered `error` severity without invalidating (strict invalidates
  instead — those runs are exit-1 failures outside the receipt); the
  `status` and `diagnostics` member descriptions no longer claim
  "warning/info only".
- `docs/provider-contract.md` (`validate` operation bullet): "covers both
  reachable shapes" → all three shapes, with the strict-profile contrast.
- `crates/lekalo-core/src/result.rs`: the false "(warning/info only)" claims
  on `DomainResult::validation` and on the `Valid` variant are replaced with
  the recorded-diagnostics truth.
- New gate vectors: `tests/fixtures/classification/invalid/unclosed-policy`
  (LEK-CLS-010) and `.../expired-public-grant` (LEK-CLS-007) are copied into
  the gate scratch dir and validated live (exit 0, first diagnostic
  severity `error`, receipt schema-valid) — the exact defect class the two
  prior rounds slipped through with now can no longer pass the gate
  unseen.

### D3. MINOR — the embedded diagnostic item and the finding bounds diverge from the wire grammar — FIXED

- The embedded item is now a **faithful projection** of the governing
  `contracts/diagnostic.schema.v0.2.16.json` (`$defs/diagnostic.*` in the
  receipt schema, same grammars and bounds; projection rather than `$ref`
  keeps each schema standalone under the gate's strict single-document Ajv
  compile): closed `source` (logicalPath + closed range, `minProperties` 1),
  `data` as the bounded named `dataValue` union (16 fields, 2-level nesting,
  64-member lists), closed `metadata` (providerMetadata), and fully checked
  `related_locations` (≤32) / `causes` (≤8) / `fixes` (≤16) items.
  `id`/`message_id`/`reasonCodes` use the governing `ruleId` grammar
  (first-segment hyphens admitted — the receipt pattern previously rejected
  36 of 449 registered rule ids), `symbol` the `symbolId` grammar, `message`
  `boundedText` (≤256). Receipt-level constants stay pinned
  (`schema_version`, `registry_version` 0.4.0).
- `generate-check-receipt.schema.v0.6.3.json`: `findings` bound raised to the
  wire bound 65536 (the manifest's artifact-entry cap,
  `artifacts/canonical.rs:408`); `finding.owner` is the exact Model 0.2.16
  symbol-id grammar (2–3 `[a-z][a-z0-9_]{0,62}` segments, total 3–191,
  `ir/grammar.rs`); `finding.path` drops its 512 cap (the wire `ArtifactPath`
  bounds per segment, not in total); `finding.kind` is the closed
  6-value `ArtifactKind` enum; `owner`/`kind`/`lifecycle` are required on
  every finding (success receipts carry only manifest-entry findings —
  orphans always block, `check.rs` `finish`/`push_finding`) and
  `manifestDigest` is required (always emitted, null when vacuous).
- The two unenforced co-presence statements are now enforced: a
  validation-report receipt with exactly one of `diagnostics`/`reasonCodes`
  is rejected, and a generate-check receipt with `verdict:"clean"` plus a
  non-empty `findings` (or `verdict:"reported"` with an empty one) is
  rejected — the three "accepted" negative vectors from the cline round-3
  closedness probe are all rejections now.

## Per-finding disposition — `issue-34-review3-cline.md`

### C1. BLOCKER — same finding as D1 — FIXED (same change; see D1)

The cline report's "produce-or-commit so the gate is self-sufficient" is
satisfied by the committed golden; the byte-fidelity concern behind
preferring in-gate production is covered by the Rust live-equality test.

### C2. MAJOR — same finding as D2 — FIXED (same change; see D2)

Cline's fixture sweep table is reproduced as a gate requirement: both
exit-0 error-bearing fixtures (`unclosed-policy`, `expired-public-grant`)
are permanent gate vectors, and the five other classification fixtures
remain outside the success contract exactly as the sweep classified them.

### C3. MINOR — same finding as D3 — FIXED (same change; see D3)

Additionally adopted from this report: the `reasonCodes` description no
longer claims "unique" ids (the success serializer maps diagnostics 1:1
without dedup — same order, same length, enforced co-present), and the
`reasonCodes` items use the governing `ruleId` grammar with `maxLength` 128.

## No-gate-weakening / no-schema-weakening statement

- Schemas: the only accept-set expansions are the reachable wire shapes the
  reviews reproduced (severity `error` under the default profile) and the
  governing diagnostic grammar the schema claims to describe (hyphen-first
  rule ids, symbol ids, bounded text). Everything else tightened toward the
  wire: 512-item envelope bound, 256-char messages, closed
  source/data/metadata, checked related/causes/fixes items, required
  entry identity on findings, required `manifestDigest`, enforced
  co-presences. A 35-vector Ajv probe (kept outside the checkout) confirms
  every round-2/3 negative vector still rejects and the round-3 gaps now
  reject.
- Gate: 12 → 15 checks (two classification-finding live vectors, one
  committed reported-receipt vector with a host-path guard); no check
  removed or relaxed; the `drift-captured-*` failure is gone because its
  precondition is gone, not because the requirement was dropped — the
  reported shape is now checked unconditionally from committed bytes.
- Tests: `drift_reported_receipt_matches_the_published_golden` keeps every
  prior assertion (status/operation/mode/verdict/finding shape, exit codes,
  no-side-effect checks elsewhere in the suite) and adds the live==golden
  equality; the only removed statement is the capture write nothing
  consumes. No `#[ignore]`, no relaxed assertion. `git status` clean at the
  fix commit.

## Verification (Windows, Node 24.13.0, Ajv 8.17.1 at the repo-convention path)

```text
cargo build -p lekalo-cli --locked                                  PASS
cargo test -p lekalo-cli --test provider --locked                   13/13 PASS
  (drift_reported_receipt_matches_the_published_golden included)
cargo test -p lekalo-cli --test generate --locked                   10/10 PASS
cargo test -p lekalo-cli --test validate_semantic --locked           7/7 PASS
node scripts/test-provider-contracts.mjs                            PASS, 15 checks
  with stale target/provider-receipts/drift-reported.json present   PASS
  after deleting the leftover tree (clean checkout)                 PASS
Ajv sweep (out-of-checkout probe): all 213 fixture projects run;
  87/87 reachable exit-0 receipts validate against the corrected
  validation-report schema — including unclosed-policy (LEK-CLS-010)
  and expired-public-grant (LEK-CLS-007), severities=[error]; the
  126 exit-1/3/5 fixtures are failure envelopes outside the contract.
Negative/positive vector probe (35 vectors)                         PASS, 0 BAD
cargo fmt --all -- --check                                          PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                           PASS
node scripts/check-contract-versions.mjs --base origin/ichinya/M7   PASS (98 artifacts, 0.6.3)
node scripts/test-contract-versions.mjs                             PASS (6 cases)
node scripts/test-fixture-provenance.mjs                            PASS (64 families)
node scripts/test-diagnostic-contracts.mjs                          PASS (governing schema untouched)
git diff --check origin/ichinya/M7..HEAD                            PASS
```

## Residual known bounds (not defects)

- `validation.rulesEnabled` (≤4096) and the `counts` members keep their
  round-1 bounds; neither review flagged them and no reachable receipt
  approaches them.
- The golden receipt pins the fixture-derived `lockDigest`/`manifestDigest`
  values; a future contract bump that moves the receipt fails the Rust
  equality test loudly, at which point the golden is regenerated
  deliberately (the failure prints both values).
