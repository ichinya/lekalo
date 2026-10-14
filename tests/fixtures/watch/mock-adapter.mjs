#!/usr/bin/env node
// Synthetic, dependency-free read-only scan/validation and fault fixture.
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const adapter={id:'watch-fixture',version:'0.8.0',digest:hash(readFileSync(fileURLToPath(import.meta.url)))};
const canonical=value=>Array.isArray(value)?'['+value.map(canonical).join(',')+']':value&&typeof value==='object'?'{'+Object.keys(value).sort().map(key=>JSON.stringify(key)+':'+canonical(value[key])).join(',')+'}':JSON.stringify(value);
const request=JSON.parse(readFileSync(0,'utf8'));
const respond=payload=>process.stdout.write(canonical({protocol:'lekalo.target/v1',protocol_version:request.protocol_version,operation:request.operation,request_id:request.request_id,status:'ok',evidence:{adapter},...payload}));
if(request.operation==='describe') {
 respond({capabilities:{adapter,protocol_versions:['0.3.2'],operations:['describe','scan','validate'],targets:['watch-fixture'],profiles:['default'],read_scopes:['.lekalo/cache/ir/**','src/**'],write_scopes:[],transports:['stdin'],progress:false,ir_versions:['0.2.16'],capabilities:{'scan.symbols':'full'},constraints:{max_entries:1000}}});
} else {
 const control=readFileSync('src/control.txt','utf8').trim();
 if(control==='child') {
   const child=spawnSync(process.execPath,['-e','setTimeout(()=>{},30000)'],{stdio:'ignore',timeout:500});
   if(!child.error||child.error.code==='ETIMEDOUT'||child.pid)throw new Error('child was not denied before execution');
 }
 if(control==='crash')process.exit(23);
 if(control==='hang')await new Promise(resolve=>setTimeout(resolve,30000));
 if(control==='write') {
   try {writeFileSync('src/control.txt','UNAUTHORIZED');throw new Error('write unexpectedly allowed');}
   catch(error) {if(error.message==='write unexpectedly allowed')throw error;}
 }
 if(request.operation==='scan')respond({result:{entries:[{path:control==='bad-scan'?'../escape':'src/control.txt',kind:'scalar',detail:JSON.stringify(control==='bad-detail'?{unexpected:true}:{s:'planner.title',n:'fixture:planner.title',q:'high',l:1})}],truncated:false}});
 else if(request.operation==='validate') {
   const ir=JSON.parse(readFileSync(request.ir_path,'utf8'));
   if(ir.contract!=='dev.lekalo.ir@0.2.16')throw new Error('captured IR missing');
   respond({result:{ok:control!=='invalid',findings:[]}});
 } else throw new Error('mutating/unexpected operation');
}
