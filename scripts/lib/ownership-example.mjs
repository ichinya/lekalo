// One reviewed synthetic consumer; canonical Model/adapter inputs reuse the managed fixture.
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {dirname,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'../..');
const sha=bytes=>'sha256:'+createHash('sha256').update(bytes).digest('hex');
function json(stage,path,value){mkdirSync(dirname(join(stage,path)),{recursive:true});writeFileSync(join(stage,path),JSON.stringify(value,null,2)+'\n');}
export function materializeOwnershipExample(stage) {
  cpSync(join(root, 'tests/fixtures/managed/planner'), stage, { recursive: true });
  const policy = JSON.parse(readFileSync(join(stage, 'ownership.json')));
  policy.modules[0].mode = 'observed';
  const other = structuredClone(policy.modules[0]);
  other.module = 'legacy'; other.targets[0].generatedRoot = 'apps/legacy/gen';
  policy.modules.push(other);
  policy.modules.sort((a,b)=>a.module.localeCompare(b.module));
  json(stage, 'lekalo/ownership.json', policy);
  mkdirSync(join(stage, 'src'), { recursive: true });
  const functions = [
    ['complete_task', 'completeTask', 'return {done: input.done, id: input.id};'],
    ['focus_task', 'focusTask', 'return {id: input.id, title: input.title};'],
  ];
  for (const [file, name, body] of functions) writeFileSync(join(stage, `src/${file}.mjs`), `export function ${name}(input) { ${body} }\n`);
  const declaration = {
    schemaVersion: 'lekalo/contracted-declaration/v0.4.0',
    adapter: { id: 'ownership-fixture', version: '1.0.0', digest: null }, project: 'planner_fixture',
    revision: sha(Buffer.from('reviewed-maintained-fixture')), artifacts: [],
    symbols: functions.map(([name]) => ({
      id: `planner.${name}`, kind: name === 'complete_task' ? 'command' : 'query',
      source: { path: `src/${name}.mjs`, line: 1 }, fingerprint: sha(readFileSync(join(stage, `src/${name}.mjs`))),
      signature: name === 'complete_task'
        ? { inputs: [{ name: 'done', type: 'planner.boolean', required: true }, { name: 'id', type: 'planner.task_id', required: true }], output: null, reads: [] }
        : { inputs: [], output: 'planner.task', reads: ['planner.task'] }, effects: [],
    })),
  };
  json(stage, 'declaration.json', declaration);
  const corpus = JSON.parse(readFileSync(join(stage, 'coverage.json'))).requirements.map(row => ({
    id: row.id, input: row.sample.value.input, expected: row.sample.value.output,
  }));
  json(stage, 'scenarios.json', corpus);
  writeFileSync(join(stage, 'runner.mjs'), `import { readFileSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const bindings = JSON.parse(readFileSync('ownership-bindings.json'));
const scenarios = JSON.parse(readFileSync('scenarios.json'));
const observed = [];
writeFileSync('scratch/probe.json', JSON.stringify({privateTestStorage:true}));
for (const binding of bindings) {
  const scenario = scenarios.find(row => row.id === binding.symbol);
  const implementation = await import(pathToFileURL(resolve(binding.path)));
  const output = await implementation[binding.nativeSymbol](scenario.input);
  observed.push({scenario:scenario.id,output,error:null,state:null,effects:[],authorization:null,transaction:null});
}
console.log(JSON.stringify(observed));\n`);
  const expected = corpus.map(row => ({scenario: row.id, output: row.expected, error: null, state: null, effects: [], authorization: null, transaction: null}));
  return { stage, request: {
    schemaVersion: 'lekalo/ownership-transition-request/v1.0.0', module: 'planner', symbol: null, to: 'contracted',
    inputs: ['runner.mjs', 'scenarios.json'], coverage: null, declaration: 'declaration.json', unknownBehavior: [],
    runner: { program: 'node', digest: sha(readFileSync(process.execPath)), args: ['runner.mjs'], expected },
  } };
}
export function qualifyOwnershipCoverage(stage,bin){
 const call=args=>{const r=spawnSync(bin,['--no-cache','--json',...args,'--project','.'],{cwd:stage,encoding:'utf8',timeout:30000,windowsHide:true,env:{...process.env,LEKALO_PROJECT:''}});assert.equal(r.status,0,r.stderr||r.stdout||String(r.error));return r.stdout.trim();};
 const model=call(['load']);const ir=call(['load','--ir']);const doc=JSON.parse(ir);
 const prefix='{"status":"valid","modelVersion":"'+doc.modelVersion+'","ir":';assert(ir.startsWith(prefix),'unexpected IR envelope');
 const coverage=JSON.parse(readFileSync(join(stage,'coverage.json')));
 coverage.authority.value.model=sha(Buffer.from(model));coverage.authority.value.ir=sha(Buffer.from(ir.slice(prefix.length,-1)));
 json(stage,'coverage.json',coverage);
}
