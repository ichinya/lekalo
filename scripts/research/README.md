# Framework Lift research helpers (#167)

## Public-fixture research recipe

`public-fixture-research/1` is a separate descriptive research format. Its
[reviewed campaign allowlist](public-fixture-research.recipe.v1.json) binds exact
original/admission archive manifests and public task/oracle/native pins. There is
no caller-supplied `public` flag. Adding a campaign requires reviewing its public
fixture and the selected output fields; arbitrary private campaigns refuse.
The recipe neither changes #102's minimum sample/rare-bucket rules nor issues
classification, declassification or consent evidence. Raw account exports and
the conservative internal history records remain private.

```text
node scripts/research/framework-lift-research-report.mjs ORIGINAL_ARCHIVE ORIGINAL_MANIFEST_SHA256 ADMISSION_ARCHIVE ADMISSION_MANIFEST_SHA256 NEW_OUTPUT_JSON
node --test scripts/research/test-framework-lift-research-report.mjs
```

The helper rehashes both archives, checks admitted arm/result joins and original
usage parity, then emits an allowlisted report with all attempts, per-profile
paired deltas, and exact rational means/relative changes. Outputs must be new and
outside both archives, including through aliases. The
[current research report](results/2026-10-27-public-fixture-research.json)
retains all eight attempts and the incomplete Devin B1. Total-token B-minus-A
changes are +17.1% for Devin and +30.5% for Codex; two pairs per CLI on one small
fixture establish no general savings or causal attribution. Post-run 1.1
admission is recorded-unverified; the historical 0.6.4 NOTRUN report stays intact.

## Experimental compact context

`prepare({compactContext: true, ...})` selects an experimental native-only entry
instruction and grouped pointer identities. It leaves the full installed product
instruction available, requiring it before Model/binding/integration operations.
The experiment keeps the Model unchanged, reports stale semantics, and preserves
the full CLI capsule body, gaps, incompleteness and pointer evidence-state counts.
Exact native names remain available through the owning bindings command.
The default still reads the full installed instruction.

The [offline preflight](results/2026-10-27-compact-context-final.json)
measures 11,905 bytes for the full instruction plus capsule and 4,598 bytes for
the compact variant (about 61% less). These are UTF-8 delivery bytes, not tokens,
paid usage, or measured token savings. Both variants suppress repeated unchanged
capsules; the public native baseline/reference controls and source preservation
passed without inference. A fresh pinned campaign must measure actual usage;
never reuse old workspaces or pool changed harnesses into the original campaign.
Reproduce without agent CLIs or dependency installation:

```text
node scripts/research/framework-lift-context-preflight.mjs NEW_OUTPUT_JSON
```

### Actual compact-context campaign (2026-10-27)

