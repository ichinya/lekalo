// Parse only the repository's explicit block-style jobs/steps, not arbitrary YAML.
// GitHub and actionlint validate YAML; this guard rejects unsupported job layouts.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
export const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
export const unquote=s=>s?.replace(/^(['"])([\s\S]*)\1$/,'$2');
// The only supported aliases are complete setup steps. Reject unknown aliases,
// duplicate anchors and overrides instead of silently omitting their gates.
export function expandSetupSteps(text){
 const lines=text.replaceAll('\r\n','\n').split('\n'),anchors=new Map(),out=[];
 for(let i=0;i<lines.length;i++){
  const anchor=lines[i].match(/^      - &(ci-[\w-]+)$/),alias=lines[i].match(/^      - \*(ci-[\w-]+)$/);
  if(anchor){
   assert(!anchors.has(anchor[1]),'Duplicate setup anchor');const body=[];
   while(i+1<lines.length&&(!lines[i+1].trim()||lines[i+1].search(/\S/)>6))body.push(lines[++i]);
   assert(/^        (name|uses|run|id):/.test(body[0]),'Unsupported setup anchor');
   const expanded=['      - '+body[0].slice(8),...body.slice(1)];anchors.set(anchor[1],expanded);out.push(...expanded);
  }else if(alias){
   assert(anchors.has(alias[1]),'Unknown setup alias');
   assert(!(lines[i+1]?.trim()&&lines[i+1].search(/\S/)>6),'Setup alias override');out.push(...anchors.get(alias[1]));
  }else{
   if(/^      - /.test(lines[i]))assert(/^      - (?:name|uses|run|id):/.test(lines[i]),'Unsupported step layout');
   out.push(lines[i]);
  }
 }
 return out.join('\n');
}
export function field(block,key,indent=4){
 const lines=block.replaceAll('\r\n','\n').split('\n');
 const re=new RegExp(`^${' '.repeat(indent)}${key}:\\s*(.*)$`);
 const found=lines.map((line,i)=>({match:line.match(re),i})).filter(x=>x.match);
 assert(found.length<=1,`Duplicate ${key}`);if(!found.length)return undefined;
 const {match,i}=found[0];if(!['|','|-','>','>-'].includes(match[1]))return unquote(match[1]);
 assert(match[1][0]==='|',`Unsupported folded ${key}`);
 const value=[];for(let j=i+1;j<lines.length;j++){if(lines[j].trim()&&lines[j].search(/\S/)<=indent)break;value.push(lines[j].slice(indent+2));}
 return value.join('\n').trimEnd();
}
export function steps(job){
 const text=job.replaceAll('\r\n','\n');const starts=[...text.matchAll(/^      - (?:name|uses|run|id):/gm)].map(m=>m.index);
 return starts.map((start,i)=>{const raw=text.slice(start,starts[i+1]??text.length).trimEnd();const normalized=raw.replace(/^      - /,'        ');return {raw,name:field(normalized,'name',8),uses:field(normalized,'uses',8),run:field(normalized,'run',8),condition:field(normalized,'if',8),timeout:field(normalized,'timeout-minutes',8),workingDirectory:field(normalized,'working-directory',8),env:environment(normalized,8)};});
}
export function environment(block,indent=4){
 const lines=block.split('\n'),start=lines.findIndex(l=>l===`${' '.repeat(indent)}env:`);if(start<0)return {};
 const env={};for(const line of lines.slice(start+1)){if(line.trim()&&line.search(/\S/)<=indent)break;const m=line.match(new RegExp(`^${' '.repeat(indent+2)}([A-Za-z_][\\w-]*):\\s*(.*)$`));if(m){assert(!(m[1] in env),'Duplicate env');env[m[1]]=unquote(m[2]);}}
 return env;
}
export function jobs(text){
 text=expandSetupSteps(text);const start=text.indexOf('\njobs:\n');assert(start>=0,'Missing jobs');
 const body=text.slice(start+7),matches=[...body.matchAll(/^  ([A-Za-z][\w-]*):\s*$/gm)];assert(matches.length,'No block-style jobs');
 const result={};for(let i=0;i<matches.length;i++){const id=matches[i][1];assert(!(id in result),'Duplicate job');const raw=body.slice(matches[i].index,matches[i+1]?.index??body.length);result[id]={id,raw,steps:steps(raw),needs:(field(raw,'needs')??'').replace(/[\[\]'" ]/g,'').split(',').filter(Boolean),uses:field(raw,'uses'),runner:field(raw,'runs-on'),env:environment(raw)};}
 return result;
}
export function platformGraph(){
 const caller=jobs(readFileSync(resolve(root,'.github/workflows/ci.yml'),'utf8'));
 const call=caller.platform;assert.equal(call?.uses,'./.github/workflows/ci-platform.yml','Missing local platform call');
 return jobs(readFileSync(resolve(root,'.github/workflows/ci-platform.yml'),'utf8'));
}
export function assertContractsGate(script){
 const caller=jobs(readFileSync(resolve(root,'.github/workflows/ci.yml'),'utf8'));
 const owners=Object.values(caller).flatMap(job=>job.steps.map((step,index)=>({job,step,index}))).filter(x=>x.step.run?.includes(`node scripts/${script}`));
 assert.equal(owners.length,1,`Expected one Contracts owner of ${script}`);
 const {job,step,index}=owners[0];assert.equal(job.id,'contracts');assert.deepEqual(job.needs,['contracts-rust']);
 const verify=job.steps.findIndex(s=>s.run==='node scripts/ci-contracts-cli.mjs verify target/debug');
 assert(verify>=0&&verify<index,'Contracts handoff must precede gates');
 assert(!step.condition&&!field(job.raw,'if')&&!field(job.raw,'continue-on-error'));
 assert('LEKALO_AJV_NODE_PATH' in step.env,'Missing Contracts Ajv');
}
export function assertRuntimeGate(script,{ajv=false,profile='debug',graph=platformGraph()}={}){
 assert(/^[\w.-]+\.mjs$/.test(script),'Invalid gate name');
 const owners=Object.values(graph).flatMap(job=>job.steps.map((step,index)=>({job,step,index}))).filter(x=>x.step.run?.includes(`node scripts/${script}`));
 assert.equal(owners.length,1,`Expected exactly one owner of ${script}`);
 const {job,step,index}=owners[0],producer=profile==='debug'?'build-debug':'build-release';
 assert(job.needs.includes(producer),`Missing ${producer} dependency for ${script}`);
 assert.deepEqual(job.needs,[producer],`Unexpected barrier for ${script}`);
 const receiver=job.steps.findIndex(s=>s.run?.includes('node scripts/ci-runtime-artifacts.mjs verify'));
 assert(receiver>=0&&receiver<index,`No verified runtime before ${script}`);
 assert.equal(job.env.RUNTIME_PROFILE,profile);
 assert.equal(job.env.LEKALO_CI_RUNTIME,'required');
 assert(!field(job.raw,'if')&&!field(job.raw,'continue-on-error'),'Conditional/optional runtime owner');
 assert.equal(job.env.RUNTIME_IMAGE,'${{ inputs.os }}');
 assert.equal(job.env.RUNTIME_PRODUCER_ATTEMPT,`\${{ needs.${producer}.outputs.producer-attempt }}`);
 assert.equal(job.env.RUNTIME_MANIFEST_SHA256,`\${{ needs.${producer}.outputs.manifest-sha256 }}`);
 const download=job.steps.findIndex(s=>s.uses==='actions/download-artifact@v8');
 assert(download>=0&&download<receiver,'Missing download before verification');
 assert.equal(field(job.steps[download].raw,'artifact-ids',10),`\${{ needs.${producer}.outputs.artifact-id }}`);
 assert(!job.steps[receiver].condition&&!job.steps[download].condition,'Conditional handoff');
 assert(!job.steps[receiver].raw.includes('continue-on-error'),'Optional verification');
 assert(!step.condition,`Mandatory gate ${script} became conditional`);
 if(ajv)assert('LEKALO_AJV_NODE_PATH' in step.env,`Missing Ajv env for ${script}`);
 return {job:job.id,step};
}
