#!/usr/bin/env node
// Issue #77: mandatory production producer, five closed families, pinned Ajv.
// Read-only against the checkout. Every mutation below is a temporary probe.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync,writeFileSync,existsSync,mkdtempSync,cpSync,readdirSync,statSync,rmSync,realpathSync } from 'node:fs';
import { resolve,dirname,join,basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const require=createRequire(import.meta.url);
const fail=(reason,detail)=>{process.stderr.write(JSON.stringify({ok:false,gate:'coupling-contracts',reason,detail})+'\n');process.exit(1);};
let Ajv;try {Ajv=require('ajv/dist/2020').default;if(require('ajv/package.json').version!=='8.17.1')fail('ajv-version','expected 8.17.1');}catch {fail('ajv-unavailable','set NODE_PATH=$LEKALO_AJV_NODE_PATH');}
const binary=process.env.LEKALO_COUPLING_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
if(!existsSync(binary))fail('binary-missing','cargo build --workspace --locked');
const read=p=>JSON.parse(readFileSync(join(root,p),'utf8'));
const fixture='tests/fixtures/coupling',schemas=new Map(),ajv=new Ajv({strict:true,allErrors:true});
for(const family of ['report','profile','comparison','evidence','change-input'])schemas.set(family,ajv.compile(read(`contracts/coupling-${family}.schema.v0.6.4.json`)));
const validates=(family,value)=>{const check=schemas.get(family);assert.ok(check(value),`${family}: ${JSON.stringify(check.errors?.slice(0,2))}`);};
const sha=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const unique=xs=>[...new Set(xs)].sort();
const checked=[];
function run(args,{cwd=root,exit=0}={}) {
 const p=spawnSync(binary,['--json',...args],{cwd,encoding:'utf8',timeout:120000,maxBuffer:32*1024*1024});
 assert.equal(p.status,exit,JSON.stringify({args,status:p.status,error:p.error?.code,stdout:p.stdout?.slice(0,500),stderr:p.stderr?.slice(0,700)}));
 if(exit===0||exit===3)assert.equal(p.stderr,'');else assert.equal(p.stdout,'');
 const document=JSON.parse(exit===0||exit===3?p.stdout:p.stderr);
 if(document.coupling)validates('report',document.coupling);
 if(document.payload?.coupling)validates('report',document.payload.coupling);
 if(document.couplingComparison)validates('comparison',document.couplingComparison);
 if(document.payload?.couplingComparison)validates('comparison',document.payload.couplingComparison);
 return {document,stdout:p.stdout,stderr:p.stderr};
}
const reportOf=o=>o.document.coupling??o.document.payload?.coupling;
const row=(r,id)=>{const s=r.subjects.find(s=>s.subject===id);assert.ok(s,id);return s;};
function files(dir,prefix=''){return readdirSync(dir).sort().flatMap(name=>{const p=join(dir,name),relative=prefix+name;return statSync(p).isDirectory()?files(p,relative+'/'):[[relative,sha(readFileSync(p))]];});}
const scratch=mkdtempSync(join(tmpdir(),'lekalo-coupling-gate-'));
const temp=(name,value)=>{const p=join(scratch,name);writeFileSync(p,typeof value==='string'?value:JSON.stringify(value));return p;};
try {
 const registry=read('contracts/diagnostic-registry.v0.6.4.json');
 const entries=registry.entries.filter(e=>e.id.startsWith('coupling.'));assert.equal(entries.length,15);
 assert.deepEqual(entries.map(e=>e.code).sort(),Array.from({length:15},(_,i)=>`LEK-COUPLING-${String(i+1).padStart(3,'0')}`));
 assert.ok(read('tests/fixtures/fixture-provenance.json').families.some(f=>f.family==='coupling'&&f.origin==='synthetic'));
 const ci=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8');const build=ci.indexOf('cargo build --workspace --locked'),gate=ci.indexOf('node scripts/test-coupling-contracts.mjs');
 assert.ok(build>=0&&gate>build,'gate must follow build');assert.match(ci.slice(gate-300,gate),/LEKALO_AJV_NODE_PATH/);
 const golden={report:'planner.report.json',profile:'advisory.profile.json',comparison:'planner.comparison.json',evidence:'planner.evidence.json','change-input':'planner.change-input.json'};
 for(const [family,path]of Object.entries(golden)){validates(family,read(`${fixture}/golden/${path}`));checked.push(`golden-${family}`);}
 for(const name of ['planner.evidence-report.json','taskhub.report.json','provider.report.json'])validates('report',read(`${fixture}/golden/${name}`));
 for(const [family,path]of [['profile','unknown-member.profile.json'],['comparison','unavailable-value.comparison.json']])assert.equal(schemas.get(family)(read(`${fixture}/invalid/${path}`)),false);
 // Independent graph oracle: occurrence multiplicity and supporting roles are
 // excluded before reverse reachability, then classification is checked below.
 const args=['coupling','--symbol','planner.task','--project',`${fixture}/planner`];
 const first=run(args),second=run(args),report=reportOf(first);assert.equal(first.stdout,second.stdout);
 assert.deepEqual(report,read(`${fixture}/golden/planner.report.json`));
 const graph=run(['graph','export','--project',`${fixture}/planner`]).document.graph;
 const roles=new Set(['entity-field','value-object-field','event-payload','command-effect','effect-entity','command-input','query-returns','query-reads','endpoint-invokes','effect-emits']);
 const edges=graph.edges.filter(e=>roles.has(e.provenance.referenceRole));
 const rootId='entity:planner.task',s=row(report,rootId);
 assert.deepEqual(s.fanIn,unique(edges.filter(e=>e.to===rootId&&e.from!==rootId).map(e=>e.from)));
 assert.deepEqual(s.fanOut,unique(edges.filter(e=>e.from===rootId&&e.to!==rootId).map(e=>e.to)));
 const closure=new Set([rootId]);let more=true;while(more){more=false;for(const e of edges)if(closure.has(e.to)&&!closure.has(e.from)){closure.add(e.from);more=true;}}
 assert.deepEqual(unique([...s.impact.publicContracts,...s.impact.internalSymbols,...s.impact.supportingSymbols,...s.impact.unclassifiedSymbols]),[...closure].sort());
 assert.equal(s.metrics.fanInSymbols.value,6);assert.equal(s.metrics.fanOutSymbols.value,5);assert.equal(s.metrics.fanInModules.value,0);assert.equal(s.metrics.publicContractsAffected.value,8);
 assert.equal(s.metrics.sharedMutableResources.value,1);assert.equal(s.metrics.affectedTests.state,'unknown');assert.equal(s.metrics.affectedTargets.state,'unsupported');
 assert.equal(report.profile.gate.mode,'advisory');assert.ok(!report.findings.some(f=>f.rule==='coupling.fan-exceeded'));
 assert.equal(row(report,'project:planner').metrics.publicContractsAffected.value,8);
 for(const w of report.witnesses)for(const e of w.edges)assert.ok(graph.edges.some(g=>`${g.from}|${g.relation}|${g.to}|${g.occurrence}`===e.key&&g.provenance.referenceRole===e.role));
 assert.ok(report.suggestions.every(s=>!s.applied&&s.witnessRefs.length>0&&s.preserve.includes('invariants-ownership')));
 checked.push('unique-semantic-neighbors','reverse-closure-oracle','actual-path-witnesses','centrality-advisory','union-arithmetic');
 const task=reportOf(run(['coupling','--symbol','tasks.task','--project',`${fixture}/taskhub`]));assert.deepEqual(task,read(`${fixture}/golden/taskhub.report.json`));
 assert.ok(row(task,'entity:tasks.task').impact.internalSymbols.includes('type:sync.local_request'));
 assert.ok(row(task,'entity:tasks.task').impact.publicContracts.includes('operation:sync.sync_event'));
 assert.equal(row(task,'entity:tasks.task').metrics.sharedMutableResources.value,0);
 const provider=reportOf(run(['coupling','--symbol','provider.manifest','--project',`${fixture}/provider`]));assert.deepEqual(provider,read(`${fixture}/golden/provider.report.json`));assert.equal(row(provider,'type:provider.manifest').metrics.fanInSymbols.value,10);
 checked.push('task-provider-planner','internal-bridge');
 const field=reportOf(run([...args,'--field','state']));const member=row(field,'entity:planner.task#field.state');assert.equal(member.metrics.publicContractsAffected.state,'unknown');assert.ok(member.possible.length>0);assert.deepEqual(member.lowerBounds,{});
 run([...args,'--field','not_a_member'],{exit:1});checked.push('member-fallback-is-not-exact');
 const evidence=read(`${fixture}/golden/planner.evidence.json`),revision=evidence.inputs.sourceRevision.value;
 const attached=reportOf(run([...args,'--evidence',join(root,fixture,'golden/planner.evidence.json'),'--source-revision',revision]));assert.deepEqual(attached,read(`${fixture}/golden/planner.evidence-report.json`));
 assert.equal(row(attached,rootId).metrics.affectedArtifacts.value,1);assert.equal(row(attached,rootId).metrics.affectedTests.value,1);assert.equal(row(attached,rootId).metrics.requiredChecks.value,1);
 run([...args,'--evidence',join(root,fixture,'invalid/stale-input.evidence.json'),'--source-revision',revision],{exit:1});
 const badAttachment=structuredClone(evidence);badAttachment.artifacts.value.digest='sha256:'+'0'.repeat(64);run([...args,'--evidence',temp('bad-attachment.json',badAttachment),'--source-revision',revision],{exit:1});checked.push('attachment-owner-and-current-pins');
 // Thresholds review measured facts; there are no universal numeric defaults.
 for(const [metric,rule,subject] of [['fanInSymbols','fan-exceeded','planner.task'],['publicContractsAffected','public-contract-amplification','planner.task'],['sharedAbstractionRadius','shared-abstraction-radius','planner.due_date'],['affectedArtifacts','change-amplification','planner.task']]) {
  const p=structuredClone(report.profile);p.project.limits=[{metric,maximum:0}];const path=temp(`threshold-${metric}.json`,p);
  const measured=reportOf(run(['coupling','--symbol',subject,'--project',`${fixture}/planner`,'--coupling-profile','advisory','--profiles',path,...(metric==='affectedArtifacts'?['--evidence',join(root,fixture,'golden/planner.evidence.json'),'--source-revision',revision]:[])]));
  assert.ok(measured.findings.some(f=>f.rule===`coupling.${rule}`&&f.metric===metric));
 }
 checked.push('configured-neighbor-threshold','configured-public-radius','shared-abstraction-threshold','artifact-amplification-threshold');
 // Explicit replicated contracts: equal typed shapes are harmless; only an
 // acknowledged obligation makes divergent fields reviewable.
 cpSync(join(root,fixture,'planner'),join(scratch,'replicas'),{recursive:true});
 const replicasFile=join(scratch,'replicas/lekalo/modules/planner/entities.yaml');
 writeFileSync(replicasFile,readFileSync(replicasFile,'utf8')+'\n  - id: planner.replica_a\n    kind: value-object\n    version: 1\n    fields:\n      - name: at\n        type: "planner.due_date"\n  - id: planner.replica_b\n    kind: value-object\n    version: 1\n    fields:\n      - name: at\n        type: "planner.due_date"\n');
 function replicaReport(){
  const own=['coupling','--symbol','planner.replica_a','--project','replicas'];const r=reportOf(run(own,{cwd:scratch}));
  const ev={...structuredClone(evidence),inputs:r.provenance.inputs,artifacts:{state:'unknown'},trace:{state:'unknown'},replicas:[{obligation:'planner.replicated-date',subjects:['type:planner.replica_a','type:planner.replica_b'],reviewRef:'issue:77-replica-probe'}]};
  return reportOf(run([...own,'--evidence',temp('replicas.json',ev)],{cwd:scratch}));
 }
 assert.equal(row(replicaReport(),'type:planner.replica_a').metrics.duplicationDivergence.value,0);
 writeFileSync(replicasFile,readFileSync(replicasFile,'utf8').replace('id: planner.replica_b\n    kind: value-object\n    version: 1\n    fields:\n      - name: at\n        type: "planner.due_date"','id: planner.replica_b\n    kind: value-object\n    version: 1\n    fields:\n      - name: at\n        type: "planner.task_id"'));
 const divergent=replicaReport();assert.equal(row(divergent,'type:planner.replica_a').metrics.duplicationDivergence.value,1);assert.ok(divergent.findings.some(f=>f.rule==='coupling.duplication-divergence'));
 checked.push('explicit-replica-divergence','harmless-typed-duplication');
 const baseBytes=JSON.stringify(report),basePath=temp('baseline.json',baseBytes);
 const comparison=run([...args,'--baseline',basePath]).document.couplingComparison;
 assert.equal(comparison.regressions,0);assert.equal(comparison.rows.find(r=>r.subject===rootId&&r.metric==='affectedTests').state,'incomparable');
 run([...args,'--baseline',join(root,fixture,'invalid/forged-count.report.json')],{exit:1});
 const missingWitness=structuredClone(report);missingWitness.witnesses=[];run([...args,'--baseline',temp('bad-witness.json',missingWitness)],{exit:1});
 const extra=structuredClone(report);extra.subjects[0].metrics.extra={state:'known',value:1};assert.equal(schemas.get('report')(extra),false);run([...args,'--baseline',temp('extra-report.json',extra)],{exit:1});
 const badState=structuredClone(report);badState.subjects[0].metrics.affectedTests={state:'unknown',value:0};assert.equal(schemas.get('report')(badState),false);run([...args,'--baseline',temp('bad-state.json',badState)],{exit:1});
 const profile=structuredClone(report.profile);profile.profileId='strict';profile.gate={mode:'strict',failOn:['baseline-regression'],baselineReadyRef:{state:'known',value:sha(baseBytes)}};profile.project.regressionLimits=[{metric:'fanInSymbols',absoluteIncrease:0,relativeIncrease:{state:'unknown'}}];
 const profilePath=temp('strict.json',profile),selected=['--coupling-profile','strict','--profiles',profilePath];
 const firstRun=run([...args,...selected],{exit:3});assert.equal(firstRun.document.status,'denied');assert.ok(reportOf(firstRun).subjects.length>0);
 cpSync(join(root,fixture,'planner'),join(scratch,'candidate'),{recursive:true});
 const entityPath=join(scratch,'candidate/lekalo/modules/planner/entities.yaml');let text=readFileSync(entityPath,'utf8');assert.ok(text.includes('name: from\n        type: "planner.due_date"'));text=text.replace('name: from\n        type: "planner.due_date"','name: from\n        type: "planner.task"');writeFileSync(entityPath,text);
 const candidateArgs=['coupling','--symbol','planner.task','--project','candidate',...selected,'--baseline',basePath];
 const before=files(join(scratch,'candidate'));const denied=run(candidateArgs,{cwd:scratch,exit:3});assert.deepEqual(files(join(scratch,'candidate')),before);
 assert.ok(denied.document.reasonCodes.includes('coupling.policy-denied'));assert.equal(row(reportOf(denied),rootId).metrics.fanInSymbols.value,7);assert.ok(denied.document.payload.couplingComparison.regressions>0);
 profile.project.regressionLimits[0].absoluteIncrease=1;writeFileSync(profilePath,JSON.stringify(profile));assert.equal(run(candidateArgs,{cwd:scratch}).document.status,'valid');
 profile.project.regressionLimits[0].metric='affectedTests';writeFileSync(profilePath,JSON.stringify(profile));run(candidateArgs,{cwd:scratch,exit:3});
 checked.push('strict-baseline-regression','equality-passes','unknown-required-denies','denial-retains-report','read-only-producer');
 const unknownProfile=read(`${fixture}/invalid/unknown-member.profile.json`);run([...args,'--coupling-profile','advisory','--profiles',temp('unknown-profile.json',unknownProfile)],{exit:1});
 const duplicated=JSON.stringify(report.profile).replace('"profileId":"advisory"','"profileId":"advisory","profileId":"advisory"');run([...args,'--coupling-profile','advisory','--profiles',temp('duplicate.json',duplicated)],{exit:1});
 const future=structuredClone(report.profile);future.schemaVersion='lekalo/coupling-profile/v99.0.0';run([...args,'--coupling-profile','advisory','--profiles',temp('future.json',future)],{exit:5});
 const changed=run(['coupling','--changed-input',join(root,fixture,'golden/planner.change-input.json'),'--project',`${fixture}/planner`]);assert.equal(reportOf(changed).scope.kind,'changed');
 run(['coupling','--changed-input',join(root,fixture,'invalid/traversal.change-input.json'),'--project',`${fixture}/planner`],{exit:1});
 checked.push('duplicate-keys-fail','unsupported-version','typed-change-input','closed-schemas');
 cpSync(join(root,fixture,'planner'),join(scratch,'formatting'),{recursive:true});
 for(const [name]of files(join(scratch,'formatting')))if(name.endsWith('.yaml')){const p=join(scratch,'formatting',name);writeFileSync(p,'# whitespace control\n\n'+readFileSync(p,'utf8').replaceAll('\n','\r\n'));}
 const reformatted=reportOf(run(['coupling','--symbol','planner.task','--project','formatting'],{cwd:scratch}));assert.deepEqual(reformatted,report);checked.push('formatting-invariant');
 cpSync(join(root,fixture,'planner'),join(scratch,'split'),{recursive:true});
 const splitFile=join(scratch,'split/lekalo/modules/planner/entities.yaml'),original=readFileSync(splitFile,'utf8'),parts=original.split(/(?=^  - id:)/m),header=parts.shift();assert.ok(parts.length>1);
 const secondFile=join(dirname(splitFile),'commands.yaml');
 writeFileSync(splitFile,header+parts.slice(0,2).join(''));writeFileSync(secondFile,readFileSync(secondFile,'utf8')+'\n'+parts.slice(2).join(''));
 const split=run(['coupling','--symbol','planner.task','--project','split'],{cwd:scratch,exit:1});assert.ok(split.document.reasonCodes.includes('ir.kind-placement'));checked.push('canonical-placement-remains-enforced');
 const planned=reportOf(run([...args,'--context-budget','1']));assert.equal(planned.planning.contextBudget.state,'known');assert.equal(JSON.parse(planned.planning.contextBudget.value).identity,'dev.lekalo.context-budget-report@0.6.3');
 checked.push('context-plan-required-facts');
 const absent=spawnSync(process.execPath,[fileURLToPath(import.meta.url)],{cwd:root,encoding:'utf8',env:{...process.env,LEKALO_COUPLING_BIN:join(scratch,'absent')}});assert.notEqual(absent.status,0);assert.match(absent.stderr,/binary-missing/);checked.push('binary-missing-fails-closed');
 process.stdout.write(JSON.stringify({ok:true,gate:'coupling-contracts',ajv:'8.17.1',schemas:5,checks:checked.length,checked})+'\n');
}catch(error){process.stderr.write(JSON.stringify({ok:false,gate:'coupling-contracts',reason:'assertion',detail:error.message})+'\n');process.exitCode=1;}
finally {
 const target=realpathSync.native(scratch),parent=realpathSync.native(tmpdir());
 assert.equal(dirname(target),parent);assert.ok(basename(target).startsWith('lekalo-coupling-gate-'));
 rmSync(target,{recursive:true,force:true});
}
