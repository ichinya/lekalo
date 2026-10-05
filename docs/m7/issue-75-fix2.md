# Issue #75 — fix round 2 finding dispositions

Implementation recovery on `ichinya/m7-issue-75`, starting at
`3cb774198aac008ee4e73b08c7b653a72f13e8ac` with 13 modified/deleted
files left by the interrupted worker. Reviewed both round-2 reports in
full: [Cline](issue-75-review2-cline.md) (1 blocker, 1 major, 3 minors)
and [Devin](issue-75-review2-devin.md) (R2-1 through R2-9, including
nits). Also read [fix round 1](issue-75-fix1.md), the
[implementation document](issue-75-implementation.md), and the established
`issue-34-fix3.md` report from Git object `7fcce15a` (that document is
absent from this checkout).

The local base ref `origin/ichinya/M7` is
`87c27d86d3fd2936586f32d95f9d0f9e1bcc7817`; its merge-base with this
branch is `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`. The fixes and this
report are delivered together in one local commit. No push is performed.

## Recovery audit and completed implementation

The recovered diff correctly moved the binary-dependent gate after the CI
build, attached pins before serialization, aligned all/module scope,
normalized the source recipe, emitted LEK-CONTEXT-004, attributed shared
cost to the subject, added reconciliation checks, removed two dead helpers,
and deleted the four committed scratch files. It did not fully address the
baseline digest or temp-directory cleanup: the baseline still serialized
only a state and its internal value was a path, while the fixture helper
still called `keep()`. Its shared-row comment also promised reconciliation
without accounting for explicitly truncated breakdowns.

This recovery completes those gaps. Both known pins now contain digests
of the exact bounded-read bytes. The baseline schema, producer, decoder,
golden and consumers agree on the same closed digest-bearing shape; the
decoder preserves provenance on round-trip and refuses malformed pins.
The input helper reads at most 16 MiB + one sentinel byte, rather than
reading an unlimited file before checking its length. Fixture copies own
their TempDir until all child processes finish and clean up on drop, with
no marker files. Markdown displays both pins and both breakdown costs;
breakdown rows sort by descending cost with an id tie-break. New process
tests exercise source normalization, the unchanged mandatory profile-pin
refusal, all three oversized document inputs, chained baselines and temp
cleanup. Core and Ajv tests check the repaired provenance shapes.

## Per-finding disposition

| Finding | Disposition | Implementation and verification evidence |
| --- | --- | --- |
| Cline **R2-B1**: CI runs the mandatory live gate without building its binary; stale skip comment. | **fixed** | `.github/workflows/ci.yml` runs the gate in `build-test`, immediately after `cargo build --workspace --locked`, using the Ajv directory provisioned earlier in that same job. It runs across Linux, Windows and macOS. The invocation is removed from the schema-only contracts job. Script comments describe mandatory live checks. Local build → gate passes; hiding the binary → exit 1 `binary-missing`, then the binary is restored. |
| Cline **R2-M1** / Devin **R2-1**: policy/baseline pins never reach the output; baseline carries no digest and uses a file path internally. | **fixed** | `main.rs` attaches pins before building canonical JSON, Markdown or the denied payload. Both pins digest the exact consumed bytes. `provenance.baseline` now matches the closed `{state,digest}` shape used by policy. `baseline.rs` validates and retains the pins. CLI tests prove policy pass, metric-policy denial, baseline consumption and chained baseline round-trip. Ajv validates baseline, policy, combined-input and denied reports, checks their exact digests, and refuses malformed pins. Core round-trip checks Markdown too. |
| Cline **R2-m1** / Devin **R2-2** (codemods): committed one-shot patch scripts. | **fixed** | Deleted `scripts/.fix-absence.mjs`, `.fix-envelope.mjs` and `.fix-envelope2.mjs`. No runtime or CI references exist; no replacement scratch script is added. |
| Cline **R2-m2** / Devin **R2-2** (probe): committed root-level baseline output. | **fixed** | Deleted `cb-gate-probe/baseline.json`; `cb-gate-probe/` is absent. Live gate inputs are generated in OS temp directories and removed after successful probes. |
| Devin **R2-3**: `--all` excludes definition kinds that `--module` includes. | **fixed** | Both selectors include every graph node except MODULE. `all_scope_covers_every_module_kind` checks module-kind coverage and exact equality between all selected ids and the real graph's complete non-module node inventory. |
| Devin **R2-4**: LEK-CONTEXT-004 has no emission site. | **fixed** | `plan` emits `artifact_evidence_incomplete` for the effective mapped-files recipe. Core and CLI tests assert the registered diagnostic and `optionalSourceTokens: unsupported`; the none recipe remains unknown without this warning. |
| Devin **R2-5**: effective recipe and profile digest disagree. | **fixed** | Core uses the profile's mapped-files selection even without a CLI flag. An explicit mapped-files request normalizes a none profile and recomputes its digest before planning. Core and CLI tests cover both input paths; a none-recipe policy pin still denies after the recipe changes. |
| Devin **R2-6**: no independent breakdown reconciliation assertion. | **fixed** | Core, CLI and Node checks independently assert sum(exclusive) + sum(shared) = required tokens on the untruncated planner report; the Node gate also checks the committed golden. Planner remains 355 required = 221 exclusive + 134 shared. Existing ledger checks remain. |
| Devin **R2-7**: dead metric helpers remain; supporting collector only tested through a supplied multiplicity map. | **fixed** | Removed unused `metrics::dependency_counts` and `semantic_counts`, retaining the production computations. `supporting_requests_collector_counts_real_edge_occurrences` invokes the production collector over the compiled fixture graph, proves repeated requests to a shared scalar, and reconciles its request total to the independently computed `edgeOccurrences` metric. Existing arithmetic vectors remain. |
| Devin **R2-8**: two auxiliary unbounded policy reads. | **fixed** | One bounded policy read and one parse supply regression limits, digest and mandatory evaluation. There are no auxiliary rereads or path-derived pins. The shared input helper bounds reads before buffering beyond the limit; the CLI oversized-input test checks profiles, policy and baseline refusals. Baseline failures retain their precedence over deferred policy-load failures. |
| Devin **R2-9**: duplicate marker write and intentionally leaked fixture temp directories. | **fixed** | No markers or `keep()` calls remain. `FixtureCopy` retains an owned TempDir for the full test lifetime; `fixture_copy_cleans_up_after_children_finish` verifies the copied directory exists during the child run and is gone after its owner drops. |
| Cline **R2-m3**: shared tokens billed to an arbitrary first dependency. | **fixed** | Shared subject/applicability/synthesized costs bill to the subject's own row at hops 0. Other dependency rows carry only exclusive cost. Core, CLI and live/golden Node checks assert the shared bucket belongs to the subject, never another dependency. The schema description and user documentation explain this attribution and the explicit truncation boundary. |

