# Issue #102: Codex independent review, round 2

**Verdict: ACCEPT. Both round-one P2 findings are resolved. No new finding
was reproduced in this review.** Reviewed fixes `78adfc6c` and `eeb4215a` on
`ichinya/m7-issue-102`, 2026-10-05. Review starting HEAD:
`b56dba7a915e0da60f5c68b0413137de187f6e71` (Devin round-two review);
implementation delta: `dabb7276..eeb4215a`. The worktree/index were clean.
Only this document is changed and committed. No implementation edit, push,
or other worktree mutation.

Authority: [live issue #102](https://github.com/ichinya/lekalo/issues/102),
refreshed with `gh issue view 102 --repo ichinya/lekalo --json number,title,state,body,url`.
Read the [research](issue-102-research.md), [implementation](issue-102-implementation.md),
[Codex round one](issue-102-review1-codex.md), [fix report](issue-102-fix1.md)
and [Devin round-two ACCEPT](issue-102-review2-devin.md). Reports were inputs
to challenge; the verdict below uses current source, rebuilt binary,
unchanged schemas, requested gates and fresh independent hostile probes.

## Prior findings and residual controls

1. **F1 [P2], repository-store same-origin contradiction: resolved.**

   `crates/lekalo-core/src/metrics_export/privacy.rs:17` derives one consumer
   token, using it for both endpoints only for `RepositoryStore`. The original
   `DestinationSpec::resolve` supplies the operation, audience and boundary.
   Fresh synthetic, purpose-bound aggregation/declassification/consent
   evidence makes repository storage ready; actual confirmation writes the
   package and authorized live status returns `valid-at-export`.

   The independent probe also obtains ready for the other five destinations.
   Workspace omits transfer consent; transfers retain distinct endpoints.
   Changing a repository-store endpoint, then refreshing all evidence
   bindings, still returns exit 3 and
   `repository.same-origin-contradiction` from real `privacy evaluate`.
   Repository-store authorization supplied to transfer-external returns
   exit 3 / `metrics-export.authorization-refused`. An unapproved destination
   fails CLI parsing with exit 1. The frozen contradiction refusal at
   `privacy/evaluate.rs:1193` is unchanged.

   | Destination | Operation / boundary | Independent authorized preview |
   | --- | --- | --- |
   | workspace | local-use / same-local-workspace | ready |
   | repository-store | repository-store / same-repository | ready |
   | transfer-tenant | transfer / same-tenant | ready |
   | transfer-external | transfer / cross-repository | ready |
   | transfer-cross-tenant | transfer / cross-tenant | ready |
   | publish | publish / public | ready |

2. **F2 [P2], schema-invalid but digest-valid sources: resolved.**

   `metrics_export/mod.rs:180` and `:210` invoke the full frozen assertion
   and record validators before aggregation. `source_schema.rs:21` compiles
   Draft 2020-12 from the exact embedded `run-record.schema.v0.4.0.json` and
   `run-assertions.schema.v0.4.0.json`. Parse, compilation and validation
   failures close admission; only the generic source refusal is emitted.
   All previous semantic/header/scope/assertion-count/linkage checks remain.

   The new independent probe ingests ten runs through the real recorder,
   then separately tests four record and four assertion mutations: extra
   root/nested prompt fields and wrong types for recordedAt,
   measurementSources, rows and dataSensitivity. Exact Ajv 8.17.1 first
   rejects each document against its unchanged frozen schema. The probe
   recomputes database body digests; assertion cases also rebind the parent
   assertion digest and rehash its record. Each parent remains schema-valid.
   Both dry-run (with and without supplied authorization) and confirmation
   refuse with exit 3 / `metrics-export.source-invalidated`. No canary enters
   refusals and no aggregate dependent is registered. Restoration permits
   a ready preview and repository-store confirmation.

   Status revalidation at `mod.rs:508` is independently exercised beyond a
   changed-hash test. The probe updates both stored body digests, matching
   `dependent_sources` digests and private custody source digests, keeping
   the historical package and authorization intact. A schema-valid timestamp
   control returns `valid-at-export`; extra root fields and wrong types in
   records/assertions return `invalidated`. Restoring the original bytes and
   custody restores valid status. This emulates coherent legacy custody in
   a disposable store, with all preceding digest/liveness checks satisfied.
   The retained original round-one invalid-assertion package also reports
   `invalidated` with the current binary and supplied authorization.

## Dependency and preservation audit

`crates/lekalo-core/Cargo.toml:19` pins `jsonschema = "=0.29.1"` with
`default-features = false`. `Cargo.lock:554` records version `0.29.1`, registry
source and checksum
`161c33c3ec738cfea3288c5c53dfcdb32fd4fc2954de86ea06f71b5a1a40bfcd`.
`cargo tree --locked -p lekalo-core -e features -i jsonschema` confirms no
jsonschema default/HTTP/file/async feature activation.
`source_schema.rs:11` implements a retriever that refuses every external
URI; `:27` installs it. Only embedded schemas/internal refs resolve. The
focused unit test actually rejects HTTP and file references and malformed
schema input, in addition to valid/invalid record and assertion documents.

`git diff --exit-code dabb7276..HEAD -- contracts crates/lekalo-core/src/privacy
crates/lekalo-core/src/run_history .github/workflows/ci.yml tests/fixtures`
returns no differences. Frozen evaluator/policy/schema/golden/provenance
surfaces and #121 code are unchanged in this fix round. The metrics diff adds
validation calls and status validation; it removes no prior checks.
`Store::get` (`run_history/store.rs:816`) still scopes both SQL reads,
rehashes both body types and checks frozen record references. Snapshot
continues checking selected run identity/scope, assertion run/set/digest
linkage and row count (`mod.rs:131`, `:154`, `:194`). Independent mismatched
raw digests, cross-scope selection, schema-valid wrong assertion runId and
wrong assertion count all refuse. The gate retains its original controls
and adds six-destination and full-schema vectors; no expectations relax.

## Acceptance-criterion coverage

The combined live gate was rerun against the rebuilt binary without golden
authoring. This fix review concentrates independent probes on F1/F2; the
other ACs also retain their round-one evidence and unchanged owners.

| Live issue AC | Current evidence / boundary |
| --- | --- |
| AC1: versioned #121 records with #120 policy only | Full schema admission plus preserved Store::get custody, frozen refs, scoped selection and linkage. Independent schema-invalid/digest-valid and schema-valid linkage negatives refuse. |
| AC2: exact payload/redaction diff before publishing | Gate checks preview/written schemas, exact digests/bytes, no preview writes, confirmation binding and cancellation. Independent repository-store confirmation and valid status pass. |
| AC3: forbidden source/prompt/secret/PII/private-id/path absent | Unchanged typed projection/redaction/leak refusal controls pass. Independent source canaries never enter refusals or released payload; run/scope ids remain absent. |
| AC4: missing values unknown/withheld, never zero | Gate preserves known zero, unknown/unsupported/withheld handling, minimum sample and small-cell suppression controls. |
| AC5: definitions/sample sizes reproducible | Unchanged embedded recipe/equality, ordering, sample, arithmetic and preview-digest controls pass. |
| AC6: deletion invalidates aggregate/manifests | Live gate retains delete/clear/prune/retention/recovery controls. Status now additionally refuses coherent schema-invalid legacy sources; independent valid control/restoration pass. |
| AC7: negative/neutral results retained | Combined gate executes negative and neutral synthetic comparisons, outages and incomplete-cell controls. No measured Framework Lift is asserted. |
| AC8: future AIFHub import shape | All five closed families, canonical goldens, current CLI payload/manifest and provenance pass exact Ajv. Candidate import shape only; no external AIFHub service acceptance. |

## Commands and observed results

```powershell
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
node scripts/test-metrics-export-contracts.mjs
cargo test --locked -p lekalo-core --lib metrics_export -- --test-threads=1
cargo fmt --all --check
cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings
node scripts/test-privacy-evaluator-parity.mjs
node scripts/test-privacy-runtime-cli.mjs
node scripts/test-run-history-contracts.mjs
node scripts/test-run-history-cli.mjs
node scripts/check-contract-versions.mjs --base dabb7276
node scripts/test-fixture-provenance.mjs
```

All pass locally on Windows with Rust/Cargo 1.98.0, Node 24.13.0 and exact
Ajv 8.17.1. Focused Rust result: **7 passed, 0 failed, 1053 filtered out**.
Metrics: five families, real binary, original 500 entries preserved/508 total;
the gate includes 14 rehashed record and six rehashed assertion mutations.
Privacy: 120 parity vectors and 13 runtime cases. History: four schemas,
11 valid/16 invalid fixtures and real offline CLI flow. Contract versions:
product 0.6.4/122 artifacts. Provenance: 78 synthetic families.
CI still installs exact Ajv before the build and executes every metrics
family gate after `cargo build --workspace --locked` (`ci.yml:204`, `:295`).
Hosted CI and the full workspace suite were not run in this review.

[#100](https://github.com/ichinya/lekalo/issues/100) remains OPEN in this
review's live refresh; #119/#120/#121 are CLOSED. The typed selection's
approved bit remains metadata. These are synthetic records and declared
synthetic #119 evidence, without issuer-authentication, remote publication,
real evaluation execution, native Rust 1.80 or Unix qualification claims.

## Independent reproduction

The following standalone harness does not call the metrics gate or its
aggregation code. It reuses committed synthetic recorder fixtures and the
shipped privacy evidence-binding helper. Save it outside the checkout and
run with the same NODE_PATH and this worktree as its first argument:

```powershell
node C:/Users/User/AppData/Local/Temp/lekalo-102-review2-codex-20261005.mjs C:/Users/User/orca/workspaces/lekalo/m7-issue-102
```

Observed result: `ok:true`, six ready destinations, eight source-schema
refusals, four schema-invalid/coherent-custody status invalidations,
preserved digest/scope/linkage refusals, written repository-store package
and restored valid status. Retained disposable project:
`C:/Users/User/AppData/Local/Temp/lekalo-102-review2-codex-RbPVFO`.
Direct database/custody mutations are adversarial test setup requiring
local write access; they are not a supported ingestion interface.

```javascript
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join, resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {DatabaseSync} from 'node:sqlite';
const repo=resolve(process.argv[2]);
const {authorizingEvidence,refreshEvidenceBindings}=await import(pathToFileURL(join(repo,'scripts/privacy-test-helpers.mjs')));
const require=createRequire(import.meta.url);assert.equal(require('ajv/package.json').version,'8.17.1');
const Ajv=require('ajv/dist/2020').default;
const ajv=new Ajv({strict:true,allErrors:true,allowUnionTypes:true});
const fixture=p=>JSON.parse(fs.readFileSync(join(repo,p),'utf8'));
const vr=ajv.compile(fixture('contracts/run-record.schema.v0.4.0.json'));
const va=ajv.compile(fixture('contracts/run-assertions.schema.v0.4.0.json'));
const project=fs.realpathSync.native(fs.mkdtempSync(join(tmpdir(),'lekalo-102-review2-codex-')));
const bin=join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={PATH:process.env.PATH,...(process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{})};
const sha=v=>'sha256:'+createHash('sha256').update(v).digest('hex');
const bytes=v=>Buffer.from(JSON.stringify(v)+'\n');
const put=(p,v)=>fs.writeFileSync(join(project,p),bytes(v));
function run(args,{input,code=0,projectArg=true}={}){
 const r=spawnSync(bin,['--json',...args,...(projectArg?['--project','.']:[])],{cwd:project,env,input,encoding:'utf8',timeout:20000,maxBuffer:4194304});
 assert.equal(r.status,code,`${args.join(' ')}: ${r.stdout} ${r.stderr}`);
 if(code===0){assert.equal(r.stderr,'');return JSON.parse(r.stdout);}
 assert.ok(!(r.stdout+r.stderr).includes('review2-private-prompt-canary'));
 return r;
}
run(['history','init']);
const scope=run(['history','scope','create']).result.tenantScopeId;
const otherScope=run(['history','scope','create']).result.tenantScopeId;
const observation=fixture('tests/fixtures/run-history/valid/observation.json');
observation.provenance=fixture('tests/fixtures/run-history/valid/greenfield.json').provenance;
observation.status={outcome:'pass',coverageState:'complete'};
observation.metrics={durationMs:{state:'known',value:10}};
observation.assertions={rows:[{assertionId:'review2-gate',subjectSemanticId:null,kind:'gate',outcome:'pass',evidenceRef:null}]};
const trials=[];
for(let i=0;i<10;i++){
 observation.runId=(60001+i).toString(16).padStart(32,'0');
 run(['history','record','--input','-','--scope',scope],{input:JSON.stringify(observation)});
 trials.push({unit:`paired-${i%5+1}`,arm:i<5?'baseline':'lekalo-assisted',runId:observation.runId,requiredAssertions:['review2-gate']});
}
put('selection.json',{schema_version:'lekalo/evaluation-export-input/v0.6.4',identity:'dev.lekalo.evaluation-export-input@0.6.4',protocol:'framework-lift-paired-trials/1',approved:true,trials});
const args=(d='publish',sc=scope)=>['metrics','export','--evaluation','selection.json','--scope',sc,'--destination',d];
function authorized(d){
 const a=run([...args(d),'--dry-run']).decisionTemplate;
 a.dataSensitivity=['public'];
 a.derivedArtifact.aggregationDecision.decisionRef=authorizingEvidence(a,'aggregation','a');
 a.derivedArtifact.declassificationDecision={policyRef:a.policyRef,version:'0.2.16',outcome:'approved',removedSensitivities:['internal'],decisionRef:authorizingEvidence(a,'declassification','c')};
 if(d!=='workspace')a.provenance.exportTransferConsentRef=authorizingEvidence(a,'exportTransferConsentRef','b');
 refreshEvidenceBindings(a);put(`authorization-${d}.json`,a);
 const preview=run([...args(d),'--dry-run','--authorization',`authorization-${d}.json`]);
 assert.equal(preview.status,'ready',JSON.stringify(preview.decision));
 return {preview,a};
}
const destinations=[];
for(const [d,op,boundary] of [['workspace','local-use','same-local-workspace'],['repository-store','repository-store','same-repository'],['transfer-tenant','transfer','same-tenant'],['transfer-external','transfer','cross-repository'],['transfer-cross-tenant','transfer','cross-tenant'],['publish','publish','public']]){
 const {preview,a}=authorized(d);
 assert.equal(a.operation.id,op);assert.equal(a.destination.trustBoundary,boundary);
 if(d==='repository-store'){
  assert.equal(a.source.repositoryRef,a.destination.repositoryRef);
  const contradicted=structuredClone(a);contradicted.destination.repositoryRef='repo-sha256:'+'f'.repeat(64);refreshEvidenceBindings(contradicted);put('contradiction.json',contradicted);
  const refusal=run(['privacy','evaluate','--decision','contradiction.json'],{code:3,projectArg:false});
  assert.ok(refusal.stdout.includes('repository.same-origin-contradiction'));
 }else if(op==='transfer')assert.notEqual(a.source.repositoryRef,a.destination.repositoryRef);
 destinations.push({destination:d,status:preview.status,operation:op,boundary});
}
run([...args('not-approved'),'--dry-run'],{code:1});
run([...args('transfer-external'),'--dry-run','--authorization','authorization-repository-store.json'],{code:3});
run([...args('publish',otherScope),'--dry-run'],{code:3});
const baseline=authorized('publish').preview;
const db=new DatabaseSync(join(project,'.lekalo/history/store.sqlite'));
const victim=trials[0].runId;
const row=db.prepare('SELECT record_bytes,record_digest,assertion_set_id FROM runs WHERE run_id=?').get(victim);
const assertionRow=db.prepare('SELECT bytes,digest FROM assertion_sets WHERE set_id=?').get(row.assertion_set_id);
const record=JSON.parse(Buffer.from(row.record_bytes).toString('utf8'));
const assertion=JSON.parse(Buffer.from(assertionRow.bytes).toString('utf8'));
assert.ok(vr(record));assert.ok(va(assertion));
const ur=db.prepare('UPDATE runs SET record_bytes=?,record_digest=? WHERE run_id=?');
const ua=db.prepare('UPDATE assertion_sets SET bytes=?,digest=? WHERE set_id=?');
function restore(){ur.run(row.record_bytes,row.record_digest,victim);ua.run(assertionRow.bytes,assertionRow.digest,row.assertion_set_id);}
const count=()=>db.prepare('SELECT COUNT(*) AS count FROM dependents WHERE tenant_scope_id=?').get(scope).count;
function sourceRefuses(){
 const before=count();
 for(const extra of [['--dry-run'],['--dry-run','--authorization','authorization-publish.json'],['--confirm',baseline.previewDigest,'--authorization','authorization-publish.json']]){
  const r=run([...args(),...extra],{code:3});assert.ok((r.stdout+r.stderr).includes('metrics-export.source-invalidated'));
 }
 assert.equal(count(),before);
}
const schemaProbes=[];
try{
 for(const [name,mutate] of [
  ['record root prompt',r=>{r.prompt='review2-private-prompt-canary';}],
  ['record recordedAt wrong type',r=>{r.recordedAt=42;}],
  ['record measurementSources wrong type',r=>{r.measurementSources={};}],
  ['record operation nested prompt',r=>{r.operation.prompt='review2-private-prompt-canary';}],
 ]){
  restore();const r=structuredClone(record);mutate(r);assert.equal(vr(r),false,name);
  const b=bytes(r);ur.run(b,sha(b),victim);assert.equal(sha(b),db.prepare('SELECT record_digest FROM runs WHERE run_id=?').get(victim).record_digest);
  sourceRefuses();schemaProbes.push(name);
 }
 for(const [name,mutate] of [
  ['assertion root prompt',a=>{a.prompt='review2-private-prompt-canary';}],
  ['assertion rows wrong type',a=>{a.rows={};}],
  ['assertion dataSensitivity wrong type',a=>{a.dataSensitivity=42;}],
  ['assertion scope nested prompt',a=>{a.scope.prompt='review2-private-prompt-canary';}],
 ]){
  restore();const a=structuredClone(assertion);mutate(a);assert.equal(va(a),false,name);
  const ab=bytes(a);ua.run(ab,sha(ab),row.assertion_set_id);
  const parent=structuredClone(record);parent.assertionsRef.digest=sha(ab);assert.ok(vr(parent));
  const rb=bytes(parent);ur.run(rb,sha(rb),victim);sourceRefuses();schemaProbes.push(name);
 }
 restore();ur.run(Buffer.from('{}\n'),row.record_digest,victim);sourceRefuses();
 restore();ua.run(Buffer.from('{}\n'),assertionRow.digest,row.assertion_set_id);sourceRefuses();
 restore();const mismatched=structuredClone(assertion);mismatched.runId='0'.repeat(32);assert.ok(va(mismatched));
 const ab=bytes(mismatched);ua.run(ab,sha(ab),row.assertion_set_id);
 const parent=structuredClone(record);parent.assertionsRef.digest=sha(ab);assert.ok(vr(parent));const rb=bytes(parent);ur.run(rb,sha(rb),victim);sourceRefuses();
 restore();const countMismatch=structuredClone(record);countMismatch.assertionsRef.count++;assert.ok(vr(countMismatch));const cb=bytes(countMismatch);ur.run(cb,sha(cb),victim);sourceRefuses();
}finally{restore();}
const {preview}=authorized('repository-store');
const release=run([...args('repository-store'),'--confirm',preview.previewDigest,'--authorization','authorization-repository-store.json']);assert.equal(release.written,true);
const statusArgs=['metrics','status',release.exportId,'--scope',scope,'--authorization','authorization-repository-store.json'];
assert.equal(run(statusArgs).status,'valid-at-export');
const cid=release.exportId.slice(3);
const custodyPath=join(project,'.lekalo/privacy/decisions/aggregate',cid.slice(0,32),cid.slice(32)+'.json');
const custodyBytes=fs.readFileSync(custodyPath);const custody=JSON.parse(custodyBytes);
const originalDependent=db.prepare('SELECT record_digest,assertion_digest FROM dependent_sources WHERE dependent_id=? AND source_id=?').get(release.exportId,victim);
const ud=db.prepare('UPDATE dependent_sources SET record_digest=?,assertion_digest=? WHERE dependent_id=? AND source_id=?');
function bindStatus(r,a=assertion){
 const ab=bytes(a),rd=structuredClone(r);rd.assertionsRef.digest=sha(ab);const rb=bytes(rd);
 ua.run(ab,sha(ab),row.assertion_set_id);ur.run(rb,sha(rb),victim);ud.run(sha(rb),sha(ab),release.exportId,victim);
 const c=structuredClone(custody);const s=c.projection.sources.find(s=>s.runId===victim);s.recordDigest=sha(rb);s.assertionDigest=sha(ab);fs.writeFileSync(custodyPath,bytes(c));
 assert.equal(db.prepare('SELECT record_digest FROM runs WHERE run_id=?').get(victim).record_digest,s.recordDigest);
 assert.equal(db.prepare('SELECT digest FROM assertion_sets WHERE set_id=?').get(row.assertion_set_id).digest,s.assertionDigest);
 return rd;
}
const statusProbes=[];
try{
 const validChanged=structuredClone(record);validChanged.recordedAt='2026-01-01T00:00:00Z';assert.ok(vr(bindStatus(validChanged)));assert.equal(run(statusArgs).status,'valid-at-export','matching-custody status control is valid');
 for(const [name,mutate] of [['record root prompt',r=>{r.prompt='review2-private-prompt-canary';}],['record timestamp wrong type',r=>{r.recordedAt=42;}]]){
  const r=structuredClone(record);mutate(r);assert.equal(vr(bindStatus(r)),false);assert.equal(run(statusArgs).status,'invalidated');statusProbes.push(name);
 }
 for(const [name,mutate] of [['assertion root prompt',a=>{a.prompt='review2-private-prompt-canary';}],['assertion rows wrong type',a=>{a.rows={};}]]){
  const a=structuredClone(assertion);mutate(a);assert.equal(va(a),false);assert.ok(vr(bindStatus(record,a)));assert.equal(run(statusArgs).status,'invalidated');statusProbes.push(name);
 }
}finally{
 restore();ud.run(originalDependent.record_digest,originalDependent.assertion_digest,release.exportId,victim);fs.writeFileSync(custodyPath,custodyBytes);
}
assert.equal(run(statusArgs).status,'valid-at-export','restored package stays valid');
assert.ok(!release.payload.includes('review2-private-prompt-canary'));
assert.ok(!release.payload.includes(victim));assert.ok(!release.payload.includes(scope));
db.close();
console.log(JSON.stringify({ok:true,project,destinations,contradictionRefused:true,wrongDestinationRefused:true,schemaProbes,statusProbes,matchingCustodyControl:'valid-at-export',rawDigestScopeRunLinkCountControls:true,repositoryStoreWritten:true,restoredStatus:'valid-at-export'}));

```
