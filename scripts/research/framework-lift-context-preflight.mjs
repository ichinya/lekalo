#!/usr/bin/env node
// Actual offline fixture/CLI controls. No model CLI, network or dependencies.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {prepare,helper} from './framework-lift-planner.mjs';
import {compactResearchInstructions} from './framework-lift-context.mjs';
import {assertFreshReportPath} from './framework-lift-research-report.mjs';
const hash=b=>createHash('sha256').update(b).digest('hex');
const repo=fileURLToPath(new URL('../../',import.meta.url));

export function contextPreflight(){
 const binary=process.env.LEKALO_BIN??join(repo,'target/debug/lekalo'+(process.platform==='win32'?'.exe':'')),root=prepare({configure:false,binary}),outputs={};
 for(const compact of [false,true]){
  const workspace=join(root,compact?'compact-context':'full-context');fs.cpSync(join(root,'adopted'),workspace,{recursive:true});fs.mkdirSync(join(workspace,'tools'),{recursive:true});fs.writeFileSync(join(workspace,'tools/semantic.mjs'),helper(binary,compact));
  const run=()=>spawnSync(process.execPath,['tools/semantic.mjs'],{cwd:workspace,encoding:'utf8',timeout:120000,maxBuffer:16e6});
  const r=run();assert.equal(r.status,0,r.stdout+r.stderr);
  for(const gap of ['no-policies','error-contracts-unrepresentable','no automatic join'])assert.ok(r.stdout.includes(gap));
  const repeated=run();assert.equal(repeated.status,0,repeated.stderr);assert.match(repeated.stdout,/@@semantic-reuse/);assert.doesNotMatch(repeated.stdout,/no-policies/);
  fs.writeFileSync(join(root,compact?'compact-context.txt':'full-context.txt'),r.stdout);
  outputs[compact?'compact':'full']={capsuleBytes:Buffer.byteLength(r.stdout),capsuleSha256:hash(r.stdout),repeatBytes:Buffer.byteLength(repeated.stdout),repeatSuppressesUnchangedCapsule:true};
 }
 const installed=fs.readFileSync(join(root,'adopted/.agents/lekalo.md')),entry=compactResearchInstructions('.agents/lekalo.md');
 outputs.full.instructionBytes=installed.length;outputs.full.instructionSha256=hash(installed);outputs.compact.instructionBytes=Buffer.byteLength(entry);outputs.compact.instructionSha256=hash(entry);
 for(const value of Object.values(outputs))value.instructionAndCapsuleBytes=value.instructionBytes+value.capsuleBytes;
 assert.ok(outputs.compact.instructionAndCapsuleBytes<outputs.full.instructionAndCapsuleBytes);
 for(const directory of ['full-context','compact-context'])assert.equal(fs.readFileSync(join(root,directory+'/.agents/lekalo.md'),'utf8'),installed.toString());
 const pin=p=>hash(fs.readFileSync(join(repo,p)));
 const report={format:'lekalo-167-compact-context-preflight/1',inference:false,tokenSavings:'NOTRUN; bytes are not tokens or a causal effect estimate',variant:'experimental native-only instruction and grouped native pointer identities; full capsule body retained',sourceInstructionUnchanged:true,sourcePins:{binary:hash(fs.readFileSync(binary)),adapter:pin('adapters/node-typescript/adapter.mjs'),renderer:pin('scripts/research/framework-lift-context.mjs'),builder:pin('scripts/research/framework-lift-planner.mjs'),sourceReader:pin('scripts/research/framework-lift-workspace.mjs'),preflight:pin('scripts/research/framework-lift-context-preflight.mjs')},preflight:{baselineHoldout:'fails as expected',referenceHoldout:20,sourcePreserved:true},outputs};
 fs.writeFileSync(join(root,'compact-preflight.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});return {root,report};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
 const [out]=process.argv.slice(2);assert.ok(out,'usage: NEW_OUTPUT_JSON');assertFreshReportPath(out,[join(repo,'.lekalo/experiments')].filter(p=>fs.existsSync(p)));const result=contextPreflight();fs.writeFileSync(out,JSON.stringify(result.report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result));
}
