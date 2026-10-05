# Issue #102: Codex independent review, round 1

**Verdict: ISSUES. Two P2 findings.** Reviewed implementation `e7065777ab57f5924f80929ac4a80dfac4c536c2`
and Devin review `a754c755` on `ichinya/m7-issue-102`, on 2026-10-05.
Implementation base: research commit `d26991a0`; integration base: `a2aa1893`.
The worktree and index were clean at review start. Only this review document
is changed and committed; no implementation edit, push or other worktree
mutation is part of this review.

Authority: [live issue #102](https://github.com/ichinya/lekalo/issues/102),
fetched with `gh issue view 102 --repo ichinya/lekalo --json title,body,state,comments,url`.
The issue is OPEN with eight ACs. Read [research](issue-102-research.md),
[implementation](issue-102-implementation.md) and [Devin's ACCEPT](issue-102-review1-devin.md).
Devin's conclusions were inputs to challenge, not acceptance evidence.
[#100](https://github.com/ichinya/lekalo/issues/100) is still OPEN at this
review's API refresh. This review tests a synthetic typed handoff, without
claiming completed evaluation execution, measured Framework Lift, hosted CI
or production AIFHub acceptance.

## Findings

1. **[P2] `repository-store` cannot reach an authorized ready state.**

   `crates/lekalo-core/src/metrics_export/privacy.rs:17` passes two different
   opaque repository identities to the shipped destination resolver: the
   projection digest and a separate consumer digest. The `RepositoryStore`
   branch at `privacy/export.rs:272` assigns those identities to destination
   and source while declaring `same-repository` / `same-origin`.
   `privacy/evaluate.rs:1193` correctly requires identical repository refs
   for that relationship and returns `repository.same-origin-contradiction`.

   Independent reproduction with five or six matching paired units and
   current, verified, subject-bound aggregation/declassification/transfer
   evidence returns exit 0, preview `status:"blocked"` and that reason. The
   two endpoint refs differ. The equivalent five other destination cases
   reach `ready`. Matching consent freshness, evidence purposes and binding
   does not repair this metadata contradiction; changing the supplied
   endpoint ref also mismatches the generated template's subject.

   This is a functional failure of the documented six-destination metrics
   surface, not a privacy permission bypass. The resolver/evaluator behavior
   is inherited from #119; the new metrics adapter supplies the incoherent
   identity pair. It needs a coherent same-origin identity for repository
   storage and a live authorized case for every destination. Keep the frozen
   evaluator's contradiction refusal intact. The current metrics gate tests
   publication, not all six authorized destination paths, so it stays green.

2. **[P2] Digest-valid sources can fail the frozen #121 schemas and still be exported.**

   `metrics_export/mod.rs:178` validates selected assertion headers, scope,
   policy and typed rows, but does not close/validate the assertion root.
   `mod.rs:207` checks record field inventories and selected typed blocks,
   without validating all record fields against the frozen record contract.
   `run_history/store.rs:816` rehashes stored bodies and frozen references;
   a matching digest is not complete schema validation.

   Independent reproduction adds a root `prompt:"private-prompt-canary"`
   to a stored assertion set. Exact Ajv 8.17.1 rejects it against
   `run-assertions.schema.v0.4.0.json`. Recompute its stored digest, the linked
   record's `assertionsRef.digest`, and that record's stored body digest.
   Metrics preview still reaches `ready` with newly bound synthetic
   authorization, and confirmation returns `written:true`. Separately, a
   record body with `recordedAt:"invalid-not-a-timestamp"` and a matching
   digest fails the record schema but receives a normal aggregate preview.

   Preconditions are write access to the disposable SQLite custody store
   and coherent digest rebinding, the same class of mutation already used
   by the shipped metrics gate's rehashed-policy/role controls. Ordinary
   recorder ingestion rejects these documents. No claim is made that an
   external caller can rewrite the database or mint authentic authorization.
   The assertion canary **does not appear in the public payload**: positive
   projection holds. The defect is incomplete source admission for AC1,
   accepting a closed-family-invalid document as a versioned #121 source.

   Validate complete record and assertion contracts before constructing the
   aggregate or subject, preserving existing semantic checks and digests.
   Add rehashed unknown-root and invalid-field controls beside the current
   SQL mutation probes; they must refuse as source-invalidated before
   producing an exportable candidate. No schema relaxation is needed.

## Independent probe and AC evidence

The independent harness used the rebuilt Windows binary and new disposable
projects outside the checkout. It did not call the metrics gate or its
aggregation code. Only committed synthetic recorder fixtures and the shipped
privacy evidence-binding helper were reused. Measurements were ingested by
the real `history record` command. All authorizing evidence is synthetic
test metadata. Eight experiment groups completed; the unexpected behaviors
above are recorded observations, not green acceptance.

| Requested probe / AC | Independent observation and source evidence | Assessment |
| --- | --- | --- |
| 1. Authorization; AC2 | Consent, aggregation decision and declassification evidence with declared `expired` each produce blocked preview, `evidence.expired`. Fresh but denied consent produces `evidence.outcome-mismatch`; unverified consent also blocks. After a valid release, all supplied denied/expired/unverified variants return `invalidated`; no supplied authorization returns `unverified`. `metrics_export/privacy.rs:115` delegates to the original validator/evaluator; `mod.rs:462` distinguishes denial from absent authorization. | PASS for declared #119 freshness and current supplied denial. No wall-clock expiry or issuer authenticity service is claimed. |
| 2. Destinations | All six exact specs parse and resolve to the shipped operation/boundary/audience. Workspace, tenant transfer, external transfer, cross-tenant transfer and publish reach ready with appropriate evidence; workspace omits transfer consent. Unknown `arbitrary-cloud` refuses; publish authorization cannot authorize a different destination. | ISSUE 1: repository-store remains blocked despite otherwise sufficient evidence. |
| 3. Source integrity; AC1/AC6 | A loose run-record cannot serve as evaluation input. Wrong raw record/assertion hashes refuse via `Store::get`. After source deletion, an observed loose copy cannot substitute, new preview refuses and status is invalidated. Real count retention on append (`max-records:12`, then a thirteenth run) invalidates the dependent; SQLite inspection reports `kind:aggregate-input,state:invalidated`. Existing gate additionally covers clear/prune/age retention/recovery. | PASS for store-only reads, raw digest integrity and lifecycle; ISSUE 2 for complete source schema admission. |
| 4. Preview; AC2 | Dry-run leaves the SQLite bytes identical and creates no privacy home. Stopping there cancels. Actual preview/written receipts pass Ajv; contradictory dryRun/status flags and written receipt without exportId fail. Caller `--output` is rejected by Clap. CLI/core have no caller-selected package output path. | PASS. |
| 5. Recipe; AC5 | A supplied semantic copy of the embedded definition preserves payload bytes. `minimumSamples:4` refuses as input-invalid. Schema/definition version and exact embedded-file digest are on the wire. Independent six-unit arithmetic: baseline duration sum 210, assisted 126, mean denominators 6; known filesRead zero survives. | PASS. |
| 6. Evaluation seam; AC1/AC4/AC5/AC7 | Root measurements, judgeScores and trial metrics fields each refuse; distinct-run duplicate baseline arm for one unit refuses. approved:false refuses; approved:true alone remains blocked without privacy authorization. Four-unit cohorts withhold samples and sums. Six-unit negative lift is -1; a separate neutral population yields exact zero. Unavailable cached tokens remain unknown. | PASS for typed handoff, uncertainty and retained negative/neutral synthetic outcomes. |
| 7. Storage; AC2/AC3 | Junctions at `.lekalo`, `.lekalo/privacy`, `aggregates`, `decisions`, and `decisions/aggregate` refuse activation. Outside targets receive no export bytes; relocating the history home behind a root junction leaves its database unchanged. Preexisting hard-linked payload is refused without modifying the outside file or creating a manifest. Forward/backslash traversal and absolute input paths refuse. | PASS on Windows. Unix descriptor-relative hardening inspected, not live-qualified here. |
| 8. Privacy; AC3 | Public payload omits all selected run ids, scope token, absolute project path, private profile id and native semantic id canaries, prompt/source keys and source links. The original `redact` runs at `privacy.rs:141`; findings/residuals are empty, with explicit typed-string representation. Security unit tests also reject secret/email/path/URL/tenant input. | PASS for observed projection and residual guard; ISSUE 2 does not demonstrate public canary disclosure. |
| 9. Honesty/import; AC7/AC8 | Docs explicitly label synthetic acceptance, #100 producer ownership and absent measured Framework Lift. Closed JSON schemas, state wrappers, canonical public bytes and exact pins validate as an import candidate. Offline manifest is unverified; local live status is required. | PASS for honest local scope and future import shape; no AIFHub service/remote acceptance proved. |

## Reproduction from the frozen checkout

Save the following code as a temporary `.mjs` outside the checkout. It creates
a new synthetic project, reproduces both findings, and prints their observed
states. It writes no source, contract, fixture or implementation file in the
review worktree. This is intentionally direct database corruption for finding
2, after real recorder ingestion; it is not a supported ingestion route.

```powershell
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
node C:/Users/User/AppData/Local/Temp/lekalo-102-codex-findings-repro-20261005.mjs C:/Users/User/orca/workspaces/lekalo/m7-issue-102
```

```javascript
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {DatabaseSync} from 'node:sqlite';
const repo=resolve(process.argv[2]);
const {authorizingEvidence,refreshEvidenceBindings}=await import(pathToFileURL(join(repo,'scripts/privacy-test-helpers.mjs')));
const require=createRequire(import.meta.url);assert.equal(require('ajv/package.json').version,'8.17.1');
const Ajv=require('ajv/dist/2020').default,ajv=new Ajv({strict:true,allErrors:true,allowUnionTypes:true});
const read=p=>JSON.parse(fs.readFileSync(join(repo,p),'utf8'));
const assertionSchema=ajv.compile(read('contracts/run-assertions.schema.v0.4.0.json'));
const root=fs.realpathSync.native(fs.mkdtempSync(join(tmpdir(),'lekalo-102-findings-repro-')));
const bin=join(repo,'target/debug',process.platform==='win32'?'lekalo.exe':'lekalo');
const env={PATH:process.env.PATH,...(process.platform==='win32'?{SystemRoot:process.env.SystemRoot}:{})};
const sha=b=>'sha256:'+createHash('sha256').update(b).digest('hex');
function cli(args,input){const r=spawnSync(bin,['--json',...args,'--project','.'],{cwd:root,env,input,encoding:'utf8',timeout:20000,maxBuffer:4194304});assert.equal(r.status,0,r.stdout+r.stderr);return JSON.parse(r.stdout);}
const put=(name,v)=>fs.writeFileSync(join(root,name),JSON.stringify(v));
cli(['history','init']);const scope=cli(['history','scope','create']).result.tenantScopeId;
const o=read('tests/fixtures/run-history/valid/observation.json');o.provenance=read('tests/fixtures/run-history/valid/greenfield.json').provenance;
o.status={outcome:'pass',coverageState:'complete'};o.metrics={durationMs:{state:'known',value:10}};
o.assertions={rows:[{assertionId:'review-gate',subjectSemanticId:null,kind:'gate',outcome:'pass',evidenceRef:null}]};
const trials=[];
for(let i=0;i<10;i++){o.runId=(50001+i).toString(16).padStart(32,'0');cli(['history','record','--input','-','--scope',scope],JSON.stringify(o));trials.push({unit:`unit-${i%5+1}`,arm:i<5?'baseline':'lekalo-assisted',runId:o.runId,requiredAssertions:['review-gate']});}
put('selection.json',{schema_version:'lekalo/evaluation-export-input/v0.6.4',identity:'dev.lekalo.evaluation-export-input@0.6.4',protocol:'framework-lift-paired-trials/1',approved:true,trials});
const exportArgs=d=>['metrics','export','--evaluation','selection.json','--scope',scope,'--destination',d];
function authorize(d){
 const a=cli([...exportArgs(d),'--dry-run']).decisionTemplate;a.dataSensitivity=['public'];
 a.derivedArtifact.aggregationDecision.decisionRef=authorizingEvidence(a,'aggregation','a');
 a.derivedArtifact.declassificationDecision={policyRef:a.policyRef,version:'0.2.16',outcome:'approved',removedSensitivities:['internal'],decisionRef:authorizingEvidence(a,'declassification','c')};
 a.provenance.exportTransferConsentRef=authorizingEvidence(a,'exportTransferConsentRef','b');refreshEvidenceBindings(a);put('authorization.json',a);
 return cli([...exportArgs(d),'--dry-run','--authorization','authorization.json']);
}
const repository=authorize('repository-store');assert.equal(repository.status,'blocked');assert.deepEqual(repository.decision.reasonCodes,['repository.same-origin-contradiction']);
console.log(JSON.stringify({finding:1,status:repository.status,reasonCodes:repository.decision.reasonCodes,source:repository.decisionTemplate.source,destination:repository.decisionTemplate.destination}));
const db=new DatabaseSync(join(root,'.lekalo/history/store.sqlite'));
const row=db.prepare('SELECT record_bytes,assertion_set_id FROM runs WHERE run_id=?').get(trials[0].runId);
const old=db.prepare('SELECT bytes FROM assertion_sets WHERE set_id=?').get(row.assertion_set_id);
const assertions=JSON.parse(Buffer.from(old.bytes).toString('utf8'));assert.ok(assertionSchema(assertions));assertions.prompt='private-prompt-canary';assert.equal(assertionSchema(assertions),false);
const bytes=Buffer.from(JSON.stringify(assertions)+'\n');db.prepare('UPDATE assertion_sets SET bytes=?,digest=? WHERE set_id=?').run(bytes,sha(bytes),row.assertion_set_id);
const record=JSON.parse(Buffer.from(row.record_bytes).toString('utf8'));record.assertionsRef.digest=sha(bytes);
const recordBytes=Buffer.from(JSON.stringify(record)+'\n');db.prepare('UPDATE runs SET record_bytes=?,record_digest=? WHERE run_id=?').run(recordBytes,sha(recordBytes),trials[0].runId);db.close();
const preview=authorize('publish');assert.equal(preview.status,'ready');
const written=cli([...exportArgs('publish'),'--confirm',preview.previewDigest,'--authorization','authorization.json']);assert.equal(written.written,true);
console.log(JSON.stringify({finding:2,assertionSchemaValid:false,previewStatus:preview.status,written:written.written,privateCanaryInPublicPayload:written.payload.includes('private-prompt-canary'),root}));
```

Observed on this candidate: finding 1 reports blocked/same-origin-contradiction;
finding 2 reports schemaValid false, preview ready, written true, and no public
canary. Both were reproduced again with this standalone code, independently
of the larger harness. The fresh retained project was
`C:/Users/User/AppData/Local/Temp/lekalo-102-findings-repro-YoT8nG`.
Larger independent experiment artifacts and `results.json` are under
`C:/Users/User/AppData/Local/Temp/lekalo-102-codex-independent-qy9XtC`;
their authoring script is
`C:/Users/User/AppData/Local/Temp/lekalo-102-codex-independent-20261005.mjs`.
All artifacts are synthetic and outside the repository.

## Checks rerun and custody

| Command/check | Result in this review |
| --- | --- |
| `cargo build --locked -p lekalo-cli` | PASS. |
| Exact supplied NODE_PATH, `node scripts/test-metrics-export-contracts.mjs` | PASS, all five families, live binary, 500 original / 508 total rules. No writer/golden-authoring mode. |
| `cargo test --locked -p lekalo-core --lib metrics_export -- --test-threads=1` | PASS, 4 focused security/lifecycle tests. |
| `cargo clippy --locked -p lekalo-core -p lekalo-cli --all-targets -- -D warnings`; `cargo fmt --all --check` | PASS. |
| `node scripts/test-privacy-evaluator-parity.mjs` | PASS, 120 real-binary/reference vectors. |
| `node scripts/test-fixture-provenance.mjs` | PASS, 78 synthetic families, zero evidence-backed families. |
| `node scripts/test-docs-ownership.mjs` | PASS, live help, 347 owned surfaces and 13 P0 owners. |
| `node scripts/check-contract-versions.mjs --base d26991a0` | PASS, product 0.6.4, 122 contract artifacts. |
| Independent larger harness and standalone findings reproduction | Completed with the observed results above; two reproducible defects remain. |
| CI inspection | `ci.yml:204` builds the binary before the five-family live loop at `:295`; exact Ajv 8.17.1 is provisioned at `:195`, Rust job uses Node 24. Existing privacy gates remain. |
| Git scope/diff checks | Only this named review artifact is staged/committed; implementation and Devin report remain unchanged. |

The full core/workspace test suite, Linux/macOS behavior, hosted workflows,
provider evaluation and external importer were not rerun here. Devin's full
core-test statement is not represented as independently reproduced evidence.
The original 500-rule registry digest and committed goldens are verified by
the requested gate. The two source/destination gaps are missing test coverage,
not failures hidden by changing schemas, gates or implementation during review.
