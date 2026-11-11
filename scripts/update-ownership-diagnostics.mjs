#!/usr/bin/env node
import {readFileSync,writeFileSync} from 'node:fs';
const path = new URL('../contracts/diagnostic-registry.v0.8.0.json', import.meta.url);
if(process.argv.slice(2).join(' ') !== '--write') throw Error('usage: --write');
const registry=JSON.parse(readFileSync(path));
const rules=[
 ['input-invalid','invalid','The ownership transition input is invalid.'],
 ['plan-stale','denied','The prepared ownership transition no longer matches its inputs.'],
 ['capability-gap','unsupported','The selected ownership transition has unsupported semantics.'],
 ['binding-unconfirmed','denied','The selected observed binding requires explicit confirmation.'],
 ['scope-refused','denied','The selected ownership scope cannot be transferred independently.'],
 ['verification-failed','denied','The maintained and candidate implementations did not satisfy the declared oracle.'],
 ['execution-unavailable','invalid','The qualified ownership verification could not execute completely.'],
 ['writer-locked','denied','Another project writer prevents the ownership transition.'],
 ['recovery-required','denied','An interrupted ownership handoff requires recovery.'],
 ['rollback-conflict','denied','Later changes conflict with ownership restoration.'],
 ['ownership-conflict','denied','The operation conflicts with recorded implementation ownership.'],
 ['version-unsupported','unsupported-version','The ownership transition contract version is unsupported.'],
];
for(const [index,[name,status,message]] of rules.entries()) {
 const id=`ownership.${name}`;
 const entry={id,code:`LEK-OWN-${String(index+1).padStart(3,'0')}`,category:'infrastructure',default_severity:'error',
  allowed_statuses:name==='recovery-required'?['invalid','denied']:[status],message_id:id,default_message:message,
  location_requirement:'none',data_fields:[{name:'reason',type:'token'}],allowed_fix_ids:[],lifecycle:'active'};
 const previous=registry.entries.find(row=>row.id===id);
 if(previous && JSON.stringify(previous)!==JSON.stringify(entry)) throw Error(`existing diagnostic drift: ${id}`);
 if(!previous)registry.entries.push(entry);
}
registry.entries.sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
writeFileSync(path,JSON.stringify(registry,null,2)+'\n');
