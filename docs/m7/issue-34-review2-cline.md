# Issue #34 review round 2 (cline)

Reviewer: dispatched worker `cline`, dispatch `ctx_2e44646822a2`. Review-only;
no implementation, test, or fixture files were modified. Branch
`ichinya/m7-issue-34`, base `origin/ichinya/M7` = `9510dd07`, HEAD `a7075590`
(fix round 1: `31d22945` + `ba3586a5` + `a7075590`).

Read in full: both round-1 reports (`issue-34-review-devin.md`,
`issue-34-review-codex.md`), the fix report (`issue-34-fix1.md`), and the peer
round-2 report `issue-34-review2-devin.md` (0 blocker / 2 major / 1 minor) so
this report does not restate its work.

## Verdict: ISSUES вЂ” 0 blocker, 2 major, 1 minor

I independently reproduce both of devin's majors against the real binary and
the pinned Ajv 8.17.1, and they are confirmed and still open. I add one minor
of my own (a doc/argv-accuracy defect devin did not report) and sharpen both
majors with the precise mechanism.

Fix round 1 did land real, verifiable improvements: the write-scope text is
truthful in all three places, `validate` is repinned from the profile schema
to a published describing schema, `drift` is a real negotiated read-only
operation, the prescribed `validate` argv carries `--no-cache`, and the
manifest schema is genuinely tightened (Codex-5's five mutation vectors all
fail even with recomputed self-digests). No blockers. But the two newly
published describing schemas are still wrong on the reachable non-empty exit-0
receipts вЂ” the exact defect class both round-1 majors flagged, and CI does not
catch it because the gate only exercises the zero-finding vectors.

## Remaining findings

### 1. MAJOR (confirmed, devin-1) вЂ” `validation-report` schema rejects every warning/info-bearing success receipt

`contracts/validation-report.schema.v0.6.3.json` closes the top level with
`"additionalProperties": false` (`:7`) and declares only `status`,
`modelVersion`, `validation` (`:8-13`). But `DomainResult::to_json_string`
appends top-level `diagnostics` **and** `reasonCodes` to any `Valid` envelope
whose diagnostics are non-empty (`crates/lekalo-core/src/result.rs:441-457`),
and `lekalo validate` returns through `DomainResult::validation`
(`crates/lekalo-cli/src/main.rs:2714-2716`) carrying the report's warning/info
findings plus the classification-review records.

Reproduced with the real binary вЂ” default profile, one info finding:

```text
target\debug\lekalo.exe validate --no-cache --project
  tests\fixtures\validation\warning\portable-target-reference --json
EXIT=0   stderr empty
status=valid  diagnostics=1  reasonCodes=1
```

Ajv 8.17.1 against the published schema:

```text
warnings-receipt-valid: false
   /  must NOT have additional properties   (diagnostics)
   /  must NOT have additional properties   (reasonCodes)
```

The schema's own `description` concedes the members exist вЂ” "Warning/info
validation findings ride the envelope's `diagnostics` array (closed
`lekalo/diagnostic/v0.2.16` items)" вЂ” while `additionalProperties: false`
forbids them. An existing CLI test asserts exactly this wire shape
(`crates/lekalo-cli/tests/validate_semantic.rs:144-172`: default profile,
exit 0, `document["diagnostics"][0]`).

This is not an edge case: a default-profile run that records any
warning/info finding вЂ” a normal outcome вЂ” yields a receipt the documented
exact output-schema negotiation rejects.

**Gate gap:** `scripts/test-provider-contracts.mjs` validates only
`tests/fixtures/validation/valid/base` (`:223`), which carries zero
diagnostics. The strings `findings`, `diagnostics`, and `reasonCodes` do not
appear in the gate script at all, so no non-empty receipt is ever checked.

Fix direction: declare optional `diagnostics` (bounded array of the closed
`lekalo/diagnostic/v0.2.16` document) and `reasonCodes` (array of rule ids)
on the receipt schema, and add the already-existing
`tests/fixtures/validation/warning/portable-target-reference` fixture as a
gate vector.

### 2. MAJOR (confirmed, devin-2) вЂ” `generate-check` schema's `finding` enums exclude two values the wire actually emits

`contracts/generate-check-receipt.schema.v0.6.3.json` closes the finding
verdict to `["orphan", "clean", "reported"]` (`:105-108`) and lifecycle to
`["generated", "scaffolded"]` (`:100-104`). The real wire emits neither for
the `custom`-drift case.

The repo's own **passing** test pins the exact exit-0 values:

```rust
// crates/lekalo-cli/tests/generate.rs:476-488
assert_eq!(exit_code(&output), 0, "custom drift never blocks: ...");
assert_eq!(document["verdict"], "reported");
assert_eq!(document["counts"]["manualDrift"], 1);
assert_eq!(document["findings"][0]["lifecycle"], "custom");     // not in the enum
assert_eq!(document["findings"][0]["verdict"], "manual-drift"); // not in the enum
```

The emitters confirm both are first-class wire values, not typos:
`Lifecycle::Custom => "custom"` (`crates/lekalo-core/src/artifacts/types.rs:197`)
and `DriftVerdict::ManualDrift => "manual-drift"`
(`crates/lekalo-core/src/artifacts/types.rs:591`).

Validating a receipt with exactly those asserted values:

```text
drift-reported-receipt-valid: false
   /  /findings/0/lifecycle  must be equal to one of the allowed values
   /  /findings/0/verdict    must be equal to one of the allowed values
```

