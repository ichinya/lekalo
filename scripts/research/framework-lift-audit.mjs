#!/usr/bin/env node
// Independent controller audit over raw exports; no inference and no candidate edits.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {sha,write} from './framework-lift-rounding.mjs';
import {completionAudit,usage,observations} from './framework-lift-runner.mjs';
export function audit(initial,devin){
  const old=JSON.parse(fs.readFileSync(join(initial,'results.json'),'utf8'));
  const revised=JSON.parse(fs.readFileSync(join(devin,'results.json'),'utf8'));
  assert.equal(old.results.length,8);assert.equal(revised.results.length,4);
  const profileFor=id=>id.startsWith('devin')?'devin':'codex';
  function row(root,r){
    const dir=join(root,'runs',r.id),cli=profileFor(r.id),actual=usage(dir,cli),completion=completionAudit(dir,cli);
    for(const k of ['input','output','cachedInput','reasoningOutput'])assert.equal(actual[k],r.usage[k],r.id+' usage mismatch '+k);
    assert.ok(actual.input>=actual.cachedInput);if(actual.reasoningOutput!==null)assert.ok(actual.output>=actual.reasoningOutput);
    if(cli==='devin'){
      const tr=JSON.parse(fs.readFileSync(join(dir,'trajectory.json'),'utf8')),metrics=tr.steps.filter(s=>s.metrics).map(s=>s.metrics);
      for(const [to,from]of [['input','prompt_tokens'],['output','completion_tokens'],['cachedInput','cached_tokens']])assert.equal(metrics.reduce((n,s)=>n+(s[from]??0),0),actual[to]);
    }
    const obs=observations(dir,cli),recorded=JSON.parse(fs.readFileSync(join(dir,'tool-observations.private.json'),'utf8'));
    assert.equal(sha(JSON.stringify(obs)),sha(JSON.stringify(recorded)),r.id+' observation export mismatch');
    const reads=JSON.parse(fs.readFileSync(join(dir,'marked-reads.json'),'utf8'));
    assert.equal(reads.unverified.length,0);
    // Recompute marker hashes independently from raw delivered tool observations.
    const found=[];
    for(const o of obs)for(const m of o.content.matchAll(/@@native-content ([^\r\n]+)\r?\n([\s\S]*?)@@native-end ([^\r\n]+)/g)){
      const data=JSON.parse(m[1]),content=m[2].replaceAll('\r\n','\n');
      assert.equal(data.path,m[3].trim());assert.equal(Buffer.byteLength(content),data.bytes);assert.equal(sha(Buffer.from(content)),data.sha256);found.push(data);
    }
    assert.deepEqual([...new Set(found.map(x=>x.path))].sort(),reads.uniqueSourceFiles);
    for(const changed of r.grade.changed.filter(p=>p.startsWith('src/')))assert.ok(reads.uniqueSourceFiles.includes(changed),'edited source path not covered by source audit: '+changed);
    const helper=obs.findIndex(o=>o.content.includes('# context capsule')&&o.content.includes('Observed mappings:'));
    if(r.arm==='B')assert.ok(helper>=0&&helper<obs.findIndex(o=>o.content.includes('@@native-content')));
    if(r.arm==='A')assert.equal(helper,-1);
    return {id:r.id,profile:r.profile,arm:r.arm,repetition:r.repetition,durationMs:r.durationMs,cliExit:r.cli.exitCode,executorComplete:completion.complete,confirmationRejected:completion.confirmationRejected,nativePass:r.grade.accepted,holdoutPassed:r.grade.holdout.passed,holdoutTotal:r.grade.holdout.total,strictSuccess:r.cli.exitCode===0&&!r.cli.timedOut&&r.grade.accepted&&completion.complete,input:actual.input,output:actual.output,total:actual.input+actual.output,cachedInput:actual.cachedInput,uncachedInput:actual.input-actual.cachedInput,reasoningOutput:actual.reasoningOutput,sourceFiles:reads.uniqueSourceFiles.length,sourceBytes:found.reduce((n,x)=>n+x.bytes,0),sourceFileList:reads.uniqueSourceFiles,toolCalls:actual.toolCalls,finalSubmissions:completion.complete?1:0,postSubmissionFixCycles:completion.complete?0:null,humanInterventions:0,monetaryCharge:{state:'unknown'}};
  }
  const preliminary=old.results.filter(r=>profileFor(r.id)==='devin').map(r=>row(initial,r));
  const primary=[...revised.results.map(r=>row(devin,r)),...old.results.filter(r=>profileFor(r.id)==='codex').map(r=>row(initial,r))];
  assert.equal(primary.length,8);
  assert.ok(primary.every(r=>r.nativePass&&r.holdoutPassed===20&&r.holdoutTotal===20));
  const mean=(a,k)=>a.reduce((n,x)=>n+x[k],0)/a.length;
  const groups=[];
  for(const profile of [...new Set(primary.map(r=>r.profile))])for(const arm of ['A','B']){
    const rows=primary.filter(r=>r.profile===profile&&r.arm===arm);assert.equal(rows.length,2);
    groups.push({profile,arm,attempts:2,completed:rows.filter(r=>r.executorComplete).length,successes:rows.filter(r=>r.strictSuccess).length,nativePasses:rows.filter(r=>r.nativePass).length,...Object.fromEntries(['durationMs','input','output','total','cachedInput','uncachedInput','sourceFiles','sourceBytes','toolCalls'].map(k=>[k,mean(rows,k)]))});
  }
  const deltas=[];
  for(const profile of [...new Set(primary.map(r=>r.profile))]){
    const a=groups.find(g=>g.profile===profile&&g.arm==='A'),b=groups.find(g=>g.profile===profile&&g.arm==='B');
    deltas.push({profile,...Object.fromEntries(['durationMs','input','output','total','uncachedInput','sourceFiles','sourceBytes'].map(k=>[k,100*(b[k]-a[k])/a[k]]))});
  }
  const all=[...preliminary,...primary],sum=k=>all.reduce((n,x)=>n+x[k],0);
  const result={scope:'rounding-supported-top-level-v1; one new synthetic task, two pairs per profile, not comparable to earlier planner task',primary,preliminary,groups,deltas,allExecution:{attempts:all.length,input:sum('input'),output:sum('output'),total:sum('total'),cachedInput:sum('cachedInput')},audit:{usage:'Raw exports reaggregated; Devin per-step metrics equal final metrics; subsets not added twice',sourceFiles:'src/**/*.ts delivered-content markers verified; controller reviewed all source-delivering tool outputs; every source edit path was already read',allFilesRead:{state:'unknown',reason:'Only native TypeScript source delivery was exhaustively audited; metadata, tool/config snippets and instruction replays use a different scope'},cost:'unknown; CLI account charge and approval reviewer usage not independently available',limitations:['One small task; two pairs per CLI; no general effect or cross-model performance claim.','Devin B1 passed native oracle but did not complete due to non-interactive npx tsc confirmation rejection.','Original 4 Devin attempts retained as technical preliminary series; corrected quartet chosen in full before those runs, not by outcome.','Legacy runner finalSubmissions:1 fields are not authoritative; completion audit corrects them without rewriting original receipts.','Source-read audit does not count imports executed by tests/compiler without delivering their source to the agent.','No post-submission functional retries; malformed intermediate self-checks stay in tool logs.']}};
  write(devin,'summary.public.json',result);return result;
}
if(process.argv[2]&&process.argv[3]){const r=audit(resolve(process.argv[2]),resolve(process.argv[3]));console.log(JSON.stringify({groups:r.groups,deltas:r.deltas,allExecution:r.allExecution}));}
