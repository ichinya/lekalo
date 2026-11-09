import assert from 'node:assert/strict';import {test} from 'node:test';
import {projectObservation,historyMetricMap} from './framework-lift-history-preview.mjs';
import {digest} from '../lib/framework-lift-executor.mjs';
const known=value=>({state:'known',value});
function fixture(){
 const campaign={campaignId:'public-fixture',profile:{executor:{id:'fixture-cli'},model:{id:'fixture-model'}}};
 const metrics=Object.fromEntries(Object.keys(historyMetricMap).map(k=>[k,{state:'unknown'}]));
 Object.assign(metrics,{durationMs:known(10),toolCalls:known(0),inputTokens:known(100),outputTokens:known(20),totalTokens:known(120),reasoningTokens:known(5),cachedInputTokens:known(80),fixCycles:known(0)});
 const arm={schemaVersion:'lekalo/framework-lift-arm/v1.1.0',profileDigest:digest(campaign.profile),campaignRef:{id:campaign.campaignId,digest:digest(campaign)},slot:{arm:'A',pairId:'pair-one',repetition:1},metrics,assertions:[{id:'native-acceptance',outcome:'pass'}]};
 const row={armDigest:known(digest(arm)),slot:structuredClone(arm.slot),metrics:structuredClone(metrics),status:'success',verifiedSuccess:true};
 return {arm,row,campaign};
}
test('compatibility projection preserves totals and does not re-add subset reasoning or cache',()=>{
 const {arm,row,campaign}=fixture(),before=JSON.stringify(arm),o=projectObservation(arm,row,campaign,'2026-10-26T10:00:00Z');
 assert.equal(JSON.stringify(arm),before);assert.deepEqual(o.metrics.tokens,{input:known(100),output:known(20),total:known(120),reasoning:{state:'unsupported'}});
 assert.deepEqual(o.metrics.filesRead,{state:'unknown'});assert.deepEqual(o.metrics.toolCalls,known(0));assert.equal(o.dataSensitivity,'internal');
 assert.ok(!Object.hasOwn(o.metrics,'cachedInputTokens'));assert.ok(!Object.hasOwn(o.metrics,'fixCycles'));
 assert.ok(o.measurementSources.every(s=>s.sourceId==='admitted-arm-'+digest(arm).slice(7)&&s.sourceKind==='derived'));
 assert.equal(o.assertions.rows.at(-1).outcome,'pass');
});
test('native-passing but incomplete executor remains infrastructure with an explicit completion assertion',()=>{
 const {arm,row,campaign}=fixture();row.status='infrastructure';row.verifiedSuccess=false;
 const o=projectObservation(arm,row,campaign,'2026-10-26T10:00:00Z');assert.equal(o.status.outcome,'infrastructure');assert.equal(o.status.coverageState,'incomplete');assert.equal(o.assertions.rows[0].outcome,'pass');assert.equal(o.assertions.rows.at(-1).outcome,'infrastructure');
});
test('unknown, withheld and unsupported metric states remain distinct',()=>{
 const {arm,row,campaign}=fixture();arm.metrics.filesRead={state:'withheld'};arm.metrics.filesChanged={state:'unsupported'};row.armDigest=known(digest(arm));row.metrics=structuredClone(arm.metrics);
 const o=projectObservation(arm,row,campaign,'2026-10-26T10:00:00Z');assert.deepEqual(o.metrics.filesRead,{state:'withheld'});assert.deepEqual(o.metrics.filesChanged,{state:'unsupported'});assert.deepEqual(o.metrics.retryCount,{state:'unknown'});
});
test('stale or contradictory arm/campaign/comparison evidence refuses',()=>{
 for(const mutate of [f=>f.row.armDigest=known('sha256:'+'0'.repeat(64)),f=>f.row.metrics.totalTokens=known(125),f=>f.row.slot.arm='B',f=>f.row.verifiedSuccess=false,f=>f.campaign.campaignId='other']){
  const f=fixture();mutate(f);assert.throws(()=>projectObservation(f.arm,f.row,f.campaign,'2026-10-26T10:00:00Z'));
 }
});
