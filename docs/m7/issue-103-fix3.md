# Issue #103 — fix round 3 (Cline and Devin reviews)

Branch `ichinya/m7-issue-103`; recovery started at `800d0330` with the six
uncommitted files left by the previous worker. Inputs read:
`issue-103-review3-cline.md`, `issue-103-review3-devin.md`,
`issue-103-fix1.md`, and `issue-103-fix2.md`. Every round-3 finding,
including the optional quality items, is addressed below. This is local
implementation verification; the fix commit is not pushed and does not
claim independent acceptance or remote CI results.

## Dispositions

| Review / ID | Severity | Finding | Disposition | Evidence |
| --- | --- | --- | --- | --- |
| Cline F1 | major | Report-write refusals disappear under `unsupported` (the R2-4 residual) | **fixed** | `ci.report-write-failed` / `LEK-CI-001` now admits `unsupported` in the active `v0.6.3` registry. `report_output::compose` carries the joined diagnostic set in both `DomainResult::Unsupported` and `UnsupportedOperation`, preserving the capability where present and the original status/exit. The unit control `a_write_refusal_joins_the_unsupported_class` asserts exit 4, both diagnostic IDs, and the original capability. The real-binary control `the_write_refusal_is_visible_on_the_unsupported_class` runs locked verify with required target `beta` against both a missing directory and a non-empty 5500-byte report destination: status stays `unsupported`, exit stays 4, both `ci.report-write-failed` and the underlying capability refusals survive, and the existing file remains byte-identical. A fresh destination produces the coherent `unsupported/4`, blocked report without a write refusal. `docs/ci-reports.md` now explicitly lists all five failing statuses. |
| Devin R3-1 | major | Same silent `unsupported` join, broken by both the registry and composition | **fixed** | Same registry and composition fix as Cline F1, with both the capability-bearing unit path and the `UnsupportedOperation` CLI path exercised. The historical fix-2 claim about every failing class was too broad; this round closes its remaining class. |
| Cline F2 | minor | `Valid` and `Unsupported` share a misleading no-op arm/comment | **fixed** | Composition matches the concrete domain variants. `Unsupported` and `UnsupportedOperation` have explicit diagnostic-carrying arms; `Valid` has its own arm. The old comment is replaced with the actual failing-class rule. The variant-preserving match also retains `DeniedWithEvidence`'s JSON and human evidence rather than flattening it into a plain denial. |
| Cline F3 | minor | Contract gates never compare committed goldens with binary output | **fixed** | New integration test `the_binary_reproduces_every_committed_json_golden` reproduces all five JSON goldens over disposable copies of the documented fixture. The case list must equal the on-disk JSON golden set. Every byte is compared after substituting only the checkout-dependent Git commit and dirty **values**; their known-state shapes and value types remain checked, and the working-set pin and every other field stay in the comparison. Each live document must also have canonical bytes with exactly one LF. Preparation follows fix-2's regeneration procedure: lock before verify/generate-check, validate before the read-only readiness projection to populate its cache, and plant `alpha.ghost` for blocked validate. A schema-valid stale model-digest mutation passes the contract gate but fails this binary comparison (exit 101), proving the new check catches the gap; the golden is restored byte-identically. The test also binds both Markdown goldens' commit and report digest to their JSON sources. The recovered verify Markdown pin/digest were stale; they now match `valid.verify.golden.json` (`4a52eccd`, digest `sha256:1edc923f97cb4e3f3a61bb64dd297391ae87658782cc6ca656d9b7ccedcdf959`). A separate live Markdown reproduction agrees byte-for-byte after the same Git-value/digest normalization. |
| Devin R3-2 | minor | Adapter-supply preflight failures record `full` for `verify --changed` | **fixed** | Both the supply block's root-resolution refusal and `AdapterSupply::new` refusal now pass `changed` into `early_verify_report`. The real-binary test `a_verify_changed_supply_refusal_records_the_changed_mode` creates a local Git fixture, commits its base, adds a valid source comment, and invokes the missing adapter. The report records `invocation.mode: changed`, contains the actual `lock.component-unavailable` supply diagnostic and blocking `verify.preflight` row, and has a blocked verdict/nonzero exit. This checks that changed-scope resolution succeeds and the supply arm is actually reached. |

## Recovery audit and no weakening

