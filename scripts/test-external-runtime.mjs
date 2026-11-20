import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,cpSync,writeFileSync,readFileSync,rmSync,symlinkSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import test from 'node:test';
import {pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';

const cli = process.env.LEKALO_BIN ?? resolve('target/debug/lekalo');
const emit = (p,v) => writeFileSync(p,JSON.stringify(v,null,2));
const run = (dir,args,cache) => spawnSync(cli,['--json',...args],{
  cwd:dir,encoding:'utf8',env:{...process.env,LEKALO_PACKAGE_HOME:cache},timeout:120_000,
});
const ok = r => {assert.equal(r.status,0,r.stderr||r.stdout);return JSON.parse(r.stdout);};
const refused = (r,detail) => {assert.notEqual(r.status,0);assert.equal(JSON.parse(r.stderr).diagnostics[0].data.detail,detail);};

// Two independently implemented source readers, neither registered in core.
// Both implement the same portable evidence contract; no shared scanner is used.
function runtime(id,reader) {
  return `import{readFileSync}from'node:fs';
const request=JSON.parse(readFileSync(0,'utf8'));
const description={id:${JSON.stringify(id)},version:'1.0.0',rules_api:['lekalo/contract-rules/v1'],capabilities:['http.routes/v1'],operations:['check']};
let result=description;
if(request.operation==='check'){
  const routes=${reader};
  const facts={};
  for(const pack of request.input.rules){facts[pack.id]={};for(const check of pack.rules.checks)facts[pack.id][check.id]={routes};}
  result={state:'complete',facts};
}
if(request.operation==='check'){
  const text=request.input.files.map(f=>f.content).join(' ');
  if(text.includes('MODE_PARTIAL'))result.state='partial';
  if(text.includes('MODE_MISSING'))result.facts={};
  if(text.includes('MODE_WRONG_ID'))request.request_id='wrong';
  if(text.includes('MODE_WRONG_VERSION'))request.protocol='lekalo/runtime/v99';
  if(text.includes('MODE_YAML')){process.stdout.write('protocol: invalid');process.exit(0);}
}
process.stdout.write(JSON.stringify({protocol:request.protocol,request_id:request.request_id,result}));`;
}

test('unknown runtimes share portable rules, pins, confined inputs and offline Git packages',()=>{
  const scratch=mkdtempSync(join(process.env.BB_THREAD_STORAGE??tmpdir(),'lekalo-runtime-test-'));
  const project=join(scratch,'project'),cache=join(scratch,'cache'),rules=join(scratch,'rules'),zen=join(scratch,'zen');
  try{
    for(const p of [project,cache,rules,zen,join(project,'app'),join(project,'second')])mkdirSync(p,{recursive:true});
    const secret=join(project,'.private-input');
    writeFileSync(secret,'not granted to runtime');
    const probe=`import {readFileSync as readHost,writeFileSync as writeHost} from 'node:fs';
for(const attempt of [()=>readHost(${JSON.stringify(secret)}),()=>writeHost(${JSON.stringify(secret)},'changed')]){try{attempt();process.exit(99);}catch{}}\n`;
    const ruleDocument={schema:'lekalo/contract-rules/v1',requires:['http.routes/v1'],checks:[{
      id:'route-contract',schema:{type:'object',required:['routes'],additionalProperties:false,properties:{routes:{type:'array',minItems:1,contains:{type:'object',required:['method','path'],properties:{method:{const:'GET'},path:{const:'/health'}}}}}},
    }]};
    emit(join(rules,'lekalo.package.json'),{schema:'lekalo/package/v1',id:'portable-http-contract',version:'1.0.0',kind:'rules',files:['rules.json'],rules:'rules.json'});
    emit(join(rules,'rules.json'),ruleDocument);
    const zenEntry=runtime('third-party-zen',`request.input.files.flatMap(f=>f.content.split('\\n').filter(l=>l.startsWith('route ')).map(l=>{const [,method,path]=l.split(' ');return{method,path};}))`);
    mkdirSync(join(zen,'lib'));
    writeFileSync(join(zen,'lib','support.mjs'),"export const packageVersion='1.0.0';");
    writeFileSync(join(zen,'main.mjs'),probe+"import {packageVersion} from './lib/support.mjs';\n"+zenEntry.replace("version:'1.0.0'","version:packageVersion"));
    emit(join(zen,'lekalo.package.json'),{schema:'lekalo/package/v1',id:'third-party-zen',version:'1.0.0',kind:'runtime',command:[process.execPath,'main.mjs'],files:['main.mjs','lib/support.mjs']});
    const second=join(scratch,'second.mjs');
    writeFileSync(second,runtime('independent-data-runtime',`request.input.files.flatMap(f=>JSON.parse(f.content).endpoints.map(e=>({method:e.verb,path:e.url})))`));
    writeFileSync(join(project,'app','server.zen'),'route GET /health\n');
    emit(join(project,'second','server.json'),{endpoints:[{verb:'GET',url:'/health'}]});
    const config={schema:'lekalo/runtime-config/v1',components:{
      zen:{path:'app',runtime:`path:${zen}`,rules:[`path:${rules}`]},
      another:{path:'second',runtime:{command:[process.execPath,second]},rules:[`path:${rules}`]},
    }};
    emit(join(project,'lekalo.yaml'),config);
    const locked=ok(run(project,['runtime','lock'],cache)).lock;
    assert.equal(locked.components.zen.runtime.description.id,'third-party-zen');
    const report=ok(run(project,['validate','--strict'],cache));
    assert.equal(report.components.length,2);
    assert(report.components.every(c=>c.status==='valid'));
    if(process.platform!=='win32'){
      symlinkSync(project,join(scratch,'linked-project'),'dir');
      const validateLinked=()=>run(scratch,['validate','--strict','--project','linked-project'],cache);
      assert.deepEqual(ok(validateLinked()),report);
      const evidenceConfig=join(project,'lekalo.evidence.json');
      emit(evidenceConfig,{schema:'lekalo/evidence-config/unknown',scans:[]});
      refused(validateLinked(),'evidence-config-invalid');
      rmSync(evidenceConfig);
    }
    assert.equal(readFileSync(secret,'utf8'),'not granted to runtime');
    const cliDigest=()=>createHash('sha256').update(readFileSync(cli)).digest('hex');
    const initialCliDigest=cliDigest();
    ok(run(project,['runtime','lock','--offline'],cache));
    assert.equal(cliDigest(),initialCliDigest,'composition never rebuilds core');

    writeFileSync(join(project,'app','server.zen'),'route GET /broken\n');
    const mismatch=run(project,['runtime','check','--component','zen'],cache);
    assert.equal(mismatch.status,1,mismatch.stderr);
    assert.equal(JSON.parse(mismatch.stderr).diagnostics[0].data.rule,'route-contract');
    writeFileSync(join(project,'app','server.zen'),'route GET /health\n');

    for (const [mode,detail] of [['PARTIAL','runtime-incomplete'],['MISSING','rules-result-missing'],['WRONG_ID','runtime-response-mismatch'],['WRONG_VERSION','runtime-response-mismatch'],['YAML','runtime-response-invalid']]) {
      writeFileSync(join(project,'app','server.zen'),`route GET /health\nMODE_${mode}\n`);
      refused(run(project,['runtime','check','--component','zen'],cache),detail);
    }
    writeFileSync(join(project,'app','server.zen'),'route GET /health\n');
    mkdirSync(join(project,'lekalo'));
    writeFileSync(join(project,'lekalo','project.yaml'),'broken: [');
    const badModel=run(project,['validate','--strict'],cache);
    assert.notEqual(badModel.status,0,'runtime config cannot hide a broken existing Model');
    assert(badModel.stderr.includes('diagnostics'));
    rmSync(join(project,'lekalo'),{recursive:true});

    // Changing a directly installed entry invalidates its lock before execution.
    const secondBytes=readFileSync(second);
    writeFileSync(second,secondBytes+'\n// changed');
    refused(run(project,['runtime','check'],cache),'runtime-changed');
    writeFileSync(second,secondBytes);

    const cacheFile=join(cache,locked.components.zen.runtime.package.digest.slice(7),'main.mjs');
    const cached=readFileSync(cacheFile);
    writeFileSync(cacheFile,cached+'\n// changed');
    refused(run(project,['runtime','check'],cache),'package-changed');
    writeFileSync(cacheFile,cached);

    const badConfig=structuredClone(config);badConfig.components.zen.path='../outside';
    emit(join(project,'lekalo.yaml'),badConfig);
    refused(run(project,['runtime','lock'],cache),'component-invalid');
    badConfig.components.zen.path='.';
    emit(join(project,'lekalo.yaml'),badConfig);
    refused(run(project,['runtime','lock'],cache),'component-overlap');
    emit(join(project,'lekalo.yaml'),config);

    if(process.platform!=='win32'){
      symlinkSync(second,join(project,'app','escape'));
      refused(run(project,['runtime','check','--component','zen'],cache),'source-file-type');
      rmSync(join(project,'app','escape'));
    }

    const incompatible=structuredClone(ruleDocument);incompatible.requires=['not-supported/v1'];
    emit(join(rules,'rules.json'),incompatible);
    const before=readFileSync(join(project,'lekalo.runtime.lock'));
    refused(run(project,['runtime','lock'],cache),'rules-capability-missing');
    assert.deepEqual(readFileSync(join(project,'lekalo.runtime.lock')),before,'failed composition preserves prior lock');
    const remoteSchema=structuredClone(ruleDocument);remoteSchema.checks[0].schema={$ref:'https://invalid.example/schema'};
    emit(join(rules,'rules.json'),remoteSchema);
    refused(run(project,['runtime','lock'],cache),'rules-schema-reference');
    emit(join(rules,'rules.json'),ruleDocument);

    // Resolve actual committed Git blobs. Dirty worktree bytes must never enter the package.
    const git=(args)=>{const r=spawnSync('git',args,{cwd:zen,encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout.trim();};
    git(['init','-q']);git(['add','.']);git(['-c','user.name=Runtime test','-c','user.email=runtime@example.invalid','-c','commit.gpgsign=false','commit','-qm','fixture']);
    const commit=git(['rev-parse','HEAD']);
    // A branch may move after review. Restore must fetch the recorded Git object.
    config.components.zen.runtime=`git+${pathToFileURL(zen).href}#HEAD`;
    emit(join(project,'lekalo.yaml'),config);
    const gitLock=ok(run(project,['runtime','lock'],cache)).lock;
    assert.equal(gitLock.schema,'lekalo/runtime-lock/v2');
    assert.equal(gitLock.components.zen.runtime.package.revision,commit);
    assert.deepEqual(Object.keys(gitLock.components.zen.runtime.files),['0','1']);
    assert(!JSON.stringify(gitLock).includes(cache));
    writeFileSync(join(zen,'main.mjs'),'throw new Error("upstream changed")');
    git(['add','main.mjs']);git(['-c','user.name=Runtime test','-c','user.email=runtime@example.invalid','-c','commit.gpgsign=false','commit','-qm','new upstream']);
    const moved=join(scratch,'relocated'), freshCache=join(scratch,'restored-cache');
    cpSync(project,moved,{recursive:true});
    const lockBefore=readFileSync(join(moved,'lekalo.runtime.lock'));
    ok(run(moved,['runtime','restore'],freshCache));
    assert.deepEqual(readFileSync(join(moved,'lekalo.runtime.lock')),lockBefore);
    ok(run(moved,['runtime','lock','--offline'],freshCache));
    ok(run(moved,['runtime','check'],freshCache));
    rmSync(zen,{recursive:true});
    rmSync(rules,{recursive:true});
    ok(run(project,['runtime','lock','--offline'],cache));
    ok(run(project,['runtime','check'],cache));
    assert.equal(cliDigest(),initialCliDigest,'composition never rebuilds core');

    // Configuration parsing is strict, not last-key-wins JSON admission.
    writeFileSync(join(project,'lekalo.yaml'),'{"schema":"lekalo/runtime-config/v1","components":{},"components":{}}');
    refused(run(project,['runtime','lock'],cache),'document-invalid');
  }finally{rmSync(scratch,{recursive:true,force:true});}
});

test('init creates a reviewable config and refuses to overwrite it',()=>{
  const root=mkdtempSync(join(process.env.BB_THREAD_STORAGE??tmpdir(),'lekalo-runtime-init-'));
  try{
    const cache=join(root,'unused');
    const legacy=run(root,['init','--dry-run','--project-id','legacy'],cache);
    assert.equal(legacy.status,0,legacy.stderr);
    assert(!existsSync(join(root,'lekalo.yaml')));
    ok(run(root,['init','--runtime','gh/example/unknown','--rules','gh/example/contracts','--component-path','src'],cache));
    const first=readFileSync(join(root,'lekalo.yaml'));
    assert.equal(JSON.parse(first).components.app.runtime,'gh/example/unknown');
    refused(run(root,['init','--runtime','gh/example/unknown'],cache),'config-exists');
    assert.deepEqual(readFileSync(join(root,'lekalo.yaml')),first);
  }finally{rmSync(root,{recursive:true,force:true});}
});

test('published schema owns the open configuration and wire shapes',async()=>{
  const require=createRequire(import.meta.url);
  const {default:Ajv2020}=require('ajv/dist/2020');
  assert.equal(require('ajv/package.json').version,'8.20.0');
  const schema=JSON.parse(readFileSync(new URL('../contracts/external-runtime.schema.v1.json',import.meta.url),'utf8'));
  const ajv=new Ajv2020({strict:false,allErrors:true});
  ajv.addSchema(schema);
  const valid=(kind,value)=>ajv.validate(`${schema.$id}#/$defs/${kind}`,value);
  const config={schema:'lekalo/runtime-config/v1',components:{unknown:{path:'src',runtime:'gh/anybody/never-seen-language',rules:['gh/another/portable-rules']}}};
  assert(valid('config',config),JSON.stringify(ajv.errors));
  config.components.unknown.language='must-not-be-registered';
  assert(!valid('config',config));
  assert(!valid('manifest',{schema:'lekalo/package/v1',id:'runtime',version:'1.0.0',kind:'runtime',files:['main']}),'runtime needs an entry');
  assert(valid('manifest',{schema:'lekalo/package/v1',id:'rules',version:'1.0.0',kind:'rules',files:['rules.json'],rules:'rules.json'}));
  assert(valid('result',{state:'complete',facts:{}}));
  assert(!valid('result',{state:'partial',facts:{}}));
  assert(!valid('response',{protocol:'lekalo/runtime/v99',request_id:`sha256:${'a'.repeat(64)}`,result:{}}));
});
