# Issue #100: Codex independent review round 2

**Verdict: ISSUES — one reproducible P2 finding in imported result validation.**
The original round-1 F1/P1, F2/P2 and F3/P2 counterexamples are corrected. The
189-check live gate and seven focused Rust tests pass. Independent partial-token
probes expose an additional admission gap outside those checks: result rows can
claim success with a known token subset larger than its known parent.

Reviewed on 2026-10-05 in `ichinya/m7-issue-100`, worktree
`C:/Users/User/orca/workspaces/lekalo/m7-issue-100`. Initial HEAD was
`01cdb6100c208765a8659f62ef22d0e9eaedceeb` (Devin round-2 ACCEPT). Reviewed fixes:
`22605b4a` (token totals), `ef188c16` (infrastructure classification), and
`2b4ea23b` (integer literals). The starting worktree and index were clean.

Read the round-1 review, fix report, Devin round-2 review, changed production
code, published schemas, fixtures, gate and CI wiring. Rebuilt and exercised the
real CLI rather than adopting either report's verdict. Fetched
[live issue #100](https://github.com/ichinya/lekalo/issues/100) with
`gh issue view 100 --repo ichinya/lekalo`; it remains OPEN with nine unchecked
ACs. This review assesses the authorized deterministic evidence scope; actual
external A/B campaigns remain pending.

## Numbered finding

### 1. P2 — imported result rows bypass known token subset consistency

Source: `crates/lekalo-core/src/framework_lift/mod.rs:218` checks
`cachedInputTokens <= inputTokens` and `reasoningTokens <= outputTokens` when both
leaves are known during arm admission. It refuses violations at line 226 with
`evaluation.metric-inconsistent`, detail `subset-count`. However,
`validate_result` at line 393 invokes only `validate_token_totals` for row token
consistency at line 399. That helper checks the total against component bounds
and, when both parents are known, their exact sum. It does not reject an
impossible known subset/parent pair.

Reproduce by copying
`tests/fixtures/framework-lift-result/golden/negative.json` and changing only
the metrics of the successful B / `pair-two` row:

| Vector | Input | Output | Cached input | Reasoning | Total |
| --- | --- | --- | --- | --- | --- |
| Cached subset | known 40 | unknown | **known 41** | unknown | known 50 |
| Reasoning subset | unknown | known 10 | known 20 | **known 11** | known 50 |

For each vector, strict Ajv 8.17.1 accepts the result's shape and
`evaluation validate --family result` exits **0**, retaining
`status: "success"`, `verifiedSuccess: true`, and `firstPassSuccess: true`.
Its existing summaries and uncertainty also remain admitted. The partial lower
bounds are 41 and 31 respectively, so they do not exceed total 50; this is how
the separate subset check is bypassed.

Putting the same metrics into the B-neutral arm, with source coverage adjusted
for the unavailable/newly known leaves, produces a schema-valid arm.
`validate --family arm`, `record-arm`, and `compare` each exit **1** with
`evaluation.metric-inconsistent` / `subset-count` for both vectors.

The frozen `framework-lift-metrics-1` recipe explicitly defines cached input as
a subset of input and reasoning as a subset of output
(`docs/framework-lift.md:114`). These contradictions are provable from the row
itself; detecting them does not require authenticating an external provider or
resolving an oracle. This affects AC3's trustworthy token records and AC7's
future import surface.

Scope: the normal comparator rejects the invalid arms, so this finding does
**not** reopen the original above-cap success exploit. Imported records remain
recorded simulations/unverified evidence, and a receiver must resolve arm refs
and replay comparison. Those external checks are still necessary. The defect
is that standalone result admission accepts internally impossible token leaves
that arm admission already refuses, while retaining success flags.

Required correction: apply the known token subset invariants to result rows as
well as arms, preserving unavailable states and the existing normalizer. Add
live imported-row counterexamples with the opposite parent unavailable for
both cached-input and reasoning subsets. No schema widening or coercion of
unknown measurements to zero is needed.

### Self-contained reproduction

Run from this worktree after the locked CLI build. This creates and removes only
its own new temporary directory; fixture bytes and implementation files remain
unchanged. The assertions describe the defective behavior observed at the
reviewed HEAD.

```powershell
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
@'
const fs=require('node:fs'), path=require('node:path'), os=require('node:os');
const assert=require('node:assert/strict'), {spawnSync}=require('node:child_process');
assert.equal(require('ajv/package.json').version,'8.17.1');
const Ajv=require('ajv/dist/2020').default, ajv=new Ajv({strict:true,allErrors:true});
const root=process.cwd(), bin=path.join(root,'target/debug/lekalo.exe');
const read=(f,n)=>JSON.parse(fs.readFileSync(`tests/fixtures/framework-lift-${f}/golden/${n}.json`));
const shapes=Object.fromEntries(['arm','result'].map(f=>[f,ajv.compile(
  JSON.parse(fs.readFileSync(`contracts/framework-lift-${f}.schema.v0.6.4.json`)))]));
const common=['--baseline','tests/fixtures/framework-lift-baseline/golden/approved.json',
  '--task','tests/fixtures/framework-lift-task/golden/priority.json',
  '--campaign','tests/fixtures/framework-lift-campaign/golden/scheduled.json'];
const tempRoot=fs.realpathSync(os.tmpdir());
const scratch=fs.mkdtempSync(path.join(tempRoot,'lekalo-100-r2-subset-'));
function run(args) {
  const p=spawnSync(bin,['--json','evaluation',...args],{cwd:root,encoding:'utf8',timeout:30000});
  assert.ok(!p.error); return {exit:p.status,value:JSON.parse(p.status===0?p.stdout:p.stderr)};
}
try {
  for(const [label,unknown,part,value] of [
    ['cached','outputTokens','cachedInputTokens',41],
    ['reasoning','inputTokens','reasoningTokens',11]
  ]) {
    const arm=read('arm','b-neutral'), result=read('result','negative');
    arm.metrics[unknown]={state:'unknown'}; arm.metrics[part]={state:'known',value};
    arm.measurementSources=arm.measurementSources.filter(s=>s.metric!==unknown);
    if(!arm.measurementSources.some(s=>s.metric===part))
      arm.measurementSources.push({...arm.measurementSources[0],metric:part});
    const target=r=>r.rows.find(x=>x.slot.arm==='B'&&x.slot.pairId==='pair-two');
    target(result).metrics=structuredClone(arm.metrics);
    assert.ok(shapes.arm(arm)); assert.ok(shapes.result(result));
    const ap=path.join(scratch,label+'-arm.json'), rp=path.join(scratch,label+'-result.json');
    fs.writeFileSync(ap,JSON.stringify(arm)); fs.writeFileSync(rp,JSON.stringify(result));
    const imported=run(['validate','--family','result','--input',rp]);
    assert.equal(imported.exit,0);
    const row=target(imported.value);
    assert.equal(row.status,'success'); assert.equal(row.verifiedSuccess,true);
    assert.equal(row.firstPassSuccess,true);
    const refusals=[];
    for(const args of [
      ['validate','--family','arm','--input',ap],
      ['record-arm',...common,'--input',ap],
      ['compare',...common,'--arm',ap,'--consumer-alias','review-two-consumer']
    ]) {
      const observed=run(args), d=observed.value.diagnostics[0];
      assert.equal(observed.exit,1); assert.equal(d.id,'evaluation.metric-inconsistent');
      assert.equal(d.data.detail,'subset-count');
      refusals.push({command:args[0],exit:observed.exit,code:d.id,detail:d.data.detail});
    }
    console.log(JSON.stringify({label,importExit:imported.exit,status:row.status,
      verifiedSuccess:row.verifiedSuccess,firstPassSuccess:row.firstPassSuccess,refusals}));
  }
} finally {
  const owned=fs.realpathSync(scratch);
  assert.equal(path.dirname(owned),tempRoot);
  assert.ok(path.basename(owned).startsWith('lekalo-100-r2-subset-'));
  fs.rmSync(owned,{recursive:true});
}
'@ | node
```

## Original finding dispositions and independent probes

The independent drivers used direct CLI calls, their own canonical hash
calculations, raw byte mutations, and the published schemas. They did not import
the shipped gate or executor's test helpers. Including a successful rerun of
the embedded reproduction, there were **363 local CLI invocations**, not
external agent runs.

| Finding | Independently observed disposition |
| --- | --- |
| F1 / P1 — original stale total | **Corrected.** Input 10001 / output 10 / total 50 under cap 10000 refuses through arm validation, `record-arm`, `compare`, and imported-result validation: exit 1, `evaluation.metric-inconsistent`, `token-total`. Output 10001 with total 50 also refuses. |
| F1 — total and partial bounds | Eleven contradiction vectors refuse across those four paths: total zero, total 39 below input 40, total 9 below output 10, total 51 above exact 40+10, unavailable output with total below input, unavailable parents with cached/reasoning lower bounds, and two maximum-safe components with a too-small total. The separate imported subset gap is finding 1 above. |
| F1 — unavailable total | For each of `unknown`, `unsupported`, and `withheld`, input 10001, output 10001, joint input/output 6000+6000, cached 10001 with unavailable input, and reasoning 10001 with unavailable output classify as `task`, never success. Small 40+10 and entirely unavailable token breakdowns classify as `unsupported`. All 21 vectors retain their total state, all four scheduled rows and B's denominator of two. |
| F1 — overlap and inclusive cap | Known input 40 / output 10 / cached 20 / reasoning 10 / total 50 succeeds without double counting subsets. Input 9990 / output 10 / total 10000 succeeds at the inclusive cap. |
| F2 / P2 — infrastructure | **Corrected.** An infrastructure assertion with `failures: []` records and compares as `infrastructure`. Hard-fail plus infrastructure stays `task` despite judge 100. Provider plus infrastructure stays `provider`; provider plus hard-fail stays `task`; custody-security takes priority over both. Unsupported/interrupted sidecars and an unavailable total do not hide the witnessed infrastructure class. Nine combinations retain false success flags. |
| F3 / P2 — integer spellings | **Corrected.** 63 raw positive encodings across the five families agree with exact Ajv and produce byte-identical CLI output and unchanged canonical digests relative to their integer-spelled controls. Forms include `1.0`, `1e0`, `1E+0`, `10E-1`, `0.1E+1`, `0.001e3`, long scaled integral mantissas, `0.0`, `0e0`, `0E+0`, negative zero and extreme exponents on zero. |
| F3 — numeric refusal and bounds | Sixteen hostile spellings in each family (80 cases) refuse with `evaluation.protocol-invalid`: exact fraction `1.000000000000000000001`, ordinary fractions, negative counts, non-finite/malformed JSON, oversized safe integers, and nonzero extreme positive/negative exponents. An additional 40 positive spellings exercise powers, scaled mantissas and values through 9007199254740991. Signed result ratios remain supported. |
| F3 — joins and closure | Re-encoding every numeric leaf in baseline, task, campaign and all four arms as decimal integers preserves approval digests, passes real preflight/recording, and produces byte-identical comparison output. All five families still refuse unknown `rawPrompt`/`__proto__` keys and an escaped duplicate root key. |

Numeric spellings must respect each field's existing bounds. The baseline's
only numeric field is approval revision, minimum 1, so `0.0` is correctly
refused there by both Ajv and the CLI. Zero was tested on eligible task,
campaign, arm and result fields; one on eligible fields in all five families.
Fractional literals close enough to round to an integer in JavaScript are
refused from their exact decimal text, rather than admitted through lossy
floating-point parsing. Exact stream-byte comparisons include the CLI's
existing line endings.

## Fresh local verification and scope

Environment: Windows, Node 24.13.0, Cargo 1.98.0, product 0.6.4. Exact Ajv 8.17.1
was supplied outside the checkout. These results were obtained in this review,
after rebuilding the corrected CLI:

| Command / inspection | Result |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | Passed. |
| `$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'; node scripts/test-framework-lift-contracts.mjs` | Passed: **189 checks**, all five live families and all six refusal codes; `origin: recorded-simulation`, `externalAgentRuns: 0`. |
| `$env:CARGO_PROFILE_TEST_DEBUG='0'; cargo test framework_lift --locked -p lekalo-core --lib -j 1` | **7 passed, 0 failed, 0 ignored**, 1053 filtered. Covers token bounds, failure precedence and integer decoder regressions. |
| `$env:CARGO_PROFILE_TEST_DEBUG='0'; cargo test framework_lift --locked -j 1` | Requested filter also passed across the workspace: the same seven matching core tests passed; other compiled test targets had zero matches. Exit 0. |
| `cargo clippy --locked -p lekalo-cli --all-targets -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `node scripts/check-contract-versions.mjs --base c4ea9a5c` | Passed: product 0.6.4, 121 artifacts. |
| `node scripts/test-fixture-provenance.mjs` | Passed: 82 families, all synthetic. |
| `node scripts/test-docs-ownership.mjs --static` | Passed: 348 surfaces, 13 P0 owners. |
| Fix diff and CI inspection | Original 49 gate checks retained, 140 added. No changes to contracts, fixture goldens/provenance, Cargo dependencies/lock, registry, executor or CI across the fixes. CI provisions exact Ajv, builds the CLI, then runs the five-family gate in the same job. |

The unfiltered core/workspace suites, hosted CI and other OSes were not rerun here;
the fix and Devin reports' full-suite counts are not substituted for fresh
evidence. No implementation edits, provider calls, private repository/prompt
uploads or empirical A/B campaigns occurred. All arm fixtures remain
`origin: recorded-simulation`; **externalAgentRuns stays 0**. Frozen fixture
hashes were unchanged after the independent probes. New review scratch was
outside the checkout and removed after verification; no historical scratch or
other worktrees were changed.

## Issue acceptance boundary

| Live issue AC | Evidence / remaining boundary |
| --- | --- |
| 1. Three greenfield planner tasks have A/B results | Pending actual external campaigns; fixture replay is not such a result. |
| 2. One observed brownfield context/impact A/B result | Pending an authorized private external campaign and local collection. |
| 3. Success, tokens, files, iterations, cost per success | Recorded schema/gate surface exists; F1's cap bypass is corrected. Finding 1 leaves imported token rows inconsistent with the frozen normalizer. Authentic external measurements remain a collector seam. |
| 4. Negative/neutral results preserved honestly | Replayed all four unchanged arm goldens against the negative/neutral result golden, retaining hard failure, provider retry, costs and attrition. Simulation remains labeled. |
| 5. Exact model/harness/profile provenance | Existing pinned joins remain enforced by the live gate; the independent all-numbers replay preserves their digests. No assumptions about parallel #84 contract names or real executor qualification. |
| 6. Hard regression cannot be hidden by subjective score | Independent hard-fail plus infrastructure/provider vectors retain `task` despite judge 100; custody precedence remains intact. |
| 7. Future AIFHub result import | Closed result schema/validator exists; finding 1 affects its numeric admission. A future receiver must resolve pinned arms, replay comparison and establish source authenticity; no integration acceptance is claimed. |
| 8. No universal superiority claim | Evidence remains task/profile-scoped, simulated, with visible scheduled denominators and uncertainty; no empirical superiority inference. |
| 9. Anonymized public consumer role aliases | Existing opaque role-alias surface remains; the probes use an opaque review alias. Results remain local-private, and public aggregation/export enforcement is a future receiver boundary. |

Only this review document is committed. There is no push. The verdict remains
ISSUES until the imported subset counterexamples are refused without weakening
the contracts or discarding unavailable metrics.