- Audited all six recovered diffs. The registry and the two mode arguments
  were the required fixes; composition needed the unsupported variants;
  the missing golden comparison and the Markdown/JSON binding needed
  completion. The six implementation/fixture files plus this report are
  the entire commit scope.
- No schema, gate script, workflow, contract version, or lockfile changed.
  The only contract edit adds `unsupported` to one diagnostic's allowed
  **failure** statuses; `valid` is still excluded. No refusal, destination
  confinement, privacy admission, policy, or nonzero exit rule was relaxed.
- No test was deleted or ignored. The CLI report suite grows from 23 to
  26 tests; report-output unit coverage grows by one. The old determinism
  test is strengthened: it now asserts both executions succeed and
  removes the first report before the second run. Previously the second
  run refused the existing non-empty destination, so the test compared
  the first file with itself.
- Existing negative controls remain green: source/existing-file
  preservation, empty placeholders, traversal/case/link refusals,
  secret-bearing projection refusal, locked/changed early-failure
  reports, and required-degraded readiness. The core report tests and
  all nine contract refusal vectors remain green.
- Temporary dependencies and this session's probe/test copies are
  confined to ignored `target/` directories. The standalone Markdown
  probe removed its own copy. Final cleanup of the remaining Rust test
  copies and `target/issue-103-r3-deps` was rejected by automatic approval
  review (`blocked by policy`), including an attempt naming the dependency
  directory explicitly. Those ignored artifacts remain outside the
  commit; no scratch file is staged. Pre-existing ignored build/review
  artifacts are preserved. `git diff --check` is clean.

## Verification outputs (local Windows host)

All eight requested gates passed. Contract commands used Node 24.13.0
and the supplied Ajv 8.17.1 directory via:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
```

```text
cargo build -p lekalo-cli --locked
  Finished `dev` profile [unoptimized + debuginfo]; exit 0

cargo test -p lekalo-cli --test ci_report --locked
  26 passed; 0 failed; 0 ignored; exit 0

node scripts/test-diagnostic-contracts.mjs
  {"ok":true,"ajv":"8.17.1","registryEntries":459,"envelopes":5,"diagnosticItems":5}

node scripts/test-fixture-provenance.mjs
  {"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

node scripts/test-contract-versions.mjs
  {"ok":true,"cases":6}

node scripts/check-contract-versions.mjs --base origin/ichinya/M7
  {"ok":true,"product":"0.6.3","contractArtifacts":96,"base":"origin/ichinya/M7"}

cargo fmt --all -- --check
  clean; exit 0

cargo clippy -p lekalo-cli -p lekalo-core --all-targets --locked -- -D warnings
  Finished `dev` profile [unoptimized + debuginfo]; no diagnostics; exit 0
```

Supplemental checks:

```text
cargo test -p lekalo-cli --bin lekalo report_output --locked
  4 passed; 0 failed; 0 ignored; exit 0

cargo test -p lekalo-core ci_report --locked
  19 passed; 0 failed; 0 ignored; exit 0

node scripts/test-ci-report-contracts.mjs
  {"ok":true,"checked":"ci-report-contracts-v1","ajv":"8.17.1",
   "goldens":5,"invalid":9,"verdicts":["blocked","degraded","ready"]}
  same passing result on Node 18.20.8

Schema-valid stale model digest mutation
  contract gate exit 0; binary golden comparison exit 101 (expected refusal)
  restoredByteIdentically: true
  restored validate golden SHA-256:
  738947ee2e5bd60b719bcd9276726b536cb58bb89586b3e36f4e32a96cafbf24

Live verify Markdown reproduction
  markdownMatchesBinary: true; liveDigestBound: true
  committedJsonDigest:
  sha256:1edc923f97cb4e3f3a61bb64dd297391ae87658782cc6ca656d9b7ccedcdf959

git diff --check
  clean; exit 0
```

The supplemental SARIF gate used `ajv-draft-04@1.0.0` and
`ajv-formats@3.0.1` provisioned with scripts disabled under
`target/issue-103-r3-deps`; `LEKALO_AJV_NODE_PATH` pointed at that
directory's `node_modules`, while `NODE_PATH` continued to resolve the
supplied exact Ajv 8.17.1. The Node 18 run used `npm exec` with
`node@18.20.8` and an npm cache in the same disposable directory. No
dependency manifest or lockfile was changed.
