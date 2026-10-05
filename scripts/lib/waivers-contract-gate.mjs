// Issue #88: schemas AND fresh binary disposition/expiry/review behavior.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdirSync,existsSync,linkSync,unlinkSync} from 'node:fs';
import {join,dirname} from 'node:path';
import {spawnSync} from 'node:child_process';
import {root,environment,allEvidence,canonical,hash,digest,known,unknown} from './ai-lint-contract-gate.mjs';
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH||dirname(import.meta.url))+'/');
assert.equal(require('ajv/package.json').version,'8.17.1');
const Ajv=require('ajv/dist/2020').default,ajv=new Ajv({strict:true,allErrors:true});
const validators=new Map(['ai-lint-waivers','waiver-input','waiver-audit'].map(f=>[f,ajv.compile(JSON.parse(readFileSync(join(root,`contracts/${f}.schema.v0.6.5.json`))))]));
const binary=join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const now='2026-10-04T12:00:00Z';
function check(f,v,expected=true){const validate=validators.get(f);assert.equal(validate(v),expected,`${f}: ${JSON.stringify(validate.errors)}`);}
function cli(env,args,code=0){if(args[0]==='waivers'&&['audit','add'].includes(args[1])&&!args.includes('--as-of'))args=[...args,'--as-of',now];const r=spawnSync(binary,['--json','--no-cache',...args],{cwd:env.project,encoding:'utf8',timeout:60000,maxBuffer:16*1024*1024});assert.ifError(r.error);const text=r.stdout.trim()||r.stderr.trim();assert.equal(r.status,code,text.slice(0,1400));const v=JSON.parse(text);return v.payload?{...v,...v.payload,status:v.status}:v;}
function file(env,name,v){env.file(name,v);return name;}
function approval(entry){const value=structuredClone(entry);delete value.approvalRef;delete value.lifecycle;entry.approvalRef.subjectDigest=hash(value);return entry;}
function entry(facts,f,id='waiver-depth',scope={kind:'symbol',id:f.symbol.value}){return approval({id,findingId:f.id,factDigest:hash(f),selector:f.selector,scope,subject:f.subject,target:f.target,profileRef:facts.profileRef,conditionDigest:f.conditionDigest,owner:'fixture-owner',approver:'fixture-reviewer',approvalRef:{id:'decision/88/approval',subjectDigest:digest('pending')},reason:'Temporary synthetic capability risk with a tracked replacement.',createdAt:now,expiresAt:known('2026-11-01T12:00:00Z'),reviewAfter:unknown(),sourceRef:{kind:'issue',id:'ichinya/lekalo#88',digest:unknown()},acceptedRisk:'correctness',fingerprint:f.fingerprint,lifecycle:'active',supersedes:unknown()});}
function store(facts,entries){return {schemaVersion:'lekalo/ai-lint-waivers/v0.6.5',identity:'dev.lekalo.ai-lint-waivers@0.6.5',projectId:facts.projectId,registryRef:JSON.parse(readFileSync(join(root,'tests/fixtures/ai-lint-config/golden/config.json'))).registryRef,entries};}
function golden(f,name,v,update){check(f,v);const dir=f==='ai-lint-waivers'?'golden-v0.6.5':'golden';const path=join(root,'tests/fixtures',f,dir,name+'.json');if(update){mkdirSync(dirname(path),{recursive:true});writeFileSync(path,canonical(v)+'\n');}else assert.equal(readFileSync(path,'utf8'),canonical(v)+'\n',`fresh binary golden drift: ${path}`);}
function nativePinCases(env, evidence) {
 const config=structuredClone(env.policy);
 for(const profile of config.profiles)for(const rule of profile.rules)
  rule.enabled=profile.id!=='off'&&rule.id==='hidden.string-reference';
 file(env,'native-pin-config.json',config);
 const args=['ai-lint','--module','planner','--config','native-pin-config.json','--lint-profile','ci','--waiver-facts','--evidence','native-pin-evidence.json'];
 const common=['--facts','native-pin-facts.json','--profile-kind','ai-lint','--lint-config','native-pin-config.json','--profile','ci'];
 const produce=e=>{
  file(env,'native-pin-evidence.json',e);
  const raw=cli(env,[...args,'--check'],3);
  check('waiver-input',raw.waiverInput);
  assert.equal(raw.report.summary.raw,1);
  assert.equal(raw.waiverInput.facts[0].target,e.target);
  file(env,'native-pin-facts.json',raw.waiverInput);
  return raw;
 };
 const add=f=>['waivers','add',f.selector.id,...common,'--id','native-pin-control','--symbol',f.symbol.value,'--subject',f.subject,'--target',f.target,'--owner','fixture-owner','--approver','fixture-reviewer','--approval-ref','decision/88/native-pin','--reason','Synthetic native provenance admission control.','--source-issue','ichinya/lekalo#88','--expires','2026-11-01T12:00:00Z'];
 const raw=produce(evidence),facts=raw.waiverInput;
 file(env,'lekalo.waivers.json',store(facts,[]));
 const preview=cli(env,add(facts.facts[0]));
 check('ai-lint-waivers',preview.candidate);
 const applied=cli(env,[...add(facts.facts[0]),'--apply',preview.planId]);
 assert.equal(applied.applied,true);
 const known=cli(env,[...args,'--waivers',join(env.project,'lekalo.waivers.json'),'--as-of',now,'--check']);
 assert.equal(known.report.summary.waived,1);
 assert.equal(known.report.summary.active,0);
 for(const pin of ['capabilities','revision'])for(const state of ['unsupported','unknown','withheld']){
  const missing=structuredClone(evidence);missing.pins[pin]={state};
  const raw=produce(missing),facts=raw.waiverInput,f=facts.facts[0];
  assert.equal(f.fingerprint[pin].state,state);
  file(env,'lekalo.waivers.json',store(facts,[]));
  const before=readFileSync(join(env.project,'lekalo.waivers.json'));
  for(const extra of [[],['--apply',preview.planId]]){
   const rejected=cli(env,[...add(f),...extra],1);
   assert.ok(canonical(rejected).includes('waivers-ineligible-candidate'));
   assert.deepEqual(readFileSync(join(env.project,'lekalo.waivers.json')),before);
  }
  // Bind a structurally valid store to these very facts, not the old known-pin
  // facts. This isolates applicability from ordinary stale/fact-hash refusal.
  const matching=store(facts,[entry(facts,f,'missing-native-pin')]);
  check('ai-lint-waivers',matching);
  file(env,'lekalo.waivers.json',matching);
  const audit=cli(env,['waivers','audit',...common,'--check'],3).audit;
  check('waiver-audit',audit);
  assert.equal(audit.entries[0].status,'unverifiable');
  assert.equal(audit.entries[0].effective,false);
  assert.ok(audit.entries[0].reasonCodes.includes('fingerprint-unverifiable'));
  assert.equal(audit.summary.waived,0);
  assert.deepEqual(audit.findings[0].fact,f);
  assert.equal(audit.findings[0].effectiveGate,'denied');
  const live=cli(env,[...args,'--waivers',join(env.project,'lekalo.waivers.json'),'--as-of',now,'--check'],3);
  check('waiver-audit',live.waiverAudit);
  assert.deepEqual(live.report.findings,raw.report.findings);
  assert.equal(live.report.summary.waived,0);
  assert.equal(live.report.summary.active,1);
  assert.equal(live.waiverAudit.entries[0].status,'unverifiable');
 }
 // Producer admission prevents native evidence from claiming Model-only pins.
 const disguised=structuredClone(evidence);disguised.target='model';
 for(const row of disguised.coverage)row.target='model';
 file(env,'native-pin-evidence.json',disguised);
 cli(env,args,1);
 return 7;
}
function producerDomainCases(env, evidence, lintArgs, modelFacts) {
 const config=structuredClone(env.policy);
 for(const profile of config.profiles)for(const rule of profile.rules)
  rule.enabled=profile.id!=='off'&&rule.id==='hidden.string-reference';
 file(env,'domain-config.json',config);
 const nativeArgs=['ai-lint','--module','planner','--config','domain-config.json','--lint-profile','ci','--waiver-facts','--evidence','domain-evidence.json'];
 const common=['--facts','domain-facts.json','--profile-kind','ai-lint','--lint-config','domain-config.json','--profile','ci'];
 const add=(f,id,args=common)=>['waivers','add',f.selector.id,...args,'--id',id,'--symbol',f.symbol.value,'--subject',f.subject,'--target',f.target,'--owner','fixture-owner','--approver','fixture-reviewer','--approval-ref','decision/88/domain','--reason','Synthetic reserved producer-domain admission control.','--source-issue','ichinya/lekalo#88','--expires','2026-11-01T12:00:00Z'];
 // A genuine emitted Model occurrence must retain normal preview/apply/audit.
 const model=modelFacts.facts.find(f=>f.target==='model'&&f.symbol.state==='known');assert.ok(model);
 const modelCommon=['--facts','domain-facts.json','--profile-kind','ai-lint','--lint-config','lint-config.json','--profile','ci'];
 file(env,'domain-facts.json',modelFacts);file(env,'lekalo.waivers.json',store(modelFacts,[]));
 const positive=cli(env,add(model,'domain-model-positive',modelCommon));check('ai-lint-waivers',positive.candidate);
 assert.equal(cli(env,[...add(model,'domain-model-positive',modelCommon),'--apply',positive.planId]).applied,true);
 const positiveAudit=cli(env,['waivers','audit',...modelCommon,'--check']).audit;check('waiver-audit',positiveAudit);assert.equal(positiveAudit.summary.waived,1);
 let controls=1;
 for(const name of ['capabilities','adapter','both','known','native-id-rewrite','model-binding-native-selector']){
  const e=structuredClone(evidence);if(name==='capabilities'||name==='both')e.pins.capabilities={state:'unsupported'};
  file(env,'domain-evidence.json',e);const raw=cli(env,[...nativeArgs,'--check'],3),facts=structuredClone(raw.waiverInput),f=facts.facts[0];assert.equal(facts.facts.length,1);
  f.target='model';
  if(name==='adapter'||name==='both')f.fingerprint.adapter={state:'unsupported'};
  if(name==='native-id-rewrite')f.id=hash([f.selector.id,f.subject,'model',f.symbol]);
  if(name==='model-binding-native-selector'){
   f.id=hash([model.selector.id,f.subject,'model',f.symbol]);
   f.fingerprint.revision=known(hash([f.fingerprint.model.value,f.fingerprint.ir.value]));
   f.fingerprint.adapter={state:'unsupported'};f.fingerprint.capabilities={state:'unsupported'};f.path=unknown();
  }
  check('waiver-input',facts);file(env,'domain-facts.json',facts);file(env,'lekalo.waivers.json',store(facts,[]));
  const before=readFileSync(join(env.project,'lekalo.waivers.json'));
  for(const extra of [[],['--apply',positive.planId]]){
   const rejected=cli(env,[...add(f,'domain-'+name),...extra],1);assert.ok(canonical(rejected).includes('waivers-ineligible-candidate'));
   assert.deepEqual(readFileSync(join(env.project,'lekalo.waivers.json')),before);
  }
  // Pre-existing decisions can be correctly approval/fact bound to an invalid
  // producer claim. Their structural validity cannot authorize acceptance.
  const matching=store(facts,[entry(facts,f,'stored-domain-'+name)]);check('ai-lint-waivers',matching);file(env,'lekalo.waivers.json',matching);
  const audit=cli(env,['waivers','audit',...common,'--check'],3).audit;check('waiver-audit',audit);
  assert.equal(audit.entries[0].status,'unverifiable');assert.equal(audit.entries[0].effective,false);
  assert.ok(audit.entries[0].reasonCodes.includes('fingerprint-unverifiable'));assert.equal(audit.summary.waived,0);
  assert.equal(audit.findings[0].effectiveGate,'denied');assert.deepEqual(audit.findings[0].fact,f);controls++;
 }
 // A Model fact in the same report cannot lend applicability to a relabelled
 // native occurrence, irrespective of input ordering.
 const mixedEvidence=structuredClone(evidence);mixedEvidence.pins.capabilities={state:'unsupported'};
 const mixed=cli(env,[...lintArgs,'--evidence',file(env,'domain-mixed-evidence.json',mixedEvidence)]).waiverInput;
 const native=mixed.facts.find(f=>f.selector.id==='hidden.string-reference'&&f.target!=='model');assert.ok(native);
 for(const reverse of [false,true]){
  const facts=structuredClone(mixed);facts.facts=[facts.facts.find(f=>f.target==='model'),{...native,target:'model'}];if(reverse)facts.facts.reverse();
  check('waiver-input',facts);file(env,'domain-facts.json',facts);
  const matching=store(facts,facts.facts.map((f,n)=>entry(facts,f,'domain-mixed-'+n)));check('ai-lint-waivers',matching);file(env,'lekalo.waivers.json',matching);
  const audit=cli(env,['waivers','audit',...modelCommon,'--check'],3).audit;check('waiver-audit',audit);
  assert.equal(audit.summary.waived,1);assert.equal(audit.findings.find(d=>d.fact.id===native.id).waiver.state,'unknown');controls++;
 }
 return controls;
}
export function gate(family){
 assert.ok(validators.has(family));const update=process.argv.includes('--update');
 const ci=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8').split('  build-test:')[1];assert.ok(ci.indexOf('node scripts/test-waivers-contracts.mjs')>ci.indexOf('cargo build --workspace --locked'));
 const provenance=JSON.parse(readFileSync(join(root,'tests/fixtures/fixture-provenance.json')));for(const f of validators.keys())assert.equal(provenance.families.find(row=>row.family===f)?.origin,'synthetic');
 const generated=spawnSync(process.execPath,[join(root,'scripts/gen-waivers-contracts.mjs'),'--check'],{cwd:root,encoding:'utf8'});assert.equal(generated.status,0,generated.stderr);
 const env=environment();let assertions=0;try{
  const lintArgs=['ai-lint','--module','planner','--config',env.policyPath,'--lint-profile','ci','--waiver-facts'];
  const raw=cli(env,lintArgs),facts=raw.waiverInput;check('waiver-input',facts);assert.ok(facts.facts.length>0);const f=facts.facts.find(f=>f.symbol.state==='known'&&f.target==='model');assert.ok(f);
  const common=['--facts','facts.json','--profile-kind','ai-lint','--lint-config','lint-config.json','--profile','ci'];file(env,'facts.json',facts);
  const waivers=store(facts,[entry(facts,f)]);file(env,'lekalo.waivers.json',waivers);check('ai-lint-waivers',waivers);
  if(family==='waiver-input')golden(family,'lint-facts',facts,update);
  if(family==='ai-lint-waivers')golden(family,'scoped',waivers,update);
  const audit=(s=waivers,i=facts,time=now,extra=[],code=0)=>{file(env,'lekalo.waivers.json',s);file(env,'facts.json',i);const a=cli(env,['waivers','audit',...common,'--as-of',time,...extra],code).audit;check('waiver-audit',a);assertions++;return a;};
  const active=audit();assert.equal(active.summary.waived,1);assert.equal(active.entries[0].status,'active');assert.deepEqual(active.findings.find(d=>d.fact.id===f.id).fact,f);assert.equal(active.findings.find(d=>d.fact.id===f.id).waiver.value,'waiver-depth');
  if(family==='waiver-audit')golden(family,'active',active,update);
  const at=audit(waivers,facts,'2026-11-01T12:00:00Z');assert.equal(at.summary.waived,1);assert.equal(at.entries[0].status,'expiring');
  const expired=audit(waivers,facts,'2026-11-01T12:00:01Z');assert.equal(expired.summary.waived,0);assert.equal(expired.entries[0].status,'expired');if(family==='waiver-audit')golden(family,'expired',expired,update);
  const afterReview=structuredClone(waivers);afterReview.entries[0].reviewAfter=known('2026-10-04T12:00:00Z');approval(afterReview.entries[0]);assert.equal(audit(afterReview,facts,'2026-10-04T12:00:01Z').entries[0].status,'expired');
  const earlier=audit(waivers,facts,'2026-10-04T11:59:59Z');assert.equal(earlier.entries[0].status,'unverifiable');
  for(const pin of ['adapter','revision','capabilities']){const s=structuredClone(waivers);s.entries[0].fingerprint[pin]=known(digest(`changed-${pin}`));approval(s.entries[0]);const a=audit(s);assert.equal(a.entries[0].status,'stale');assert.ok(a.entries[0].mismatchedPins.includes(pin));if(family==='waiver-audit'&&pin==='revision')golden(family,'stale',a,update);}
  for(const pin of ['model','ir']){const s=structuredClone(waivers);s.entries[0].fingerprint[pin]=known(digest(`changed-${pin}`));approval(s.entries[0]);assert.equal(audit(s).entries[0].status,'stale');}
  const unknownPins=structuredClone(waivers);unknownPins.entries[0].fingerprint.revision=unknown();approval(unknownPins.entries[0]);assert.equal(audit(unknownPins).entries[0].status,'unverifiable');
  for(const kind of ['symbol','module','project','target','profile']){const id=({symbol:f.symbol.value,module:f.module.value,project:facts.projectId,target:f.target,profile:facts.profileRef.id})[kind];const s=store(facts,[entry(facts,f,'scope-'+kind,{kind,id})]);assert.equal(audit(s).summary.waived,1);s.entries[0].scope.id='unrelated';approval(s.entries[0]);if(kind==='symbol')s.entries[0].scope.id='planner.unrelated';approval(s.entries[0]);assert.equal(audit(s).summary.waived,0);}
  const unrelated=structuredClone(facts);const extra=structuredClone(f);extra.id=digest('unrelated-symbol');extra.symbol=known('planner.edit_task_cmd');extra.subject='planner.edit_task_cmd';unrelated.facts.push(extra);const aUnrelated=audit(waivers,unrelated);assert.equal(aUnrelated.summary.waived,1);assert.equal(aUnrelated.findings.find(d=>d.fact.id===extra.id).waiver.state,'unknown');
  const orphanFacts={...facts,facts:[],complete:true};assert.equal(audit(waivers,orphanFacts).entries[0].status,'orphan');assert.equal(audit(waivers,{...orphanFacts,complete:false}).entries[0].status,'unexamined');
  const broad=store(facts,[entry(facts,f,'project-occurrence',{kind:'project',id:facts.projectId})]);const replacementSymbol=structuredClone(facts);replacementSymbol.facts=[{...f,id:digest('replaced-occurrence'),symbol:known('planner.edit_task_cmd')}];assert.equal(audit(broad,replacementSymbol).summary.waived,0);
  replacementSymbol.facts[0].id=f.id;const collision=audit(broad,replacementSymbol);assert.equal(collision.summary.waived,0);assert.equal(collision.entries[0].status,'stale');assert.ok(collision.entries[0].mismatchedPins.includes('fact'));
  const revoked=structuredClone(waivers);revoked.entries[0].lifecycle='revoked';assert.equal(audit(revoked).summary.waived,0);
  const superseded=structuredClone(waivers);superseded.entries[0].lifecycle='superseded';const replacement=entry(facts,f,'replacement');replacement.supersedes=known('waiver-depth');approval(replacement);superseded.entries.push(replacement);const lineage=audit(superseded);assert.equal(lineage.summary.waived,1);assert.equal(lineage.summary.superseded,1);
  for(const mutation of [s=>s.entries[0].scope.id='*',s=>s.entries[0].scope.id='planner.*',s=>s.entries[0].reason=' ',s=>s.entries[0].expiresAt=unknown(),s=>s.entries[0].owner=s.entries[0].approver,s=>s.entries[0].createdAt='2026-02-30T00:00:00Z',s=>s.entries.push(structuredClone(s.entries[0])),s=>s.entries[0].supersedes=known('missing')]){const bad=structuredClone(waivers);mutation(bad);bad.entries.forEach(approval);file(env,'lekalo.waivers.json',bad);file(env,'facts.json',facts);cli(env,['waivers','audit',...common],1);assertions++;}
  const forged=structuredClone(waivers);forged.entries[0].reason='Unreviewed change';file(env,'lekalo.waivers.json',forged);cli(env,['waivers','list'],1);
  file(env,'lekalo.waivers.json',canonical(waivers).replace('"entries":','"entries":[],"entries":'));cli(env,['waivers','list'],1);
  file(env,'lekalo.waivers.json',waivers);const escaped=canonical(waivers).replace('"entries":','"entr\\u0069es":[],"entries":');file(env,'lekalo.waivers.json',escaped);cli(env,['waivers','list'],1);
  for(const fam of validators.keys()){const v=structuredClone(fam==='ai-lint-waivers'?waivers:fam==='waiver-input'?facts:active);v.unexpected=true;check(fam,v,false);}
  file(env,'lekalo.waivers.json',waivers);file(env,'facts.json',facts);const listed=cli(env,['waivers','list']);assert.equal(listed.effectiveness,'unexamined');assert.equal(listed.storeDigest,hash(waivers));
  const before=file(env,'base.json',waivers),changed=structuredClone(waivers);changed.entries[0].reason+=' Reviewed update.';approval(changed.entries[0]);const diff=audit(changed,facts,now,['--base',before]);assert.equal(diff.changes[0].kind,'changed');assert.notEqual(diff.doneDigest,active.doneDigest);if(family==='waiver-audit')golden(family,'review-diff',diff,update);
  audit(waivers,facts,now,['--done',active.doneDigest]);file(env,'lekalo.waivers.json',changed);cli(env,['waivers','audit',...common,'--done',active.doneDigest],1);assertions++;
  // Security criticality is derived from the actual validation profile selection.
  const security=JSON.parse(readFileSync(join(root,'contracts/validation-profile.default.v0.6.4.json')));file(env,'validation-profile.json',security);
  const selected='semantic.public-output-private-type';const registry=JSON.parse(readFileSync(join(root,'contracts/diagnostic-registry.v0.6.4.json')));assert.equal(registry.entries.find(e=>e.id===selected).default_severity,'error');assert.ok(security.rules.some(r=>r.id===selected));
  const securityFacts=structuredClone(facts);securityFacts.profileRef={id:security.profile_id,version:security.version,digest:hash(security)};securityFacts.facts=[{...f,selector:{kind:'rule',id:selected},sourceOutcome:'warning',sourceSeverity:known('warning')}];const securityStore=store(securityFacts,[entry(securityFacts,securityFacts.facts[0],'security')]);file(env,'lekalo.waivers.json',securityStore);file(env,'security-facts.json',securityFacts);
  const sec=cli(env,['waivers','audit','--facts','security-facts.json','--profile-file','validation-profile.json','--profile','default','--as-of',now,'--check'],3).audit;check('waiver-audit',sec);assert.equal(sec.summary.nonWaivable,1);assert.equal(sec.findings[0].severity.value,'error');assert.equal(sec.findings[0].fact.sourceSeverity.value,'warning');assert.equal(sec.findings[0].effectiveGate,'denied');if(family==='waiver-audit')golden(family,'security-critical',sec,update);
  // Required coverage comes from current lint profile policy and cannot be waived.
  const required=structuredClone(env.policy);required.profiles.find(p=>p.id==='ci').gate.requiredCoverage=[f.selector.id];file(env,'required-config.json',required);const requiredFacts=structuredClone(facts);requiredFacts.profileRef.digest=hash(required);const requiredStore=store(requiredFacts,[entry(requiredFacts,f)]);file(env,'lekalo.waivers.json',requiredStore);file(env,'required-facts.json',requiredFacts);const req=cli(env,['waivers','audit','--facts','required-facts.json','--profile-kind','ai-lint','--lint-config','required-config.json','--profile','ci','--as-of',now]).audit;assert.equal(req.summary.nonWaivable,1);
  const targetDoc=JSON.parse(readFileSync(join(root,'tests/fixtures/target-profile/valid/node.json'))),resolved=JSON.parse(readFileSync(join(root,'tests/fixtures/target-profile/valid/resolved/node-postgres-http.expect.json')));
  file(env,'target-profile.json',targetDoc);const capFacts=structuredClone(facts);capFacts.profileRef={id:resolved.id,version:resolved.version,digest:hash(resolved)};capFacts.facts=[{...f,selector:{kind:'capability',id:'transport.streaming'},sourceOutcome:'unsupported'}];const capStore=store(capFacts,[entry(capFacts,capFacts.facts[0],'capability-gap')]);file(env,'capability-facts.json',capFacts);file(env,'lekalo.waivers.json',capStore);
  const cap=cli(env,['waivers','audit','--facts','capability-facts.json','--profile-kind','target','--profile-file','target-profile.json','--profile',resolved.id,'--as-of',now]).audit;check('waiver-audit',cap);assert.equal(cap.summary.waived,1);assert.equal(cap.findings[0].fact.sourceOutcome,'unsupported');assert.equal(cap.findings[0].effectiveGate,'accepted-risk');if(family==='waiver-input')golden(family,'capability-facts',capFacts,update);if(family==='waiver-audit')golden(family,'capability-gap',cap,update);
  // Real lint suppression retains every native field and report summary invariant.
  file(env,'lekalo.waivers.json',waivers);const live=cli(env,[...lintArgs,'--waivers',join(env.project,'lekalo.waivers.json'),'--as-of',now]);assert.equal(live.report.summary.raw,raw.report.summary.raw);assert.equal(live.report.summary.waived,1);for(const item of live.report.findings){const old=raw.report.findings.find(o=>o.id===item.id);const immutable=v=>{const c=structuredClone(v);delete c.disposition;delete c.waiver;return c;};assert.deepEqual(immutable(item),immutable(old));}check('waiver-audit',live.waiverAudit);
  const allStore=store(facts,facts.facts.map((f,i)=>entry(facts,f,'all-'+i,{kind:'project',id:facts.projectId})));file(env,'all-waivers.json',allStore);const all=cli(env,[...lintArgs,'--waivers',join(env.project,'all-waivers.json'),'--as-of',now,'--check']);assert.equal(all.report.summary.active,0);cli(env,[...lintArgs,'--waivers',join(env.project,'all-waivers.json'),'--as-of','2026-11-01T12:00:01Z','--check'],3);
  const e=allEvidence(env);e.pins.capabilities=known(digest('synthetic-capability-map'));const nativeArgs=[...lintArgs,'--evidence',env.file('native.json',e)],native=cli(env,nativeArgs);const nf=native.waiverInput.facts.find(f=>f.target===e.target&&f.path.state==='known');assert.ok(nf);const pathStore=store(native.waiverInput,[entry(native.waiverInput,nf,'path-waiver',{kind:'path',id:nf.path.value})]);file(env,'path-store.json',pathStore);const pathResult=cli(env,[...nativeArgs,'--waivers',join(env.project,'path-store.json'),'--as-of',now]);assert.equal(pathResult.report.summary.waived,1);if(family==='waiver-input')golden(family,'native-facts',native.waiverInput,update);
  const newer=structuredClone(e);newer.producer.version='0.6.5';const staleNative=cli(env,[...lintArgs,'--evidence',env.file('new-adapter.json',newer),'--waivers',join(env.project,'path-store.json'),'--as-of',now]);assert.equal(staleNative.waiverAudit.entries[0].status,'stale');assert.ok(staleNative.waiverAudit.entries[0].mismatchedPins.includes('adapter'));
  const newRevision=structuredClone(e);newRevision.pins.revision=known('b'.repeat(40));const changedRevision=cli(env,[...lintArgs,'--evidence',env.file('new-revision.json',newRevision),'--waivers',join(env.project,'path-store.json'),'--as-of',now]);assert.equal(changedRevision.waiverAudit.entries[0].status,'stale');assert.ok(changedRevision.waiverAudit.entries[0].mismatchedPins.includes('revision'));
  const nativePinControls=nativePinCases(env,e);
  const producerDomainControls=producerDomainCases(env,e,lintArgs,facts);
  // Preview/write/stale plan and root-home refusal, exercised through the real CLI.
  file(env,'lekalo.waivers.json',store(facts,[]));file(env,'facts.json',facts);
  const add=['waivers','add',f.selector.id,...common,'--id','added','--symbol',f.symbol.value,'--subject',f.subject,'--target',f.target,'--owner','fixture-owner','--approver','fixture-reviewer','--approval-ref','decision/88/add','--reason','Reviewed temporary synthetic risk.','--source-issue','ichinya/lekalo#88','--expires','2026-11-01T12:00:00Z'];
  const bytes=readFileSync(join(env.project,'lekalo.waivers.json'),'utf8'),preview=cli(env,add);check('ai-lint-waivers',preview.candidate);assert.equal(preview.applied,false);assert.equal(readFileSync(join(env.project,'lekalo.waivers.json'),'utf8'),bytes);cli(env,[...add,'--apply',digest('wrong-plan')],1);assert.equal(readFileSync(join(env.project,'lekalo.waivers.json'),'utf8'),bytes);
  const applied=cli(env,[...add,'--apply',preview.planId]);assert.equal(applied.applied,true);assert.equal(readFileSync(join(env.project,'lekalo.waivers.json'),'utf8'),canonical(applied.candidate)+'\n');cli(env,[...add,'--apply',preview.planId],1);
  for(const home of ['package.json','../outside.json','.lekalo/cache.sqlite','lekalo/project.yaml'])cli(env,['waivers','list','--store',home],1);
  const linked=join(env.project,'hardlink.json');linkSync(join(env.project,'lekalo.waivers.json'),linked);cli(env,['waivers','list'],1);unlinkSync(linked);
  // Lock proof is a source-run prerequisite, not an implicit waiver pin in v0.3.2.
  cli(env,['lock']);const lock=digest(readFileSync(join(env.project,'lekalo.lock'),'utf8').trimEnd());const lockedFacts={...facts,lockRef:known(lock)};file(env,'lekalo.waivers.json',waivers);file(env,'facts.json',lockedFacts);const locked=cli(env,['waivers','audit',...common,'--locked']).audit;assert.equal(locked.lockRef.value,lock);lockedFacts.lockRef=known(digest('other-lock'));file(env,'facts.json',lockedFacts);cli(env,['waivers','audit',...common,'--locked'],1);
  console.log(JSON.stringify({ok:true,family,version:'0.6.5',ajv:'8.17.1',live:true,audits:assertions,nativePinControls,producerDomainControls}));
 }finally{env.close();}
}
