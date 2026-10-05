# Issue #84: Codex independent review round 1

**Verdict: ISSUES. Two reproducible P2 findings prevent acceptance of the core milestone.** Reviewed implementation `82dbeb25cc29b6d09186df5ead221d54b8a1cc07` at branch HEAD `0d334024` on `ichinya/m7-issue-84`, on 2026-10-05. Initial working tree was clean. Implementation comparison base: committed research `60b54b51166240afb17668ba1ca3341a8f4ed231`.

Authority was refreshed with `gh issue view 84 --repo ichinya/lekalo --json title,body,state,url,updatedAt`: [issue #84](https://github.com/ichinya/lekalo/issues/84) remains open; body updated `2026-08-30T10:17:54Z`. Read the research, implementation report and Devin review, then rebuilt and tested the current source independently. The earlier ACCEPT and implementation validation ledger were inputs to challenge, not acceptance evidence.

## Numbered findings

### 1. [P2] Malformed multibyte input crashes document admission instead of returning a diagnostic

**Owner:** `crates/lekalo-core/src/architecture_profile/mod.rs:77`, `:79`; error-span construction in `crates/lekalo-core/src/loader/frontends/json.rs:44`, `:88` and `crates/lekalo-core/src/loader/source.rs:89`. CLI callers include `crates/lekalo-cli/src/architecture_profile.rs:92`, `:122`, `:131`, `:135`, `:139`.

The new decoder invokes the strict JSON frontend before Serde. When an unexpected token starts with a multibyte Unicode scalar, the frontend reports a one-byte span. `LineIndex::position` slices the string at that non-character boundary and panics. Valid UTF-8 containing the single bare token `é` is enough. A UTF-8 BOM before the otherwise valid published profile collection also triggers it. BOM rejection would be acceptable; a process crash is not a structured refusal.

Reproduce from the checkout in PowerShell; writes below are confined to a new temporary directory:

```powershell
$reviewRoot = (Get-Location).Path
$reviewBin = Join-Path $reviewRoot 'target/debug/lekalo.exe'
$reviewTmp = Join-Path ([IO.Path]::GetTempPath()) ('lekalo-84-repro-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path $reviewTmp | Out-Null
$unicodeInput = Join-Path $reviewTmp 'unicode.json'
[IO.File]::WriteAllText($unicodeInput, [string][char]0x00e9, [Text.UTF8Encoding]::new($false))
& $reviewBin --json architecture-profile resolve --architecture-profile ai-strict --architecture-profiles $unicodeInput
$LASTEXITCODE

$bomInput = Join-Path $reviewTmp 'bom-profiles.json'
$profileText = [IO.File]::ReadAllText((Join-Path $reviewRoot 'contracts/architecture-profile.v0.6.5.json'))
[IO.File]::WriteAllText($bomInput, $profileText, [Text.UTF8Encoding]::new($true))
& $reviewBin --json architecture-profile resolve --architecture-profile ai-strict --architecture-profiles $bomInput
$LASTEXITCODE
```

Both commands returned **101**, empty stdout and a Rust panic on stderr:

```text
panicked at crates\lekalo-core\src\loader\source.rs:89:26:
end byte index 1 is not a char boundary
```

Additional independently reproduced cases: `{"x":é}` panics at byte 6; a BOM-prefixed valid architecture lock crashes `assess --architecture-lock`; the bare Unicode token also crashes `lock --architecture-profiles`, `diff --base-profiles`, `assess --baseline`, and `assess --adoption` with its required baseline/date arguments. These all share `decode`, so this is one finding, not separate defects. Temporary filesystem manifests remained unchanged during the four primary crash probes.

**Expected:** malformed JSON returns bounded `architecture-profile.input-invalid`, status `invalid`, exit 1, with valid JSON on the normal failure channel. No panic text should substitute for the contract envelope. Make error positions total at UTF-8 boundaries and cover BOM/unquoted Unicode/Unicode after an ASCII prefix in the live gate. Rejecting all non-ASCII input would incorrectly exclude legal JSON strings. The existing frontend span defect is shared code, but this new externally supplied document surface reaches it directly; the architecture admission claim in `docs/m7/issue-84-implementation.md:17` is therefore incomplete.

### 2. [P2] A purely advisory abstraction rule can become the sole blocking obligation without justification

**Owner:** `crates/lekalo-core/src/architecture_profile/mod.rs:153`, `:157`, `:160`, `:625`, `:797`, `:801`. Published rule: `contracts/architecture-rule-catalog.v0.6.5.json:103`, `:110`, `:112`; generator owner `scripts/gen-architecture-profile-contracts.mjs:33`.

`selection_valid` restricts numeric limits to measured rules, but permits `required: true` on any enabled advisory rule. Assessment turns every required partial row into `evidence-gap`, then lets that row deny `--check`, without checking the rule's blocking basis. A child of `contracted-standard` that changes only `architecture.justified-abstractions.required` to true consequently blocks; it passed both exact Ajv 8.17.1 schema validation and runtime admission. The catalog explicitly says this partial radius proxy cannot decide whether abstraction is premature and “never blocks here.” There is no threshold, calibration reference or semantic justification in the child document.

Reproduce from the checkout using Node; this copies only the synthetic fixture into a new temporary directory:

```powershell
@'
const fs = require('node:fs'), path = require('node:path');
const os = require('node:os'), cp = require('node:child_process');
const root = process.cwd();
const bin = path.join(root, 'target/debug', process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
const project = fs.mkdtempSync(path.join(os.tmpdir(), 'lekalo-84-style-'));
fs.cpSync(path.join(root, 'tests/fixtures/context-budget/planner'), project, {recursive:true});
const resolved = JSON.parse(cp.spawnSync(bin, [
  '--json', 'architecture-profile', 'resolve', '--architecture-profile', 'contracted-standard'
], {encoding:'utf8'}).stdout).architectureProfile;
const document = JSON.parse(fs.readFileSync('contracts/architecture-profile.v0.6.5.json', 'utf8'));
const rule = structuredClone(resolved.rules.find(r => r.id === 'architecture.justified-abstractions'));
rule.required = true;
document.profiles.push({id:'style-required', version:'1',
  extends:{state:'known', value:resolved.profileRef}, rules:[rule]});
document.profiles.sort((a,b) => a.id < b.id ? -1 : 1);
const input = path.join(project, 'style-profiles.json');
fs.writeFileSync(input, JSON.stringify(document));
for (const check of [false,true]) {
  const child = cp.spawnSync(bin, ['--json', 'architecture-profile', 'assess',
    '--module', 'planner', '--architecture-profiles', input,
    '--architecture-profile', 'style-required', ...(check ? ['--check'] : [])
  ], {cwd:project, encoding:'utf8'});
  const envelope = JSON.parse(child.stdout || child.stderr);
  const report = envelope.architectureProfile ?? envelope.payload?.architectureProfile;
  console.log(JSON.stringify({check, exit:child.status, status:envelope.status,
    assessment:report.assessment, gaps:report.rows.filter(r => r.disposition === 'evidence-gap')}));
}
'@ | node
```

Observed:

| Selection | Exit / status | Sole unmet obligation |
| --- | --- | --- |
| Advisory | 0 / valid | `architecture.justified-abstractions`, evidence-gap |
| Same document with `--check` | **3 / denied** | Same advisory rule, evidence-gap |

The row has `severity: info`, `coverage: partial`, `value: {state:known,value:10}`, `limit: {state:unknown}`, empty witnesses, and the catalog rationale saying it never blocks. No other row is a violation or evidence gap. Merely selecting required completeness for a producer permanently classified partial is sufficient; no additional measurement or semantic rationale is admitted. Calling the blocker a gap rather than a style violation preserves uncertainty, but does not satisfy the issue's prohibition on an unjustified subjective rule becoming blocking.

**Expected:** enforce explicit eligibility for blocking evidence obligations on subjective rules, not only eligibility for numeric limits. R14 must remain advisory unless a separately published qualifying semantic/measured basis exists. Preserve intentional behavior-proof gaps such as managed generation using an explicit catalog distinction. Add a negative live probe for `required:true` on R14 and revise the AC6 “Implemented core governance” claim at `docs/m7/issue-84-implementation.md:74` until that path is guarded.

## Independent live verification

All Node contract runs used the user-specified external installation:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/ajv/node_modules'
$env:LEKALO_AJV_NODE_PATH = $env:NODE_PATH
node -p "require('ajv/package.json').version"
# 8.17.1
```

Git subprocess checks used a command-scoped `safe.directory` value for this checkout; no global Git configuration was changed.

| Command | Independently observed result |
| --- | --- |
| `cargo build --locked` | Exit 0, dev binary built/current; product 0.6.5. |
| `node scripts/test-architecture-profile-contracts.mjs` | Exit 0; exact Ajv 8.17.1; 7 families, 9 goldens, 508 entries, 8 diagnostics, 62 live probes, 4 artifact faults, read-only mode. |
| `cargo test --locked -p lekalo-core --lib architecture_profile::tests` | 5 passed, 0 failed, 1053 filtered out. |
| `node scripts/test-diagnostic-contracts.mjs` | Exit 0; Ajv 8.17.1, 500 predecessor entries, 5 envelopes / 5 items. |
| `node scripts/test-provider-contracts.mjs` | Exit 0; 15 checks. |
| `node scripts/test-context-budget-contracts.mjs` | Exit 0; Ajv 8.17.1, 500 predecessor entries, 8 context rules, live binary checked. |
| `node scripts/test-docs-ownership.mjs --static` | Exit 0; 355 surfaces, 13 P0 owners, 33 negative owner controls. |
| `node scripts/test-docs-ownership.mjs` | Exit 0; same census, independently discovered live CLI help. |
| Five `scripts/test-ai-lint-{report,evidence,config,waivers,comparison}-contracts.mjs` gates | Each exit 0; schema 0.6.4, live binary checked. |
| `node scripts/gen-architecture-profile-contracts.mjs --check` | Exit 0; 11 artifacts match, no writes. |
| `node scripts/test-fixture-provenance.mjs` | Exit 0; 78 families, all synthetic. |
| `node scripts/check-contract-versions.mjs` | Exit 0; product 0.6.5, 125 contract artifacts, base HEAD. |
| `cargo fmt --all -- --check`; `git diff --check` | Both exit 0. |

The green architecture gate misses both numbered cases. Full Rust suites, Clippy, MSRV, Linux/macOS execution and hosted CI were not independently rerun here; no earlier result is promoted into this review's ledger.

### Registry union, shadowing and mixed sets

An independent scanner extracted each entry's raw `{...}` UTF-8 slice, retaining whitespace and key order, from both registry files. **500/500 predecessor slices were byte-identical** in `diagnostic-registry.v0.6.5.json`; no predecessor entry disappeared. The only eight additions are `LEK-APR-001` through `LEK-APR-008`, all `architecture-profile.*`; the successor contains 508 unique entries. This is stronger than equality after `JSON.stringify`. The implementation diff contains only added contract paths; no predecessor contract was edited.

A temporary Rust executable linked against the freshly built `lekalo_core` and `serde_json` libraries and exercised public provider normalization and set admission. Its independent output was:

```text
oldPins=500 newPins=8 mixedInvalidAcceptedAndDeduped=2
invalidValidStatusRefused=true unknownVersionsRefused=3 forgedFieldsRefused=6
```

Every one of the 500 old rules normalized with registry version `0.6.4`; every APR rule normalized with `0.6.5`. `normalize::build` chooses the predecessor when an ID exists in both (`crates/lekalo-core/src/diagnostics/normalize.rs:53`, `:67`). **This shadowing is correct for this frozen additive union:** each shared entry is identical, and old producers must preserve their wire pin. A future lifecycle/metadata change to a shared ID would need an explicit reviewed producer/version migration; this fallback should not be mistaken for selecting the newest semantics.

`DiagnosticRegistry::for_version` admits only the two exact closed versions (`crates/lekalo-core/src/diagnostics/registry.rs:162`). `DiagnosticSet::try_from_unsorted` selects a registry for **each** diagnostic and checks active membership and the envelope status (`crates/lekalo-core/src/diagnostics/normalize.rs:182`, `:187`, `:190`). The Rust probe combined `cli.usage@0.6.4` and `architecture-profile.input-invalid@0.6.5` under `invalid`, verified sorting/deduplication, and verified refusal when a valid-only APR diagnostic was mixed under `invalid`, or the invalid pair was presented under `valid`. Unknown/noncanonical versions were refused.

A hostile input cannot select a different registry by adding wire fields: `Diagnostic` has private fields and no general Deserialize (`crates/lekalo-core/src/diagnostics/mod.rs:99`, `:103`), while the provider decoder rejects unknown fields (`crates/lekalo-core/src/diagnostics/provider.rs:138`) and reconstructs identity/severity via `build` (`crates/lekalo-core/src/diagnostics/provider.rs:66`). Supplying `registry_version`, `schema_version`, `severity`, `code`, `message` or `allowed_statuses` to a provider proposal was refused in all six probes. No mixed-version status bypass was found.

### CLI, malformed inputs and filesystem boundaries

An independent temporary fixture harness exercised **82 successful/refusing CLI probes plus four primary crash probes**, without `--no-cache`. For each invocation it compared directory inventory, SHA-256 file contents and symlink targets across the entire temporary fixture/input root. Every manifest remained unchanged; no cache, baseline, lock or report was implicitly persisted. The four additional shared-decoder crash reproductions and the two R14 probes are reported separately above.

| Surface | Live observations |
| --- | --- |
| `catalog` | Valid/0; fifteen owned rules, published coverage and rationale/alternatives. |
| `resolve` | Valid/0; exact ai-strict chain and effective pins. Unknown IDs refused; profile version 2 returned unsupported-version/5. |
| `lock` | Valid/0; dedicated payload, no filesystem write. Exact lock accepted; stale digest/duplicate keys refused. CLI envelope supplied instead of its lock payload refused. |
| `assess` | Legacy default, standard/module and ai-strict/all paths exercised. Calibrated fan-out maximum 0 produced a raw warning violation: advisory/0 without `--check`, denied/3 with it, report retained. Managed missing-generation proof returned advisory/0 or denied/3. |
| Baseline/adoption | Baseline alone still denied; exact reviewed debt was adopting/0 on expiry day and denied/3 the next day. Baseline/ledger bytes stayed unchanged. The live gate additionally reproduced increased debt, forged rows/pins and mandatory errors outside the selected module. |
| `diff` | Same documents produced no policy changes; adding the measured root produced explicit membership/provenance changes. Gate also verified measured-limit strength and unchanged Model/IR pins. |

Deterministic malformed-document mutations covered empty/null/array roots, invalid UTF-8, BOM, comments, duplicate root keys, 1,000 nesting levels, 8 MiB + 1, unknown root/nested members, duplicate/unsorted profiles, incomplete root inventory, foreign catalog, wrong profile version, invalid parent state, disabled mandatory rule, unknown rule and fractional limits. They refused with structured exit 1 or 5 except the Unicode/BOM crashes in finding 1. Missing/conflicting scope arguments also refused.

Hostile profile/module/assignment selectors included empty IDs, `*`, `../planner`, `planner/**`, comma lists, newline injection, Windows drive paths, backslashes, fullwidth Unicode and 257-character IDs. None selected a hidden scope or bypassed project assignment; an explicit profile conflict with the project's assignment was refused. Resolver scope IDs are semantic identities, not filesystem paths (`crates/lekalo-core/src/architecture_profile/mod.rs:137`, `:430`, `:445`).

`--architecture-lock` is an explicitly caller-selected input file, read by `File::open`, followed by file-kind and byte-bound checks (`crates/lekalo-cli/src/architecture_profile.rs:72`, `:75`, `:82`). It is **not confined to the Model directory**: `../lock.json`, an absolute valid lock path, and an actual file symlink to the same valid lock were accepted and still verified against the current snapshot. A traversal/symlink to malformed JSON refused; directory/junction, dangling symlink, missing-parent path and Windows `NUL` refused. All four real link cases ran on this Windows host. These supported explicit reads produced no writes or pin bypass; they must not be represented as a symlink/traversal prohibition. Symlinks were read as links in the filesystem manifest, not recursively traversed by the reviewer. FIFO/device behavior on other OSes was not tested.

### Gate preservation, CI and documentation

Compared the implementation to `60b54b51`: the four requested existing gate scripts and all predecessor `contracts/` paths are unchanged. The workflow diff only adds the new six-line gate step. `build-test` provisions and checks exact Ajv 8.17.1 at `.github/workflows/ci.yml:195`, `:198`, `:200`, builds at `:204`, then invokes the new gate with the external dependency path at `:295`, `:297`, `:300`. No later-job binary dependency or removed predecessor gate was found.

The adjacent AI-lint gate helper changed to select explicit product-qualified 0.6.5 snapshots. Inspected the diff at `scripts/lib/ai-lint-contract-gate.mjs:13`, `:99`, `:284`: original goldens remain present and schema-validated, current outputs still require exact golden equality, the binary product must match, and previous negative assertions remain. All five live gates passed independently. This change is not a removal of an existing schema/negative gate.

New command and seven family ownership entries exist at `scripts/lib/docs-maintenance.mjs:47`, `:61`; the generated command index includes all five subcommands at `docs/cli.md:1144`. Both ownership modes passed against product 0.6.5. The metadata contains 160 command records, not the 155 stated in the Devin prose; this count discrepancy is not a runtime blocker.

## Acceptance criterion audit

| Live issue AC | Independent evidence and honest boundary |
| --- | --- |
| AC1 published profile schema/catalog | Seven schema families, fifteen catalog rows, four profiles, nine goldens, generator parity and exact-Ajv live gate are delivered. Adapter catalog admission is explicitly deferred. Malformed-input robustness is **not accepted** because of finding 1. |
| AC2 incremental legacy/contracted use | Exact project/module assignments, legacy default, scope conflict refusals and measured debt/expiry are live core behavior. The gate proved different module selections and preserved mandatory semantic errors. Actual observed/native promotion and a mixed consumer migration were not demonstrated; the report properly limits this to declared core evidence. |
| AC3 Laravel strict maps to Mago without PHP core coupling | **Deferred, not completed.** Existing mapping inspected at `adapters/php-laravel/src/strict-profile.php:23`, `:105`, `:165`, with Mago 1.0.0 pin at `adapters/php-laravel/mago-toolchain.lock.json:9`. The core adds no adapter receipt/catalog join and runs no Mago. A neutral catalog alone does not satisfy this AC; the implementation report admits that. |
| AC4 rationale and explicit alternative | Catalog and live measured violation/adopted/evidence-gap rows preserve both, along with coverage, raw values and witness IDs. Delivered for core report rows; no qualified native source explanation is claimed. |
| AC5 lock/diff/evidence visibility | Dedicated lock is verified against a recomputed complete snapshot (`crates/lekalo-core/src/architecture_profile/mod.rs:272`); explicit diff (`:279`) and reports carry policy/document pins independently of Model/IR. Sidecar behavior is real. Predecessor lock/semantic-diff envelope integration remains deferred and is disclosed; this is milestone evidence, not full integration acceptance. |
| AC6 subjective style cannot block without semantic/measured reason | Numeric-limit eligibility and equality/one-over controls are real. **Claim overstates delivery:** required-evidence eligibility remains unguarded for R14 and makes a purely advisory rule block, finding 2. Naming the denial an evidence gap does not remove the blocker. |
| AC7 A/B tests AI task outcomes | **Not executed.** Structural token/fan-out/radius/scenario projections are available; no causal AI comprehension/change-safety outcome exists. The report correctly avoids an A/B acceptance claim. |

No fabricated adapter or A/B outcome was found. Acceptance is limited to the user's measurable-core milestone; it does not close the full live issue. Even within that milestone, the two numbered defects require correction and independent rerun. Reproductions and scratch Rust/Node harnesses stayed outside the checkout. This review changes and commits only `docs/m7/issue-84-review1-codex.md`; implementation files remain untouched, and no push or other-worktree mutation is authorized or performed.
