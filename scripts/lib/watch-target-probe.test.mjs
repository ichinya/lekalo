import assert from 'node:assert/strict';
import test from 'node:test';
import {replaceFixture} from './watch-target-probe.mjs';

test('Windows sharing refusals retry the same replacement without skipping it',()=>{
 const calls=[],waits=[];let elapsed=0;
 const retries=replaceFixture('staged','source',120000,{platform:'win32',now:()=>elapsed,wait:ms=>{waits.push(ms);elapsed+=ms;},rename(...args){calls.push(args);if(calls.length<=3)throw Object.assign(new Error('sharing'),{code:['EPERM','EACCES','EBUSY'][calls.length-1]});}});
 assert.equal(retries,3);assert.equal(calls.length,4);assert(calls.every(args=>args[0]==='staged'&&args[1]==='source'));assert.deepEqual(waits,[1,1,1]);
});
test('other Windows errors and sharing errors on Unix fail immediately',()=>{
 for(const [platform,code]of [['win32','ENOENT'],['win32','ENOSPC'],['linux','EPERM'],['darwin','EACCES']]){
  const failure=Object.assign(new Error(code),{code});let calls=0;
  assert.throws(()=>replaceFixture('staged','source',120000,{platform,now:()=>0,wait(){assert.fail('must not wait');},rename(){calls++;throw failure;}}),error=>error===failure);
  assert.equal(calls,1);
 }
});
test('permanent sharing refusal retains its error and the five-second bound',()=>{
 let elapsed=0;const failure=Object.assign(new Error('sharing'),{code:'EPERM'});
 assert.throws(()=>replaceFixture('staged','source',120000,{platform:'win32',now:()=>elapsed,wait:ms=>{elapsed+=ms;},rename(){throw failure;}}),error=>error===failure);
 assert.equal(elapsed,5000);
});
test('sharing retries cannot extend the enclosing probe deadline',()=>{
 let elapsed=119999;const failure=Object.assign(new Error('sharing'),{code:'EACCES'});
 assert.throws(()=>replaceFixture('staged','source',120000,{platform:'win32',now:()=>elapsed,wait:ms=>{elapsed+=ms;},rename(){throw failure;}}),error=>error===failure);
 assert.equal(elapsed,120000);
});
