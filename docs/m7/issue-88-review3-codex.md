# Issue #88: Codex independent review round 3

**Verdict: ISSUES.** Fix `880e0639` closes the original target-only relabelling
case, including a store actually applied before the fix. One residual P2
remains: recomputing the public Model ID/revision recipe can admit a fact whose
retained occurrence fields cannot come from that producer. This reproduces in
the current lint authoring/audit workflow, target-capability authoring/audit,
and an independently compiled profile owner inheriting the trait defaults.
Actual native lint remains blocking; this finding concerns neutral admission,
accepted-risk audit/done evidence and the public profile seam.

Reviewed on 2026-10-05 in `ichinya/m7-issue-88`, starting from clean HEAD
`cc1d6f7dd911068ba9d84239e35f150cbdb32c3d`. The fix under review is
`880e06394cc156c47cdc4eb2dcd97190b1e4b08f`. I read the research, implementation,
round-2 review, fix-2 report and Devin round-3 report, then inspected the changed
source, schemas, goldens, shared gate and CI order. Devin's ACCEPT is not evidence
for this verdict. The live issue was fetched again with
`gh issue view 88 --repo ichinya/lekalo --json title,body,state,url,updatedAt`;
[issue #88](https://github.com/ichinya/lekalo/issues/88) is open, with its seven
acceptance criteria and dependency on #84 unchanged.

## 1. [P2] The shared Model-domain recipe remains forgeable for inconsistent native occurrences

**Locations:** `crates/lekalo-core/src/waivers/policy.rs:48` (shared admission),
`:70` and `:74` (trait defaults), `:121` (lint override), and `:176`
(target profile inherits the defaults). Effectiveness consumes the result in
`crates/lekalo-core/src/waivers/mod.rs:415` and `:421`.

The new admission checks an ID computed with the fixed Model depth rule,
canonical Model/IR revision, unsupported adapter/capabilities and unknown path.
The lint override additionally checks the selector ID. However, the ID helper
accepts arbitrary subject and symbol state, and the shared admission never
checks whether those values can be emitted by the recognized Model producer.
The hash is a deterministic consistency calculation, not producer evidence:
the caller can recompute it without any collision or retained native ID.

The current core Model producer always passes a **known** semantic symbol to
`make`, with its dependency-witness subject (`ai_lint/mod.rs:478`–`:491`). The
native depth producer instead passes an opaque native-witness subject and
**unknown** semantic symbol (`:749`–`:757`). Both use
`indirection.depth-exceeded`. This defeats the lint selector guard when the
remaining recipe fields are reconstructed.

I independently emitted one real native depth finding under a CI profile with
only this rule enabled, semantic-depth threshold 100 and native-depth threshold
1. With unsupported capabilities its native waiver correctly refused preview.
I then retained its rule, native subject, unknown symbol, condition digest,
source severity/confidence and evidence references; changed the target to
`model`; recomputed the finding ID and canonical Model/IR revision; and declared
adapter/capabilities unsupported with unknown path. The current project,
Model/IR, profile and lock remained admitted. Normal `add` generated fresh
fact/approval bindings and an exact plan; no schema bypass or hand-edited
approval was involved.

| Current-binary operation | Observed result |
| --- | --- |
| Real native depth lint with `--check` | Exit 3, one blocking finding |
| Add preview for the original native fact | Exit 1, `waivers-ineligible-candidate` |
| Add preview after reconstructing the Model recipe | Exit 0, waived 1 |
| Apply that preview's exact plan with `--locked` | Exit 0, `applied:true` |
| Audit with `--locked --check` | Exit 0, effective true; original gate `denied`, effective gate `accepted-risk`; unknown symbol and unsupported native pins retained |
| Audit with that report's `--done` digest | Exit 0 |
| Mixed original-native and forged facts, either order | Exit 3 overall, but forged fact remains accepted-risk and waived 1 |
| Actual native lint using that store | Exit 3, waived 0; target-bound occurrence does not match the forged entry |

Two adjacent probes establish that this is shared admission behavior:

- Under the actual resolved `node-postgres-http` target profile, a native
  string-reference occurrence projected to optional `transport.streaming`
  refused with its native target and with target-only relabelling. Reconstructing
  the same Model recipe made preview/apply/audit succeed with waived 1 and
  accepted-risk. Its retained subject was the native opaque hash, while its
  symbol remained `planner.focus_task`; this is not the current Model producer's
  witness identity. The genuinely Model-derived capability case also succeeds.
  The optional capability's original gate is advisory, so this is an incorrect
  applicability/acceptance decision, not a required-capability gate bypass.
- A separately compiled external `ProfileState` owner implemented only the two
  required methods, admitting the native `hidden.string-reference` rule as a
  waivable blocking warning. The original real native input audited as
  unverifiable/denied. Reconstructing the Model recipe while retaining that
  native-only selector made the inherited domain check return true, adapter and
  capability requirements false, and shared audit change `denied` to
  `accepted-risk` (waived 1). Unlike `LintState`, the default does not bind the
  selector to an admitted producer domain. Both mixed-input orders still grant
  acceptance to the forged fact; the other native fact keeps the overall result
  denied.

**Impact and boundary:** add can write a committed governance decision and audit
can mint accepted-risk/done evidence without the native provenance required by
`docs/waivers.md`. The trait default can also silently relax applicability for a
future owner that implements only its required methods. This does not suppress
the recomputed real native lint occurrence, downgrade an error/required-evidence
policy, authenticate an external producer or bypass privacy enforcement.
The problem is narrower than arbitrary false supplied reports: these admitted
facts violate the very current producer recipe the fix claims to recognize.
Its Model producer never emits an identity bound to an unknown semantic symbol,
and a native-only
profile need not admit Model ownership merely because a caller computed a hash
with a different producer's fixed rule. No SHA-256 collision is required.

**Correction needed:** make the default conservative and require an admitted
profile/producer binding before relaxing native applicability. Check the full
recognized Model occurrence recipe, including emitted subject/symbol-state
constraints, or establish its source occurrence through admitted evidence.
Derived capability selectors must preserve an established Model source identity;
they should not establish that identity themselves. Keep genuine Model and
Model-derived capability cases working, and apply the same decision to add and
stored-waiver effectiveness. Add live native-depth/unknown-symbol and capability
forgeries, plus a public-trait default control. Do not assume #84's future wire,
hardcode a severity list, weaken closed decoding or change predecessor schemas.
The existing nine producer controls focus on native string-reference facts under
the concrete lint override and do not cover these paths.

### Standalone current-CLI reproduction

Save this `.mjs` **outside the checkout** and run it with Node from the worktree
after the locked CLI build. It uses committed synthetic fixtures as data and
creates an external project; it does not import gate helpers. All assertions
passed against the reviewed binary, including exact-plan apply, source-lock,
done and both mixed-input orders. The unknown-symbol ID binding distinguishes
this occurrence from any finding identity the current Model producer can emit.

```javascript
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {cpSync,mkdtempSync,readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(),project=mkdtempSync(join(tmpdir(),'lekalo-88-r3-depth-'));
const bin=join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
cpSync(join(repo,'tests/fixtures/ai-lint/model'),project,{recursive:true});
const read=p=>JSON.parse(readFileSync(join(repo,p),'utf8'));
const sort=v=>Array.isArray(v)?v.map(sort):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sort(v[k])])):v;
const hash=v=>'sha256:'+createHash('sha256').update(JSON.stringify(sort(v))).digest('hex');
const known=value=>({state:'known',value});
const file=(p,v)=>writeFileSync(join(project,p),JSON.stringify(v)+'\n');
const log=[];
function cli(args,expected){
 const r=spawnSync(bin,['--json','--no-cache',...args],{cwd:project,encoding:'utf8',timeout:60000});
 assert.ifError(r.error);assert.equal(r.status,expected,r.stdout+r.stderr);
 const e=JSON.parse(r.stdout.trim()||r.stderr.trim());log.push({args,exit:r.status,envelope:e});
 file('review-results.json',log);return e.payload?{...e,...e.payload}:e;
}
const config=read('tests/fixtures/ai-lint-config/golden/config.json');
for(const p of config.profiles){
 for(const r of p.rules)r.enabled=p.id!=='off'&&r.id==='indirection.depth-exceeded';
 p.thresholds.semanticDependencyDepth=known(100);
}
file('config.json',config);cli(['lock'],0);
const lint=['ai-lint','--module','planner','--config','config.json','--lint-profile','ci','--waiver-facts'];
const model=cli(lint,0).report;
const evidence=read('tests/fixtures/ai-lint-evidence/golden/evidence.json');
evidence.pins.model=known(model.modelRef);evidence.pins.ir=known(model.irRef);
evidence.pins.capabilities={state:'unsupported'};file('evidence.json',evidence);
const native=[...lint,'--evidence','evidence.json'];
const raw=cli([...native,'--check'],3);assert.equal(raw.waiverInput.facts.length,1);
const input=raw.waiverInput,f=input.facts[0],original=structuredClone(f);
assert.equal(f.selector.id,'indirection.depth-exceeded');assert.equal(f.target,'node-typescript');
assert.equal(f.symbol.state,'unknown');file('facts.json',input);
const common=['--facts','facts.json','--profile-kind','ai-lint','--lint-config','config.json','--profile','ci','--locked'];
const now='2026-10-05T12:30:00Z';
const add=()=>['waivers','add',f.selector.id,...common,'--as-of',now,'--id','native-depth-model-recipe',
 '--project-scope','--target',f.target,'--subject',f.subject,'--owner','fixture-owner',
 '--approver','fixture-reviewer','--approval-ref','review/88/r3','--reason','Synthetic producer recipe probe.',
 '--source-issue','ichinya/lekalo#88','--expires','2026-10-05T12:30:10Z'];
assert.ok(JSON.stringify(cli(add(),1)).includes('waivers-ineligible-candidate'));
f.target='model';f.id=hash([f.selector.id,f.subject,'model',f.symbol]);
f.fingerprint.revision=known(hash([f.fingerprint.model.value,f.fingerprint.ir.value]));
f.fingerprint.adapter={state:'unsupported'};f.fingerprint.capabilities={state:'unsupported'};
f.path={state:'unknown'};file('facts.json',input);
assert.equal(f.subject,original.subject);assert.deepEqual(f.symbol,original.symbol);
assert.equal(f.conditionDigest,original.conditionDigest);assert.deepEqual(f.selector,original.selector);
const preview=cli(add(),0);assert.equal(preview.audit.summary.waived,1);
assert.equal(cli([...add(),'--apply',preview.planId],0).applied,true);
const auditArgs=['waivers','audit',...common,'--as-of',now,'--check'];
const audit=cli(auditArgs,0).audit;
assert.equal(audit.findings[0].originalGate,'denied');assert.equal(audit.findings[0].effectiveGate,'accepted-risk');
assert.equal(audit.entries[0].effective,true);assert.equal(audit.findings[0].fact.symbol.state,'unknown');
cli([...auditArgs,'--done',audit.doneDigest],0);
const live=cli([...native,'--waivers','lekalo.waivers.json','--as-of',now,'--check'],3);
assert.equal(live.report.summary.waived,0);
for(const reverse of [false,true]){
 input.facts=[structuredClone(f),structuredClone(original)];if(reverse)input.facts.reverse();file('facts.json',input);
 const mixed=cli(auditArgs,3).audit;assert.equal(mixed.summary.waived,1);
 assert.equal(mixed.findings.find(d=>d.fact.id===f.id).effectiveGate,'accepted-risk');
}
console.log(JSON.stringify({project,calls:log.length,nativePreview:1,forgedPreview:0,forgedApply:0,
 audit:0,done:0,originalGate:audit.findings[0].originalGate,effectiveGate:audit.findings[0].effectiveGate,
 forgedSymbol:f.symbol,realNativeLint:3}));
```

The public-default probe was an external Cargo project, with a path dependency
on this checkout's `lekalo-core`, using the same public `waivers::audit` and
`approval_subject` functions as the implementation. Its owner was exactly:

```rust
struct NativeOnly;
impl ProfileState for NativeOnly {
    fn reference(&self) -> ProfileRef {
        ProfileRef { id: "review-native-only".into(), version: "1".into(),
            digest: hash(&"review native-only policy") }
    }
    fn rule(&self, selector: &Selector, _fact: &Fact) -> Option<RuleState> {
        (selector.kind == SelectorKind::Rule && selector.id == "hidden.string-reference")
            .then_some(RuleState { enabled: true, severity: Severity::Warning,
                required_evidence: false, blocking: true, waivable: true })
    }
}
```

It parsed an independently emitted native input, rebound its exact profile to
this owner, and constructed a matching project-scoped entry with a correct fact
digest and approval subject. The original fact remained unverifiable. Applying
the same recipe reconstruction as above, with the fixed Model depth rule in
the ID hash but the hidden-string selector retained, made admission true and
audit effective. The executable assertions also checked immutable returned
facts, the five requirement flags and both mixed orders. Full source, manifest,
input and JSON results are retained in the external evidence directory below.

## Original residual: independently verified correction

The round-2 external project was copied rather than rewritten. Its captured old
apply result establishes `applied:true` and its old audit establishes waived 1.
The old store, facts, preview/apply arguments and done digest were then replayed
against the rebuilt current binary.

| Requested probe | Current result |
| --- | --- |
| Old-build applied target-only relabelled store | Locked audit exit 3, `unverifiable`, `fingerprint-unverifiable`, effective false, waived 0; store bytes unchanged |
| Old preview and exact old apply plan, store absent | Both exit 1, `waivers-ineligible-candidate`; no store created |
| Old accepted-risk done digest | Exit 1, `waivers-done-stale` |
| Actual native lint with the old store | Exit 3, waived 0 |
| Newly emitted hidden-string fact, target-only relabel | Preview/apply exit 1, stored matching audit unverifiable, exit 3; actual lint exit 3, waived 0 |
| Recompute native hidden-string ID with Model target only | Same refusal and ineffective stored audit |
| Reconstruct complete Model recipe but retain hidden-string selector under concrete lint profile | Same refusal, including copying genuine Model binding fields; lint selector override is effective |
| Genuine freshly emitted Model fact | Locked preview/apply/audit and actual lint exit 0, waived 1 |
| Genuine Model-derived optional capability fact | Preview/apply/audit exit 0, waived 1; unsupported capability outcome retained |
| Mixed genuine Model and invalid hidden-string facts, either order | Exit 3, only genuine Model waived; native occurrence remains unwaived |
| Native revision unsupported, unknown or withheld | Preview exit 1; actual matching stored lint waiver unverifiable, waived 0, exit 3 |
| Applicable native capability unknown/withheld, known pins and mismatches | Retained live gate controls pass: incomplete pins refuse, known pins accept, changed producer/revision pins are stale |

**Live status distinction:** the actual native lint path regenerates the native
target and finding ID. A relabelled stored entry therefore has no matching
occurrence, and the incomplete producer input reports it as `unexamined`, not
`unverifiable`. The matching supplied-input audit reports `unverifiable` with
the fingerprint reason. Both paths are ineffective and blocking as required;
I do not claim the requested literal `unverifiable` status was observed in live
lint for a nonmatching old entry. The original F1 incomplete-pin stores that
still match native occurrences remain unverifiable in actual lint.

## Acceptance evidence and limits

| Issue acceptance criterion | Current round evidence / remaining limit |
| --- | --- |
| 1. Visible fact and waiver link | Live gates retain native fields and raw counts; independent genuine Model lint remains visible and waived. Neutral forged acceptance is finding 1. |
| 2. Expiry restores gate | Retained live gate tests deadline equality and the next second, including actual blocking lint. No new expiry-policy defect found. |
| 3. No unrelated-symbol leakage | Retained exact occurrence/scope and hostile replacement controls pass. Independent mixed probes do not waive the actual unrelated native occurrence. |
| 4. Policy-controlled Model/adapter revision invalidation | Known-change and incomplete-pin controls pass, but finding 1 leaves the native applicability/provenance assurance incomplete at neutral admission and the trait default. |
| 5. Profile-owned non-waivable security rule | Current live gates test real validation-profile error selection and lint required coverage; all pass. No hardcoded severity list or future #84 contract was used. |
| 6. Active/expiring/expired/stale audit | Current schema/golden/live status controls pass. Finding 1 can incorrectly report an effective active/expiring approval for an inconsistent producer claim. |
| 7. Review/done waiver evidence | Current change/digest/lock controls pass and old done evidence refuses. The new forged native-depth acceptance also produces a valid done digest, so digest integrity does not establish producer admission. |

This is a focused review of fix 2 and adjacent provenance/default-policy
boundaries, not a new full-system acceptance. The supplied-report ownership
boundary remains explicit; external approvals, arbitrary external producer
truth, privacy exports and HLV/AIFHub execution were not verified. Parallel #84
was neither awaited nor assigned an assumed wire; the probe exercises the
existing public seam its future owner could inherit.

## Build, gates and delivery custody

All requested checks passed against the reviewed source and freshly built CLI:

```powershell
cargo build --locked -p lekalo-cli
cargo test --locked waivers
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
node scripts/gen-waivers-contracts.mjs --check
cargo fmt --all -- --check
```

| Check | Observed result |
| --- | --- |
| Locked CLI build | Passed, development debug profile |
| Filtered `cargo test --locked waivers` | 12 matching core tests passed, zero failed; other test binaries were filtered, not full-suite runs |
| Three successor gates | Each passed, `live:true`, exact Ajv 8.17.1, 39 retained audits, `nativePinControls:7`, `producerDomainControls:9` |
| Predecessor waiver gate | Passed, `live:true`, schema 0.6.4 |
| Generator and format | Both read-only checks passed |
| Independent CLI probes | 57-call own matrix plus 11-call native-depth reproducer; all executable assertions passed |
| Public API/default probe | External offline Cargo build/run passed; original unverifiable/denied, forged active/accepted-risk, waived 1; both mixed orders retain the false acceptance |
| Independent closed-schema validation | Strict Ajv 8.17.1 validated the matrix input/store/audits and 13 additional depth/public-API artifacts; the bypass does not rely on an invalid wire shape |
| Frozen boundaries | `git diff --exit-code f71a5b5b HEAD -- contracts tests/fixtures .github/workflows/ci.yml` passed; schemas, goldens/provenance and CI unchanged by fix 2 |
| CI order | Exact Ajv install at `ci.yml:198`, Cargo build at `:204`, predecessor gate at `:273`, successor gates at `:280`–`:282` |

Independent scripts, raw envelopes, assertions and API source/results are under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-review3-ce814ee8706c46deb61071c7b0c1f1ee`.
The standalone native-depth project is
`C:/Users/User/AppData/Local/Temp/lekalo-88-r3-depth-5azttW`, with all eleven calls
in `review-results.json`. The external API executable was run with
`cargo run --offline --quiet --manifest-path <evidence>/api/Cargo.toml -- <evidence>/native-facts.json`.
The original round-2 evidence directory was left untouched.

Passing existing gates does not cover finding 1. Full unfiltered suites, Clippy,
hosted CI and Linux/macOS were not run in this round. Implementation, schemas,
goldens, gates and other worktrees were not edited. The only repository delivery
is this review document, committed locally with
`docs(m7): codex review round 3 for issue 88`; no push.
