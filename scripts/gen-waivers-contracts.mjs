#!/usr/bin/env node
// Only issue #88 successors are generated; frozen predecessors are never written.
import { readFileSync, writeFileSync } from 'node:fs';
const version='0.6.5', wire=readFileSync('crates/lekalo-core/src/waivers/wire.rs','utf8');
const camel=s=>s.replace(/_([a-z])/g,(_,c)=>c.toUpperCase());
const object=properties=>({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const token={type:'string',minLength:1,maxLength:256,pattern:'^[A-Za-z0-9][A-Za-z0-9_.:/#@\\\\-]*$'};
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$',maxLength:71};
const timestamp={type:'string',pattern:'^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$',minLength:20,maxLength:20};
const path={type:'string',minLength:1,maxLength:512,pattern:'^(?!.*(?:^|/)\\.{1,2}(?:/|$))[a-z0-9._-]{1,64}(?:/[a-z0-9._-]{1,64})*$'};
const defs={RegistryRef:object({version:token,digest}),SelectorKind:{enum:['rule','capability']},ScopeKind:{enum:['project','module','symbol','path','target','profile']},Risk:{enum:['correctness','compatibility','security','data-loss','capability-gap']},Lifecycle:{enum:['active','revoked','superseded']}};
const state=inner=>({oneOf:[object({state:{const:'known'},value:inner}),...['unknown','withheld','unsupported'].map(s=>object({state:{const:s}}))]});
function type(t,key='') {
 if(t==='String')return structuredClone(/Digest$/.test(key)||['digest','conditionDigest'].includes(key)?digest:['asOf','createdAt','deadline'].includes(key)?timestamp:key==='reason'?{type:'string',minLength:1,maxLength:2000,pattern:'^[^\\u0000-\\u001f\\u007f]+$'}:token);
 if(t==='bool')return {type:'boolean'};
 if(t==='u64')return {type:'integer',minimum:0,maximum:key==='expiringWindowSeconds'?31536000:10000};
 if(t.startsWith('Vec<'))return {type:'array',maxItems:key==='evidenceRefs'?32:10000,items:type(t.slice(4,-1)),...(['evidenceRefs','reasonCodes','mismatchedPins'].includes(key)?{uniqueItems:true}:{})};
 if(t==='State<String>')return state(['expiresAt','reviewAfter'].includes(key)?timestamp:['model','ir','adapter','revision','capabilities','lockRef','waiverEntryDigest','baseStoreDigest','before','after'].includes(key)?digest:key==='path'?path:token);
 return {$ref:`#/$defs/${t}`};
}
for(const m of wire.matchAll(/pub struct (\w+)\s*\{([^}]+)\}/g)){
 const properties={};for(const f of m[2].matchAll(/pub\s+(\w+):\s*([^,\n}]+)/g)){const key=camel(f[1]);properties[key]=type(f[2].trim(),key);}
 defs[m[1]]=object(properties);
}
defs.SourceRef.properties.kind={enum:['issue','decision']};
const segment='[a-z][a-z0-9_]{0,62}',semantic=segments=>({type:'string',maxLength:191,pattern:`^${segment}${segments===1?'':`\\.${segment}(?:\\.${segment})?`}$`});
defs.Scope={oneOf:['project','module','symbol','path','target','profile'].map(kind=>object({kind:{const:kind},id:kind==='path'?path:['project','module','symbol'].includes(kind)?semantic(kind==='symbol'?2:1):token}))};
defs.Store.properties.projectId=semantic(1);defs.Input.properties.projectId=semantic(1);defs.Audit.properties.projectId=semantic(1);
defs.Fact.properties.symbol=state(semantic(2));defs.Fact.properties.module=state(semantic(1));
defs.SourceRef.properties.digest=state(digest);
defs.Fact.properties.sourceSeverity=state({enum:['info','warning','error']});
defs.Fact.properties.sourceConfidence=state({enum:['unknown','low','medium','high','exact']});
defs.Fact.properties.sourceOutcome={enum:['warning','unsupported','unknown','invalid','security','data-loss']};
defs.AuditEntry.properties.status={enum:['active','expiring','expired','stale','unverifiable','non-waivable','orphan','unexamined','revoked','superseded']};
defs.AuditEntry.properties.reasonCodes.items={enum:['fingerprint-unverifiable','fingerprint-changed','expired','not-created','profile-non-waivable','revoked','superseded']};
defs.AuditEntry.properties.mismatchedPins.items={enum:['model','ir','adapter','revision','capabilities','profile','condition','fact']};
defs.Disposition.properties.originalGate={enum:['denied','advisory']};defs.Disposition.properties.effectiveGate={enum:['denied','advisory','accepted-risk']};
defs.Disposition.properties.severity=state({enum:['info','warning','error']});defs.Change.properties.kind={enum:['added','removed','changed']};
for(const [family,name] of [['ai-lint-waivers','Store'],['waiver-input','Input'],['waiver-audit','Audit']]){
 const shape=structuredClone(defs[name]);shape.properties.schemaVersion={const:`lekalo/${family}/v${version}`};shape.properties.identity={const:`dev.lekalo.${family}@${version}`};
 const schema={$schema:'https://json-schema.org/draft/2020-12/schema',$id:`https://dev.lekalo/${family}.schema.v${version}.json`,title:`Lekalo ${family}`, ...shape,$defs:defs};
 const path=`contracts/${family}.schema.v${version}.json`,bytes=JSON.stringify(schema,null,2)+'\n';
 if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==bytes)throw new Error(`schema drift: ${path}`);}else writeFileSync(path,bytes);
}
