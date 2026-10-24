#!/usr/bin/env node
// Issue #156: real provisioned tools, immutable plan, production verify entrypoint.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {tmpdir} from 'node:os';
import {basename,dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createNodeToolCatalog} from './lib/node-tool-catalog.mjs';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(repo,'target/release',process.platform==='win32'?'lekalo.exe':'lekalo');
const fixture=join(repo,'tests/fixtures/node-native-tools');
const tools=resolve(process.env.LEKALO_NODE_TOOLS_HOME??join(repo,'target/toolchains/node-native-tools'));
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
const bytes=value=>JSON.stringify(canonical(value));
const save=(path,value)=>fs.writeFileSync(path,JSON.stringify(value,null,2)+'\n');
const rootPackage=JSON.parse(fs.readFileSync(join(fixture,'project/package.json'),'utf8'));
if(process.argv.includes('--provision')) {
  // Explicit development/CI preparation, completed before planning/execution.
  fs.mkdirSync(tools,{recursive:true});
  const real=fs.realpathSync(tools);assert.equal(real.toLowerCase(),tools.toLowerCase(),'tool store must be a physical dedicated directory');
  const owner=join(tools,'.lekalo-node-tools-owner.json');
  if(fs.readdirSync(tools).length)assert.equal(JSON.parse(fs.readFileSync(owner,'utf8')).owner,'lekalo-node-native-tools','refuse to replace an unowned tool store');
  const modules=join(tools,'node_modules');
  if(fs.existsSync(modules))assert.equal(fs.realpathSync(modules).toLowerCase(),modules.toLowerCase(),'npm ci cleanup must stay inside the owned tool store');
  save(owner,{owner:'lekalo-node-native-tools',format:1});
  const lock=fs.readFileSync(join(fixture,'toolchain-lock.json'));
  const metadata=JSON.parse(lock);assert.deepEqual(metadata.packages[''].devDependencies,rootPackage.devDependencies);
  save(join(tools,'package.json'),{name:metadata.name,version:metadata.version,private:true,devDependencies:rootPackage.devDependencies});
  fs.writeFileSync(join(tools,'package-lock.json'),lock);
  const npm=process.env.npm_execpath??join(dirname(process.execPath),'node_modules/npm/bin/npm-cli.js');
  assert.ok(fs.existsSync(npm),'provide npm_execpath for an npm installation separate from node');
  const run=spawnSync(process.execPath,[npm,'ci','--ignore-scripts','--no-audit','--no-fund'],{cwd:tools,encoding:'utf8',timeout:300000,maxBuffer:8e6});
  assert.equal(run.status,0,run.stdout+'\n'+run.stderr);
}
for(const [name,version]of Object.entries(rootPackage.devDependencies))assert.equal(JSON.parse(fs.readFileSync(join(tools,'node_modules',name,'package.json'),'utf8')).version,version);
const assetRoot=fs.realpathSync(join(tools,'node_modules'));
function inventory(root,prefix='',assets=false) {
  return fs.readdirSync(join(root,prefix),{withFileTypes:true}).sort((a,b)=>a.name<b.name?-1:1).flatMap(entry=>{
    if(assets&&entry.name==='.bin')return [];
    const path=prefix?prefix+'/'+entry.name:entry.name,full=join(root,path),meta=fs.lstatSync(full);
    assert.ok(!meta.isSymbolicLink());
    return meta.isDirectory()?inventory(root,path,assets):[{path,digest:hash(fs.readFileSync(full)),bytes:meta.size}];
  });
}
const catalog=createNodeToolCatalog(assetRoot);
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH??repo)+'/');
assert.equal(require('ajv/package.json').version,'8.20.0');const Ajv=require('ajv/dist/2020').default;
const validators=Object.fromEntries(['tool-catalog','execution-plan','verify-report'].map(name=>[name,new Ajv({strict:true,allErrors:true}).compile(JSON.parse(fs.readFileSync(join(repo,`contracts/native-${name}.schema.v1.0.0.json`),'utf8')))]));
assert.ok(validators['tool-catalog'](catalog),JSON.stringify(validators['tool-catalog'].errors));
const temp=fs.realpathSync(tmpdir()),work=fs.mkdtempSync(join(temp,'lekalo-native-tools-'));
const results=[];
const reports=process.env.LEKALO_NATIVE_REPORTS_DIR?resolve(process.env.LEKALO_NATIVE_REPORTS_DIR):null;
if(reports){fs.mkdirSync(reports,{recursive:true});assert.equal(fs.readdirSync(reports).length,0,'acceptance output must be an empty dedicated directory');}

