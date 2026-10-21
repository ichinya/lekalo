// Real selective-adapter session: retained unrelated diagnostics through
// rapid planner saves, cancellation, and target recovery. No polling sleep
// is used as readiness: edits are driven by stream barriers.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,readdirSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {StringDecoder} from 'node:string_decoder';
function inventory(dir,prefix=''){
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(e=>{
  assert(e.isFile()||e.isDirectory());const path=prefix+e.name;
  return e.isDirectory()?inventory(join(dir,e.name),path+'/'):[[path,readFileSync(join(dir,e.name)).toString('hex')]];
 });
}
export async function selectiveTargetProbe({root,binary,env,fixture='project'}){
 const project=mkdtempSync(join(tmpdir(),'lekalo-watch-selected-'));
 try{
  cpSync(join(root,'tests/fixtures/watch',fixture),project,{recursive:true});
  mkdirSync(join(project,'adapters'));mkdirSync(join(project,'src'));
  const entry=join(project,'adapters/fixture.mjs');
  writeFileSync(entry,readFileSync(join(root,'tests/fixtures/watch/mock-adapter.mjs'),'utf8').replace('const selective = false','const selective = true'));
  const control=join(project,'src/control.txt');writeFileSync(control,'unrelated-invalid');
  const lock=spawnSync(binary,['--no-cache','--json','lock','--',process.execPath,entry],{cwd:project,env,encoding:'utf8',timeout:30000});
  assert.ifError(lock.error);assert.equal(lock.status,0,lock.stdout+lock.stderr);
  const source=join(project,'lekalo/modules/planner/entities.yaml'),original=readFileSync(source,'utf8');
  const events=[],completed=[],decoder=new StringDecoder('utf8');let pending='',stderr='',failure,cancelOnStart=false,expected=inventory(project);
  const flags=['watch',...(fixture==='project-successor'?['--locked']:[]),'--backend','poll','--poll-interval-ms','100','--reconcile-ms','100','--debounce-ms','30','--max-wait-ms','150','--adapter-timeout-ms','5000','--target','watch-fixture','--validate','--json-lines'];
  const child=spawn(binary,[...flags,'--stop-after-runs','4'],{cwd:project,env,stdio:['ignore','pipe','pipe']});
  const edit=mode=>{writeFileSync(control,mode);expected=inventory(project);};
  await new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{child.kill();reject(new Error('selective target deadline: '+stderr));},120000);
   child.on('error',error=>{clearTimeout(timer);reject(error);});
   child.stderr.on('data',data=>{stderr+=data;});
   child.stdout.on('data',data=>{try{
    pending+=decoder.write(data);assert(pending.length<=16*1024*1024);
    for(let newline;(newline=pending.indexOf('\n'))>=0;){
     const e=JSON.parse(pending.slice(0,newline));pending=pending.slice(newline+1);
     assert.equal(e.sequence,events.length+1);events.push(e);
     if(e.event==='run-started'&&cancelOnStart){cancelOnStart=false;edit('unrelated-invalid');}
     if(e.event!=='run-completed')continue;
     assert.deepEqual(inventory(project),expected,'selected adapters never write project state');completed.push(e);
     assert.equal(e.diagnosticsMode,'replace');
     if(completed.length===1){
      for(let version=2;version<=9;version++)writeFileSync(source,original.replace('version: 1',`version: ${version}`));
      expected=inventory(project);
     }else if(completed.length===2){
      const work=e.result.pins.adapters[0].work;
      assert(work.some(w=>w.module==='planner'&&w.state==='executed'));
      assert(work.some(w=>w.module==='unrelated'&&w.state==='reused'));
      assert.deepEqual(e.result.diagnostics,completed[0].result.diagnostics,'rapid planner edits cannot lose the unrelated adapter failure');
      // Clean full verification of the same settled tree is independent
      // of watch reuse/partition merging. Work in a private copy because
      // ordinary verify owns its ephemeral IR staging conventions.
      const oracle=mkdtempSync(join(tmpdir(),'lekalo-watch-target-oracle-'));
      try{
       cpSync(project,oracle,{recursive:true});
       if(fixture==='project-successor'){
        const clean=spawnSync(binary,['--no-cache',...flags,'--stop-after-runs','1'],{cwd:oracle,env,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
        assert.ifError(clean.error);
        const result=clean.stdout.trim().split(/\r?\n/).map(JSON.parse).find(e=>e.event==='run-completed');
        assert.equal(result.status,e.status,'successor targeted status equals a fresh no-reuse captured cycle');
        assert.deepEqual(result.result.diagnostics,e.result.diagnostics);
        assert.equal(result.result.pins.modelVersion,'0.8.0');
        assert.equal(result.result.pins.versions.ir,'0.8.0');
       }else{
       const loaded=spawnSync(binary,['--no-cache','--json','load','--ir'],{cwd:oracle,env,encoding:'utf8',timeout:30000});
       assert.ifError(loaded.error);assert.equal(loaded.status,0,loaded.stdout+loaded.stderr);
       const ir=JSON.parse(loaded.stdout).ir;
       mkdirSync(join(oracle,'.lekalo/cache/ir'),{recursive:true});
       writeFileSync(join(oracle,'.lekalo/cache/ir',ir.project.id+'.json'),JSON.stringify(ir));
       const clean=spawnSync(binary,['--no-cache','--json','verify','--target','watch-fixture','--report-file',join(oracle,'verification.json'),'--',process.execPath,join(oracle,'adapters/fixture.mjs')],{cwd:oracle,env,encoding:'utf8',timeout:30000,maxBuffer:16*1024*1024});
       assert.ifError(clean.error);const result=JSON.parse(clean.stdout.trim()||clean.stderr.trim());
       assert.equal(result.status,'invalid',JSON.stringify(result));
       const report=JSON.parse(readFileSync(join(oracle,'verification.json'),'utf8'));
       assert.equal(report.checks.find(c=>c.id==='adapter.watch-fixture')?.sourceOutcome,'fail');
       assert.equal(e.status,result.status,'settled targeted status equals full clean adapter validation');
       }
      }finally{rmSync(oracle,{recursive:true,force:true});}
      cancelOnStart=true;edit('hang');
     }else if(completed.length===3){
      assert(e.result.diagnostics.some(d=>d.id==='target.operation-failed'),'cancelled work cannot erase retained diagnostics');
      edit('ok');
     }
    }
   }catch(error){failure=error;child.kill();}});
   child.on('close',(code,signal)=>{clearTimeout(timer);try{if(failure)throw failure;assert.equal(signal,null);assert.equal(code,0,JSON.stringify({stderr,completed:completed.map(e=>({run:e.runId,status:e.status,diagnostics:e.result.diagnostics}))}));assert.equal(pending,'');resolve();}catch(error){reject(error);}});
  });
  assert.equal(completed.length,4);
  assert.deepEqual(completed.map(e=>e.status),['invalid','invalid','invalid','valid']);
  assert.deepEqual(completed.at(-1).result.diagnostics,[]);
  assert(events.some(e=>e.event==='run-cancelled'));
  for(const start of events.filter(e=>e.event==='run-started'))assert.equal(events.filter(e=>['run-completed','run-cancelled'].includes(e.event)&&e.runId===start.runId).length,1);
  assert.deepEqual(inventory(project),expected);
  return {settled:completed.length,cancelled:events.filter(e=>e.event==='run-cancelled').length,cleanAdapterComparisons:1};
 }finally{rmSync(project,{recursive:true,force:true});}
}
