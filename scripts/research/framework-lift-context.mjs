// Shared, bounded semantic capsule for research workspaces. No inference.
export function renderSemanticContext(context,mappings,symbol,note){
 const lines=context.replaceAll('\r\n','\n').split('\n'),at=lines.findIndex(l=>l.startsWith('## '));
 const status=lines.slice(0,at<0?lines.length:at).filter(l=>l&&l!=='# context capsule'&&!/^- (identity|contract|ir digest|estimator|estimator digest):/.test(l));
 const body=at<0?context:lines.slice(at).join('\n');
 const paths=new Map();
 for(const row of mappings.bindings??[]){
  if(typeof row.path!=='string')continue;
  const data=paths.get(row.path)??[];data.push((row.semantic??'?')+(row.line?':'+row.line:'')+' ['+row.source+', '+row.state+']');paths.set(row.path,data);
 }
 return '# Lekalo capsule '+symbol+'\n'+status.join('\n')+'\n'+body.trim()+'\n\nObserved source pointers (currency is not behavior conformance):\n'
  +[...paths].sort((a,b)=>a[0]<b[0]?-1:1).map(([path,rows])=>path+' -> '+rows.join(', ')).join('\n')
  +'\n'+(note?note+'\n':'')+'Use uncovered source ranges only where these contracts are insufficient. Request impact separately when additional risks or gates are required.\n';
}
export function semanticToolSource({binary,symbol,budget=1200,note}){
 return [
  "import {spawnSync} from 'node:child_process';",
  "import {emitSemanticContext} from './workspace.mjs';",
  'const config='+JSON.stringify({binary,symbol,budget,note})+';',
  'const render='+renderSemanticContext.toString()+';',
  "function cli(args,json=false){const r=spawnSync(config.binary,[...(json?['--json']:[]),'--no-cache',...args,'--project','.'],{encoding:'utf8',timeout:120000,maxBuffer:16000000});if(r.stderr)process.stderr.write(r.stderr);if(r.status!==0){process.stdout.write(r.stdout??'');process.exit(r.status??1);}return json?JSON.parse(r.stdout):r.stdout;}",
  "const args=process.argv.slice(2);if(args.length&&(args.length!==2||args[0]!=='--replay'))throw Error('--replay REASON is the only option');",
  "const capsule=cli(['context',config.symbol,'--budget',String(config.budget)]),mappings=cli(['bindings','list'],true);",
  'emitSemanticContext(render(capsule,mappings,config.symbol,config.note),args[1]);'
 ].join('\n')+'\n';
}
