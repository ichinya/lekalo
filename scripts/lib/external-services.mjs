// Development/CI selections only. The product reads project service selections.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
export const serviceSpecs = JSON.parse(readFileSync(new URL('../../external-tools.lock.json', import.meta.url))).services;
const checkouts = fileURLToPath(new URL('../../tests/external/', import.meta.url));
export const serviceBinaries = [...new Set(Object.values(serviceSpecs).flatMap(s => s.binary ? [s.binary] : []))].sort();
export function serviceEnvironment(target, platform=process.platform) {
  const entries = Object.entries(serviceSpecs).map(([id, spec]) => {
    // Resolve the interpreter when the consumer runs, after CI selects its Node toolchain.
    const command = spec.script
      ? ['node', resolve(checkouts, spec.package, spec.script)]
      : [resolve(target, 'external-services/debug', spec.binary + (platform === 'win32' ? '.exe' : ''))];
    return [id, {command}];
  });
  return JSON.stringify(Object.fromEntries(entries));
}
