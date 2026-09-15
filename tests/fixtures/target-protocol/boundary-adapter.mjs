// Executable hostile-adapter controls. Every supplied host path belongs to
// the invoking test; no real user project or secrets are probed.
import * as fs from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { dirname, join } from 'node:path';
const arg = key => { const i = process.argv.indexOf(key); return i < 0 ? undefined : process.argv[i+1]; };
const mode = arg('--mode') ?? 'normal';
const host = arg('--host-root');
const digest = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const adapter = { id:'test', version:'1.0.0', digest:digest('boundary adapter v1') };
const file = arg('--lekalo-request-file');
const req = JSON.parse(fs.readFileSync(file ?? 0, 'utf8'));
const res = { protocol:'lekalo.target/v1', protocol_version:'1.0.0', operation:req.operation, request_id:req.request_id, status:'ok', evidence:{adapter} };
const caps = { adapter, protocol_versions:['1.0.0'], operations:['describe','scan','bind','validate','verify','generate','plan-clean','clean'], transports:['stdin','file'], targets:['test'], profiles:['default','other'], read_scopes:['.lekalo/ir/**'], write_scopes:['out/**'], progress:false };
const output = arg('--output') ?? 'out/file.txt';
if (arg('--write-scopes')) caps.write_scopes = arg('--write-scopes').split(',');
function attempt(fn) { try { fn(); return 'allowed'; } catch { return 'denied'; } }
function put(path, content='forbidden') { fs.mkdirSync(dirname(path),{recursive:true}); fs.writeFileSync(path,content); }
if (mode === 'hang' && req.operation !== 'describe') setTimeout(()=>process.exit(),3000);
else if (mode === 'descendant' && req.operation !== 'describe') {
  spawn(process.execPath,['--preserve-symlinks','--preserve-symlinks-main','-e',"setTimeout(()=>{},3000)"],{stdio:['ignore','inherit','inherit']});
  setTimeout(()=>process.exit(),3000);
} else {
  if (req.operation === 'describe') {
    if (mode === 'alias') caps.write_scopes=['./lekalo/**'];
    if (mode === 'empty-read') caps.read_scopes=[];
    if (mode === 'file-transport') caps.transports=['file'];
    if (mode === 'invalid-refresh' && arg('--invalid') === 'yes') res.protocol_version='0.9.0';
    if (mode === 'describe-mutate') put('lekalo/model.yaml');
    if (mode === 'describe-read') {
      if (attempt(()=>fs.readFileSync(join(host,'.lekalo/ir/input.json'))) !== 'denied') throw new Error('describe-read-escaped');
      if (attempt(()=>put(join(host,'other/new.txt'))) !== 'denied') throw new Error('describe-write-escaped');
    }
    res.capabilities=caps;
  } else if (mode === 'unicode-error') {
    res.status='error';
    res.error={class:'invalid',code:arg('--error-unit').repeat(Number(arg('--error-repeat'))),message:'owned synthetic error'};
  } else if (req.operation === 'scan') {
    const entries=[];
    if (mode === 'scope-probes') {
      for (const [name,path,write] of [
        ['scoped-read','.lekalo/ir/input.json',false], ['empty-read','private.txt',false],
        ['canonical-relative','lekalo/model.yaml',true], ['outside-relative','other/new.txt',true],
        ['absolute-read',join(host,'private.txt'),false], ['absolute-write',join(host,'private.txt'),true],
        ['absolute-canonical',join(host,'lekalo/model.yaml'),true],
        ['dry-output','out/forbidden.txt',true], ['directory','out/empty',true],
      ]) {
        const result=attempt(()=> name==='directory'?fs.mkdirSync(path,{recursive:true}):write?put(path):fs.readFileSync(path));
        entries.push({path:name,kind:result});
      }
      entries.push({path:'write-content',kind:fs.readFileSync('out/existing.txt','utf8')===''?'redacted':'leaked'});
    }
    if (mode === 'scan-secret') entries.push({path:'C:/Users/private-secret/token',kind:'source'});
    res.result={entries,truncated:false};
    if (mode === 'extra-capabilities') res.capabilities=caps;
  } else if (req.operation === 'validate' || req.operation === 'verify') {
    fs.readFileSync(req.ir_path);
    res.result={ok:true,findings:[]};
  } else if (req.operation === 'bind') res.result={bindings:[{module:'test',target:'test',profile:'default'}]};
  else {
    const path=output;
    const cleaning=req.operation==='clean'||req.operation==='plan-clean';
    const action=cleaning?'delete':mode==='replace'?'replace':'create';
    const writes=[{path,action,...(cleaning?{}:{sha256:digest('generated')})}];
    res.writes=writes;
    res.evidence.plan_id=req.plan_id??('plan-'+createHash('sha256').update(JSON.stringify(writes)).digest('hex'));
    if (mode==='mutate-dry' && req.dry_run===true) put(path);
    if (req.operation==='clean'||req.dry_run===false) {
      if (cleaning) fs.unlinkSync(path); else put(path,'generated');
      if (mode==='outside-apply') put('other/hidden.txt');
      if (mode==='extra-write'||mode==='apply-error') put('out/undeclared.txt');
      if (mode==='sibling-write') put(join(dirname(path),'undeclared.txt'));
      if (mode==='input-write') put(req.ir_path);
      if (mode==='no-echo') delete res.evidence.plan_id;
      if (mode==='apply-error') {
        res.status='error'; delete res.writes;
        res.error={class:'conflict',code:'C:/Users/private-secret/token',message:'owned probe',partial:true};
      }
    }
  }
  process.stdout.write(JSON.stringify(res));
}
