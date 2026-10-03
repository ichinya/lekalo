# Issue #34 fix round 2

The devin round-2 review (`issue-34-review2-devin.md`: 2 major,
1 minor) returned ISSUES, and the cline round-2 review
(`issue-34-review2-cline.md`: 0 blocker, 2 major, 1 minor — delivered
mid-round) confirmed both devin majors independently and added one
new minor. Every finding was reproduced against the real binary before
fixing. Dispositions: all **fixed**; none rebutted. No gate, test, or
schema was weakened — both receipt schemas became strictly more
truthful (they now accept reachable shapes they previously rejected
and still reject everything else).

## M1 (MAJOR) — `validation-report` schema rejects warning/info-bearing success receipts — FIXED

Reproduced first: `lekalo validate --no-cache --json --project
tests/fixtures/validation/warning/portable-target-reference` exits 0
with `status:"valid"` plus top-level `diagnostics` (one `info` item,
`LEK-SEM-012`) and `reasonCodes` — Ajv rejected both members under the
round-1 schema (`additionalProperties` violations), exactly as
reported.

Fix: `contracts/validation-report.schema.v0.6.3.json` now declares the
optional `diagnostics` array (closed items: `schema_version` const
`lekalo/diagnostic/v0.2.16`, `registry_version` const `0.4.0`,
dotted-id/`LEK-`-code patterns, severity restricted to the
success-reachable `info|warning`, closed category enum, bounded
optional `symbol`/`source`, required `data`/`related_locations`/
`causes`/`fixes`/`metadata`) and the derived `reasonCodes` array
(dotted rule-id pattern). Verified on both reachable exit-0 shapes
with the pinned Ajv 8.17.1 against the live binary:

```text
warning receipt valid: true   (warning/portable-target-reference; diagnostics+reasonCodes present)
clean receipt valid: true     (valid/base; both members absent)
```

The description now states the exact presence rule (present-together,
non-empty, absent on the zero-diagnostic shape). Gate vector added
(check 9b): the live warning fixture receipt must carry non-empty
`diagnostics` and validate — the gate fails if the shape regresses in
either direction.

## M2 (MAJOR) — `generate-check` finding enums inverted vs the emitted wire — FIXED

Verified the reviewer's code analysis directly: `check.rs`
`push_finding` puts a finding in the receipt only for `stale`,
`manual-drift`, `missing` on **non-generated** lifecycles; orphans
always go to `blocking` (exit 1); `clean` entries are only counted;
`reported` is a receipt-level verdict, never a finding verdict; and
`Lifecycle::as_str` yields exactly
`generated|scaffolded|checked|external|custom`, with `generated`
always blocking.

Fix: `contracts/generate-check-receipt.schema.v0.6.3.json` —
`finding.verdict` enum is now `["stale","manual-drift","missing"]`,
`finding.lifecycle` is now
`["scaffolded","checked","external","custom"]`, and both the schema
description and the contract doc's blocking rule were corrected to the
code's actual rule (orphans/generated block; the other three report).

To prove it on a **real** receipt (not a hand-written one — a
hand-authored ownership manifest proved too fragile to canonicalize
outside Rust), a new Rust child-process test
(`drift_reported_receipt_is_captured_for_the_schema_gate`) authors the
manifest through the core `GenerateService::inputs` pins exactly the
way `generate.rs` does, drifts a `custom`-lifecycle file, runs the real
`generate --check --json` (exit 0, `verdict:"reported"`), asserts the
wire values (`findings[0].lifecycle:"custom"`,
`findings[0].verdict:"manual-drift"`), and captures the receipt under
the git-ignored `target/provider-receipts/`. The Node gate now
requires and validates that captured receipt:

```text
reported drift receipt valid: true
```

## Minor — dead `RuntimeMetadata` variant + stale `read-or-check` module doc — FIXED (trivially)

The module doc's `read-or-check` bullet is gone (the drift variant is
described under `read-only`, which is its actual class), and the unused
`EffectClass::RuntimeMetadata` variant is removed — the enum is back
to the two classes every operation uses, and the schema's
`effectClass` enum shrinks to `["read-only","generated-artifacts"]`
accordingly.

## Cline findings — cross-disposition

### Cline-1 (MAJOR, confirms devin-1) — `validation-report` rejects warning/info receipts — FIXED

Same defect, fixed by the same M1 correction above. Cline's gate-gap
observation (the gate never exercised a non-empty receipt) is also
addressed: the gate now runs the warning fixture as a first-class
vector and fails on both a missing `diagnostics` member and a schema
violation.

### Cline-2 (MAJOR, confirms devin-2) — `generate-check` finding enums — FIXED

Same defect, fixed by the same M2 correction above. Cline's sharpening
is adopted in the wording: the round-1 description did not merely
misstate — it encoded a blocking model the implementation contradicts
while simultaneously counting `manualDrift` in its own `counts`. The
corrected descriptions state the code's rule. The gate-gap observation
is likewise addressed via the captured `verdict:"reported"` receipt
vector, which exercises the `finding` subschema end to end.

### Cline-3 (MINOR, new) — `drift` requirement understates the unconditional lock — FIXED

Reproduced: with `lekalo.lock` deleted and **no** `--locked`, a valid
project fails `generate --check` with exit 1, `lock.missing` —
`Prepared::prepare` refuses an absent lock unconditionally
(`check.rs:57-60`). The contract's operation table now states
`project + lock (unconditional: a missing lekalo.lock fails with
lock.missing; --locked only adds the freshness check)`.

## Round-1 disposition re-check

All seven round-1 findings remain fixed (write-scope text, pin
separation, no command strings, frozen `productVersion`, argv subset,
`--no-cache`, drift operation). Round 2's two majors were residual
defects in the round-1 schemas, both now corrected; nothing regressed.

## Verification (all re-run in this worktree after the fixes)

```text
cargo fmt --all -- --check                                        PASS
cargo clippy -p lekalo-cli -p lekalo-core --all-targets
  --locked -- -D warnings                                         PASS
cargo test -p lekalo-cli -p lekalo-core --locked                  91 suites, 0 failed
  (provider CLI now 13 tests incl. the receipt-capture test)
node scripts/test-provider-contracts.mjs (Ajv 8.17.1)             PASS, 12 checks
  (added: live warning-bearing validate receipt; captured reported
   drift receipt — both against the corrected schemas)
node scripts/check-contract-versions.mjs --base HEAD              PASS (98 artifacts)
node scripts/check-contract-versions.mjs --base origin/ichinya/M7 PASS (98 artifacts)
node scripts/test-contract-versions.mjs                           PASS (6 cases)
node scripts/test-fixture-provenance.mjs                          PASS (64 families)
```

Contract-version policy: the two describing schemas keep version
`0.6.3` — they were published this same product generation and have
never been consumed by a released consumer, so the corrections land
inside their original version per `docs/versioning.md` (the product
version of the implementation commit; the fix commits are part of that
same implementation series).

Commits: `141512fc` (schemas + gate vectors + capture test + enum
cleanup + contract-doc corrections) plus this report. Branch pushed to
origin.

## Rebuttals

None. Both devin majors reproduced exactly as reported; the devin
minor was verified by inspection; the cline review confirmed both
majors independently and its new minor reproduced verbatim
(`lock.missing` exit 1 without `--locked`).
