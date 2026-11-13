import assert from 'node:assert/strict';
import test from 'node:test';
import {execFileSync,spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,readdirSync,symlinkSync,unlinkSync,statSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {prepare,verify} from './ci-runtime-artifacts.mjs';
const hash=b=>createHash('sha256').update(b).digest('hex');
function fixture(t,platform='linux',profile='debug'){
 const root=mkdtempSync(join(tmpdir(),'lekalo-runtime-'));
 t.after(()=>rmSync(root,{recursive:true,force:true}));
 const c={sourceSha:'a'.repeat(40),runId:'1234',attempt:'1',platform,arch:platform==='darwin'?'arm64':'x64',image:{linux:'ubuntu-26.04',win32:'windows-latest',darwin:'macos-latest'}[platform],imageOS:{linux:'ubuntu26',win32:'win25',darwin:'macos26'}[platform],profile};
 const source=join(root,'source'),directory=join(root,'artifact'),destination=join(root,'target'),suffix=platform==='win32'?'.exe':'';
 mkdirSync(join(source,profile,'examples'),{recursive:true});mkdirSync(destination);
 writeFileSync(join(source,profile,'lekalo'+suffix),'fixture-cli');
 if(profile==='debug')writeFileSync(join(source,'debug/examples/typescript-native-contract'+suffix),'fixture-example');
 const r=prepare({source,destination:directory,...c});
 const options={directory,destination,manifestSha256:r.manifestSha256,...c};
 return {root,c,source,directory,destination,suffix,options,verify:()=>verify(options),mutate(fn){const m=JSON.parse(readFileSync(join(directory,'runtime.json')));fn(m);const b=JSON.stringify(m);writeFileSync(join(directory,'runtime.json'),b);options.manifestSha256=hash(b);}};
}
for(const platform of ['linux','win32','darwin'])for(const profile of ['debug','release'])test(`${platform}/${profile} installs the exact executable set`,t=>{
 const f=fixture(t,platform,profile);f.verify();
 assert.equal(readFileSync(join(f.destination,profile,'lekalo'+f.suffix),'utf8'),'fixture-cli');
 if(platform===process.platform&&process.platform!=='win32')assert.equal(statSync(join(f.destination,profile,'lekalo')).mode&0o777,0o755);
 assert.deepEqual(readdirSync(f.destination),[profile]);
 assert.throws(f.verify,/existing executable/);
});
for(const key of ['sourceSha','runId','attempt','platform','arch','image','imageOS','profile'])test(`rejects mismatched ${key} before installing`,t=>{
 const f=fixture(t);f.mutate(m=>m[key]=({sourceSha:'b'.repeat(40),runId:'1235',attempt:'2',platform:'darwin',arch:'arm64',image:'ubuntu-24.04',imageOS:'ubuntu24',profile:'release'})[key]);
 assert.throws(f.verify,/context mismatch/);assert.deepEqual(readdirSync(f.destination),[]);
});
for(const [name,change] of [
 ['schema',m=>m.schemaVersion=2],['unknown metadata',m=>m.extra=true],
 ['traversal',m=>m.files[0].file='../escape'],['absolute path',m=>m.files[0].file='/escape'],
 ['unknown executable',m=>m.files[0].name='shell'],['missing entry',m=>m.files.pop()],
 ['size',m=>m.files[0].bytes++],['digest',m=>m.files[0].sha256='b'.repeat(64)],
 ['negative size',m=>m.files[0].bytes=-1],['unknown file metadata',m=>m.files[0].mode=777],
])test(`rejects ${name} even with a matching manifest digest`,t=>{const f=fixture(t);f.mutate(change);assert.throws(f.verify);assert.deepEqual(readdirSync(f.destination),[]);});
test('refuses tampered manifest, bytes, unknown and missing files',t=>{
 const f=fixture(t);writeFileSync(join(f.directory,'lekalo'),'changed-cli');assert.throws(f.verify,/digest mismatch/);
 writeFileSync(join(f.directory,'lekalo'),'fixture-cli');writeFileSync(join(f.directory,'unknown'),'bad');assert.throws(f.verify,/contents/);unlinkSync(join(f.directory,'unknown'));
 unlinkSync(join(f.directory,'typescript-native-contract'));assert.throws(f.verify,/ENOENT/);assert.deepEqual(readdirSync(f.destination),[]);
 const g=fixture(t);writeFileSync(join(g.directory,'runtime.json'),'{}');assert.throws(g.verify,/Manifest digest/);
});
test('refuses a missing producer digest and invalid context',t=>{
 const f=fixture(t);delete f.options.manifestSha256;assert.throws(f.verify,/digest/);
 f.options.attempt='0';assert.throws(f.verify,/attempt/);
});
test('consumer attempt 2 accepts producer attempt 1; producer attempt 2 is distinct',t=>{
 const f=fixture(t),old=process.env.GITHUB_RUN_ATTEMPT;process.env.GITHUB_RUN_ATTEMPT='2';
 try{assert.equal(f.verify().attempt,'1');}finally{if(old===undefined)delete process.env.GITHUB_RUN_ATTEMPT;else process.env.GITHUB_RUN_ATTEMPT=old;}
 const next=join(f.root,'artifact-2');const r=prepare({source:f.source,destination:next,...f.c,attempt:'2'});
 assert.notEqual(r.manifestSha256,f.options.manifestSha256);assert.throws(()=>verify({...f.options,directory:next,manifestSha256:r.manifestSha256}),/attempt/);
});
test('refuses symlink files and destination directories',t=>{
 // Windows unprivileged processes cannot create file symlinks. A junction
 // exercises the destination guard there; Linux/macOS also test file links.
 const f=fixture(t),outside=join(f.root,'outside');mkdirSync(outside);
 symlinkSync(outside,join(f.destination,'debug'),process.platform==='win32'?'junction':'dir');
 assert.throws(f.verify,/plain directory/);assert.deepEqual(readdirSync(outside),[]);
 if(process.platform!=='win32'){
  unlinkSync(join(f.destination,'debug'));const binary=join(f.directory,'lekalo');unlinkSync(binary);symlinkSync(join(f.source,'debug/lekalo'),binary);
  assert.throws(f.verify,/plain file/);
 }
});
test('consumer CLI uses the producer attempt and refuses corruption before gates',t=>{
 const f=fixture(t,process.platform);
 const git=args=>execFileSync('git',args,{cwd:f.root,encoding:'utf8',windowsHide:true}).trim();
 git(['init','--quiet']);git(['-c','user.name=Runtime Test','-c','user.email=runtime@example.invalid','-c','commit.gpgsign=false','-c','core.hooksPath=/dev/null','commit','--quiet','--allow-empty','-m','fixture']);
 const sourceSha=git(['rev-parse','HEAD']);f.mutate(m=>m.sourceSha=sourceSha);
 const script=fileURLToPath(new URL('./ci-runtime-artifacts.mjs',import.meta.url));
 const env={...process.env,GITHUB_SHA:sourceSha,GITHUB_RUN_ID:f.c.runId,GITHUB_RUN_ATTEMPT:'2',RUNTIME_PRODUCER_ATTEMPT:'1',RUNTIME_MANIFEST_SHA256:f.options.manifestSha256,RUNTIME_IMAGE:f.c.image,ImageOS:f.c.imageOS,RUNTIME_PROFILE:f.c.profile};
 writeFileSync(join(f.directory,'lekalo'+f.suffix),'corrupted');
 const bad=spawnSync(process.execPath,[script,'verify',f.directory],{cwd:f.root,env,encoding:'utf8'});
 assert.equal(bad.status,1,bad.stderr);assert.match(bad.stderr,/mismatch/);assert.deepEqual(readdirSync(f.destination),[]);
 writeFileSync(join(f.directory,'lekalo'+f.suffix),'fixture-cli');
 const good=spawnSync(process.execPath,[script,'verify',f.directory],{cwd:f.root,env,encoding:'utf8'});
 assert.equal(good.status,0,good.stderr);assert.equal(JSON.parse(good.stdout).producerAttempt,'1');
});
