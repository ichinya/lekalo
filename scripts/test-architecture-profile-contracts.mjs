// All issue #84 families: exact schemas + committed goldens + independent
// expectations + fresh built-binary behavior. Acceptance is read-only.
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,existsSync,mkdtempSync,cpSync,mkdirSync,rmSync,rmdirSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {spawnSync} from 'node:child_process';
import {root,version,header,hash,digest,known,unknown,generate,shapes} from './gen-architecture-profile-contracts.mjs';
const write=process.argv.length===3&&process.argv[2]==='--write-goldens';
assert.ok(process.argv.length===2||write,'usage: node scripts/test-architecture-profile-contracts.mjs [--write-goldens]');
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH||root+'/scripts')+'/');
assert.equal(require('ajv/package.json').version,'8.17.1','exact Ajv gate dependency');
const Ajv=require('ajv/dist/2020').default;
const ajv=new Ajv({strict:true,allErrors:true});
const read=p=>JSON.parse(readFileSync(join(root,p),'utf8'));
const pretty=v=>JSON.stringify(v,null,2)+'\n';
const schemas=new Map(Object.keys(shapes).map(f=>[f,ajv.compile(read(`contracts/${f}.schema.v${version}.json`))]));
function schema(f,v,valid=true){const check=schemas.get(f);assert.equal(check(v),valid,`${f}: ${JSON.stringify(check.errors)}`);}
const bin=join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
assert.ok(existsSync(bin),'binary-missing: cargo build --workspace --locked must precede gate');
assert.equal(generate(true),11);
const old=read('contracts/diagnostic-registry.v0.6.4.json');
const registry=read(`contracts/diagnostic-registry.v${version}.json`);
assert.equal(registry.entries.length,508);assert.equal(old.entries.length,500);
for(const e of old.entries)assert.deepEqual(registry.entries.find(n=>n.id===e.id),e,'predecessor entry changed');
assert.equal(new Set(registry.entries.map(e=>e.code)).size,508);
const registrySchema=ajv.compile(read(`contracts/diagnostic-registry.schema.v${version}.json`));
assert.ok(registrySchema(registry),JSON.stringify(registrySchema.errors));
const provenance=read('tests/fixtures/fixture-provenance.json');
assert.equal(provenance.families.filter(f=>f.family==='architecture-profile'&&f.origin==='synthetic').length,1);
const fixture='tests/fixtures/architecture-profile';
const project=mkdtempSync(join(tmpdir(),'lekalo-issue-84-'));
const emitted=new Set();const goldenNames=[];let probes=0;
function file(name,value){const path=join(project,name);writeFileSync(path,typeof value==='string'?value:pretty(value));return path;}
function invoke(args,status='valid',cwd=project){
 const child=spawnSync(bin,['--json','--no-cache','architecture-profile',...args],{cwd,encoding:'utf8',timeout:60000,maxBuffer:32*1024*1024});
 assert.ifError(child.error);const text=child.stdout.trim()||child.stderr.trim();
 let envelope;try{envelope=JSON.parse(text);}catch{throw Error(`invalid output exit ${child.status}: ${text.slice(0,1800)}`);}
 assert.equal(envelope.status,status,text.slice(0,1600));assert.equal(child.status,({valid:0,invalid:1,denied:3,'unsupported-version':5})[status]);
 if(status==='invalid'||status==='unsupported-version')assert.equal(child.stdout,'');else assert.equal(child.stderr,'');
 for(const d of envelope.diagnostics??[]){if(d.id.startsWith('architecture-profile.')){emitted.add(d.id);assert.equal(d.registry_version,version);assert.ok(registry.entries.some(e=>e.id===d.id&&e.code===d.code));}}
 probes++;return {envelope,value:envelope.architectureProfile??envelope.payload?.architectureProfile,raw:text};
}
function gold(name,family,value){
 schema(family,value);goldenNames.push({name,family});
 const path=join(root,fixture,'golden',name+'.json');
 if(write)writeFileSync(path,pretty(value));else assert.equal(readFileSync(path,'utf8'),pretty(value),`golden drift ${name}`);
}
function rejected(doc,id='architecture-profile.input-invalid',command='resolve',extra=[]){
 const path=file('bad.json',doc);const args=command==='resolve'?['resolve','--architecture-profile','ai-strict']:['assess','--all'];
 const result=invoke([...args,'--architecture-profiles',path,...extra],id==='architecture-profile.version-unsupported'?'unsupported-version':'invalid');
 assert.ok(result.envelope.reasonCodes.includes(id),`expected ${id}: ${result.raw}`);
}
try{
 cpSync(join(root,'tests/fixtures/context-budget/planner'),project,{recursive:true});
 if(write)mkdirSync(join(root,fixture,'golden'),{recursive:true});
 const catalog=invoke(['catalog']).value;
 assert.equal(catalog.rules.length,15);assert.equal(catalog.rules.filter(r=>r.mandatory).length,1);
 assert.equal(catalog.registryRef.digest,digest(readFileSync(join(root,`contracts/diagnostic-registry.v${version}.json`))));
 assert.ok(catalog.rules.every(r=>r.owner==='lekalo-core'&&r.rationale&&r.alternative));
 assert.ok(!JSON.stringify(catalog).match(/\bPHP\b|Laravel|Mago/),'core has no target-specific rule coupling');
 gold('catalog','architecture-rule-catalog',catalog);
 const doc=read(`contracts/architecture-profile.v${version}.json`);
 assert.deepEqual(doc.profiles.map(p=>p.id),['ai-strict','contracted-standard','legacy-observed','managed-generated']);
 gold('profiles','architecture-profile',doc);
 const resolved=invoke(['resolve','--architecture-profile','ai-strict']).value;
 assert.deepEqual(resolved.chain.map(p=>p.id),['legacy-observed','contracted-standard','ai-strict']);
 assert.deepEqual(resolved.rules.filter(r=>r.required).map(r=>r.id),['architecture.bounded-context','architecture.explicit-dependencies','architecture.stable-semantic-ids']);
 gold('resolved','architecture-profile-resolved',resolved);
 const lock=invoke(['lock']).value;gold('lock','architecture-profile-lock',lock);
 const lockPath=file('lock.json',lock);
 const strictArgs=['assess','--all','--architecture-profile','ai-strict','--check'];
 const strict=invoke([...strictArgs,'--architecture-lock',lockPath]).value;
 assert.equal(strict.assessment,'enforced-core');assert.equal(strict.rows.length,30);
 assert.deepEqual([...new Set(strict.rows.map(r=>r.module))],['notify','planner']);
 const plannerDeps=strict.rows.find(r=>r.module==='planner'&&r.measurement==='fanOutModules');
 assert.deepEqual(plannerDeps.value,known(1));assert.deepEqual(plannerDeps.witnesses,['notify']);
 assert.deepEqual(strict.rows.find(r=>r.module==='notify'&&r.measurement==='fanOutModules').value,known(0));
 const tokens=strict.rows.filter(r=>r.measurement==='minimumRequiredSemanticTokens');assert.ok(tokens.every(r=>r.coverage==='complete'&&r.value.value>0));
 // Independent producer agreement; architecture does not define a new token recipe.
 const budgetRun=spawnSync(bin,['--json','--no-cache','context-budget','--module','planner','--budget','1000000'],{cwd:project,encoding:'utf8',timeout:60000,maxBuffer:16*1024*1024});assert.ifError(budgetRun.error);assert.equal(budgetRun.status,0);
 const budget=JSON.parse(budgetRun.stdout);const report=budget.contextBudget??budget.report??budget;
 const serialized=JSON.stringify(report);assert.ok(serialized.includes('minimumRequiredSemanticTokens'));
 // Named row maxima are asserted below against the exact existing producer projection.
 const walk=v=>v&&typeof v==='object'?[...(v.minimumRequiredSemanticTokens?.state==='known'?[v.minimumRequiredSemanticTokens.value]:[]),...Object.values(v).flatMap(walk)]:[];
 const minima=walk(report);assert.ok(minima.length);assert.equal(tokens.find(r=>r.module==='planner').value.value,Math.max(...minima));
 gold('report','architecture-profile-report',strict);
 assert.equal(invoke(strictArgs).raw,invoke(strictArgs).raw,'repeated CLI bytes');
 const legacy=invoke(['assess','--all']).value;assert.ok(legacy.rows.filter(r=>r.ruleId!=='architecture.stable-semantic-ids').every(r=>r.coverage==='disabled'));
 const standard=invoke(['assess','--module','planner','--architecture-profile','contracted-standard','--check']);assert.equal(standard.value.rows.length,15);assert.ok(standard.envelope.reasonCodes.includes('architecture-profile.evidence-incomplete'));
 const managed=invoke(['assess','--all','--architecture-profile','managed-generated','--check'],'denied').value;
 assert.ok(managed.rows.some(r=>r.ruleId==='architecture.deterministic-generation'&&r.disposition==='evidence-gap'&&r.coverage==='unsupported'));
 // Explicit test-only root: all rule selections complete and pinned, one measured maximum.
 const measured=structuredClone(doc);measured.profiles.push({id:'measured',version:'1',extends:unknown(),rules:structuredClone(resolved.rules)});measured.profiles.sort((a,b)=>a.id<b.id?-1:1);
 const measurement=measured.profiles.find(p=>p.id==='measured').rules.find(r=>r.id==='architecture.explicit-dependencies');measurement.limit=known({maximum:1,calibrationRef:'synthetic/fan-out-equality'});
 const equalPath=file('equal.json',measured);invoke(['assess','--all','--architecture-profile','measured','--architecture-profiles',equalPath,'--check']);
 measurement.limit.value.maximum=0;
 const measuredPath=file('measured.json',measured);const measuredArgs=['assess','--all','--architecture-profile','measured','--architecture-profiles',measuredPath];
 const violated=invoke([...measuredArgs,'--check'],'denied').value;
 assert.equal(violated.modelRef,strict.modelRef);assert.equal(violated.irRef,strict.irRef,'policy never changes Model semantics');
 assert.equal(violated.rows.filter(r=>r.disposition==='violation').length,1);
 const debt=violated.rows.find(r=>r.disposition==='violation');assert.equal(debt.severity,'warning');assert.ok(debt.rationale&&debt.alternative);assert.deepEqual(debt.witnesses,['notify']);
 gold('violation','architecture-profile-report',violated);
 const diff=invoke(['diff','--base-profiles',file('base.json',doc),'--candidate-profiles',measuredPath]).value;
 assert.ok(diff.changes.some(v=>v.member==='membership'));gold('diff','architecture-profile-diff',diff);
 const noDiff=invoke(['diff','--base-profiles',equalPath,'--candidate-profiles',measuredPath]).value;
 assert.ok(noDiff.changes.some(v=>v.member==='limit'&&v.strength==='strengthened'));
 const baseline=file('baseline.json',violated);const ledger={...header('architecture-adoption'),snapshotRef:violated.snapshotRef,baselineRef:hash(violated),entries:[{ruleId:debt.ruleId,module:debt.module,conditionDigest:debt.conditionDigest,owner:'synthetic-owner',reason:'Adopt existing synthetic module fan-out while retaining its exact condition.',reviewRef:'synthetic/review-84',expiresOn:'2026-10-05'}]};
 gold('adoption','architecture-adoption',ledger);
 const ledgerPath=file('adoption.json',ledger);const adoptArgs=[...measuredArgs,'--check','--baseline',baseline,'--adoption',ledgerPath,'--as-of','2026-10-04'];
 const adopted=invoke(adoptArgs).value;assert.equal(adopted.assessment,'adopting');assert.deepEqual(adopted.rows.find(r=>r.disposition==='adopted').value,debt.value);
 gold('adopted','architecture-profile-report',adopted);
 invoke([...measuredArgs,'--check','--baseline',baseline],'denied');
 invoke([...adoptArgs.slice(0,-1),'2026-10-06'],'denied');
 // A comparable module baseline never adopts increased measured debt.
 const moduleArgs=['assess','--module','planner','--architecture-profile','measured','--architecture-profiles',measuredPath];
 const moduleBaseline=invoke([...moduleArgs,'--check'],'denied').value;
 const moduleLedger={...ledger,baselineRef:hash(moduleBaseline)};
 const moduleAdopt=[...moduleArgs,'--check','--baseline',file('module-baseline.json',moduleBaseline),'--adoption',file('module-adoption.json',moduleLedger),'--as-of','2026-10-04'];
 assert.equal(invoke(moduleAdopt).value.assessment,'adopting');
 const importsPath=join(project,'lekalo/modules/planner/module.yaml'),savedImports=readFileSync(importsPath);
 const extraModule=join(project,'lekalo/modules/alerts');mkdirSync(extraModule);
 try{
  writeFileSync(join(extraModule,'module.yaml'),'schema_version: "0.2.16"\ndefinitions:\n  - id: alerts\n    kind: module\n    version: 1\n');
  writeFileSync(importsPath,savedImports.toString().replace('      - notify','      - notify\n      - alerts'));
  const worse=invoke(moduleAdopt,'denied').value;
  assert.equal(worse.rows.find(r=>r.ruleId==='architecture.explicit-dependencies').value.value,2);
  assert.notEqual(worse.rows.find(r=>r.ruleId==='architecture.explicit-dependencies').conditionDigest,debt.conditionDigest);
 }finally{writeFileSync(importsPath,savedImports);rmSync(join(extraModule,'module.yaml'));rmdirSync(extraModule);}
 const frozen=readFileSync(baseline),frozenLedger=readFileSync(ledgerPath);
 const brokenLedger=structuredClone(ledger);brokenLedger.entries[0].conditionDigest='sha256:'+'0'.repeat(64);
 const invalidAdopt=invoke([...adoptArgs.map(x=>x===ledgerPath?file('broken-adoption.json',brokenLedger):x)],'invalid');assert.ok(invalidAdopt.envelope.reasonCodes.includes('architecture-profile.adoption-invalid'));
 assert.deepEqual(readFileSync(baseline),frozen);assert.deepEqual(readFileSync(ledgerPath),frozenLedger);
 // Stale policy/scope, forged coverage, numbers, profile selections and omitted rows.
 const otherScope=structuredClone(violated);otherScope.scope.id='another';
 const incomparable=invoke([...measuredArgs,'--baseline',file('other-scope.json',otherScope)],'invalid');assert.ok(incomparable.envelope.reasonCodes.includes('architecture-profile.baseline-incomparable'));
 for(const mutate of [r=>r.rows.pop(),r=>r.rows.find(v=>v.coverage==='unsupported').coverage='complete',r=>r.rows[0].conditionDigest='sha256:'+'0'.repeat(64),r=>r.profiles[0].rules.pop(),r=>r.rows.find(v=>v.disposition==='violation').disposition='observed']){
  const b=structuredClone(violated);mutate(b);invoke([...measuredArgs,'--baseline',file('forged.json',b)],'invalid');
 }
 // Project and exact module opt-in, including conflict and foreign-assignment rejection.
 const scopes=structuredClone(doc);scopes.assignments=[{scope:{kind:'module',id:'planner'},profileRef:resolved.profileRef}];
 const scoped=invoke(['assess','--all','--architecture-profiles',file('scopes.json',scopes)]).value;
 assert.equal(scoped.rows.find(r=>r.module==='planner'&&r.measurement==='fanOutModules').coverage,'complete');assert.equal(scoped.rows.find(r=>r.module==='notify'&&r.measurement==='fanOutModules').coverage,'disabled');
 const projectScope=structuredClone(doc);projectScope.assignments=[{scope:{kind:'project',id:'planner'},profileRef:resolved.profileRef}];
 const projectPath=file('project-scope.json',projectScope);assert.equal(invoke(['assess','--all','--architecture-profiles',projectPath,'--check']).value.profiles[0].profileRef.id,'ai-strict');
 invoke(['assess','--all','--architecture-profiles',projectPath,'--architecture-profile','legacy-observed'],'invalid');
 scopes.assignments[0].scope.id='foreign';rejected(scopes,'architecture-profile.input-invalid','assess');
 scopes.assignments[0].scope.id='planner';scopes.assignments[0].profileRef=lock.profiles.find(p=>p.profileRef.id==='legacy-observed').profileRef;
 rejected(scopes,'architecture-profile.weakening-unacknowledged','assess',['--architecture-profile','ai-strict']);
 for(const kind of ['path','recursive','project:*']){const d=structuredClone(projectScope);d.assignments[0].scope.kind=kind;rejected(d);schema('architecture-profile',d,false);}
 const wildcard=structuredClone(projectScope);wildcard.assignments[0].scope.id='*';rejected(wildcard);schema('architecture-profile',wildcard,false);
 const badLock=structuredClone(lock);badLock.profiles[0].rules.pop();invoke([...strictArgs,'--architecture-lock',file('bad-lock.json',badLock)],'invalid');
 // Every weakening dimension, cycles, foreign catalog, duplicate keys, versions and nested closure.
 for(const alter of [r=>r.enabled=false,r=>r.required=false,r=>r.severity='info']){const d=structuredClone(doc);const p=d.profiles.find(p=>p.id==='contracted-standard');p.rules.push({...resolved.rules.find(r=>r.id==='architecture.stable-semantic-ids')});alter(p.rules.at(-1));p.rules.sort((a,b)=>a.id<b.id?-1:1);rejected(d);}
 const weaker=structuredClone(doc);weaker.profiles.find(p=>p.id==='ai-strict').rules[0].enabled=false;weaker.profiles.find(p=>p.id==='ai-strict').rules[0].required=false;rejected(weaker,'architecture-profile.weakening-unacknowledged');
 const cycle=structuredClone(doc);cycle.profiles.find(p=>p.id==='legacy-observed').extends=known(resolved.profileRef);rejected(cycle,'architecture-profile.inheritance-invalid');
 const missing=structuredClone(doc);missing.profiles[0].extends.value.id='missing';rejected(missing,'architecture-profile.inheritance-invalid');
 const versioned=structuredClone(doc);versioned.schemaVersion='lekalo/architecture-profile/v99.0.0';rejected(versioned,'architecture-profile.version-unsupported');
 const foreign=structuredClone(doc);foreign.catalogRef.digest='sha256:'+'0'.repeat(64);rejected(foreign);
 const extension=structuredClone(doc);extension.profiles[0].rules[0].extension=true;rejected(extension);schema('architecture-profile',extension,false);
 rejected(pretty(doc).replace('"assignments": []','"assignments": [], "assignments": []'));
 for(const badLimit of [known({maximum:-1,calibrationRef:'fixture'}),known({maximum:0,calibrationRef:''}),{state:'known'}]){const d=structuredClone(measured);d.profiles.find(p=>p.id==='measured').rules.find(r=>r.id==='architecture.explicit-dependencies').limit=badLimit;rejected(d);schema('architecture-profile',d,false);}
 const subjective=structuredClone(measured);subjective.profiles.find(p=>p.id==='measured').rules.find(r=>r.id==='architecture.justified-abstractions').limit=known({maximum:0,calibrationRef:'fixture/style'});rejected(subjective);
 for(const limit of [unknown(),known({maximum:1,calibrationRef:'fixture/relax'})]){
  const d=structuredClone(measured),parent=d.profiles.find(p=>p.id==='measured');
  d.profiles.push({id:'child-measured',version:'1',extends:known({id:parent.id,version:'1',digest:hash(parent.rules)}),rules:[{...measurement,limit}]});d.profiles.sort((a,b)=>a.id<b.id?-1:1);
  rejected(d,'architecture-profile.weakening-unacknowledged');
 }
 for(const alter of [r=>r.severity='info',r=>r.required=false,r=>{r.enabled=false;r.required=false;r.limit=unknown();}]){
  const d=structuredClone(measured),parent=d.profiles.find(p=>p.id==='measured'),childRule=structuredClone(parent.rules.find(r=>r.id==='architecture.explicit-dependencies'));alter(childRule);
  d.profiles.push({id:'child-measured',version:'1',extends:known({id:parent.id,version:'1',digest:hash(parent.rules)}),rules:[childRule]});d.profiles.sort((a,b)=>a.id<b.id?-1:1);
  rejected(d,'architecture-profile.weakening-unacknowledged');
 }
 const noCalibration=structuredClone(measured);delete noCalibration.profiles.find(p=>p.id==='measured').rules.find(r=>r.id==='architecture.explicit-dependencies').limit.value.calibrationRef;rejected(noCalibration);schema('architecture-profile',noCalibration,false);
 invoke(['resolve','--architecture-profile','ai-strict','--profile-version','2'],'unsupported-version');
 invoke(['assess','--module','unknown'],'invalid');
 const badDate=invoke([...adoptArgs.slice(0,-1),'2026-02-30'],'invalid');assert.ok(badDate.envelope.reasonCodes.includes('architecture-profile.adoption-invalid'));
 const emptyInput=invoke(['resolve','--architecture-profile','ai-strict','--architecture-profiles',file('empty.json','')],'invalid');assert.ok(emptyInput.envelope.reasonCodes.includes('architecture-profile.input-invalid'));
 const huge=invoke(['resolve','--architecture-profile','ai-strict','--architecture-profiles',file('huge.json',' '.repeat(8*1024*1024+1))],'invalid');assert.ok(huge.envelope.reasonCodes.includes('architecture-profile.input-invalid'));
 // No regression to mandatory semantic errors across a selected module boundary.
 const badModel=join(project,'lekalo/modules/notify/commands.yaml');const saved=readFileSync(badModel);
 writeFileSync(badModel,saved.toString().replace('notify.rename_user"','notify.missing_effect"'));
 const semantic=invoke(['assess','--module','planner','--architecture-profile','legacy-observed'],'invalid');assert.ok(semantic.envelope.reasonCodes.some(id=>!id.startsWith('architecture-profile.')));writeFileSync(badModel,saved);
 // All schemas fail closed at root; each family has live output and a committed golden.
 for(const {name,family} of goldenNames){const v=read(`${fixture}/golden/${name}.json`);schema(family,v);schema(family,{...v,extension:true},false);}
 assert.deepEqual([...new Set(goldenNames.map(r=>r.family))].sort(),Object.keys(shapes).sort());
 assert.deepEqual([...emitted].sort(),registry.entries.filter(e=>e.id.startsWith('architecture-profile.')).map(e=>e.id));
 // Fault probes never remove a real artifact: isolated copies must fail closed.
 const isolated=mkdtempSync(join(tmpdir(),'lekalo-84-gate-fault-'));
 try{
  mkdirSync(join(isolated,'scripts'),{recursive:true});mkdirSync(join(isolated,'contracts'));
  for(const name of ['gen-architecture-profile-contracts.mjs','test-architecture-profile-contracts.mjs'])cpSync(join(root,'scripts',name),join(isolated,'scripts',name));
  for(const path of [...(await import('./gen-architecture-profile-contracts.mjs')).artifacts().keys(),'contracts/diagnostic-registry.v0.6.4.json','contracts/diagnostic-registry.schema.v0.6.4.json'])cpSync(join(root,path),join(isolated,path));
  const fault=(text)=>{const result=spawnSync(process.execPath,[join(isolated,'scripts/test-architecture-profile-contracts.mjs')],{encoding:'utf8',timeout:60000,env:process.env});assert.ifError(result.error);assert.notEqual(result.status,0);assert.ok(result.stderr.includes(text),result.stderr.slice(0,1000));};
  fault('binary-missing');
  const missingSchema=`contracts/architecture-profile-report.schema.v${version}.json`;rmSync(join(isolated,missingSchema));fault('ENOENT');cpSync(join(root,missingSchema),join(isolated,missingSchema));
  // Placeholder is never invoked: both following faults must precede CLI execution.
  mkdirSync(join(isolated,'target/debug'),{recursive:true});writeFileSync(join(isolated,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo'),'synthetic preflight placeholder');
  const missingCatalog=`contracts/architecture-rule-catalog.v${version}.json`;rmSync(join(isolated,missingCatalog));fault('ENOENT');cpSync(join(root,missingCatalog),join(isolated,missingCatalog));
  fault('fixture-provenance.json');
 }finally{assert.ok(isolated.startsWith(join(tmpdir(),'lekalo-84-gate-fault-')));rmSync(isolated,{recursive:true,force:true});}
 if(write)writeFileSync(join(root,fixture,'golden/index.json'),pretty(goldenNames));else assert.deepEqual(read(`${fixture}/golden/index.json`),goldenNames);
 console.log(JSON.stringify({ok:true,product:version,ajv:'8.17.1',families:schemas.size,goldens:goldenNames.length,registryEntries:registry.entries.length,diagnostics:emitted.size,liveProbes:probes,artifactFaults:4,mode:write?'authoring':'read-only'}));
}finally{
 // One native shell/process owns this fully resolved, freshly created temporary root.
 assert.ok(project.startsWith(join(tmpdir(),'lekalo-issue-84-')));
 rmSync(project,{recursive:true,force:true});
}