The [preregistration](https://github.com/ichinya/lekalo/issues/167#issuecomment-6439183338)
froze eight new independent slots at `afeb423187ba05c6353a6bb692a3f6c824ac9a76`.
All eight executor workflows finished. Six candidates passed all 20 holdout
assertions; Codex B2 passed 19/20 and A2 passed 17/20. Both stay in the denominator;
there were no reruns. The [new descriptive report](results/2026-10-27-compact-context-research.json)
uses the separately pinned `public-fixture-research/2` recipe and joins all eight
arms to their [post-run 1.1 admission](results/2026-10-27-compact-admission-v11.json).
The v1 recipe and historical reports remain unchanged. For this report, append
`2` after the new output path in the research-report command above.

| Profile | A mean total tokens | B mean total tokens | B total change | B uncached input change | Successes A / B |
| --- | ---: | ---: | ---: | ---: | --- |
| Devin / swe-2-high | 190,865.5 | 324,286 | +69.9% | +2.9% | 2/2 / 2/2 |
| Codex / gpt-6-luna medium | 181,076.5 | 223,373 | +23.4% | +15.9% | 1/2 / 1/2 |

Total usage was 1,839,202 tokens, including 1,514,688 cached input tokens as a
subset; monetary cost remains unknown. No token savings were demonstrated.
Devin read the full installed instruction in both B runs despite the compact
initial entrypoint. Its B1 used 13 measured agent steps versus A1's seven and
included a 14,312-output-token step. Codex's B runs did not show that full-prompt
read, yet still used more tokens. Verified source-file lower bounds stayed at
nine per Devin run and nine versus 8.5 on average for Codex A versus B. The
observed workflow therefore added semantic context while reading nearly the
same small implementation. These facts identify overhead and failed progressive
disclosure; they do not isolate a causal token share or provider-side variance.

The reader also refused attempts to include tests/package metadata in its
`src/**/*.ts`-only interface (both Devin B runs and A2). The next instruction
revision should state that boundary explicitly. A future compact managed prompt
should place the short instruction in the file agents actually read, with full
reference material disclosed when needed. Neither follow-up is claimed measured
by this campaign. There are no further automatic inference runs.

Raw tool receipts and before/after candidates remain local in the immutable
622-file campaign bundle (manifest
`c368daf597aee29d47353e19875059f8399f563a3ca11ae6831dd6fa6f59ecad`),
with a separate 19-file admitted bundle (manifest
`2e99ba296d694729aa5c5d20b58fecb55ffb859e1d4f6437316ba243fe3ae4dd`).
All eight supplementary scan/reconfirmation cycles preserved native source and
were deterministic. This is native evidence maintenance outside agent timing,
not semantic Model conformance or human review. Private export rules stay intact.

## Actual history / privacy preview

`framework-lift-history-preview.mjs` verifies both immutable evidence manifests,
projects the representable metrics of all eight admitted arms into a fresh #121
history store, checks exact input/output/total parity and executor completion,
then calls the existing #102 `metrics export --dry-run` twice. No source archive,
history schema, aggregation recipe, consent evidence or minimum sample is changed.

```text
node scripts/research/framework-lift-history-preview.mjs ADMISSION_ARCHIVE ADMISSION_MANIFEST_SHA256 ORIGINAL_ARCHIVE NEW_PROJECT
node --test scripts/research/test-framework-lift-history-preview.mjs
```

This is an explicitly limited compatibility projection, not full 1.1.0 history
ingestion. `source-links.local.json` binds each generated history record to its
full admitted parent. Cached input, fix cycles, source-read lower bounds and raw
evidence refs remain in that parent. History 0.4 treats reasoning as a disjoint
addition, while this provider reports a subset of output: its legacy reasoning
leaf is therefore **unsupported**, and original input/output/total stay unchanged.
The script never invents a shared Git revision, a model revision or a price.

The [actual preview audit](results/2026-10-26-history-privacy-preview.json)
retains seven completed workflows and 1,648,102
tokens in local history. Four arm/profile cohorts have only two trials each,
below the unchanged minimum five, so all cohort numeric values are withheld.
Source records retain the conservative internal label. The preview is **blocked**
pending genuine aggregation/declassification authorization; its template has no
authorizing evidence. No confirmation or package publication occurred. A larger
sample alone would not guarantee disclosure: small outcome buckets are also
suppressed by the existing recipe. This boundary is not bypassed by pooling CLI
profiles or duplicating the same recorded attempts.
An invalidation rehearsal deletes a source only in a disposable history copy;
the next preview refuses with `metrics-export.source-invalidated`. All eight
original projected records remain intact. Rehearsal copies are not trial units.

## Versioned post-run admission

The 1.1.0 evaluation successor can now represent the original read-once campaign
without inventing sampling defaults, prices, a token cap or a common Git revision.
The explicit importer verifies the archived manifest and usage, creates one
campaign per CLI profile and admits all eight original attempts. Comparison
preserves seven completed workflows and 1,648,102 tokens, including Devin B1's
failure. No new inference runs occur.

The allowlisted [admission audit report](results/2026-10-26-admission-v11.json)
pins the translator and actual CLI, retains both comparison digests and confirms
usage/outcome parity. Full result/arm documents and CLI operation receipts remain
in the local evidence bundle; the report is a research disclosure, not a product
public-export receipt.

```text
node scripts/research/framework-lift-import.mjs ARCHIVE_ROOT MANIFEST_SHA256 NEW_OUTPUT_DIRECTORY
```

The output directory must be new and outside the source archive. Full normalized
records and CLI operation receipts stay local-private. Provenance binds the
original campaign, manifest and translator; the import is explicitly post-run.
Both results remain `recorded-unverified`, with partial profiles and unknown
prices/telemetry/server revisions. Source read lower bounds have separate fields;
exact all-native `filesRead` stays unknown. This is admission and arithmetic
parity, not provider authentication or product privacy-export qualification.

## Original disclosure and 0.6.4 admission gap

`results/2026-10-26-read-once.json` is an allowlisted research disclosure of the
eight actual attempts discussed in issue #167. All eight native candidates pass
the 20-case holdout; seven finish the executor workflow. Devin B1 remains in the
denominator after a noninteractive confirmation refusal. Total usage is 1,648,102
tokens, including 1,369,728 cached input tokens as a subset, with money unknown.
Mean B total tokens exceed A by 17.1% for Devin and 30.5% for Codex. There is no
general savings claim. The sample is two pairs per CLI on one small public task;
ordering, cache behavior and an incomplete attempt limit interpretation.

The read-only exporter verifies all 625 files against an explicitly supplied
archive manifest pin, independently sums Devin step metrics / Codex turn usage,
checks them against collector and summary values, and checks native oracle
receipts. It emits only selected counts, public profile identifiers and hashes;
raw account exports and workstation paths stay local. It never rewrites an
archive or an existing output. Use an output path that does not yet exist:

```text
node scripts/research/framework-lift-public-result.mjs ARCHIVE_ROOT MANIFEST_SHA256 NEW_OUTPUT.json
node --test scripts/research/test-framework-lift-public-result.mjs
```

This is not a product privacy-export receipt or provider-authenticated execution.
The raw archive remains local and is needed to replay its integrity audit. Its
manifest pin and source receipt hashes are in the disclosure. Unknown data is
not inferred from zero charges or from missing events.

`results/2026-10-26-admission-probe.json` records **synthetic contract probes**
against the actual candidate CLI, not imported empirical arms. A valid 0.6.4
campaign passes; unknown temperature/top-p/max-output/pricing and an unbounded
token limit each refuse with `evaluation.protocol-invalid / closed-contract`:

```text
node scripts/research/framework-lift-admission-probe.mjs
```

At the original disclosure, actual `record-arm/compare` were **NOTRUN**. The 0.6.4 protocol requires
numeric sampling defaults and finite limits plus a tariff pin. The actual CLI
defaults and prices are not fully exposed, and the authorized budget is uncapped.
Also, agent wall time excludes independent grading, whereas the protocol's
measurement window ends at acceptance. Filling these gaps with guessed values
would corrupt custody. A versioned import must preserve unknowns, explicit
unbounded policy and the original measurement window, and mark translation as
post-run. Published 0.6.4 simulation records retain their original status.
The import also needs an explicit public synthetic observed profile: the old
`brownfield-observed` pilot enforces `local-only` egress, while the measured task
is public synthetic and the agents use hosted providers. Relabelling it as
`greenfield-contracted` would misstate the actual adoption workflow. Keep the
private brownfield restriction intact when introducing the new profile.
The 1.1.0 import above addresses representation and admission. Issue #167 stays
open pending the remaining evidence qualification and product privacy-export work.

## Source and capsule reuse for future campaigns

The shared `framework-lift-workspace.mjs` reader is copied unchanged into
both A and B. It records emitted coverage by agent context, path, exact
source digest and line range in `.benchmark/native-reads.json`. Repeated
unchanged ranges return metadata; overlapping requests emit only unseen
lines. Search and read share coverage. Changed versions remain readable.

```text
node tools/workspace.mjs --help
node tools/workspace.mjs read src/domain/task.ts --lines 1:4
node tools/workspace.mjs search priority
node tools/workspace.mjs read src/domain/task.ts --lines 1:4 --replay "previous output was truncated"
node tools/workspace.mjs stats
```

`new-session --reason TEXT` is for a genuinely replaced/compacted agent
context, not a routine reset. Collector state is not authored project
content; only its exact path is excluded from end-state modification
checks. Raw tool deliveries, not editable ledger claims, remain the audit
source of truth. An emission does not prove that a truncated response
reached the agent. Native Read/Edit/diff outputs require independent audit;
this helper does not intercept those tools. Instructions prefer an editor's
required Read as the first source read, avoiding a helper + Read duplicate.

The semantic helper now requests one bounded capsule and groups pointers
by file. It preserves gaps, incomplete status and warnings. Impact is a
separate request when extra risks/gates are needed. The same context ledger
suppresses unchanged capsule text; reasoned replay remains available.
Neither cache eliminates the CLI's repeated transmission of conversation
history to its model. Token savings require a new preregistered measurement.

Offline behavior gate:

```text
node --test scripts/research/test-framework-lift-workspace.mjs
```

Earlier frozen campaign snapshots and receipts are unchanged. Prepare and
freeze a new campaign after helper/instruction/binary changes; never run a
previous frozen campaign with updated tooling.

## Planner after the native scan changes

`framework-lift-planner.mjs` prepares the original public priority planner
and 20-case oracle against the 1.1.0 development candidate. It performs no
inference. `framework-lift-runner.mjs --run ROOT` performs the explicitly
authorized real CLI runs and consumes account usage. The planner builder
retains the baseline's strict authorization refusal and the unsupported
canonical semantic/native ID join; this is a bounded assisted workflow,
not qualified complete semantic adoption. Native source bytes are checked
against the committed first-pilot inventory, and cold CLI time excludes unknown
Model authoring. Public task, oracle and partial authored Model inputs live in
`tests/fixtures/public-typescript-scan/planner/`; preparation requires no ignored
experiment archive. Set `LEKALO_DEVIN` to the Devin executable and `LEKALO_CODEX`
to the Codex JavaScript entrypoint. `LEKALO_BIN` defaults to the checkout's debug
CLI and `LEKALO_TSC` to the adapter's installed pinned TypeScript `bin/tsc`.
Preparation never installs dependencies. For offline adoption/oracle preflight
without either agent CLI, call the module's `prepare({configure: false})` API.
Default CLI profile versions describe the measured campaign; callers using other
versions must supply explicit `profiles` to `prepare` and freeze a new campaign.

`framework-lift-planner-audit.mjs --maintain ROOT` separately checks each
candidate's native scan/reconfirmation/audit cycle in a fresh copy, without
inference. `--audit ROOT` reaggregates raw CLI usage and source-delivery
markers. Exact raw marker bytes are checked first; an LF fallback is
admitted only if both the original byte count and SHA-256 match. Frozen
collector receipts are preserved, including their earlier CRLF rejections.
Unverified/line-number-decorated fragments are not counted as verified bytes.
Marker byte counts are lower bounds; unique source files require review of
other tool deliveries. Metadata and all-files read counts stay separate.

The after-fix run has eight initially frozen slots plus a separately
preregistered whole Devin quartet allowing read-only `dir`. All twelve
attempts and their usage remain in the evidence. The primary eight are the
whole corrected Devin quartet plus the original Codex quartet. Neither
native failures nor the incomplete original Devin A2 are silently replaced.

## Earlier supported rounding workload

The following records the archived 1.0-era workload and its qualification
limits. The 1.1 planner and the current reuse policy are described above.

These scripts prepare and measure the explicitly bounded
`rounding-supported-top-level-v1` synthetic TypeScript workload. They do
not extend the production scanner or qualify the earlier nested planner.
The task, native corpus, authored baseline Model, independent oracle,
execution order, and executables are frozen before inference.

Preparation does not call a model. Set `LEKALO_BIN` to a built/published
CLI and `LEKALO_TSC` to the pinned TypeScript compiler's `bin/tsc`, then run:

```text
node scripts/research/framework-lift-rounding.mjs
```

The command prints a new disposable root and retains the public adoption
receipts, original native files, authored Model, oracle, and preflight
results there. Native source bytes must remain unchanged by adoption.
Oracle controls require the reference implementation to pass all 20 rows
and the baseline to fail. The holdout stays outside agent workspaces.

Configure fresh workspaces using the module API, with local CLI paths:

```js
import { configure } from './framework-lift-runner.mjs';

configure(preparedRoot, {
  devin: devinExecutable,
  codex: codexJavaScriptEntrypoint,
  execReadPrefixes: ['cat', 'type', 'ls', 'rg'],
});
```

Review and record the resulting campaign digest before starting the
explicit inference command. Running it consumes provider/account usage:

```text
node scripts/research/framework-lift-runner.mjs --run PREPARED_ROOT
```

The default schedule contains two A/B pairs per CLI, in A1/B1/B2/A2
order. Each slot gets a fresh native copy and context. B also receives
the accepted semantic baseline and observed index. Its helper executes
the real CLI context/impact during the measurement window, retaining
warnings and adding confirmed source locations. Both arms share the
same native source inspection tool, task, tests, and external oracle.

Acceptance requires a completed agent response, no confirmation refusal,
CLI success, public tests, typecheck, all holdout rows, and no changes
outside the allowed paths. Exit zero alone does not prove completion.
Original raw receipts are never rewritten to repair their interpretation.
An execution-policy correction uses a separately frozen whole-profile
campaign; preliminary attempts and their usage remain visible.

The 2026-10-26 audit combines the corrected Devin quartet with the
original Codex quartet and retains the original Devin quartet separately:

```text
node scripts/research/framework-lift-audit.mjs INITIAL_ROOT CORRECTED_DEVIN_ROOT
```

The audit reaggregates raw usage, checks Devin step/aggregate parity,
verifies delivered source hashes, and distinguishes native candidate
acceptance from executor completion. Source counts cover `src/**/*.ts`;
all-files delivery and monetary charges remain unknown. Cached input
and reasoning output are subsets, not additional tokens. Timings exclude
independent grading; cold CLI adoption excludes authoring/review labor.

Baseline scan success does not qualify a post-change scan or Model
maintenance. Ordinary local declarations can exceed the public scanner's
supported surface. A valid semantic Model also does not establish current
native bindings. Results and limits are recorded in
[issue #167](https://github.com/ichinya/lekalo/issues/167).
