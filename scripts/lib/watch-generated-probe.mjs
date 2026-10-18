import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,writeFileSync,unlinkSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {StringDecoder} from 'node:string_decoder';
export async function generatedProbe({root,binary,backend,validate,inventory}){
 const project=mkdtempSync(join(tmpdir(),'lekalo-watch-generated-'));
 const env={...process.env};delete env.LEKALO_PROJECT;
 try{
  cpSync(join(root,'tests/fixtures/watch/project'),project,{recursive:true});
  const lock=spawnSync(binary,['--no-cache','--json','lock'],{cwd:project,env,encoding:'utf8',timeout:20000});
  assert.ifError(lock.error);assert.equal(lock.status,0,lock.stdout+lock.stderr);
  mkdirSync(join(project,'.lekalo/generated'),{recursive:true});
  const generated=join(project,'.lekalo/generated/probe.ts');
  let expected=inventory(project),pending='',stderr='',failure,feedbackTimer;
  const events=[],completed=[],decoder=new StringDecoder('utf8');
  const child=spawn(binary,['watch','--backend',backend,'--poll-interval-ms','100','--reconcile-ms','100','--debounce-ms','20','--json-lines','--validate','--generate-check','--stop-after-runs','3'],{cwd:project,env,stdio:['ignore','pipe','pipe']});
  await new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{child.kill();reject(new Error('generated feedback probe deadline'));},45000);
   child.on('error',error=>{clearTimeout(timer);clearTimeout(feedbackTimer);reject(error);});
   child.stderr.on('data',data=>{stderr+=data;if(stderr.length>1024*1024){failure=new Error('stderr bound');child.kill();}});
   child.stdout.on('data',data=>{
    try{
     pending+=decoder.write(data);assert(pending.length<=16*1024*1024);
     for(let newline;(newline=pending.indexOf('\n'))>=0;){
      const event=JSON.parse(pending.slice(0,newline));pending=pending.slice(newline+1);
      assert(validate(event),JSON.stringify(validate.errors));assert.equal(event.sequence,events.length+1);events.push(event);
      if(event.event!=='run-completed')continue;
      assert.deepEqual(inventory(project),expected,'watch must not mutate generated/ignored inputs');completed.push(event);
      if(completed.length===1){
       for(const path of ['.lekalo/cache/feedback.txt','target/feedback.txt','dist/feedback.txt','node_modules/pkg/feedback.js']){
        const dest=join(project,path);mkdirSync(dirname(dest),{recursive:true});writeFileSync(dest,'synthetic excluded feedback\n');
       }
       expected=inventory(project);
       feedbackTimer=setTimeout(()=>{
        try{assert.equal(events.filter(e=>e.event==='run-started').length,1,'excluded writes cannot start a feedback run');writeFileSync(generated,'// synthetic orphan\n');expected=inventory(project);}
        catch(error){failure=error;child.kill();}
       },700);
      }else if(completed.length===2){
       feedbackTimer=setTimeout(()=>{
        try{
         assert.equal(events.filter(e=>e.event==='run-started').length,2,'one generated save must remain quiet over multiple reconciliation intervals');
         unlinkSync(generated);expected=inventory(project);
        }catch(error){failure=error;child.kill();}
       },700);
      }
     }
    }catch(error){failure=error;child.kill();}
   });
   child.on('close',(code,signal)=>{
    clearTimeout(timer);clearTimeout(feedbackTimer);
    try{if(failure)throw failure;assert.equal(signal,null);assert.equal(code,0,stderr);assert.equal(pending,'');resolve();}catch(error){reject(error);}
   });
  });
  assert.equal(completed.length,3);
  assert.deepEqual(completed.map(e=>e.status),['valid','invalid','valid']);
  assert.equal(new Set(completed.map(e=>e.sourceRevision)).size,1,'generated changes cannot alter semantic revision');
  assert(completed[1].result.changedInputs.some(row=>row.class==='generated'));
  assert.equal(completed[1].result.drift,undefined,'failed current drift check cannot retain successful prior receipt');
  assert.equal(completed[2].result.drift.verdict,'clean');
  for(const event of completed.slice(1))assert(event.result.work.every(unit=>unit.state==='reused'),'generated-only revision reuses semantic diagnostics');
  assert.deepEqual(inventory(project),expected);
  return {events:events.length,settled:completed.length,semanticRevisions:1};
 }finally{rmSync(project,{recursive:true,force:true});}
}
