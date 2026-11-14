import assert from 'node:assert/strict';
import test from 'node:test';
import {spawnSync} from 'node:child_process';
import {copyFileSync,mkdirSync,mkdtempSync,rmSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

function fixture(t) {
  const root=mkdtempSync(join(tmpdir(),'lekalo-privacy-handoff-'));
  t.after(()=>rmSync(root,{recursive:true,force:true}));
  mkdirSync(join(root,'scripts'));
  const module=join(root,'scripts','privacy-runtime-helpers.mjs');
  copyFileSync(new URL('./privacy-runtime-helpers.mjs',import.meta.url),module);
  return {
    binary(profile){const file=join(root,'target',profile,process.platform==='win32'?'lekalo.exe':'lekalo');mkdirSync(join(root,'target',profile),{recursive:true});writeFileSync(file,'synthetic executable marker');return file;},
    run({required=true,findOnly=false}={}) {
      // A CI consumer must work even when Cargo is unavailable. Use the real
      // module with a private checkout; no product binary is executed here.
      const env={...process.env,PATH:''};delete env.LEKALO_CI_RUNTIME;
      if(required)env.LEKALO_CI_RUNTIME='required';
      return spawnSync(process.execPath,['--input-type=module','-e',`import {buildLekaloBinary,findLekaloBinary} from ${JSON.stringify(pathToFileURL(module).href)};console.log(JSON.stringify(${findOnly?'findLekaloBinary':'buildLekaloBinary'}()));`],{env,encoding:'utf8',windowsHide:true});
    },
  };
}

test('required privacy runtime reuses debug handoff without Cargo',t=>{
  const f=fixture(t),binary=f.binary('debug');f.binary('release');
  const r=f.run();assert.equal(r.status,0,r.stderr);assert.equal(JSON.parse(r.stdout),binary);
});
test('required privacy runtime refuses missing debug even when a release exists',t=>{
  for(const release of [false,true]) {
    const f=fixture(t);if(release)f.binary('release');
    const r=f.run();assert.notEqual(r.status,0);assert.match(r.stderr,/Verified CI runtime is missing; refusing Cargo fallback/);
  }
});
test('standalone privacy lookup retains its release fallback',t=>{
  const f=fixture(t),binary=f.binary('release'),r=f.run({required:false,findOnly:true});
  assert.equal(r.status,0,r.stderr);assert.equal(JSON.parse(r.stdout),binary);
});
