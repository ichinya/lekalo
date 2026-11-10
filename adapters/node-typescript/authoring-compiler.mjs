/** Explicit authoring dependency injection. Product execution is core-supervised.
 * Authoring tools use the pinned native API outside confinement; this is not sandbox evidence. */
import {API} from './node_modules/typescript/dist/api/sync/api.js';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {createCompiler} from './src/compiler.mjs';
const pins = JSON.parse(readFileSync(new URL('./compiler-resources.json', import.meta.url)));
const resource = pins.resources.find(value => value.platform === `${process.platform}-${process.arch}`);
if (!resource) throw new Error('unsupported native compiler test platform');
const binary = fileURLToPath(new URL(`./node_modules/${resource.package}/${resource.file}`, import.meta.url));
const bytes = readFileSync(binary);
if (bytes.length !== resource.bytes || createHash('sha256').update(bytes).digest('hex') !== resource.sha256) throw new Error('native compiler test integrity');
const configured = new WeakSet();
export function createAuthoringCompiler() { return createCompiler(options => new API({...options, tsserverPath: binary})); }
export function configureAuthoringCompiler(adapter) {
  const ts = adapter.compilerHostApi;
  if (ts && !configured.has(ts)) {
    ts.__setApiFactoryForTests(options => new API({...options, tsserverPath: binary}));
    configured.add(ts);
    adapter.__lekaloKernel.__setCompilerRuntime({schema_version: 'lekalo/compiler-runtime/v1.3.0', kind: 'typescript-native',
      version: pins.typescript, platform: resource.platform, digest: `sha256:${resource.sha256}`, libraries_digest: `sha256:${pins.standardLibraries.sha256}`});
  }
  return adapter;
}
