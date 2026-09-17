# Native gates: pnpm workspaces, targeted plans, fixture-only execution

Issue #48 adds the native gate seam: detect Node.js workspaces, build
an immutable execution plan over confirmed gates, and — for trusted
synthetic fixtures only — execute it in a disposable copy through the
test-only harness. Arbitrary scripts are never run, package managers
are never launched, and the original workspace is never the working
directory of any command.

## Version and family identities

| Contract | Current identity |
| --- | --- |
| Target protocol | `lekalo.target/v1`, contract `0.3.2` (`contracts/target-protocol.schema.v0.3.2.json`) |
| Execution policy | `lekalo/native-gate-policy/v0.3.2` (`contracts/native-gate-policy.schema.v0.3.2.json`) |
| Execution plan | `lekalo/native-gate-plan/v0.3.2` (`contracts/native-gate-plan.schema.v0.3.2.json`) |
| Run request/receipt | `lekalo/native-gate-run/v0.3.2` (`contracts/native-gate-run.schema.v0.3.2.json`) |
| Observed view | `lekalo/native-gate-view/v0.3.2` (`contracts/native-gate-view.schema.v0.3.2.json`) |
| Diagnostic registry | `dev.lekalo.diagnostic-registry@0.3.2` (14 new `native-gate.*` rules, `LEK-NGT-001..014`) |
| Authority matrix | `dev.lekalo.authority-matrix@0.3.2` (4 new `lekalo.native-gate-*` kinds) |
| Privacy policy | `dev.lekalo.privacy-export-policy@0.3.2` |

The new protocol operation is `plan-native`: one read-only exchange
whose request alone carries `native_request` (bounded changed
files/symbols plus digest-addressed custody references) and whose
successful result alone carries `native_plan` (the plan digest and a
bounded summary). Negotiation is current-only 0.3.2: a 0.3.1 peer is refused at
negotiation entirely (the frozen 0.3.1 documents keep their exact
published meanings as superseded history). `dry_run`, `plan_id`,
and `writes` stay forbidden on plan-native; the generation write-plan
seam is never reused, and a native plan can never authorize a publish.

## Workspace detection and package graph

- Membership comes only from an in-scope `pnpm-workspace.yaml` parsed
  with a strict bounded subset: canonical relative literals, `*`,
  `**`, `?`; tags, anchors, aliases, flow syntax, block scalars, tabs,
  and duplicate keys are structured refusals, never approximations.
  Dot directories never match; a leading `!` marks a negated
  exclusion that narrows membership and is recorded as an
  uncertainty.
- Package manifests are strict-JSON objects from the read view; every
  package id is `<root>=<name>`. Duplicate names and oversized
  inventories are refusals.
- Edges are typed from `dependencies`/`devDependencies`/
  `optionalDependencies`/`peerDependencies`; `workspace:*`-style
  specifiers prove local edges, plain name matches count as
  manifest-evidence. Unresolved local specifiers become uncertainties.
- With no in-scope workspace manifest the layout is `npm-standalone`
  (its own capability path); yarn and bun are detected and answered
  plan-only/unsupported. The pnpm CLI is never executed.

## Affected selection and confirmed scripts

- A change inside a package affects that package plus its reverse
  transitive dependents, each with bounded reason paths
  (`changed-package`, `dependent-closure`, `build-prerequisite`,
  ...). Unrelated packages stay excluded with a stable reason.
- Only exact confirmed entries execute: the policy names
  package+script+manifest-hash+tool recipe. The confirmed script must
  match the full literal argv — prefix matches never confirm. Shell
  metacharacters, interpolation, assignment prefixes, `node flags in bare or
  attached-value form (`--eval`, `--eval=1`, `--require=x`,
  `--import=y`, `--run=z`), `pnpm/npm/yarn/bun/corepack/npx`,
  shells, `.cmd/.bat/.ps1/.sh` refuse. Lifecycle hooks
  (`preinstall`/`postinstall`/`prepare`) never run; installation and
  updates never happen.
- Fallback to full gates is available only under an explicit
  `release-full` rule with a recorded digest; failures, missing
  tools, network gaps, and untrusted repos never trigger it.

## Digest, approval, run boundary

`plan_digest = sha256("lekalo.native-plan.v0.3.2" || canonical(plan
without plan_digest))` over compact recursively key-sorted UTF-8 JSON
(shared Node/Rust golden vectors). The approval always lives outside
the hashed plan: a run names one exact approved digest, and the runner
re-verifies every input, tool, policy, and catalog digest before the
first launch — drift is `plan-stale`, zero spawns.

Execution states are closed: `passed`, `failed` (real nonzero gate),
`missing` (confirmed script/tool absent), `blocked` (policy/trust/
approval), `unsupported` (manager/platform capability),
`infrastructure` (spawn/crash/deadline/flood/cleanup), `security`
(unexpected write, original mutation, env escape). Measurements use
the #120 `valueState` vocabulary — a never-executed command reports
unknown, never exit 0. Security and cleanup defects are never masked
by a gate exit.

## Platform capability honesty

| Platform | Base | Gap behavior |
| --- | --- | --- |
| Windows (verified) | bounded direct spawn: deadline+kill, both pipes drained under caps, process-group kill on unix; capability evidence reports `unavailable` honestly | Network denial and descendant containment are NOT enforced — confinement is #89 |
| Linux/macOS | not exercised on this runner | capability reported `unavailable`; confinement stays with #89/the backend suites |
| macOS | sandbox-exec profile route | Unproven backend → blocked |

Network denial is NOT enforced by this runner; the receipt reports
`network_denial: unavailable` honestly. True confinement is #89.

## M3 execution boundary (read this before running anything)

The production `lekalo native run` binary never launches a gate
command. It independently validates the plan and approval, then
answers with a typed receipt: `blocked` + `confinement-required` for
private/untrusted repositories (plan-only until #89), `unsupported` +
`fixture-runner-not-shipped` for trusted synthetic plans — the real
runner is compiled only into the Rust test harness
(`native_gate::fixture_tests`, `#[cfg(test)]`), accepts only trusted
catalog entries, executes only in a disposable byte copy, and proves
original-unchanged plus cleanup-complete in the receipt.

There is no flag, environment variable, or marker file that turns gate
execution on in a production build. Caller-controlled `public-fixture`
markers are not trust: only the checked-in catalog identity, synthetic
provenance, and exact digests admit a plan to the harness.
