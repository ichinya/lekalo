import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import test from 'node:test';

const adapter=fileURLToPath(new URL('../../templates/typescript/reference/adapter.mjs',import.meta.url));
const fixtures=fileURLToPath(new URL('../../../tests/fixtures/adapter-sdk/valid/',import.meta.url));
// Execute the shipped bundle with a non-seekable stdin driver on every OS.
// Windows pipes can ignore positional reads, masking the Unix ESPIPE bug.
const child=String.raw`
 import assert from 'node:assert/strict';
 import fs from 'node:fs';
 import {syncBuiltinESMExports} from 'node:module';
 import {pathToFileURL} from 'node:url';
 const [mode,adapter,fixtures]=process.argv.slice(1);
 const name=mode==='straddle'?'plan-native-request.json':'describe-request.json';
 const request=JSON.parse(fs.readFileSync(fixtures+'/'+name,'utf8'));
 let bytes;
 if(mode==='straddle') {
  request.native_request.changes.symbols=['é'];
  const compact=JSON.stringify(request);
  const offset=Buffer.byteLength(compact.slice(0,compact.indexOf('é')));
  bytes=Buffer.from(' '.repeat(65535-offset)+compact);
  assert.equal(bytes[65535],0xc3);
  assert.equal(bytes[65536],0xa9);
 }else bytes=Buffer.from(JSON.stringify(request));
 let offset=0;
 const original=fs.readSync;
 fs.readSync=(fd,buffer,start,length,position)=>{
  if(fd!==0)return original(fd,buffer,start,length,position);
  if(position!==null)throw Object.assign(new Error('non-seekable stdin'),{code:'ESPIPE'});
  if(mode==='error'&&offset===bytes.length)throw Object.assign(new Error('read failed'),{code:'EIO'});
  const count=Math.min(length,65536,bytes.length-offset);
  bytes.copy(buffer,start,offset,offset+count);
  offset+=count;
  return count;
 };
 syncBuiltinESMExports();
 if(mode==='straddle')process.argv.push('--lekalo-fault','native-plan-control');
 await import(pathToFileURL(adapter).href);
`;

function invoke(mode) {
 const result=spawnSync(process.execPath,['--input-type=module','-e',child,mode,adapter,fixtures],{
  encoding:'utf8',timeout:10000,maxBuffer:1024*1024,
 });
 assert.ifError(result.error);
 assert.equal(result.signal,null,result.stderr);
 return result;
}

test('non-seekable stdin preserves a multibyte character across 64 KiB reads',()=>{
 const result=invoke('straddle');
 assert.equal(result.status,0,result.stderr);
 const response=JSON.parse(result.stdout);
 assert.equal(response.status,'ok');
 assert.equal(response.operation,'plan-native');
});

test('a read error after a complete JSON prefix cannot be accepted as EOF',()=>{
 const result=invoke('error');
 assert.notEqual(result.status,0);
 assert.equal(result.stdout,'');
 assert.match(result.stderr,/transport: request input unreadable/);
});
