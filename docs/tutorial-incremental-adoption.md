# Incremental adoption tutorial

Status: **Implemented** selective adoption, verified refactoring, and hybrid mode. Owner: bootstrap/evidence maintainers. [Issue #136](https://github.com/ichinya/lekalo/issues/136), [research](m8/issue-136-research.md).

This tutorial walks one existing project from a read-only analysis through one selective confirmation, one contracted legacy command, one verified behavior-preserving refactor, and one new contracted feature — the exact incremental path the [adoption modes](adoption.md) page describes. Every command below runs verbatim in a POSIX shell with a built `lekalo` binary on `PATH`; [CI replays the same lines](../scripts/test-adoption-tutorial.mjs) against the [synthetic adoption fixture](../tests/fixtures/adoption/app/package.json). No real consumer repository is needed and no private code is read.

Working copy: the fixture staged into `.adoption-tutorial/` (all commands run from there with `--project .`).

## 0. Stage the walkthrough (repository root)

```sh
rm -rf .adoption-tutorial
cp -r tests/fixtures/adoption/app .adoption-tutorial
cp tests/fixtures/adoption/verification/verification-spec.json .adoption-tutorial/verification-spec.json
cp tests/fixtures/adoption/refactor/candidate-patch.json .adoption-tutorial/candidate-patch.json
cp tests/fixtures/adoption/contracts/declaration.json .adoption-tutorial/declaration-legacy.json
cp tests/fixtures/adoption/scanner/adoption-scanner.mjs .adoption-tutorial/adoption-scanner.mjs
cd .adoption-tutorial
```

## 1. Analyze one use case (read-only)

Every adoption command accepts `--project DIR` and honors it; the analysis is a derived proposal and never writes.

```sh
lekalo --json scan --target node-typescript --profile default --project . node adoption-scanner.mjs > scan.json
lekalo --no-cache --json adoption analyze --use-case complete_case --target node-typescript --profile default --project . > analysis.json
```

Expected: the scan records derived evidence in `.lekalo/import/observed/` (exit 0/stdout), then the analysis (exit 0/stdout) proposes `taskboard.complete_task` with per-fact provenance, `low` confidence while the binding is inferred, explicit unknowns, and the exact source pin for `src/tasks.mjs`. No application or canonical file changes.

## 2. Confirm the reviewed definition

`adoption confirm --dry-run` previews the exact canonical write plan; only the reviewed plan digest authorizes application.

```sh
lekalo --no-cache --json adoption confirm --analysis analysis.json --definition taskboard.complete_task --dry-run --project . > confirmation-plan.json
lekalo --json adoption confirm --plan confirmation-plan.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('confirmation-plan.json','utf8')).plan")" --project . > confirmation-receipt.json
```

Expected: the preview (exit 0/stdout) names one write, `lekalo/modules/taskboard/commands.yaml`, with the exact before/after digests; the apply (exit 0/stdout) records a `completed` confirmation receipt under `.lekalo/import/adoption/receipts/`. Re-analysis now reports the definition as `confirmed`; a wrong `--confirm` digest refuses with `adoption.confirmation-required` (LEK-ADOPT-005, exit 3).

Rollback is always available: `adoption rollback --receipt RECEIPT --dry-run` previews the exact restoration, and `adoption rollback --receipt RECEIPT --confirm RECEIPT_DIGEST` restores the exact prior bytes — created files are removed again, and a later edit at a mutated path refuses as a conflict (`adoption.rollback-conflict`, LEK-ADOPT-009, exit 3) instead of overwriting your work.

## 3. Contract the legacy command (the adapted-legacy mode)

The confirmed command joins the conformed registry: the shipped declaration names the exact current bytes of `src/tasks.mjs`, so the conformance gate passes now and fails after any drift.

```sh
lekalo --json contract update --declaration declaration-legacy.json --project . > contract-update.json
lekalo --json contract attach taskboard.complete_task --native-test "test/tasks.test.mjs#completeTask" --project . > contract-attach.json
lekalo --json contract check --project . > contract-check.json
```

Expected: the update records `taskboard.complete_task` (exit 0/stdout), the attach links the verbatim native test id, and the check (exit 0/stdout) reports the symbol conformant. This is the contracted-legacy side of the hybrid join: the legacy implementation keeps running, and its recorded fingerprint makes any later edit visible.

## 4. Plan, verify, and apply a behavior-preserving refactor

`adoption refactor` turns a reviewed candidate patch into an exact, revision-bound plan; `adoption verify` really executes the pinned checks — baseline and candidate runs in disposable copies with an independent behavior oracle — and only execution evidence produced by such a run can authorize the apply.

```sh
lekalo --no-cache --json adoption refactor --use-case complete_case --target node-typescript --profile default --verification verification-spec.json --candidate candidate-patch.json --project . > refactor-plan.json
lekalo --json adoption verify --plan refactor-plan.json --verification verification-spec.json --candidate candidate-patch.json --project . > execution.json
lekalo --json adoption apply --plan refactor-plan.json --verification execution.json --candidate candidate-patch.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('refactor-plan.json','utf8')).plan")" --project . > apply-receipt.json
```

Expected: the plan (exit 0/stdout) binds the candidate patch digest, the verification specification digest, the read/mutation scopes, and the preservation obligations; a plan without a candidate is advice and can never apply. The verification (exit 0/stdout) concludes `preserved` with the oracle `passed` on both sides; a candidate that breaks an oracle obligation concludes `failed`, and apply refuses (`adoption.verification-failed`, LEK-ADOPT-007, exit 1). The apply (exit 0/stdout) installs exactly the reviewed bytes and records the receipt. A drifted tree refuses as `adoption.plan-stale` (LEK-ADOPT-004, exit 3) — before execution and again before install.

What verification evidence proves — and what it does not: the admission re-derives everything a run binds. Measured outcomes must cover the declared checks exactly 1:1, the oracle verdict and the conclusion must follow from the envelope's own measurements, the consumed runner and oracle bytes must still match the live tree, and the run must name this exact candidate — empty, short, invented, stale, swapped, or replayed documents refuse before any write (LEK-ADOPT-007/001). The custody recipe is public, so a deliberate in-tree hand-forgery that recomputes every public member over invented outcomes is a consistency pass, not an authenticity proof; the guarantee is binding and consistency, and the rollback surface owns the restoration. The executor's confinement is run hygiene, not an operating-system boundary: the disposable copy is the child's working directory with a scrubbed environment, bounded output, and a hard deadline — and a specification that requests `no-network` is refused (`adoption.scope-refused`, exit 3) instead of silently run while the claim is dropped.

Declining the preview writes nothing; deleting files works through the same plan (`"action": "delete"` with the exact pre-state digest) and the rollback restores deleted bytes.

## 5. Add a new contracted feature (the new-contracted mode)

New features start canonical: the fixture already declares `taskboard.audit_event` canonically (`lekalo/modules/taskboard/entities.yaml`). Author its maintained implementation, wrap the exact bytes in a reviewed patch document, and run the same plan → verify → apply path.

```sh
cat > src/audit.mjs << 'EOF'
// The maintained implementation of the audit_event obligation.
export function auditEvent(task) {
  return { id: task.id, state: task.state, kind: "task.audited" };
}
EOF
node -e "const fs=require('fs'),c=require('crypto');const canon=(v)=>Array.isArray(v)?'['+v.map(canon).join(',')+']':(v&&typeof v==='object')?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canon(v[k])).join(',')+'}':JSON.stringify(v);const dig=(f,v)=>'sha256:'+c.createHash('sha256').update('lekalo.'+f+'.v0.8.0\n'+canon(v)).digest('hex');const sha=(b)=>'sha256:'+c.createHash('sha256').update(b).digest('hex');const content=fs.readFileSync('src/audit.mjs','utf8');const doc={schema_version:'lekalo/adoption-patch/v0.8.0',identity:'dev.lekalo.adoption-patch@0.8.0',patch:'',origin:{kind:'supplied-reviewed',identity:'tutorial-feature-patch'},scope:{symbol:'taskboard.audit_event',target:'node-typescript',profile:'default'},files:[{path:'src/audit.mjs',action:'create',before:null,after:sha(Buffer.from(content)),content}]};const rest={...doc};delete rest.patch;doc.patch=dig('adoption-patch',rest);fs.writeFileSync('feature-patch.json',JSON.stringify(doc,null,2))"
rm src/audit.mjs
lekalo --no-cache --json adoption refactor --module taskboard --target node-typescript --profile default --verification verification-spec.json --candidate feature-patch.json --project . > feature-plan.json
lekalo --json adoption verify --plan feature-plan.json --verification verification-spec.json --candidate feature-patch.json --project . > feature-execution.json
lekalo --json adoption apply --plan feature-plan.json --verification feature-execution.json --candidate feature-patch.json --confirm "$(node -p "JSON.parse(require('fs').readFileSync('feature-plan.json','utf8')).plan")" --project . > feature-receipt.json
```

Then bring the conformed registry back in sync and declare the new symbol: the applied refactor changed the maintained bytes of `src/tasks.mjs`, so the legacy command must be re-declared over the exact new bytes — until then the conformance gate refuses with `contracted.binding-drift` (LEK-CNT-006) — and the new symbol is declared over the bytes just applied. Both declarations are authored from the live files:

```sh
node -e "const fs=require('fs'),c=require('crypto');const fp=(p)=>'sha256:'+c.createHash('sha256').update(fs.readFileSync(p)).digest('hex');const base={schemaVersion:'lekalo/contracted-declaration/v0.4.0',adapter:{id:'lekalo-target-node-typescript',version:'0.2.16',digest:null},project:'taskboard',revision:'sha256:1361a56d4a5b8e7c1c9f0d2a3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6',artifacts:[]};fs.writeFileSync('declaration-legacy.json',JSON.stringify({...base,symbols:[{id:'taskboard.complete_task',kind:'command',source:{path:'src/tasks.mjs',line:11},fingerprint:fp('src/tasks.mjs'),signature:{inputs:[],output:null,reads:[]},effects:[]}]},null,2));fs.writeFileSync('declaration-feature.json',JSON.stringify({...base,symbols:[{id:'taskboard.audit_event',kind:'scalar',source:{path:'src/audit.mjs',line:3},fingerprint:fp('src/audit.mjs'),signature:null,effects:[]}]},null,2))"
lekalo --json contract update --declaration declaration-legacy.json --project . > contract-reupdate.json
lekalo --json contract update --declaration declaration-feature.json --project . > feature-update.json
lekalo --json contract attach taskboard.audit_event --native-test "test/audit.test.mjs#auditEvent" --project . > feature-attach.json
lekalo --json contract check --project . > feature-check.json
```

Expected: every apply exits 0/stdout with a `completed` receipt; the maintained implementation is hand-written from then on (source-owned), exactly like any maintained body. The authored bytes travel inside the reviewed patch — a `create` write installs under compare-and-swap against typed absence, so the working copy must not hold the file beforehand (that is why the walkthrough removes it after wrapping). The re-update re-records the drifted legacy binding, the feature update records `taskboard.audit_event`, and the check reports both symbols conformant.

## 6. Read the hybrid join

```sh
lekalo --no-cache --json adoption coverage --module taskboard --project . > coverage.json
lekalo --json status --adoption coverage.json --project . > status.json
lekalo --json impact taskboard.complete_task --adoption coverage.json --project . > impact.json
lekalo --json trace query ../tests/fixtures/trace/partial.trace.json gaps --adoption coverage.json > trace.json
```

Expected: coverage (exit 0/stdout) reports the mixed-mode project — `contracted-legacy` for `taskboard.complete_task`, `contracted-new` for `taskboard.audit_event`, and the untouched `unbound` rows beside them — with disjoint aggregate counts and explicit gaps. The coverage document pins the consumed input revisions it was derived from and carries the executed provenance: a row's `verification` names exactly the persisted receipts whose writes touched that row's paths. `status --adoption` re-fingerprints those pins live — a fresh attachment presents `fresh` panels, and a drifted tree downgrades them to the typed `unqualified` freshness instead of presenting stale evidence as fresh. `impact --adoption` adds the mode-boundary witnesses with the same freshness axis, and `trace query --adoption` joins the executed receipt digests plus the stale obligations. Regenerating an attachment never launders a drifted contracted source: after a real edit to `src/tasks.mjs`, `contract check` refuses, a freshly generated coverage downgrades the row to `conflicting` with the typed gap, and the status panel reads `stale`.

## 7. Failure and recovery

A partial application is never reported as complete: every write is snapshotted first, journaled, and installed through compare-and-swap; a mid-apply failure rolls the completed bytes back through the snapshots, and the whole snapshot set is digest-verified before the first restoration — a corrupt snapshot refuses the entire rollback instead of stopping halfway.

When a receipt cannot be persisted after the writes completed, the refusal is the typed recovery contract (`adoption.recovery-required`, LEK-ADOPT-008, exit 1): the journal names every stranded write with its snapshot and pre-state, and the executable path restores them without the receipt. Every plan's durable state is inspectable the same way — here for the completed refactor apply — and the executable recovery defers to the rollback surface while a completed receipt exists (LEK-ADOPT-009, exit 3):

```sh
lekalo --json adoption recovery --digest "$(node -p "JSON.parse(require('fs').readFileSync('apply-receipt.json','utf8')).plan")" --project . > recovery-view.json
lekalo --json adoption recovery --digest "$(node -p "JSON.parse(require('fs').readFileSync('apply-receipt.json','utf8')).plan")" --restore > recovery-refusal.json
```

Verification children run in disposable copies with drained bounded output, process-tree teardown on the deadline, and a scrubbed environment; a missing or unqualified tool concludes `unavailable` honestly and never reports a test as passed, and disposable-copy preparation never leaves project material in the host temp area — not on a failure and not as empty directories on success.

## 8. Clean up

```sh
cd ..
rm -rf .adoption-tutorial
```
