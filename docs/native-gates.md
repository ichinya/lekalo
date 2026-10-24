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
| Production run receipt | [native-gate-run v1.0.0](../contracts/native-gate-run.schema.v1.0.0.json) |

The selected policy sets `allow_execution`, `platforms: ["windows"]` and
`runtime_profile: "windows-node-lpac-job-v1"`. Its `planning_policy` is a complete
independently validated v0.4.0 policy. Every confirmation names exact package,
script, canonical package-manifest digest, script-text digest, complete literal
argv and pinned tool. The first profile uses `legacy-suite` for generic existing
Node gates and accepts an empty environment recipe. Arbitrary environment grants remain unsupported. Declared package tool entries
use the successor catalog below.

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

## Declared Vitest, TypeScript and Vite tools

Status: **Implemented** for the same Windows x64 / pinned Node 24 profile.
The reproducible [npm workspace](../tests/fixtures/node-native-tools/project/package.json)
has core and web packages, four explicit scripts, no HTTP server and no database.
The acceptance lane pins Vitest 5.0.5, TypeScript 5.9.3 and Vite 8.3.5 with the
[committed toolchain lock](../tests/fixtures/node-native-tools/toolchain-lock.json).
Vitest uses a threads pool with explicit preserve-symlinks worker arguments
and disables its filesystem module cache; Vite loads
native configuration and uses its in-process build backend. Both configure
preserveSymlinks=true for the already link-free private tree, avoiding Vite's
Windows mapped-drive shell probe; the numeric loopback host avoids DNS probing.
The runtime disables Node's optional compile cache, preserving the declared
cache/write inventory. These settings avoid
child stdio/IPC pipes on the declared Node runtime. Different versions and
subprocess configurations require their own qualification.

Provision dependencies explicitly before planning, in a dedicated tool store.
Execution does not run npm or use workspace node_modules. Inspect the locked
package list and complete installation first. From the matching Lekalo source
snapshot, create a catalog of the already installed physical tool tree:

~~~sh
node scripts/create-node-tool-catalog.mjs /physical/tool-store/node_modules > controls/native-tools.json
~~~

The helper only reads bytes and emits JSON. The production host independently
checks it. [Catalog v1.0.0](../contracts/native-tool-catalog.schema.v1.0.0.json)
separates the Node program/version/digest from each package bin entry,
entry digest, package version and complete asset manifest digest. It records
all dependency files, including native add-ons, with sorted paths, SHA-256 and
byte lengths. The .bin shim directory is excluded. Links, hardlinks, case aliases,
credential homes and traversal refuse. Bounds are 4,096 files/directories,
32 MiB per asset, 128 MiB total and depth 32. Assets are copied read-only into
private node_modules; only the private project and temporary homes are writable.
Relative imports keep the package layout. No PATH bin lookup is involved.

Use an explicit npm workspaces array in the root package.json. The initial
production subset accepts literal member directories and one-level patterns
such as packages/*. The source profile must grant the entire pattern prefix
as a tree; excluding members can make discovery incomplete and refuse execution.
The adapter enumerates actual physical member names inside this grant.

For each selected package script, the planning policy confirmation records:

| Field | Value to review |
| --- | --- |
| package_id / cwd | packages/core=@native/core / packages/core, or the corresponding web package |
| gate / gate_id | test, typecheck or build / a distinct stable identifier |
| gate_kind / required | legacy-suite / true for required gates |
| script_name / script_digest | Existing package.json script key / SHA-256 of its exact text |
| manifest_digest | SHA-256 of compact recursively key-sorted package.json |
| argv | Complete single-space literal script tokens; for example tsc -p tsconfig.json |
| tool_ref | Catalog id, version, asset artifact_digest and entry_digest |
| rule_version / rule_digest | Explicitly reviewed confirmation rule identity and digest |

Keep environment grants empty and select bounded write scopes for cache/build
outputs. For the example these are .lekalo-temp (private runtime home),
packages/core/cache, packages/web/cache and packages/web/out. Source profiles exclude these outputs. A release-full fallback
selects all eligible confirmations; a tool or package omitted from the proposed
plan cannot silently satisfy the required gate. The remaining policy, provenance,
limits and authority fields follow the control contracts above.

Create and review the plan with the same native plan command. A catalog v1.0.0
produces an [outer plan v1.0.0](../contracts/native-execution-plan.schema.v1.0.0.json),
whose digest domain is lekalo.native-execution-plan.v1.0.0. Approve that outer
digest explicitly. To run semantic verification and actual native gates together:

~~~sh
lekalo --json verify --project . --native-plan native-plan.json --native-confirm sha256:APPROVED_DIGEST --native-adapter /path/to/node-typescript-package --native-policy controls/native-policy.json --native-profile controls/native-profile.json --native-catalog controls/native-tools.json --native-cancel-file native.cancel
~~~

The [verify report](../contracts/native-verify-report.schema.v1.0.0.json) contains
semantic results before/after execution and an actual
[execution report](../contracts/native-execution-report.schema.v1.0.0.json).
Declared-tool runs use the [run receipt v1.0.0](../contracts/native-gate-run.schema.v1.0.0.json):
its output paths can retain real cache names such as CACHEDIR.TAG. Source roots
and command cwd keep the predecessor grammar. Output paths still refuse
traversal, protected names, links, hardlinks and case aliases; all writes need
the selected scopes and budgets. All production runs use v1.0.0 receipts, including standalone cwd=dot.
Historical fixture-only receipts retain v0.4.0; approved v0.9.1 plans remain admitted.
Every command keeps its measured exit, outcome, failure class and output reference;
the report retains the referenced redacted stdout and stderr artifacts separately.
For example, tsc exit 2 remains 2 inside a failed report while the overall CLI exit
is 1. Security and infrastructure defects take precedence over a zero tool exit.
Timeouts, cancellation and unavailable streams remain explicit unknown states.
A semantic change during execution blocks an overall PASS.

The legacy semantic-only orchestration receipt retains its optional unsupported
native component; actual execution is the separate native member of this successor
report. Legacy report-file/report-format/ci-policy options cannot be combined
with native-plan. Save the versioned JSON output explicitly. No report or caller
receipt input substitutes for execution. A native error has execution=unverified;
it does not claim that no process ran before an internal failure.

Maintainer replay: build the release CLI, provision pinned Ajv, then run
node scripts/test-node-native-tools.mjs --provision with an explicitly owned
LEKALO_NODE_TOOLS_HOME directory. The lane executes all four tools and checks
real Vitest, typecheck and Vite failures, original bytes, output digests and
approval/source/forged-receipt refusals. Other OSes validate planning and report
unsupported execution; those lanes do not count as Windows qualification.
