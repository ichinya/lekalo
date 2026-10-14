#!/usr/bin/env node
// Real pipes: broken readers and backpressure must fail-stop, not hang watch.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdtempSync,readFileSync,readdirSync,writeFileSync,renameSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={...process.env};delete env.LEKALO_PROJECT;
function inventory(dir,prefix=''){
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(entry=>entry.isDirectory()?inventory(join(dir,entry.name),prefix+entry.name+'/'):[[prefix+entry.name,readFileSync(join(dir,entry.name)).toString('hex')]]);
}
const results=[];
for(const mode of ['broken','slow']){
 const project=mkdtempSync(join(tmpdir(),'lekalo-watch-consumer-'));
 try{
  cpSync(join(root,'tests/fixtures/watch/project'),project,{recursive:true});
  if(mode==='slow'){
   const module='planner_'+'x'.repeat(55);
   renameSync(join(project,'lekalo/modules/planner'),join(project,'lekalo/modules',module));
   const moduleFile=join(project,'lekalo/modules',module,'module.yaml');
   writeFileSync(moduleFile,readFileSync(moduleFile,'utf8').replaceAll('planner',module));
   const file=join(project,'lekalo/modules',module,'entities.yaml');
   let source=readFileSync(file,'utf8').replaceAll('planner',module);
   source+=`  - id: ${module}.record_${'y'.repeat(56)}\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: ${module}.title\n        required: true\n    identity:\n`;
   for(let i=0;i<255;i++)source+=`      - absent_${'z'.repeat(52)}${String(i).padStart(3,'0')}\n`;
   writeFileSync(file,source);
   const check=spawnSync(binary,['--json','--no-cache','validate'],{cwd:project,env,encoding:'utf8',timeout:20000,maxBuffer:16*1024*1024});
   assert.ifError(check.error);assert.equal(check.status,1);const result=JSON.parse(check.stderr||check.stdout);
   assert.equal(result.diagnostics.length,255,JSON.stringify(result.diagnostics.slice(0,3)));
   assert(JSON.stringify(result.diagnostics).length>192*1024,'payload must exceed pipe and reader buffers');
  }
  const before=inventory(project),since=Date.now();
  const child=spawn(binary,['watch','--backend','poll','--json-lines','--stop-after-runs','1'],{cwd:project,env,stdio:['ignore','pipe','pipe']});
  if(mode==='broken')child.stdout.destroy();else child.stdout.pause();
  let stderr='';
  child.stderr.on('data',data=>{stderr+=data;assert(stderr.length<=1024*1024);});
  await new Promise((resolve,reject)=>{
   const timer=setTimeout(()=>{child.kill();reject(new Error(`${mode}: consumer teardown deadline`));},15000);
   child.on('error',error=>{clearTimeout(timer);reject(error);});
   child.on('exit',(code,signal)=>{clearTimeout(timer);try{assert.equal(signal,null);assert.equal(code,1,stderr);resolve();}catch(error){reject(error);}});
  });
  child.stdout.destroy();child.stderr.destroy();
  const elapsed=Date.now()-since;
  if(mode==='slow')assert(elapsed>=4500,'real output acknowledgement deadline must fire');
  assert.deepEqual(inventory(project),before,'consumer failure cannot mutate the project');
  results.push({mode,elapsedMs:elapsed});
 }finally{rmSync(project,{recursive:true,force:true});}
}
console.log(JSON.stringify({ok:true,platform:process.platform,results}));
