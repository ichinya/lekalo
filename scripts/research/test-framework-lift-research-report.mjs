import assert from 'node:assert/strict';
import {test} from 'node:test';
import fs from 'node:fs';
import {tmpdir} from 'node:os';
import {dirname,join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {assertFreshReportPath,pairedStatistics,researchReport,exposureSummary} from './framework-lift-research-report.mjs';
const read=p=>JSON.parse(fs.readFileSync(new URL(p,import.meta.url),'utf8'));

test('paired descriptive statistics retain incomplete attempts and exact signed changes',()=>{
 const source=read('./results/2026-10-26-read-once.json'),stats=pairedStatistics(source.rows,source.profiles.map(p=>p.id));
 assert.equal(stats[0].pairs,2);assert.deepEqual(stats[0].successes,{A:2,B:1});
 assert.deepEqual(stats[0].metrics.total.sums,{A:399062,B:467113});
 assert.deepEqual(stats[0].metrics.total.relativeChangeBvsA,{state:'known',numerator:'68051',denominator:'399062'});
 assert.deepEqual(stats[0].metrics.total.pairedDeltas,[{repetition:1,BminusA:24251},{repetition:2,BminusA:43800}]);
 assert.deepEqual(stats[0].metrics.uncachedInput.relativeChangeBvsA,{state:'known',numerator:'-4273',denominator:'6901'});
 assert.deepEqual(stats[1].metrics.total.sums,{A:339221,B:442706});
 const broken=structuredClone(source.rows);broken.push(broken[0]);assert.throws(()=>pairedStatistics(broken,source.profiles.map(p=>p.id)),/one scheduled A/);
 broken.pop();broken[0].state='not-run';assert.throws(()=>pairedStatistics(broken,source.profiles.map(p=>p.id)),/unattempted/);
});

test('research report joins the historical disclosure and current admission without relabeling either',()=>{
 const original=read('./results/2026-10-26-read-once.json'),admission=read('./results/2026-10-26-admission-v11.json'),report=read('./results/2026-10-27-public-fixture-research.json');
 assert.deepEqual(report.rows,original.rows);assert.deepEqual(report.totals,original.totals);
 assert.equal(original.admission.recordArm,'NOTRUN');assert.equal(report.evidence,'empirical-local-receipts-recorded-unverified');
 assert.deepEqual(report.comparisons,pairedStatistics(original.rows,original.profiles.map(p=>p.id)));
 assert.equal(report.totals.strictSuccesses,admission.parity.strictSuccesses);assert.equal(report.totals.total,admission.parity.totalTokens);
 assert.deepEqual(report.sources.admissionResults,admission.profiles.map(p=>({profile:p.profile,resultDigest:p.resultDigest,evidenceStatus:p.evidenceStatus})));
 const recipe=fs.readFileSync(new URL('./public-fixture-research.recipe.v1.json',import.meta.url));assert.equal(report.recipe.sha256,createHash('sha256').update(recipe).digest('hex'));
 assert.equal(report.sources.originalManifestSha256,original.archive.manifestSha256);
 assert.equal(report.sources.admissionManifestSha256,JSON.parse(recipe).campaigns[0].admissionManifestSha256);
 assert.match(report.interpretation.sample,/descriptive only/);assert.match(report.interpretation.causality,/does not isolate/);
 assert.doesNotMatch(JSON.stringify(report),/C:\\|C:\/Users|account[_-]?id|access[_-]?token|api[_-]?key/i);
});

test('caller supplied classification or arbitrary pins cannot admit another campaign',()=>{
 assert.throws(()=>researchReport('missing','0'.repeat(64),'missing','1'.repeat(64)),/not in the reviewed public-fixture allowlist/);
 assert.throws(()=>researchReport('missing','0'.repeat(64),'missing','1'.repeat(64),'2'),/not in the reviewed public-fixture allowlist/);
 assert.throws(()=>researchReport('missing','0'.repeat(64),'missing','1'.repeat(64),'../private'),/unsupported research recipe version/);
});

test('output inside an archive, including through an alias, refuses before writes',()=>{
 const root=fs.mkdtempSync(join(tmpdir(),'lekalo-research-path-'));
 try{
  const archive=join(root,'archive'),alias=join(root,'alias');fs.mkdirSync(archive);fs.symlinkSync(archive,alias,process.platform==='win32'?'junction':'dir');
  for(const target of [join(archive,'new.json'),join(alias,'new-dir/report.json')])assert.throws(()=>assertFreshReportPath(target,[archive]),/outside evidence archives/);
  const existing=join(root,'existing.json');fs.writeFileSync(existing,'preserve');assert.throws(()=>assertFreshReportPath(existing,[archive]),/must be new/);
  assertFreshReportPath(join(root,'report.json'),[archive]);assert.deepEqual(fs.readdirSync(archive),[]);assert.equal(fs.readFileSync(existing,'utf8'),'preserve');
 }finally{assert.equal(dirname(resolve(root)),resolve(tmpdir()));fs.rmSync(root,{recursive:true,force:true});}
});

test('actual exposure audit verifies semantic bytes and separates usage unit kinds',()=>{
 const body='no-policies\n',hash=createHash('sha256').update(body).digest('hex');
 const marker='@@semantic-content '+JSON.stringify({digest:hash,bytes:Buffer.byteLength(body)})+'\n';
 const obs=[{content:marker+body},{content:'# Work with Lekalo in this project\nRead source once per version and needed range\n'},{content:'confined src/**/*.ts path required'}];
 const result=exposureSummary(obs,[{output:20},{output:100}],'devin-agent-step');assert.equal(result.fullInstructionReadDetected,true);assert.equal(result.confinedReaderRefusals,1);assert.equal(result.usageUnitCount,2);assert.equal(result.largestUsageUnitOutputTokens.value,100);assert.equal(result.semanticBytesLowerBound,12);assert.equal(result.unverifiedSemanticMarkers,0);
 const broken=exposureSummary([{content:marker+'truncated'}],[],'codex-turn-completed-aggregate');assert.equal(broken.semanticBytesLowerBound,0);assert.equal(broken.unverifiedSemanticMarkers,1);assert.equal(broken.largestUsageUnitOutputTokens.state,'unknown');assert.equal(broken.fullInstructionReadDetected,false);
});

test('compact campaign retains native failures, actual full-instruction reads and admitted totals',()=>{
 const report=read('./results/2026-10-27-compact-context-research.json'),admission=read('./results/2026-10-27-compact-admission-v11.json'),recipeBytes=fs.readFileSync(new URL('./public-fixture-research.recipe.v2.json',import.meta.url)),recipe=JSON.parse(recipeBytes);
 assert.equal(report.format,'lekalo-public-fixture-research/2');assert.equal(report.recipe.sha256,createHash('sha256').update(recipeBytes).digest('hex'));
 assert.equal(report.sources.originalManifestSha256,recipe.campaigns[0].originalManifestSha256);assert.equal(report.sources.admissionManifestSha256,recipe.campaigns[0].admissionManifestSha256);
 assert.equal(report.totals.attempted,8);assert.equal(report.totals.completed,8);assert.equal(report.totals.strictSuccesses,6);assert.equal(report.totals.total,1839202);assert.equal(report.totals.cost.state,'unknown');
 assert.equal(admission.recordedArms,8);assert.equal(admission.parity.totalTokens,report.totals.total);assert.equal(admission.parity.strictSuccesses,6);
 assert.deepEqual(report.comparisons,pairedStatistics(report.rows,report.profiles.map(p=>p.id)));
 assert.deepEqual(report.comparisons.map(c=>c.successes),[{A:2,B:2},{A:1,B:1}]);
 for(const row of report.rows){assert.equal(row.input+row.output,row.total);assert.ok(row.cachedInput<=row.input);assert.equal(report.workflowObservations.find(o=>o.id===row.id).providerReceiptSha256,row.completionEvidence.provider);}
 const failed=report.rows.filter(r=>!r.success);assert.deepEqual(failed.map(r=>[r.id,r.holdoutPassed]),[['codex-gpt6-luna-medium-b2',19],['codex-gpt6-luna-medium-a2',17]]);
 assert.deepEqual(report.rows.find(r=>r.id==='codex-gpt6-luna-medium-a2').reasoningOutput,{state:'known',value:0});
 assert.deepEqual(report.workflowObservations.filter(o=>o.fullInstructionReadDetected).map(o=>o.id),['devin-swe2-high-b1','devin-swe2-high-b2']);
 assert.deepEqual(report.workflowObservations.find(o=>o.id==='codex-gpt6-luna-medium-b2').failedHoldoutAssertions,['update-undefined-preserves']);
 assert.equal(report.workflowObservations.reduce((n,o)=>n+o.unverifiedSemanticMarkers,0),0);
 assert.equal(report.workflowObservations.reduce((n,o)=>n+o.unexplainedMarkedSourceRepeats,0),0);
 assert.doesNotMatch(JSON.stringify(report),/C:\\|C:\/Users|account[_-]?id|access[_-]?token|api[_-]?key/i);
});
