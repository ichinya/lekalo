# Native gate planning and confirmed execution

Lekalo proposes existing Node commands without running them, then executes an
explicitly approved plan in a disposable OS-confined copy. The first production
execution profile supports **Windows x64 with exactly pinned Node 24**.
Linux/macOS execution returns unsupported. Read-only adapter planning is available
where the required adapter confinement is available.

Stock Node 24/libuv 1.52 has an upstream AppContainer limitation: creating
child stdio/IPC pipes can hang until the execution deadline. Inherited handles
work; the containment acceptance uses them to observe a real detached child.
This profile does not claim arbitrary Node subprocess recipes are compatible.
See the [Node issue](https://github.com/nodejs/node/issues/66374) and
[libuv fix](https://github.com/libuv/libuv/pull/5181).

Execution never installs dependencies, invokes a package manager, changes the
original workspace, or uses ambient PATH to choose a program. A `public-fixture`
marker grants no execution authority.

## Public CLI

Create a plan with explicitly selected controls:

```sh
lekalo --json native plan --project . --adapter /path/to/node-typescript-package --policy controls/native-policy.json --profile controls/native-profile.json --catalog controls/native-tools.json > native-plan.json
```

This launches only the manifested read-only planning adapter against a private
view. Check its exit status before using the output. Optional `--changes FILE`
supplies the closed native changed-input document. Without changes, the policy
must explicitly select a full release rule; missing changes never invent a
changed-file claim.

Review commands, package selection, allowed writes, limits and custody. Approve
the **outer** `plan_digest`, then supply the same controls:

```sh
lekalo --json native run native-plan.json --confirm sha256:APPROVED_DIGEST --project . --adapter /path/to/node-typescript-package --policy controls/native-policy.json --profile controls/native-profile.json --catalog controls/native-tools.json --cancel-file native.cancel
```

Replace package path/digest with the inspected values. Selectors are relative to
the invocation directory. Creating the optional cancellation file cancels work;
the CLI does not create it. It is required if a console interrupt handler cannot
be registered. Ctrl-C and this file cooperate with process-tree teardown and
cleanup. Hard process/OS termination cannot promise a terminal cleanup receipt.

## Control documents and custody

| Document | Published contract |
| --- | --- |
| Selected execution policy | [native-execution-policy v0.9.1](../contracts/native-execution-policy.schema.v0.9.1.json) |
| Program catalog | [native-tool-catalog v0.9.1](../contracts/native-tool-catalog.schema.v0.9.1.json) |
| Outer execution plan | [native-execution-plan v0.9.1](../contracts/native-execution-plan.schema.v0.9.1.json) |
| Nested planning policy | [native-gate-policy v0.4.0](../contracts/native-gate-policy.schema.v0.4.0.json) |
| Nested native plan | [native-gate-plan v0.4.0](../contracts/native-gate-plan.schema.v0.4.0.json) |
| Run receipt | [native-gate-run v0.4.0](../contracts/native-gate-run.schema.v0.4.0.json) |

The selected policy sets `allow_execution`, `platforms: ["windows"]` and
`runtime_profile: "windows-node-lpac-job-v1"`. Its `planning_policy` is a complete
independently validated v0.4.0 policy. Every confirmation names exact package,
script, canonical package-manifest digest, script-text digest, complete literal
argv and pinned tool. The first profile uses `legacy-suite` for generic existing
Node gates and accepts an empty environment recipe. Arbitrary environment grants
and separate tool-entry recipes require a supported successor profile.

A full fallback rule has `mode: "release-full"`,
`trigger: "explicit-release-invocation"`, explicit `gate_kinds` and `rule_digest`.
Its selection carries release-rule reasons, not fictional source changes. The
package graph must still be complete.

Catalogs select absolute **physical** Node executable paths, exact program/artifact
SHA-256 digests and versions. Resolve nvm symlinks/junctions before selecting their
targets. Entries use the same Node 24 runtime. The program is copied and checked
before the planning adapter starts, then checked again when the OS sandbox stages
it. No tool is downloaded or selected through PATH.

The explicit launch profile supplies file/tree source roots and literal exclusion
roots (optional trailing `/**`). Paths are portable, lowercase and relative.
Protected credential/runtime homes cannot be included. Links, junctions,
hard-linked source files, case aliases, unsupported exclusions, unreadable inputs
and exhausted budgets refuse. Capture is bounded to 4,096 files and 4,096 directories,
4 MiB per source file and 64 MiB total. Manifest scopes are an independent ceiling.

The outer plan records sorted source paths/byte digests, raw policy/profile/catalog
SHA-256 pins, verified adapter manifest/package/entry and runtime pins. Its domain
is `lekalo.native-execution-plan.v0.9.1` followed by compact recursively key-sorted
JSON without `plan_digest`. Source additions, package-graph changes, reformatted
controls and replaced tools revoke approval. Decoded duplicate keys refuse.

Core checks approval, policy authorization and current custody **before any adapter
or gate process**. It repeats read-only planning over pinned private inputs and
requires an exact complete-plan match. OS qualification precedes gates. Shell
syntax, eval/import hooks, executable shims and package-manager recipes refuse.
Confirmed scripts must exist in the captured view.

## Results and platform guarantees

Windows uses LPAC filesystem/network denial and a Job Object that contains
descendants, kills them on closure, caps each process and the entire job at
512 MiB committed memory, and permits at most 32 processes. Missing primitives or
a failed runtime probe refuse; no raw-spawn fallback exists. The capability
receipt digest commits to this configuration, not a peak-memory sample.

Only the private project is writable. The fixed Windows environment uses private
temp/profile paths and the OS system root. Inherited credentials, Node options
and package-manager tokens are absent. Per-command/whole-run deadlines,
cancellation and output caps bound work. A reduced total output budget is
apportioned between stdout and stderr.

The receipt binds the nested plan digest; `provenance` names the approved outer
plan. It records actual command exit/outcome, bounded redacted output references,
observed private mutations, original verification and measured cleanup.
Each output reference binds a [combined stdout/stderr artifact](../contracts/native-command-output.schema.v0.9.1.json); unavailable streams retain explicit states.
Timeout/cancellation/flood retain unknown exits and unavailable outputs, never
fabricated zero/empty-output digests. An unavailable mutation audit is explicit
infrastructure/security failure; its empty list does not prove zero mutations.

| Result | CLI exit and evidence |
| --- | --- |
| Passed gates and cleanup | 0; run receipt on stdout |
| Real required gate failure | 1; receipt on stderr, actual nonzero command exit |
| Missing approval or policy denial | 3; registered diagnostic, no gate |
| Stale source/tool/control | 1; `native-gate.plan-stale`, no gate |
| Missing script or unsupported platform/confinement | 4; capability diagnostic, no fallback |
| Timeout, cancellation, flood or infrastructure failure | 4; bounded failure evidence |
| Unexpected writes, write-budget violation or original mutation | 3; security evidence |

Original verification compares declared captured inputs and selected control bytes;
confinement denies writes to the rest of the host.

Command exit and execution verdict are separate: zero cannot mask security or
cleanup failure. Actual receipts can enter trace/gate evidence; semantic
validation alone cannot produce passing native evidence.

## Protocol and historical boundary

Protocol 0.9.1 carries the complete native plan and matching digest. Core checks
its document, adapter identity and optional summary counts independently.
Predecessors retain their summary-only wire members; summaries cannot authorize
execution. The issue #48 fixture harness and frozen v0.4.0 documents retain their
historical meaning. A raw v0.4.0 `native run PLAN` still returns plan-only refusal.
Production execution requires the selected policy and approved v0.9.1 outer plan.

Acceptance uses `cargo build --release -p lekalo-cli --locked` then
`node scripts/test-native-execution.mjs` with pinned Ajv. The public release CLI
executes `tests/fixtures/native-execution/project`; no test-only switch exists.
Windows must pass real execution/refusal, original-byte, descendant and cleanup
lanes. Other OS results report unsupported and do not qualify execution.