All round-2 findings are addressed; none is rebutted or deferred. Prior
round-1 claims contradicted by these reviews are superseded by the evidence
above. Existing limits and scoped deferrals in the implementation document
(artifact/ownership adapters, empirical calibration and semantic-diff
envelope attachment) remain explicit; this report does not claim those
successor features shipped.

## No-gate-weakening / no-schema-weakening statement

- No existing test is ignored, skipped or removed; no assertion or policy
  threshold is relaxed. The CLI suite grows 13 → 16 tests and the core
  context-budget suite grows 53 → 57. New assertions strengthen existing
  policy, baseline and breakdown tests.
- The CI gate is moved to its binary-producing job, retaining the exact
  pinned Ajv requirement, mandatory binary refusal, live schema validation,
  existing negative vectors, determinism, privacy and live/golden equality.
  It gains checks for both consumed digests, combined inputs, denied payloads,
  malformed pins and breakdown attribution/reconciliation.
- The only schema wire change repairs the baseline provenance pin requested
  by Devin R2-1: a bare state enum is replaced by a closed digest-bearing
  object. Known requires the existing lowercase SHA-256 grammar; non-known
  forbids a digest; unknown fields are forbidden. Policy, report metrics,
  ledger reason/class coupling and other schema constraints are unchanged.
  The Rust decoder also rejects missing provenance, malformed digests,
  extra fields and explicit null digests. No tolerant legacy alias is added.
  The changed contract uses the current product version 0.6.3, as required
  by `docs/versioning.md`; the requested base-relative version gate passes.
- The golden was regenerated from the built CLI: required 355, direct /
  transitive / indirect 3 / 9 / 6, unchanged metric values and ledger. Its
  changes are the subject-owned shared row, descending breakdown order and
  the repaired unknown baseline object. The gate compares it to live output.
- The four scratch files are deleted, the scratch directory is absent,
  writer tests clean up their private fixture copies, and verification
  produces no new tracked/untracked fixture residue.

## Verification output — real local runs

Windows / PowerShell, Node `v24.13.0`, Cargo `1.98.0`. For Node gates:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH = $env:NODE_PATH
```

Every requested gate exits 0:

```text
$ cargo build -p lekalo-cli --locked
Finished `dev` profile [unoptimized + debuginfo] target(s) in 52.16s

$ cargo test -p lekalo-cli --test context_budget --locked
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ node scripts/test-context-budget-contracts.mjs
{"ok":true,"ajv":"8.17.1","registryEntries":457,"contextRules":8,
 "liveChecked":true,"golden":"tests/fixtures/context-budget/golden/planner.over-budget.json"}

$ node scripts/test-fixture-provenance.mjs
{"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

$ node scripts/check-contract-versions.mjs --base origin/ichinya/M7
{"ok":true,"product":"0.6.3","contractArtifacts":99,"base":"origin/ichinya/M7"}

$ cargo fmt --all -- --check
(no output)

$ cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
Checking lekalo-core v0.6.3
Checking lekalo-cli v0.6.3
Finished `dev` profile [unoptimized + debuginfo] target(s) in 24.70s
```

Additional focused validation, all exit 0:

```text
$ cargo test -p lekalo-core --lib context_budget --locked
test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 957 filtered out

$ node scripts/test-context-contracts.mjs
{"ok":true,"ajv":"8.17.1","goldens":1,"facts":13}

$ node scripts/test-diagnostic-contracts.mjs
{"ok":true,"ajv":"8.17.1","registryEntries":307,"envelopes":5,"diagnosticItems":5}

$ git diff --check
(no errors)
```

Missing-binary negative control (binary hidden temporarily and restored in
`finally`, no missing-binary fallback or gate modification):

```text
$ node scripts/test-context-budget-contracts.mjs
{"ok":false,"reason":"binary-missing",
 "detail":"build target/debug/lekalo before this gate; the live checks are mandatory"}
exit 1 (expected)
```

The CI step placement and its in-job Ajv/build prerequisites were audited
from the actual workflow. These are local runs using the existing Cargo
target cache, not a hosted CI result or a cold three-platform build. The
entire workspace test suite was not rerun; the changed core service, CLI
suite, requested all-target Clippy check and contract gates were exercised.
