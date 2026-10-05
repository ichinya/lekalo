# Issue #88: Codex independent review round 2

**Verdict: ISSUES.** The original native-lint P2 is fixed, including replay of
stores actually applied by the old build. One residual P2 remains in neutral
fact admission: changing an exported native fact's target to `model` grants
Model-only fingerprint applicability in `add` and standalone `audit`. It can
produce an accepted-risk audit and done receipt without an applicable native
capability pin. Actual native lint remains blocking for that relabelled waiver.

Reviewed on 2026-10-05 in `ichinya/m7-issue-88`, starting from clean HEAD
`c94dbe223e6c71b2250954514292a9d21c5d2a36`. The implementation under review is
`faa220fa48780bb8e2307cb2c862646dfbd1a452`, with correction
`92dc6d54795be4aae7873c10f0fc3d8289fa2ea9`. I read the research, implementation,
round-1 review, fix report and Devin round-2 report. Devin's ACCEPT was a claim
to test, not verification evidence. The live issue was fetched again with
`gh issue view 88 --repo ichinya/lekalo --json title,body,state,url,updatedAt`;
[issue #88](https://github.com/ichinya/lekalo/issues/88) remains open.

## 1. [P2] A supplied target label can bypass native fingerprint applicability in add/audit

**Location:** `crates/lekalo-core/src/waivers/policy.rs:53`, particularly the
`fact.target == "model"` branch at line 56. The admission gap is
`crates/lekalo-cli/src/waivers.rs:234` and
`crates/lekalo-core/src/waivers/mod.rs:251`; effectiveness consumes that decision
at `mod.rs:415`.

The new matcher correctly requires known pins once native applicability is
selected. However, the default profile method selects applicability solely from
the caller-supplied target string. Neutral input admission verifies current
Model/IR, profile, project and known symbols/modules, but does not establish
that a fact claiming the reserved Model target belongs to the Model producer
domain. Unlike native evidence admission, it never reaches
`ai_lint/input.rs:205`.

I produced one real native `hidden.string-reference` finding with current
Model/IR and known adapter/revision pins, but unsupported capabilities. The
correctly labelled exported input refused `waivers add`. Changing **only**
`facts[0].target` from `node-typescript` to `model` made preview and apply succeed.
The native finding ID, subject, condition digest, source path, source severity,
confidence, evidence references and every fingerprint remained identical.
The command generated the new approval and fact bindings normally; there was no
hand-edited approval, stale receipt or schema bypass.

This is an inconsistent producer-domain claim, not a request to authenticate
every external report. This lint rule is emitted from native StringReference
records (`ai_lint/mod.rs:490`, `:598`), and its emitted finding ID binds the native
target (`:161`). A Model label does not turn that native occurrence into a
Model-only fact. Producer ownership of supplied evidence, documented in
`docs/waivers.md:59`, does not establish the policy's claim of inapplicability.
The stated requirement that native facts have all five known dimensions
(`docs/waivers.md:93`) is therefore enforced on direct native lint, but remains
avoidable through the neutral authoring/audit surface.

| Operation on the independently produced occurrence | Observed result |
| --- | --- |
| Native `ai-lint --check`, unsupported capability pin | Exit 3, one active finding |
| Add preview with the original native target | Exit 1, `waivers-ineligible-candidate` |
| Change only the neutral fact target to `model`; preview with `--locked` | Exit 0, effective candidate |
| Apply that preview's exact plan with `--locked` | Exit 0, canonical root store written |
| Audit with current profile, `--locked --check` | Exit 0; original gate `denied`, effective gate `accepted-risk`; waived 1; unsupported capability pin retained |
| Audit with that audit's `--done` digest and `--locked --check` | Exit 0 |
| Actual native lint using the written waiver | Exit 3, waived 0; entry `unexamined` because its target does not match the actual occurrence |

**Impact and boundary:** the neutral workflow admits a committed decision and
machine-readable accepted-risk evidence with incomplete applicable provenance.
This affects add eligibility and standalone audit/review evidence. It does
**not** suppress the actual native lint finding: the native decoder rejects
`target:"model"`, and actual lint recomputes the correct target and exact match.
Standalone audit is documented as a companion to producer gates; I do not claim
that it replaces or bypasses those gates. The residual defect is its own
applicability/admission decision, including the false acceptance receipt.

The same neutral path accepted unsupported adapter alone and unsupported
adapter plus capabilities. Unsupported revision still refused under the Model
requirements. A mixed input containing a genuine Model fact and this relabelled
native fact also accepted both occurrences (waived 2, exit 0). Correctly labelled
mixed input remained denied in both array orders.

**Correction needed:** establish Model-only applicability from admitted producer
domain and resolved profile/recipe state, rather than granting it from a free
target label. Reject inconsistent Model claims, or keep them unverifiable when
that domain cannot be established. Apply the same rule at add and effectiveness
so pre-existing relabelled stores cannot grant acceptance. Retain genuine Model
inapplicability, unknown/withheld refusal and native known-pin controls. Add live
neutral-input relabelling and mixed-input controls; the current control at
`scripts/lib/waivers-contract-gate.mjs:79` tests only native Evidence admission.
Use the existing profile seam without assuming parallel #84's future contract
names or weakening either wire version.

### Standalone reproduction

Save the following as an `.mjs` file **outside the checkout** and run it with
Node from this worktree after the locked CLI build. It uses committed synthetic
fixtures as data, not the contract-gate helpers. It creates its project outside
the checkout. All assertions passed against this reviewed binary, including
the locked source-run and done controls.

```javascript
import assert from 'node:assert/strict';
import {cpSync,mkdtempSync,readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(),project=mkdtempSync(join(tmpdir(),'lekalo-88-r2-domain-'));
const bin=join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
cpSync(join(repo,'tests/fixtures/ai-lint/model'),project,{recursive:true});
const read=p=>JSON.parse(readFileSync(join(repo,p),'utf8'));
const file=(p,v)=>writeFileSync(join(project,p),JSON.stringify(v)+'\n');
function cli(args,expected){
 const r=spawnSync(bin,['--json','--no-cache',...args],{cwd:project,encoding:'utf8',timeout:60000});
 assert.ifError(r.error);assert.equal(r.status,expected,r.stdout+r.stderr);
 const e=JSON.parse(r.stdout.trim()||r.stderr.trim());
 return e.payload?{...e,...e.payload}:e;
}
const config=read('tests/fixtures/ai-lint-config/golden/config.json');
for(const p of config.profiles)for(const r of p.rules)
 r.enabled=p.id!=='off'&&r.id==='hidden.string-reference';
file('config.json',config);
cli(['lock'],0);
const lint=['ai-lint','--module','planner','--config','config.json','--lint-profile','ci','--waiver-facts'];
const model=cli(lint,0).report;
const evidence=read('tests/fixtures/ai-lint-evidence/golden/evidence.json');
evidence.pins.model={state:'known',value:model.modelRef};
evidence.pins.ir={state:'known',value:model.irRef};
evidence.pins.capabilities={state:'unsupported'};file('evidence.json',evidence);
const native=[...lint,'--evidence','evidence.json'];
const raw=cli([...native,'--check'],3);assert.equal(raw.report.summary.raw,1);
const input=raw.waiverInput,f=input.facts[0],original=structuredClone(f);
file('facts.json',input);
const common=['--facts','facts.json','--profile-kind','ai-lint','--lint-config','config.json','--profile','ci','--locked'];
const now='2026-10-05T12:30:00Z';
const add=()=>['waivers','add',f.selector.id,...common,'--as-of',now,'--id','native-model-domain',
 '--symbol',f.symbol.value,'--target',f.target,'--subject',f.subject,'--owner','fixture-owner',
 '--approver','fixture-reviewer','--approval-ref','review/88/r2','--reason','Synthetic producer domain probe.',
 '--source-issue','ichinya/lekalo#88','--expires','2026-10-05T12:30:10Z'];
assert.ok(JSON.stringify(cli(add(),1)).includes('waivers-ineligible-candidate'));
f.target='model';file('facts.json',input);
const unchanged=structuredClone(f);unchanged.target=original.target;assert.deepEqual(unchanged,original);
const preview=cli(add(),0);assert.equal(preview.audit.summary.waived,1);
assert.equal(cli([...add(),'--apply',preview.planId],0).applied,true);
const auditArgs=['waivers','audit',...common,'--as-of',now,'--check'];
const audit=cli(auditArgs,0).audit;
assert.equal(audit.findings[0].originalGate,'denied');
assert.equal(audit.findings[0].effectiveGate,'accepted-risk');
assert.equal(audit.entries[0].effective,true);
assert.equal(audit.findings[0].fact.fingerprint.capabilities.state,'unsupported');
cli([...auditArgs,'--done',audit.doneDigest],0);
const live=cli([...native,'--waivers','lekalo.waivers.json','--as-of',now,'--check'],3);
assert.equal(live.report.summary.waived,0);
console.log(JSON.stringify({project,nativePreview:1,relabelPreview:0,relabelApply:0,
 audit:0,done:0,effectiveGate:audit.findings[0].effectiveGate,realNativeLint:3}));
```

## Original F1: independently verified correction

The original own native producer recipe was replayed in a fresh fixture copy,
not by trusting the new gate's oracle. Six cases varied capability/revision pins
across unsupported, unknown and withheld. Every case refused add preview and
apply with exit 1 and `waivers-ineligible-candidate`, preserving store bytes.
Separately approval-bound matching stores for those exact incomplete facts
reported `unverifiable`, effective false, reason `fingerprint-unverifiable`,
waived 0, exit 3 in both standalone audit and actual lint. The actual lint
findings were unchanged from the raw blocking findings.

I also replayed **both actual old-build applied candidates**, capability and
revision, using the captured round-1 input, candidate, apply result and plan.
The captures establish old `applied:true`, exit 0; their current effectiveness
was evaluated afresh. Replaying preview and the old apply command against an
absent root store now refused with `waivers-ineligible-candidate` and created
no store. Restoring each pre-existing candidate unchanged produced
`unverifiable` / `fingerprint-unverifiable`, waived 0, exit 3 in standalone audit
and live lint. Audit/lint left those old store bytes unchanged. This isolates
missing applicable pins from expiry, stale facts and invalid approval binding.

| Requested control | Independent current result |
| --- | --- |
| Known native pins | Preview and exact-plan apply exit 0; one blocking finding becomes waived, lint exit 0 |
| Expiry boundary | Equality with deadline remains effective; one second later lint returns to exit 3, waived 0 |
| Unsupported capability/revision | Preview/apply exit 1; matching stored waiver unverifiable in audit and live lint, exit 3 |
| Old-build stored waivers | Both old applied candidates remain structurally admitted but cannot change disposition; audit/lint exit 3 |
| Unknown/withheld capability/revision | Same refusal and unverifiable results; neither state grants acceptance |
| Known mismatches | Actual adapter producer-version, source revision and capability-map changes each yield `stale`, the named pin plus fact mismatch, waived 0, lint exit 3 |
| Native Evidence claims Model target | Exit 1, `evidence-scope`; no applicability bypass through the native evidence decoder |
| Correctly labelled mixed producer input | Genuine Model waiver remains effective; incomplete native fact remains unwaived; audit exit 3 in either fact order |
| Capability facts with a native target | Unsupported/unknown/withheld adapter and capability pins refuse add; stored entries are unverifiable and waived 0 under the actual resolved target profile |
| Capability facts with Model target | Optional `transport.streaming` with matching unsupported adapter/capabilities is eligible; unknown/withheld still refuse. Its original gate is advisory, so audit exit 0 for an ineffective optional waiver is not a blocking-gate bypass. Applicability depends on the same target-domain admission gap in finding 1 |

The fixed native path preserves diagnostic facts and gate restoration. Finding 1
narrows the fix report's admission assurance: the native Evidence check does not
also admit the neutral report's producer domain. It leaves the required-known
provenance claim incomplete on add/audit, and can contaminate the AC4/AC7 audit
and review/done evidence for supplied facts. No independent acceptance of a
future #84 profile wire or external producer identity is claimed.

## Build, gates and custody

All requested checks passed locally against the reviewed source and rebuilt CLI:

```powershell
cargo build --locked -p lekalo-cli
cargo test --locked -p lekalo-core waivers:: --lib
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
cargo test --locked waivers
node scripts/gen-waivers-contracts.mjs --check
```

| Check | Observed result |
| --- | --- |
| Locked CLI build | Passed, development debug profile |
| Focused core waiver suite | 9 passed, 0 failed |
| Requested `cargo test --locked waivers` | Passed; 9 matching core tests passed, other test binaries had their tests filtered out; this is not a full workspace-suite run |
| Three successor gates | Each passed with `live:true`, exact Ajv 8.17.1, 39 retained audits and `nativePinControls:7` |
| Predecessor waiver gate | Passed with `live:true`, schema 0.6.4 |
| Generator/frozen boundaries | Generator check passed; read-only Git comparison with round-1 HEAD showed no fix changes to contracts, CI workflow or four gate entrypoints |
| CI inspection | Existing exact Ajv installation at `ci.yml:198`, Cargo build at `:204`; predecessor gate at `:273`, three successor gates at `:280`–`:282` |

The independent matrix completed 78 real CLI calls, with strict Ajv 8.17.1
validation of input/store/audit shapes. A separate committed-fixture reproducer
completed nine calls, including `--locked` and `--done`; an additional mixed
relabelling audit confirmed accepted-risk for both facts. Temporary scripts,
raw envelopes and results are outside the checkout under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-review2-ec76c0f183cc43068155a9e9c312921f`.
The locked minimal reproduction project is
`C:/Users/User/AppData/Local/Temp/lekalo-88-r2-domain-zXjxpu`, with its complete
invocation log in `review-repro-results.json`. The original round-1 captured
old-build stores remain untouched in their external review directory.

This is synthetic local Windows verification. Full unfiltered suites, hosted
CI, Linux/macOS, external approval issuers and HLV/AIFHub execution were not run
in this review. Passing gates do not cover the neutral relabelling finding.
Implementation, schemas, goldens, gates and other worktrees were not edited.
The only repository delivery is this review file, committed locally with
`docs(m7): codex review round 2 for issue 88`; no push.
