# Incremental adoption tutorial

Status: **Implemented** selective adoption, verified refactoring, and hybrid mode. Owner: bootstrap/evidence maintainers. [Issue #136](https://github.com/ichinya/lekalo/issues/136), [adoption reference](adoption.md).

This tutorial walks one existing project from a read-only analysis through one selective confirmation, one contracted legacy command, one verified behavior-preserving refactor, and one new contracted feature — the exact incremental path the [adoption modes](adoption.md) page describes. Every command below runs verbatim in a POSIX shell with a built `lekalo` binary on `PATH`; [CI replays the same lines](../scripts/test-adoption-tutorial.mjs) against the [synthetic adoption fixture](../tests/fixtures/adoption/app/package.json). No real consumer repository is needed and no private code is read.

Working copy: the fixture staged into `.adoption-tutorial/app` (all commands run from `.adoption-tutorial/` with `--project app`, so the derived documents land beside the project, never inside the tree a verified plan pins).

On Windows the replay gate needs native Git Bash. If `bash` on `PATH` selects
the WSL launcher, set `LEKALO_TUTORIAL_BASH` to the Git Bash executable before
running `node scripts/test-adoption-tutorial.mjs`; script paths use forward
slashes. The walkthrough itself keeps the same POSIX commands.

## 0. Stage the walkthrough (repository root)

```sh
rm -rf .adoption-tutorial
mkdir .adoption-tutorial
cp -r tests/fixtures/adoption/app .adoption-tutorial/app
cp tests/fixtures/adoption/verification/verification-spec.json .adoption-tutorial/verification-spec.json
cp tests/fixtures/adoption/refactor/candidate-patch.json .adoption-tutorial/candidate-patch.json
cp tests/fixtures/adoption/contracts/declaration.json .adoption-tutorial/declaration-legacy.json
cp tests/fixtures/adoption/scanner/adoption-scanner.mjs .adoption-tutorial/adoption-scanner.mjs
cd .adoption-tutorial
```

## 1. Analyze one use case (read-only)

Every adoption command accepts `--project DIR` and honors it; the analysis is a derived proposal and never writes.

```sh
lekalo --json scan --target node-typescript --profile default --project app node adoption-scanner.mjs > scan.json
lekalo --no-cache --json adoption analyze --use-case complete_case --target node-typescript --profile default --project app > analysis.json
```

Expected: the scan records derived evidence in `.lekalo/import/observed/` (exit 0/stdout), then the analysis (exit 0/stdout) proposes `taskboard.complete_task` with per-fact provenance, `low` confidence while the binding is inferred, explicit unknowns, and the exact source pin for `src/tasks.mjs`. No application or canonical file changes.

## 2. Confirm the reviewed definition

`adoption confirm --dry-run` previews the exact canonical write plan; only the reviewed plan digest authorizes application.

```sh
lekalo --no-cache --json adoption confirm --analysis analysis.json --definition taskboard.complete_task --dry-run --project app > confirmation-plan.json
lekalo --json adoption confirm --plan confirmation-plan.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('confirmation-plan.json','utf8')).plan")" --project app > confirmation-receipt.json
```

Expected: the preview (exit 0/stdout) names one write, `lekalo/modules/taskboard/commands.yaml`, with the exact before/after digests; the apply (exit 0/stdout) records a `completed` confirmation receipt under `.lekalo/import/adoption/receipts/`. Re-analysis now reports the definition as `confirmed`; a wrong `--confirm` digest refuses with `adoption.confirmation-required` (LEK-ADOPT-005, exit 3).

Rollback is always available: `adoption rollback --receipt RECEIPT --dry-run` previews the exact restoration, and `adoption rollback --receipt RECEIPT --confirm RECEIPT_DIGEST` restores the exact prior bytes — created files are removed again, and a later edit at a mutated path refuses as a conflict (`adoption.rollback-conflict`, LEK-ADOPT-009, exit 3) instead of overwriting your work.

## 3. Contract the legacy command (the adapted-legacy mode)

The confirmed command joins the conformed registry: the shipped declaration names the exact current bytes of `src/tasks.mjs`, so the conformance gate passes now and fails after any drift.

```sh
lekalo --json contract update --declaration declaration-legacy.json --project app > contract-update.json
lekalo --json contract attach taskboard.complete_task --native-test "test/tasks.test.mjs#completeTask" --project app > contract-attach.json
lekalo --json contract check --project app > contract-check.json
```

Expected: the update records `taskboard.complete_task` (exit 0/stdout), the attach links the verbatim native test id, and the check (exit 0/stdout) reports the symbol conformant. This is the contracted-legacy side of the hybrid join: the legacy implementation keeps running, and its recorded fingerprint makes any later edit visible.

## 4. Plan, verify, and apply a behavior-preserving refactor

`adoption refactor` turns a reviewed candidate patch into an exact, revision-bound plan; `adoption verify` really executes the pinned checks — baseline and candidate runs in disposable copies with an independent behavior oracle — and records the measured execution; `adoption apply` re-executes the same pinned specification itself through the admitted host capability, so the run that authorizes installation is the boundary's own trusted run, never a caller-supplied record.

```sh
lekalo --no-cache --json adoption refactor --use-case complete_case --target node-typescript --profile default --verification verification-spec.json --candidate candidate-patch.json --project app > refactor-plan.json
lekalo --json adoption verify --plan refactor-plan.json --verification verification-spec.json --candidate candidate-patch.json --project app > execution.json
lekalo --json adoption apply --plan refactor-plan.json --verification verification-spec.json --candidate candidate-patch.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('refactor-plan.json','utf8')).plan")" --project app > apply-receipt.json
```

Expected: the plan (exit 0/stdout) binds the candidate patch digest, the verification specification digest, the read/mutation scopes, the whole-tree state digest of the complete consumed input closure, and the preservation obligations; a plan without a candidate is advice and can never apply. The verification (exit 0/stdout) concludes `preserved` with the oracle `passed` on both sides; a candidate that breaks an oracle obligation concludes `failed`, and apply refuses (`adoption.verification-failed`, LEK-ADOPT-007, exit 1). The apply (exit 0/stdout) installs exactly the reviewed bytes and records the receipt. A drifted tree refuses as `adoption.plan-stale` (LEK-ADOPT-004, exit 3) — before execution and again before install.

What authorizes an installation: verification authority is a fresh run, not a document. `execution.json` records one measured run. Apply re-executes the pinned specification through the trusted host over the exact current tree and candidate bytes. A caller-supplied execution document, even with recomputed checksums, refuses as `adoption.input-invalid` (exit 1). Plans pin the complete consumed input closure; stale imports, Model, lock, tool or candidate bytes refuse before execution and installation.

Within adoption, the pure offline core never spawns a process, resolves PATH or reads the environment. The host uses the adapter execution boundary: Windows LPAC/job containment, Linux bubblewrap namespaces or macOS seatbelt. Only a private copy is writable; the original source tree, its `.lekalo` runtime and external paths remain outside that write grant. Children inherit containment. Missing isolation refuses without an ambient-process fallback. A verifier attempting an external write fails its native checks and cannot authorize apply. The whole-tree digest barrier remains an additional drift check, not the filesystem sandbox.

The reference verifier requires Node 24. It executes the same native test and independent oracle with inherited bounded output and `--test-isolation=none`, keeping Node test-runner IPC out of the Windows LPAC lane. Other interpreter/framework/IPC combinations require their own qualification; no general native-runtime support is implied. Output beyond the cap yields `output-limit` and an unavailable verification, never a truncated successful result. The explicit `no-network` specification token remains outside the admitted specification profile and refuses; the host sandbox denies network access by default.

Declining the preview writes nothing; deleting files works through the same plan (`"action": "delete"` with the exact pre-state digest) and the rollback restores deleted bytes.

## 5. Add a new contracted feature (the new-contracted mode)

New features start canonical: the fixture already declares `taskboard.audit_event` canonically (`lekalo/modules/taskboard/entities.yaml`). Author its maintained implementation, wrap the exact bytes in a reviewed patch document, and run the same plan → verify → apply path.

```sh
cat > app/src/audit.mjs << 'EOF'
// The maintained implementation of the audit_event obligation.
export function auditEvent(task) {
  return { id: task.id, state: task.state, kind: "task.audited" };
}
EOF
node -e "const fs=require('fs'),c=require('crypto');const canon=(v)=>Array.isArray(v)?'['+v.map(canon).join(',')+']':(v&&typeof v==='object')?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canon(v[k])).join(',')+'}':JSON.stringify(v);const dig=(f,v)=>'sha256:'+c.createHash('sha256').update('lekalo.'+f+'.v0.8.0\n'+canon(v)).digest('hex');const sha=(b)=>'sha256:'+c.createHash('sha256').update(b).digest('hex');const content=fs.readFileSync('app/src/audit.mjs','utf8');const doc={schema_version:'lekalo/adoption-patch/v0.8.0',identity:'dev.lekalo.adoption-patch@0.8.0',patch:'',origin:{kind:'supplied-reviewed',identity:'tutorial-feature-patch'},scope:{symbol:'taskboard.audit_event',target:'node-typescript',profile:'default'},files:[{path:'src/audit.mjs',action:'create',before:null,after:sha(Buffer.from(content)),content}]};const rest={...doc};delete rest.patch;doc.patch=dig('adoption-patch',rest);fs.writeFileSync('feature-patch.json',JSON.stringify(doc,null,2))"
rm app/src/audit.mjs
lekalo --no-cache --json adoption refactor --module taskboard --target node-typescript --profile default --verification verification-spec.json --candidate feature-patch.json --project app > feature-plan.json
lekalo --json adoption verify --plan feature-plan.json --verification verification-spec.json --candidate feature-patch.json --project app > feature-execution.json
lekalo --json adoption apply --plan feature-plan.json --verification verification-spec.json --candidate feature-patch.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('feature-plan.json','utf8')).plan")" --project app > feature-receipt.json
```

Then bring the conformed registry back in sync and declare the new symbol: the applied refactor changed the maintained bytes of `src/tasks.mjs`, so the legacy command must be re-declared over the exact new bytes — until then the conformance gate refuses with `contracted.binding-drift` (LEK-CNT-006) — and the new symbol is declared over the bytes just applied. Both declarations are authored from the live files:

```sh
node -e "const fs=require('fs'),c=require('crypto');const fp=(p)=>'sha256:'+c.createHash('sha256').update(fs.readFileSync(p)).digest('hex');const base={schemaVersion:'lekalo/contracted-declaration/v0.4.0',adapter:{id:'lekalo-target-node-typescript',version:'0.2.16',digest:null},project:'taskboard',revision:'sha256:1361a56d4a5b8e7c1c9f0d2a3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6',artifacts:[]};fs.writeFileSync('declaration-legacy.json',JSON.stringify({...base,symbols:[{id:'taskboard.complete_task',kind:'command',source:{path:'src/tasks.mjs',line:11},fingerprint:fp('app/src/tasks.mjs'),signature:{inputs:[],output:null,reads:[]},effects:[]}]},null,2));fs.writeFileSync('declaration-feature.json',JSON.stringify({...base,symbols:[{id:'taskboard.audit_event',kind:'scalar',source:{path:'src/audit.mjs',line:3},fingerprint:fp('app/src/audit.mjs'),signature:null,effects:[]}]},null,2))"
lekalo --json contract update --declaration declaration-legacy.json --project app > contract-reupdate.json
lekalo --json contract update --declaration declaration-feature.json --project app > feature-update.json
lekalo --json contract attach taskboard.audit_event --native-test "test/audit.test.mjs#auditEvent" --project app > feature-attach.json
lekalo --json contract check --project app > feature-check.json
```

Expected: every apply exits 0/stdout with a `completed` receipt; the maintained implementation is hand-written from then on (source-owned), exactly like any maintained body. The authored bytes travel inside the reviewed patch — a `create` write installs under compare-and-swap against typed absence, so the working copy must not hold the file beforehand (that is why the walkthrough removes it after wrapping). The re-update re-records the drifted legacy binding, the feature update records `taskboard.audit_event`, and the check reports both symbols conformant.

## 6. Read the hybrid join

```sh
lekalo --no-cache --json adoption coverage --module taskboard --project app > coverage.json
lekalo --json status --adoption coverage.json --project app > status.json
lekalo --json impact taskboard.complete_task --adoption coverage.json --project app > impact.json
lekalo --json trace query ../tests/fixtures/trace/partial.trace.json gaps --adoption coverage.json --project app > trace.json
```

Expected: coverage (exit 0/stdout) reports the mixed-mode project: `contracted-legacy` for `taskboard.complete_task`, `contracted-new` for `taskboard.audit_event`, and untouched `unbound` rows, with disjoint counts and explicit gaps. A row's `verification` joins retained apply transactions only after project, receipt digest, plan filename, phase, journal/write-set and current written-byte checks. Confirmation alone is not native verification, and a completed-looking receipt JSON is not enough to enter the join. These are local transaction records, not signed remote execution attestations.

`status --adoption` re-fingerprints the consumed pins: a fresh attachment presents `fresh` panels, and a drifted tree becomes `unqualified`. `impact --adoption` adds mode-boundary witnesses with that freshness axis; `trace query --adoption` joins admitted receipt digests and stale obligations. Regenerating an attachment cannot hide contracted-source drift: after editing `src/tasks.mjs`, `contract check` refuses, fresh coverage marks the row `conflicting`, and its status panel reads `stale`.

## 7. Failure and recovery

A partial application is never reported as complete: every write is snapshotted first, journaled, and installed through compare-and-swap; a mid-apply failure rolls the completed bytes back through the snapshots, and the whole snapshot set is digest-verified before the first restoration — a corrupt snapshot refuses the entire rollback instead of stopping halfway.

When a receipt cannot be persisted after the writes completed, the refusal is the typed recovery contract (`adoption.recovery-required`, LEK-ADOPT-008, exit 1): the journal names every stranded write with its snapshot, pre-state, and exact post-state, and the executable path restores them without the receipt. Recovery conflict-checks every target exactly like rollback before the first byte moves — a later user edit at any stranded path refuses as `adoption.rollback-conflict` (LEK-ADOPT-009, exit 3) and the edit is preserved — and an interrupted recovery is resumable: progress is journaled beside (never over) the original journal, so a retry skips the already-restored paths and completes the rest idempotently. Every mutating adoption entry — apply, confirm, rollback, and recovery restore — joins the same project-wide mutation lock as the change, migration, and format writers: another live writer refuses the run outright (`adoption.writer-locked`, LEK-ADOPT-010, exit 3, or the sibling family's own busy rule), and a stranded adoption journal refuses every non-adoption writer until the recovery above resolves it (Model readers stay open by contract — adoption's compare-and-swap writes are per-file atomic, and the resolver must stay reachable). Every plan's durable state is inspectable the same way — here for the completed refactor apply — and the executable recovery defers to the rollback surface while a completed receipt exists (LEK-ADOPT-009, exit 3):

```sh
lekalo --json adoption recovery --digest "$(node -p "JSON.parse(require('fs').readFileSync('apply-receipt.json','utf8')).plan")" --project app > recovery-view.json
lekalo --json adoption recovery --digest "$(node -p "JSON.parse(require('fs').readFileSync('apply-receipt.json','utf8')).plan")" --restore --project app > recovery-refusal.json
```

Verification children run in disposable copies with drained bounded output, process-tree teardown on the deadline, and a scrubbed environment; a missing or unqualified tool concludes `unavailable` honestly and never reports a test as passed, and disposable-copy preparation never leaves project material in the host temp area — not on a failure and not as empty directories on success.

## 8. Clean up

```sh
cd ..
rm -rf .adoption-tutorial
```
