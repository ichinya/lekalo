#!/usr/bin/env node
// #167 after-fix planner campaign preparation. No inference in this builder.
import {semanticToolSource} from './framework-lift-context.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {fileURLToPath} from 'node:url';
import {repo,sha,write,inventory,exec} from './framework-lift-rounding.mjs';
import {configure} from './framework-lift-runner.mjs';
import {workspaceToolSource} from './framework-lift-workspace.mjs';
const defaultBinary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':''));
const defaultTsc=process.env.LEKALO_TSC??join(repo,'adapters/node-typescript/node_modules/typescript/bin/tsc');
const adapter=join(repo,'adapters/node-typescript/adapter.mjs');
const fixture=join(repo,'tests/fixtures/public-typescript-scan/planner');
export function helper(binary=defaultBinary){return semanticToolSource({binary,symbol:'synthetic_priority_planner.task',note:'Canonical lowercase IDs and native proposal IDs are separate; no automatic join is claimed. Semantic fields use snake_case aliases.'});}

export function prepare(options={}){
 const binary=options.binary??defaultBinary,tsc=options.tsc??defaultTsc;
 const devin=options.devin??process.env.LEKALO_DEVIN,codex=options.codex??process.env.LEKALO_CODEX;
 for(const path of [binary,tsc,...(options.configure===false?[]:[devin,codex])])assert.ok(path&&fs.existsSync(path),'Set valid LEKALO_BIN/LEKALO_TSC/LEKALO_DEVIN/LEKALO_CODEX paths; preparation never installs tools.');
 const root=fs.mkdtempSync(join(tmpdir(),'lekalo-167-planner-read-once-')),baseline=join(root,'baseline'),adopted=join(root,'adopted'),gold=join(root,'reference');
 fs.cpSync(join(fixture,'baseline'),baseline,{recursive:true});
 const original=inventory(baseline);
 assert.deepEqual(original,JSON.parse(fs.readFileSync(join(fixture,'baseline-files.json'),'utf8')),'original task native bytes changed');
 write(baseline,'tools/workspace.mjs',workspaceToolSource());
 write(baseline,'tools/typecheck.mjs',"import {spawnSync} from 'node:child_process';const r=spawnSync(process.execPath,["+JSON.stringify(tsc)+",'-p','tsconfig.json'],{stdio:'inherit'});process.exitCode=r.status??1;\n");
 write(baseline,'.gitignore','.lekalo/cache/\n.benchmark/\n');
 const before=inventory(baseline);
 write(root,'task.txt',fs.readFileSync(join(fixture,'task.txt'),'utf8')+'\nUse node tools/typecheck.mjs for the pinned compiler. Do not invoke npx or package installation.\n');
 write(root,'oracle.mjs',fs.readFileSync(join(fixture,'oracle.mjs'),'utf8'));
 assert.equal(sha(fs.readFileSync(join(root,'oracle.mjs'))),sha(fs.readFileSync(join(fixture,'oracle.mjs'))));
 exec(process.execPath,['--test','test/public.test.mjs'],baseline);exec(process.execPath,[tsc,'-p','tsconfig.json'],baseline);
 fs.cpSync(baseline,adopted,{recursive:true});
 const receipts=[];
 function cli(args,json=true,expected=0){const t=performance.now(),r=exec(binary,[...(json?['--json']:[]),'--no-cache',...args,'--project','.'],adopted,expected);receipts.push({args,exit:r.status,durationMs:performance.now()-t,stdout:r.stdout,stderr:r.stderr});return json?JSON.parse(r.stdout):r.stdout;}
 const start=performance.now();
 cli(['init','--adopt','--project-id','native_planner']);cli(['module','new','synthetic_priority_planner']);
 for(const p of ['lekalo/project.yaml','lekalo/modules/synthetic_priority_planner/module.yaml'])write(adopted,p,fs.readFileSync(join(adopted,p),'utf8').replaceAll('0.2.16','0.8.0'));
 const prefix='synthetic_priority_planner';
 for(const name of ['entities.yaml','queries.yaml']){
   const doc=JSON.parse(fs.readFileSync(join(fixture,'semantic',name),'utf8'));
   let s=JSON.stringify(doc).replaceAll('0.2.16','0.8.0').replaceAll('planner.task',prefix+'.task').replaceAll('planner.text',prefix+'.text').replaceAll('planner.list_tasks',prefix+'.list_tasks');
   write(adopted,'lekalo/modules/'+prefix+'/'+name,JSON.parse(s));
 }
 write(adopted,'lekalo/modules/'+prefix+'/scenarios.yaml',{schema_version:'0.8.0',definitions:[
  {id:prefix+'.task_lifecycle',kind:'scenario',version:1,summary:'Creation/update in task-service.ts, JSON boundaries in task-json.ts and detached workspace-isolated MemoryStore copies in memory.ts preserve Task data. Baseline has no priority.',covers:[prefix+'.task']},
  {id:prefix+'.day_planning',kind:'scenario',version:1,summary:'listForDay filters workspace, owner, exact day and not-done, orders position then id, clones results without mutating inputs. plannerView uses the Task JSON boundary.',covers:[prefix+'.task',prefix+'.list_tasks']}
 ]});
 const strict=cli(['validate','--strict'],true,3);assert.ok(strict.diagnostics.some(d=>d.id==='authorization.effect-unprotected'));
 write(root,'strict-model-limitation.json',strict);cli(['validate']);
 cli(['install','prompt']);
 const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{kind:'file',path:'package.json'},{kind:'file',path:'tsconfig.json'},{kind:'tree',path:'src'}],exclusions:[],provenance:{origin:'declared',revision:'planner-afterfix-baseline-v1',disposition:'public-fixture'}};
 write(root,'scan-profile.json',profile);
 const scan=cli(['scan','--target','node-typescript','--profile','standalone',process.execPath,adapter,'--lekalo-project-profile-json',JSON.stringify(profile)]);
 const proposal=cli(['bindings','propose']);assert.equal(proposal.ambiguous,0);assert.equal(proposal.proposals.length,14);
 const plan=cli(['bindings','confirm','--batch','--preview']);
 cli(['bindings','confirm','--batch','--confirm',plan.plan]);
 const audit=cli(['bindings','audit']);assert.equal(audit.current,14);
 const ir=cli(['load','--ir']);const adoptionMs=performance.now()-start;
 const index=JSON.parse(fs.readFileSync(join(adopted,'.lekalo/import/observed/index.json'),'utf8'));assert.equal(index.native_symbols.length,38);
 const context=cli(['context',prefix+'.task','--budget','1600'],false),impact=cli(['impact',prefix+'.task'],false);
 write(root,'semantic-native-join-limitation.json',{state:'unsupported',detail:'Canonical Model IDs require lowercase. Public observed roots preserve Task/listForDay case. No automatic canonical semantic remapping is claimed; authored semantic context and confirmed native inventory are separate evidence.'});
 for(const f of before)assert.equal(sha(fs.readFileSync(join(adopted,f.path))),f.sha256,'adoption changed native bytes');
 fs.cpSync(baseline,gold,{recursive:true});fs.cpSync(join(fixture,'priority-src'),join(gold,'src'),{recursive:true});
 exec(process.execPath,[tsc,'-p','tsconfig.json'],gold);exec(process.execPath,['--test','test/public.test.mjs'],gold);
 const referenceGrade=JSON.parse(exec(process.execPath,[join(root,'oracle.mjs'),gold],root).stdout),baselineGrade=JSON.parse(exec(process.execPath,[join(root,'oracle.mjs'),baseline],root,1).stdout);
 assert.equal(referenceGrade.passed,20);assert.ok(baselineGrade.passed<20);
 write(root,'adoption-receipts.private.json',receipts);write(root,'oracle-preflight.json',{reference:referenceGrade,baseline:baselineGrade});write(root,'context.md',context);write(root,'impact.txt',impact);
 const p={task:'priority-planner-read-once-v1',root,binary,tsc,builderPath:'scripts/research/framework-lift-planner.mjs',builderDigest:sha(fs.readFileSync(fileURLToPath(import.meta.url))),sourceCommit:exec('git',['rev-parse','HEAD'],repo).stdout.trim(),sourceDirty:exec('git',['status','--porcelain','--untracked-files=no'],repo).stdout.trim().length>0,nativeFiles:before,nativeDigest:sha(JSON.stringify(before)),modelDigest:sha(JSON.stringify(inventory(join(adopted,'lekalo')))),irDigest:sha(JSON.stringify(ir.ir)),adoption:{adoptionMs,cliMs:receipts.slice(0,-2).reduce((n,r)=>n+r.durationMs,0),sourcePreserved:true,roots:14,nativeDeclarations:38,nestedDeclarations:24,inference:false,authoringTime:'unknown; authoring baseline Model excluded',authoringTokens:'unknown; not zero'},context:{humanBytes:Buffer.byteLength(context),impactBytes:Buffer.byteLength(impact)},limitations:['Same original native planner/task/oracle; harness now has a pinned compiler helper and measured compact semantic helper, so not an isolated product-version causal experiment.','Two pairs per CLI; no general savings claim.','Cold CLI setup timed separately; semantic authoring labor/tokens and monetary charges unknown.','Warm measurement includes agent and B semantic helper, excludes independent grading and automated post-change maintenance.','Automated post-change scan/reconfirmation will be measured separately without inference; human review cost unknown.']};
 write(root,'preparation.json',p);
 const campaign=options.configure===false?null:configure(root,{devin,codex,...(options.profiles?{profiles:options.profiles}:{}),execReadPrefixes:['cat','type','ls','rg','dir'],semanticToolSource:helper(binary),agentInstructionPath:'.agents/lekalo.md'});
 console.log(JSON.stringify({root,campaign,adoption:p.adoption,context:p.context,baselineHoldout:baselineGrade.passed,referenceHoldout:referenceGrade.passed}));return root;
}
if(process.argv[1]===fileURLToPath(import.meta.url))prepare();
