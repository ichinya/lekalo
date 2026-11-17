#!/usr/bin/env node
// Development/test tools live in their own repositories. No source snapshots
// are written into the host repository or compiled into its production binary.
import {execFileSync, spawnSync} from 'node:child_process';
import {appendFileSync, copyFileSync, chmodSync, existsSync, mkdirSync, readFileSync, realpathSync, symlinkSync} from 'node:fs';
import {homedir} from 'node:os';
import {dirname, join, resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {serviceSpecs, serviceEnvironment} from './lib/external-services.mjs';
export const hostRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function externalRoot(name) {
  const lock = JSON.parse(readFileSync(join(hostRoot, 'external-tools.lock.json'), 'utf8'));
  const pin = lock.packages[name];
  if (!pin || !/^[a-f0-9]{40}$/.test(pin.commit)) throw new Error(`No immutable tool pin for ${name}`);
  const overrides = JSON.parse(process.env.LEKALO_EXTERNAL_TOOLS ?? '{}');
  if (overrides[name]) return realpathSync(overrides[name]); // explicit development checkout
  const cache = process.env.LEKALO_EXTERNAL_TOOL_CACHE ?? join(homedir(), '.cache/lekalo/development');
  const root = join(cache, name, pin.commit);
  const git = args => execFileSync('git', ['-c', `core.hooksPath=${join(cache,'.disabled-hooks')}`, ...args], {encoding:'utf8', stdio:['ignore','pipe','pipe']}).trim();
  if (!existsSync(join(root, '.git'))) {
    mkdirSync(dirname(root), {recursive:true});
    git(['clone','--no-checkout','--',pin.repository,root]);
    git(['-C',root,'checkout','--detach',pin.commit]);
  }
  if (git(['-C',root,'rev-parse','HEAD']) !== pin.commit || git(['-C',root,'status','--porcelain','--untracked-files=no'])) throw new Error(`Tool cache is not the pinned clean checkout: ${name}`);
  return root;
}
export function runExternal(name, script, args = process.argv.slice(2)) {
  const root = externalRoot(name);
  if (!script.startsWith('scripts/') || script.includes('..')) throw new Error('Invalid tool entry');
  const binDir = process.env.CARGO_TARGET_DIR ? resolve(process.env.CARGO_TARGET_DIR, 'debug') : join(hostRoot,'target/debug');
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const env = {...process.env, LEKALO_TYPESCRIPT_ROOT:externalRoot('lekalo-typescript'), LEKALO_BIN:process.env.LEKALO_BIN ?? join(binDir,'lekalo'+suffix), LEKALO_SDK_ORACLE:process.env.LEKALO_SDK_ORACLE ?? join(binDir,'adapter-sdk-oracle'+suffix)};
  if (script === 'scripts/test-adapter-sdk-vectors.mjs' && !process.env.LEKALO_SDK_ORACLE) {
    const build = spawnSync('cargo',['build','--locked','-p','lekalo-cli','--bin','adapter-sdk-oracle'],{cwd:hostRoot,stdio:'inherit'});
    if (build.status !== 0) process.exit(build.status ?? 1);
  }
  const result = spawnSync(process.execPath,[join(root,script),...args],{cwd:root,env,stdio:'inherit'});
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
export function provision() {
  // Legacy test/CI path compatibility. These are ignored links to external
  // checkouts, never vendored or committed implementation directories.
  for (const name of Object.keys(JSON.parse(readFileSync(join(hostRoot,'external-tools.lock.json'))).packages)) {
    const destination = 'tests/external/'+name;
    const target = externalRoot(name), link = join(hostRoot,destination);
    if (existsSync(link)) {
      if (realpathSync(link) !== realpathSync(target)) throw new Error(`Refusing to replace existing ${destination}`);
      continue;
    }
    mkdirSync(dirname(link),{recursive:true});
    symlinkSync(target,link,process.platform === 'win32' ? 'junction' : 'dir');
  }
  if (process.env.GITHUB_ENV) {
    appendFileSync(process.env.GITHUB_ENV,`LEKALO_SERVICES=${serviceEnvironment(join(hostRoot,'target'))}\n`);
    const provider=JSON.stringify('path:'+externalRoot('lekalo-postgres'));
    appendFileSync(process.env.GITHUB_ENV,`LEKALO_STORAGE_PROVIDER=${provider}\nLEKALO_TYPESCRIPT_ROOT=${externalRoot('lekalo-typescript')}\n`);
  }

}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === '--provision') provision();
  else if (process.argv[2] === '--build-services') buildServices();
  else if (process.argv.length === 3) console.log(externalRoot(process.argv[2]));
  else runExternal(process.argv[2],process.argv[3],process.argv.slice(4));
}

export function buildServices() {
  for (const spec of Object.values(serviceSpecs)) {
    if (!/^[a-z][a-z0-9-]*$/.test(spec.binary) || spec.manifest.split('/').some(p=>!p||p==='..'||p.includes('\\'))) throw new Error('Invalid service build specification');
    const root=externalRoot(spec.package);
    const result=spawnSync('cargo',['build','--locked','--manifest-path',join(root,spec.manifest),'--bin',spec.binary],{cwd:root,env:{...process.env,CARGO_TARGET_DIR:join(hostRoot,'target/external-service-build'),CARGO_PROFILE_DEV_DEBUG:'0',CARGO_INCREMENTAL:'0'},stdio:'inherit'});
    if (result.status!==0) throw new Error(`Service build failed: ${spec.package}`);
    // Cargo hardlinks its build artifacts; install a separate regular file for
    // the host's no-hardlink executable admission before pinning it.
    const suffix=process.platform==='win32'?'.exe':'';
    const destination=join(hostRoot,'target/external-services/debug',spec.binary+suffix);
    mkdirSync(dirname(destination),{recursive:true});
    copyFileSync(join(hostRoot,'target/external-service-build/debug',spec.binary+suffix),destination);
    chmodSync(destination,0o755);
  }
}
