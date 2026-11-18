import assert from 'node:assert/strict';
import test from 'node:test';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {root,jobs,assertRuntimeGate,assertContractsGate,platformGraph,expandSetupSteps} from './lib/ci-workflow.mjs';
import {checkPlatform} from './check-ci-platform.mjs';
const caller=readFileSync(join(root,'.github/workflows/ci.yml'),'utf8').replaceAll('\r\n','\n');
const callee=readFileSync(join(root,'.github/workflows/ci-platform.yml'),'utf8').replaceAll('\r\n','\n');
test('host gates have one owner and preserve their commands',()=>assert.deepEqual(checkPlatform(),{ok:true,gates:46,platforms:3,jobsPerPlatform:8}));
const mutations=[
 ['missing external service build',s=>s.replace('      - name: Build pinned external services\n        run: node scripts/external-tool.mjs --build-services\n','')],
 ['missing qualification prebuild',s=>s.replace('      - name: Build the core qualification test executable before its deadline\n        run: cargo test -p lekalo-core --lib --no-run --locked\n','')],
 ['conditional qualification prebuild',s=>s.replace('      - name: Build the core qualification test executable before its deadline\n','      - name: Build the core qualification test executable before its deadline\n        if: false\n')],
 ['conditional external service build',s=>s.replace('      - name: Build pinned external services\n','      - name: Build pinned external services\n        if: false\n')],
 ['Rust barrier',s=>s.replace('  rust:\n','  rust:\n    needs: build-debug\n')],
 ['process Rust barrier',s=>s.replace('  rust-process:\n','  rust-process:\n    needs: build-debug\n')],
 ['optional process Rust',s=>s.replace('  rust-process:\n','  rust-process:\n    if: false\n')],
 ['core Rust repeats all groups',s=>s.replace('--lane=core','--lane=all')],
 ['process Rust repeats core groups',s=>s.replace('--lane=process','--lane=core')],
 ['conditional process group',s=>s.replace('      - name: Run CLI and Windows process fixtures independently','      - name: Run CLI and Windows process fixtures independently\n        if: false')],
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
 ['missing producer binary',s=>s.replace('          cargo build -p lekalo-cli --bin lekalo --locked\n','')],
 ['rerun name collision',s=>s.replace('runtime-debug-${{ inputs.os }}-${{ github.run_attempt }}','runtime-debug-${{ inputs.os }}')],
 ['shortened retention',s=>s.replace('retention-days: 7','retention-days: 1')],
 ['uncompressed runtime uploads',s=>s.replace('compression-level: 6','compression-level: 0')],
 ['duplicate under another name',s=>s.replace('  rust:\n','  rust:\n').replace('      - name: Run the classification CLI e2e gate (issue #87)','      - name: Unexpected duplicate\n        run: node scripts/test-classification-cli.mjs\n      - name: Run the classification CLI e2e gate (issue #87)')],
];
for(const [name,change] of mutations)test(`graph guard rejects ${name}`,()=>{const changed=change(callee);assert.notEqual(changed,callee,'Negative test must mutate its input');assert.throws(()=>checkPlatform({callee:changed}));});
test('caller cannot change event/fork policy or remove an OS',()=>{
 assert.throws(()=>checkPlatform({caller:caller.replace('os: [ubuntu-26.04, windows-latest, macos-latest]','os: [ubuntu-26.04, windows-latest]')}));
 assert.throws(()=>checkPlatform({caller:caller.replace('pull_request:', 'pull_request_target:')}));
});

test('live structural guards resolve the called workflow and pinned handoff',()=>{
 for(const script of ['test-privacy-evaluator-parity.mjs','test-privacy-runtime-cli.mjs','test-privacy-leak-corpus.mjs'])assertRuntimeGate(script);
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
