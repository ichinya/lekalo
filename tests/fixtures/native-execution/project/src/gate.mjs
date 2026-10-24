// Synthetic existing project commands. Only the production CLI may qualify them.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';
import {spawn} from 'node:child_process';
const config = JSON.parse(fs.readFileSync('src/fixture.json', 'utf8'));
const mode = process.argv[2];
assert.equal(process.env.LEKALO_NATIVE_SECRET, undefined);
assert.equal(process.env.NODE_OPTIONS, undefined);
assert.equal(process.env.NPM_TOKEN, undefined);
fs.mkdirSync('out', {recursive:true});
fs.writeFileSync('out/started.json', JSON.stringify({runId:config.runId, pid:process.pid}));
if (mode === 'fail') { fs.writeSync(2,'synthetic assertion failed');process.exit(7); }
else if (mode === 'timeout' || mode === 'cancel') setInterval(()=>{},100);
else if (mode === 'flood') { while (true) process.stdout.write('x'.repeat(8192)); }
else if (mode === 'stderr') { while (true) process.stderr.write('x'.repeat(8192)); }
else if (mode === 'security') fs.writeFileSync('src/unexpected.txt','not allowed');
else if (mode === 'budget') fs.writeFileSync('out/too-large.txt','x'.repeat(1024));
else if (mode === 'filesystem') {
  for (const action of [()=>fs.readFileSync(config.sentinel), ()=>fs.writeFileSync(config.sentinel,'escaped')]) {
    let denied = false;
    try { action(); } catch (error) { denied = ['EACCES','EPERM'].includes(error.code); }
    assert.equal(denied,true,'host file access must be denied');
  }
} else if (mode === 'network') {
  await new Promise((resolve,reject)=>{
    const socket = net.connect({host:'127.0.0.1',port:config.port});
    socket.on('connect',()=>{socket.destroy();reject(Error('network access escaped'));});
    socket.on('error',error=>['EACCES','EPERM'].includes(error.code) ? resolve() : reject(Error('network denial was not established')));
    socket.setTimeout(1500,()=>{socket.destroy();reject(Error('network denial was not established'));});
  });
} else if (mode === 'tree') {
  // Node 24/libuv 1.52 cannot create its own named stdio pipes in LPAC.
  // Inherited handles still exercise actual detached descendant containment.
  const childCode=`const fs=require('node:fs');fs.writeFileSync(${JSON.stringify(process.cwd()+'/out/child.json')},JSON.stringify({runId:${JSON.stringify(config.runId)},pid:process.pid}));setInterval(()=>{},100);`;
  const child = spawn(process.execPath,['-e',childCode],{detached:true,stdio:'inherit',cwd:process.cwd()});
  child.once('error',error=>fs.writeFileSync('out/child-error.json',JSON.stringify({runId:config.runId,error:error.code})));
  child.once('exit',(code,signal)=>fs.writeFileSync('out/child-error.json',JSON.stringify({runId:config.runId,error:{code,signal}})));
  child.unref();
  const deadline = Date.now()+3000;
  while (!fs.existsSync('out/child.json')) {
    if (Date.now() > deadline) throw Error('child did not start');
    await new Promise(resolve=>setTimeout(resolve,20));
  }
  await new Promise(resolve=>setTimeout(resolve,500));
} else if (mode === 'memory') {
  // Each child stays below the per-process limit; their overlapping
  // allocations exceed the whole-job bound. A process-only cap fails this gate.
  const outcomes = await Promise.all([0,1,2].map(()=>new Promise(resolve=>{
    const code='const allocation=Buffer.alloc(180*1024*1024,1);setTimeout(()=>{if(allocation[allocation.length-1]!==1)process.exit(2);},2000);';
    const child=spawn(process.execPath,['-e',code],{stdio:'inherit',cwd:process.cwd()});
    child.once('error',()=>resolve('spawn-denied'));
    child.once('exit',code=>resolve(code===0?'allocated':'allocation-failed'));
  })));
  assert.ok(outcomes.includes('allocation-failed'),'whole-job committed memory must be bounded after a child started');
  assert.ok(outcomes.includes('allocated'),'a real child allocation succeeds below the bound');
} else if (mode !== 'pass') throw Error('unknown fixture mode');
if (!['timeout','cancel'].includes(mode)) {
  fs.writeFileSync('out/result.txt','actual gate');
  process.stdout.write('gate-passed');
}
