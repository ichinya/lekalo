#!/usr/bin/env node
// Synthetic, dependency-free read-only scan/validation and fault fixture.
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {spawn,spawnSync} from 'node:child_process';
const hash=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
const adapter={id:'watch-fixture',version:'0.8.0',digest:hash(readFileSync(fileURLToPath(import.meta.url)))};
const canonical=value=>Array.isArray(value)?'['+value.map(canonical).join(',')+']':value&&typeof value==='object'?'{'+Object.keys(value).sort().map(key=>JSON.stringify(key)+':'+canonical(value[key])).join(',')+'}':JSON.stringify(value);
const selective = false; // replaced only by the opt-in selection tests
const modelVersions = ['0.2.16','0.8.0']; // exact explicit support, not a protocol inference
const request=JSON.parse(readFileSync(0,'utf8'));
const selection=request.watch_request;
let wrongSelection=false;
const respond=payload=>process.stdout.write(canonical({protocol:'lekalo.target/v1',protocol_version:request.protocol_version,operation:request.operation,request_id:request.request_id,status:'ok',evidence:{adapter,...(selection?{watch_digest:wrongSelection?'sha256:'+'0'.repeat(64):hash(canonical(selection))}:{})},...payload}));
if(request.operation==='describe') {
 respond({capabilities:{adapter,protocol_versions:selective?['0.3.2','0.8.0']:['0.3.2'],...(selective&&request.protocol_version==='0.8.0'?{watch:{selection:true,model_versions:modelVersions}}:{}),operations:['describe','scan','validate','verify'],targets:['watch-fixture'],profiles:['default'],read_scopes:['.lekalo/cache/ir/**','src/**'],write_scopes:[],transports:['stdin'],progress:false,ir_versions:modelVersions,capabilities:{'scan.symbols':'full'},constraints:{max_entries:1000}}});
} else {
 const control=readFileSync('src/control.txt','utf8').trim();
 wrongSelection=control==='bad-echo';
 if(control==='linux-limits') {
   const limits=readFileSync('/proc/self/limits','utf8');
   if(!/^Max address space\s+8589934592\s+8589934592\s+bytes[ \t]*$/m.test(limits)||!/^Max processes\s+64\s+64\s+processes[ \t]*$/m.test(limits))throw new Error('namespace bounds missing');
 }
 if(control==='tree-hang') {
   spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:['ignore','inherit','inherit']});
   await new Promise(resolve=>setTimeout(resolve,30000));
 }
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
 if(selection&&(!selective||request.protocol_version!=='0.8.0'))throw new Error('unnegotiated selection');
 if(selection) {
   const ir=JSON.parse(readFileSync(request.ir_path,'utf8'));
   if(!modelVersions.includes(selection.pins.model_version)||selection.pins.ir_version!==selection.pins.model_version||ir.contract!==`dev.lekalo.ir@${selection.pins.ir_version}`||ir.modelVersion!==selection.pins.model_version||hash(readFileSync(request.ir_path))!==selection.pins.ir_digest)throw new Error('stale or unknown pins');
 }
 if(selection&&selection.scope==='module'&&selection.module==='unrelated'&&selection.changed_paths.length===1&&selection.changed_paths[0].startsWith('lekalo/modules/planner/'))throw new Error('unrelated validation executed');
 if(request.operation==='scan')respond({result:{entries:selection&&selection.module!=='planner'?[]:[{path:control==='bad-scan'?'../escape':'src/control.txt',kind:'scalar',detail:JSON.stringify(control==='bad-detail'?{unexpected:true}:{s:'planner.title',n:'fixture:planner.title',q:'high',l:1})}],truncated:false}});
 else if(request.operation==='validate'||request.operation==='verify') {
   const ir=JSON.parse(readFileSync(request.ir_path,'utf8'));
   if(ir.contract!==`dev.lekalo.ir@${selection?.pins.ir_version??'0.2.16'}`)throw new Error('captured IR missing');
   respond({result:{ok:control!=='invalid'&&!(control==='unrelated-invalid'&&(!selection||selection.module==='unrelated')),findings:[]}});
 } else throw new Error('mutating/unexpected operation');
}
