// Every new #102 family is a real-binary release gate, not schema-only evidence.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {readFileSync,writeFileSync,mkdtempSync,realpathSync,mkdirSync,readdirSync,rmSync,existsSync,symlinkSync,linkSync,unlinkSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {AUTHORIZING_EVIDENCE_CONTRACT_REF, validateDecisionInput, loadTrustedContext, authorizationSubjectDigest} from './check-privacy.mjs';
import {refreshEvidenceBindings, AUTHORIZATION_SUBJECT_PROFILE} from './privacy-test-helpers.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..'),require=createRequire(import.meta.url);
assert.equal(require('ajv/package.json').version,'8.17.1');
const Ajv2020=require('ajv/dist/2020').default;
// #119's frozen scalar value-state explicitly uses JSON Schema union types.
const ajv=new Ajv2020({strict:true,allErrors:true,allowUnionTypes:true});
const families=['evaluation-export-input','metrics-aggregation-definition','public-metrics','metrics-export-manifest','metrics-export-preview'];
const args=process.argv.slice(2),authoring=args.includes('--write-goldens');
const selected=args.includes('--family')?args[args.indexOf('--family')+1]:null;
assert.ok(!selected||families.includes(selected),'unknown family');
const read=p=>JSON.parse(readFileSync(join(root,p),'utf8'));
const sha=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const canon=v=>Array.isArray(v)?`[${v.map(canon).join(',')}]`:v&&typeof v==='object'?`{${Object.keys(v).sort().map(k=>`${JSON.stringify(k)}:${canon(v[k])}`).join(',')}}`:JSON.stringify(v);
const canonical=v=>canon(v)+'\n';
const validators=new Map(families.map(f=>[f,ajv.compile(read(`contracts/${f}.schema.v0.6.4.json`))]));
const recordValidator=ajv.compile(read('contracts/run-record.schema.v0.4.0.json'));
const assertionValidator=ajv.compile(read('contracts/run-assertions.schema.v0.4.0.json'));
function valid(f,v){const validator=validators.get(f);assert.ok(validator(v),`${f}: ${JSON.stringify(validator.errors)}`);}
function golden(f,value,suffix=''){
 const p=join(root,`tests/fixtures/metrics-export/goldens/${f}${suffix}.json`);
 if(authoring)writeFileSync(p,canonical(value));
 assert.ok(existsSync(p),`missing golden ${f}${suffix}`);
 const bytes=readFileSync(p,'utf8'),expected=JSON.parse(bytes);
 valid(f,expected);assert.equal(bytes,canonical(expected),'golden canonical bytes');
 return expected;
}
const baseline=read('tests/fixtures/metrics-export/registry-baseline.json');
const registry=read('contracts/diagnostic-registry.v0.6.4.json');
const old=registry.entries.filter(e=>!e.id.startsWith('metrics-export.'));
assert.equal(old.length,baseline.entries);assert.equal(sha(JSON.stringify(old)),baseline.digest,'500 predecessor entries remain exact');
assert.equal(registry.entries.length,508);
const definition=read('contracts/metrics-aggregation-definition.v0.6.4.json');
golden('metrics-aggregation-definition',definition);
const selection=read('tests/fixtures/metrics-export/goldens/evaluation-export-input.json');
golden('evaluation-export-input',selection);
for(const f of families){
 const g=existsSync(join(root,`tests/fixtures/metrics-export/goldens/${f}.json`))?read(`tests/fixtures/metrics-export/goldens/${f}.json`):null;
 if(g){const mutant=structuredClone(g);mutant.prompt='forbidden';assert.equal(validators.get(f)(mutant),false,'closed root');}
}
const bin=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
assert.ok(existsSync(bin),'cargo build --locked -p lekalo-cli must run before this gate');
const env={PATH:process.env.PATH??'',...(process.platform==='win32'?{SystemRoot:process.env.SystemRoot??''}:{})};
const project=realpathSync.native(mkdtempSync(join(tmpdir(),'lekalo-metrics-export-')));
const write=(p,v)=>writeFileSync(join(project,p),typeof v==='string'?v:JSON.stringify(v));
function invoke(argv,input,code=0){
 const r=spawnSync(bin,['--json',...argv,'--project','.'],{cwd:project,env,encoding:'utf8',input,timeout:20000,maxBuffer:2097152});
 assert.equal(r.status,code,`${argv.join(' ')}: ${r.stdout} ${r.stderr}`);
 if(code!==0){assert.ok(!r.stdout.includes('consumer-private-canary')&&!r.stderr.includes('consumer-private-canary'),'refusal never echoes unsafe input');return r;}
 assert.equal(r.stderr,'');return JSON.parse(r.stdout);
}
const metrics=(path,scope,extra=[],code=0)=>invoke(['metrics','export','--evaluation',path,'--scope',scope,...extra],undefined,code);
const snapshot=()=>readFileSync(join(project,'.lekalo/history/store.sqlite'));
try {
 invoke(['history','init']);
 const scope=invoke(['history','scope','create']).result.tenantScopeId;
const observation=read('tests/fixtures/run-history/valid/observation.json');
 observation.provenance=read('tests/fixtures/run-history/valid/greenfield.json').provenance;
 const trials=[];
 for(let i=0;i<10;i++){
  const o=structuredClone(observation),arm=i<5?'baseline':'lekalo-assisted';
  o.runId=(i+1).toString(16).padStart(32,'0');o.status={outcome:i<5?'pass':'fail',coverageState:'complete'};
  o.assertions={rows:[{assertionId:'acceptance',subjectSemanticId:null,kind:'gate',outcome:i<5?'pass':'fail',evidenceRef:null}]};
  const known=value=>({state:'known',value});
  o.metrics={durationMs:known(i<5?10:20),filesRead:known(0),filesChanged:known(0),toolCalls:known(2),retryCount:known(0),replanCount:known(0),tokens:{input:known(100),output:known(20),total:known(120)},cost:{amount:known('1.25'),currency:known('USD'),basis:known('reported')}};
  invoke(['history','record','--input','-','--scope',scope],JSON.stringify(o));
  trials.push({unit:`trial-${i%5+1}`,arm,runId:o.runId,requiredAssertions:['acceptance']});
 }
 const evaluation={...selection,trials};valid('evaluation-export-input',evaluation);write('evaluation.json',evaluation);
 const before=snapshot();
 const pending=metrics('evaluation.json',scope,['--dry-run']);
 valid('metrics-export-preview',pending);assert.equal(pending.status,'blocked');assert.equal(pending.written,false);
 assert.deepEqual(snapshot(),before,'dry-run never mutates history');
 assert.equal(existsSync(join(project,'.lekalo/privacy')),false,'cancel-before-publish writes nothing');
 const payload=JSON.parse(pending.payload);valid('public-metrics',payload);valid('metrics-export-manifest',pending.manifest);
 assert.equal(payload.definitionRef.digest,sha(readFileSync(join(root,'contracts/metrics-aggregation-definition.v0.6.4.json'))),'embedded recipe exact bytes');
 assert.equal(payload.cohorts.length,2);
 assert.deepEqual(payload.cohorts.map(c=>c.metrics.durationMs.sum),[{state:'known',value:'50'},{state:'known',value:'100'}],'independent sums');
 assert.equal(payload.cohorts[0].metrics.filesRead.sum.value,'0');
 assert.equal(payload.cohorts[0].metrics.cachedTokens.sum.state,'unknown');
 assert.equal(payload.cohorts[0].verifiedSuccess.value,5);assert.equal(payload.cohorts[1].verifiedSuccess.value,0);
 assert.equal(Number(payload.comparisons[0].successLift.value.numerator)/Number(payload.comparisons[0].successLift.value.denominator),-1,'negative result preserved');
 assert.equal(payload.cohorts[0].cost.value.amount,'6.250000');assert.equal(payload.cohorts[1].cost.value.perSuccess.state,'unknown','zero success never zero cost');
 assert.deepEqual(payload,golden('public-metrics',payload));
 assert.deepEqual(pending.manifest,golden('metrics-export-manifest',pending.manifest));
 golden('metrics-export-preview',pending);
 assert.equal(sha(pending.payload),pending.payloadDigest);
 assert.equal(sha(canonical(pending.manifest)),pending.manifestDigest);
 for(const word of ['runId','tenantScopeId','subjectSemanticId','sourceRef','provenance','planner.focus_task','sourceMap','prompt','repositoryUrl'])assert.ok(!pending.payload.includes(word)&&!canonical(pending.manifest).includes(word),`no ${word} in public bytes`);
 const reversed={...evaluation,trials:[...trials].reverse()};write('reversed.json',reversed);
 assert.equal(metrics('reversed.json',scope,['--dry-run']).payload,pending.payload,'enumeration does not change public bytes');
 assert.equal(metrics('reversed.json',scope,['--dry-run']).previewDigest,pending.previewDigest,'normalized selection gives exact same reviewed subject');
 const evidence=(kind,purpose,outcome,char)=>({...AUTHORIZING_EVIDENCE_CONTRACT_REF,evidenceKind:kind,purpose,outcome,evidenceId:`evidence-sha256:${char.repeat(64)}`,verificationState:'verified',freshnessState:'current',binding:{}});
 const auth=structuredClone(pending.decisionTemplate);
 auth.derivedArtifact.aggregationDecision.decisionRef=evidence('aggregation-decision','authorize-public-aggregation','approved','a');
 auth.provenance.exportTransferConsentRef=evidence('export-transfer-consent','authorize-export-transfer-or-storage','granted','b');
 const aggregationOnly=structuredClone(auth);refreshEvidenceBindings(aggregationOnly);write('aggregation-only.json',aggregationOnly);
 assert.equal(metrics('evaluation.json',scope,['--dry-run','--authorization','aggregation-only.json']).status,'blocked','aggregation approval cannot remove the internal classification floor');
 function requestInternalDeclassification(v){
  v.dataSensitivity=['public'];
  v.derivedArtifact.declassificationDecision={policyRef:v.policyRef,version:'0.2.16',outcome:'approved',removedSensitivities:['internal'],decisionRef:evidence('declassification-decision','authorize-declassification','approved','c')};
  return v;
 }
 requestInternalDeclassification(auth);
 refreshEvidenceBindings(auth);write('authorization.json',auth);
 const ctx=await loadTrustedContext();
 assert.equal(validateDecisionInput(auth,ctx),null,'synthetic authorization uses the frozen strict input');
 const privacyProbe=spawnSync(bin,['privacy','evaluate','--decision','authorization.json'],{cwd:project,env,encoding:'utf8',timeout:20000});
 assert.equal(privacyProbe.status,0,`frozen privacy probe: ${privacyProbe.stdout} ${privacyProbe.stderr}`);
 const badDeclassification=structuredClone(auth);
 badDeclassification.derivedArtifact.declassificationDecision={policyRef:auth.policyRef,version:'0.2.16',outcome:'approved',removedSensitivities:[],decisionRef:evidence('declassification-decision','authorize-declassification','approved','c')};
 refreshEvidenceBindings(badDeclassification);write('declassification.json',badDeclassification);
 assert.equal(validateDecisionInput(badDeclassification,ctx),'input.derived-artifact');
 const declassificationProbe=spawnSync(bin,['privacy','evaluate','--decision','declassification.json'],{cwd:project,env,encoding:'utf8',timeout:20000});
 assert.equal(declassificationProbe.status,1,'empty declassification remains malformed');
 const ready=metrics('evaluation.json',scope,['--dry-run','--authorization','authorization.json']);
 assert.equal(authorizationSubjectDigest(auth,AUTHORIZATION_SUBJECT_PROFILE),ready.authorizationSubject,'issuer binding matches requested declassification subject');
 assert.equal(ready.status,'ready',JSON.stringify(ready.decision));
 // Each shipped destination must admit a coherent authorized candidate.
 // Workspace local-use does not use export-transfer consent; every other
 // destination requires its own exact subject-bound consent.
 for(const [destination,operation,boundary,audience] of [
  ['workspace','local-use','same-local-workspace','operator-only'],
  ['repository-store','repository-store','same-repository','repository-collaborators'],
  ['transfer-tenant','transfer','same-tenant','tenant-members'],
  ['transfer-external','transfer','cross-repository','named-external'],
  ['transfer-cross-tenant','transfer','cross-tenant','named-external'],
  ['publish','publish','public','public'],
 ]) {
  const candidate=metrics('evaluation.json',scope,['--dry-run','--destination',destination]);
  const authorization=structuredClone(candidate.decisionTemplate);
  authorization.derivedArtifact.aggregationDecision.decisionRef=evidence('aggregation-decision','authorize-public-aggregation','approved','a');
  if(destination!=='workspace')authorization.provenance.exportTransferConsentRef=evidence('export-transfer-consent','authorize-export-transfer-or-storage','granted','b');
  requestInternalDeclassification(authorization);refreshEvidenceBindings(authorization);
  write('destination-auth.json',authorization);
  assert.equal(validateDecisionInput(authorization,ctx),null,`${destination}: frozen authorization shape`);
  const allowed=metrics('evaluation.json',scope,['--dry-run','--destination',destination,'--authorization','destination-auth.json']);
  valid('metrics-export-preview',allowed);assert.equal(allowed.status,'ready',`${destination}: ${JSON.stringify(allowed.decision)}`);
  assert.equal(allowed.decisionTemplate.operation.id,operation);
  assert.equal(allowed.decisionTemplate.destination.trustBoundary,boundary);
  assert.equal(allowed.decisionTemplate.audience,audience);
  if(destination==='repository-store') {
   assert.equal(authorization.source.repositoryRef,authorization.destination.repositoryRef,'same-origin repository identity');
   const contradiction=structuredClone(authorization);
   contradiction.destination.repositoryRef='repo-sha256:'+'0'.repeat(64);refreshEvidenceBindings(contradiction);
   write('destination-contradiction.json',contradiction);
   const refused=spawnSync(bin,['privacy','evaluate','--decision','destination-contradiction.json'],{cwd:project,env,encoding:'utf8',timeout:20000});
   assert.equal(refused.status,3,`${refused.stdout} ${refused.stderr}`);
   assert.ok(refused.stdout.includes('repository.same-origin-contradiction'),'frozen evaluator still refuses contradictory repository identities');
  } else if(operation==='transfer') {
   assert.notEqual(authorization.source.repositoryRef,authorization.destination.repositoryRef,'transfer endpoints stay distinct');
  }
 }
 metrics('evaluation.json',scope,['--dry-run','--destination','not-approved'],1);
 metrics('evaluation.json',scope,['--dry-run','--destination','transfer-external','--authorization','authorization.json'],3);
 const expired=structuredClone(auth);expired.provenance.exportTransferConsentRef.freshnessState='expired';write('expired.json',expired);
 assert.equal(metrics('evaluation.json',scope,['--dry-run','--authorization','expired.json']).status,'blocked','expired evidence refuses');
 const expiredLowering=structuredClone(auth);expiredLowering.derivedArtifact.declassificationDecision.decisionRef.freshnessState='expired';write('expired-lowering.json',expiredLowering);
 assert.equal(metrics('evaluation.json',scope,['--dry-run','--authorization','expired-lowering.json']).status,'blocked','expired declassification cannot remove the source floor');
 const badBinding=structuredClone(auth);badBinding.derivedArtifact.aggregationDecision.decisionRef.binding.subjectDigest='subject-sha256:'+'0'.repeat(64);write('binding.json',badBinding);
 assert.equal(metrics('evaluation.json',scope,['--dry-run','--authorization','binding.json']).status,'blocked');
 const extra=structuredClone(evaluation);extra.prompt='consumer-private-canary';write('unsafe.json',extra);metrics('unsafe.json',scope,['--dry-run'],1);
 write('duplicate.json','{"approved":true,"approved":false}');metrics('duplicate.json',scope,['--dry-run'],1);
 const wrong=structuredClone(evaluation);wrong.schema_version='lekalo/evaluation-export-input/v9.9.9';write('wrong.json',wrong);metrics('wrong.json',scope,['--dry-run'],3);
 const unapproved={...evaluation,approved:false};write('unapproved.json',unapproved);metrics('unapproved.json',scope,['--dry-run'],3);
 metrics('evaluation.json',scope,['--confirm','sha256:'+'0'.repeat(64),'--authorization','authorization.json'],3);
 const small={...evaluation,trials:trials.filter((_,i)=>i%5<4)};write('small.json',small);
 const withheld=JSON.parse(metrics('small.json',scope,['--dry-run']).payload);
 assert.ok(withheld.cohorts.every(c=>c.sample.state==='withheld'&&c.metrics.durationMs.sum.state==='withheld'),'k-1 withholding');
 const published=metrics('evaluation.json',scope,['--confirm',ready.previewDigest,'--authorization','authorization.json']);
 assert.equal(published.payload,ready.payload);assert.deepEqual(published.manifest,ready.manifest);
 assert.equal(published.written,true);valid('metrics-export-preview',published);
 const overlap=metrics('evaluation.json',scope,['--confirm',ready.previewDigest,'--authorization','authorization.json'],3);
 assert.ok(overlap.stdout.includes('metrics-export.overlap-refused'),'lifetime release budget is enforced');
 const id=published.exportId;
 const local=JSON.parse(readFileSync(join(project,`.lekalo/privacy/decisions/aggregate/${id.slice(3,35)}/${id.slice(35)}.json`),'utf8'));valid('metrics-export-manifest',local);golden('metrics-export-manifest',local,'-local');
 const otherScope=invoke(['history','scope','create']).result.tenantScopeId;
 invoke(['metrics','status',id,'--scope',otherScope],undefined,3);
 assert.equal(invoke(['metrics','status',id,'--scope',scope]).status,'unverified','historical consent is not current authenticity');
 assert.equal(invoke(['metrics','status',id,'--scope',scope,'--authorization','authorization.json']).status,'valid-at-export');
 assert.equal(invoke(['metrics','status',id,'--scope',scope,'--authorization','expired.json']).status,'invalidated','a supplied current denial invalidates eligibility instead of claiming unknown approval');
 metrics('evaluation.json',scope,['--confirm',ready.previewDigest,'--authorization','authorization.json'],3);
 invoke(['history','delete',trials[0].runId,'--scope',scope,'--apply']);
 assert.equal(invoke(['metrics','status',id,'--scope',scope,'--authorization','authorization.json']).status,'invalidated','source deletion invalidates manifest');
 metrics('evaluation.json',scope,['--dry-run'],3);
 // Do not accept an immutable package whose bytes have been replaced.
 writeFileSync(join(project,`.lekalo/privacy/aggregates/${id.slice(3)}/payload.json`),'{}\n');
 invoke(['metrics','status',id,'--scope',scope],undefined,1);
 // Closed nested leaves refuse privacy/custody escape hatches.
 const mutation=structuredClone(payload);mutation.cohorts[0].metrics.durationMs.sum={state:'unknown',value:'0'};
 assert.equal(validators.get('public-metrics')(mutation),false);
 const tooSmall=structuredClone(payload);tooSmall.cohorts[0].sample.value=4;
 assert.equal(validators.get('public-metrics')(tooSmall),false,'imports enforce the known sample floor');
 const missingId=structuredClone(published);delete missingId.exportId;
 assert.equal(validators.get('metrics-export-preview')(missingId),false,'written receipts require an export id');
 const leak=structuredClone(payload);leak.cohorts[0].profileAlias='consumer-private-canary';assert.equal(validators.get('public-metrics')(leak),false);
 // The shared manifest lane covers the larger lifecycle corpus once in CI;
 // every selected family still executes the real preview/confirm/delete flow.
 if(!selected || selected==='metrics-export-manifest') {
 // Additional real-recorder populations, no injected measurements at the seam.
 let nextId=100;
 function population(name, count, mutate){
  const sc=invoke(['history','scope','create']).result.tenantScopeId,rows=[];
  for(let i=0;i<count*2;i++){
   const arm=i<count?'baseline':'lekalo-assisted',o=structuredClone(observation);
   o.runId=(++nextId).toString(16).padStart(32,'0');
   o.status={outcome:'pass',coverageState:'complete'};
   o.assertions={rows:[{assertionId:'acceptance',subjectSemanticId:null,kind:'gate',outcome:'pass',evidenceRef:null}]};
   o.metrics={durationMs:{state:'known',value:10},filesRead:{state:'known',value:0}};
   mutate(o,i%count,arm);invoke(['history','record','--input','-','--scope',sc],JSON.stringify(o));
   rows.push({unit:`trial-${i%count+1}`,arm,runId:o.runId,requiredAssertions:['acceptance']});
  }
  const input={...selection,trials:rows};write(`${name}.json`,input);
  const preview=metrics(`${name}.json`,sc,['--dry-run']);valid('metrics-export-preview',preview);
  const data=JSON.parse(preview.payload);valid('public-metrics',data);return {sc,input,preview,data,path:`${name}.json`};
 }
 const neutral=population('neutral',6,()=>{});
 assert.equal(neutral.data.cohorts[0].sample.value,6,'k+1 uses scheduled trials');
 assert.equal(neutral.data.comparisons[0].successLift.value.numerator,'0','neutral result preserved');
 const uncertain=population('uncertain',5,(o)=>{
  o.metrics.durationMs={state:'withheld'};o.metrics.filesRead={state:'unsupported'};
  o.provenance.harness.modelRevision={state:'unknown'};
 });
 assert.ok(uncertain.data.cohorts.every(c=>c.metrics.durationMs.sum.state==='withheld'&&c.metrics.filesRead.sum.state==='unsupported'&&c.metrics.toolCalls.sum.state==='unknown'));
 assert.equal(uncertain.data.comparisons[0].successLift.state,'unknown','absent provenance never produces a lift claim');
 const cells=population('cells',6,(o,i)=>{if(i===0){o.status.outcome='fail';o.assertions.rows[0].outcome='fail';}});
 assert.ok(cells.data.cohorts.every(c=>c.outcomes.state==='withheld'&&c.metrics.durationMs.sum.state==='withheld'&&c.verifiedSuccess.state==='withheld'),'small cells cannot be recovered through numeric complements');
 const outages=population('outages',5,(o,_,arm)=>{o.status.outcome=arm==='baseline'?'unsupported':'infrastructure';o.assertions.rows[0].outcome=o.status.outcome;});
 assert.equal(outages.data.cohorts[0].outcomes.value.unsupported,5);
 assert.equal(outages.data.cohorts[1].outcomes.value.infrastructure,5);
 assert.ok(outages.data.cohorts.every(c=>c.verifiedSuccess.state==='unknown'));
 const native=population('aliases',5,o=>{o.provenance.profile.id={state:'known',value:'private_native_symbol'};});
 assert.ok(!native.preview.payload.includes('private_native_symbol'),'private token-shaped identities are never copied');
 // Wrong scope, loose observation, recipe mutation and confined input refusal.
 metrics(neutral.path,uncertain.sc,['--dry-run'],3);
 write('loose.json',observation);metrics('loose.json',neutral.sc,['--dry-run'],1);
 write('recipe.json',{...definition,minimumSamples:1});metrics(neutral.path,neutral.sc,['--dry-run','--recipe','recipe.json'],1);
 metrics('../escape.json',neutral.sc,['--dry-run'],3);
 // A read-only dry-run can inspect a candidate but cannot activate through a link.
 const authorize=preview=>{
  const v=structuredClone(preview.decisionTemplate);
  v.derivedArtifact.aggregationDecision.decisionRef=evidence('aggregation-decision','authorize-public-aggregation','approved','a');
  v.provenance.exportTransferConsentRef=evidence('export-transfer-consent','authorize-export-transfer-or-storage','granted','b');
  requestInternalDeclassification(v);
  refreshEvidenceBindings(v);return v;
 };
 const current=metrics(neutral.path,neutral.sc,['--dry-run']);
 write('neutral-auth.json',authorize(current));
 const reviewed=metrics(neutral.path,neutral.sc,['--dry-run','--authorization','neutral-auth.json']);
 const escaped=join(project,'outside');mkdirSync(escaped);
 const aggregates=join(project,'.lekalo/privacy/aggregates');
 const digestDir=reviewed.previewDigest.slice(7);
 symlinkSync(escaped,join(aggregates,digestDir),process.platform==='win32'?'junction':'dir');
 metrics(neutral.path,neutral.sc,['--confirm',reviewed.previewDigest,'--authorization','neutral-auth.json'],3);
 assert.deepEqual(readdirSync(escaped),[],'linked output never receives public bytes');
 rmSync(join(aggregates,digestDir),{force:true,recursive:true});
 // Different history mutation invalidates a previously reviewed generation.
 const staleCandidate=metrics(uncertain.path,uncertain.sc,['--dry-run']);
 write('stale-auth.json',authorize(staleCandidate));
 const staleReady=metrics(uncertain.path,uncertain.sc,['--dry-run','--authorization','stale-auth.json']);
 const staleObservation=structuredClone(observation);staleObservation.runId=(++nextId).toString(16).padStart(32,'0');
 invoke(['history','record','--input','-','--scope',uncertain.sc],JSON.stringify(staleObservation));
 metrics(uncertain.path,uncertain.sc,['--confirm',staleReady.previewDigest,'--authorization','stale-auth.json'],3);
 // SQLite custody attacks do not become aggregate input, even if an attacker
 // also updates the digest column. All values here are synthetic.
 const {DatabaseSync}=await import('node:sqlite');
 const database=join(project,'.lekalo/history/store.sqlite');
 const victim=native.input.trials[0].runId;
 const sql=new DatabaseSync(database);
 const stored=sql.prepare('SELECT record_bytes,record_digest,assertion_set_id FROM runs WHERE run_id=?').get(victim);
 const record=JSON.parse(Buffer.from(stored.record_bytes).toString('utf8'));assert.ok(recordValidator(record));
 const update=sql.prepare('UPDATE runs SET record_bytes=?,record_digest=? WHERE run_id=?');
 const restored=()=>update.run(stored.record_bytes,stored.record_digest,victim);
 write('schema-auth.json',authorize(metrics(native.path,native.sc,['--dry-run'])));
 const schemaReady=metrics(native.path,native.sc,['--dry-run','--authorization','schema-auth.json']);
 assert.equal(schemaReady.status,'ready');
 const dependentCount=()=>sql.prepare('SELECT COUNT(*) AS count FROM dependents WHERE tenant_scope_id=?').get(native.sc).count;
 const beforeDependents=dependentCount();
 const sourceRefusal=()=>{
  for(const extra of [['--dry-run'],['--confirm',schemaReady.previewDigest,'--authorization','schema-auth.json']]){
   const refused=metrics(native.path,native.sc,extra,3);
   assert.ok((refused.stdout+refused.stderr).includes('metrics-export.source-invalidated'),'schema-invalid source refuses before preview or activation');
   assert.ok(!(refused.stdout+refused.stderr).includes('private-prompt-canary'),'source details never enter refusals');
  }
  assert.equal(dependentCount(),beforeDependents,'source refusal registers no aggregate');
 };
 try {
  update.run(Buffer.from('{}\n'),stored.record_digest,victim);metrics(native.path,native.sc,['--dry-run'],3);
  restored();const wrongPolicy=structuredClone(record);wrongPolicy.privacy.policyRef.version='9.9.9';const bytes=Buffer.from(canonical(wrongPolicy));update.run(bytes,sha(bytes),victim);
  metrics(native.path,native.sc,['--dry-run'],3);
  for(const mutate of [
   r=>{r.scope.repositoryRole='private_native_symbol';},
   r=>{r.pilot.mode='private_native_symbol';},
   r=>{r.privacy.classificationContractRef.version='9.9.9';},
   r=>{r.prompt='consumer-private-canary';},
   r=>{r.metrics.durationMs={state:'unknown',value:0};},
   r=>{r.recordedAt='invalid-not-a-timestamp';},
   r=>{r.timestamp='invalid-not-a-timestamp';},
   r=>{r.operation.prompt='private-prompt-canary';},
   r=>{r.operation.affectedSemanticIds=['bad/id'];},
   r=>{r.measurementSources=[{prompt:'private-prompt-canary'}];},
   r=>{r.testGateSummaries=[{prompt:'private-prompt-canary'}];},
   r=>{r.diagnostics=[{prompt:'private-prompt-canary'}];},
   r=>{r.privacy.provenance.prompt='private-prompt-canary';},
   r=>{r.repeat={prompt:'private-prompt-canary'};},
  ]){
   const changed=JSON.parse(Buffer.from(stored.record_bytes).toString('utf8'));mutate(changed);
   assert.equal(recordValidator(changed),false,'record mutation must violate the unchanged frozen schema');
   const mutated=Buffer.from(canonical(changed));update.run(mutated,sha(mutated),victim);
   sourceRefusal();
  }
  restored();
  const assertionStored=sql.prepare('SELECT bytes,digest FROM assertion_sets WHERE set_id=?').get(stored.assertion_set_id);
  const assertion=JSON.parse(Buffer.from(assertionStored.bytes).toString('utf8'));assert.ok(assertionValidator(assertion));
  const assertionUpdate=sql.prepare('UPDATE assertion_sets SET bytes=?,digest=? WHERE set_id=?');
  try {
   for(const mutate of [
    a=>{a.prompt='private-prompt-canary';},
    a=>{a.scope.prompt='private-prompt-canary';},
    a=>{delete a.exportDisposition;},
    a=>{a.rows[0].evidenceRef='invalid-evidence';},
    a=>{a.rows[0].subjectSemanticId='bad/id';},
    a=>{a.rows[0].assertionId='a'.repeat(129);},
   ]){
    const changed=structuredClone(assertion);mutate(changed);
    assert.equal(assertionValidator(changed),false,'assertion mutation must violate the unchanged frozen schema');
    const bytes=Buffer.from(canonical(changed));assertionUpdate.run(bytes,sha(bytes),stored.assertion_set_id);
    const rebound=structuredClone(record);rebound.assertionsRef.digest=sha(bytes);
    assert.ok(recordValidator(rebound),'rebound parent stays schema-valid');
    const parentBytes=Buffer.from(canonical(rebound));update.run(parentBytes,sha(parentBytes),victim);
    sourceRefusal();
   }
  } finally {assertionUpdate.run(assertionStored.bytes,assertionStored.digest,stored.assertion_set_id);restored();}
  assert.equal(metrics(native.path,native.sc,['--dry-run','--authorization','schema-auth.json']).status,'ready','restored valid sources remain usable');
 } finally {restored();sql.close();}
 // Fresh lifecycles verify the filesystem bridge for clear, prune/retention,
 // and recovery as well as the explicit delete tested above.
 for(const mode of ['clear','prune','retention']){
  const p=population(`lifecycle-${mode}`,5,()=>{});
  write('lifecycle-auth.json',authorize(p.preview));
  const plan=metrics(p.path,p.sc,['--dry-run','--authorization','lifecycle-auth.json']);
  const release=metrics(p.path,p.sc,['--confirm',plan.previewDigest,'--authorization','lifecycle-auth.json']);
  if(mode==='clear')invoke(['history','clear','--scope',p.sc,'--apply']);
  else {
   const db=new DatabaseSync(database);
   db.prepare("UPDATE runs SET recorded_at='2000-01-01T00:00:00Z' WHERE tenant_scope_id=?").run(p.sc);db.close();
   if(mode==='prune')invoke(['history','prune','--scope',p.sc,'--apply']);
   else {
    const o=structuredClone(observation);o.runId=(++nextId).toString(16).padStart(32,'0');
    invoke(['history','record','--input','-','--scope',p.sc],JSON.stringify(o));
   }
  }
  const status=invoke(['metrics','status',release.exportId,'--scope',p.sc,'--authorization','lifecycle-auth.json']);
  valid('metrics-export-manifest',status);assert.equal(status.status,'invalidated',mode);
  invoke(['history','recover']);
  assert.equal(invoke(['metrics','status',release.exportId,'--scope',p.sc]).status,'invalidated','recovery cannot revive release');
 }
 }
 console.log(JSON.stringify({ok:true,family:selected??'all',families,live:true,registryBaseline:500,registryEntries:508}));
}finally{rmSync(project,{recursive:true,force:true});}
