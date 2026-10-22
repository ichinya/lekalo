#!/usr/bin/env node
// A real long-lived CLI session: adapter crash -> invalid result -> stale
// cancellation -> successful recovery, never a fresh service per fault.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {StringDecoder} from 'node:string_decoder';
import {selectiveTargetProbe} from './lib/watch-target-probe.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={...process.env};delete env.LEKALO_PROJECT;
const project=mkdtempSync(join(tmpdir(),'lekalo-watch-faults-'));
function inventory(dir,prefix='') {
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(entry=>{
  assert(entry.isFile()||entry.isDirectory());const path=prefix+entry.name;
  return entry.isDirectory()?inventory(join(dir,entry.name),path+'/'):[[path,readFileSync(join(dir,entry.name)).toString('hex')]];
 });
}
try {
 cpSync(join(root,'tests/fixtures/watch/project'),project,{recursive:true});
 mkdirSync(join(project,'adapters'));mkdirSync(join(project,'src'));
 cpSync(join(root,'tests/fixtures/watch/mock-adapter.mjs'),join(project,'adapters/fixture.mjs'));
 const control=join(project,'src/control.txt');writeFileSync(control,'ok');
 const lock=spawnSync(binary,['--no-cache','--json','lock','--',process.execPath,join(project,'adapters/fixture.mjs')],{cwd:project,env,encoding:'utf8',timeout:30000});
 assert.ifError(lock.error);assert.equal(lock.status,0,lock.stdout+lock.stderr);
 let expected=inventory(project),pending='',stderr='',failure,staleStarted=false,editOnStart=false;
 const events=[],completed=[],decoder=new StringDecoder('utf8');
 const change=mode=>{writeFileSync(control,mode);expected=inventory(project);};
 const adapterTimeout=process.platform==='darwin'?'15000':'5000';
 const child=spawn(binary,['watch','--backend','poll','--poll-interval-ms','100','--reconcile-ms','100','--debounce-ms','30','--max-wait-ms','150','--adapter-timeout-ms',adapterTimeout,'--target','watch-fixture','--validate','--json-lines','--stop-after-runs','4'],{cwd:project,env,stdio:['ignore','pipe','pipe']});
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>{child.kill();reject(new Error(`watch fault deadline: ${stderr}`));},60000);
  child.on('error',error=>{clearTimeout(timer);reject(error);});
  child.stderr.on('data',data=>{stderr+=data;if(stderr.length>1024*1024){failure=new Error('stderr bound');child.kill();}});
  child.stdout.on('data',data=>{
   try {
    pending+=decoder.write(data);assert(pending.length<=16*1024*1024);
    for(let newline;(newline=pending.indexOf('\n'))>=0;){
     const event=JSON.parse(pending.slice(0,newline));pending=pending.slice(newline+1);
     assert.equal(event.sequence,events.length+1);events.push(event);
     if(event.event==='run-started'&&editOnStart){
      // The hang snapshot has started, so the following edit MUST
      // supersede it, not merely debounce away before it starts.
      editOnStart=false;staleStarted=true;change('ok');
     }
     if(event.event!=='run-completed')continue;
     assert.deepEqual(inventory(project),expected,'watch cannot write inputs/cache/observed state');
     assert.equal(event.diagnosticsMode,'replace');completed.push(event);
     if(completed.length===1)change('crash');
     else if(completed.length===2)change('invalid');
     else if(completed.length===3){editOnStart=true;change('hang');}
    }
   }catch(error){failure=error;child.kill();}
  });
  child.on('close',(code,signal)=>{
   clearTimeout(timer);
   try {
    if(failure)throw failure;
    assert.equal(signal,null);assert.equal(pending,'');
    assert.equal(code,0,JSON.stringify({stderr,events}));assert.equal(completed.length,4);
    resolve();
   }catch(error){reject(error);}
  });
 });
 {
  assert.deepEqual(completed.map(e=>e.status),['valid','unavailable','invalid','valid'],JSON.stringify(completed.map(e=>({run:e.runId,status:e.status,diagnostics:e.result.diagnostics}))));
  assert(completed[1].result.diagnostics.some(d=>d.id==='watch.adapter-recovery'));
  assert(completed[2].result.diagnostics.some(d=>d.id==='target.operation-failed'));
  assert.deepEqual(completed[3].result.diagnostics,[],'recovery removes stale failure diagnostics');
  assert.equal(completed[3].result.pins.adapters.length,1);
  assert(staleStarted);assert(events.some(e=>e.event==='run-cancelled'));
  for(const start of events.filter(e=>e.event==='run-started'))assert.equal(events.filter(e=>['run-completed','run-cancelled'].includes(e.event)&&e.runId===start.runId).length,1);
 }
 assert.deepEqual(inventory(project),expected);
 console.log(JSON.stringify({ok:true,platform:process.platform,settled:completed.length,cancelled:events.filter(e=>e.event==='run-cancelled').length,targetLane:process.platform==='darwin'?'resident-supervised-degraded':'qualified'}));
}finally{rmSync(project,{recursive:true,force:true});}
console.log(JSON.stringify({ok:true,platform:process.platform,selectiveTarget:await selectiveTargetProbe({root,binary,env})}));
