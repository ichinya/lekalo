#!/usr/bin/env node
// Exercise the production provenance gate against real private Git filters.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtureGit, fixtureGitEnv } from "./lib/fixture-git.mjs";

const source=fileURLToPath(new URL("../",import.meta.url));
const root=realpathSync.native(mkdtempSync(join(tmpdir(),"lekalo-fixture-git-test-")));
try {
  const repository=join(root,"repository");
  mkdirSync(join(repository,"scripts/lib"),{recursive:true});
  mkdirSync(join(repository,"tests/fixtures/binary"),{recursive:true});
  for(const path of ["scripts/test-fixture-provenance.mjs","scripts/lib/fixture-git.mjs"]) cpSync(join(source,path),join(repository,path));
  const attributes=join(repository,".gitattributes");
  const normal="tests/fixtures/binary/** -text !eol\n";
  writeFileSync(attributes,normal);
  writeFileSync(join(repository,"tests/fixtures/binary/large.sqlite"),Buffer.concat([Buffer.alloc(256*1024,0),Buffer.from("\r\nbinary\r\n")]));
  writeFileSync(join(repository,"tests/fixtures/fixture-provenance.json"),JSON.stringify({manifestId:"dev.lekalo.fixture-provenance",version:"test",families:[{family:"binary",origin:"synthetic"}]}));
  fixtureGit(repository,["init","--quiet"],"test-init");
  fixtureGit(repository,["add","--",".gitattributes","tests/fixtures"],"test-index");
  const sentinel=join(root,"foreign-index");
  writeFileSync(sentinel,"unrelated caller index bytes");
  const temporary=join(root,"tmp"); mkdirSync(temporary);
  const fixture=join(repository,"tests/fixtures/binary/large.sqlite");
  const fixtureBefore=readFileSync(fixture);
  const environment={...fixtureGitEnv(),GIT_DIR:join(root,"foreign-git"),GIT_WORK_TREE:root,GIT_INDEX_FILE:sentinel,TMPDIR:temporary,TEMP:temporary,TMP:temporary};
  function gate() {
    const result=spawnSync(process.execPath,["scripts/test-fixture-provenance.mjs"],{cwd:repository,env:environment,encoding:"utf8",timeout:90000,killSignal:"SIGKILL"});
    assert.deepEqual(readdirSync(temporary),[],"provenance canary leaked after child exit");
    assert.deepEqual(readFileSync(fixture),fixtureBefore,"provenance modified the source fixture");
    return result;
  }
  // Match the synchronous parent/child shape of documentation replay.
  // The original macOS stdin deadlock was intermittent, so repeat real input
  // larger than a pipe buffer rather than relying on a single green run.
  for(let attempt=0;attempt<5;attempt++) {
    const pass=gate();
    assert.equal(pass.error,undefined);
    assert.equal(pass.status,0,pass.stderr);
    assert.equal(JSON.parse(pass.stdout).binaryFixtures,1);
  }
  assert.equal(readFileSync(sentinel,"utf8"),"unrelated caller index bytes");
  const index=join(repository,".git/index"), savedIndex=readFileSync(index);
  try {
    writeFileSync(index,"invalid-index");
    const broken=gate();
    assert.notEqual(broken.status,0,"corrupt Git index silently accepted");
    assert.match(broken.stderr,/fixture Git failed: inventory/);
  } finally {
    writeFileSync(index,savedIndex);
  }
  writeFileSync(attributes,"tests/fixtures/binary/** text eol=lf\n");
  const text=gate();
  assert.notEqual(text.status,0);
  assert.match(text.stderr,/binary fixture must be -text/);
  // -text alone does not disable custom clean filters. Prove --path still
  // invokes the real configured filter, and the production gate catches it.
  writeFileSync(attributes,normal.trim()+" filter=custody\n");
  writeFileSync(join(repository,"scripts/corrupt.mjs"),'const chunks=[]; for await (const chunk of process.stdin) chunks.push(chunk); process.stdout.write(Buffer.concat([...chunks,Buffer.from("corrupt")]));\n');
  fixtureGit(repository,["config","filter.custody.clean","node scripts/corrupt.mjs"],"test-filter");
  fixtureGit(repository,["config","filter.custody.required","true"],"test-filter-required");
  const corrupted=gate();
  assert.notEqual(corrupted.status,0);
  assert.match(corrupted.stderr,/clean filtering rewrites binary fixture bytes/);
  writeFileSync(attributes,normal);
  const restored=gate();
  assert.equal(restored.status,0,restored.stderr);
  assert.equal(JSON.parse(restored.stdout).binaryFixtures,1);
  assert.equal(readFileSync(sentinel,"utf8"),"unrelated caller index bytes");
  renameSync(join(repository,".git"),join(repository,".git-held"));
  const missing=gate();
  assert.notEqual(missing.status,0,"missing private Git index silently accepted");
  assert.match(missing.stderr,/fixture Git failed: inventory/);
  console.log(JSON.stringify({ok:true,gate:"fixture-git",binaryBytes:256*1024,repetitions:5,controls:["normal","corrupt-index-refused","text-refused","corrupt-clean-filter-refused","restored","foreign-index-preserved","missing-index-refused","canaries-cleaned","fixture-unchanged"]}));
} finally {
  assert.equal(dirname(root),realpathSync.native(tmpdir()));
  assert.ok(basename(root).startsWith("lekalo-fixture-git-test-"));
  rmSync(root,{recursive:true,force:true});
}