Sharpening devin's account: the schema description at `:106` ("only `orphan`
may appear as reported-only; stale/manual-drift/missing block") is not merely
misstated wording вЂ” it encodes a **blocking** model that the implementation
contradicts. `generate.rs:479` asserts the opposite in its own assertion
message ("custom drift never blocks"), and `counts.manualDrift` is a
first-class member of the schema's own `counts` (`:70`) while `verdict` is
`"reported"` вЂ” so the schema simultaneously models `manualDrift` as a
countable outcome and forbids it from ever appearing in a finding. The
correct shape is that `lifecycle` admits `custom` and the non-blocking
`verdict` enum admits `manual-drift`.

**Gate gap:** the gate's drift vector has `findings: []`, so the `finding`
subschema вЂ” including both enums вЂ” is **never exercised**. This is why the
defect passes CI despite a passing unit test asserting the opposite values.

### 3. MINOR (new вЂ” doc/argv accuracy) вЂ” `provider-contract.md` understates the unconditional `lekalo.lock` requirement for `drift`

`docs/provider-contract.md:108` documents the `drift` operation's requirements
as:

```text
| `drift` | `lekalo generate --check [--locked] [--project DIR]` | read-only |
  `lekalo/generate-check/v0.6.3` | project (lock for `--locked`) |
```

"lock for `--locked`" reads as though the lock is only needed when
`--locked` is passed. It is not optional at all: `Prepared::prepare` returns
`LockFailure::Missing` unconditionally when the lock is absent
(`crates/lekalo-core/src/artifacts/check.rs:57-60`):

```rust
let lock = match LockService::read_state_at(&root)? {
    LockState::Present(lock) => lock,
    LockState::Absent => return Err(ArtifactFailure::Lock(LockFailure::Missing)),
};
```

Verified end-to-end against the real binary вЂ” with a valid project and
**without** `--locked`, deleting `lekalo.lock` yields:

```text
EXIT=1   reasonCodes=["lock.missing"]
```

So a consumer following the documented recipe verbatim on a fresh checkout
gets a failed run. The manifest itself declares only `requiresProject` /
`requiresAdapter` and says nothing about the lock, so the argv table is the
only place a consumer learns the precondition вЂ” and it understates it.

Fix direction: change the requirement cell to `project + lock` (unconditional),
keeping `[--locked]` in the argv column as the freshness/staleness check.

## Round-1 disposition audit

Every round-1 finding has a disposition in `issue-34-fix1.md`; I re-verified
each independently.

| Round-1 finding | Claimed | This review |
|---|---|---|
| Devin-1/2/3 (validate pin + non-empty receipt shape) | fixed | **PARTIAL** — repinned to a real published schema, but it still rejects every warning/info-bearing exit-0 receipt (finding 1). |
| Devin-4 (`productVersion` frozen at 0.6.3) | fixed | FIXED — truthful in `version.rs:52-58`, `manifest.rs:81-84`, schema `:409-412`, contract doc `:83`. |
| Devin-5 (argv subset / readiness `model` / drift adapter) | fixed | FIXED — recipes marked a provider-relevant subset; `drift` is a separate `requiresAdapter:false` operation. My finding 3 is an adjacent, distinct doc inaccuracy. |
| Codex-1 (validate receipt schema) | fixed | Same underlying defect as finding 1. |
| Codex-2 (read-only `validate` writes cache) | fixed | FIXED — prescribed argv carries `--no-cache`; new test `prescribed_validate_argv_with_no_cache_writes_nothing` plus gate check 9 enforce zero writes. Re-ran: PASS. |
| Codex-3 (drift-check has no negotiated contract) | fixed | **PARTIAL** — the operation, pin, and read-only semantics exist and are tested, but the schema rejects the drift receipts the implementation emits (finding 2). |
| Codex-5 (schema doesn't enforce declared invariants) | fixed | FIXED — per-operation const tuples at canonical positions, exact sorted 9-pin tuple; all five mutation vectors rejected with recomputed digests. |

## Reviewer-verified gates (this worktree, Windows, Node 24.13.0, Ajv 8.17.1)

```text
cargo test -p lekalo-cli --test provider --locked            12/12 PASS
cargo test -p lekalo-core provider:: --locked                21/21 PASS
cargo test -p lekalo-cli --test generate --locked            PASS (pins the finding wire)
cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
                                                                  PASS
cargo fmt --all -- --check                                   PASS
node scripts/test-provider-contracts.mjs                     PASS, 11 checks
node scripts/check-contract-versions.mjs --base origin/ichinya/M7
                                                                  PASS
git diff --check origin/ichinya/M7..HEAD                     PASS
git status --porcelain                                       clean
```

Devin's claimed gate outputs are plausible and reproduce. The two schemas are
the only defects.

## Regressions / gate-weakening

None found. The diff's removals are superseded assertions replaced by stronger
ones (wrong `validation-profile` pin → `validation-report`; seven→nine pins;
nine→ten operations; added field-level const constraints). Test count grew
(provider CLI 10→12), the Node gate grew 9→11 checks, and no schema was
loosened — the manifest schema is strictly tighter than the base.

One structural observation worth recording, not a separate finding: the gate's
two live receipt validations both use **degenerate vectors** — a zero-diagnostic
`validate` receipt and a zero-finding `drift` receipt. That is exactly why both
schema defects survived a fix round whose stated goal was making these two
receipts negotiable. Adding the non-empty fixtures is the durable fix.

No unreviewed artifacts: the diff is docs, contracts, gate script, and tests
only. Probe directories were created under gitignored `target/` and outside the
repo.

## Bottom line

Close, but not landable. Both majors are the same defect class both rounds
flagged: the two describing schemas published to make `validate` and
`generate --check` negotiable fail on the non-empty cases, which are the
reachable ones. Each needs a small schema correction plus one gate vector, and
both fixtures already exist in the repo. The minor doc inaccuracy rides along.
Everything else in fix round 1 verifies clean.
