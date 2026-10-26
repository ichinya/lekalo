#!/usr/bin/env node
// Standalone source reader for disposable experiments; never invokes inference.
import fs from 'node:fs';
import path from 'node:path';
import {createHash,randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';

export const LEDGER_PATH='.benchmark/native-reads.json';
export const sourceReadInstructions=[
 'Read only the implementation needed for the task; a semantic capsule is not a mandatory extra dump.',
 'Track paths, versions and ranges already in context. Do not repeat unchanged source or duplicate Model/CLI projections.',
 'Batch known needed paths in one read where the result will fit the client output limit.',
 'Use node tools/workspace.mjs --help for reader syntax instead of opening its implementation.',
 'The reader emits only unseen lines for a source version; .benchmark/native-reads.json is collector state, not authored project content.',
 'If an editor requires its own Read, use it as the first read for that file, rather than preloading the same file through another tool.',
 'Respect editor requirements; use a compatible edit method when content is already available.',
 'A changed file, uncovered range, truncated output or lost context can justify a scoped reread. Record the reason with --replay when needed.',
 'Do not edit/reset collector state; use new-session only when the agent context was actually replaced, with a reason.',
].join(' ');
export function workspaceToolSource(){return fs.readFileSync(fileURLToPath(import.meta.url),'utf8');}
const hash=b=>createHash('sha256').update(b).digest('hex');
const help='list [src] | search [src] TEXT | read PATH [PATH...] [--lines START:END] [--replay REASON] | stats | new-session --reason REASON\nRead emits unseen lines only. Repeat unchanged ranges return metadata. Replays require a reason; changed versions remain readable. State is local to this agent context.\n';

function physical(root,relative,{optional=false}={}){
 if(typeof relative!=='string'||!relative||relative.includes('\\')||path.isAbsolute(relative)||relative.split('/').some(p=>!p||p==='.'||p==='..'||/[:\0]/.test(p)))throw Error('unsafe path');
 let current=root;
 for(const part of relative.split('/')){
  current=path.join(current,part);
  try{const m=fs.lstatSync(current);if(m.isSymbolicLink()||(m.isFile()&&m.nlink!==1))throw Error('linked path refused');}
  catch(e){if(optional&&e.code==='ENOENT')return current;throw e;}
 }
 return current;
}
function files(root){
 const out=[];
 function visit(dir){const full=physical(root,dir);if(!fs.statSync(full).isDirectory())throw Error('source directory required');
  for(const e of fs.readdirSync(full,{withFileTypes:true}).sort((a,b)=>Buffer.compare(Buffer.from(a.name),Buffer.from(b.name)))){
   const p=dir+'/'+e.name,actual=physical(root,p);
   if(e.isDirectory())visit(p);else if(e.isFile()&&p.endsWith('.ts'))out.push(p);else if(!fs.statSync(actual).isFile())throw Error('unsupported source path');
  }
 }visit('src');return out;
}
function parse(args){
 const names=[],options={};
 for(let i=0;i<args.length;i++){
  if(args[i]==='--lines'||args[i]==='--replay'||args[i]==='--reason'){const k=args[i].slice(2);if(options[k]!==undefined||!args[i+1]||args[i+1].startsWith('--'))throw Error('option value required');options[k]=args[++i];}
  else if(args[i].startsWith('--'))throw Error('unknown option');else names.push(args[i]);
 }
 if(options.lines!==undefined&&!/^[1-9][0-9]*:[1-9][0-9]*$/.test(options.lines))throw Error('line range required');
 for(const k of ['replay','reason'])if(options[k]!==undefined&&(options[k].trim().length<3||options[k].length>240||/[\x00-\x1f\x7f]/.test(options[k])))throw Error('specific replay/session reason required');
 return {names,options};
}
function ranges(indices){
 const result=[];for(const line of indices){const last=result.at(-1);if(last&&last[1]+1===line)last[1]=line;else result.push([line,line]);}return result;
}
function fresh(){return {format:1,session:randomUUID(),sequence:0,files:{},contexts:{},events:[]};}
function validate(s){
 if(!s||s.format!==1||typeof s.session!=='string'||!Number.isSafeInteger(s.sequence)||s.sequence<0||!s.files||typeof s.files!=='object'||Array.isArray(s.files)||!Array.isArray(s.events))throw Error('invalid read ledger');
 for(const [p,versions]of Object.entries(s.files)){if(!p.startsWith('src/')||!p.endsWith('.ts')||!versions||typeof versions!=='object'||Array.isArray(versions))throw Error('invalid read ledger');
  for(const [digest,v]of Object.entries(versions)){if(!/^[a-f0-9]{64}$/.test(digest)||!v||!Array.isArray(v.ranges)||typeof v.empty!=='boolean')throw Error('invalid read ledger');
   for(const r of v.ranges)if(!Array.isArray(r)||r.length!==3||!r.every(Number.isSafeInteger)||r[0]<1||r[1]<r[0]||r[2]<1||r[2]>s.sequence)throw Error('invalid read ledger');
  }
 }
 if(s.contexts!==undefined){if(!s.contexts||typeof s.contexts!=='object'||Array.isArray(s.contexts)||Object.entries(s.contexts).some(([k,v])=>!/^[a-f0-9]{64}$/.test(k)||!Number.isSafeInteger(v)||v<1||v>s.sequence))throw Error('invalid context ledger');}
 return s;
}
export function emitSemanticContext(text,replayReason){workspaceMain(['semantic-output',...(replayReason!==undefined?['--replay',replayReason]:[])],text);}
export function workspaceMain(argv=process.argv.slice(2),semanticText){
 const root=fs.realpathSync(process.cwd()),[mode,...args]=argv;
 if(!mode||mode==='--help'||mode==='-h'){process.stdout.write(help);return;}
 const {names,options}=parse(args);
 if(mode==='list'){if(options.lines||options.replay||options.reason||names.length>1||(names.length&&names[0]!=='src'))throw Error('list [src]');process.stdout.write(files(root).join('\n')+'\n');return;}
 if(!['read','search','stats','new-session','semantic-output'].includes(mode))throw Error(help.trim());
 const home=path.join(root,'.benchmark');physical(root,'.benchmark',{optional:true});fs.mkdirSync(home,{recursive:true});
 const ledger=physical(root,LEDGER_PATH,{optional:true}),lockPath=physical(root,LEDGER_PATH+'.lock',{optional:true});
 const lock=fs.openSync(lockPath,'wx',0o600);let stage=null;
 try{
  const stagePath=physical(root,LEDGER_PATH+'.stage',{optional:true});if(fs.existsSync(stagePath))throw Error('unowned read-ledger stage');
  let state=fs.existsSync(ledger)?validate(JSON.parse(fs.readFileSync(ledger,'utf8'))):fresh();
  if(mode==='stats'){if(names.length||Object.keys(options).length)throw Error('stats takes no arguments');process.stdout.write(JSON.stringify({session:state.session,emissions:state.sequence,emittedBytes:state.events.filter(e=>e.kind==='emit').reduce((n,e)=>n+e.bytes,0),replayBytes:state.events.filter(e=>e.kind==='emit'&&e.reason).reduce((n,e)=>n+e.bytes,0),reuseRequests:state.events.filter(e=>e.kind==='reuse').length,semanticBytes:state.events.filter(e=>e.kind==='semantic-emit').reduce((n,e)=>n+e.bytes,0)})+'\n');return;}
  if(mode==='new-session'){if(names.length||options.lines||options.replay||!options.reason)throw Error('new-session --reason REASON');const old=state.session;state={...fresh(),sequence:state.sequence,events:[...state.events,{kind:'new-session',previous:old,reason:options.reason}]};}
  else if(mode==='semantic-output'){
   if(typeof semanticText!=='string'||names.length||options.lines||options.reason)throw Error('semantic output requires an internal payload');
   const digest=hash(Buffer.from(semanticText));state.contexts??={};
   if(state.contexts[digest]&&!options.replay){process.stdout.write('@@semantic-reuse '+JSON.stringify({digest,session:state.session,delivery:state.contexts[digest]})+'\n');state.events.push({kind:'semantic-reuse',digest});}
   else{const id=++state.sequence,bytes=Buffer.byteLength(semanticText);process.stdout.write('@@semantic-content '+JSON.stringify({digest,bytes,session:state.session,delivery:id,...(options.replay?{reason:options.replay}:{})})+'\n'+semanticText);state.contexts[digest]=id;state.events.push({kind:'semantic-emit',digest,bytes,...(options.replay?{reason:options.replay}:{})});}
  }else{
   if(options.reason)throw Error('use --replay for a repeated source request');
   const available=files(root);let targets;
   if(mode==='search'){if(options.lines)throw Error('search takes a literal needle');if(names[0]==='src'&&names.length===2)names.shift();if(names.length!==1||!names[0])throw Error('search [src] TEXT');targets=available;}
   else{if(!names.length||(options.lines&&names.length!==1))throw Error('read PATH...; --lines requires one path');targets=names;}
   const requests=targets.map(p=>{
    if(!available.includes(p))throw Error('confined src/**/*.ts path required');
    const raw=fs.readFileSync(physical(root,p));if(raw.length>1024*1024)throw Error('source byte limit');
    new TextDecoder('utf-8',{fatal:true}).decode(raw);
    const text=raw.toString('utf8'),lines=text.match(/[^\n]*\n|[^\n]+$/g)??[],digest=hash(raw);
    let selected=lines.map((_,i)=>i+1);
    if(options.lines){const [start,end]=options.lines.split(':').map(Number);if(start>end||end>lines.length)throw Error('range outside source');selected=selected.filter(n=>n>=start&&n<=end);}
    if(mode==='search')selected=selected.filter(n=>lines[n-1].includes(names[0]));
    return {p,raw,lines,digest,selected};
   });
   // Preflight every request before emitting or updating any source coverage.
   for(const {p,raw,lines,digest,selected}of requests){
    if(mode==='search'&&!selected.length)continue;
    const versions=state.files[p]??={},version=versions[digest]??{ranges:[],empty:false};
    const seen=new Map();for(const [a,b,id]of version.ranges)for(let n=a;n<=b;n++)seen.set(n,id);
    const missing=options.replay?selected:selected.filter(n=>!seen.has(n));
    const reused=selected.filter(n=>seen.has(n));
    if(!options.replay&&((selected.length&&reused.length)||(!raw.length&&version.empty))){
     process.stdout.write('@@native-reuse '+JSON.stringify({path:p,sourceDigest:digest,lines:ranges(reused),deliveries:[...new Set(reused.map(n=>seen.get(n)))],empty:!raw.length})+'\n');
     state.events.push({kind:'reuse',path:p,sourceDigest:digest,lines:ranges(reused)});
    }
    const chunks=ranges(missing);if(!raw.length&&(!version.empty||options.replay))chunks.push([0,0]);
    for(const [start,end]of chunks){
     let body=start?lines.slice(start-1,end).join(''):'';if(body&&!body.endsWith('\n'))body+='\n';
     const bytes=Buffer.byteLength(body),id=++state.sequence;
     const receipt={path:p,kind:mode==='search'?'matching-lines':'range',session:state.session,startLine:start,endLine:end,sourceDigest:digest,delivery:id,bytes,sha256:hash(Buffer.from(body)),...(options.replay?{replayReason:options.replay}:{})};
     process.stdout.write('@@native-content '+JSON.stringify(receipt)+'\n'+body+'@@native-end '+p+'\n');
     if(start)version.ranges.push([start,end,id]);else version.empty=true;
     state.events.push({kind:'emit',path:p,sourceDigest:digest,delivery:id,startLine:start,endLine:end,bytes,...(options.replay?{reason:options.replay}:{})});
    }
    versions[digest]=version;state.files[p]=versions;
   }
  }
  const fd=fs.openSync(stagePath,'wx',0o600);stage=stagePath;
  try{fs.writeFileSync(fd,JSON.stringify(state)+'\n');}finally{fs.closeSync(fd);}
  fs.renameSync(stage,ledger);stage=null;
  if(mode==='new-session')process.stdout.write(JSON.stringify({session:state.session,reason:options.reason})+'\n');
 }finally{
  // Only the lock/stage created by this invocation are removed.
  if(stage&&fs.existsSync(stage))fs.unlinkSync(stage);
  fs.closeSync(lock);fs.unlinkSync(lockPath);
 }
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 try{workspaceMain();}catch(e){process.stderr.write(String(e.message)+'\n');process.exitCode=1;}
}
