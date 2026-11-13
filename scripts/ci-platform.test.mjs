import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {root,assertRuntimeGate,assertContractsGate,platformGraph,expandSetupSteps} from './lib/ci-workflow.mjs';
import {checkPlatform} from './check-ci-platform.mjs';
const caller=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8').replaceAll('\r\n','\n');
const callee=readFileSync(join(root,'.github/workflows/ci-platform.yml'),'utf8').replaceAll('\r\n','\n');
test('all 91 original gates have one owner with preserved semantics',()=>assert.deepEqual(checkPlatform(),{ok:true,gates:91,platforms:3,jobsPerPlatform:11}));
const mutations=[
 ['Rust barrier',s=>s.replace('  rust:\n','  rust:\n    needs: build-debug\n')],
 ['release barrier',s=>s.replace('    needs: build-debug','    needs: [build-debug, build-release]')],
 ['cross-OS runner',s=>s.replace('runs-on: ${{ inputs.os }}','runs-on: ubuntu-24.04')],
 ['current consumer attempt',s=>s.replace('RUNTIME_PRODUCER_ATTEMPT: ${{ needs.build-debug.outputs.producer-attempt }}','RUNTIME_PRODUCER_ATTEMPT: ${{ github.run_attempt }}')],
 ['wrong artifact ID',s=>s.replace('artifact-ids: ${{ needs.build-debug.outputs.artifact-id }}','artifact-ids: 123')],
 ['conditional verification',s=>s.replace('      - name: "Verify and install runtime before gates"','      - name: "Verify and install runtime before gates"\n        if: false')],
 ['optional consumer',s=>s.replace('  cli-core:\n','  cli-core:\n    continue-on-error: true\n')],
 ['optional Rust',s=>s.replace('  rust:\n','  rust:\n    if: false\n')],
 ['different gate arguments',s=>s.replace('node scripts/run-golden.mjs --verify','node scripts/run-golden.mjs --update')],
 ['missing gate',s=>s.replace('node scripts/test-classification-cli.mjs','echo skipped')],
 ['missing runtime provision',s=>s.replaceAll('uses: shivammathur/setup-php@v2','uses: actions/setup-node@v7')],
 ['weaker timeout',s=>s.replace('timeout-minutes: 3','timeout-minutes: 30')],
 ['disabled assertions',s=>s.replace('CARGO_PROFILE_DEV_DEBUG_ASSERTIONS: "true"','CARGO_PROFILE_DEV_DEBUG_ASSERTIONS: "false"')],
 ['overwriting verified CLI from cache',s=>s.replace('cache-targets: false','cache-targets: true')],
 ['missing producer binary',s=>s.replace('          cargo build -p lekalo-core --example typescript-native-contract --locked\n','')],
 ['rerun name collision',s=>s.replace('runtime-debug-${{ inputs.os }}-${{ github.run_attempt }}','runtime-debug-${{ inputs.os }}')],
 ['shortened retention',s=>s.replace('retention-days: 7','retention-days: 1')],
 ['duplicate under another name',s=>s.replace('  rust:\n','  rust:\n').replace('      - name: Run the classification CLI e2e gate (issue #87)','      - name: Unexpected duplicate\n        run: node scripts/test-classification-cli.mjs\n      - name: Run the classification CLI e2e gate (issue #87)')],
];
for(const [name,change] of mutations)test(`graph guard rejects ${name}`,()=>{const changed=change(callee);assert.notEqual(changed,callee,'Negative test must mutate its input');assert.throws(()=>checkPlatform({callee:changed}));});
test('caller cannot change event/fork policy or remove an OS',()=>{
 assert.throws(()=>checkPlatform({caller:caller.replace('os: [ubuntu-26.04, windows-latest, macos-latest]','os: [ubuntu-26.04, windows-latest]')}));
 assert.throws(()=>checkPlatform({caller:caller.replace('pull_request:', 'pull_request_target:')}));
});
test('live structural guards resolve the called workflow and pinned handoff',()=>{
 assertContractsGate('test-mcp-contracts.mjs');assertContractsGate('test-mcp-boundaries.mjs');
 for(const script of ['test-coupling-contracts.mjs','test-framework-lift-contracts.mjs','test-ai-lint-report-contracts.mjs','test-waivers-contracts.mjs','test-mcp-stdio.mjs'])assertRuntimeGate(script,{ajv:true});
 const graph=platformGraph();graph['cli-core'].steps.find(s=>s.run?.includes('ci-runtime-artifacts.mjs verify')).condition='false';
 assert.throws(()=>assertRuntimeGate('test-coupling-contracts.mjs',{graph}),/Conditional handoff/);
});
test('unsupported aliases, duplicate anchors and overrides fail closed',()=>{
 assert.throws(()=>expandSetupSteps('      - *ci-unknown'),/Unknown/);
 assert.throws(()=>expandSetupSteps('      - &ci-one\n        run: true\n      - &ci-one\n        run: true'),/Duplicate/);
 assert.throws(()=>expandSetupSteps('      - &ci-one\n        run: true\n      - *ci-one\n        if: false'),/override/);
});
test('docs replay requires its own Go toolchain for the UI checked join',()=>{
 const start=callee.indexOf('  docs:'),end=callee.indexOf('  mcp:',start);
 assert(start>=0&&end>start);
 const block=callee.slice(start,end),missing=block.replace('      - *ci-setup-90\n','');
 assert.notEqual(block,missing,'Docs must explicitly provision Go/gofmt');
 assert.throws(()=>checkPlatform({callee:callee.slice(0,start)+missing+callee.slice(end)}),/docs: missing or altered prerequisite/);
});