function cli(root,args,exit=0) {
  const run=spawnSync(binary,['--json','--no-cache',...args],{cwd:root,encoding:'utf8',timeout:240000,maxBuffer:24e6,env:{...process.env,LEKALO_NATIVE_SECRET:'synthetic-host-token',NPM_TOKEN:'synthetic-host-token'}});
  assert.equal(run.status,exit,args.slice(0,3).join(' ')+'\n'+run.stdout+'\n'+run.stderr);
  return JSON.parse(run.stdout.trim()||run.stderr.trim());
}
function prepare(name,{only,mutate,adoptScope,afterAdopt}={}) {
  const home=join(work,name),root=join(home,'project');fs.cpSync(join(fixture,'project'),root,{recursive:true});if(mutate)mutate(root,home);
  const scoped=adoptScope?['--discovery-scope','discovery-scope.json']:[];
  if(adoptScope){save(join(root,'discovery-scope.json'),adoptScope);const before=inventory(root);cli(root,['init','--adopt','--project-id','native_workspace',...scoped,'--dry-run']);assert.deepEqual(inventory(root),before,'scoped adoption preview is read-only');}
  cli(root,['init','--adopt','--project-id','native_workspace',...scoped]);
  if(afterAdopt)afterAdopt(root,home);
  cli(root,['lock']);
  const planning=JSON.parse(fs.readFileSync(join(repo,'tests/fixtures/node-native-gates/protocol/policy.golden.json'),'utf8'));
  planning.identity={id:'node-workspace-policy',version:'0.4.0'};planning.trust={mode:'private'};
  planning.confirmations=[];
  for(const [pkgRoot,gate,gateId,script]of [['packages/core','test','core-test','test'],['packages/core','typecheck','core-types','typecheck'],['packages/web','typecheck','web-types','typecheck'],['packages/web','build','web-build','build']]) {
    if(only&&gateId!==only)continue;
    const pkg=JSON.parse(fs.readFileSync(join(root,pkgRoot,'package.json'),'utf8')),argv=pkg.scripts[script].split(' '),tool=catalog.tools.find(tool=>tool.id===argv[0]);
    planning.confirmations.push({package_id:`${pkgRoot}=${pkg.name}`,gate,gate_id:gateId,gate_kind:'legacy-suite',required:true,cwd:pkgRoot,script_name:script,manifest_digest:hash(bytes(pkg)),script_digest:hash(pkg.scripts[script]),argv,tool_ref:{id:tool.id,artifact_digest:tool.artifact_digest,entry_digest:tool.entry_digest,version:tool.version},rule_version:'0.4.0',rule_digest:hash('explicit existing script '+gateId)});
  }
  planning.fallback_rule={mode:'release-full',trigger:'explicit-release-invocation',gate_kinds:['test','typecheck','build'],rule_digest:hash('explicit workspace full gate selection')};
  planning.env_recipe={allowed_names:[],bindings:[]};
  planning.limits={timeout_ms_per_command:30000,timeout_ms_per_run:120000,max_stdout_bytes:16384,max_stderr_bytes:16384,max_output_bytes_per_run:131072};
  planning.write_policy={mode:'scoped',scopes:['.lekalo-temp','packages/core/cache','packages/web/cache','packages/web/out'].map(root=>({root,max_files:256,max_bytes:8*1024*1024}))};
  delete planning.policy_digest;planning.policy_digest=hash(bytes(planning));
  const policy={schema_version:'lekalo/native-execution-policy/v0.9.1',allow_execution:true,platforms:['windows'],runtime_profile:'windows-node-lpac-job-v1',planning_policy:planning};
  const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{path:'package.json',kind:'file'},{path:'packages',kind:'tree'}],exclusions:['packages/core/cache/**','packages/web/cache/**','packages/web/out/**'],provenance:{origin:'declared',revision:'native-tools-1',disposition:'user-confirmed'}};
  save(join(home,'policy.json'),policy);save(join(home,'profile.json'),profile);save(join(home,'catalog.json'),catalog);
  const controls=['--project',root,'--adapter',join(repo,'adapters/node-typescript'),'--policy',join(home,'policy.json'),'--profile',join(home,'profile.json'),'--catalog',join(home,'catalog.json')];
  const plan=cli(root,['native','plan',...controls]);assert.ok(validators['execution-plan'](plan),JSON.stringify(validators['execution-plan'].errors));assert.equal(plan.native_plan.workspace.manager,'npm-workspaces');save(join(home,'plan.json'),plan);
  return {root,home,plan};
}
function verify(f,exit=0) {
  const before=inventory(f.root),args=['verify','--project','.', '--native-plan',join(f.home,'plan.json'),'--native-confirm',f.plan.plan_digest,'--native-adapter',join(repo,'adapters/node-typescript'),'--native-policy',join(f.home,'policy.json'),'--native-profile',join(f.home,'profile.json'),'--native-catalog',join(f.home,'catalog.json'),'--native-cancel-file',join(f.home,'cancel.marker')];
  const report=cli(f.root,args,exit);assert.ok(validators['verify-report'](report),JSON.stringify(validators['verify-report'].errors));assert.deepEqual(inventory(f.root),before,'the original workspace survives verify');assert.equal(report.exit_code,exit);
  if(report.native.kind==='native-execution-report') {
    const run=report.native.run;assert.equal(run.original_verification.state,'unchanged');assert.equal(run.cleanup.state,'complete');
    for(const output of report.native.outputs)assert.equal(output.digest,hash(bytes(output.output)));
    for(const command of run.commands)if(typeof command.output_ref==='string')assert.ok(report.native.outputs.some(output=>output.digest===command.output_ref));
  }
  if(reports){save(join(reports,basename(f.home)+'.plan.json'),f.plan);save(join(reports,basename(f.home)+'.verify.json'),report);}
  return report;
}
const isMain=process.argv[1]&&resolve(process.argv[1]).toLowerCase()===fileURLToPath(import.meta.url).toLowerCase();
if(isMain){
try {
  const f=prepare('passing');
  if(process.platform!=='win32') {
    const report=verify(f,4);assert.equal(report.native.state,'error');assert.equal(report.status,'unsupported');results.push({lane:'platform',execution:'unsupported',qualified:false});
  } else {
    const report=verify(f);assert.equal(report.status,'passed');assert.equal(report.native.run.commands.length,4);assert.ok(report.native.run.commands.every(command=>command.exit?.value===0));
    const text=report.native.outputs.map(output=>output.output.stdout.text??'').join('\n');assert.match(text,/Test Files.*passed/);assert.match(text,/built in/);
    results.push({lane:'workspace',status:'passed',planDigest:f.plan.plan_digest,reportDigest:hash(bytes(report)),outputDigests:report.native.outputs.map(row=>row.digest),gates:report.native.run.commands.map(command=>({gate:command.gate_id,exit:command.exit.value}))});
    for(const [name,only,path,change,expected,pattern]of [
      ['vitest-failure','core-test','packages/core/test/add.test.ts',text=>text.replace('toBe(5)','toBe(6)'),1,/AssertionError|expected/i],
      ['typecheck-failure','core-types','packages/core/src/index.ts',text=>text.replace('return left + right','return String(left + right)'),2,/TS2322/],
      ['build-failure','web-build','packages/web/src/main.ts',text=>"import './missing-module';\n"+text,1,/missing-module|Could not resolve/i],
    ]) {
      const bad=prepare(name,{only,mutate:root=>{const file=join(root,path),before=fs.readFileSync(file,'utf8'),after=change(before);assert.notEqual(after,before);fs.writeFileSync(file,after);}});
      const result=verify(bad,1);assert.equal(result.status,'failed');assert.equal(result.native.run.commands.length,1);assert.equal(result.native.run.commands[0].exit.value,expected);
      const output=result.native.outputs.map(row=>[row.output.stdout.text??'',row.output.stderr.text??''].join('\n')).join('\n');assert.match(output,pattern);
      results.push({lane:name,status:result.status,toolExit:expected,cliExit:result.exit_code});
    }
    const originalPlan=fs.readFileSync(join(f.home,'plan.json'));
    save(join(f.home,'plan.json'),report);const forged=verify(f,1);assert.equal(forged.native.state,'error');assert.equal(forged.native.diagnostic.status,'invalid');fs.writeFileSync(join(f.home,'plan.json'),originalPlan);
    const approved=f.plan.plan_digest;f.plan.plan_digest=hash('wrong approval');const denied=verify(f,3);assert.equal(denied.native.state,'error');f.plan.plan_digest=approved;
    const source=join(f.root,'packages/core/src/index.ts'),originalSource=fs.readFileSync(source);fs.appendFileSync(source,'\n// changed after approval\n');const stale=verify(f,1);assert.ok(stale.native.diagnostic.reasonCodes.includes('native-gate.plan-stale'));fs.writeFileSync(source,originalSource);
    results.push({lane:'custody',forgedReceipt:'refused',wrongApproval:'denied',changedSource:'refused'});

  }
  const summary={ok:true,gate:'node-native-tools',productionCli:true,executionQualified:process.platform==='win32',binaryDigest:hash(fs.readFileSync(binary)),runtime:process.versions.node,tools:catalog.tools.map(({id,version,entry_digest,artifact_digest})=>({id,version,entry_digest,artifact_digest})),results};
   if(reports)save(join(reports,'summary.json'),summary);
   console.log(JSON.stringify(summary));
} finally {cleanup();}
}
function cleanup(){const real=fs.realpathSync(work);assert.equal(dirname(real),temp);assert.ok(basename(real).startsWith('lekalo-native-tools-'));fs.rmSync(real,{recursive:true,force:true});}
export {repo,binary,catalog,cli,prepare,verify,save,hash,bytes,inventory,cleanup};
