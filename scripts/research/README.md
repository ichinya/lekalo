# Framework Lift research helpers (#167)

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
