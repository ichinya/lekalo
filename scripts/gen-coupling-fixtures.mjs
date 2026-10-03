#!/usr/bin/env node
// Deliberate fixture authoring. Release gates never call this writer.
import { readFileSync,writeFileSync,mkdirSync,cpSync,existsSync,unlinkSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve,dirname,join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const home=join(root,'tests/fixtures/coupling');
const binary=join(root,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
if(!existsSync(binary))throw new Error('binary-missing');
const sha=b=>'sha256:'+createHash('sha256').update(b).digest('hex');
const canonical=v=>JSON.stringify(sort(v));
function sort(v){return Array.isArray(v)?v.map(sort):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a<b?-1:1).map(([k,v])=>[k,sort(v)])):v;}
const write=(path,value)=>{mkdirSync(dirname(join(home,path)),{recursive:true});writeFileSync(join(home,path),typeof value==='string'?value:JSON.stringify(value,null,2)+'\n');};
const run=(argv)=>{const p=spawnSync(binary,['--json',...argv],{cwd:root,encoding:'utf8',maxBuffer:32*1024*1024});if(p.status!==0)throw new Error(JSON.stringify({argv,status:p.status,stderr:p.stderr.slice(0,1500)}));return JSON.parse(p.stdout);};
const version='0.6.4',unknown={state:'unknown'},known=value=>({state:'known',value});
const def=(id,kind,extra={})=>({id,kind,version:1,...extra});
const field=(name,type,required=true)=>({name,type,required});
function yaml(v,n=0){const pad=' '.repeat(n);if(Array.isArray(v))return v.map(x=>typeof x==='object'?`${pad}-\n${yaml(x,n+2)}`:`${pad}- ${JSON.stringify(x)}\n`).join('');return Object.entries(v).filter(([,x])=>!Array.isArray(x)||x.length>0).map(([k,x])=>x&&typeof x==='object'?`${pad}${k}:\n${yaml(x,n+2)}`:`${pad}${k}: ${JSON.stringify(x)}\n`).join('');}
function document(p,v){write(p.replace(/\.json$/,'.yaml'),yaml(v));if(existsSync(join(home,p)))unlinkSync(join(home,p));}
function model(path,project,modules){document(`${path}/lekalo/project.json`,{schema_version:'0.2.16',definitions:[def(project,'project')]});for(const [module,data] of Object.entries(modules))for(const [kind,definitions] of Object.entries(data))document(`${path}/lekalo/modules/${module}/${kind}.json`,{schema_version:'0.2.16',definitions});}
model('taskhub','taskhub',{
 tasks:{module:[def('tasks','module')],entities:[def('tasks.text','scalar',{base:'string'}),def('tasks.count','scalar',{base:'number'}),def('tasks.state','enum',{values:['backlog','focused','done'].map(value=>({value}))}),def('tasks.task','entity',{fields:[field('task_id','tasks.text'),field('title','tasks.text'),field('state','tasks.state')],identity:['task_id']}),def('tasks.new_task_input','value-object',{fields:[field('task_id','tasks.text'),field('title','tasks.text')]}),def('tasks.task_page','value-object',{fields:[field('total','tasks.count'),field('open','tasks.count'),field('items','list<tasks.task>')]} )]},
 integrations:{module:[def('integrations','module')],entities:[def('integrations.text','scalar',{base:'string'}),def('integrations.target_kind','enum',{values:['calendar','webhook'].map(value=>({value}))}),def('integrations.calendar_payload','value-object',{fields:[field('calendar_id','integrations.text'),field('date','integrations.text'),field('title','integrations.text')]}),def('integrations.webhook_payload','value-object',{fields:[field('endpoint','integrations.text'),field('body','integrations.text')]}),def('integrations.client','value-object',{fields:[field('kind','integrations.target_kind'),field('endpoint','integrations.text')]}),def('integrations.delivery_ack','value-object',{fields:[field('target','integrations.text'),field('accepted_at','integrations.text')]})],commands:[def('integrations.deliver_calendar','command',{input:[field('payload','integrations.calendar_payload')],effects:[]}),def('integrations.deliver_webhook','command',{input:[field('payload','integrations.webhook_payload')],effects:[]})]},
 sync:{module:[def('sync','module',{imports:['tasks','integrations']})],entities:[def('sync.request','value-object',{fields:[field('task','tasks.task'),field('target','integrations.target_kind')]}),def('sync.local_request','value-object',{visibility:'module',fields:[field('request','sync.request')]} )],commands:[def('sync.sync_event','command',{input:[field('request','sync.local_request'),field('client','integrations.client')],effects:[]}),def('sync.to_payload','command',{input:[field('request','sync.request')],effects:[]})]},
});
const ops=[...readFileSync(join(root,'crates/lekalo-core/src/provider/operations.rs'),'utf8').matchAll(/^\s+id: "([a-z.]+)",/gm)].map(m=>m[1]);
model('provider','workflow_provider',{
 provider:{module:[def('provider','module')],entities:[def('provider.text','scalar',{base:'string'}),def('provider.manifest','value-object',{fields:[field('provider_id','provider.text'),field('protocol_version','provider.text'),field('operations','list<provider.text>')]} )],commands:ops.map(id=>def('provider.'+id.replaceAll('.','_'),'command',{input:[field('manifest','provider.manifest')],effects:[]}))},
});
const planner=['coupling','--symbol','planner.task','--project','tests/fixtures/coupling/planner'];
const report=run(planner).coupling;
write('golden/planner.report.json',report);write('golden/advisory.profile.json',report.profile);
const comparison=run([...planner,'--baseline',join(home,'golden/planner.report.json')]).couplingComparison;
write('golden/planner.comparison.json',comparison);
const change={schemaVersion:`lekalo/coupling-change-input/v${version}`,identity:`dev.lekalo.coupling-change-input@${version}`,mode:'worktree',baseRevision:unknown,candidateRevision:unknown,entries:[{symbolIds:['planner.task'],members:[],path:'lekalo/modules/planner/entities.yaml',fromPath:unknown,change:'modified',evidence:'canonical'}]};
write('golden/planner.change-input.json',change);
const manifest=JSON.parse(readFileSync(join(root,'tests/fixtures/artifacts/wire/valid/one-generated.manifest.json'),'utf8'));
manifest.inputs.ir.digest=report.provenance.inputs.irDigest;manifest.inputs.model.digest=report.provenance.inputs.modelDigest.value;
delete manifest.manifest_digest;manifest.manifest_digest=sha(canonical(manifest));
const artifactBytes=canonical(manifest)+'\n';
// Fully confirmed owner-validated trace. Current revision is explicit and must
// agree with the caller; no Git parser, native run, or fake provider execution.
const sourceRevision=report.provenance.inputs.semanticDigest.slice(7);
const trace={schemaVersion:'lekalo/trace-manifest/v0.2.16',identity:'dev.lekalo.trace-manifest@0.2.16',manifestId:'coupling-planner-trace',projectRef:'planner',completeness:'full',sourceRevision,modelRef:{schemaVersion:'0.2.16',digest:report.provenance.inputs.modelDigest.value},irRef:{schemaVersion:'0.2.16',digest:report.provenance.inputs.irDigest},graphRef:{schemaVersion:'0.2.16',digest:report.provenance.inputs.graphDigest},exportProfile:'requirement-to-gate',nodes:[{nodeId:'requirement:COUPLING-REQ-001',nodeKind:'requirement',requirementId:'COUPLING-REQ-001'},{nodeId:'symbol:planner.focus_task',nodeKind:'symbol',semanticId:'planner.focus_task',contractVersion:'0.2.16'},{nodeId:'test:planner-focus',nodeKind:'native_test',testId:'planner.test.focus',contractVersion:'0.2.16'},{nodeId:'gate:planner-focus',nodeKind:'gate',gateId:'planner.gate.focus',contractVersion:'0.2.16'}],relations:[],gaps:[]};
for(const [relationKind,fromNode,toNode] of [['implements','symbol:planner.focus_task','requirement:COUPLING-REQ-001'],['verifies','test:planner-focus','symbol:planner.focus_task'],['evidences','gate:planner-focus','test:planner-focus']]){
 const occurrence='coupling.fixture';
 const relationId=sha(JSON.stringify(['lekalo/trace-manifest/v0.2.16/relation',relationKind,fromNode,toNode,occurrence]));
 trace.relations.push({relationId,relationKind,fromNode,toNode,occurrence,provenance:{origin:'declared',sourceSystem:'lekalo',sourceRevision,sourceDigest:report.provenance.inputs.semanticDigest,recordedBy:'lekalo.core'},confidence:'exact',status:'confirmed',evidenceRefs:['gate:planner-focus']});
}
const traceBytes=JSON.stringify(trace);
const evidence={schemaVersion:`lekalo/coupling-evidence/v${version}`,identity:`dev.lekalo.coupling-evidence@${version}`,inputs:{...report.provenance.inputs,sourceRevision:known(sourceRevision)},artifacts:known({digest:sha(artifactBytes),bytes:artifactBytes}),trace:known({digest:sha(traceBytes),bytes:traceBytes}),queries:unknown,transactions:unknown,replicas:[]};
write('golden/planner.evidence.json',evidence);
write('golden/planner.evidence-report.json',run([...planner,'--source-revision',sourceRevision,'--evidence',join(home,'golden/planner.evidence.json')]).coupling);
write('golden/taskhub.report.json',run(['coupling','--symbol','tasks.task','--project','tests/fixtures/coupling/taskhub']).coupling);
write('golden/provider.report.json',run(['coupling','--symbol','provider.manifest','--project','tests/fixtures/coupling/provider']).coupling);
const negative=structuredClone(report);negative.subjects[0].metrics.fanInSymbols={state:'known',value:999};write('invalid/forged-count.report.json',negative);
const unknownField=structuredClone(report.profile);unknownField.extra=true;write('invalid/unknown-member.profile.json',unknownField);
const evidenceBad=structuredClone(evidence);evidenceBad.inputs.irDigest='sha256:'+'0'.repeat(64);write('invalid/stale-input.evidence.json',evidenceBad);
const comparisonBad=structuredClone(comparison);comparisonBad.rows[0].absolute={state:'unknown',value:0};write('invalid/unavailable-value.comparison.json',comparisonBad);
const changeBad=structuredClone(change);changeBad.entries[0].path='../secret.txt';write('invalid/traversal.change-input.json',changeBad);
const refs=[['tests/fixtures/context-budget/planner','planner.task / planner.focus_task'],['tests/fixtures/pilot/taskhub/packages/tasks/src/index.ts','Task, TASK_STATE, TaskPage, NewTaskInput'],['tests/fixtures/pilot/taskhub/packages/integrations/src/index.ts','IntegrationClient, CalendarSyncPayload, WebhookSyncPayload, DeliveryAck'],['tests/fixtures/pilot/taskhub/packages/sync/src/index.ts','SyncRequest, toPayload, syncEvent'],['crates/lekalo-core/src/provider/operations.rs','OPERATIONS'],['contracts/provider-capabilities.schema.v0.6.4.json','provider manifest public boundary']];
write('README.md',`# Coupling synthetic conformance corpus\n\nPlanner is copied from the accepted context-budget workload. Taskhub and provider are authored semantic reference projections of the existing synthetic fixtures and production provider contract. They are not scanner output or measurements of the repository implementation.\n\n| Reference | Searchable anchors | Bytes SHA-256 |\n| --- | --- | --- |\n${refs.map(([p,a])=>`| \`${p}\` | \`${a}\` | ${p.endsWith('planner')?'directory copy, byte parity checked by gate':sha(readFileSync(join(root,p)))} |`).join('\n')}\n\nTaskPage.at is represented by an items reference to Task. SyncPayload union alternatives remain separate records because Model has no union type. Callable argument footprints use command inputs without declared writes. The local_request bridge is an explicit synthetic stress case. The normalized manifest has a shared operation input; it does not advertise coupling in the live provider. These limitations are intentional visible projections. transitionTask returns a copy and this corpus declares no shared mutable writes for it.\n\nEvidence fixtures rebind current IR/graph/effect pins and pass each attachment owner's parser. Native trace rows are synthetic confirmed declarations; no native test or provider was executed. The fixture revision is the semantic digest used as an explicit current revision alias. Golden report/comparison/profile/evidence/change-input payloads are produced by the built CLI; invalid vectors test closed decoding and semantic joins. Regenerate deliberately with \`node scripts/gen-coupling-fixtures.mjs\`; gates remain read-only.\n`);
cpSync(join(home,'planner'),join(root,'crates/lekalo-core/tests/fixtures/coupling/planner'),{recursive:true});
console.log(JSON.stringify({ok:true,fixtureFamily:'coupling',contractFamilies:5,sourceRevision}));
