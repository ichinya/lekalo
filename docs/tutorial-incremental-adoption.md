# Incremental adoption tutorial

Status: **Implemented** selective adoption, verified refactoring, and hybrid mode. Owner: bootstrap/evidence maintainers. [Issue #136](https://github.com/ichinya/lekalo/issues/136), [research](m8/issue-136-research.md).

This tutorial walks one existing project from a read-only analysis through one selective confirmation, one verified behavior-preserving refactor, and one new contracted feature — the exact incremental path the [adoption modes](adoption.md) page describes. It runs on the synthetic [adoption fixture](../tests/fixtures/adoption/app/package.json); no real consumer repository is needed and no private code is read.

Working root: [tests/fixtures/adoption/app](../tests/fixtures/adoption/app).

## 1. Analyze one use case (read-only)

Every adoption command accepts `--project DIR` and honors it; the analysis is a derived proposal and never writes.

```sh
lekalo --json scan --target node-typescript --profile default --project . node ../../tests/fixtures/adoption/scanner/adoption-scanner.mjs
lekalo --no-cache --json adoption analyze --use-case complete_case --target node-typescript --profile default --project .
```

Expected: the scan records derived evidence in `.lekalo/import/observed/` (exit 0/stdout), then the analysis (exit 0/stdout) proposes `taskboard.complete_task` with per-fact provenance, `low` confidence while the binding is inferred, explicit unknowns, and the exact source pin for `src/tasks.mjs`. No application or canonical file changes.

## 2. Confirm the reviewed definition

`adoption confirm --dry-run` previews the exact canonical write plan; only the reviewed plan digest authorizes application. Add `--binding PROPOSAL --candidate NATIVE` from `bindings propose` to confirm the binding in the same reviewed plan.

```sh
lekalo --no-cache --json adoption analyze --module taskboard --target node-typescript --profile default --project .
lekalo --no-cache --json adoption confirm --analysis analysis-module.json --definition taskboard.complete_task --dry-run --project .
lekalo --json adoption confirm --plan confirmation-plan.json --confirm PLAN_DIGEST --project .
```

Expected: the preview (exit 0/stdout) names one write, `lekalo/modules/taskboard/commands.yaml`, with the exact before/after digests; the apply (exit 0/stdout) records a `completed` confirmation receipt under `.lekalo/import/adoption/receipts/`. Re-analysis now reports the definition as `confirmed`; a wrong `--confirm` digest refuses with `adoption.confirmation-required` (LEK-ADOPT-005, exit 3).

Rollback is always available: `adoption rollback --receipt RECEIPT --dry-run` previews the exact restoration, and `adoption rollback --receipt RECEIPT --confirm RECEIPT_DIGEST` restores the exact prior bytes — created files are removed again, and a later edit at a mutated path refuses as a conflict (`adoption.rollback-conflict`, LEK-ADOPT-009, exit 3) instead of overwriting your work.

## 3. Plan and verify a behavior-preserving refactor

`adoption refactor` turns a reviewed candidate patch into an exact, revision-bound plan; `adoption verify` really executes the pinned checks — baseline and candidate runs in disposable copies with an independent behavior oracle — and only execution evidence produced by such a run can authorize the apply.

```sh
lekalo --no-cache --json adoption refactor --use-case complete_case --target node-typescript --profile default --verification verification-spec.json --candidate candidate-patch.json --project .
lekalo --json adoption verify --plan refactor-plan.json --verification verification-spec.json --candidate candidate-patch.json --project .
lekalo --json adoption apply --plan refactor-plan.json --verification execution.json --candidate candidate-patch.json --confirm PLAN_DIGEST --project .
```

Expected: the plan (exit 0/stdout) binds the candidate patch digest, the verification specification digest, the read/mutation scopes, and the preservation obligations; a plan without a candidate is advice and can never apply. The verification (exit 0/stdout) concludes `preserved` with the oracle `passed` on both sides; a candidate that breaks an oracle obligation concludes `failed`, and apply refuses (`adoption.verification-failed`, LEK-ADOPT-007, exit 1). The apply (exit 0/stdout) installs exactly the reviewed bytes and records the receipt. A drifted tree refuses as `adoption.plan-stale` (LEK-ADOPT-004, exit 3); a hand-written or replayed execution document refuses before any write (`adoption.input-invalid`, LEK-ADOPT-001, exit 1).

Declining the preview writes nothing; deleting files works through the same plan (`"action": "delete"` with the exact pre-state digest) and the rollback restores deleted bytes.

## 4. Add a new contracted feature (hybrid mode)

New features start canonical: author the definition, plan its maintained implementation, and apply — untouched observed code, adapted legacy contracted code, and new contracted code then coexist per symbol.

```sh
lekalo --no-cache --json adoption analyze --module taskboard --target node-typescript --profile default --project .
lekalo --no-cache --json adoption refactor --module taskboard --target node-typescript --profile default --verification verification-spec.json --candidate new-feature-patch.json --project .
lekalo --json adoption apply --plan feature-plan.json --verification execution.json --candidate new-feature-patch.json --confirm PLAN_DIGEST --project .
```

Expected: each apply exits 0/stdout with a `completed` receipt; the maintained implementation is hand-written from then on (source-owned), exactly like any maintained body.

## 5. Read the hybrid join

```sh
lekalo --no-cache --json adoption coverage --module taskboard --project .
lekalo --json status --adoption coverage.json --project .
lekalo --json impact --symbol taskboard.complete_task --adoption coverage.json --project .
```

Expected: coverage (exit 0/stdout) reports per-symbol effective mode (`observed`, `contracted-legacy`, `contracted-new`, `unbound`, `conflicting`), disjoint aggregate counts, and explicit gaps. The coverage document pins the consumed input revisions it was derived from; `status --adoption` re-fingerprints those pins live — a fresh attachment presents `fresh` panels, and a drifted tree downgrades them to the typed `unqualified` freshness instead of presenting stale evidence as fresh. `impact --adoption` adds the mode-boundary witnesses.

## Failure and recovery

A partial application is never reported as complete: every write is snapshotted first, journaled, and installed through compare-and-swap, and a mid-apply failure rolls the completed bytes back through the snapshots. `adoption recovery --digest PLAN_DIGEST` (or `--plan PLAN_FILE`) prints the durable journal of an interrupted phase with every snapshot's integrity classified, so a partial mutation is discoverable instead of invisible. Verification children run in disposable copies with drained bounded output, process-tree teardown on the deadline, and a scrubbed environment; a missing or unqualified tool concludes `unavailable` honestly and never reports a test as passed.
