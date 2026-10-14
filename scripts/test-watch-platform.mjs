#!/usr/bin/env node
// Real OS watcher probe. Run on Windows/Linux/macOS with the built binary;
// a readiness/result barrier, not an arbitrary sleep, owns each edit.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createRequire} from 'node:module';
import {cpSync,mkdtempSync,readFileSync,writeFileSync,readdirSync,rmSync,renameSync,unlinkSync} from 'node:fs';
import {StringDecoder} from 'node:string_decoder';
import {tmpdir} from 'node:os';
import {resolve,join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {generatedProbe} from './lib/watch-generated-probe.mjs';
const require=createRequire(import.meta.url);
const Ajv=require('ajv/dist/2020').default;
assert.equal(require('ajv/package.json').version,'8.17.1','pinned independent validator');
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const schema=JSON.parse(readFileSync(join(root,'contracts/watch-event.schema.v0.8.0.json'),'utf8'));
const validate=new Ajv({strict:true,allErrors:true}).compile(schema);
function inventory(dir,prefix=''){
  const rows=[];
  for(const entry of readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))){
    const path=prefix+entry.name;
    if(entry.isDirectory())rows.push(...inventory(join(dir,entry.name),path+'/'));
    else {assert(entry.isFile(),'test inventory refuses links');rows.push([path,readFileSync(join(dir,entry.name)).toString('hex')]);}
  }
  return rows;
}
async function probe(backend){
  const project=mkdtempSync(join(tmpdir(),'lekalo-watch-'));
  try{
    cpSync(join(root,'tests/fixtures/watch/project'),project,{recursive:true});
    const path=join(project,'lekalo/modules/planner/entities.yaml');
    const moved=join(project,'lekalo/modules/moved/entities.yaml');
    let expected=inventory(project);
    const events=[];
    const revisions=[];
    const actions=[
      ()=>{for(const version of [2,3,4])writeFileSync(path,readFileSync(path,'utf8').replace(/version: \d+/,`version: ${version}`));},
      ()=>{const temp=path+'.tmp';writeFileSync(temp,readFileSync(path,'utf8').replace('version: 4','version: 5'));renameSync(temp,path);},
      ()=>renameSync(join(project,'lekalo/modules/planner'),join(project,'lekalo/modules/moved')),
      ()=>unlinkSync(moved),
    ];
    const decoder=new StringDecoder('utf8');
    const child=spawn(binary,['watch','--backend',backend,'--poll-interval-ms','100','--reconcile-ms','100','--debounce-ms','20','--max-wait-ms','100','--json-lines','--stop-after-runs','5'],{cwd:project,stdio:['ignore','pipe','pipe']});
    let pending='',stderr='',failure;
    await new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{child.kill();reject(new Error(`${backend}: watch deadline; ${stderr}`));},45000);
      child.on('error',error=>{clearTimeout(timer);reject(error);});
      child.stderr.on('data',data=>{stderr+=data;if(stderr.length>1024*1024){failure=new Error('stderr bound');child.kill();}});
      child.stdout.on('data',data=>{
        try{
          pending+=decoder.write(data);assert(pending.length<=16*1024*1024,'stream buffer bound');
          for(let newline;(newline=pending.indexOf('\n'))>=0;){
            const line=pending.slice(0,newline);pending=pending.slice(newline+1);
            const event=JSON.parse(line);assert(validate(event),JSON.stringify(validate.errors));
            assert.equal(event.sequence,events.length+1,'strict sequence');events.push(event);
            if(event.event==='run-completed'){
              assert.equal(event.result.status,'valid');
              revisions.push(event);
              assert.deepEqual(inventory(project),expected,'no watcher writes between edit barriers');
              if(revisions.length<=actions.length){actions[revisions.length-1]();expected=inventory(project);}
            }
          }
        }catch(error){failure=error;child.kill();}
      });
      child.on('close',(code,signal)=>{
        clearTimeout(timer);
        try{if(failure)throw failure;assert.equal(signal,null);assert.equal(code,0,stderr);assert.equal(pending,'');resolve();}catch(error){reject(error);}
      });
    });
    assert.equal(revisions.length,5,'all readiness/result edit barriers settled');
    assert.equal(events[0].backend,backend,'explicit backend must not silently switch');
    const completed=events.filter(event=>event.event==='run-completed');assert.equal(completed.length,5);
    const result=completed[1].result;
    assert.equal(result.affected.scope,'selective');assert.deepEqual(result.affected.modules,['planner']);
    assert.equal(result.work.find(unit=>unit.module==='planner').state,'executed');
    assert.equal(result.work.find(unit=>unit.module==='unrelated').state,'reused');
    assert.equal(result.pipelineCounts.decodedDocuments,1);assert.equal(result.pipelineCounts.decodedDefinitions,1);
    assert.equal(result.pipelineCounts.emittedGraphUnits,1);assert.equal(result.pipelineCounts.reusedGraphUnits,4);
    assert.equal(completed[2].result.pipelineCounts.emittedGraphUnits,1,'atomic replacement rebuilds only changed graph source');
    assert.equal(completed[3].result.pipelineCounts.emittedGraphUnits,0,'filesystem move is not a semantic graph rename');
    assert(completed[3].result.changedInputs.some(row=>row.renameOf),'parent rename updates logical source binding');
    assert(completed[4].result.capsuleInvalidations.includes('planner.title'),'deletion invalidates removed symbols');
    assert.equal(completed[4].result.pipelineCounts.assembledGraphNodes,4,'deleted symbol leaves current graph');
    for(const revision of completed)assert.deepEqual(revision.result.diagnostics,[]);
    assert.equal(new Set(completed.map(event=>event.sourceRevision)).size,5,'each stable edited inventory has its own revision');
    assert.equal(events.at(-1).event,'session-stopped');
    for(const event of events.filter(event=>event.event==='run-started')){
      assert.equal(events.filter(terminal=>['run-completed','run-cancelled'].includes(terminal.event)&&terminal.runId===event.runId).length,1,'one terminal per start');
    }
    assert.deepEqual(inventory(project),expected,'watch writes no cache, history, Model or output');
    return {backend,platform:process.platform,events:events.length,settled:completed.length};
  }finally{rmSync(project,{recursive:true,force:true});}
}
const results=[];for(const backend of ['native','poll'])results.push({...await probe(backend),generatedFeedback:await generatedProbe({root,binary,backend,validate,inventory})});
process.stdout.write(JSON.stringify({ok:true,results})+'\n');
