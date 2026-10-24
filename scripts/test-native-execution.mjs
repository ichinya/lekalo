#!/usr/bin/env node
// Issue #151: release CLI only, actual confinement, no synthetic runner switch.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {createHash,randomUUID} from 'node:crypto';
import {spawn} from 'node:child_process';
import net from 'node:net';
import {tmpdir} from 'node:os';
import {basename,dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createRequire} from 'node:module';

const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN ?? join(repo,'target/release',process.platform==='win32'?'lekalo.exe':'lekalo');
assert.ok(fs.existsSync(binary),'build the release CLI first');
const temp=fs.realpathSync(tmpdir());
const work=fs.mkdtempSync(join(temp,'lekalo-production-native-'));
const node=fs.realpathSync(process.execPath);
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const canonical=value=>Array.isArray(value)?value.map(canonical):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
const nodeDigest=hash(fs.readFileSync(node));
const adapter=join(repo,'adapters/node-typescript');
const require=createRequire((process.env.LEKALO_AJV_NODE_PATH ?? repo)+'/');
assert.equal(require('ajv/package.json').version,'8.20.0');
const Ajv=require('ajv/dist/2020').default;
const validators=Object.fromEntries(['execution-plan','execution-policy','tool-catalog'].map(name=>[name,new Ajv({strict:true,allErrors:true}).compile(JSON.parse(fs.readFileSync(join(repo,`contracts/native-${name}.schema.v0.9.1.json`),'utf8')))]));
const runValidator=new Ajv({strict:true,allErrors:true}).compile(JSON.parse(fs.readFileSync(join(repo,'contracts/native-gate-run.schema.v1.0.0.json'),'utf8')));
const results=[];
const save=(path,value)=>fs.writeFileSync(path,JSON.stringify(value,null,2)+'\n');
const inventory=(root,prefix='')=>fs.readdirSync(join(root,prefix),{withFileTypes:true}).sort((a,b)=>a.name<b.name?-1:1).flatMap(entry=>{
  const path=prefix?`${prefix}/${entry.name}`:entry.name;
  return entry.isDirectory()?inventory(root,path):[[path,hash(fs.readFileSync(join(root,path)))]];
});
function fixture(name,mode='pass') {
  const home=join(work,name),root=join(home,'project');
  fs.cpSync(join(repo,'tests/fixtures/native-execution/project'),root,{recursive:true});
  const runId=randomUUID();
  fs.writeFileSync(join(home,'outside.txt'),'outside-source-sentinel');
  save(join(root,'src/fixture.json'),{runId,sentinel:join(home,'outside.txt')});
  const pkg=JSON.parse(fs.readFileSync(join(root,'package.json'),'utf8'));
  const script=mode==='pass'?'test':`test:${mode}`;
  const planning=JSON.parse(fs.readFileSync(join(repo,'tests/fixtures/node-native-gates/protocol/policy.golden.json'),'utf8'));
  planning.identity={id:'native-execution-policy',version:'0.4.0'};
  planning.trust={mode:'private'};
  planning.confirmations=[{package_id:'.=@native/execution',gate:'test',gate_id:'native-test',gate_kind:'legacy-suite',required:true,cwd:'.',script_name:script,
    manifest_digest:hash(JSON.stringify(canonical(pkg))),script_digest:hash(pkg.scripts[script]),argv:pkg.scripts[script].split(' '),
    tool_ref:{id:'node',artifact_digest:nodeDigest,version:process.versions.node},rule_version:'0.4.0',rule_digest:hash('explicit existing script rule')}];
  planning.fallback_rule={mode:'release-full',trigger:'explicit-release-invocation',gate_kinds:['test'],rule_digest:hash('explicit full selection rule')};
  planning.env_recipe={allowed_names:[],bindings:[]};
  planning.limits={timeout_ms_per_command:mode==='timeout'?6000:10000,timeout_ms_per_run:30000,max_stdout_bytes:4096,max_stderr_bytes:4096,max_output_bytes_per_run:16384};
  planning.write_policy={mode:'scoped',scopes:[{root:'out',max_files:32,max_bytes:mode==='budget'?512:65536}]};
  planning.policy_digest=hash(JSON.stringify(canonical(planning)));
  const policy={schema_version:'lekalo/native-execution-policy/v0.9.1',allow_execution:true,platforms:['windows'],runtime_profile:'windows-node-lpac-job-v1',planning_policy:planning};
  const profile={id:'standalone',mode:'observed',target:'node-typescript',readRoots:[{path:'package.json',kind:'file'},{path:'src',kind:'tree'}],exclusions:[],provenance:{origin:'declared',revision:'production-native-fixture-1',disposition:'user-confirmed'}};
  const catalog={schema_version:'lekalo/native-tool-catalog/v0.9.1',tools:[{id:'node',program:node,artifact_digest:nodeDigest,program_digest:nodeDigest,version:process.versions.node}]};
  assert.ok(validators['execution-policy'](policy),JSON.stringify(validators['execution-policy'].errors));
  assert.ok(validators['tool-catalog'](catalog),JSON.stringify(validators['tool-catalog'].errors));
  for(const [name,value] of Object.entries({policy,profile,catalog}))save(join(home,`${name}.json`),value);
  return {home,root,runId,policy,profile,catalog,adapter};
}
const controls=f=>['--project',f.root,'--adapter',f.adapter,'--policy',join(f.home,'policy.json'),'--profile',join(f.home,'profile.json'),'--catalog',join(f.home,'catalog.json')];
async function cli(args,expected=0,options={}) {
  const before=new Set(fs.readdirSync(temp).filter(name=>name.startsWith('lekalo-target-sandbox-')));
  const beforePrepared=new Set(fs.readdirSync(temp).filter(name=>name.startsWith('lekalo-native-prepared-')));
  const child=spawn(binary,['--json',...args],{cwd:repo,env:{...process.env,LEKALO_NATIVE_SECRET:'synthetic-do-not-inherit',NPM_TOKEN:'synthetic-do-not-inherit'}});
  let stdout='',stderr='',started=false,childPid,preparationWitness=false;
  child.stdout.on('data',bytes=>{stdout+=bytes;if(stdout.length>4*1024*1024)child.kill();});
  child.stderr.on('data',bytes=>{stderr+=bytes;if(stderr.length>4*1024*1024)child.kill();});
  let watcher;
  if(options.fixture) watcher=setInterval(()=>{
    const f=options.fixture;
    if(options.duringPreparation&&!preparationWitness) {
      for(const name of fs.readdirSync(temp).filter(name=>name.startsWith('lekalo-native-prepared-')&&!beforePrepared.has(name))) {
        try {
          const root=join(temp,name);if(fs.lstatSync(root).isSymbolicLink())continue;
          const marker=JSON.parse(fs.readFileSync(join(root,'project/src/fixture.json'),'utf8'));
          if(marker.runId!==f.runId)continue;
          options.duringPreparation();preparationWitness=true;break;
        } catch(error) {if(!(error instanceof SyntaxError)&&!['ENOENT','ENOTDIR','EACCES','EPERM'].includes(error.code))throw error;}
      }
    }
    for(const name of fs.readdirSync(temp).filter(name=>name.startsWith('lekalo-target-sandbox-')&&!before.has(name))) {
      const root=join(temp,name);
      try {
        if(fs.lstatSync(root).isSymbolicLink())continue;
        const read=path=>{const file=join(root,'project/out',path),meta=fs.lstatSync(file);return !meta.isSymbolicLink()&&meta.size<4096?JSON.parse(fs.readFileSync(file,'utf8')):null;};
        const marker=read('started.json');
        if(marker?.runId!==f.runId)continue;
        started=true;
        if(options.cancel)fs.writeFileSync(join(f.home,'cancel.marker'),'cancel');
        if(options.tree){
          try {const error=read('child-error.json');if(error?.runId===f.runId)console.error('synthetic child diagnostic:',error.error);}catch{}
          const entry=read('child.json');if(entry?.runId===f.runId)childPid=entry.pid;
        }
      } catch(error) {if(!(error instanceof SyntaxError)&&!['ENOENT','ENOTDIR','EACCES','EPERM'].includes(error.code))throw error;}
    }
  },20);
  const timer=setTimeout(()=>child.kill(),120000);
  const code=await new Promise((resolve,reject)=>{child.once('error',reject);child.once('close',resolve);});
  clearTimeout(timer);clearInterval(watcher);
  assert.equal(code,expected,`${args.slice(0,3).join(' ')}: ${stdout}\n${stderr}`);
  const text=stdout.trim()||stderr.trim();
  const document=JSON.parse(text);
  return {document,started,childPid,preparationWitness};
}
async function plan(f) {
  const before=inventory(f.root);
  const {document}=await cli(['native','plan',...controls(f)]);
  assert.ok(validators['execution-plan'](document),JSON.stringify(validators['execution-plan'].errors));
  assert.deepEqual(inventory(f.root),before,'planning does not execute or write project commands');
  save(join(f.home,'plan.json'),document);
  return document;
}
async function run(f,p,expected=0,options={}) {
  const before=inventory(f.root);
  const result=await cli(['native','run',join(f.home,'plan.json'),'--confirm',p.plan_digest,...controls(f),'--cancel-file',join(f.home,'cancel.marker')],expected,options);
  assert.deepEqual(inventory(f.root),before,'original project bytes survive every run/refusal');
  assert.equal(fs.readFileSync(join(f.home,'outside.txt'),'utf8'),'outside-source-sentinel');
  return result;
}
function actual(receipt) {
  assert.equal(receipt.kind,'native-run-result');
  assert.equal(receipt.cleanup.state,'complete');
  assert.equal(receipt.original_verification.state,'unchanged');
  assert.equal(receipt.capability_evidence.network_denial,'enforced');
  assert.equal(receipt.capability_evidence.process_containment,'enforced');
  assert.equal(receipt.capability_evidence.backend,'windows-lpac-job');
  assert.match(receipt.capability_evidence.receipt_digest,/^sha256:[a-f0-9]{64}$/);
  assert.match(receipt.provenance,/^sha256:[a-f0-9]{64}$/);
  assert.ok(runValidator(receipt),JSON.stringify(runValidator.errors));
  assert.equal(receipt.commands.length,1);
}
function refusal(document,rule) {assert.ok(document.reasonCodes?.includes(rule),JSON.stringify(document));}
function unconfirmedAdapter(f) {
  f.adapter=join(f.home,'unconfirmed-adapter');fs.mkdirSync(f.adapter);
  // Negative lane only: an otherwise real bundle emits a self-consistent
  // plan for a different existing script than the selected policy approved.
  const interceptor=`const __nativeWrite=process.stdout.write.bind(process.stdout);
process.stdout.write=(chunk,...rest)=>{const out=JSON.parse(String(chunk));const ref=out.result?.native_plan;
if(ref?.plan){const plan=ref.plan,command=plan.commands[0];command.script_name='test:fail';command.argv=['node','src/gate.mjs','fail'];
command.script_digest=__nativeHash(command.argv.join(' '));const {plan_digest,...body}=plan;
plan.plan_digest=__nativeHash('lekalo.native-plan.v0.4.0'+JSON.stringify(__nativeCanonical(body)));ref.digest=plan.plan_digest;}
return __nativeWrite(JSON.stringify(out),...rest);};
function __nativeCanonical(v){return Array.isArray(v)?v.map(__nativeCanonical):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,__nativeCanonical(v[k])])):v;}
import {createHash as __nativeCreateHash} from 'node:crypto';
function __nativeHash(v){return 'sha256:'+__nativeCreateHash('sha256').update(v).digest('hex');}
`;
  const bytes=Buffer.from(interceptor+fs.readFileSync(join(adapter,'adapter.mjs'),'utf8').replace(/^#![^\n]*\n/,''));
  fs.writeFileSync(join(f.adapter,'adapter.mjs'),bytes);
  const manifest=JSON.parse(fs.readFileSync(join(adapter,'adapter.manifest.json'),'utf8'));
  manifest.integrity.files=[{path:'adapter.mjs',digest:hash(bytes),bytes:bytes.length}];
  manifest.integrity.packageDigest='sha256:'+'0'.repeat(64);
  const framed=(name,content)=>{const n=Buffer.alloc(8);n.writeBigUInt64BE(BigInt(content.length));return Buffer.concat([Buffer.from(name),Buffer.from([0]),n,Buffer.from([0]),content]);};
  const parts=[framed('adapter.mjs',bytes),framed('adapter.manifest.json',Buffer.from(JSON.stringify(canonical(manifest))))].map(part=>{const n=Buffer.alloc(8);n.writeBigUInt64BE(BigInt(part.length));return Buffer.concat([n,part]);});
  manifest.integrity.packageDigest=hash(Buffer.concat(parts));save(join(f.adapter,'adapter.manifest.json'),manifest);
}
try {
  const f=fixture('pass'),p=await plan(f);
  assert.deepEqual(await plan(f),p,'plan is deterministic without host staging paths');
  const unauthorized=fixture('unconfirmed-adapter');unconfirmedAdapter(unauthorized);
  refusal((await cli(['native','plan',...controls(unauthorized)],3)).document,'native-gate.trust-insufficient');
  results.push({lane:'adapter-proposed-unconfirmed-command',refusal:'native-gate.trust-insufficient'});
  refusal((await cli(['native','run',join(f.home,'plan.json')],3)).document,'native-gate.approval-missing');
  const bad=structuredClone(p);bad.plan_digest='sha256:'+'0'.repeat(64);
  save(join(f.home,'bad.json'),bad);
  refusal((await cli(['native','run',join(f.home,'bad.json')],1)).document,'native-gate.plan-stale');
  if(process.platform!=='win32') {
    refusal((await run(f,p,4)).document,'native-gate.capability-missing');
    results.push({platform:process.platform,planning:'passed',execution:'unsupported',qualified:false});
  } else {
    const {document:receipt}=await run(f,p);actual(receipt);
    assert.equal(receipt.outcome,'passed');assert.equal(receipt.verdict,'passed');assert.equal(receipt.commands[0].exit.value,0);
    assert.ok(receipt.mutation_summary.created.some(item=>item.path==='out/result.txt'));
    results.push({lane:'pass',outcome:receipt.outcome,original:'unchanged',cleanup:'complete'});
    for(const [name,mutation,rule,exit] of [
      ['source',f=>fs.appendFileSync(join(f.root,'src/gate.mjs'),'\n// stale source\n'),'native-gate.plan-stale',1],
      ['new-source',f=>fs.writeFileSync(join(f.root,'src/new.mjs'),'export const added=true;'),'native-gate.plan-stale',1],
      ['package-graph',f=>fs.appendFileSync(join(f.root,'package.json'),'\n'),'native-gate.plan-stale',1],
      ['policy',f=>fs.appendFileSync(join(f.home,'policy.json'),'\n'),'native-gate.plan-stale',1],
      ['profile',f=>fs.appendFileSync(join(f.home,'profile.json'),'\n'),'native-gate.plan-stale',1],
      ['catalog',f=>fs.appendFileSync(join(f.home,'catalog.json'),'\n'),'native-gate.plan-stale',1],
      ['denied',f=>{f.policy.allow_execution=false;save(join(f.home,'policy.json'),f.policy);},'native-gate.trust-insufficient',3],
      ['junction',f=>{const outside=join(f.home,'outside-dir');fs.mkdirSync(outside);fs.symlinkSync(outside,join(f.root,'src/linked'),'junction');},'native-gate.security-violation',3],
    ]) {
      const f=fixture(`stale-${name}`),p=await plan(f);mutation(f);
      // A junction is deliberately not traversed by the original-byte checker.
      const {document}=name==='junction'
        ? await cli(['native','run',join(f.home,'plan.json'),'--confirm',p.plan_digest,...controls(f),'--cancel-file',join(f.home,'cancel.marker')],exit)
        : await run(f,p,exit);
      refusal(document,rule);results.push({lane:name,refusal:rule});
    }
    {
      const f=fixture('wrong-approval'),p=await plan(f);
      refusal((await cli(['native','run',join(f.home,'plan.json'),'--confirm','sha256:'+'0'.repeat(64),...controls(f),'--cancel-file',join(f.home,'cancel.marker')],3)).document,'native-gate.approval-missing');
      results.push({lane:'wrong-approval',refusal:'native-gate.approval-missing'});
    }
    {
      const f=fixture('revoke-during-preparation'),p=await plan(f);
      const result=await run(f,p,3,{fixture:f,duringPreparation:()=>{f.policy.allow_execution=false;save(join(f.home,'policy.json'),f.policy);}});
      assert.equal(result.preparationWitness,true,'revocation follows the initial custody capture');
      refusal(result.document,'native-gate.trust-insufficient');
      results.push({lane:'revoked-during-preparation',refusal:'native-gate.trust-insufficient'});
    }
    {
      const f=fixture('source-during-preparation'),p=await plan(f);let edited;
      const result=await cli(['native','run',join(f.home,'plan.json'),'--confirm',p.plan_digest,...controls(f),'--cancel-file',join(f.home,'cancel.marker')],1,
        {fixture:f,duringPreparation:()=>{fs.appendFileSync(join(f.root,'src/gate.mjs'),'\n// concurrent editor change\n');edited=inventory(f.root);}});
      assert.equal(result.preparationWitness,true);refusal(result.document,'native-gate.plan-stale');
      assert.deepEqual(inventory(f.root),edited,'the editor change remains exactly as written');
      results.push({lane:'source-changed-during-preparation',refusal:'native-gate.plan-stale'});
    }
    {
      const f=fixture('stale-tool');
      const privateNode=join(f.home,'node.exe');fs.copyFileSync(node,privateNode);
      f.catalog.tools[0].program=privateNode;save(join(f.home,'catalog.json'),f.catalog);
      const p=await plan(f);fs.appendFileSync(privateNode,'stale');
      refusal((await run(f,p,1)).document,'native-gate.plan-stale');results.push({lane:'tool-bytes',refusal:'native-gate.plan-stale'});
    }
    {
      const f=fixture('stale-adapter');f.adapter=join(f.home,'adapter');fs.mkdirSync(f.adapter);
      for(const file of ['adapter.mjs','adapter.manifest.json'])fs.copyFileSync(join(adapter,file),join(f.adapter,file));
      const p=await plan(f);fs.appendFileSync(join(f.adapter,'adapter.mjs'),'\n// stale adapter\n');
      refusal((await run(f,p,1)).document,'native-gate.plan-stale');results.push({lane:'adapter-bytes',refusal:'native-gate.plan-stale'});
    }
    {
      const f=fixture('missing-command');const pkg=JSON.parse(fs.readFileSync(join(f.root,'package.json'),'utf8'));
      pkg.scripts.test='node src/missing.mjs';save(join(f.root,'package.json'),pkg);
      const confirmation=f.policy.planning_policy.confirmations[0];confirmation.argv=['node','src/missing.mjs'];
      confirmation.manifest_digest=hash(JSON.stringify(canonical(pkg)));confirmation.script_digest=hash(pkg.scripts.test);save(join(f.home,'policy.json'),f.policy);
      refusal((await cli(['native','plan',...controls(f)],4)).document,'native-gate.capability-missing');
      results.push({lane:'missing-command',refusal:'native-gate.capability-missing'});
    }
    {
      const f=fixture('security-stops-independent-gate','security');
      const first=f.policy.planning_policy.confirmations[0];first.gate_id='native-a-security';
      const second=structuredClone(first);second.gate_id='native-b-followup';second.script_name='test';second.argv=['node','src/gate.mjs','pass'];
      second.script_digest=hash(second.argv.join(' '));second.rule_digest=hash('separate explicitly confirmed followup');
      f.policy.planning_policy.confirmations.push(second);save(join(f.home,'policy.json'),f.policy);
      const p=await plan(f),{document}=await run(f,p,3);
      assert.equal(document.outcome,'security');assert.equal(document.cleanup.state,'complete');
      assert.equal(document.commands.length,2);assert.equal(document.commands[1].exit?.state??'unknown','unknown');
      assert.ok(document.commands[1].reason_codes.includes('previous-execution-unsafe'));
      results.push({lane:'security-stops-independent-gate',outcome:'security',followup:'not-run'});
    }
    for(const [mode,exit,reason] of [['fail',1,'assertion-failed'],['timeout',4,'timeout'],['flood',4,'output-limit'],['stderr',4,'output-limit'],['security',3,'unexpected-write'],['budget',3,'write-budget-exceeded'],['cancel',4,'cancelled']]) {
      const f=fixture(mode,mode),p=await plan(f);
      const {document,started}=await run(f,p,exit,mode==='cancel'?{fixture:f,cancel:true}:mode==='timeout'?{fixture:f}:{});actual(document);
      const reasons=[...document.reason_codes,...document.commands.flatMap(command=>command.reason_codes??[])];
      assert.ok(reasons.includes(reason),JSON.stringify(document));
      if(mode==='fail') {
        assert.equal(document.commands[0].exit.value,7);
        const artifact={schema_version:'lekalo/native-command-output/v0.9.1',stdout:{state:'known',text:'',truncated:false},stderr:{state:'known',text:'synthetic assertion failed',truncated:false}};
        assert.equal(document.commands[0].output_ref,hash(JSON.stringify(canonical(artifact))),'stderr survives in the combined output artifact');
      }
      // Frozen native-run 0.4 permits an absent exit member as unknown.
      if(['timeout','flood','stderr','cancel'].includes(mode))assert.equal(document.commands[0].exit?.state??'unknown','unknown');
      if(mode==='cancel'||mode==='timeout')assert.equal(started,true,'termination only qualifies after an actual gate started');
      results.push({lane:mode,outcome:document.outcome,reason,cleanup:document.cleanup.state});
    }
    for(const mode of ['filesystem','tree','network','memory']) {
      const f=fixture(mode,mode);
      let server;
      if(mode==='network') {
        server=net.createServer(socket=>socket.destroy());await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
        const config=JSON.parse(fs.readFileSync(join(f.root,'src/fixture.json'),'utf8'));config.port=server.address().port;save(join(f.root,'src/fixture.json'),config);
      }
      try {
        const p=await plan(f),{document,childPid}=await run(f,p,0,mode==='tree'?{fixture:f,tree:true}:{});actual(document);assert.equal(document.outcome,'passed');
        if(mode==='tree') {
          assert.ok(Number.isInteger(childPid),'actual detached child start was observed');
          assert.throws(()=>process.kill(childPid,0),error=>error.code==='ESRCH','no descendant survives the closed job');
        }
        results.push({lane:mode,outcome:'passed',cleanup:document.cleanup.state});
      } finally {if(server)await new Promise(resolve=>server.close(resolve));}
    }
  }
  console.log(JSON.stringify({ok:true,gate:'production-native-execution',binaryDigest:hash(fs.readFileSync(binary)),executionQualified:process.platform==='win32',platform:process.platform,results}));
} finally {
  const real=fs.realpathSync(work);
  assert.equal(dirname(real),temp);assert.ok(basename(real).startsWith('lekalo-production-native-'));
  fs.rmSync(real,{recursive:true,force:true});
}
