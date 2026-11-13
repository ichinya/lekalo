import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {chmodSync,existsSync,lstatSync,mkdirSync,readFileSync,readdirSync,writeFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const manifestName='runtime.json';
const plain=p=>{const s=lstatSync(p);assert(s.isFile()&&!s.isSymbolicLink(),`Expected plain file: ${p}`);return readFileSync(p);};
function directory(p){const s=lstatSync(p);assert(s.isDirectory()&&!s.isSymbolicLink(),`Expected plain directory: ${p}`);}
function context(c){
 assert.deepEqual(Object.keys(c).sort(),['sourceSha','runId','attempt','platform','arch','image','imageOS','profile'].sort(),'Unexpected runtime context');
 assert(Object.values(c).every(v=>typeof v==='string'),'Runtime context must contain strings');
 assert(/^[a-f0-9]{40}$/.test(c.sourceSha),'Invalid source SHA');
 assert(/^\d+$/.test(c.runId),'Invalid workflow run');
 assert(/^[1-9]\d*$/.test(c.attempt),'Invalid producer attempt');
 assert(['linux','win32','darwin'].includes(c.platform),'Unsupported platform');
 assert(['x64','arm64'].includes(c.arch),'Unsupported architecture');
 assert(['ubuntu-26.04','windows-latest','macos-latest'].includes(c.image),'Unknown runner image');
 assert.equal(c.platform,{'ubuntu-26.04':'linux','windows-latest':'win32','macos-latest':'darwin'}[c.image],'Image/platform mismatch');
 // ImageOS is supplied by the hosted runner, not by the downloaded manifest.
 // Rolling label migrations must not silently cross system-image families.
 assert(/^[a-zA-Z0-9][a-zA-Z0-9._-]{1,63}$/.test(c.imageOS),'Invalid hosted image family');
 if(c.platform==='linux')assert.equal(c.imageOS,'ubuntu26','Expected Ubuntu 26.04 image family');
 else assert(new RegExp(c.platform==='win32'?'^win[0-9]+(?:[._-].*)?$':'^macos[0-9]+(?:[._-].*)?$').test(c.imageOS),'Image family/platform mismatch');
 assert(['debug','release'].includes(c.profile),'Invalid profile');
}
const names=c=>c.profile==='debug'?['lekalo','typescript-native-contract']:['lekalo'];
const filename=(name,c)=>name+(c.platform==='win32'?'.exe':'');
const outputPath=(name,c)=>name==='lekalo'?join(c.profile,filename(name,c)):join('debug','examples',filename(name,c));
export function prepare({source,destination,...c}){
 context(c);
 const files=names(c).map(name=>{const bytes=plain(join(source,outputPath(name,c)));assert(bytes.length>0,'Empty executable');return {name,bytes};});
 mkdirSync(dirname(destination),{recursive:true});mkdirSync(destination);
 const entries=files.map(({name,bytes})=>{const file=filename(name,c);writeFileSync(join(destination,file),bytes,{flag:'wx',mode:0o755});return {name,file,bytes:bytes.length,sha256:sha(bytes)};});
 const manifest={schemaVersion:1,...c,files:entries};
 const bytes=Buffer.from(JSON.stringify(manifest,null,2)+'\n');writeFileSync(join(destination,manifestName),bytes,{flag:'wx'});
 return {manifest,manifestSha256:sha(bytes)};
}
export function verify({directory:input,destination,manifestSha256,...expected}){
 context(expected);directory(input);
 assert(/^[a-f0-9]{64}$/.test(manifestSha256??''),'Missing producer manifest digest');
 const raw=plain(join(input,manifestName));assert.equal(sha(raw),manifestSha256,'Manifest digest mismatch');
 const m=JSON.parse(raw.toString('utf8'));
 assert.equal(m.schemaVersion,1,'Unknown manifest');
 assert.deepEqual(Object.keys(m).sort(),['schemaVersion',...Object.keys(expected),'files'].sort(),'Unexpected manifest fields');
 for(const [key,value] of Object.entries(expected))assert.equal(m[key],value,`Runtime context mismatch: ${key}`);
 assert(Array.isArray(m.files)&&m.files.length===names(expected).length,'Incorrect executable set');
 const validated=m.files.map((entry,index)=>{
   const name=names(expected)[index];assert.deepEqual(Object.keys(entry).sort(),['name','file','bytes','sha256'].sort(),'Unknown file metadata');
   assert.equal(entry.name,name);assert.equal(entry.file,filename(name,expected),'Unexpected artifact path');
   assert(Number.isSafeInteger(entry.bytes)&&entry.bytes>0,'Invalid executable size');
   assert(/^[a-f0-9]{64}$/.test(entry.sha256),'Invalid executable digest');
   const bytes=plain(join(input,entry.file));assert.equal(bytes.length,entry.bytes,'Executable size mismatch');assert.equal(sha(bytes),entry.sha256,'Executable digest mismatch');
   return {name,bytes};
 });
 assert.deepEqual(readdirSync(input).sort(),[manifestName,...m.files.map(f=>f.file)].sort(),'Unexpected artifact contents');
 // Validate every byte before installing anything or restoring execute bits.
 directory(destination);
 for(const {name} of validated){let parent=destination;const parts=outputPath(name,expected).split(/[\\/]/);parts.pop();for(const part of parts){parent=join(parent,part);if(existsSync(parent))directory(parent);}assert(!existsSync(join(destination,outputPath(name,expected))),'Refusing to replace an existing executable');}
 for(const {name,bytes} of validated){const dest=join(destination,outputPath(name,expected));mkdirSync(dirname(dest),{recursive:true});writeFileSync(dest,bytes,{flag:'wx',mode:0o600});chmodSync(dest,0o755);}
 return m;
}
function main(){
 const [mode,location]=process.argv.slice(2);assert(process.argv.length===4&&['prepare','verify'].includes(mode),'Usage: ci-runtime-artifacts.mjs prepare|verify <directory>');
 const sourceSha=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();assert.equal(sourceSha,process.env.GITHUB_SHA,'Checkout differs from workflow SHA');
 const c={sourceSha,runId:process.env.GITHUB_RUN_ID,attempt:process.env.RUNTIME_PRODUCER_ATTEMPT??process.env.GITHUB_RUN_ATTEMPT,platform:process.platform,arch:process.arch,image:process.env.RUNTIME_IMAGE,imageOS:process.env.ImageOS,profile:process.env.RUNTIME_PROFILE};
 if(mode==='prepare'){
   const r=prepare({source:resolve(process.env.CARGO_TARGET_DIR??'target'),destination:resolve(location),...c});
   if(process.env.GITHUB_OUTPUT){writeFileSync(process.env.GITHUB_OUTPUT,`manifest-sha256=${r.manifestSha256}\nproducer-attempt=${c.attempt}\n`,{flag:'a'});}
   console.log(JSON.stringify({prepared:true,sourceSha,profile:c.profile,manifestSha256:r.manifestSha256}));
 }else{
   const dest=resolve('target');mkdirSync(dest,{recursive:true});
   verify({directory:resolve(location),destination:dest,manifestSha256:process.env.RUNTIME_MANIFEST_SHA256,...c});
   console.log(JSON.stringify({verified:true,sourceSha,profile:c.profile,producerAttempt:c.attempt}));
 }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){try{main();}catch(e){console.error(e.message);process.exitCode=1;}}
