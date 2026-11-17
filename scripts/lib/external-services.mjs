// Development/CI selections only. The product reads project service selections.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
export const serviceSpecs = JSON.parse(readFileSync(new URL('../../external-tools.lock.json', import.meta.url))).services;
export const serviceBinaries = [...new Set(Object.values(serviceSpecs).map(s=>s.binary))].sort();
export function serviceEnvironment(target, platform=process.platform) {
  return JSON.stringify(Object.fromEntries(Object.entries(serviceSpecs).map(([id,s])=>[id,{command:[resolve(target,'external-services/debug',s.binary+(platform==='win32'?'.exe':''))]}])));
}
