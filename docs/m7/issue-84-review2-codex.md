# Issue #84: Codex independent review round 2

**Verdict: ACCEPT for the F1/F2 fix delta and the measurable core milestone. No remaining finding requiring a change was reproduced.** Reviewed fix `96cf8535` at clean branch HEAD `8fa19ae9`, on `ichinya/m7-issue-84`, on 2026-10-05. Rebuilt the current source independently. The only change between the fix and review HEAD is `docs/m7/issue-84-review2-devin.md`.

Read the [round-1 findings](issue-84-review1-codex.md), [fix record](issue-84-fix1.md) and [Devin round-2 opinion](issue-84-review2-devin.md) as claims to challenge. Refreshed authority with `gh issue view 84 --repo ichinya/lekalo --json title,body,state,url,updatedAt`: [issue #84](https://github.com/ichinya/lekalo/issues/84) is open, body updated `2026-08-30T10:17:54Z`. This review does not accept adapter/Mago integration, predecessor lock/diff envelope integration or AI-agent A/B outcomes; those previously disclosed milestone boundaries remain.

## 1. F1 closure: independently reproduced structured refusals

A separate Node harness copied the synthetic planner fixture to a fresh OS temporary directory, generated a current lock and a fresh measured baseline/adoption ledger from the rebuilt binary, then exercised the original payloads. It did not import the architecture gate or generator helpers. The three payloads were:

- Bare U+00E9: `é`.
- Unicode after an ASCII prefix: `{"x":é}`.
- UTF-8 BOM (U+FEFF) followed by an **otherwise valid document for each admission route**.

| Route | Bare token | ASCII prefix | BOM + valid family document |
| --- | --- | --- | --- |
| `resolve --architecture-profiles` | invalid / 1 | invalid / 1 | invalid / 1 |
| `lock --architecture-profiles` | invalid / 1 | invalid / 1 | invalid / 1 |
| `diff --base-profiles` | invalid / 1 | invalid / 1 | invalid / 1 |
| `diff --candidate-profiles` | invalid / 1 | invalid / 1 | invalid / 1 |
| `assess --architecture-profiles` | invalid / 1 | invalid / 1 | invalid / 1 |
| `assess --architecture-lock` | invalid / 1 | invalid / 1 | invalid / 1 |
| `assess --baseline` | invalid / 1 | invalid / 1 | invalid / 1 |
| `assess --adoption`, valid baseline/date supplied | invalid / 1 | invalid / 1 | invalid / 1 |

All **24/24** returned parseable JSON on stderr, empty stdout, exactly one `architecture-profile.input-invalid` / `LEK-APR-001` diagnostic pinned to registry **0.6.5**, and exit **1**. The largest trimmed serialized envelope was **621 UTF-8 bytes**, below the asserted 4096-byte bound. None returned 101 or panic text.

Independent positive controls used adoption prose containing U+00E9, U+6F22, U+1F600, U+10000 and U+10FFFF. One document contained literal scalars; another escaped every non-ASCII UTF-16 code unit, including supplementary surrogate pairs. Both returned `valid`, exit **0**, assessment `adopting`. A direct library probe additionally checked exact decoded equality and full root spans for three escaped/literal pairs, including the minimum and maximum supplementary scalars: **six successful parses**. Six lone/mismatched/truncated/invalid Unicode escape controls still returned frontend errors with sliceable spans.

Source verification: `LineIndex::position` floors interior offsets to a scalar boundary (`crates/lekalo-core/src/loader/source.rs:83`), while `span` rounds its end upward (`:99`). The architecture decoder still calls the strict frontend and maps failure to its existing bounded refusal (`crates/lekalo-core/src/architecture_profile/mod.rs:72`). No ASCII-only filter replaces JSON parsing. The supplementary escape correction checks the following `\\u` at the cursor left by `parse_hex4` (`crates/lekalo-core/src/loader/frontends/json.rs:288`, `:368`); successful literal/escaped decoding was verified live, rather than inferred from this diff.

## 2. F2 closure and attempts to bypass catalog authority

Reconstructed the original exploit from the live `contracted-standard` resolution: a pinned child with only `architecture.justified-abstractions.required=true`. Exact Ajv **8.17.1** still accepts its structural shape. Runtime `assess --module planner` now refuses **both with and without `--check`**: invalid / exit **1**, `architecture-profile.input-invalid`. It does not reach assessment as a subjective evidence gap.

The live catalog contains **11 advisory rules**. Independently required each rule in an inherited child, tested assessment with and without `--check`, and tested a complete independent root through `resolve`: **33/33 admission refusals**, plus the two separately reconstructed original exploit calls. The rules tested were `errors-as-contracts`, `explicit-operation-contracts`, `explicit-target-bindings`, `immutable-defaults`, `justified-abstractions`, `no-hidden-observers`, `no-service-locator-magic`, `scenario-linked-behavior`, `single-entrypoint`, `strict-native-types` and `tracked-duplication`, all under the `architecture.` namespace.

Additional independent probes:

| Attempt | Observed result |
| --- | --- |
| Add `blockingBasis:"semantic"` to a selection, its child profile, or the document | Structural schema refusal and runtime invalid / 1. |
| Add selection `coverage:"complete"` or forge its `measurement` | Structural schema refusal and runtime invalid / 1. |
| Attach an inline catalog reclassifying R14 and repin `catalogRef` to it | Unknown-field refusal, invalid / 1. |
| Supply a schema-valid counterfeit catalog digest | Catalog pin refusal, invalid / 1. |
| Set R14 required in a caller-supplied lock | Exact lock verification refuses, invalid / 1. |
| Put a required R14 row into a baseline | Report admission refuses, invalid / 1. |
| Forge complete generation evidence in a baseline, with recomputed condition digest | Report admission refuses, invalid / 1. |
| Give semantic generation a numeric maximum | Selection admission refuses, invalid / 1; semantic basis does not grant measured-limit authority. |
| Child of `managed-generated` removes generation's required flag | Invalid / 1, `architecture-profile.weakening-unacknowledged`. |

The authority is the **embedded** catalog (`architecture_profile/mod.rs:12`, `:92`), not fields supplied by the profile. Documents must match its current reference (`:105`). Closed DTOs reject metadata injection (`crates/lekalo-core/src/architecture_profile/wire.rs:45`); `Selection` has only ID, enabled, severity, required and limit (`:76`). `selection_valid` admits required obligations only for enabled **semantic/measured** rules (`architecture_profile/mod.rs:153`), and is used in effective resolution (`:215`) and report/profile-row validation (`:875`, `:907`). Public typed document operations re-enter admission (`:177`); the touched Rust tests exercised those routes. Locks are compared against a freshly recomputed complete snapshot (`:273`).

The generation reclassification is a justified semantic obligation: repeated outputs for pinned inputs must be equal. It is not a way to reclassify arbitrary style rules. A valid child of `contracted-standard` explicitly requiring generation resolves, but its generation row remains **unsupported**, value `unsupported`, disposition `evidence-gap`; R14 remains non-required/advisory. Assessment returns evidence with exit **0** without `--check`, and denies with exit **3** with `--check`. The built-in managed profile also denies / 3. An unchanged generation baseline succeeds as a positive control; its forged counterpart uses the valid `observed` disposition and a recomputed condition digest, but still refuses complete coverage. Removal of the inherited obligation also refuses. No new receipts, clean coverage or empirical determinism were accepted.

This closes the round-1 AC6 admission gap. The semantic/measured qualification is a catalog invariant in addition to the closed structural schema; the schema was not weakened to accommodate the fix.

### Compact reproduction of the original failures

Run from this checkout after `cargo build --locked`. All writes are confined to a new temporary fixture copy. This independently reproduces the five primary F1 routes and the exact original F2 exploit; the full gate below additionally exercises lock/baseline/adoption inputs.

```powershell
@'
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const cp=require('node:child_process'),assert=require('node:assert/strict');
const root=process.cwd(),bin=path.join(root,'target/debug/lekalo.exe');
const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'lekalo-84-r2-repro-'));
fs.cpSync(path.join(root,'tests/fixtures/context-budget/planner'),tmp,{recursive:true});
const good=path.join(root,'contracts/architecture-profile.v0.6.5.json');
const document=JSON.parse(fs.readFileSync(good,'utf8'));
function run(args,invalid=false){
 const p=cp.spawnSync(bin,['--json','--no-cache','architecture-profile',...args],
  {cwd:tmp,encoding:'utf8',timeout:60000});
 assert.ifError(p.error);const e=JSON.parse(p.stdout||p.stderr);
 assert.equal(p.status,invalid?1:0);assert.equal(e.status,invalid?'invalid':'valid');
 if(invalid){assert.equal(p.stdout,'');assert.ok(e.reasonCodes.includes('architecture-profile.input-invalid'));}
 return e.architectureProfile??e.payload?.architectureProfile;
}
for(const bytes of ['\u00e9','{"x":\u00e9}','\ufeff'+JSON.stringify(document)]){
 const bad=path.join(tmp,'bad.json');fs.writeFileSync(bad,bytes);
 for(const args of [
  ['resolve','--architecture-profile','ai-strict','--architecture-profiles',bad],
  ['lock','--architecture-profiles',bad],
  ['diff','--base-profiles',bad,'--candidate-profiles',good],
  ['diff','--base-profiles',good,'--candidate-profiles',bad],
  ['assess','--module','planner','--architecture-profiles',bad]
 ])run(args,true);
}
const parent=run(['resolve','--architecture-profile','contracted-standard']);
const rule={...parent.rules.find(r=>r.id==='architecture.justified-abstractions'),required:true};
document.profiles.push({id:'style-required',version:'1',
 extends:{state:'known',value:parent.profileRef},rules:[rule]});
document.profiles.sort((a,b)=>a.id<b.id?-1:a.id>b.id?1:0);
const hostile=path.join(tmp,'style.json');fs.writeFileSync(hostile,JSON.stringify(document));
for(const check of [false,true])run(['assess','--module','planner',
 '--architecture-profile','style-required','--architecture-profiles',hostile,
 ...(check?['--check']:[])],true);
console.log('PASS: original F1/F2 refusals');
'@ | node
```

## 3. Span regression: ASCII and valid boundaries remain sane

Compiled a standalone Rust probe against the freshly built `target/debug/liblekalo_core.rlib`, with dependencies from `target/debug/deps`. Its source, executable and outputs stayed in the review's OS temporary directory. It called the public strict frontend and `LineIndex` directly, without copying their implementations or adding tests to the checkout.

All **15 malformed-token/error vectors** returned expected half-open spans. Representative exact outputs below use `[byte, line, column]`; bytes are zero-based, lines/columns one-based:

| Input | Start | End |
| --- | --- | --- |
| `x` | `[0,1,1]` | `[1,1,2]` |
| `{"x":?}` | `[5,1,6]` | `[6,1,7]` |
| `{"x":truX}` | `[5,1,6]` | `[6,1,7]` |
| `{"x":` at EOF | `[5,1,6]` | `[5,1,6]` |
| `"a\q"`, invalid escape character | `[3,1,4]` | `[4,1,5]` |
| `{"x":1,"x":2}`, duplicate key | `[7,1,8]` | `[10,1,11]` |
| `?` on line 2 after LF | `[8,2,7]` | `[9,2,8]` |
| Same line/column after CRLF | `[9,2,7]` | `[10,2,8]` |
| ASCII `?` after a preceding Unicode string and CRLF | `[29,3,7]` | `[30,3,8]` |
| `{"x":é}` | `[5,1,6]` | `[7,1,7]` |
| U+FEFF before `{}` | `[0,1,1]` | `[3,1,2]` |

Missing-colon/value and trailing-comma cases also passed. An oracle over empty, ASCII/LF/CRLF and mixed two/three/four-byte Unicode strings verified **34 position checks** and **244 span checks**, including every bounded start/end pair, offsets beyond EOF and `usize::MAX`. Valid boundaries keep their exact bytes; all accepted ranges remained ordered and sliceable. One initial scratch expectation pointed at the slash of `\q`; reading the unchanged scanner showed that its diagnostic intentionally points at `q`. Correcting that scratch expectation yielded the outputs above; this was not a product panic or a source change.

## 4. Independent gate results, preservation and custody

The final separate CLI harness completed **85 invocations** in a fresh fixture copy. It compared a recursive file-content manifest before and after **each** binary invocation: **85/85 unchanged**, including caller-selected profiles, locks, baselines and ledgers. Its F1/F2, legal Unicode and residual probes are reported above; these counts are independent of the repository gate's 124 probes. The compact reproduction embedded in this document was also extracted and replayed successfully (`PASS: original F1/F2 refusals`).

Used the requested external dependency directory and verified its actual package version:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH = $env:NODE_PATH
node -p "require('ajv/package.json').version"
# 8.17.1
```

| Command | Independently observed result |
| --- | --- |
| `cargo build --locked`; `target/debug/lekalo.exe --version` | Exit 0; current binary reports `lekalo 0.6.5`. |
| `cargo test --locked -p lekalo-core --lib loader:: -- --test-threads=1` | **39 passed**, 0 failed. |
| `cargo test --locked -p lekalo-core --lib architecture_profile::tests` | **7 passed**, 0 failed. |
| `node scripts/test-architecture-profile-contracts.mjs` | Exit 0; exact Ajv 8.17.1; 7 families, 9 goldens, 508 registry entries, 8 diagnostic IDs, **124 live probes**, 4 artifact faults; read-only. |
| `node scripts/test-fixture-provenance.mjs` | Exit 0; 78 families, all synthetic. |
| `node scripts/test-golden-normalization.mjs` | Exit 0; 243 files; Unicode ordering, CRLF and path producer controls. |
| `node scripts/check-contract-versions.mjs --base 827b15e5` | Exit 0; product 0.6.5; 125 versioned contract artifacts. |
| `node scripts/test-docs-ownership.mjs --static`; `node scripts/test-docs-ownership.mjs` | Both exit 0; 355 surfaces, 13 P0 owners, 33 negative owner controls; live help checked. |
| `cargo fmt --all -- --check`; `git diff --check` | Both exit 0. |

Full gate output:

```json
{"ok":true,"product":"0.6.5","ajv":"8.17.1","families":7,"goldens":9,"registryEntries":508,"diagnostics":8,"liveProbes":124,"artifactFaults":4,"fixProbes":{"multibyteRefusals":24,"unicodeSuccesses":2,"advisoryRequiredRefusals":36},"mode":"read-only"}
```

An independent byte audit against pre-fix review `827b15e5` found all **115 schemas unchanged**, both diagnostic registries unchanged, and only the two declared contract instance changes: generation's catalog blocking basis, plus the profile collection's catalog digest. After substituting precisely those fields, old/new parsed instances are equal; the new pin equals the SHA-256 of the live catalog bytes. Existing predecessor gate scripts, CI workflow, CLI index and provenance declaration remain byte-identical. Inspected the architecture gate diff: original negative/equality/golden checks remain, with F2 coverage added at `scripts/test-architecture-profile-contracts.mjs:168`, F1 coverage at `:202`, and exact new counts at `:254`. Read-only acceptance matched all nine committed current goldens; no authoring flag or regeneration ran during this review.

The unchanged workflow still provisions and verifies Ajv 8.17.1 (`.github/workflows/ci.yml:198`, `:200`), builds the locked workspace (`:204`), then invokes the architecture gate in the same `build-test` lane (`:295`, `:300`). Hosted CI was not executed here.

This round did not rerun the full workspace Rust suite, Clippy, MSRV, all neighbor contract gates, other operating systems, live adapters or A/B experiments. Devin's wider Rust count and the fix report's neighbor results are not promoted into this independent ledger. Local acceptance is for the tested fix/core boundary. Implementation files remained immutable; only `docs/m7/issue-84-review2-codex.md` is written and committed. No push or other-worktree mutation was performed.
