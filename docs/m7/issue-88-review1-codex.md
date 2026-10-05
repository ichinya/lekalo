# Issue #88: Codex independent review round 1

**Verdict: ISSUES.** One reproducible P2 finding: the successor accepts a native
waiver without its required capability fingerprint when the producer supplies
`unsupported`. Normal preview/apply then suppresses a blocking finding. The four
requested contract gates pass; they do not cover this admission case.

Reviewed on 2026-10-05 in `C:/Users/User/orca/workspaces/lekalo/m7-issue-88`, branch
`ichinya/m7-issue-88`. Implementation:
`faa220fa48780bb8e2307cb2c862646dfbd1a452`; starting HEAD:
`e509ad99e65701124608d24829eac086a8927ff1`; integration base:
`a2aa1893766c01434842ca5062188a43898f41c5`. The starting index and worktree were
clean. Read the research, implementation report and Devin review. Devin's ACCEPT
was treated as a claim and independently challenged.

Acceptance authority: [live issue #88](https://github.com/ichinya/lekalo/issues/88),
fetched with `gh issue view 88 --repo ichinya/lekalo` using JSON output. It remains
OPEN, with seven acceptance criteria, no comments and body update
`2026-08-30T10:17:56Z`. The review uses the existing profile machinery and the
declared #84 seam; it does not assume the parallel branch's contract names.

## 1. P2 — Applicable native provenance pins can be declared unsupported and still authorize suppression

**Location:** `crates/lekalo-core/src/waivers/mod.rs:305`, with the native projection
at `crates/lekalo-core/src/waivers/lint.rs:84` and `:106`, and eligibility/application
at `waivers/mod.rs:403`, `:446` and `:468`.

`pin_differences` treats any pair of `Unsupported` states as satisfied. It does
not check whether that dimension is actually inapplicable to this producer under
the admitted policy. The native adapter copies `e.pins.capabilities` directly;
its revision projection also preserves `Unsupported`. No native admission check
or profile requirement prevents these states from producing an effective waiver.

This contradicts `docs/waivers.md:91-93`, which requires a known native capability
digest. It also limits the implementation report's claim that incomplete proof
cannot grant acceptance. The shared family gates supply a known native capability
digest for their positive native case, so this omission is not caught by their
39 audit controls.

**Independent reproduction:** a disposable copy of the real AI-lint Model, one
enabled native `hidden.string-reference` rule, admitted native source bytes and
record, current Model/IR pins, and the current CI lint profile. Change only the
native capability pin to `{"state":"unsupported"}` before authoring the waiver.
Use the actual `waiverInput` emitted by the rebuilt binary, then ordinary
`waivers add` preview and `--apply` with its returned plan ID. No forged store or
recomputed approval hash is required.

| Operation | Observed result |
| --- | --- |
| Raw native `ai-lint --check` | Exit 3; one active warning |
| `waivers add` preview | Exit 0; candidate eligible |
| Same add with `--apply <planId>` | Exit 0; store written |
| Native `ai-lint --waivers ... --check` | Exit 0; one waived finding |
| Audit entry | `expiring`, `effective:true`, no unverifiable-pin reason |
| Fact fingerprint | `capabilities:{state:"unsupported"}` |

A separately constructed native producer document reproduced the same result
for an unsupported revision pin. Controls with a **known** capability pin allow
add, while `unknown` and `withheld` capability pins refuse add with exit 1. Known
adapter/version and revision changes correctly invalidate existing waivers. This
finding concerns required-pin admission; it does not claim those changes evade
the existing whole-fact digest comparison.

**Expected:** preserve the diagnostic fact, but keep acceptance ineffective when
an applicable native provenance requirement is unavailable. Resolve required
pins/applicability through the admitted policy/producer seam. Preserve the
legitimate Model-only unsupported adapter/capability dimensions. Add independent
native unsupported-pin controls alongside the known/unknown/withheld controls.

The following standalone reproducer was executed successfully. Save it outside
the checkout as a `.mjs` file and run it with Node from this repository root after
the locked build. It creates only an external temporary project and uses the
committed fixtures as input data, without importing the implementation's gate
helpers.

```javascript
import assert from 'node:assert/strict';
import {cpSync, mkdtempSync, readFileSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
const repo=process.cwd(),project=mkdtempSync(join(tmpdir(),'lekalo-88-native-pin-'));
const bin=join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
cpSync(join(repo,'tests/fixtures/ai-lint/model'),project,{recursive:true});
const read=p=>JSON.parse(readFileSync(join(repo,p),'utf8'));
const write=(p,v)=>writeFileSync(join(project,p),JSON.stringify(v));
function cli(args,expected){
 const r=spawnSync(bin,['--json','--no-cache',...args],{cwd:project,encoding:'utf8',timeout:60000});
 assert.ifError(r.error);assert.equal(r.status,expected,r.stdout+r.stderr);
 const v=JSON.parse(r.stdout.trim()||r.stderr.trim());return v.payload??v;
}
const config=read('tests/fixtures/ai-lint-config/golden/config.json');
for(const p of config.profiles)for(const r of p.rules)
 r.enabled=p.id!=='off'&&r.id==='hidden.string-reference';
write('config.json',config);
const lint=['ai-lint','--module','planner','--config','config.json','--lint-profile','ci','--waiver-facts'];
const model=cli(lint,0).report;
const evidence=read('tests/fixtures/ai-lint-evidence/golden/evidence.json');
evidence.pins.model={state:'known',value:model.modelRef};
evidence.pins.ir={state:'known',value:model.irRef};
evidence.pins.capabilities={state:'unsupported'};
write('evidence.json',evidence);
const native=[...lint,'--evidence','evidence.json'];
const raw=cli([...native,'--check'],3);assert.equal(raw.report.summary.raw,1);
write('facts.json',raw.waiverInput);const f=raw.waiverInput.facts[0];
const add=['waivers','add',f.selector.id,'--facts','facts.json','--profile-kind','ai-lint',
 '--lint-config','config.json','--profile','ci','--as-of','2026-10-05T12:30:00Z',
 '--id','missing-native-capability','--symbol',f.symbol.value,'--target',f.target,
 '--subject',f.subject,'--owner','synthetic-owner','--approver','synthetic-reviewer',
 '--approval-ref','review/88','--reason','Synthetic native fingerprint admission probe.',
 '--source-issue','ichinya/lekalo#88','--expires','2026-10-05T12:30:10Z'];
const preview=cli(add,0);const applied=cli([...add,'--apply',preview.planId],0);
assert.equal(applied.applied,true);
const result=cli([...native,'--waivers',join(project,'lekalo.waivers.json'),
 '--as-of','2026-10-05T12:30:00Z','--check'],0);
assert.equal(result.report.summary.waived,1);
console.log(JSON.stringify({project,rawExit:3,previewExit:0,applyExit:0,waivedExit:0,
 pin:result.waiverInput.facts[0].fingerprint.capabilities,
 status:result.waiverAudit.entries[0].status,waived:result.report.summary.waived}));
```

## Requested checks

The exact locked CLI build passed:

```powershell
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
```

All four gates passed with updates disabled and the rebuilt CLI. The three
successor gates report `live:true`, version `0.6.5`, exact Ajv **8.17.1**, and
39 audits each. The predecessor gate reports `live:true`, schema `0.6.4`.
The live gates compare generated reports to committed goldens, not just schemas.

Additional checks passed: seven focused `waivers::` core unit tests; schema
generator `--check`; fixture provenance (79 synthetic families); contract version
check against the integration base (product `0.6.5`, 118 artifact families);
Rust format check; live documentation ownership (345 surfaces, 13 P0 page owners).
The full core/workspace suites and Clippy were not rerun for this review; their
earlier reported results are not independent evidence here.

Read-only Git comparison shows that all **133 pre-existing contract artifacts**
are unchanged from the integration base; the only contract additions are the
three `0.6.5` schemas. The actual registry has **500 entries** at `0.6.4`.
CI provisions exact Ajv 8.17.1 at `ci.yml:198`, builds at `:204`, and invokes
the predecessor and three successor gates at `:273` and `:280-282`, in the same
build-test job after the build. The matrix uses the existing Bash convention.
No gate or predecessor schema weakening was found in this delta.

## Independent probes and acceptance mapping

The main probe harness made **74 real CLI invocations** in an external disposable
Model copy. Separate normal-authoring, native pin-state and optional-capability
controls brought the independent total to **97 CLI invocations**. It did not
import `scripts/lib/waivers-contract-gate.mjs` or its fixture builders. Positive
successor stores, inputs and audits were validated separately with strict Ajv
8.17.1. Actual exit codes, JSON facts, dispositions and filesystem bytes were
asserted; 67 emitted successor contract instances passed schema validation.
All probe materials are synthetic.

| Requested probe | Independent result |
| --- | --- |
| Expiry boundary | One real blocking Model finding, all eligible findings waived: at `2026-10-05T12:30:10Z`, exit 0/active 0; at `12:30:11Z`, exit 3/waived 0/expired 1. Same raw finding before and after. |
| Project/module scope leakage | Added a different actual Model symbol occurrence with the same rule, subject, condition and other pins. Only the approved occurrence was waived. Reusing the old finding ID for the different symbol produced `stale`, mismatch `fact`, waived 0. Both project and module scopes were exercised. |
| Stale pins | Separate changes to every stored fingerprint dimension stopped acceptance. A real Model description change refused the old input (exit 1); freshly produced input made the old waiver stale. A real native producer version change and a real source revision change independently made the native waiver stale. See finding 1 for missing-pin admission. |
| Non-waivable profile state | Current default validation profile: `semantic.public-output-private-type` remained effective severity `error`, non-waivable and denied, exit 3, despite a supplied warning and matching waiver. Current lint config with required coverage also refused waiver effectiveness. The seven core tests additionally cover actual resolved target component requirements. |
| Predecessor dispatch | A `0.6.4` store with unknown expiry still waived one finding through the legacy path and emitted no successor `waiverAudit`. A `0.6.5` store containing legacy entry shapes refused with exit 1. Successor inventory also refused the predecessor shape. |
| Add path/plan safety | Preview preserved store bytes; wrong digest, changed reason and changed pre-state bytes each refused. Correct digest applied exactly the candidate. Parent traversal, absolute outside-root destination, Model-root and cache destinations refused; no outside file was created. A hard-linked canonical store refused. |
| Closed decode fuzz | 26 main negative cases refused: nested/top-level unknown members; snake-case alias; invalid rule ID; wildcard/short/array/wrong-kind scopes; extra state payload; malformed UTC/date; no finite deadline; control characters; duplicate and escaped duplicate keys; invalid UTF-8; malformed/aliased facts and duplicate fact IDs. A separate correctly re-bound entry using the real code `LEK-INDIRECTION-001` in place of its rule ID also refused with exit 1: wire matching does not resolve that presentation alias. |
| AC honesty | Every AC is mapped below. Required native fingerprint completeness is overstated; finding 1 is reproducible. Companion evidence, synthetic producer inputs, approval metadata and external integration limits are distinguished from automatic workflow/provider acceptance. |

| AC | Concrete evidence and review assessment |
| --- | --- |
| 1. Waived fact visible with waiver link | `waivers/lint.rs::apply`, `mod.rs::audit`; independent comparison removed only disposition/link and proved all other native finding fields unchanged. Waiver ID remains visible; raw = active + waived. Optional `transport.streaming` capability audit retained the identical `unsupported` fact with `accepted-risk` disposition. **Demonstrated** in complete reports; the frozen lint envelope's diagnostics remain active-only as documented. |
| 2. Expiry restores gate | `mod.rs:412`, `:440`, `:468`; exact equality/next-second real `ai-lint --check` pair above. **Demonstrated.** |
| 3. No unrelated-symbol acceptance | `mod.rs::scope_matches`, occurrence match and `:397` fact binding; independent project/module occurrence and reused-ID probes. **Demonstrated.** |
| 4. Revision invalidation by policy | `lint.rs::facts`, `mod.rs::pin_differences`; real Model, adapter version and revision changes, plus all five individual stored pin mismatches. **Known-pin invalidation demonstrated; required native pin admission has issue 1.** |
| 5. Security-critical non-waivable fixture | `policy.rs::ValidationState` consumes admitted `validator/profile.rs` severity; independent registered security-sensitive rule under the actual default profile, plus required-coverage control. **Demonstrated for current profile machinery.** Integration with #84's future resolved profile remains a seam. |
| 6. Active/expiring/expired/stale audit | `mod.rs::audit`, closed audit schema; separate far-future active, near-deadline expiring, next-second expired and changed-pin stale results. **Statuses and counters demonstrated.** Missing native pin state is incorrectly eligible in issue 1. |
| 7. Waiver diff in review/done evidence | `mod.rs::compare`, `done_digest`; CLI `audit --base/--done/--locked`. Independent reason change produced a typed `changed` row and new done digest; the prior done digest refused, unchanged done proof passed; current source lock digest passed and mismatched digest refused. **Demonstrated for the explicitly selected audit companion.** Existing semantic diff/lock/readiness/CI-report consumers do not automatically attach or require it, and lock 0.3.2 has no waiver pin. This is not evidence of automatic integration into those workflows. |

All eleven field groups are present in the successor DTO and schema. Normal add
populates ID, exact selector/scope/occurrence, reason, owner/approver, creation and
finite deadline, source/risk, fingerprint states and lineage. Distinct owner and
approver strings and an approval-subject hash are local integrity metadata, not
authenticated issuer approval. The implementation documentation acknowledges
that boundary; this review does not claim organizational approval authentication.

Waivers remain explicit versioned-file inputs, with add writing the project-root
store rather than committing Git. No second successor suppression format or
union with legacy input was found. No privacy export authority is granted.
Neutral fact/audit digests provide a supplied-document provenance seam; no live
HLV/AIFHub transport or acceptance was exercised or established. Evaluation time
is explicitly supplied; trusted current CI time remains the caller's duty.

## Delivery boundary

Only `docs/m7/issue-88-review1-codex.md` is changed for delivery. Implementation,
contracts, tracked fixtures and other worktrees were not edited. Temporary
probes and their JSON outputs are outside the checkout at
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-codex-review-c58161ab06df4f99ab41291e5fab7114`;
the compact reproducer also created `lekalo-88-native-pin-vTfVqk` under the same
system temporary directory. They are local review evidence, not committed
fixtures. Delivery is a local review-only commit; no push, hosted CI run,
Linux/macOS runtime acceptance, implementation fix or issue closure is claimed.
