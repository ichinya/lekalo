#!/usr/bin/env node
// Explicit successor; published 0.6.4 schema bytes are never modified.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../',import.meta.url));
const obj=properties=>({type:'object',additionalProperties:false,required:Object.keys(properties),properties});
const unknown={enum:['unknown','withheld','unsupported']};
const unavailable=obj({state:unknown});
const stateful=s=>({oneOf:[obj({state:{const:'known'},value:s}),unavailable]});
const digest={type:'string',pattern:'^sha256:[0-9a-f]{64}$'};
const id={type:'string',minLength:1,maxLength:96,pattern:'^[a-z][a-z0-9-]*$'};
const provenance={oneOf:[obj({kind:{const:'preregistered'},evidenceDigest:digest}),obj({kind:{const:'post-run-import'},sourceCampaignDigest:digest,archiveManifestDigest:digest,translator:{$ref:'#/$defs/component'}})]};
assert.ok(process.argv.length===2||process.argv[2]==='--check');
for(const family of ['baseline','task','campaign','arm','result']){
 const s=JSON.parse(fs.readFileSync(root+'contracts/framework-lift-'+family+'.schema.v0.6.4.json','utf8').replaceAll('0.6.4','1.1.0'));
 s.description='Closed local-only recorded evaluation successor. Unknown policy and post-run import provenance do not authenticate execution.';
 if(s.$defs.pilot)s.$defs.pilot.enum.push('public-synthetic-observed');
 if(family==='baseline')s.properties.revision={oneOf:[s.properties.revision,obj({state:{const:'unknown'}})]};
 if(s.$defs.file)s.$defs.file.properties.path.pattern='^[a-zA-Z0-9.][a-zA-Z0-9._/-]*$';
 if(s.$defs.limits)for(const [k,v]of Object.entries(s.$defs.limits.properties))s.$defs.limits.properties[k]={oneOf:[v,obj({state:{enum:['unbounded','unknown']}})]};
 if(s.$defs.profile){
  const p=s.$defs.profile;
  for(const k of ['temperatureMicros','topPMicros','maxOutputTokens'])p.properties.sampling.properties[k]={oneOf:[p.properties.sampling.properties[k],unavailable]};
  p.properties.pricing={oneOf:[p.properties.pricing,unavailable]};
  p.properties.serverModelRevision=stateful(id);p.required.push('serverModelRevision');
  p.properties.telemetry={enum:['disabled','unknown']};
  p.properties.measurementRecipe={enum:['framework-lift-metrics-1','framework-lift-agent-window-1']};
 }
 if(s.$defs.metrics){
  for(const k of ['sourceFilesReadLowerBound','sourceMarkerBytes']){s.$defs.metrics.properties[k]=structuredClone(s.$defs.metrics.properties.filesRead);s.$defs.metrics.required.push(k);}
  if(s.$defs.source)s.$defs.source.properties.metric.enum.push('sourceFilesReadLowerBound','sourceMarkerBytes');
  if(s.properties.measurementSources)s.properties.measurementSources.maxItems=27;
 }
 if(family==='campaign'){s.properties.provenance=provenance;s.required.push('provenance');s.properties.randomizationSeed={oneOf:[s.properties.randomizationSeed,unavailable]};}
 if(family==='result'){
  // The result carries policy needed to interpret costs, timing and import status.
  const campaign=JSON.parse(fs.readFileSync(root+'contracts/framework-lift-campaign.schema.v1.1.0.json','utf8'));
  for(const k of ['component','profile','limits'])s.$defs[k]=campaign.$defs[k];
  s.properties.profile={$ref:'#/$defs/profile'};s.required.push('profile');
  s.properties.campaignProvenance=provenance;
  s.properties.measurementRecipe={enum:['framework-lift-metrics-1','framework-lift-agent-window-1']};
  s.properties.pricing=campaign.$defs.profile.properties.pricing;
  s.properties.profileCompleteness={enum:['complete','partial']};
  s.required.push('campaignProvenance','measurementRecipe','pricing','profileCompleteness');
 }
 const path=root+'contracts/framework-lift-'+family+'.schema.v1.1.0.json',bytes=JSON.stringify(s,null,2)+'\n';
 if(process.argv[2]==='--check')assert.equal(fs.readFileSync(path,'utf8'),bytes,'generated schema drift '+family);else fs.writeFileSync(path,bytes);
}
console.log(JSON.stringify({ok:true,families:5,version:'1.1.0',check:process.argv[2]==='--check'}));
