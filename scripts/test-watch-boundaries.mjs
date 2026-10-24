#!/usr/bin/env node
// Actual producer events at inventory-pair and runtime-home boundaries.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,mkdtempSync,readFileSync,renameSync,rmSync,symlinkSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {StringDecoder} from 'node:string_decoder';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const require=createRequire(import.meta.url);
const Ajv=require('ajv/dist/2020').default;
const validate=new Ajv({strict:true,allErrors:true}).compile(JSON.parse(readFileSync(join(root,'contracts/watch-event.schema.v0.8.14.json'),'utf8')));
const env={...process.env};delete env.LEKALO_PROJECT;
const work=mkdtempSync(join(tmpdir(),'lekalo-watch-boundaries-'));
const project=join(work,'project');
const check=event=>assert(validate(event),JSON.stringify(validate.errors));
let pairEvents=0;
try {
 cpSync(join(root,'tests/fixtures/watch/project'),project,{recursive:true});
 const lock=spawnSync(binary,['lock'],{cwd:project,env,encoding:'utf8',timeout:30000});assert.equal(lock.status,0,lock.stderr);
 const generated=join(project,'.lekalo/generated');mkdirSync(generated,{recursive:true});
 for(let i=0;i<2050;i++)writeFileSync(join(generated,`old-${i}.txt`),String(i));
 const child=spawn(binary,['watch','--generate-check','--backend','poll','--json-lines','--poll-interval-ms','100','--reconcile-ms','1000','--debounce-ms','1000','--max-wait-ms','4000','--stop-after-runs','2'],{cwd:project,env,stdio:['ignore','pipe','pipe']});
 const completed=[],events=[];let pending='',stderr='',failure;const decoder=new StringDecoder('utf8');
 await new Promise((resolve,reject)=>{
  const timer=setTimeout(()=>{child.kill();reject(new Error(`pair probe deadline: ${stderr}`));},90000);
  child.on('error',error=>{clearTimeout(timer);reject(error);});child.stderr.on('data',data=>{stderr+=data;});
  child.stdout.on('data',data=>{
   try {
    pending+=decoder.write(data);assert(pending.length<=16*1024*1024);
    for(let end;(end=pending.indexOf('\n'))>=0;){
     const event=JSON.parse(pending.slice(0,end));pending=pending.slice(end+1);check(event);events.push(event);
     if(event.event==='run-completed'){
      completed.push(event);
      if(completed.length===1)for(let i=0;i<2050;i++)renameSync(join(generated,`old-${i}.txt`),join(generated,`new-${i}.txt`));
     }
    }
   }catch(error){failure=error;child.kill();}
  });
  child.on('close',(code,signal)=>{
   clearTimeout(timer);try{if(failure)throw failure;assert.equal(signal,null);assert.equal(code,3,stderr);assert.equal(pending,'');resolve();}catch(error){reject(error);}
  });
 });
 assert.equal(completed.length,2);
 const refused=completed[1].result;
 assert.equal(refused.status,'denied');assert.equal(refused.previousSourceRevision,completed[0].sourceRevision);
 assert.deepEqual(refused.changedInputs,[]);assert.equal(refused.counts.changedFiles,0);
 assert(refused.stages.filter(s=>s.state!=='not-requested').every(s=>s.state==='unavailable'&&s.coverage==='partial'));
 assert(refused.diagnostics.some(d=>d.id==='watch.resource-limit'&&d.data.limit==='changed-inputs'&&d.data.count===4100));
 for(const started of events.filter(e=>e.event==='run-started'))assert.equal(events.filter(e=>['run-completed','run-cancelled'].includes(e.event)&&e.runId===started.runId).length,1);
 pairEvents=events.length;
 rmSync(generated,{recursive:true,force:true});mkdirSync(generated,{recursive:true});
 const outside=join(work,'outside');mkdirSync(outside);writeFileSync(join(outside,'secret.txt'),'outside-marker-never-read');
 symlinkSync(outside,join(generated,'link'),process.platform==='win32'?'junction':'dir');
 const refusal=spawnSync(binary,['watch','--generate-check','--json-lines'],{cwd:project,env,encoding:'utf8',timeout:30000,maxBuffer:1024*1024});
 assert.ifError(refusal.error);assert.equal(refusal.status,3,refusal.stderr);
 const rows=refusal.stdout.trim().split('\n').map(line=>JSON.parse(line));rows.forEach(check);
 const security=rows.find(e=>e.event==='recovery-required'&&e.status==='denied');assert(security);
 assert(security.diagnostics.some(d=>d.id==='structure.path-link'&&!d.source));
 assert(!refusal.stdout.includes('outside-marker-never-read'));
 assert.equal(readFileSync(join(outside,'secret.txt'),'utf8'),'outside-marker-never-read');
 console.log(JSON.stringify({ok:true,pairEvents,pairRows:4100,generatedLinkRefusal:true,platform:process.platform}));
} finally {rmSync(work,{recursive:true,force:true});}
