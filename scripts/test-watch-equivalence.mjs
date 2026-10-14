#!/usr/bin/env node
// Independent clean CLI equivalence over default/strict/scoped metadata reviews.
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {cpSync,mkdtempSync,readFileSync,readdirSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={...process.env};delete env.LEKALO_PROJECT;
function inventory(dir,prefix=''){
 return readdirSync(dir,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(entry=>{
   const path=prefix+entry.name;assert(entry.isFile()||entry.isDirectory());
   return entry.isDirectory()?inventory(join(dir,entry.name),path+'/'):[[path,readFileSync(join(dir,entry.name)).toString('hex')]];
 });
}
function invoke(project,args){
 const result=spawnSync(binary,args,{cwd:project,env,encoding:'utf8',timeout:20000,maxBuffer:16*1024*1024});
 assert.ifError(result.error);assert.equal(result.signal,null,result.stderr);
 return result;
}
let cases=0;
for(const fixture of ['watch/project','authorization/planner','authorization/uncovered','authorization/malformed','authorization/stale','authorization/mapping-partial']){
 const project=mkdtempSync(join(tmpdir(),'lekalo-watch-equivalence-'));
 try{
   cpSync(join(root,'tests/fixtures',fixture),project,{recursive:true});
   const before=inventory(project);
   const scopes=fixture==='watch/project'?[undefined,'planner','unrelated','missing']:[undefined,'planner'];
   for(const strict of [false,true])for(const module of scopes){
     const flags=[...(strict?['--strict']:[]),...(module?['--module',module]:[])];
     const clean=invoke(project,['--json','--no-cache','validate',...flags]);
     const owned=clean.stdout.trim()||clean.stderr.trim();
     assert(owned,`${fixture} strict=${strict} module=${module}: missing status-owned JSON`);
     const expected=JSON.parse(owned);
     const watched=invoke(project,['watch','--backend','poll','--json-lines','--stop-after-runs','1','--validate',...flags]);
     const events=watched.stdout.trim().split('\n').map(line=>JSON.parse(line));
     const terminal=events.find(event=>event.event==='run-completed')??events.find(event=>event.event==='recovery-required'&&event.status);
     assert(terminal,watched.stdout+watched.stderr);
     const actual=terminal.result??terminal;
     assert.equal(watched.status,clean.status,`${fixture} strict=${strict} module=${module}: exit`);
     assert.equal(actual.status,expected.status,`${fixture}: status`);
     assert.deepEqual(actual.diagnostics,expected.diagnostics??[],`${fixture} strict=${strict} module=${module}: diagnostics`);
     assert.deepEqual(inventory(project),before,'neither no-cache clean validation nor watch may write');
     cases++;
   }
 }finally{rmSync(project,{recursive:true,force:true});}
}
console.log(JSON.stringify({ok:true,cases,platform:process.platform}));
