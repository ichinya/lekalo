#!/usr/bin/env node
// Issue #157: actual CLI, managed-byte custody and explicit writes.
import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import {spawnSync} from 'node:child_process';
import {resolve,join,dirname,relative,basename} from 'node:path';
import {fileURLToPath} from 'node:url';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const binary=process.env.LEKALO_BIN??join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const parent=join(repo,'target');fs.mkdirSync(parent,{recursive:true});
const work=fs.mkdtempSync(join(parent,'prompt-acceptance-'));
const cases=[];
function cli(root,args=[],exit=0){const r=spawnSync(binary,['--json','install','prompt','--project',root,...args],{cwd:repo,encoding:'utf8',timeout:30000});assert.equal(r.status,exit,r.stdout+'\n'+r.stderr);return JSON.parse(r.stdout.trim()||r.stderr.trim());}
function fixture(name){const root=join(work,name);fs.mkdirSync(root);return root;}
function inventory(root,prefix=''){return fs.readdirSync(join(root,prefix),{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name)).flatMap(e=>{const p=prefix?prefix+'/'+e.name:e.name;const m=fs.lstatSync(join(root,p));return m.isSymbolicLink()?[[p,'link']]:m.isDirectory()?inventory(root,p):[[p,fs.readFileSync(join(root,p)).toString('hex')]];});}
function refused(root,args,reason){const before=inventory(root);const result=cli(root,args,1);assert.ok(result.diagnostics.some(d=>d.data.reason===reason),JSON.stringify(result));assert.deepEqual(inventory(root),before);}
try {
  const empty=fixture('empty'),preview=cli(empty,['--dry-run']);
  assert.equal(preview.state,'planned');assert.equal(preview.changes.length,1);assert.match(preview.changes[0].diff,/Work with Lekalo/);assert.deepEqual(inventory(empty),[]);
  refused(empty,['--check'],'prompt-stale');
  const installed=cli(empty);assert.equal(installed.state,'updated');assert.ok(!fs.existsSync(join(empty,'AGENTS.md')));
  const before=inventory(empty),mtime=fs.statSync(join(empty,'.agents/lekalo.md')).mtimeMs;
  assert.equal(cli(empty).state,'current');assert.deepEqual(inventory(empty),before);assert.equal(fs.statSync(join(empty,'.agents/lekalo.md')).mtimeMs,mtime);assert.equal(cli(empty,['--check']).state,'current');cases.push('empty-dry-run-idempotent-check');
  const root=fixture('existing-agents'),prefix='\ufeff# Project instructions\r\nPreserve this line.\r\n';
  fs.writeFileSync(join(root,'AGENTS.md'),prefix);cli(root);
  let agents=fs.readFileSync(join(root,'AGENTS.md'),'utf8');assert.ok(agents.startsWith(prefix));assert.ok(agents.includes('<!-- lekalo:begin -->\r\n'));assert.ok(!agents.replaceAll('\r\n','').includes('\n'));
  const suffix='\r\n## Other instructions\r\nKeep the existing terminal workflow.\r\n';
  fs.appendFileSync(join(root,'AGENTS.md'),suffix);const unchanged=inventory(root);assert.equal(cli(root).state,'current');assert.deepEqual(inventory(root),unchanged);cases.push('outside-bytes-and-crlf-preserved');
  const prompt=join(root,'.agents/lekalo.md');fs.appendFileSync(prompt,'Manual edit\n');refused(root,[],'prompt-content-conflict');cli(root,['--force']);
  agents=fs.readFileSync(join(root,'AGENTS.md'),'utf8').replace('Read [.agents/lekalo.md]','Manually changed [.agents/lekalo.md]');fs.writeFileSync(join(root,'AGENTS.md'),agents);refused(root,[],'prompt-section-conflict');cli(root,['--force']);
  agents=fs.readFileSync(join(root,'AGENTS.md'),'utf8');assert.ok(agents.startsWith(prefix));assert.ok(agents.endsWith(suffix));cases.push('manual-conflicts-force-preserves-outside');
  for(const path of [prompt,join(root,'AGENTS.md')])fs.writeFileSync(path,fs.readFileSync(path,'utf8').replace(/product=[0-9.]+/,'product=0.0.1').replace(/registry=sha256:[a-f0-9]{64}/,'registry=sha256:'+'0'.repeat(64)));
  refused(root,['--check'],'prompt-stale');const upgrade=cli(root,['--dry-run']);assert.equal(upgrade.changes.length,2);cli(root);assert.equal(cli(root,['--check']).state,'current');cases.push('version-and-registry-staleness');
  fs.appendFileSync(join(root,'AGENTS.md'),'<!-- lekalo:begin -->\n');refused(root,['--force'],'prompt-markers-ambiguous');cases.push('ambiguous-markers-never-overwritten');
  const create=fixture('explicit-create');assert.equal(cli(create,['--create-agents-md','--dry-run']).changes.length,2);assert.deepEqual(inventory(create),[]);cli(create,['--create-agents-md']);assert.ok(fs.existsSync(join(create,'AGENTS.md')));cases.push('explicit-agents-create');
  const link=fixture('linked-home'),outside=fixture('outside');fs.writeFileSync(join(outside,'sentinel'),'unchanged');fs.symlinkSync(outside,join(link,'.agents'),process.platform==='win32'?'junction':'dir');
  // Inventory does not follow the deliberately denied junction.
  refused(link,['--force'],'prompt-path-unconfined');assert.equal(fs.readFileSync(join(outside,'sentinel'),'utf8'),'unchanged');cases.push('linked-home-refused');
  const hard=fixture('hardlink');fs.linkSync(join(outside,'sentinel'),join(hard,'AGENTS.md'));refused(hard,['--force'],'prompt-path-unconfined');assert.equal(fs.readFileSync(join(outside,'sentinel'),'utf8'),'unchanged');cases.push('hardlink-refused');
  const stage=fixture('staging-collision');fs.writeFileSync(join(stage,'AGENTS.md'),'Existing instructions\n');fs.writeFileSync(join(stage,'AGENTS.md.stage'),'unrelated stage bytes');refused(stage,[],'prompt-stage-conflict');cases.push('unowned-stage-preserved');
  const agent=fixture('agent-project');fs.cpSync(join(repo,'tests/fixtures/validation/valid/base'),agent,{recursive:true});cli(agent,['--create-agents-md']);
  const body=fs.readFileSync(join(agent,'.agents/lekalo.md'),'utf8');assert.ok(body.split('\n').length>=100&&body.split('\n').length<=150);
  for(const args of [['load','--ir'],['validate','--strict'],['inspect','planner.task'],['impact','planner.task'],['context','planner.task','--budget','4000'],['status']]) {
    const r=spawnSync(binary,['--json',...args,'--project','.'],{cwd:agent,encoding:'utf8',timeout:30000,maxBuffer:8e6});assert.equal(r.status,0,args.join(' ')+'\n'+r.stdout+'\n'+r.stderr);JSON.parse(r.stdout);
  }
  const structure=spawnSync(process.execPath,['scripts/check-structure.mjs','--project',relative(repo,agent).replaceAll('\\','/')],{cwd:repo,encoding:'utf8'});assert.equal(structure.status,0,structure.stdout+'\n'+structure.stderr);cases.push('installed-prompt-public-command-smoke');
  console.log(JSON.stringify({ok:true,gate:'install-prompt',cases}));
} finally {const real=fs.realpathSync(work);assert.equal(dirname(real),fs.realpathSync(parent));assert.ok(basename(real).startsWith('prompt-acceptance-'));fs.rmSync(real,{recursive:true,force:true});}
