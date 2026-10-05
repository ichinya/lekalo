# Issue #100: Codex independent review round 1

**Verdict: ISSUES — 3 reproducible findings (1 P1, 2 P2).** The supplied
Devin ACCEPT is challenged by live counterexamples to token-budget acceptance,
infrastructure classification, and schema/runtime integer parity. The required
49-check gate passes; these counterexamples are outside its coverage.

Reviewed on 2026-10-05 in `ichinya/m7-issue-100`, worktree
`C:/Users/User/orca/workspaces/lekalo/m7-issue-100`. Implementation:
`7cf81e235fad1db51b23552d118625c5b10b6173`; initial HEAD / Devin review:
`ba9c094c56229905f6db2c0a7be08d0e79b3ddbd`; implementation predecessor:
`ea903b90b9d4ac1764f975ea4a717044931cbc25`.

Read the research, implementation, Devin review, production core/CLI/executor,
five schemas, fixtures, provenance, documentation ownership and CI wiring.
Fetched [live issue #100](https://github.com/ichinya/lekalo/issues/100) with
`gh issue view 100 --repo ichinya/lekalo` and read its profile-separation comment
with `--comments`. It remains OPEN with nine unchecked ACs. This review evaluates
the authorized deterministic recording/admission/replay scope; actual external
agent campaigns remain pending and are not substituted by fixture replay.

## Numbered findings

### 1. P1 — inconsistent token totals can produce verified, first-pass success above budget

Source: `crates/lekalo-core/src/framework_lift/mod.rs:174` validates source
coverage and several subset relations, including cached-input/input and
reasoning/output, but never reconciles `totalTokens` with input/output.
`mod.rs:296` applies `maxTokens` only to the supplied `totalTokens` leaf.

Reproduction: copy `framework-lift-arm/golden/b-neutral.json`; change only
`metrics.inputTokens.value` from 40 to **10001**. Retain known `outputTokens: 10`,
`totalTokens: 50`, the versioned metric sources and all task/campaign pins.
The frozen task/campaign limit is **10000**. Exact Ajv accepts the arm;
`record-arm` exits 0; `compare` exits 0 and emits:

```json
{"status":"success","verifiedSuccess":true,"firstPassSuccess":true}
```

B's summary counts one success and returns known cost per success `100 / 1`.
A second probe with input/output 40/10 and known **totalTokens: 0** also admits
and succeeds. The `framework-lift-metrics-1` recipe documents provider totals
with cached input and reasoning as subsets, so total usage cannot be smaller
than a known input or output component. Source authenticity is an external
seam; detecting a contradiction between the admitted numeric leaves is a
deterministic responsibility already claimed by this implementation.

Required correction: reconcile totals under the frozen normalizer, without
adding cached input or reasoning twice. Reject inconsistent known totals; use
known component lower bounds when deciding whether a token cap was exceeded.
Preserve unknown/unsupported states when the recipe cannot establish a total.
Add live counterexamples for total zero, total below a component, and input or
output above the cap. This affects AC3 and the within-budget success guarantee
under AC6; valid source pins alone currently do not protect that guarantee.

### 2. P2 — an explicit infrastructure assertion becomes unsupported in the result

Source: `contracts/framework-lift-arm.schema.v0.6.4.json:211` admits the assertion
outcome `infrastructure`. `mod.rs:307` recognizes assertion `fail`, while
`mod.rs:313` classifies infrastructure only from the separate `failures` array.
An infrastructure assertion then falls through to the required-pass check at
`mod.rs:332`, producing `unsupported`.

Reproduction: copy the same B-neutral arm; set
`assertions[0].outcome = "infrastructure"`, leaving `failures: []`. Schema
validation and `record-arm` succeed, preserving that assertion outcome.
`compare` nevertheless reports the slot as
`status: "unsupported", verifiedSuccess: false`.

The assertion reports a known verifier infrastructure outcome, rather than an
unavailable assertion capability. The primary result classification loses that
distinction unless the collector duplicates it into another field. Admission
does not require that duplicate declaration. This does not create a false pass,
but it makes infrastructure attrition accounting depend on an undocumented
redundancy and contradicts the promised separate infrastructure classification.

Required correction: derive the primary failure class from admitted assertion
source outcomes, preserving custody/task precedence, or refuse inconsistent
failure declarations with an explicit closed rule. Add a live infrastructure
assertion vector without a redundant failure sidecar and a hard-fail plus
infrastructure precedence vector. This affects the issue's failure-classification
requirement and the honest attrition disposition supporting AC4.

### 3. P2 — all five families reject JSON Schema-valid integer encodings

Source: `crates/lekalo-core/src/framework_lift/schema.rs:33` decodes decimal or
exponent numbers as floating JSON numbers; `schema.rs:201` requires `as_i64()`
for every schema `integer`. Published schemas describe numeric integer values,
not a restriction to decimal-free JSON lexemes.

Exact strict Ajv 8.17.1 accepts each following document, but production
`evaluation validate` exits 1 with `evaluation.protocol-invalid`,
`detail: closed-contract`:

| Family | Golden member | Schema-valid replacement |
| --- | --- | --- |
| baseline | `approval.revision: 1` | `1.0`, `1e0` |
| task | `taskVersion: 1` | `1.0`, `1e0` |
| campaign | `randomizationSeed: 1` | `1.0`, `1e0` |
| arm | `attempt: 0` | `0.0`, `0e0` |
| result | first row's `attempt: 0` | `0.0`, `0e0` |

These substitutions change neither numeric value nor the JavaScript canonical
digest. A future schema-valid collector/importer can therefore fail admission
despite retaining the same protocol values. This is an interoperability defect,
not a closure bypass.

Required correction: admit and canonicalize exactly integral numbers within the
existing bounds consistently with the schema oracle; retain rejection of
fractional, negative, non-finite and oversized values. Test byte-level integer
encodings, rather than only objects serialized by `JSON.stringify`. This affects
the closed-schema seam and AC7 importability.

## Reproduction commands

Run from the reviewed worktree after the requested build. This self-contained
PowerShell/Node reproduction writes only test-owned temporary files and prints
the counterexamples for findings 1–3. All inputs remain recorded simulations.

```powershell
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
@'
const fs = require('node:fs'), path = require('node:path');
const os = require('node:os'), assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const Ajv = require('ajv/dist/2020').default;
assert.equal(require('ajv/package.json').version, '8.17.1');
const ajv = new Ajv({strict:true, allErrors:true});
const tempRoot = path.resolve(os.tmpdir());
const tmp = fs.mkdtempSync(path.join(tempRoot, 'lekalo-100-review-repro-'));
const bin = path.resolve('target/debug/lekalo.exe');
const fixture = (f,n) => `tests/fixtures/framework-lift-${f}/golden/${n}.json`;
const read = p => JSON.parse(fs.readFileSync(p, 'utf8'));
const schemas = new Map();
const schemaFor = f => {
  if (!schemas.has(f)) schemas.set(f,
    ajv.compile(read(`contracts/framework-lift-${f}.schema.v0.6.4.json`)));
  return schemas.get(f);
};
const common = ['--baseline',fixture('baseline','approved'),
  '--task',fixture('task','priority'),'--campaign',fixture('campaign','scheduled')];
let serial = 0;
const write = raw => {
  const p = path.join(tmp, `${serial++}.json`);
  fs.writeFileSync(p, typeof raw === 'string' ? raw : JSON.stringify(raw));
  return p;
};
const run = args => {
  const p = spawnSync(bin, ['--json','evaluation',...args],
    {encoding:'utf8',timeout:30000,maxBuffer:8*1024*1024});
  if (p.error) throw p.error;
  return {exit:p.status, value:JSON.parse(p.status === 0 ? p.stdout : p.stderr)};
};
try {
  for (const [label,edit] of [
    ['above-budget',a => a.metrics.inputTokens.value = 10001],
    ['zero-total',a => a.metrics.totalTokens.value = 0],
    ['infra-assertion',a => a.assertions[0].outcome = 'infrastructure']
  ]) {
    const a = read(fixture('arm','b-neutral')); edit(a); const input = write(a);
    const schema = schemaFor('arm');
    const admitted = run(['record-arm',...common,'--input',input]);
    const result = run(['compare',...common,'--arm',input,
      '--consumer-alias','review-consumer-one']);
    const row = result.value.rows.find(r => r.slot.arm === 'B' && r.slot.pairId === 'pair-two');
    console.log({label,ajv:schema(a),recordExit:admitted.exit,compareExit:result.exit,
      status:row.status,verifiedSuccess:row.verifiedSuccess,firstPassSuccess:row.firstPassSuccess});
  }
  for (const [f,n,key,num] of [
    ['baseline','approved','revision',1], ['task','priority','taskVersion',1],
    ['campaign','scheduled','randomizationSeed',1], ['arm','b-neutral','attempt',0],
    ['result','negative','attempt',0]
  ]) {
    const raw = fs.readFileSync(fixture(f,n),'utf8');
    const schema = schemaFor(f);
    for (const lexeme of [`${num}.0`,`${num}e0`]) {
      const changed = raw.replace(`"${key}":${num}`, `"${key}":${lexeme}`);
      assert.notEqual(changed,raw);
      const r = run(['validate','--family',f,'--input',write(changed)]);
      console.log({family:f,lexeme,ajv:schema(JSON.parse(changed)),cliExit:r.exit,
        diagnostic:r.value.diagnostics?.[0]?.id});
    }
  }
} finally {
  assert.equal(path.dirname(path.resolve(tmp)), tempRoot);
  assert.ok(path.basename(tmp).startsWith('lekalo-100-review-repro-'));
  fs.rmSync(tmp,{recursive:true,force:true});
}
'@ | node
```

## Independent probe disposition

The primary disposable driver made 801 direct CLI calls, including 518 recursive
closed-shape mutations. An additional driver made 75 validation calls plus two
infrastructure reproduction calls. Four separate baseline/custody CLI checks
covered file and approval drift and a Windows junction. Executor callbacks
received synthetic requests only. These counts are test invocations, not A/B
agent runs. Temporary drivers and mutated fixtures lived outside the checkout.

| Requested probe | Independent result / evidence |
| --- | --- |
| 1. Arm/campaign pins | **Pass.** Reapproved changes to model, harness, executor, provider, runtime, sampling seed and optional architecture-profile ref reject the old arm. Rebinding only `campaignRef` still rejects stale `profileDigest`. Application/slot/network pins and A/B exposure deltas reject with `evaluation.arm-incomparable`. |
| 2. Missing scheduled arm | **Pass.** Comparing no records emits all four `not-started` rows; both arms retain scheduled denominator 2, started 0, paired incomplete 2, attrition bounds −100/+100 percentage points. The committed missing A partner remains visible. |
| 3. Hard fail plus judge 100 | **Pass.** Hard assertion failure remains `task`, including alongside provider/infrastructure/unsupported/interrupted declarations; custody-security retains higher precedence. Judge score 100 never grants success. |
| 4. Private non-local policy | **Pass.** Independently rebound brownfield baseline/task/campaign with `public-synthetic` refuses `private-egress-denied`; executor callback count stays 0. A separately approved local-only policy admits the synthetic callback. This checks declared policy, not external egress containment. |
| 5. Metric sources/budget | **Partial; finding 1.** Missing source, missing component digest/revision, duplicate source and source attached to unknown metric reject. Each of totalTokens/toolCalls/durationMs/fixCycles in unknown/unsupported/withheld state prevents success. Known excess totalTokens/durationMs is task failure. Internally contradictory known token totals still succeed. |
| 6. Duplicate/superseded records and pairing | **Pass for tested replay accounting.** Duplicate run ID, duplicate slot/attempt with another ID, absent attempt 0, retry after success and retry after task failure refuse. Reversing all input records yields the same result. Provider attempt 0 plus successful attempt 1 retains A cost 100+200, two attempts, first-pass false. Paired counts are bothSuccess 0 / aOnly 1 / bOnly 0 / neither 0 / incomplete 1; lift is 0, attrition bounds −50/0 pp. Finding 2 concerns classification of infrastructure evidence. |
| 7. Closed decode and hostile refs/paths | **Closure pass; parity defect in finding 3.** At every reachable golden object, both an unknown field and a deleted required field were refused (518 mutations). All five families refuse duplicate root/Unicode-escaped keys, trailing documents, null/array roots, deep nesting, wrong versions, prototype-like unknown keys, wrong count types/bounds, oversized input and traversal/absolute URL/drive/backslash/NUL/newline reference IDs. Baselines additionally reject dot/empty segments, reserved device names, trailing dot/space and encoded traversal; junction children refuse `reparse-point`. Ten valid integer lexeme substitutions fail production validation. |
| 8. Executor neutrality | **Pass for the documented seam.** No callback means `explicit-executor-required`. The same explicit callback receives equal request/profile/application/campaign/task/baseline pins for A/B; the declared Lekalo/semantic policy is the intentional delta. Source uses explicit binary argv, no shell flag, provider import, endpoint, cloud route or implicit model. Actual tool views, fresh candidates and containment remain qualified-host obligations. |
| 9. Evidence honesty | **Pass.** All four committed arm goldens say `origin: recorded-simulation`; result says `evidenceStatus: recorded-simulation`, `local-private`, `tested-task-profile-only`. Fixture provenance is synthetic. Changing an input origin to `recorded-external` produces only `recorded-unverified`. Required gate and this review have `externalAgentRuns: 0`. |

Additional custody checks refused changed listed bytes with `file-bytes`, changed
approved content with `approval-digest`, and a valid pinned path through a
Windows junction with `reparse-point`. Original tracked fixture bytes were
unchanged. Approval authenticity, exhaustive workspace inventory, resolving
external receipts and detecting deliberately withheld attempts remain explicitly
documented external obligations; this review does not treat them as delivered
execution guarantees.

## AC and delivery boundary

| Live issue AC | Independent disposition |
| --- | --- |
| AC1: three greenfield tasks with A/B results | **Pending real campaigns**, as authorized. Seven task classes and a paired protocol exist; one simulated priority vector is not three executed planner tasks. |
| AC2: one observed brownfield context/impact result | **Pending local private campaign.** Pilot distinction/private policy seam exists; no private consumer or actual local model was run. |
| AC3: success/tokens/files/iterations/cost per success | Recording and derivation exist, failed retry cost is retained, missing costs stay unknown; **finding 1** blocks acceptance of the metric-consistency guarantee. Actual telemetry remains external. |
| AC4: honest negative/neutral result | Neutral replay, losing hard failure and missing slots are retained. **Finding 2** identifies an infrastructure classification loss; complete external event custody remains necessary. |
| AC5: exact model/harness/profile provenance | Tested digest joins reject changed shared pins. Truth of the pinned artifacts requires external resolution. #84 remains an optional opaque seam; no parallel contract names are assumed or verified here. |
| AC6: subjective score cannot hide hard regression | Hard fail plus judge 100 passes the negative control. **Finding 1** separately breaks the claimed within-budget predicate used for success. |
| AC7: future AIFHub importability | Versioned closed result/arm/campaign schemas exist; receiver integration remains pending. **Finding 3** blocks schema/runtime numeric agreement for importers. Standalone result validation does not authenticate referenced evidence; imports must resolve pins and replay comparison. |
| AC8: no universal superiority claim | Fixed tested-task-profile claim, scheduled counts, marginal Wilson intervals and paired attrition are present. These intervals are not a paired lift CI; no universal superiority is claimed. |
| AC9: anonymized public evidence | Opaque caller alias/consumer role and local-private disposition exist. Public export and alias-to-consumer custody remain external; no private code/prompt, repository identity or raw private upload was used in this review. |

## Gates and scope

Windows local environment: Node 24.13.0, Cargo/Rust 1.98.0, exact Ajv 8.17.1.
Independent commands and results:

| Command | Result |
| --- | --- |
| `gh issue view 100 --repo ichinya/lekalo` and `--comments` | Read live requirements and separate-pilot comment. |
| `cargo build --locked -p lekalo-cli` | Passed before binary-dependent probes. |
| `NODE_PATH=C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules node scripts/test-framework-lift-contracts.mjs` (PowerShell environment equivalent) | Passed: 49 checks, all five live families, all six negative codes, simulation origin, externalAgentRuns 0. |
| `cargo test --locked -p lekalo-core --lib -j 1` | 1051 passed, 0 failed, 2 ignored. Used `CARGO_PROFILE_TEST_DEBUG=0` and process-scoped Git safe.directory for this exact checkout. |
| `cargo clippy --locked -p lekalo-cli --all-targets -j 1 -- -D warnings` | Passed. |
| `cargo fmt --all -- --check` | Passed. |
| `node scripts/test-fixture-provenance.mjs` | Passed: 82 families, all synthetic. |
| `node scripts/test-docs-ownership.mjs` and `--static` | Passed: 348 surfaces, 13 P0 page owners. |
| `node scripts/check-contract-versions.mjs --base ea903b90` | Passed: product 0.6.4, 121 artifacts. |
| `lekalo.exe evaluation --help` | Honest four-command offline surface. |

CI source inspection confirms the Framework Lift loop at
`.github/workflows/ci.yml:289` runs each family after the same job's locked cargo
build at line 204 and uses externally provisioned exact Ajv 8.17.1. The gate's
predecessor digest check confirms preservation of the 500 existing registry
entries plus six evaluation entries. Command and all five family owners are
registered. These are local/source checks, not hosted CI acceptance. Devin's
parallel #84 merge-order claim was not independently exercised; no other
worktree was accessed or changed.

The full workspace suite was not rerun; its three reportedly inherited golden
failures remain implementation-report evidence, not a fresh confirmation from
this review. No generator, implementation fix, contract weakening, provider
request, agent run or push was performed. Initial worktree/index were clean.
Only `docs/m7/issue-100-review1-codex.md` is changed and included in the authorized
review commit.

The self-contained reproduction printed above was also extracted from this
document and rerun successfully. Its own temporary directory was removed by its
verified cleanup. The separate review-driver scratch directory
`C:/Users/User/AppData/Local/Temp/lekalo-issue-100-codex-2ca143f947014b5ba6c21ad8bdbe8215`
remains outside the checkout: automatic command review rejected its cleanup with
the stated reason `blocked by policy`. The rejected command did not stage or
change repository files; staging and commit are performed separately.
