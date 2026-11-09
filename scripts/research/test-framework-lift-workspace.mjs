import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join,dirname,resolve,basename} from 'node:path';
import {workspaceToolSource,LEDGER_PATH} from './framework-lift-workspace.mjs';
import {markedReads} from './framework-lift-runner.mjs';
import {renderSemanticContext,semanticToolSource} from './framework-lift-context.mjs';
const hash=s=>createHash('sha256').update(s).digest('hex');
function sandbox(fn){
 const root=fs.mkdtempSync(join(tmpdir(),'lekalo-reader-'));
 try{
  fs.mkdirSync(join(root,'src'));fs.mkdirSync(join(root,'tools'));
  fs.writeFileSync(join(root,'src/task.ts'),'one\nneedle two\nthree\nneedle four\nfive\n');
  fs.writeFileSync(join(root,'tools/workspace.mjs'),workspaceToolSource());
  const run=(args,status=0)=>{const r=spawnSync(process.execPath,['tools/workspace.mjs',...args],{cwd:root,encoding:'utf8',timeout:10000});assert.equal(r.status,status,r.stdout+'\n'+r.stderr);return r;};
  fn(root,run);
 }finally{assert.equal(dirname(root),resolve(tmpdir()));assert.ok(basename(root).startsWith('lekalo-reader-'));fs.rmSync(root,{recursive:true,force:true});}
}
function deliveries(r){
 return [...r.stdout.matchAll(/@@native-content ([^\r\n]+)\n([\s\S]*?)@@native-end ([^\r\n]+)\n/g)].map(m=>{const receipt=JSON.parse(m[1]);assert.equal(receipt.path,m[3]);assert.equal(receipt.bytes,Buffer.byteLength(m[2]));assert.equal(receipt.sha256,hash(m[2]));return {...receipt,body:m[2]};});
}
test('a second process reuses unchanged contents; originals and helper stay unchanged',()=>sandbox((root,run)=>{
 const before=fs.readFileSync(join(root,'src/task.ts')),tool=fs.readFileSync(join(root,'tools/workspace.mjs'));
 assert.equal(deliveries(run(['read','src/task.ts'])).length,1);
 const again=run(['read','src/task.ts']);assert.equal(deliveries(again).length,0);assert.match(again.stdout,/@@native-reuse/);assert.ok(!again.stdout.includes('needle two'));
 assert.deepEqual(fs.readFileSync(join(root,'src/task.ts')),before);assert.deepEqual(fs.readFileSync(join(root,'tools/workspace.mjs')),tool);
 const stats=JSON.parse(run(['stats']).stdout);assert.equal(stats.reuseRequests,1);assert.equal(stats.replayBytes,0);
}));
test('overlapping ranges emit only uncovered source lines',()=>sandbox((root,run)=>{
 assert.equal(deliveries(run(['read','src/task.ts','--lines','1:3']))[0].body,'one\nneedle two\nthree\n');
 const next=deliveries(run(['read','src/task.ts','--lines','2:5']));assert.equal(next.length,1);assert.equal(next[0].startLine,4);assert.equal(next[0].body,'needle four\nfive\n');
 assert.equal(deliveries(run(['read','src/task.ts'])).length,0);
}));
test('search and full read share coverage without repeating matching lines',()=>sandbox((root,run)=>{
 assert.deepEqual(deliveries(run(['search','needle'])).map(r=>r.startLine),[2,4]);
 const rest=deliveries(run(['read','src/task.ts']));assert.deepEqual(rest.map(r=>r.startLine),[1,3,5]);assert.ok(rest.every(r=>!r.body.includes('needle')));
 assert.equal(deliveries(run(['search','src','needle'])).length,0);
}));
test('changed bytes are readable; reverting to an already seen version reuses it',()=>sandbox((root,run)=>{
 const file=join(root,'src/task.ts'),before=fs.readFileSync(file);run(['read','src/task.ts']);
 fs.writeFileSync(file,'new content\n');const fresh=deliveries(run(['read','src/task.ts']));assert.equal(fresh[0].body,'new content\n');assert.equal(fresh[0].sourceDigest,hash(fs.readFileSync(file)));
 fs.writeFileSync(file,before);assert.equal(deliveries(run(['read','src/task.ts'])).length,0);
}));
test('explicit replay needs a reason and is accounted separately',()=>sandbox((root,run)=>{
 run(['read','src/task.ts']);const before=fs.readFileSync(join(root,LEDGER_PATH));
 run(['read','src/task.ts','--replay'],1);assert.deepEqual(fs.readFileSync(join(root,LEDGER_PATH)),before);
 const replay=deliveries(run(['read','src/task.ts','--lines','2:2','--replay','earlier tool result was truncated']));assert.equal(replay[0].body,'needle two\n');assert.ok(replay[0].replayReason);
 assert.equal(JSON.parse(run(['stats']).stdout).replayBytes,Buffer.byteLength('needle two\n'));
}));
test('a genuinely new context explicitly resets coverage',()=>sandbox((root,run)=>{
 run(['read','src/task.ts']);const first=JSON.parse(run(['stats']).stdout).session;
 run(['new-session'],1);run(['new-session','--reason','agent context replaced after compaction']);
 assert.notEqual(JSON.parse(run(['stats']).stdout).session,first);assert.equal(deliveries(run(['read','src/task.ts'])).length,1);
}));
test('all paths/ranges are preflighted before any content or coverage update',()=>sandbox((root,run)=>{
 for(const args of [['read','src/task.ts','../outside.ts'],['read','src/task.ts','--lines','2:99'],['read','C:/outside.ts'],['read','src\\task.ts']]){
  const r=run(args,1);assert.equal(r.stdout,'');assert.ok(!fs.existsSync(join(root,LEDGER_PATH)));
 }
 assert.equal(deliveries(run(['read','src/task.ts'])).length,1);
}));
test('junctions and hard-linked source files refuse without disclosing outside content',()=>sandbox((root,run)=>{
 fs.mkdirSync(join(root,'outside'));fs.writeFileSync(join(root,'outside/secret.ts'),'outside sentinel');
 fs.symlinkSync(join(root,'outside'),join(root,'src/link'),process.platform==='win32'?'junction':'dir');
 assert.equal(run(['read','src/task.ts'],1).stdout,'');fs.unlinkSync(join(root,'src/link'));
 fs.linkSync(join(root,'outside/secret.ts'),join(root,'src/hard.ts'));assert.equal(run(['read','src/hard.ts'],1).stdout,'');
}));
test('foreign locks and stages survive refusals unchanged',()=>sandbox((root,run)=>{
 fs.mkdirSync(join(root,'.benchmark'));const lock=join(root,LEDGER_PATH+'.lock'),stage=join(root,LEDGER_PATH+'.stage');
 fs.writeFileSync(lock,'other invocation');assert.equal(run(['read','src/task.ts'],1).stdout,'');assert.equal(fs.readFileSync(lock,'utf8'),'other invocation');fs.unlinkSync(lock);
 fs.writeFileSync(stage,'unrelated stage');assert.equal(run(['read','src/task.ts'],1).stdout,'');assert.equal(fs.readFileSync(stage,'utf8'),'unrelated stage');assert.ok(!fs.existsSync(lock));
}));
test('corrupt ledger refuses instead of silently reprinting a whole file',()=>sandbox((root,run)=>{
 fs.mkdirSync(join(root,'.benchmark'));fs.writeFileSync(join(root,LEDGER_PATH),'{broken');
 assert.equal(run(['read','src/task.ts'],1).stdout,'');assert.equal(fs.readFileSync(join(root,LEDGER_PATH),'utf8'),'{broken');
}));
test('CRLF, Unicode, no-final-newline and empty files have exact byte receipts',()=>sandbox((root,run)=>{
 for(const [name,body]of [['crlf','a\r\nb\r\n'],['unicode','Пример\n'],['bom','\ufeffretained BOM\n'],['final','no final newline'],['empty','']]){
  const p='src/'+name+'.ts';fs.writeFileSync(join(root,p),body);const result=deliveries(run(['read',p]));assert.equal(result.length,1);assert.equal(result[0].sourceDigest,hash(Buffer.from(body)));assert.equal(result[0].body,body&& !body.endsWith('\n')?body+'\n':body);assert.equal(deliveries(run(['read',p])).length,0);
 }
}));
test('duplicate path arguments do not cause a second emission',()=>sandbox((_root,run)=>{
 const r=run(['read','src/task.ts','src/task.ts']);assert.equal(deliveries(r).length,1);assert.match(r.stdout,/@@native-reuse/);
}));
test('collector distinguishes unexplained duplicates, explicit replay and a new context',()=>sandbox((_root,run)=>{
 const first=run(['read','src/task.ts','--lines','1:2']).stdout;
 assert.equal(markedReads([{content:first},{content:first}]).unexplainedRepeatedRanges.length,1);
 const replay=run(['read','src/task.ts','--lines','2:2','--replay','prior output was unavailable']).stdout;
 const accounted=markedReads([{content:first},{content:replay}]);assert.equal(accounted.unexplainedRepeatedRanges.length,0);assert.equal(accounted.overlappingSourceRanges.length,1);
 run(['new-session','--reason','new agent context']);const next=run(['read','src/task.ts','--lines','1:2']).stdout;
 assert.equal(markedReads([{content:first},{content:next}]).unexplainedRepeatedRanges.length,0);
}));
test('collector verifies raw CRLF bytes without silently changing their digest',()=>sandbox((root,run)=>{
 fs.writeFileSync(join(root,'src/task.ts'),'a\r\nb\r\n');const output=run(['read','src/task.ts']).stdout;
 const result=markedReads([{content:output}]);assert.equal(result.unverified.length,0);assert.equal(result.deliveredBytes,6);
}));
test('unchanged semantic text is reused; changed output and reasoned replay remain available',()=>sandbox((root,run)=>{
 const file=join(root,'tools/semantic.mjs');
 function semantic(text,reason){fs.writeFileSync(file,"import {emitSemanticContext} from './workspace.mjs';emitSemanticContext("+JSON.stringify(text)+","+JSON.stringify(reason)+");\n");const r=spawnSync(process.execPath,['tools/semantic.mjs'],{cwd:root,encoding:'utf8'});assert.equal(r.status,0,r.stderr);return r.stdout;}
 assert.match(semantic('gaps: no-policies\n'),/@@semantic-content/);
 const repeat=semantic('gaps: no-policies\n');assert.match(repeat,/@@semantic-reuse/);assert.ok(!repeat.includes('gaps: no-policies'));
 assert.match(semantic('gaps: new unsupported obligation\n'),/new unsupported obligation/);
 assert.match(semantic('gaps: no-policies\n','previous result was truncated'),/gaps: no-policies/);
 assert.equal(JSON.parse(run(['stats']).stdout).emittedBytes,0);
}));
test('compact rendering preserves contracts, gaps, incomplete status and unknown header facts',()=>{
 const body='## symbol\nTask fields: id, title\n\n## gaps\nerror-contracts-unrepresentable\nno-policies\n';
 const raw='# context capsule\n- identity: verbose\n- project: planner\n- complete: false\nwarning: required evidence unavailable\n- coverage: 2 included, 1 excluded\n- future obligation: unresolved\n\n'+body;
 const result=renderSemanticContext(raw,{bindings:[{semantic:'planner.Task',path:'src/task.ts',line:1,source:'user-confirmed',state:'stale'},{semantic:'planner.NewTask',path:'src/task.ts',line:4,source:'inferred',state:'unknown'}]},'planner.task','No automatic semantic/native join.');
 assert.ok(result.includes(body.trim()));for(const token of ['complete: false','required evidence unavailable','2 included, 1 excluded','future obligation: unresolved','planner.Task','planner.NewTask','stale','unknown','No automatic semantic/native join.'])assert.ok(result.includes(token),token);
 assert.equal(result.split('\n').filter(l=>l.startsWith('src/task.ts ->')).length,1);
});
test('generated semantic helper is standalone within the workspace tools directory',()=>sandbox((root,_run)=>{
 fs.writeFileSync(join(root,'tools/semantic.mjs'),semanticToolSource({binary:'unused-for-syntax-check',symbol:'planner.task'}));
 const result=spawnSync(process.execPath,['--check','tools/semantic.mjs'],{cwd:root,encoding:'utf8'});assert.equal(result.status,0,result.stderr);
}));

test('experimental compact pointers retain every evidence state and the full capsule body',()=>{
 const capsule='# context capsule\n- complete: false\nwarning: missing behavior\n## symbol\nTask fields: id, title\n## gaps\nno-policies\n';
 const mappings={bindings:[{path:'src/task.ts',line:1,semantic:'planner.Task',source:'user-confirmed',state:'current'},{path:'src/task.ts',line:1,semantic:'planner.NewTask',source:'inferred',state:'stale'},{path:'src/task.ts',semantic:'planner.Other',state:'unknown'}]};
 const result=renderSemanticContext(capsule,mappings,'planner.task','No automatic join.',true);
 for(const value of ['complete: false','warning: missing behavior','Task fields: id, title','no-policies','user-confirmed/current:1','inferred/stale:1','unknown/unknown:1','lines 1,unknown','No automatic join.','Pointer names omitted'])assert.ok(result.includes(value),value);
 assert.equal(result.split('\n').filter(l=>l.startsWith('src/task.ts ->')).length,1);
 assert.ok(!result.includes('planner.NewTask'));
});
