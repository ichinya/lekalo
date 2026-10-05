# Issue #35 — Independent review round 1 (codex)

Reviewer: codex, independent of the implementation author. Review date: 2026-10-04.
Worktree: `C:/Users/User/orca/workspaces/lekalo/m7-issue-35`.
Branch: `ichinya/m7-issue-35`.
Reviewed base: `a56ee5786f1724f9ee61ef973088c7679684c14d`.
Reviewed HEAD: `088cc5bc20185b15303309a541ee42d078e29418` (feature commits `dded99f9` and `2868c285`, plus research and Devin's review).

## Verdict

**ACCEPT** for the supplied Lekalo-side implementation delta. No blocking, major or minor code findings were reproduced. All five mandatory checks in `C:/Users/User/AppData/Local/Temp/brief-35.txt` passed independently.

This verdict covers the neutral assessment service, CLI, contracts, diagnostics, fixture custody and CI wiring. Real HLV execution and AIFHub Extension verify/done aggregation remain external acceptance work, as explicitly stated in `docs/m7/issue-35-implementation.md:5` and `docs/trace-assessment.md:59`. The issue-level boundaries below remain part of this verdict; synthetic receipts do not establish full external integration acceptance.

The complete brief and all three required input documents were read. The [issue #35 acceptance text](https://github.com/ichinya/lekalo/issues/35) was reread through `gh issue view 35 --repo ichinya/lekalo --json number,title,state,body,url`. Devin's verdict was treated as a claim to verify. Source, schema bytes, registry entries and live process results supplied the evidence for this review.

## Mandatory checks

| Criterion | Independent evidence | Result |
| --- | --- | --- |
| Locked rebuild and live success/refusal | `cargo build -p lekalo-cli --locked` exited 0. Built CLI reports `lekalo 0.6.4`. Direct `trace assess` probes with `trace.json` and `input/ready.json` / `input/missing-mapping.json` ran with an empty `PATH`; details below. | PASS |
| Exact Ajv gates and counts | `LEKALO_AJV_NODE_PATH=C:/Users/User/AppData/Local/Temp/ajv/node_modules`; input gate: Ajv **8.17.1**, **48 checks**, `synthetic:true`. Assessment gate with `--require-binary`: **38 cases**, `live:true`, `execution:"synthetic receipts; no HLV invocation"`. Every committed case was reproduced by the rebuilt CLI and compared with its entire JSON golden, including stream and exit assertions. | PASS |
| Unchanged neutral schema and additive registry | Node compared `readFileSync` bytes with `git show a56ee578:contracts/trace-manifest.schema.v0.2.16.json` using `Buffer.equals`: **16,166 bytes**, identical. SHA-256: `5f9c5baf4dcc5d48087be2e003320999aa6b071fd06764d5276e007361c094e2`. Parsed registry comparison used `assert.deepEqual` for every predecessor entry: **459 → 468**, **0 changed predecessors**, exactly **9 added `trace.bridge-*` entries**, `LEK-TRACE-001..009`, and unique IDs. | PASS |
| No real HLV invocation; synthetic labelling | Process-spawn search across core trace/assessment and provider paths found no invocation. `crates/lekalo-cli/src/main.rs:6154` only opens bounded inputs, parses and assesses. The Node assessment gate's sole `spawnSync` at `scripts/test-trace-assessment-contracts.mjs:52` runs the Lekalo binary. CLI custody tests launch `CARGO_BIN_EXE_lekalo` with empty `PATH`. Provenance registers this family synthetic at `tests/fixtures/fixture-provenance.json:268`; gate output and layout sentinels also identify injected evidence. No HLV was installed or run during this review. | PASS |
| Format, delta whitespace and worktree hygiene | `cargo fmt --all -- --check` and `git diff --check a56ee578..HEAD` exited 0. Initial and post-verification `git status --short` were empty. Only this named review document was subsequently added; staging and the local commit are restricted to it. | PASS |

The live commands were equivalent to:

```powershell
./target/debug/lekalo.exe trace assess tests/fixtures/trace-assessment/trace.json --evidence tests/fixtures/trace-assessment/input/ready.json --json
./target/debug/lekalo.exe trace assess tests/fixtures/trace-assessment/trace.json --evidence tests/fixtures/trace-assessment/input/missing-mapping.json --json
```

`ready` exited **0**, emitted stdout only, and returned `status:"valid"`, `verdict:"ready"`, `coverage:"complete"`, one chain, `execution:"passed"` and zero findings. Its three independent receipts were retained. Strict Ajv validation of the input and assessment output passed.

`missing-mapping` exited **3**, emitted stdout only, and returned `status:"denied"`. **`payload.assessment` was present**, with `verdict:"blocked"`, `coverage:"partial"`, one chain, `execution:"unverified"` and all three original receipts. The envelope and retained assessment passed the published output schema. Diagnostics included `trace.bridge-mapping-missing`, `trace.bridge-execution-unverified`, `trace.bridge-chain-uncovered` and `trace.bridge-policy-denied`; denial diagnostics mirror the report diagnostics.

## Issue acceptance criteria

| Issue criterion | Source and independently executed evidence | Acceptance boundary |
| --- | --- | --- |
| AC1: requirement → symbol → binding → scenario → HLV gate end-to-end | `crates/lekalo-core/src/trace/assessment/mod.rs:22` resolves exact node kinds and all five directed joins, including `symbol → artifact`; execution qualification is at line 353. Live ready, missing binding/mapping, unrelated execution, digest mismatch and stale-pin cases reproduce their goldens. | Supplied neutral chain and receipt assessment PASS. Actual HLV relations/check execution through the Extension remains pending. |
| AC2: missing HLV mapping appears as a trace gap | Mapping and revision-bound HLV aliases are checked at `mod.rs:210`; selected scope coverage is checked after chain assessment. Live `missing-mapping`, `missing-binding`, `missing-hlv-test-id`, `dangling-mapping` and `uncovered-scenario` cases retain registered findings and deny required policy. | PASS for visible assessment findings and partial/conflicting coverage. The existing trace manifest is preserved; adapter composition of neutral trace gaps remains external. |
| AC3: HLV diagnostic codes survive normalization | Closed `OriginalDiagnostic` wire retains code, severity and subject. The live `hlv-fail` golden retains `CTR-030` and `GATE-005` exactly, separately from registered Lekalo summaries. Contradictory pass/error and malformed inputs fail closed. | PASS for consuming and retaining already normalized receipts. Native HLV JSON decoding remains Extension-owned. |
| AC4: OpenSpec and HLV work together with Lekalo | Ready/failure cases retain separate OpenSpec, HLV and source-native receipts. `mod.rs:264` checks freshness and negotiated pins; line 324 requires exactly one passing check for every required provider in each chain. Input accepts references without canonical requirement bodies. | Neutral coexistence, required/optional policy and standalone Lekalo PASS. Concurrent real provider execution and lifecycle aggregation remain pending. |
| AC5: core independent of HLV crate/source layout | Cargo.lock changes only the two workspace package versions; dependency manifests add no HLV crate, path or Git dependency. Core assessment imports neutral trace/wire types and contains no filesystem/process/Git discovery. Both gates and independent empty-PATH probes ran successfully. | PASS locally. Caller-supplied pins are compared for integrity; this service does not attest a provider process or the live checkout. |
| AC6: adopt/greenfield fixtures covered | Live gate executes both `greenfield/evidence.json` and `adopt/evidence.json` from their corresponding directories, compares full output with ready, and hashes the entire fixture family before/after (`scripts/test-trace-assessment-contracts.mjs:60`). CLI tests also exercise failure then success while checking sentinels, inputs and absence of `.lekalo`. | Synthetic layout independence and read-only custody PASS. Native HLV layout resolution/execution remains external. |
| AC7: HLV failure preserves Lekalo/native evidence | Live `hlv-fail` / `hlv-unavailable` cases preserve all normalized receipts and their distinct outcomes; qualified native execution remains passed. `mod.rs:573` uses `DomainResult::denied_json` to retain the report. Optional unavailability emits valid/degraded rather than ready. Family hashes remain unchanged. | PASS for assessment and CLI evidence retention. Extension persistence, timeout/crash handling and sequential verify/done custody require external acceptance. |

## Additional independent verification

Seven disposable hostile probes outside the checkout passed: duplicate **required** provider checks, execution tool digest mismatch, check model digest mismatch, mapping outside selected scope, empty chains for a nonempty scope, an unknown nested protocol member with a private canary, and failed supplied execution with `requireExecution:false`. Expected exits were 3 for semantic refusals and 1 for the unknown member; all semantic refusals retained their assessment and supplied evidence. The invalid envelope omitted the private canary. No repository fixture was edited to create these inputs.

Explicit non-required configurations were also inspected: distinct mapping occurrences remain distinct; `requiredProviders:[]` plus `requireExecution:false` permits a scoped structural assessment; the report states `execution:"not-requested"`. These choices do not produce a runtime execution claim.

Programmatic comparison of diagnostic-registry schema, validation-profile schema/default/strict, and validation-report schema showed only their `0.6.3 → 0.6.4` version promotion. Existing provider operation definitions retain their per-operation const constraints, with validation's output pin advanced to 0.6.4; additions are the assessment operation and two new schema pins. Provider discovery, diagnostics, validation-profile, classification, expressions and context-budget gates passed; classification and context-budget confirmed their live binary legs. The existing trace gate passed with **37 invalid fixtures** and unchanged canonical digest `sha256:ebb5fc53c04befc3edb6c01a671c697f46f48aad611814c1bb9a0d6e11da45b2`.

Contract-version check passed for product **0.6.4**, **106 contract artifacts**, base `a56ee578`. Fixture provenance passed for **69 synthetic families**. Catalog passed with **21 cases**, **6 imported evidence entries**, **468 registry rules**; hygiene passed for **259 files** and **6 controls**. Structure, authority and privacy guards passed with the existing accepted authority/privacy 0.3.2 pins. Older gates that require Node module resolution were run with `NODE_PATH` set to the same provisioned Ajv directory; the initial missing-module setup error was corrected without any checkout edits or dependency installation.

CI inspection confirms Node 18/24 contract checks at `.github/workflows/ci.yml:45` and mandatory real Lekalo execution at line 206, following Ajv provisioning and `cargo build --workspace --locked` in the same Ubuntu/Windows/macOS build-test job. Hosted CI and Linux/macOS execution were not performed in this local review.

`cargo clippy --workspace --all-targets --locked -- -D warnings` passed. The full `cargo test --workspace --locked --no-fail-fast --quiet` run exited **1**: **1,927 passed**, **1 failed**, **2 ignored**, across **95 result targets**. The only failure was the existing `exact_file_scopes_create_replace_and_clean_through_the_confined_client` test at `crates/lekalo-core/tests/target_protocol_boundaries.rs:325`, returning `ResponseInvalid { detail: NotJson }` during generation. The new core assessment **7/7** and CLI assessment **5/5** tests passed in that run.

The complete failing target was rerun without source changes using `cargo test -p lekalo-core --test target_protocol_boundaries --locked -- --nocapture`: **13/13 passed**, exit 0. Git blob comparisons confirmed that the boundary test, its adapter fixture, and target-protocol wire/transport/confinement sources are identical to the review base; checkout differences in these Rust files are only Windows line endings. This leaves an observed intermittent test failure whose cause was not established. The original full workspace command remains a failed run; the focused rerun does not turn it into a green full run. No implementation change or test weakening was made, and no #35 code defect was established from this failure.

Local tools were Node **24.13.0**, Rust/Cargo **1.98.0**, and Ajv **8.17.1** on Windows. Rust 1.80 MSRV, Node 18 and hosted platform checks were not rerun here.

## Scope and custody

This review changes and commits only `docs/m7/issue-35-review1-codex.md`. Implementation, contracts and fixtures were left unchanged. Disposable probe inputs/results were kept under the operating-system temporary directory; build/test artifacts remain ignored. No other worktree or external AIFHub checkout was modified. No push or publication was performed.
