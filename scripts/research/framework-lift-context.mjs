// Shared, bounded semantic capsule for research workspaces. No inference.
export function renderSemanticContext(context,mappings,symbol,note,compact=false){
 const lines=context.replaceAll('\r\n','\n').split('\n'),at=lines.findIndex(l=>l.startsWith('## '));
 const status=lines.slice(0,at<0?lines.length:at).filter(l=>l&&l!=='# context capsule'&&!/^- (identity|contract|ir digest|estimator|estimator digest):/.test(l));
 const body=at<0?context:lines.slice(at).join('\n');
 const paths=new Map();
 for(const row of mappings.bindings??[]){
  if(typeof row.path!=='string')continue;
  const data=paths.get(row.path)??[];data.push(row);paths.set(row.path,data);
 }
 const pointer=(path,rows)=>{
  if(!compact)return path+' -> '+rows.map(row=>(row.semantic??'?')+(row.line?':'+row.line:'')+' ['+row.source+', '+row.state+']').join(', ');
  const states=new Map();for(const row of rows){const key=(row.source??'unknown')+'/'+(row.state??'unknown');states.set(key,(states.get(key)??0)+1);}
  const locations=[...new Set(rows.map(row=>row.line??'unknown'))];
  return path+' -> lines '+locations.join(',')+'; '+[...states].sort().map(([state,n])=>state+':'+n).join(', ');
 };
 return '# Lekalo capsule '+symbol+'\n'+status.join('\n')+'\n'+body.trim()+'\n\nObserved source pointers (currency is not behavior conformance):\n'
  +[...paths].sort((a,b)=>a[0]<b[0]?-1:1).map(([path,rows])=>pointer(path,rows)).join('\n')
  +(compact?'\nPointer names omitted; use lekalo bindings list --json --project . for exact native identities when needed.':'')
  +'\n'+(note?note+'\n':'')+'Use uncovered source ranges only where these contracts are insufficient. Request impact separately when additional risks or gates are required.\n';
}
export function semanticToolSource({binary,symbol,budget=1200,note,compact=false}){
 return [
  "import {spawnSync} from 'node:child_process';",
  "import {emitSemanticContext} from './workspace.mjs';",
  'const config='+JSON.stringify({binary,symbol,budget,note,compact})+';',
  'const render='+renderSemanticContext.toString()+';',
  "function cli(args,json=false){const r=spawnSync(config.binary,[...(json?['--json']:[]),'--no-cache',...args,'--project','.'],{encoding:'utf8',timeout:120000,maxBuffer:16000000});if(r.stderr)process.stderr.write(r.stderr);if(r.status!==0){process.stdout.write(r.stdout??'');process.exit(r.status??1);}return json?JSON.parse(r.stdout):r.stdout;}",
  "const args=process.argv.slice(2);if(args.length&&(args.length!==2||args[0]!=='--replay'))throw Error('--replay REASON is the only option');",
  "const capsule=cli(['context',config.symbol,'--budget',String(config.budget)]),mappings=cli(['bindings','list'],true);",
  'emitSemanticContext(render(capsule,mappings,config.symbol,config.note,config.compact),args[1]);'
 ].join('\n')+'\n';
}

// Experimental native-change entrypoint, not a replacement for the installed
// product instruction or an instruction to omit required project policy.
export function compactResearchInstructions(path){
 if(!path)throw Error('installed instruction path required');
 return 'Experimental compact Lekalo workflow for this native-only public-fixture task. Run node tools/semantic.mjs once before source reads. Reuse its contracts and source pointers; read required implementation where contracts, mappings or coverage are insufficient. Native inventory does not prove behavior. Keep missing, stale, unsupported and failed evidence visible. Canonical and native IDs have no automatic join. Read unchanged source only once per digest/range; editor-required Read should be the first read. Changed bytes, truncation or lost context justify a reasoned replay. Do not dump the Model or repeat overlapping projections as a checklist. Request impact only for unresolved risks/gates. Change only authorized application source and run native checks. Leave the Model unchanged in this pilot; report stale semantics. Preserve project pins, unrelated edits and managed state. Before any Model, binding, managed-state or integration operation, read the full installed instruction '+path+' and applicable authority/privacy policy. Local semantic checks do not establish runtime or production acceptance. Do not install dependencies, run additional inference, publish, commit or deploy without authorization.\n';
}
