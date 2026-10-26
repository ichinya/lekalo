#!/usr/bin/env node
// Controlled #167 runner. Preparing does not invoke a model. Running is explicit.
import {semanticToolSource} from './framework-lift-context.mjs';
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {spawn,spawnSync} from 'node:child_process';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {finished} from 'node:stream/promises';
import {sha,write,inventory,exec,repo} from './framework-lift-rounding.mjs';
import {LEDGER_PATH,sourceReadInstructions,workspaceToolSource} from './framework-lift-workspace.mjs';

function tree(root,prefix=''){return fs.readdirSync(join(root,prefix),{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(e=>{if(e.name==='.git')return [];const p=prefix?prefix+'/'+e.name:e.name;return e.isDirectory()?tree(root,p):[{path:p,sha256:sha(fs.readFileSync(join(root,p))),bytes:fs.statSync(join(root,p)).size}];});}
const save=(root,p,v)=>write(root,p,v);
function semanticTool(binary){return semanticToolSource({binary,symbol:'pricing.discount'});}

export function configure(root,options){
  const p=JSON.parse(fs.readFileSync(join(root,'preparation.json'),'utf8'));
  const builderPath=p.builderPath??'scripts/research/framework-lift-rounding.mjs';
  assert.equal(p.builderDigest,sha(fs.readFileSync(join(repo,builderPath))));
  assert.ok(fs.existsSync(options.devin)&&fs.existsSync(options.codex));
  assert.equal(fs.existsSync(join(root,'campaign.json')),false,'do not overwrite an existing campaign');
  const profiles=options.profiles??[{id:'devin-swe2-high',cli:'devin',model:'swe-2-high',version:'3000.11.205'},{id:'codex-gpt6-luna-medium',cli:'codex',model:'gpt-6-luna',reasoning:'medium',version:'0.164.0'}];
  const slots=[];
  for(const profile of profiles)for(const [arm,repetition]of [['A',1],['B',1],['B',2],['A',2]]){
    const id=profile.id+'-'+arm.toLowerCase()+repetition,workspace=join(root,'runs',id,'workspace');
    fs.cpSync(join(root,arm==='B'?'adopted':'baseline'),workspace,{recursive:true});
    for(const native of p.nativeFiles)assert.equal(sha(fs.readFileSync(join(workspace,native.path))),native.sha256);
    assert.equal(sha(fs.readFileSync(join(workspace,'tools/workspace.mjs'))),sha(workspaceToolSource()),'prepare with the current source reader');
    const common='Work only in this disposable synthetic repository. Keep inference and subprocess work scoped to this task. Source reading tools and accounting are identical in both arms. '+sourceReadInstructions+' Do not inspect sibling directories or parent projects.\n';
    if(arm==='B'&&options.agentInstructionPath)assert.ok(fs.existsSync(join(workspace,options.agentInstructionPath)),'install the instruction during preparation');
    const extra=arm==='B'?(options.agentInstructionPath?'Read the installed Lekalo instruction '+options.agentInstructionPath+' once within this run and follow it; its reading cost is part of B. ':'')+'Lekalo-assisted workflow: before source reads, run node tools/semantic.mjs once for a bounded project capsule. Reuse its facts to select required source ranges; inspect implementation only where that evidence is insufficient. Do not run overlapping projections, dump the Model, or read all files as a checklist. Native inventory is not a behavior contract. Model maintenance is optional in this one-change pilot; report stale or unsupported evidence honestly.\n':'Use the ordinary native workflow and the available native tools.\n';
    save(workspace,'AGENTS.md',common+extra);
    if(arm==='B')save(workspace,'tools/semantic.mjs',options.semanticToolSource??semanticTool(p.binary));
    const initial=tree(workspace);
    const hooks=join(root,'empty-hooks');fs.mkdirSync(hooks,{recursive:true});
    exec('git',['init','--quiet'],workspace);
    exec('git',['-c','core.hooksPath='+hooks,'add','--all'],workspace);
    exec('git',['-c','core.hooksPath='+hooks,'-c','user.name=Benchmark','-c','user.email=benchmark@example.invalid','-c','commit.gpgsign=false','commit','--quiet','-m','Frozen synthetic baseline'],workspace);
    save(root,'runs/'+id+'/initial-tree.json',initial);
    save(root,'runs/'+id+'/prompt.txt',common+extra+'\n'+fs.readFileSync(join(root,'task.txt'),'utf8'));
    slots.push({id,profile:profile.id,arm,repetition,workspace,promptDigest:sha(fs.readFileSync(join(root,'runs',id,'prompt.txt'))),initialTreeDigest:sha(JSON.stringify(initial))});
  }
  save(root,'builder.snapshot.mjs',fs.readFileSync(join(repo,builderPath),'utf8'));
  save(root,'runner.snapshot.mjs',fs.readFileSync(fileURLToPath(import.meta.url),'utf8'));
  const config={schema:'lekalo-167-live-campaign/2',createdAt:new Date().toISOString(),task:p.task??'rounding-supported-top-level-v1',sourceCommit:p.sourceCommit,builderDigest:p.builderDigest,runnerDigest:sha(fs.readFileSync(fileURLToPath(import.meta.url))),taskDigest:sha(fs.readFileSync(join(root,'task.txt'))),oracleDigest:sha(fs.readFileSync(join(root,'oracle.mjs'))),nativeDigest:p.nativeDigest,modelDigest:p.modelDigest,irDigest:p.irDigest,profiles,slots,paths:{binary:p.binary,tsc:p.tsc,devin:options.devin,codex:options.codex},executables:{node:sha(fs.readFileSync(process.execPath)),binary:sha(fs.readFileSync(p.binary)),tsc:sha(fs.readFileSync(p.tsc)),devin:sha(fs.readFileSync(options.devin)),codex:sha(fs.readFileSync(options.codex)),adapter:sha(fs.readFileSync(join(repo,'adapters/node-typescript/adapter.mjs'))),manifest:sha(fs.readFileSync(join(repo,'adapters/node-typescript/adapter.manifest.json')))},runtime:{node:process.version,platform:process.platform,architecture:process.arch},budget:{authorization:'user approved unlimited spending and repeating the revised experiment',monetaryCap:null,maxAutomaticReruns:0,perRunMs:1800000},measurement:{order:'A1 B1 B2 A2 within each profile; not randomized',timing:'agent CLI start to process close; post-run independent grading excluded',coldAdoption:'separate preparation receipt; authoring labor unknown',nativeRead:'delivered-content markers verified from actual tool observations; unclassified source delivery stays unknown',usage:'CLI-reported aggregates; cached input subset of input; never add subsets twice',serverModelRevision:'unknown; exact requested model alias pinned',sampling:'provider-managed defaults not fully exposed; no fabricated temperature/topP/maxOutputTokens',money:'unknown unless provider reports charge',acceptance:'public tests, native TypeScript, all 20 holdout rows, CLI success and unchanged non-allowed paths',retries:'no reruns of any final slot; all failures remain in denominators',context:'B requests one bounded capsule and grouped source pointers; repeated unchanged output is suppressed within the agent context. Impact is requested only when additional risks/gates are needed. No pre-attached semantic output.',limits:p.limitations??['One warm code change; cold authoring tokens unknown.']},officialDocs:['https://learn.chatgpt.com/docs/non-interactive-mode','https://docs.devin.ai/cli/reference/commands']};
  config.execReadPrefixes=options.execReadPrefixes??[];
  config.collectorPaths=[LEDGER_PATH];
  config.sourceReaderDigest=sha(workspaceToolSource());
  config.agentInstructionPath=options.agentInstructionPath??null;
  config.agentInstructionDigest=options.agentInstructionPath?sha(fs.readFileSync(join(root,'adopted',options.agentInstructionPath))):null;
  config.measurement.acceptance+='; final agent response required and no non-interactive confirmation rejection';
  config.predecessorCampaign=options.predecessorCampaign??null;
  save(root,'campaign.json',config);save(root,'campaign.sha256',sha(fs.readFileSync(join(root,'campaign.json')))+'\n');
  return {root,slots:slots.length,digest:sha(fs.readFileSync(join(root,'campaign.json')))};
}
function nativeAssessment(root,slot,c){
  const dir=join(root,'runs',slot.id),r=(name,args)=>{const x=spawnSync(process.execPath,args,{cwd:slot.workspace,encoding:'utf8',timeout:120000,maxBuffer:16e6,windowsHide:true});save(dir,name+'.stdout.txt',x.stdout??'');save(dir,name+'.stderr.txt',x.stderr??'');return x;};
  const tests=r('public-tests',['--test','test/public.test.mjs']),typecheck=r('typecheck',[c.paths.tsc,'-p','tsconfig.json']),oracle=r('holdout',[join(root,'oracle.mjs'),slot.workspace]);
  let grade;try{grade=JSON.parse(oracle.stdout)}catch{grade={passed:null,total:20,invalidReceipt:true};}
  const before=JSON.parse(fs.readFileSync(join(dir,'initial-tree.json'),'utf8')),after=tree(slot.workspace),old=new Map(before.map(x=>[x.path,x.sha256])),now=new Map(after.map(x=>[x.path,x.sha256]));
  const changed=[...new Set([...old.keys(),...now.keys()])].filter(p=>old.get(p)!==now.get(p));
  const unexpected=changed.filter(p=>!p.startsWith('src/')&&!(slot.arm==='B'&&p.startsWith('lekalo/'))&&!(c.collectorPaths??[]).includes(p));
  const diff=spawnSync('git',['diff','--binary','HEAD'],{cwd:slot.workspace,encoding:'utf8',windowsHide:true,maxBuffer:16e6});
  save(dir,'candidate.patch',diff.stdout??'');save(dir,'candidate-tree.json',after);
  return {publicExit:tests.status,typecheckExit:typecheck.status,holdoutExit:oracle.status,holdout:grade,changed,unexpected,accepted:tests.status===0&&typecheck.status===0&&oracle.status===0&&unexpected.length===0};
}
export function observations(dir,cli){
  if(cli==='codex'){
    const events=fs.readFileSync(join(dir,'stdout.jsonl'),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l));
    return events.filter(e=>e.type==='item.completed'&&e.item?.type==='command_execution').map(e=>({kind:'exec',command:e.item.command,content:e.item.aggregated_output??'',exit:e.item.exit_code}));
  }
  const tr=JSON.parse(fs.readFileSync(join(dir,'trajectory.json'),'utf8')),out=[];
  for(const step of tr.steps??[])for(const call of step.tool_calls??[]){
    const responses=(step.observation?.results??[]).filter(x=>x.source_call_id===call.tool_call_id);
    out.push({kind:call.function_name,arguments:call.arguments,command:call.arguments?.command,content:responses.map(x=>x.content??'').join('\n')});
  }return out;
}
export function usage(dir,cli){
  if(cli==='codex'){
    const events=fs.readFileSync(join(dir,'stdout.jsonl'),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l)),u=events.filter(e=>e.type==='turn.completed').map(e=>e.usage);
    const sum=k=>u.length&&u.every(x=>Number.isFinite(x[k]))?u.reduce((a,x)=>a+x[k],0):null;
    return {source:'Codex turn.completed.usage',input:sum('input_tokens'),output:sum('output_tokens'),cachedInput:sum('cached_input_tokens'),reasoningOutput:sum('reasoning_output_tokens'),turns:u.length,toolCalls:events.filter(e=>e.type==='item.completed'&&['command_execution','file_change','mcp_tool_call','web_search'].includes(e.item?.type)).length,monetaryCharge:null};
  }
  const tr=JSON.parse(fs.readFileSync(join(dir,'trajectory.json'),'utf8')),m=tr.final_metrics??{};
  return {source:'Devin ATIF final_metrics',input:m.total_prompt_tokens??null,output:m.total_completion_tokens??null,cachedInput:m.total_cached_tokens??null,reasoningOutput:null,toolCalls:(tr.steps??[]).reduce((n,s)=>n+(s.tool_calls?.length??0),0),monetaryCharge:null};
}
export function markedReads(obs){
  const rows=[],unverified=[];
  for(let i=0;i<obs.length;i++){
    const text=obs[i].content;
    const regex=/@@native-content ([^\r\n]+)\r?\n([\s\S]*?)@@native-end ([^\r\n]+)/g;
    for(const match of text.matchAll(regex)){
      try{const receipt=JSON.parse(match[1]),bodies=[match[2],match[2].replaceAll('\r\n','\n')];if(receipt.path!==match[3].trim()||!bodies.some(body=>Buffer.byteLength(body)===receipt.bytes&&sha(Buffer.from(body))===receipt.sha256))unverified.push({observation:i,path:receipt.path,reason:'marker bytes or digest mismatch'});else rows.push({observation:i,...receipt});}catch{unverified.push({observation:i,reason:'invalid marker'});}
    }
  }
  const covered=new Map(),overlaps=[];
  for(const row of rows){if(!row.sourceDigest||!Number.isInteger(row.startLine)||row.startLine<1||!Number.isInteger(row.endLine)||row.endLine<row.startLine||row.endLine>1000000)continue;const key=row.path+'#'+row.sourceDigest+'#'+(row.session??'unknown'),seen=covered.get(key)??new Set(),duplicate=[];for(let n=row.startLine;n<=row.endLine;n++){if(seen.has(n))duplicate.push(n);seen.add(n);}covered.set(key,seen);if(duplicate.length)overlaps.push({observation:row.observation,path:row.path,sourceDigest:row.sourceDigest,lines:duplicate,reason:row.replayReason??null});}
  return {verifiedMarkers:rows,uniqueSourceFiles:[...new Set(rows.map(r=>r.path))].sort(),deliveredBytes:rows.reduce((n,r)=>n+r.bytes,0),unverified,overlappingSourceRanges:overlaps,unexplainedRepeatedRanges:overlaps.filter(r=>!r.reason),exhaustive:{state:'unknown',reason:'Only emitted and verified marker content is counted; inspect native Read/Edit/diff outputs and truncation independently.'}};
}
export function completionAudit(dir,cli){
  const stderr=fs.readFileSync(join(dir,'stderr.txt'),'utf8');
  const confirmationRejected=/rejected a tool call that requires confirmation/i.test(stderr);
  if(cli==='devin'){
    const tr=JSON.parse(fs.readFileSync(join(dir,'trajectory.json'),'utf8')),last=tr.steps.at(-1);
    const rejected=(tr.steps??[]).some(s=>(s.observation?.results??[]).some(r=>/Tool execution was rejected by the user/.test(r.content??'')));
    const finalResponse=last?.source==='agent'&&!(last.tool_calls?.length)&&Boolean(last.message?.trim());
    return {complete:finalResponse&&!confirmationRejected&&!rejected,finalResponse,confirmationRejected:confirmationRejected||rejected};
  }
  const events=fs.readFileSync(join(dir,'stdout.jsonl'),'utf8').split(/\r?\n/).filter(Boolean).map(l=>JSON.parse(l));
  const finalResponse=events.some(e=>e.type==='item.completed'&&e.item?.type==='agent_message'&&e.item.text?.trim());
  const turnCompleted=events.some(e=>e.type==='turn.completed'),turnFailed=events.some(e=>e.type==='turn.failed');
  return {complete:finalResponse&&turnCompleted&&!turnFailed&&!confirmationRejected,finalResponse,turnCompleted,turnFailed,confirmationRejected};
}
export async function run(root){
  const bytes=fs.readFileSync(join(root,'campaign.json')),c=JSON.parse(bytes);
  assert.equal(sha(bytes),fs.readFileSync(join(root,'campaign.sha256'),'utf8').trim());
  assert.equal(c.runnerDigest,sha(fs.readFileSync(fileURLToPath(import.meta.url))),'runner changed after freeze');
  if(c.sourceReaderDigest)assert.equal(c.sourceReaderDigest,sha(workspaceToolSource()),'source reader changed after freeze');
  assert.equal(fs.existsSync(join(root,'status.json')),false,'campaign already started; no silent resume/rerun');
  for(const [key,p]of Object.entries({node:process.execPath,...c.paths,adapter:join(repo,'adapters/node-typescript/adapter.mjs'),manifest:join(repo,'adapters/node-typescript/adapter.manifest.json')}))assert.equal(sha(fs.readFileSync(p)),c.executables[key],'executable changed '+key);
  const startedAt=new Date().toISOString(),results=[];let current=null;
  const status=()=>save(root,'status.json',{startedAt,updatedAt:new Date().toISOString(),scheduled:c.slots.length,completed:results.length,current,results});
  status();
  for(const slot of c.slots){
    const dir=join(root,'runs',slot.id),profile=c.profiles.find(p=>p.id===slot.profile);
    assert.equal(sha(fs.readFileSync(join(root,'task.txt'))),c.taskDigest);assert.equal(sha(fs.readFileSync(join(root,'oracle.mjs'))),c.oracleDigest);
    assert.equal(sha(fs.readFileSync(join(dir,'prompt.txt'))),slot.promptDigest);
    assert.equal(sha(JSON.stringify(tree(slot.workspace))),slot.initialTreeDigest,'initial workspace changed');
    const env={...process.env};
    for(const k of Object.keys(env))if(/^(CODEX_THREAD_ID|CODEX_SESSION_ID|CODEX_TOOL_CALL_ID|GH_TOKEN|GITHUB_TOKEN|NPM_TOKEN|AWS_|AZURE_|OPENAI_API_KEY|CODEX_API_KEY|LEKALO_|DEVIN_MODEL|DEVIN_CONFIG)/.test(k))delete env[k];
    const entries=(process.env.Path??process.env.PATH??'').split(process.platform==='win32'?';':':').filter(p=>p&&!fs.existsSync(join(p,'lekalo.exe'))&&!fs.existsSync(join(p,'lekalo.cmd'))&&!fs.existsSync(join(p,'lekalo')));
    for(const k of Object.keys(env))if(k.toLowerCase()==='path')delete env[k];
    env[process.platform==='win32'?'Path':'PATH']=[...entries,...(slot.arm==='B'?[dirname(c.paths.binary)]:[])].join(process.platform==='win32'?';':':');env.DEVIN_REFUSAL_FALLBACK='';
    let program,args,outfile;
    if(profile.cli==='devin'){
      const config={agent:{model:profile.model},subagents_enabled:false,auto_update:false,notify:'never',attribution:false,read_config_from:{agents_standard:false,cursor:false,windsurf:false,claude:false,copilot:false,opencode:false,zed:false,cloud:false},permissions:{allow:['Read(**)','Write(**)','Exec(node)','Exec(git)','Exec(lekalo)',...(c.execReadPrefixes??[]).map(p=>'Exec('+p+')')],deny:[],ask:[]}};
      save(dir,'devin-config.json',config);program=c.paths.devin;args=['--config',join(dir,'devin-config.json'),'--model',profile.model,'--permission-mode','accept-edits','--respect-workspace-trust','false','--export',join(dir,'trajectory.json'),'--prompt-file',join(dir,'prompt.txt'),'--print'];outfile='stdout.txt';
    }else{
      program=process.execPath;args=[c.paths.codex,'-a','on-request','exec','--approve-for-me','--ignore-user-config','--ignore-rules','--ephemeral','-c','default_permissions=":workspace"','--cd',slot.workspace,'--model',profile.model,'-c','model_reasoning_effort="medium"','-c','project_doc_max_bytes=0','-c','features.multi_agent=false','--json','-o',join(dir,'final-message.txt'),'-'];outfile='stdout.jsonl';
    }
    const invocation={startedAt:new Date().toISOString(),program,args,cwd:slot.workspace,promptDigest:slot.promptDigest,profile,auth:'Existing CLI account login; API key environment removed',containment:profile.cli==='codex'?'workspace sandbox and automatic approval review':'CLI permissions on disposable public fixture; no Windows OS containment claim'};
    save(dir,'invocation.json',invocation);
    current={id:slot.id,startedAt:invocation.startedAt,phase:'agent'};status();console.log(JSON.stringify({event:'start',id:slot.id}));
    const out=fs.createWriteStream(join(dir,outfile),{flags:'wx'}),err=fs.createWriteStream(join(dir,'stderr.txt'),{flags:'wx'}),before=Date.now();
    const result=await new Promise(resolveRun=>{
      let timedOut=false;
      const child=spawn(program,args,{cwd:slot.workspace,env,windowsHide:true,stdio:['pipe','pipe','pipe']});
      current.pid=child.pid;status();child.stdout.pipe(out);child.stderr.pipe(err);
      const timer=setTimeout(()=>{timedOut=true;if(child.pid)spawnSync('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true});},c.budget.perRunMs);
      child.on('error',e=>{clearTimeout(timer);resolveRun({exitCode:null,error:e.message,timedOut});});
      child.on('close',(code,signal)=>{clearTimeout(timer);resolveRun({exitCode:code,signal,timedOut});});
      child.stdin.on('error',()=>{});
      child.stdin.end(profile.cli==='codex'?fs.readFileSync(join(dir,'prompt.txt'),'utf8'):undefined);
    });
    const durationMs=Date.now()-before;await Promise.all([finished(out),finished(err)]);
    current.phase='independent-grading';status();
    const grade=nativeAssessment(root,slot,c);let tokens,obs,reads;
    try{tokens=usage(dir,profile.cli);obs=observations(dir,profile.cli);reads=markedReads(obs);}catch(e){tokens={state:'unknown',reason:e.message};obs=[];reads={exhaustive:{state:'unknown',reason:'missing/invalid export'}};}
    save(dir,'tool-observations.private.json',obs);save(dir,'marked-reads.json',reads);
    const completion=completionAudit(dir,profile.cli);
    const row={id:slot.id,profile:slot.profile,arm:slot.arm,repetition:slot.repetition,startedAt:invocation.startedAt,completedAt:new Date().toISOString(),durationMs,cli:result,grade,completion,usage:tokens,readMarkers:reads.uniqueSourceFiles??null,success:result.exitCode===0&&!result.timedOut&&grade.accepted&&completion.complete,finalSubmissions:completion.complete?1:0,postSubmissionFixCycles:completion.complete?0:null,humanInterventions:0};
    save(dir,'result.json',row);results.push(row);current=null;status();console.log(JSON.stringify({event:'end',id:slot.id,success:row.success,holdout:grade.holdout.passed+'/'+grade.holdout.total,durationMs,usage:tokens}));
    if(tokens.state==='unknown'||(!obs.length&&!grade.accepted)){console.log(JSON.stringify({event:'stopped',reason:'unusable executor receipts; remaining slots NOTRUN'}));break;}
  }
  save(root,'results.json',{campaignDigest:sha(bytes),completedAt:new Date().toISOString(),scheduled:c.slots.length,results});
  return {root,completed:results.length,scheduled:c.slots.length,successes:results.filter(r=>r.success).length};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  assert.ok(process.argv[2]==='--run'&&process.argv[3],'Use --run FROZEN_CAMPAIGN_ROOT; import configure() for non-inference preparation.');
  console.log(JSON.stringify(await run(resolve(process.argv[3]))));
}
