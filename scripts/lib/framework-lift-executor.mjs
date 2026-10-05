// Explicit callback seam for a neutral external executor. No shell/provider defaults.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
const sort=value=>Array.isArray(value)?value.map(sort):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(k=>[k,sort(value[k])])):value;
export const canonical=value=>JSON.stringify(sort(value))+'\n';
export const digest=value=>'sha256:'+createHash('sha256').update(canonical(value)).digest('hex');
function run(binary,args){const p=spawnSync(binary,['--json','evaluation',...args],{encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});if(p.status!==0)throw new Error('evaluation-admission-refused');return JSON.parse(p.stdout);}
/** The host supplies the same executor callback for A/B. It must enforce tool views.
 * Private campaigns require a qualified local-only executor; no automatic cloud route.
 * Return value is the host's closed arm document, admitted through record-arm later.
 * This seam cannot attest an arbitrary callback's actual behavior or egress containment.
 */
export async function executeArm({binary,baseline,task,campaign,workspace,slot,executor}){
 if(typeof executor!=='function')throw new Error('explicit-executor-required');
 const inputs=['--baseline',baseline,'--task',task,'--campaign',campaign];
 const approved=run(binary,['preflight',...inputs,'--workspace',workspace]);
 if(!approved.slots.some(s=>canonical(s)===canonical(slot)))throw new Error('slot-not-scheduled');
 const assisted=slot.arm==='B';
 const taskDocument=JSON.parse(readFileSync(task,'utf8'));
 const baselineDocument=JSON.parse(readFileSync(baseline,'utf8'));
 if(digest(taskDocument)!==approved.taskRef.digest||digest(baselineDocument)!==approved.baselineRef.digest)throw new Error('protocol-changed-after-preflight');
 const context=Object.freeze({campaignRef:{id:approved.campaignId,digest:digest(approved)},taskRef:approved.taskRef,baselineRef:approved.baselineRef,profileDigest:digest(approved.profile),applicationDigest:baselineDocument.approval.contentDigest,slot:structuredClone(slot),profile:structuredClone(approved.profile),requestDigest:taskDocument.requestDigest,policy:{lekalo:assisted,semanticExposure:assisted,network:approved.profile.network}});
 const result=await executor(context);
 const after=run(binary,['preflight',...inputs,'--workspace',workspace]);
 if(canonical(after)!==canonical(approved))throw new Error('protocol-changed-during-execution');
 return result;
}
export function admitRecordedArm({binary,baseline,task,campaign,input}){
 return run(binary,['record-arm','--baseline',baseline,'--task',task,'--campaign',campaign,'--input',input]);
}
