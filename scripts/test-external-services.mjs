import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,cpSync,rmSync,existsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
const cli=process.env.LEKALO_BIN??resolve('target/debug/lekalo');
const emit=(p,v)=>writeFileSync(p,JSON.stringify(v));
function fixture(t) {
 const dir=mkdtempSync(join(process.env.BB_THREAD_STORAGE??tmpdir(),'lekalo-services-'));
 t.after(()=>rmSync(dir,{recursive:true,force:true}));
 const project=join(dir,'project');mkdirSync(project);const entry=join(dir,'service.mjs');
 const code=`import {readFileSync,writeFileSync} from 'node:fs';
const req=JSON.parse(readFileSync(0,'utf8')),input=req.input,api='lekalo/discovery/v1';
let result;
if(req.operation==='describe')result={id:'independent-zen-services',version:'1.0.0',rules_api:[],capabilities:['discovery/v1','requirements.snapshot/v1','trace.mappings/v1'],operations:['discovery.describe','discovery.inspect','requirements.snapshot','trace.mappings']};
else if(req.operation==='discovery.describe')result={schema:api,files:['zen.project'],reads:['zen.project']};
else if(req.operation==='discovery.inspect'){
 const mode=input.files[0].value?.mode;
 const path=mode==='forge'?'../outside':input.files[0].path;
 result={schema:api,manifests:[{path,detail:'zen'}],managers:[],hints:[{path,label:'zen',kind:'language',field:null,confidence:'high'}],workspaces:[],tools:[],gates:[],modules:[],names:[{path,name:'zen-example',source:'zen.project#name',priority:0}],refused:[]};
 if(mode==='version')result.schema='lekalo/discovery/v99';
 if(mode==='workspace')result.workspaces=[{path:'.',ecosystem:'zen',members:['src/*'],confidence:'high'}];
 // No application filesystem view is granted to the provider process.
 try{readFileSync(${JSON.stringify(join(project,'.private'))});process.exit(99);}catch{}
 try{writeFileSync(${JSON.stringify(join(project,'.private'))},'changed');process.exit(99);}catch{}
}
else if(req.operation==='requirements.snapshot')result={schema:'lekalo/requirements-provider/v1',state:'read',requests:[{kind:'file',path:'../outside'}]};
else if(req.operation==='trace.mappings')result={schema:'lekalo/trace-mappings/v1',results:[]};
else process.exit(2);
process.stdout.write(JSON.stringify({protocol:req.protocol,request_id:req.request_id,result}));`;
 writeFileSync(entry,code);writeFileSync(join(project,'.private'),'unchanged');emit(join(project,'zen.project'),{name:'zen-example'});
 const config={schema:'lekalo/runtime-config/v1',components:{},services:{'discovery/unknown':{command:[process.execPath,entry]}}};
 emit(join(project,'lekalo.yaml'),config);
 const run=(args,cwd=project)=>{const env={...process.env,LEKALO_PACKAGE_HOME:join(dir,'cache')};delete env.LEKALO_SERVICES;delete env.LEKALO_PROJECT;return spawnSync(cli,['--json',...args],{cwd,env,encoding:'utf8',timeout:120000});};
 const ok=(args,cwd)=>{const r=run(args,cwd);assert.equal(r.status,0,r.stderr||r.stdout);return JSON.parse(r.stdout);};
 return {dir,project,entry,code,config,run,ok,save:()=>emit(join(project,'lekalo.yaml'),config)};
}
test('unknown discovery service is selected and locked without registering its language',t=>{
 const f=fixture(t);const locked=f.ok(['runtime','lock']).lock;
 assert.equal(locked.services['discovery/unknown'].description.id,'independent-zen-services');
 f.ok(['runtime','lock','--offline']);
 const r=f.ok(['init','--adopt','--project','.','--dry-run']);
 assert.match(JSON.stringify(r),/zen-example/);assert.match(JSON.stringify(r),/"label":"zen"/);
 assert.equal(readFileSync(join(f.project,'.private'),'utf8'),'unchanged');
 for(const mode of ['forge','version']) {emit(join(f.project,'zen.project'),{mode});const r=f.run(['init','--adopt','--project','.','--dry-run']);assert.notEqual(r.status,0);assert.match(r.stderr,/discovery-response/);}
 writeFileSync(f.entry,f.code+'\n// tampered');const stale=f.run(['init','--adopt','--project','.','--dry-run']);assert.notEqual(stale.status,0);assert.match(stale.stderr,/runtime-changed/);
});
test('missing or stale service locks never silently disable configured discovery',t=>{
 const f=fixture(t);let r=f.run(['init','--adopt','--project','.','--dry-run']);assert.notEqual(r.status,0);
 f.ok(['runtime','lock']);f.config.services['discovery/unknown'].command.push('changed');f.save();
 r=f.run(['init','--adopt','--project','.','--dry-run']);assert.notEqual(r.status,0);assert.match(r.stderr,/lock-stale/);
});
test('nested adoption resolves the locked ancestor discovery selection',t=>{
 const f=fixture(t),nested=join(f.project,'src','nested');mkdirSync(nested,{recursive:true});
 f.ok(['runtime','lock']);
 const args=['init','--adopt','--dry-run'];
 const r=f.ok(args,nested);
 assert.equal(r.detection.basis,'manifest');assert.deepEqual(r.detection.candidates,['../..']);assert.equal(r.projectId,'zen_example');
 assert(!existsSync(join(f.project,'lekalo')));assert(!existsSync(join(nested,'lekalo')));
 emit(join(f.project,'zen.project'),{mode:'workspace'});
 const workspace=f.ok(args,nested);assert.equal(workspace.detection.basis,'workspace-root');assert.deepEqual(workspace.detection.candidates,['../..']);
 // Explicit root selection still wins over ancestor inference.
 const explicit=f.ok([...args,'--project','.'],nested);assert.equal(explicit.detection.basis,'explicit');assert.equal(explicit.projectId,'nested');
 // A stale ancestor selection must refuse rather than fall back to this cwd.
 f.config.services['discovery/unknown'].command.push('changed');f.save();
 const stale=f.run(args,nested);assert.notEqual(stale.status,0);assert.match(stale.stderr,/lock-stale/);
 assert(!existsSync(join(nested,'lekalo')));
});
test('requirements providers cannot ask the host to read outside their declared root',t=>{
 const f=fixture(t);cpSync(resolve('tests/fixtures/requirements/planner'),f.project,{recursive:true});
 f.config.services={requirements:{command:[process.execPath,f.entry]}};f.save();f.ok(['runtime','lock']);
 const r=f.run(['requirements','validate','requirements.attachment.json','--project','.']);assert.notEqual(r.status,0);assert.match(r.stderr,/provider-read-scope/);
});
test('trace service results must cover every requested mapping',t=>{
 const f=fixture(t);f.config.services={trace:{command:[process.execPath,f.entry]}};f.save();f.ok(['runtime','lock']);
 const r=f.run(['trace','assess',resolve('tests/fixtures/trace-assessment/trace.json'),'--evidence',resolve('tests/fixtures/trace-assessment/input/ready.json')]);
 assert.notEqual(r.status,0);assert.match(r.stderr,/provider-response/);
});

test('service DTO schemas reject unknown fields and incompatible versions',()=>{
 const require=createRequire(import.meta.url);const Ajv=require('ajv/dist/2020').default;
 const schema=JSON.parse(readFileSync(resolve('contracts/external-services.schema.v1.json')));
 const ajv=new Ajv({strict:false});ajv.addSchema(schema);
 for(const [kind,value] of [
  ['discoveryDescriptor',{schema:'lekalo/discovery/v1',files:['zen.project'],reads:['zen.project']}],
  ['requirementsResponse',{schema:'lekalo/requirements-provider/v1',state:'read',requests:[{kind:'file',path:'specs/one.md'}]}],
  ['RawTraceResult',{schema:'lekalo/trace-mappings/v1',results:[{id:'opaque',state:'valid'}]}],
 ]) {
  const valid=ajv.getSchema(`${schema.$id}#/$defs/${kind}`);assert(valid,kind);
  assert(valid(value),JSON.stringify(valid.errors));assert(!valid({...value,extra:true}));assert(!valid({...value,schema:'unknown'}));
 }
});
