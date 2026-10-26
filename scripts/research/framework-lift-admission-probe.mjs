#!/usr/bin/env node
// Synthetic closed-contract probes only. Never relabel these as imported live arms.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {canonical,digest} from '../lib/framework-lift-executor.mjs';
const repo=fileURLToPath(new URL('../../',import.meta.url));
export function probe(binary){
 const base=JSON.parse(fs.readFileSync(join(repo,'tests/fixtures/framework-lift-campaign/golden/scheduled.json'),'utf8'));
 const root=fs.mkdtempSync(join(tmpdir(),'lekalo-evaluation-admission-'));
 const cases=[['known-fixture-control',()=>{}],...['temperatureMicros','topPMicros','maxOutputTokens'].map(k=>['unknown-'+k,c=>{c.profile.sampling[k]={state:'unknown'};}]),['unknown-pricing',c=>{c.profile.pricing={state:'unknown'};}],['unbounded-tokens',c=>{c.profile.limits.maxTokens=null;}]];
 const rows=[];
 try{for(const [name,change]of cases){
  const c=structuredClone(base);change(c);const body=structuredClone(c);delete body.approval;c.approval.contentDigest=digest(body);
  const input=join(root,name+'.json');fs.writeFileSync(input,canonical(c));
  const r=spawnSync(binary,['--json','evaluation','validate','--family','campaign','--input',input],{encoding:'utf8',timeout:30000,maxBuffer:2*1024*1024});
  assert.ok(!r.error,'CLI failed to execute');const output=JSON.parse(r.stdout.trim()||r.stderr.trim());
  assert.equal(r.status,name==='known-fixture-control'?0:1,'unexpected contract admission');
  const diagnostics=(output.diagnostics??[]).map(d=>({id:d.id,detail:d.data?.detail}));
  if(r.status!==0)assert.ok(diagnostics.some(d=>d.id==='evaluation.protocol-invalid'&&d.detail==='closed-contract'));
  rows.push({name,exit:r.status,diagnostics});
 }}finally{fs.rmSync(root,{recursive:true,force:true});}
 return {format:'lekalo-167-admission-probe/1',kind:'synthetic-contract-probes-not-empirical-results',contract:'0.6.4',actualRecordArm:'NOTRUN',actualCompare:'NOTRUN',binarySha256:createHash('sha256').update(fs.readFileSync(binary)).digest('hex'),rows};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const binary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':'')),result=probe(binary);
 if(process.argv[2])fs.writeFileSync(process.argv[2],JSON.stringify(result,null,2)+'\n',{flag:'wx'});
 console.log(JSON.stringify(result));
}
