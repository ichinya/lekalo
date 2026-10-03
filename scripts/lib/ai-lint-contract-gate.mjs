// Issue #76. Every family gate checks committed bytes AND fresh CLI behavior.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdtempSync, cpSync, mkdirSync, rmSync, existsSync, unlinkSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
export const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const fixture = join(root, 'tests/fixtures/ai-lint');
const binary = join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const require = createRequire((process.env.LEKALO_AJV_NODE_PATH || dirname(fileURLToPath(import.meta.url))) + '/');
assert.equal(require('ajv/package.json').version, '8.17.1', 'exact Ajv gate dependency');
const Ajv2020 = require('ajv/dist/2020').default;
const ajv = new Ajv2020({ strict: true, allErrors: true });
const validators = new Map();
const read = p => JSON.parse(readFileSync(join(root,p),'utf8'));
export const unknown = () => ({state:'unknown'});
export const known = value => ({state:'known',value});
export const ordered = v => Array.isArray(v) ? v.map(ordered) : v && typeof v==='object' ? Object.fromEntries(Object.keys(v).sort().map(k=>[k,ordered(v[k])])) : v;
export const canonical = v => JSON.stringify(ordered(v));
export const digest = bytes => 'sha256:'+createHash('sha256').update(bytes).digest('hex');
export const hash = v => digest(canonical(v));
const header = family => ({schemaVersion:`lekalo/ai-lint-${family}/v0.6.4`,identity:`dev.lekalo.ai-lint-${family}@0.6.4`});
export const rules = ['ambiguity.implicit-target-defaults','ambiguity.multiple-resolutions','ambiguity.scattered-state-writes','hidden.convention-only-path','hidden.dispatch-without-binding','hidden.observer-write','hidden.path-without-trace-owner','hidden.reflective-call','hidden.string-reference','hidden.undeclared-effect','indirection.depth-exceeded'];
export function config() {
  return {...header('config'),registryRef:{version:'0.6.4',digest:digest(readFileSync(join(root,'contracts/diagnostic-registry.v0.6.4.json')))},relatedWriterGroups:[],permittedDefaults:[],profiles:['advisory','ci','off'].map(id=>({id,recipe:'ai-readability/1',rules:rules.map(idRule=>({id:idRule,enabled:id!=='off',severity:unknown()})),thresholds:{semanticDependencyDepth:known(0),nativeCallDepth:known(1),uncoveredWriterGroups:known(1)},gate:{minimumConfidence:'high',failOnActiveWarnings:id==='ci',requiredCoverage:[],requireComparableBaseline:false,failOnRegression:false}}))};
}
export function schema(family,value,valid=true) {
  let validate=validators.get(family);
  if(!validate) { validate=ajv.compile(read(`contracts/ai-lint-${family}.schema.v0.6.4.json`));validators.set(family,validate); }
  assert.equal(validate(value),valid,`${family} schema: ${JSON.stringify(validate.errors)}`);
}
export function run(project,args,expected='valid') {
  const child=spawnSync(binary,['--json','--no-cache','ai-lint','--module','planner',...args],{cwd:project,encoding:'utf8',maxBuffer:16*1024*1024,timeout:60000});
  assert.ifError(child.error);
  const text=child.stdout.trim()||child.stderr.trim();
  let envelope;try { envelope=JSON.parse(text); } catch { throw new Error(`CLI output (${child.status}): ${text.slice(0,1000)}`); }
  if(expected) assert.equal(envelope.status,expected,text.slice(0,1000));
  assert.equal(child.status,({valid:0,invalid:1,'unsupported-version':5,denied:3})[expected??envelope.status],`exit: ${text.slice(0,500)}`);
  return {envelope,report:envelope.report??envelope.payload?.report,code:child.status};
}
function raw(project,args) {
  const child=spawnSync(binary,['--json','--no-cache',...args],{cwd:project,encoding:'utf8',maxBuffer:16*1024*1024,timeout:60000});
  assert.ifError(child.error);return {child,envelope:JSON.parse(child.stdout.trim()||child.stderr.trim())};
}
export function environment() {
  assert.ok(existsSync(binary),'cargo build must precede this gate');
  const project=mkdtempSync(join(tmpdir(),'lekalo-issue-76-'));
  cpSync(join(fixture,'model'),project,{recursive:true});
  const policy=config();const file=(name,value)=>{const path=join(project,name);writeFileSync(path,typeof value==='string'?value:canonical(value)+'\n');return path;};
  const policyPath=file('lint-config.json',policy),base=run(project,[]).report;
  const pins={model:known(base.modelRef),ir:known(base.irRef),observed:unknown(),revision:known('a'.repeat(40)),profile:unknown(),capabilities:unknown()};
  return {project,policy,policyPath,pins,file,args:['--config',policyPath],close:()=>rmSync(project,{recursive:true,force:true})};
}
export function evidence(env,target='node-typescript') {
  const bytes=readFileSync(join(env.project,'src/events.ts')),text=bytes.toString('utf8'),fingerprint=digest(bytes);
  const source={id:hash(['src/events.ts',fingerprint]),path:'src/events.ts',fingerprint,bytes:bytes.length};
  const start=Buffer.byteLength(text.slice(0,text.indexOf('task.state'))),end=start+Buffer.byteLength('task.state');
  const position=offset=>{const lines=bytes.subarray(0,offset).toString('utf8').split('\n');return [lines.length,[...lines.at(-1)].length+1];};
  const [line,column]=position(start),[endLine,endColumn]=position(end);
  const location={id:hash([source.id,start,end]),source:source.id,start,end,line,column,endLine,endColumn};
  return {...header('evidence'),target,scope:['planner'],producer:{id:'synthetic.ai-lint',version:'0.6.4',artifactDigest:digest('synthetic-source-collector/1'),tool:'synthetic-static',compiler:known('fixture/1'),framework:known('fixture-events/1'),recipe:'ai-readability/1'},pins:structuredClone(env.pins),inputManifestDigest:hash([source]),sources:[source],locations:[location],records:[],coverage:rules.map(rule=>({rule,target,scope:'planner',state:'complete',eligible:known(1),examined:known(1),limitations:[]})),limitations:[]};
}
export function record(e,kind,subject=kind) {
  const r={id:'',kind,mechanism:'direct',subject,semanticSymbol:known('planner.focus_task'),operation:known('planner.focus_task'),resource:unknown(),field:unknown(),nativeId:`fixture/${subject}`,confidence:'high',currency:'current',origin:'extracted',claim:'possible-behavior',binding:unknown(),configuration:unknown(),ownership:unknown(),trace:unknown(),value:unknown(),key:unknown(),candidates:[],activation:[],locations:[e.locations[0].id],guards:[]};
  return r;
}
export function add(e,r) {r.id=hash([r.kind,r.subject,r.nativeId,r.locations]);e.records.push(r);e.records.sort((a,b)=>a.id.localeCompare(b.id));return r;}
export function allEvidence(env,target='node-typescript') {
  const e=evidence(env,target);
  add(e,{...record(e,'binding-candidates'),confidence:'exact',claim:'structural',candidates:['one','two'].map(identity=>({identity,confidence:'exact',currency:'current',origin:'explicit'}))});
  for(const kind of ['dispatch','reflection','string-reference'])add(e,{...record(e,kind),binding:known(false)});
  add(e,{...record(e,'convention'),configuration:known(false)});
  add(e,record(e,'path'));
  for(const mechanism of ['direct','observer'])add(e,{...record(e,'effect',mechanism),mechanism,resource:known('planner.task'),field:known('state'),key:known('update'),activation:['binding','trigger','registration','callback','effect'].map(role=>({role,identity:`fixture/${role}`,locations:[e.locations[0].id],guards:['listener-still-registered'],confidence:'high'}))});
  for(const operation of ['planner.focus_task','planner.edit_task_cmd']) add(e,{...record(e,'field-write',operation),operation:known(operation),semanticSymbol:known(operation),resource:known('planner.task'),field:known('state'),claim:'structural'});
  add(e,{...record(e,'default','planner.focus_task'),key:known('batch-size'),value:known(target==='node-typescript'?'1':'20'),configuration:known(false),claim:'structural'});
  for(let i=0;i<3;i++)add(e,{...record(e,'native-edge',`call${i}`),nativeId:`call${i}`,value:known(`call${i+1}`),claim:'structural'});
  return e;
}
export function attachments(env) {
  const transition={attachmentRevision:'0.2.16',identity:'dev.lekalo.invariant-transition@0.2.16',schemaVersion:'lekalo/invariant-transition/v0.2.16',projectId:'planner',modelRef:{digest:env.pins.model.value,modelVersion:'0.2.16'},irRef:{digest:env.pins.ir.value,identity:'dev.lekalo.ir@0.2.16'},stateSpaces:[{stateSpaceId:'planner.task',entity:'planner.task',cyclePolicy:'allow',deadPolicy:'allow',states:[{stateId:'backlog',initial:true,terminal:true}]}],invariants:[],transitions:[],verificationMappings:[],propertyHints:[]};
  const trace=read('tests/fixtures/trace/full.trace.json');trace.modelRef.digest=env.pins.model.value;trace.irRef={schemaVersion:'0.2.16',digest:env.pins.ir.value};trace.sourceRevision='a'.repeat(40);
  if(!existsSync(join(env.project,'lekalo.lock'))) {const locked=raw(env.project,['lock']);assert.equal(locked.child.status,0,JSON.stringify(locked.envelope));}
  const currentLoad=raw(env.project,['load']).child.stdout.trim();
  const manifest={schema_version:'lekalo/artifact-manifest/v0.2.16',identity:'dev.lekalo.artifact-manifest@0.2.16',project_ref:'planner',lock_ref:{schema_version:'lekalo/lock/v0.3.2',digest:digest(readFileSync(join(env.project,'lekalo.lock'),'utf8').trimEnd())},inputs:{model:{version:'0.2.16',digest:digest(currentLoad)},ir:{version:'0.2.16',digest:env.pins.ir.value}},artifacts:[],source_maps:[]};manifest.manifest_digest=hash(manifest);trace.artifactManifestRef={schemaVersion:'0.2.16',digest:manifest.manifest_digest};
  return {transition,trace,manifest,args:['--transitions',env.file('transitions.json',transition),'--trace',env.file('trace.json',trace),'--artifacts',env.file('artifacts.json',manifest)]};
}
export function golden(name,value,update) {
  const family=['report','evidence','config','waivers','comparison'].find(f=>name===f||name.startsWith(f+'-'));
  const path=family?join(root,'tests/fixtures',`ai-lint-${family}`,'golden',name+'.json'):join(fixture,'golden',name+'.json');
  if(update) {mkdirSync(dirname(path),{recursive:true});writeFileSync(path,canonical(value)+'\n');}
  else assert.equal(readFileSync(path,'utf8'),canonical(value)+'\n',`golden drift: ${name}`);
}
const clone=structuredClone;
function refusal(env,args,id,status='invalid') { const r=run(env.project,args,status);assert.ok(r.envelope.diagnostics.some(d=>d.id===id),JSON.stringify(r.envelope));return r; }
export function reportCases(env,update) {
  const e=allEvidence(env),php=allEvidence(env,'php-laravel'),a=attachments(env),args=[...env.args,'--evidence',env.file('evidence.json',e),'--evidence',env.file('php-evidence.json',php),...a.args];
  const {report,envelope}=run(env.project,args);schema('report',report);schema('evidence',e);schema('evidence',php);
  for(const rule of rules)assert.ok(report.findings.some(f=>f.ruleId===rule),`producing rule: ${rule}`);
  for(const rule of rules)assert.ok(envelope.diagnostics.some(d=>d.id===rule),`registered projection: ${rule}`);
  assert.ok(envelope.diagnostics.every(d=>d.registry_version==='0.6.4'));
  assert.ok(!canonical(report).includes('src/events.ts'),'default report privacy');
  assert.ok(report.findings.every(f=>f.evidence.length&&f.scope.join()==='planner'&&f.conditionDigest&&f.disposition==='active'&&f.alternative));
  assert.ok(report.findings.filter(f=>f.ruleId.startsWith('hidden.')).every(f=>f.claim!=='verified-behavior'));
  assert.equal(report.summary.verifiedEffects,0);
  assert.deepEqual(run(env.project,args).report,report,'determinism');
  golden('report',report,update);golden('evidence',e,update);golden('evidence-php',php,update);golden('config',env.policy,update);
  const check=run(env.project,[...args,'--lint-profile','ci','--check'],'denied');assert.ok(check.envelope.diagnostics.some(d=>d.id==='ai-lint.policy-denied'));
  const advisory=run(env.project,[...args,'--lint-profile','ci']);assert.equal(advisory.envelope.status,'valid');
  const spans=run(env.project,[...args,'--spans']);assert.ok(canonical(spans.envelope.sourceLocations).includes('src/events.ts'));
  const off=run(env.project,[...args,'--lint-profile','off']).report;assert.equal(off.summary.raw,0);assert.ok(off.coverage.every(c=>c.state==='disabled'));
  const gap=run(env.project,env.args).report;assert.ok(gap.coverage.some(c=>c.state==='unknown'));assert.equal(gap.summary.verifiedEffects,0);
  return {report,e,php,a,args};
}
export function configCases(env,update) {
  schema('config',env.policy);golden('config',env.policy,update);
  for(const mutate of [p=>p.extra=1,p=>p.profiles[0].rules[0].severity=known('error'),p=>p.profiles[0].rules[0].id='unknown.rule',p=>p.profiles.reverse(),p=>p.profiles[0].gate.minimumConfidence='low',p=>p.profiles[0].gate.requiredCoverage=['unregistered.rule']]) {
    const p=clone(env.policy);mutate(p);refusal(env,['--config',env.file('invalid-config.json',p)],'ai-lint.input-invalid');
  }
  const v=clone(env.policy);v.schemaVersion='lekalo/ai-lint-config/v999';refusal(env,['--config',env.file('wrong-version.json',v)],'ai-lint.version-unsupported','unsupported-version');
  const e=allEvidence(env),a=attachments(env);const args=['--evidence',env.file('config-evidence.json',e),...a.args];
  const related=clone(env.policy);related.relatedWriterGroups=[{resource:'planner.task',field:'state',operations:['planner.edit_task_cmd','planner.focus_task'],owner:'task-owner',reason:'One coordinated workflow.'}];
  related.profiles[0].thresholds.uncoveredWriterGroups=known(1);
  const grouped=run(env.project,['--config',env.file('related.json',related),...args]).report;assert.ok(!grouped.findings.some(f=>f.ruleId==='ambiguity.scattered-state-writes'));
  const downgraded=clone(env.policy);downgraded.profiles[1].rules.forEach(r=>r.severity=known('info'));
  const info=run(env.project,['--config',env.file('info.json',downgraded),...args,'--lint-profile','ci','--check']);assert.ok(info.report.findings.every(f=>f.severity==='info'));
  const required=clone(env.policy);required.profiles[1].gate.requiredCoverage=['hidden.observer-write'];run(env.project,['--config',env.file('required.json',required),'--lint-profile','ci','--check'],'denied');
  const duplicate=canonical(env.policy).replace('"profiles":','"profiles":[],"profiles":');refusal(env,['--config',env.file('duplicate.json',duplicate)],'ai-lint.input-invalid');
  const nul=clone(env.policy);nul.profiles[0].thresholds.nativeCallDepth=null;schema('config',nul,false);refusal(env,['--config',env.file('null.json',nul)],'ai-lint.input-invalid');
}
export function evidenceCases(env,update) {
  const e=allEvidence(env);schema('evidence',e);golden('evidence',e,update);const a=attachments(env);
  const live=run(env.project,[...env.args,'--evidence',env.file('input.json',e),...a.args]).report;
  const f=live.findings.find(f=>f.ruleId==='hidden.observer-write');assert.equal(f.confidence,'high');assert.equal(f.claim,'possible-behavior');
  const low=clone(e);low.records.find(r=>r.mechanism==='observer').activation[2].confidence='low';const lr=run(env.project,[...env.args,'--evidence',env.file('low.json',low),...a.args]).report;assert.equal(lr.findings.find(f=>f.ruleId==='hidden.observer-write').severity,'info');
  for(const mutate of [v=>v.pins.model=known(digest('other-model')),v=>v.locations[0].end++,v=>v.sources[0].fingerprint=digest('stale'),v=>v.records[0].claim='verified-behavior',v=>v.records.find(r=>r.kind==='effect').claim='structural',v=>v.records.find(r=>r.kind==='effect').activation.pop(),v=>v.coverage[0].examined=known(2),v=>v.records[0].resource=known('planner.absent'),v=>v.records.find(r=>r.kind==='field-write').field=known('absent')]) {const v=clone(e);mutate(v);refusal(env,[...env.args,'--evidence',env.file('bad-evidence.json',v)],'ai-lint.input-invalid');}
  const partial=clone(e);partial.coverage.forEach(c=>{c.state='partial';c.limitations=['unresolved-external'];});const pr=run(env.project,[...env.args,'--evidence',env.file('partial.json',partial)]).report;assert.ok(!pr.findings.some(f=>['hidden.dispatch-without-binding','hidden.convention-only-path'].includes(f.ruleId)));
  const old=clone(e);old.records.forEach(r=>r.currency='stale');const sr=run(env.project,[...env.args,'--evidence',env.file('old.json',old)]).report;assert.ok(!sr.findings.some(f=>f.target==='node-typescript'));assert.ok(sr.coverage.some(c=>c.state==='partial'));
  const owned=attachments(env),manifest=clone(owned.manifest);manifest.artifacts=[{semantic_owner:'planner.focus_task',path:'src/events.ts',artifact_kind:'source',lifecycle:'custom',content:{algorithm:'sha256',digest:e.sources[0].fingerprint,canonicalization:'exact-file-bytes'},input_refs:['planner.focus_task'],regeneration_policy:'manual-only'}];delete manifest.manifest_digest;manifest.manifest_digest=hash(manifest);
  const ownerTrace=clone(owned.trace),artifact=ownerTrace.nodes.find(n=>n.nodeKind==='artifact');artifact.path='src/events.ts';artifact.contentDigest=e.sources[0].fingerprint;artifact.ownership='custom';ownerTrace.artifactManifestRef.digest=manifest.manifest_digest;
  const exactOwner=run(env.project,[...env.args,'--evidence',env.file('owned-input.json',e),'--transitions',env.file('owned-transitions.json',owned.transition),'--trace',env.file('owned-trace.json',ownerTrace),'--artifacts',env.file('owned-artifacts.json',manifest)]).report;assert.ok(!exactOwner.findings.some(f=>f.ruleId==='hidden.path-without-trace-owner'),'current file + exact manifest owner + trace binding');
  const originalSource=readFileSync(join(env.project,'src/events.ts'),'utf8');writeFileSync(join(env.project,'src/events.ts'),'// shifted caf? ??\n'+originalSource);const shifted=allEvidence(env);const afterShift=run(env.project,[...env.args,'--evidence',env.file('shifted.json',shifted),...a.args]).report;assert.deepEqual(afterShift.findings.filter(f=>f.target==='node-typescript').map(f=>f.id),live.findings.filter(f=>f.target==='node-typescript').map(f=>f.id),'stable semantic identities exclude spans');assert.ok(afterShift.findings.filter(f=>f.target==='node-typescript').every(f=>f.conditionDigest!==live.findings.find(old=>old.id===f.id)?.conditionDigest),'source changes revoke target condition pins');writeFileSync(join(env.project,'src/events.ts'),originalSource);
  const noTrace=run(env.project,[...env.args,'--evidence',env.file('no-trace.json',e)]).report;assert.ok(!noTrace.findings.some(f=>f.ruleId==='hidden.path-without-trace-owner'));assert.ok(noTrace.coverage.some(c=>c.rule==='hidden.path-without-trace-owner'&&c.state==='unknown'));
}
export function waiverCases(env,update) {
  const e=allEvidence(env),a=attachments(env),args=[...env.args,'--evidence',env.file('waiver-input.json',e),...a.args],report=run(env.project,args).report;
  const entries=report.findings.map((f,i)=>({id:`waiver-${i}`,ruleId:f.ruleId,subject:f.subject,target:f.target,conditionDigest:f.conditionDigest,owner:'issue-76-owner',reason:'Intentional fixture exception with exact semantic scope.',expiresOn:known('2026-10-03'),sourceDigest:unknown()}));
  const w={...header('waivers'),configRef:hash(env.policy),entries};schema('waivers',w);golden('waivers',w,update);
  const path=env.file('waivers.json',w),result=run(env.project,[...args,'--waivers',path,'--as-of','2026-10-03']).report;
  assert.equal(result.summary.raw,report.summary.raw);assert.equal(result.summary.active,0);assert.equal(result.summary.waived,result.summary.raw);golden('report-waived',result,update);
  assert.equal(run(env.project,[...args,'--waivers',path,'--as-of','2026-10-04']).report.summary.waived,0);
  refusal(env,[...args,'--waivers',path],'ai-lint.waiver-invalid');
  const conflict=clone(w);conflict.entries.push({...conflict.entries[0],id:'duplicate-scope'});refusal(env,[...args,'--waivers',env.file('conflict.json',conflict),'--as-of','2026-10-03'],'ai-lint.waiver-invalid');
  for(const mutate of [v=>v.entries[0].subject='*',v=>v.entries[0].reason=' ',v=>v.entries[0].expiresOn=known('2026-02-30')]){const v=clone(w);mutate(v);refusal(env,[...args,'--waivers',env.file('bad-waiver.json',v),'--as-of','2026-10-03'],'ai-lint.waiver-invalid');}
  const permanent=clone(w);permanent.entries.forEach(v=>v.expiresOn=unknown());assert.equal(run(env.project,[...args,'--waivers',env.file('permanent.json',permanent)]).report.summary.waived,report.summary.raw);
  const foreign=clone(permanent);foreign.entries.forEach(v=>v.target='php-laravel');assert.equal(run(env.project,[...args,'--waivers',env.file('foreign-waivers.json',foreign)]).report.summary.waived,0);
  const changed=clone(w);changed.entries.forEach(v=>v.conditionDigest=digest('changed'));const stale=run(env.project,[...args,'--waivers',env.file('changed-waiver.json',changed),'--as-of','2026-10-03']).report;assert.equal(stale.summary.waived,0);assert.ok(stale.waiverAudit.every(a=>a.disposition==='condition-changed'));
  return {report,result,w,args};
}
export function comparisonCases(env,update) {
  const {report,result,args}=waiverCases(env,update),base=env.file('baseline.json',report);
  const same=run(env.project,[...args,'--baseline',base]).report;
  schema('comparison',same.comparison.value);assert.equal(same.comparison.value.comparable,true);assert.equal(same.comparison.value.regression.value,false);golden('comparison',same.comparison.value,update);
  const waived=env.file('waived-baseline.json',result),churn=run(env.project,[...args,'--baseline',waived]).report.comparison.value;assert.equal(churn.regression.value,false);assert.ok(churn.deltas.some(d=>d.active>0&&d.raw===0));
  const gap=run(env.project,[...env.args,'--baseline',base]).report.comparison.value;assert.equal(gap.comparable,false);assert.equal(gap.regression.state,'unknown');golden('comparison-incomparable',gap,update);
  const beforeBytes=readFileSync(base),next=allEvidence(env);next.pins.revision=known('b'.repeat(40));
  const nextAttachments=attachments(env);nextAttachments.trace=JSON.parse(JSON.stringify(nextAttachments.trace).replaceAll('a'.repeat(40),'b'.repeat(40)));nextAttachments.args[3]=env.file('next-trace.json',nextAttachments.trace);
  const compared=run(env.project,[...env.args,'--evidence',env.file('next-revision.json',next),...nextAttachments.args,'--baseline',base]).report.comparison.value;assert.equal(compared.comparable,true);assert.equal(compared.regression.value,false);assert.deepEqual(readFileSync(base),beforeBytes,'immutable baseline');
  const deeper=clone(next);add(deeper,{...record(deeper,'native-edge','call3'),nativeId:'call3',value:known('call4'),claim:'structural'});
  const deepened=run(env.project,[...env.args,'--evidence',env.file('deeper.json',deeper),...nextAttachments.args,'--baseline',base]).report.comparison.value;assert.equal(deepened.regression.value,true);assert.ok(deepened.depthDeltas.some(d=>d.dimension==='native-call'&&d.change===1));
  const fewer=clone(next);fewer.records.find(r=>r.kind==='binding-candidates').candidates.pop();const improved=run(env.project,[...env.args,'--evidence',env.file('unambiguous.json',fewer),...nextAttachments.args,'--baseline',base]).report.comparison.value;assert.ok(improved.deltas.some(d=>d.rule==='ambiguity.multiple-resolutions'&&d.raw===-1));assert.equal(improved.regression.value,false);
  const bad=clone(report);bad.summary.raw++;refusal(env,[...env.args,'--baseline',env.file('bad-baseline.json',bad)],'ai-lint.input-invalid');
  const metrics=clone(report);metrics.metrics.reverse();refusal(env,[...env.args,'--baseline',env.file('unordered-baseline.json',metrics)],'ai-lint.input-invalid');
  const regression=clone(report);regression.findings=[];regression.metrics.forEach(m=>m.raw=m.active=m.waived=0);Object.assign(regression.summary,{raw:0,active:0,waived:0,possibleEffects:0,ambiguitySets:0});
  const deterioration=run(env.project,[...args,'--baseline',env.file('clean-baseline.json',regression)]).report.comparison.value;assert.equal(deterioration.regression.value,true);assert.ok(deterioration.deltas.some(d=>d.raw>0));
}
export function adapterCases(env,update) {
  const protocol=ajv.compile(read('contracts/target-protocol.schema.v0.6.4.json')),frozen=ajv.compile(read('contracts/target-protocol.schema.v0.3.2.json'));
  for(const [target,runtime,path,bindings] of [
    ['node-typescript',process.execPath,'src/events.ts',[['src/events.ts#Task','planner.task','entity'],['src/events.ts#focus','planner.focus_task','command'],['src/events.ts#edit','planner.edit_task_cmd','command']]],
    ['php-laravel','php','app/events.php',[['app/events.php#Task','planner.task','entity'],['app/events.php#Commands.focus','planner.focus_task','command'],['app/events.php#Commands.edit','planner.edit_task_cmd','command']]]]) {
    const adapter=join(root,'adapters',target,target==='node-typescript'?'adapter.mjs':'adapter.php'),bytes=readFileSync(join(env.project,path));
    const request={protocol:'lekalo.target/v1',protocol_version:'0.6.4',request_id:'req-'+'7'.repeat(64),operation:'lint',project_root:'.',target,limits:{max_output_bytes:8388608,timeout_ms:600000},lint_request:{scope:['planner'],pins:env.pins,bindings:bindings.map(([nativeId,symbol,kind])=>({path,nativeId,symbol,kind,fingerprint:digest(bytes)})),files:[path]}};
    assert.ok(protocol(request),JSON.stringify(protocol.errors));assert.equal(frozen(request),false);
    const call=r=>spawnSync(runtime,[adapter],{cwd:env.project,input:canonical(r),encoding:'utf8',timeout:60000,maxBuffer:16*1024*1024});
    const child=call(request);assert.equal(child.status,0,child.stderr);const response=JSON.parse(child.stdout);assert.ok(protocol(response),JSON.stringify(protocol.errors));const e=response.result.lint_evidence;schema('evidence',e);
    assert.ok(e.records.some(r=>r.kind==='effect'&&r.mechanism==='observer'&&r.activation.length===5),`${target} actual observer chain`);assert.ok(e.records.some(r=>r.kind==='reflection'),`${target} dynamic call`);assert.ok(e.coverage.every(c=>c.state!=='complete'));
    const live=run(env.project,[...env.args,'--evidence',env.file(`${target}-live.json`,e)]).report;
    assert.ok(live.findings.some(f=>f.ruleId==='hidden.observer-write'&&f.semanticSymbol.value==='planner.focus_task'));
    assert.ok(!live.findings.some(f=>f.ruleId==='hidden.observer-write'&&f.semanticSymbol.value==='planner.edit_task_cmd'),'declared effect control');
    assert.ok(live.findings.some(f=>f.ruleId==='hidden.reflective-call'));assert.equal(live.summary.verifiedEffects,0);
    // Compiler/runtime pins are part of custody, so golden producer receipts are
    // schema vectors; live receipts are always regenerated and admitted above.
    const snapshot=clone(e);snapshot.producer.compiler=known(target==='php-laravel'?'php/8.3':'typescript/5.9.3');golden(`adapter-${target}`,snapshot,update);
    const duplicateBinding=clone(request);duplicateBinding.lint_request.bindings.push({...duplicateBinding.lint_request.bindings[0],symbol:'planner.edit_task_cmd'});assert.notEqual(call(duplicateBinding).status,0,'ambiguous native join is refused');
    const old=clone(request);old.protocol_version='0.3.2';assert.notEqual(call(old).status,0);
    const extra=clone(request);extra.lint_request.extra=true;assert.notEqual(call(extra).status,0);
    const nullRequest=clone(request);nullRequest.lint_request=null;assert.notEqual(call(nullRequest).status,0);
    const traversal=clone(request);traversal.lint_request.files=['../host.ts'];assert.notEqual(call(traversal).status,0);
    // A same-spelled API without recognized receiver/registration is not a hook.
    const original=bytes.toString('utf8'),negative=target==='node-typescript'?original.replace("bus.on('saved',", "// bus.on('saved',"):original.replace('Task::observe(TaskObserver::class);','// Task::observe(TaskObserver::class);');
    writeFileSync(join(env.project,path),negative);const negRequest=clone(request);negRequest.lint_request.bindings.forEach(b=>b.fingerprint=digest(negative));const nc=call(negRequest);assert.equal(nc.status,0,nc.stderr);assert.ok(!JSON.parse(nc.stdout).result.lint_evidence.records.some(r=>r.kind==='effect'));writeFileSync(join(env.project,path),bytes);
    const install=raw(env.project,['adapter','install','path:'+join(root,'adapters',target),'--dry-run']);assert.equal(install.child.status,0,JSON.stringify(install.envelope));
    const plan=install.envelope.plan?.planId??install.envelope.planId??install.envelope.plan?.id;assert.ok(plan,JSON.stringify(install.envelope));
    const applied=raw(env.project,['adapter','install','path:'+join(root,'adapters',target),'--confirm',plan]);assert.equal(applied.child.status,0,JSON.stringify(applied.envelope));
    const scan=run(env.project,[...env.args,'--scan-target',target,'--source-file',path]);assert.ok(scan.report.coverage.some(c=>c.target===target));assert.equal(scan.report.summary.verifiedEffects,0);
    const observed={schemaVersion:'lekalo/observed-scan/v0.2.16',adapter:{id:'lekalo-target-'+target,version:target==='node-typescript'?'0.4.0':'0.2.0',digest:null},project:'planner',revision:digest(bytes),symbols:bindings.map(([nativeId,symbol,kind])=>({id:symbol,kind,stableKey:nativeId,location:{path,line:1},fingerprint:digest(bytes),mappingConfidence:'high',evidence:{}})),endpoints:[],schemas:[]};
    const updated=raw(env.project,['observe','update','--scan',env.file('observed-scan.json',observed)]);assert.equal(updated.child.status,0,JSON.stringify(updated.envelope));
    for(const [,symbol]of bindings){const confirmed=raw(env.project,['observe','confirm',symbol]);assert.equal(confirmed.child.status,0,JSON.stringify(confirmed.envelope));}
    const indexPath=join(env.project,'.lekalo/import/observed/index.json'),beforeIndex=readFileSync(indexPath),beforeSource=readFileSync(join(env.project,path));
    const collected=run(env.project,[...env.args,'--scan-target',target,'--source-file',path,'--spans']);
    assert.ok(collected.report.findings.some(f=>f.ruleId==='hidden.observer-write'&&f.semanticSymbol.value==='planner.focus_task'),target+' installed production collector positive');
    assert.ok(!collected.report.findings.some(f=>f.ruleId==='hidden.observer-write'&&f.semanticSymbol.value==='planner.edit_task_cmd'));
    assert.ok(collected.report.findings.some(f=>f.ruleId==='hidden.reflective-call'));assert.equal(collected.report.summary.verifiedEffects,0);
    assert.deepEqual(readFileSync(indexPath),beforeIndex,'lint retains observed index');assert.deepEqual(readFileSync(join(env.project,path)),beforeSource,'lint retains source');
    unlinkSync(indexPath);
  }
}
export function gate(family) {
  const update=process.argv.includes('--update'),env=environment();
  try {
    const ci=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8').split('  build-test:')[1];assert.ok(ci,'build-test job');const buildAt=ci.indexOf('cargo build --workspace --locked');assert.ok(buildAt>=0);for(const f of ['report','evidence','config','waivers','comparison']) {assert.ok(ci.indexOf('node scripts/test-ai-lint-'+f+'-contracts.mjs')>buildAt,'gate runs after build');assert.equal(read('tests/fixtures/fixture-provenance.json').families.find(e=>e.family==='ai-lint-'+f)?.origin,'synthetic');}
    if(family==='report') {for(const generator of ['gen-ai-lint-contracts.mjs','gen-ai-lint-protocol.mjs']) {const r=spawnSync(process.execPath,[join(root,'scripts',generator),'--check'],{cwd:root,encoding:'utf8'});assert.equal(r.status,0,r.stderr);}}
    if(family==='report')reportCases(env,update);
    else if(family==='evidence'){evidenceCases(env,update);adapterCases(env,update);}
    else if(family==='config')configCases(env,update);
    else if(family==='waivers')waiverCases(env,update);
    else if(family==='comparison')comparisonCases(env,update);
    else throw new Error('unknown contract gate');
    process.stdout.write(JSON.stringify({ok:true,family:`ai-lint-${family}`,schema:'0.6.4',live:true})+'\n');
  } finally {env.close();}
}
