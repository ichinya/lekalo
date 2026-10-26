import assert from 'node:assert/strict';
import {test} from 'node:test';
import fs from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash} from 'node:crypto';
import {publicResult} from './framework-lift-public-result.mjs';
const sha=b=>createHash('sha256').update(b).digest('hex');
function scratch(fn){const root=fs.mkdtempSync(join(tmpdir(),'lekalo-public-result-test-'));try{fn(root);}finally{fs.rmSync(root,{recursive:true,force:true});}}
test('published rows preserve usage subsets, all attempts and the incomplete native pass',()=>{
 const bytes=fs.readFileSync(new URL('./results/2026-10-26-read-once.json',import.meta.url)),r=JSON.parse(bytes);
 assert.equal(r.rows.length,8);assert.equal(r.rows.filter(r=>r.complete).length,7);assert.equal(r.rows.filter(r=>r.nativePass).length,8);
 assert.equal(r.rows.find(r=>!r.complete).id,'devin-swe2-high-b1');
 for(const row of r.rows){assert.equal(row.total,row.input+row.output);assert.ok(row.cachedInput<=row.input);assert.equal(row.uncachedInput,row.input-row.cachedInput);assert.equal(row.holdoutPassed,20);assert.equal(row.holdoutTotal,20);}
 for(const k of ['input','output','total','cachedInput','uncachedInput'])assert.equal(r.rows.reduce((n,row)=>n+row[k],0),r.totals[k]);
 assert.equal(r.totals.total,1648102);assert.equal(r.totals.cost.state,'unknown');assert.equal(r.admission.recordArm,'NOTRUN');assert.equal(r.admission.compare,'NOTRUN');
 assert.doesNotMatch(bytes.toString(),/C:\\|C:\/Users|@example|api[_-]?key|access[_-]?token|account[_-]?id/i);
});
test('the externally supplied archive manifest pin is mandatory',()=>scratch(root=>{
 fs.writeFileSync(join(root,'SHA256-MANIFEST.json'),'{}');assert.throws(()=>publicResult(root,'0'.repeat(64)),/manifest drift/);
}));
test('a matching manifest cannot escape its archive',()=>scratch(root=>{
 const bytes=JSON.stringify({algorithm:'sha256',files:[{path:'../outside',sha256:'0'.repeat(64),bytes:0}]});fs.writeFileSync(join(root,'SHA256-MANIFEST.json'),bytes);assert.throws(()=>publicResult(root,sha(bytes)),/unsafe archive path/);
}));
test('evidence content drift refuses before any public export',()=>scratch(root=>{
 fs.writeFileSync(join(root,'receipt.json'),'changed');const bytes=JSON.stringify({algorithm:'sha256',files:[{path:'receipt.json',sha256:sha('initial'),bytes:7}]});fs.writeFileSync(join(root,'SHA256-MANIFEST.json'),bytes);assert.throws(()=>publicResult(root,sha(bytes)),/archive content drift/);
}));
