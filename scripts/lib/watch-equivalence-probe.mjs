// Independent settled-tree oracle for every selected watch stage. All
// subprocess output stays in memory: an untracked output file inside the
// project would itself contaminate `impact --changed --worktree`.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,renameSync,rmSync,unlinkSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {StringDecoder} from 'node:string_decoder';
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const stable=value=>Array.isArray(value)?value.map(stable):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,stable(value[key])])):value;
const diagnostics=rows=>rows.map(row=>JSON.stringify(stable(row))).filter((row,i,all)=>all.indexOf(row)===i).sort();
function inventory(dir,prefix='') {
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).filter(entry=>entry.name!=='.git').flatMap(entry=>{
  assert(entry.isFile()||entry.isDirectory());const path=prefix+entry.name;
  return entry.isDirectory()?inventory(join(dir,entry.name),path+'/'):[[path,readFileSync(join(dir,entry.name)).toString('hex')]];
 });
}
function invoke(program,args,project,env) {
 const result=spawnSync(program,args,{cwd:project,env,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
 assert.ifError(result.error);assert.equal(result.signal,null,result.stderr);return result;
}
function clean(binary,project,args,env) {
 const result=invoke(binary,['--json','--no-cache',...args],project,env);
 const owned=result.stdout.trim()||result.stderr.trim();assert(owned,`${args}: missing status-owned JSON`);
 return {exit:result.status,...JSON.parse(owned)};
}
function git(project,env,args) {
 const result=invoke('git',['-c','core.autocrlf=false','-c','user.name=Watch fixture','-c','user.email=watch-fixture@example.invalid',...args],project,env);
 assert.equal(result.status,0,result.stderr);return result.stdout.trim();
}
function commit(project,env) {
 git(project,env,['add','-A']);git(project,env,['commit','--allow-empty','-qm','captured oracle baseline']);
}
function assertDrift(actual,expected) {
 if(expected.status!=='valid') { assert.equal(actual,undefined,'failed check cannot retain an old receipt');return; }
 assert.deepEqual(actual,{
  verdict:expected.verdict,clean:expected.counts.clean,stale:expected.counts.stale,
  manualDrift:expected.counts.manualDrift,missing:expected.counts.missing,orphan:expected.counts.orphan,
  reported:expected.counts.reported,findings:expected.findings.length,
 });
}
function assertImpact(actual,expected,previous,current) {
 assert.equal(expected.status,'valid');assert(actual,'impact result must be present');
 const oracle=structuredClone(expected.impact);
 // Verify the independent CLI's canonical digest, then bind its same
 // base/candidate content pair to watch's source-set revision namespace.
 // No domain sections are dropped: the ENTIRE rebound payload and its
 // recomputed digest must equal watch, including completeness/evidence.
 const original=oracle.digest;oracle.digest='';assert.equal(hash(JSON.stringify(oracle)),original);
 assert.equal(oracle.input.baseRevisionRef,null);assert.equal(oracle.input.candidateRevisionRef,null);
 oracle.input.baseRevisionRef=previous.sourceRevision.slice(7);
 oracle.input.candidateRevisionRef=current.sourceRevision.slice(7);
 oracle.digest=hash(JSON.stringify(oracle));
 assert.deepEqual(actual,oracle,'all impact fields and rebound digest equal the clean command');
}
async function trace({binary,project,env,flags,edits,check}) {
 const events=[],completed=[],decoder=new StringDecoder('utf8');let pending='',stderr='',failure;
 const child=spawn(binary,['watch','--backend','poll','--poll-interval-ms','100','--reconcile-ms','100','--debounce-ms','30','--max-wait-ms','150','--json-lines','--stop-after-runs',String(edits.length+1),...flags],{cwd:project,env,stdio:['ignore','pipe','pipe']});
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>{child.kill();reject(new Error(`equivalence trace deadline: ${stderr}`));},60000);
  child.on('error',error=>{clearTimeout(timer);reject(error);});
  child.stderr.on('data',data=>{stderr+=data;if(stderr.length>1024*1024){failure=new Error('stderr bound');child.kill();}});
  child.stdout.on('data',data=>{
   try {
    pending+=decoder.write(data);assert(pending.length<=16*1024*1024,'bounded stream buffer');
    for(let newline;(newline=pending.indexOf('\n'))>=0;){
     const event=JSON.parse(pending.slice(0,newline));pending=pending.slice(newline+1);
     assert.equal(event.sequence,events.length+1);events.push(event);
     if(event.event!=='run-completed')continue;
     assert.equal(event.diagnosticsMode,'replace');
     const before=inventory(project);
     const semanticBase=completed.find(e=>e.sourceRevision===event.result.previousSourceRevision);
     check(event,semanticBase,completed.length);
     assert.deepEqual(inventory(project),before,'watch and clean --no-cache commands must not write');
     completed.push(event);
     // Parse-invalid snapshots are observable replacements, but cannot
     // become the semantic/impact baseline. Keep Git pinned to the same
     // last parse-valid tree watch retains, including during recovery.
     if(event.result.affected.reason!=='parse-invalid')commit(project,env);
     if(completed.length<=edits.length)edits[completed.length-1](project);
    }
   } catch(error){failure=error;child.kill();}
  });
  child.on('close',(code,signal)=>{
   clearTimeout(timer);
   try{if(failure)throw failure;assert.equal(signal,null);assert.equal(code,completed.at(-1).status==='valid'?0:completed.at(-1).status==='unsupported-version'?5:1,stderr);assert.equal(pending,'');resolve();}catch(error){reject(error);}
  });
 });
 assert.equal(completed.length,edits.length+1);
 for(const started of events.filter(e=>e.event==='run-started'))assert.equal(events.filter(e=>['run-cancelled','run-completed'].includes(e.event)&&e.runId===started.runId).length,1,'exactly one terminal per start');
 return completed.length;
}
export async function multiStageEquivalence({root,binary}) {
 const env={...process.env};delete env.LEKALO_PROJECT;let comparisons=0,settled=0;
 for(const fixture of ['project','project-successor']) {
 for(const scenario of ['semantic-and-format','parse-recovery','generated-and-lock']) {
  const project=mkdtempSync(join(tmpdir(),'lekalo-watch-stages-'));
  try {
   cpSync(join(root,'tests/fixtures/watch',fixture),project,{recursive:true});
   assert.equal(clean(binary,project,['lock'],env).status,'valid');
   git(project,env,['init','-q']);commit(project,env);
   const source=join(project,'lekalo/modules/planner/entities.yaml');
   const original=readFileSync(source,'utf8');
   const generated=join(project,'.lekalo/generated/probe.ts');
   const lock=readFileSync(join(project,'lekalo.lock'));
   const edits=scenario==='semantic-and-format'?[
    ()=>writeFileSync(source,original.replace('version: 1','version: 2')),
    ()=>writeFileSync(source,readFileSync(source,'utf8')+'\n'),
    ()=>{const temp=source+'.tmp';writeFileSync(temp,readFileSync(source,'utf8').replace('version: 2','version: 3'));renameSync(temp,source);},
   ]:scenario==='parse-recovery'?[
    ()=>writeFileSync(source,'schema_version: [\n'),
    ()=>writeFileSync(source,original.replace('version: 1','version: 4')),
   ]:[
    ()=>{mkdirSync(join(project,'.lekalo/generated'),{recursive:true});writeFileSync(generated,'// synthetic orphan\n');},
    ()=>unlinkSync(generated),
    ()=>writeFileSync(join(project,'lekalo.lock'),'invalid lock\n'),
    ()=>writeFileSync(join(project,'lekalo.lock'),lock),
   ];
   const flags=scenario==='generated-and-lock'?['--generate-check']:['--validate','--impact','--generate-check'];
   settled+=await trace({binary,project,env,flags,edits,check:(event,previous)=>{
    const drift=clean(binary,project,['generate','--check','--locked'],env);comparisons++;
    assertDrift(event.result.drift,drift);
    const validation=scenario==='generated-and-lock'?{status:'valid',diagnostics:[]}:clean(binary,project,['validate'],env);
    if(scenario!=='generated-and-lock')comparisons++;
    assert.deepEqual(diagnostics(event.result.diagnostics),diagnostics([...(drift.diagnostics??[]),...(validation.diagnostics??[])]),'complete diagnostic replacement equals all clean selected stages');
    assert.equal(event.status,drift.status==='valid'?validation.status:drift.status);
    if(previous&&scenario!=='generated-and-lock'&&validation.status==='valid'){
     const impact=clean(binary,project,['impact','--changed','--worktree'],env);comparisons++;
     assertImpact(event.result.impact,impact,previous,event);
     assert.equal(event.result.stages.find(s=>s.stage==='impact').coverage,'complete');
    }
   }});
  }finally{rmSync(project,{recursive:true,force:true});}
 }
 }
 return {comparisons,settled,modelVersions:['0.2.16','0.8.0']};
}
