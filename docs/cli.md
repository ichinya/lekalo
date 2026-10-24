# CLI reference

Status: **Implemented** handlers for this checkout; watch remains experimental as documented below. Owner: CLI maintainers. Syntax comes from the checked [command index](#command-index).

## Global options

| Option | Purpose |
| --- | --- |
| `--help` | Show help for the selected command or group. |
| `--version` | Show the product version. |
| `--json` | Select the command's typed JSON result. |
| `--no-cache` | Bypass the cache where the command supports cached reads. |

`--project DIR` and domain-specific flags belong to individual commands; consult their help. Run examples from the stated project root. Installation and PATH setup are covered in [installation](releases.md) and [contributing](contributing.md).

## Commands

Use `lekalo --help` for groups and `lekalo <command> --help` for the exact options supported by your binary. The [command index](#command-index) maps every registered command to one reference. The sections below preserve navigation links; detailed behavior and runnable workflows live in their owners.

### `lekalo watch` (experimental, issue #95)

Current event/result wire contracts are `0.8.14`. Their producing-core
product version is separate metadata; Model/IR and registry pins keep their
own admitted versions. The original `0.8.0` schemas remain published unchanged.

The implementation is a read-only incremental developer loop. Its functional
successor-pin gap is implemented; native macOS qualification remains explicitly
bounded by [the target protocol evidence boundary](target-protocol.md#issue-95-delivery-evidence-boundary). Bare `lekalo watch` selects validation. Explicit
`--validate`, `--impact`, and `--generate-check` select those stages; `--strict`,
`--module`, `--locked`, and the invocation-relative `--project` selector retain
their command policies. Use `--json-lines` for the separate versioned stream;
global `--json` is refused. Human output projects the same replacement results.

`--backend auto|native|poll` defaults to native with an observable auto-only poll
fallback. `--debounce-ms`/`--max-wait-ms` default to 150/1000; polling and independent
reconciliation use content inventories, not event ordering or timestamps. Runs
consume immutable captured inputs outside the project. No source, generated,
lock, observed-index, history or SQLite cache writes are authorized. Generated
files are drift inputs only when `--generate-check` is selected; watch never
calls generation, formatting, cleanup, installation or native production gates.

Global `--no-cache` bypasses session-only document, IR, graph and validation
reuse. `pipelineCounts` distinguishes decoded/reused documents and definitions,
emitted/reused graph source units, and the mandatory global graph assembly and
cycle obligations. Selection uses old/new dependency closures; successful
settled revisions replace the complete effective diagnostic set. Cancelled
runs cannot commit. The closed event/result schemas are
`dev.lekalo.watch-event@0.8.14` and `dev.lekalo.watch-result@0.8.14`.

`--target` now runs ephemeral locked scan/validation against captured inputs,
without an observed index or scan receipt. Legacy adapters receive the visible
`adapter-full-scope` fallback. An adapter must explicitly advertise protocol
0.8.0 and `watch: {selection: true, model_versions: [...]}` in the upgraded
describe to receive the closed `watch_request`: a snapshot-bound module
partition (including removed symbols) or mandatory global obligations, with
exact Model/IR version and captured IR digest pins. Both scan and validate
verify those pins before disclosure. Successor Models require explicit
Model **and** IR `0.8.0` admission; legacy fallback applies only to `0.2.16`. Each response echoes the selection
digest. `pins.adapters[].work` records actual executed/reused partitions and
the source revision of their last execution; untouched diagnostic partitions
are retained, not discarded. A planner-only save executes planner/dependent
partitions plus global obligations, never unrelated module validation.
Generated-only saves reuse target results without even describe.
Entry checksums precede describe, including startup preparation. Only committed project-source
adapters with full `scan.symbols` and scan/validate operations are supported;
installed/registry supplies, overlapping generated read/write scopes, build
read roots, implicit environment/network/child grants, and hosts without an
admitted resource-control lane are refused. Windows uses the 2 GiB job
memory cap and one-process cap. Linux qualifies with bubblewrap, prlimit and
kernel >= 5.14: watch explicitly opts into an 8 GiB address-space cap and a
64-task per-user-namespace bound. Address space includes reservations, not
RSS/commit charge; resource enforcement is honestly `degraded`, and the task
bound is not a prohibition on individual forks. Other command policies are
unchanged. macOS watch uses deny-default seatbelt with explicit `process-fork`
denial and a fail-closed parent-side `proc_pidinfo` resident-memory supervisor.
The 2 GiB resident threshold is sampled at the transport poll interval (5 ms),
so overshoot between samples is possible: this is `degraded` supervision,
**not** a hard allocation ceiling or Windows-job equivalent. Missing sampler
or sandbox support refuses admission; a failed sample or threshold crossing
kills/reaps the owned process before replacement. Ordinary command policies
retain their previous enforcement records.

Results pin component versions, captured Model/IR/source-map/graph/lock/profile
and manifest digests, and verified adapter/capability/protocol identities.
Describe's self-asserted digest is recorded separately from the verified lock
pin. Watch admission caps Model inputs at 1 MiB, 256 modules, 2048 definitions
and 32768 normalized nodes. The closed stream admits at most eight targets and
4096 retained module partitions per target (plus global obligations); accumulated
failed removals refuse observably before exceeding the wire bound. Revision-pair
admission also caps the complete changed-input list at 4096 rows, counting both
sides of renames across semantic and generated trees. Over-limit pairs publish
only a denied, partial-coverage resource-limit replacement with the observed
count and no change rows; they execute no stage and never advance accepted state.
Capture security/I/O refusals retain their original classified diagnostic; paths
outside the frozen diagnostic location grammar (including `.lekalo/generated`)
are deliberately unlocated, never truncated or renamed into a fictitious path.
Session state uses a conservative 64 MiB charge
with cache eviction before refusing mandatory state; this is not an RSS
measurement. Output has one outstanding record (16 MiB maximum), a five-second
acknowledgement deadline, and fail-stop behavior for slow/broken consumers.
Cancellation allows fifteen seconds for worker termination; unproven teardown
emits `recovery-required` with `action: stop`, never a fake cancellation or an
overlapping replacement. An unterminated final line after output failure is
not a complete replacement.

Watch admits exact source-bound Model/IR/graph pairs **0.2.16** and **0.8.0**.
The successor includes typed relations and secondary uniqueness, a separately
versioned impact `0.8.0` payload (unchanged impact algorithm), exact lock and
artifact-check input pins, and independent settled-stage oracle comparisons.
Unknown, mixed, undeclared or stale pins refuse without launching scan/validate;
no implicit conversion or protocol-only inference is allowed. The complete
successor result and refusal classifications are golden-pinned. Broader CPU/RSS
audits remain follow-up work; sampled supervision is never a hard RSS promise.

Validator instrumentation proves actual planner/dependent rule-subject visits
without unrelated subjects, plus explicit global-input fallback. Twenty-eight
clean validation comparisons are supplemented by settled-stage comparisons
across predecessor and successor Models: validate, the
complete impact payload/digest rebound to the
same source-set base/candidate pair, and generate-check, including format-only
saves, atomic replacement, definition additions/removals and semantic-ID
replacement inside an existing document, invalid parse/recovery, generated drift
and invalid lock/recovery. Impact roots use the union of old/current memberships
resolved against the current graph; retired identities remain in old/new module
reverse-closure selection and capsule invalidation, not fictitious current roots. Invalid parse does not advance the accepted semantic baseline.
Real long-lived CLI fault probes verify crash -> invalid -> stale cancellation
-> recovery, and negotiated selection with an unrelated failing partition
surviving rapid planner saves and cancellation. The selected target failure is
compared to a clean full `verify` adapter component for predecessor inputs;
its IR is produced independently by `load --ir --no-cache` in a private oracle
copy. Successor targets compare with an independent fresh no-cache locked
watch cycle; ordinary `verify` does not implicitly acquire successor pins.
A fresh no-cache core service additionally compares the complete retained
diagnostic set. Generated saves stay quiet
for multiple reconciliation intervals; cache/build feedback remains excluded.

Windows native/poll and a complete Debian WSL inotify/poll pass cover burst
edits, atomic replacement, parent rename/deletion and drift. Later WSL reruns
and even shell startup hit host deadlines; the retained full pass and this
intermittent revalidation bound are [golden-pinned](target-protocol.md#issue-95-delivery-evidence-boundary),
not an unconditional stable-Linux-host claim. Hermetic notify traces
exercise the real callback/channel/dirty-reader paths for inotify overflow,
FSEvents rescan/error, split rename and metadata/identical-byte events; they do
not substitute for a native macOS result. Real slow/broken pipe probes verify
fail-stop output. Linux target tests inspect the actual namespace resource
limits, not just declared values. The macOS supervisor threshold, sample failure,
cancellation and kill/reap paths are driver-qualified through the production
pump on any host; these synthetic samples do **not** execute `proc_pidinfo` or
seatbelt. The Darwin cfg also cross-compiles; that is not a native runtime test.
The same real target crash/selection probes now run on macOS in CI rather than
accepting terminal refusal. CI includes Linux/macOS/Windows, but wiring is not
evidence of a hosted pass. The supported delivery boundary is documented in
[the target protocol](target-protocol.md#issue-95-delivery-evidence-boundary).

### `lekalo init`

Create a new minimal Model project with [bootstrap](bootstrap.md). Start with [minimal project](project-layout.md#minimal-project) for a complete first run.

### `lekalo init --adopt`

Add a minimal skeleton to an existing repository using [adopt](adopt.md). Detection remains evidence; it does not generate an application or rewrite source.

### `lekalo module new`

Create an empty module using the grammar and no-clobber rules in [bootstrap](bootstrap.md).

### `lekalo load`

[Loader](loader.md) owns document admission and IR output.

### `lekalo validate`

[Validation](validation.md) owns scopes, profiles and semantic findings.

### `lekalo lock` and `lekalo update`

[Lockfile](lockfile.md) owns resolution, locked/offline behavior and explicit update confirmation.

### `lekalo migrate` and `lekalo compatibility`

[Versioning](versioning.md#compatibility-and-migrations) owns exact version admission and registered migration paths.

### `lekalo adapter test`

[Adapter conformance](adapter-conformance.md) owns profiles, fixture classes and verdicts.

## Storage (issue #69)

[Storage engine](storage-engine.md) owns admission, migration plans and checks. Domain projection is specified separately in [storage projection](storage-projection.md).

## Run history (issue #121)

[Run history](run-history.md) owns recorder inputs, assertion evidence, retention and scope isolation.

###  package surfaces (issue #32)

[Adapter installation](adapter-install.md) owns trust, install plans, quarantine and revocation. [Package manifests](adapter-manifest.md) specify exact bytes and digests.

## Exit and stream contract

[Diagnostics](diagnostics.md) is the canonical exit/stream table. Read the command verdict; warning severity does not independently choose an exit.

## JSON wire shapes

[Diagnostics](diagnostics.md) specifies shared result envelopes. Each command owner links to its exact schema; `--json` does not make different command payloads interchangeable.

## Provider (issue #34)

[Workflow provider](provider-contract.md) owns discovery, operation envelopes and process limits. [Integrations](integrations.md) owns the handoff walkthrough.

## Graph

[Graph](graph.md) owns graph kinds, relations and completeness.

## Effects

[Effect graph](effect-graph.md) owns declared/detected effects, paths and uncertainty.

## Diff

[Semantic diff](semantic-diff.md) owns compatibility classes and migration hints.

## Context

[Context](context.md) owns capsule selection and budgets. [Context budget](context-budget.md) owns measured budget reports.

## Trace

[Trace manifest](trace-manifest.md) owns trace grammar and queries. [Trace assessment](trace-assessment.md) owns neutral evaluation.

## Inspect

[Inspect](inspect.md) owns the bounded symbol card.

## Generate

[Orchestration](orchestration.md) owns dry-run, generation, cleanup and verification. [Managed mode](managed-mode.md) owns its separate confirmed transaction lane.

### Zod schemas for the Node/TypeScript target (issue #45)

[Zod generation](zod-generation.md) owns mapping, codegen policy and drift checks.

## Requirements

[Requirements](requirements.md) owns OpenSpec references, revision custody and coverage.

## NFR (issue #85)

[NFR](nfr.md) owns nonfunctional requirements and admitted evidence.

## Impact

[Impact](impact.md) owns closure, changed-input selection and coverage. The [architecture example](architecture.md#projection-example) is the maintained walkthrough.

## Contract (contracted mode)

[Contracted mode](contracted-mode.md) owns maintained-code conformance. [Contract one module](adoption.md#contract-one-module) is the runnable walkthrough.

## Scan and bindings (issue #42)

[Observed mode](observed-mode.md) owns scanning and [bindings](bindings.md) owns explicit confirmation.

## Expressions (issue #66)

[Expressions](expressions.md) owns declarations and [reference evaluation](reference-evaluation.md) owns the supported evaluator subset.

## Storage and storage profile (issue #117)

[Storage projection](storage-projection.md), [engine profile](storage-engine-profile.md) and [introspection](storage-introspection.md) own the three distinct stages.

## OpenAPI (issue #46)

[OpenAPI](openapi.md) owns projection versions, modes and supported mappings.

## Coupling (issue #77)

[Coupling](coupling.md) owns report metrics, policy and uncertainty.

## Privacy (issue #119)

[Privacy runtime](privacy-runtime.md) owns evaluation/enforcement; [privacy policy](privacy.md) owns admitted policy semantics.

<!-- issue-105-command-index -->
## Command index

Status: **Implemented** command handlers for product 0.9.0. Handler existence does not imply every target capability, migration or native execution is available. Each row comes from recursive CLI help and has one owner in [the checked ownership inventory](documentation-owners.json). `init --adopt` is described in [adopt](adopt.md); native execution remains a typed refusal in the production CLI.

| Command | Synopsis from this binary | Reference owner |
| --- | --- | --- |
| `lekalo adapter` | `lekalo adapter [OPTIONS] <COMMAND>` | [reference](adapter-install.md) |
| `lekalo adapter discover` | `lekalo adapter discover [OPTIONS] --source <SOURCE>` | [reference](adapter-install.md) |
| `lekalo adapter info` | `lekalo adapter info [OPTIONS] <ID>` | [reference](adapter-install.md) |
| `lekalo adapter install` | `lekalo adapter install [OPTIONS] <SOURCE>` | [reference](adapter-install.md) |
| `lekalo adapter list` | `lekalo adapter list [OPTIONS]` | [reference](adapter-install.md) |
| `lekalo adapter quarantine` | `lekalo adapter quarantine [OPTIONS] <COMMAND>` | [reference](adapter-install.md) |
| `lekalo adapter quarantine list` | `lekalo adapter quarantine list [OPTIONS]` | [reference](adapter-install.md) |
| `lekalo adapter quarantine purge` | `lekalo adapter quarantine purge [OPTIONS]` | [reference](adapter-install.md) |
| `lekalo adapter quarantine release` | `lekalo adapter quarantine release [OPTIONS] <ID>` | [reference](adapter-install.md) |
| `lekalo adapter revoke` | `lekalo adapter revoke [OPTIONS] --reason <TOKEN> <ID>` | [reference](adapter-install.md) |
| `lekalo adapter rollback` | `lekalo adapter rollback [OPTIONS] --to <VERSION> <ID>` | [reference](adapter-install.md) |
| `lekalo adapter test` | `lekalo adapter test [OPTIONS] [PROGRAM_ARGS]...` | [reference](adapter-conformance.md) |
| `lekalo adapter trust` | `lekalo adapter trust [OPTIONS] --level <LEVEL> <ID>` | [reference](adapter-install.md) |
| `lekalo adapter update` | `lekalo adapter update [OPTIONS] <ID>` | [reference](adapter-install.md) |
| `lekalo adoption` | `lekalo adoption [OPTIONS] <COMMAND>` | [reference](adoption.md) |
| `lekalo adoption analyze` | `lekalo adoption analyze [OPTIONS]` | [reference](adoption.md) |
| `lekalo adoption apply` | `lekalo adoption apply [OPTIONS] --plan <FILE> --verification <FILE> --confirm <DIGEST>` | [reference](adoption.md) |
| `lekalo adoption confirm` | `lekalo adoption confirm [OPTIONS]` | [reference](adoption.md) |
| `lekalo adoption coverage` | `lekalo adoption coverage [OPTIONS]` | [reference](adoption.md) |
| `lekalo adoption recovery` | `lekalo adoption recovery [OPTIONS]` | [reference](adoption.md) |
| `lekalo adoption refactor` | `lekalo adoption refactor [OPTIONS] --verification <FILE>` | [reference](adoption.md) |
| `lekalo adoption rollback` | `lekalo adoption rollback [OPTIONS] --receipt <FILE>` | [reference](adoption.md) |
| `lekalo adoption verify` | `lekalo adoption verify [OPTIONS] --plan <FILE> --verification <FILE>` | [reference](adoption.md) |
| `lekalo ai-lint` | `lekalo ai-lint [OPTIONS]` | [reference](ai-lint.md) |
| `lekalo architecture-profile` | `lekalo architecture-profile [OPTIONS] <COMMAND>` | [reference](architecture-profile.md) |
| `lekalo architecture-profile assess` | `lekalo architecture-profile assess [OPTIONS] <--module <MODULE>\|--all>` | [reference](architecture-profile.md) |
| `lekalo architecture-profile catalog` | `lekalo architecture-profile catalog [OPTIONS]` | [reference](architecture-profile.md) |
| `lekalo architecture-profile diff` | `lekalo architecture-profile diff [OPTIONS] --base-profiles <BASE_PROFILES> --candidate-profiles <CANDIDATE_PROFILES>` | [reference](architecture-profile.md) |
| `lekalo architecture-profile lock` | `lekalo architecture-profile lock [OPTIONS]` | [reference](architecture-profile.md) |
| `lekalo architecture-profile resolve` | `lekalo architecture-profile resolve [OPTIONS] --architecture-profile <ARCHITECTURE_PROFILE>` | [reference](architecture-profile.md) |
| `lekalo bindings` | `lekalo bindings [OPTIONS] <COMMAND>` | [reference](bindings.md) |
| `lekalo bindings audit` | `lekalo bindings audit [OPTIONS]` | [reference](bindings.md) |
| `lekalo bindings confirm` | `lekalo bindings confirm [OPTIONS] [PROPOSAL]` | [reference](bindings.md) |
| `lekalo bindings list` | `lekalo bindings list [OPTIONS]` | [reference](bindings.md) |
| `lekalo bindings propose` | `lekalo bindings propose [OPTIONS]` | [reference](bindings.md) |
| `lekalo cache` | `lekalo cache [OPTIONS] <COMMAND>` | [reference](cache.md) |
| `lekalo cache clear` | `lekalo cache clear [OPTIONS]` | [reference](cache.md) |
| `lekalo cache status` | `lekalo cache status [OPTIONS]` | [reference](cache.md) |
| `lekalo change` | `lekalo change [OPTIONS] <COMMAND>` | [reference](change.md) |
| `lekalo change binding:add` | `lekalo change binding:add [OPTIONS] --symbol <ID> --definition-json <JSON>` | [reference](change.md) |
| `lekalo change command-effect:add` | `lekalo change command-effect:add [OPTIONS] --command <ID>` | [reference](change.md) |
| `lekalo change command-effect:remove` | `lekalo change command-effect:remove [OPTIONS] --command <ID>` | [reference](change.md) |
| `lekalo change custody-init` | `lekalo change custody-init [OPTIONS] --expect-history <DIGEST>` | [reference](change.md) |
| `lekalo change definition:add` | `lekalo change definition:add [OPTIONS] --kind <KIND> --module <NAME> --name <NAME>` | [reference](change.md) |
| `lekalo change entity-field:add` | `lekalo change entity-field:add [OPTIONS] --entity <ID> --name <NAME> --type <TYPE>` | [reference](change.md) |
| `lekalo change entity-field:remove` | `lekalo change entity-field:remove [OPTIONS] --entity <ID> --name <NAME>` | [reference](change.md) |
| `lekalo change entity-field:rename` | `lekalo change entity-field:rename [OPTIONS] --entity <ID> --name <NAME> --to <NAME>` | [reference](change.md) |
| `lekalo change enum-case:add` | `lekalo change enum-case:add [OPTIONS] --enum <ID> --name <NAME>` | [reference](change.md) |
| `lekalo change enum-case:change` | `lekalo change enum-case:change [OPTIONS] --enum <ID> --name <NAME>` | [reference](change.md) |
| `lekalo change migration:apply` | `lekalo change migration:apply [OPTIONS] --step <ID>` | [reference](change.md) |
| `lekalo change module:move` | `lekalo change module:move [OPTIONS] --module <ID> --directory <LEAF>` | [reference](change.md) |
| `lekalo change operation-error:set` | `lekalo change operation-error:set [OPTIONS] --operation <ID> --definition-json <JSON>` | [reference](change.md) |
| `lekalo change operation-input:set` | `lekalo change operation-input:set [OPTIONS] --operation <ID> --input-json <JSON>` | [reference](change.md) |
| `lekalo change operation-output:set` | `lekalo change operation-output:set [OPTIONS] --operation <ID> --type <TYPE>` | [reference](change.md) |
| `lekalo change ownership:set` | `lekalo change ownership:set [OPTIONS] --module <ID> --mode <MODE> --evidence-json <JSON>` | [reference](change.md) |
| `lekalo change query-effect:add` | `lekalo change query-effect:add [OPTIONS] --query <ID> --effect <ID>` | [reference](change.md) |
| `lekalo change query-effect:remove` | `lekalo change query-effect:remove [OPTIONS] --query <ID> --effect <ID>` | [reference](change.md) |
| `lekalo change receipt` | `lekalo change receipt [OPTIONS] --idempotency-key <KEY>` | [reference](change.md) |
| `lekalo change recover` | `lekalo change recover [OPTIONS] --transaction <ID>` | [reference](change.md) |
| `lekalo change request` | `lekalo change request [OPTIONS] --request <FILE>` | [reference](change.md) |
| `lekalo change requirement:link` | `lekalo change requirement:link [OPTIONS] --symbol <ID> --requirement <ID>` | [reference](change.md) |
| `lekalo change requirement:unlink` | `lekalo change requirement:unlink [OPTIONS] --symbol <ID> --requirement <ID>` | [reference](change.md) |
| `lekalo change symbol:move` | `lekalo change symbol:move [OPTIONS] --symbol <ID> --to-module <ID>` | [reference](change.md) |
| `lekalo classification` | `lekalo classification [OPTIONS] <COMMAND>` | [reference](classification.md) |
| `lekalo classification inspect` | `lekalo classification inspect [OPTIONS] --attachment <PATH> --policy <PATH>` | [reference](classification.md) |
| `lekalo classification validate` | `lekalo classification validate [OPTIONS] --attachment <PATH> --policy <PATH>` | [reference](classification.md) |
| `lekalo compatibility` | `lekalo compatibility [OPTIONS]` | [reference](versioning.md) |
| `lekalo context` | `lekalo context [OPTIONS] --budget <TOKENS> [SYMBOL]` | [reference](context.md) |
| `lekalo context-budget` | `lekalo context-budget [OPTIONS]` | [reference](context-budget.md) |
| `lekalo contract` | `lekalo contract [OPTIONS] <COMMAND>` | [reference](contracted-mode.md) |
| `lekalo contract attach` | `lekalo contract attach [OPTIONS] <SYMBOL>` | [reference](contracted-mode.md) |
| `lekalo contract check` | `lekalo contract check [OPTIONS]` | [reference](contracted-mode.md) |
| `lekalo contract support` | `lekalo contract support [OPTIONS] --kind <KIND> --path <PATH> <SYMBOL>` | [reference](contracted-mode.md) |
| `lekalo contract update` | `lekalo contract update [OPTIONS] --declaration <FILE>` | [reference](contracted-mode.md) |
| `lekalo coupling` | `lekalo coupling [OPTIONS] <--symbol <SYMBOL>\|--module <MODULE>\|--all\|--changed-input <CHANGED_INPUT>>` | [reference](coupling.md) |
| `lekalo dataflow` | `lekalo dataflow [OPTIONS] <COMMAND>` | [reference](classification.md) |
| `lekalo dataflow report` | `lekalo dataflow report [OPTIONS] --attachment <PATH> --policy <PATH>` | [reference](classification.md) |
| `lekalo diff` | `lekalo diff [OPTIONS] <OLD> [NEW]` | [reference](semantic-diff.md) |
| `lekalo doctor` | `lekalo doctor [OPTIONS]` | [reference](doctor.md) |
| `lekalo effects` | `lekalo effects [OPTIONS] <COMMAND>` | [reference](effect-graph.md) |
| `lekalo effects conflicts` | `lekalo effects conflicts [OPTIONS] --changed <OPERATIONS>` | [reference](effect-graph.md) |
| `lekalo effects show` | `lekalo effects show [OPTIONS] <OPERATION>` | [reference](effect-graph.md) |
| `lekalo effects writers` | `lekalo effects writers [OPTIONS] <RESOURCE>` | [reference](effect-graph.md) |
| `lekalo evaluation` | `lekalo evaluation [OPTIONS] <COMMAND>` | [reference](framework-lift.md) |
| `lekalo evaluation compare` | `lekalo evaluation compare [OPTIONS] --baseline <BASELINE> --task <TASK> --campaign <CAMPAIGN> --consumer-alias <CONSUMER_ALIAS>` | [reference](framework-lift.md) |
| `lekalo evaluation preflight` | `lekalo evaluation preflight [OPTIONS] --baseline <BASELINE> --task <TASK> --campaign <CAMPAIGN> --workspace <WORKSPACE>` | [reference](framework-lift.md) |
| `lekalo evaluation record-arm` | `lekalo evaluation record-arm [OPTIONS] --baseline <BASELINE> --task <TASK> --campaign <CAMPAIGN> --input <INPUT>` | [reference](framework-lift.md) |
| `lekalo evaluation validate` | `lekalo evaluation validate [OPTIONS] --family <FAMILY> --input <INPUT>` | [reference](framework-lift.md) |
| `lekalo explain` | `lekalo explain [OPTIONS] [SELECTORS]...` | [reference](explain.md) |
| `lekalo expressions` | `lekalo expressions [OPTIONS] <COMMAND>` | [reference](expressions.md) |
| `lekalo expressions diff` | `lekalo expressions diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](expressions.md) |
| `lekalo expressions eval` | `lekalo expressions eval [OPTIONS] --vectors <FILE> <PATH>` | [reference](expressions.md) |
| `lekalo expressions render` | `lekalo expressions render [OPTIONS] --target <TARGET> <PATH>` | [reference](expressions.md) |
| `lekalo expressions validate` | `lekalo expressions validate [OPTIONS] <PATH>` | [reference](expressions.md) |
| `lekalo find` | `lekalo find [OPTIONS]` | [reference](semantic-search.md) |
| `lekalo fix` | `lekalo fix [OPTIONS] [SELECTOR]` | [reference](fixes.md) |
| `lekalo format` | `lekalo format [OPTIONS] [PATH]` | [reference](format.md) |
| `lekalo generate` | `lekalo generate [OPTIONS] [PROGRAM_ARGS]...` | [reference](orchestration.md) |
| `lekalo graph` | `lekalo graph [OPTIONS] <COMMAND>` | [reference](graph.md) |
| `lekalo graph callers` | `lekalo graph callers [OPTIONS] <SYMBOL>` | [reference](graph.md) |
| `lekalo graph export` | `lekalo graph export [OPTIONS]` | [reference](graph.md) |
| `lekalo graph path` | `lekalo graph path [OPTIONS] <FROM> <TO>` | [reference](graph.md) |
| `lekalo graph show` | `lekalo graph show [OPTIONS] <SYMBOL>` | [reference](graph.md) |
| `lekalo history` | `lekalo history [OPTIONS] <COMMAND>` | [reference](run-history.md) |
| `lekalo history clear` | `lekalo history clear [OPTIONS] --scope <TOKEN>` | [reference](run-history.md) |
| `lekalo history compact` | `lekalo history compact [OPTIONS]` | [reference](run-history.md) |
| `lekalo history delete` | `lekalo history delete [OPTIONS] --scope <TOKEN> <RUN_ID>` | [reference](run-history.md) |
| `lekalo history dependents` | `lekalo history dependents [OPTIONS] <COMMAND>` | [reference](run-history.md) |
| `lekalo history dependents register` | `lekalo history dependents register [OPTIONS] --kind <KIND> --scope <TOKEN> <DEPENDENT_ID>` | [reference](run-history.md) |
| `lekalo history dependents resolve` | `lekalo history dependents resolve [OPTIONS] --scope <TOKEN> <DEPENDENT_ID>` | [reference](run-history.md) |
| `lekalo history init` | `lekalo history init [OPTIONS]` | [reference](run-history.md) |
| `lekalo history list` | `lekalo history list [OPTIONS] --scope <TOKEN>` | [reference](run-history.md) |
| `lekalo history prune` | `lekalo history prune [OPTIONS] --scope <TOKEN>` | [reference](run-history.md) |
| `lekalo history record` | `lekalo history record [OPTIONS] --input <SOURCE> --scope <TOKEN>` | [reference](run-history.md) |
| `lekalo history recover` | `lekalo history recover [OPTIONS]` | [reference](run-history.md) |
| `lekalo history retention` | `lekalo history retention [OPTIONS] --scope <TOKEN>` | [reference](run-history.md) |
| `lekalo history scope` | `lekalo history scope [OPTIONS] <COMMAND>` | [reference](run-history.md) |
| `lekalo history scope create` | `lekalo history scope create [OPTIONS]` | [reference](run-history.md) |
| `lekalo history show` | `lekalo history show [OPTIONS] --scope <TOKEN> <RUN_ID>` | [reference](run-history.md) |
| `lekalo impact` | `lekalo impact [OPTIONS] [SYMBOL]` | [reference](impact.md) |
| `lekalo init` | `lekalo init [OPTIONS]` | [reference](bootstrap.md) |
| `lekalo inspect` | `lekalo inspect [OPTIONS] <SYMBOL>` | [reference](inspect.md) |
| `lekalo load` | `lekalo load [OPTIONS]` | [reference](loader.md) |
| `lekalo lock` | `lekalo lock [OPTIONS] [PROGRAM_ARGS]...` | [reference](lockfile.md) |
| `lekalo managed` | `lekalo managed [OPTIONS] <COMMAND>` | [reference](managed-mode.md) |
| `lekalo managed apply` | `lekalo managed apply [OPTIONS] --plan <FILE> --coverage <FILE> --ownership <FILE>` | [reference](managed-mode.md) |
| `lekalo managed authority` | `lekalo managed authority [OPTIONS] --module <MODULE>` | [reference](managed-mode.md) |
| `lekalo managed check` | `lekalo managed check [OPTIONS] --ownership <FILE>` | [reference](managed-mode.md) |
| `lekalo managed equivalence` | `lekalo managed equivalence [OPTIONS] --evidence <FILE>` | [reference](managed-mode.md) |
| `lekalo managed plan` | `lekalo managed plan [OPTIONS] --to <MODE> --ownership <FILE> --coverage <FILE> --module <MODULE> --target <TARGET>` | [reference](managed-mode.md) |
| `lekalo managed recover` | `lekalo managed recover [OPTIONS]` | [reference](managed-mode.md) |
| `lekalo managed resolve` | `lekalo managed resolve [OPTIONS] --path <PATH>` | [reference](managed-mode.md) |
| `lekalo managed rollback` | `lekalo managed rollback [OPTIONS] --to <DIGEST>` | [reference](managed-mode.md) |
| `lekalo managed upgrade-plan` | `lekalo managed upgrade-plan [OPTIONS] --module <MODULE> --target <TARGET> --ownership <FILE> --base <FILE> --candidate <FILE>` | [reference](managed-mode.md) |
| `lekalo mcp` | `lekalo mcp [OPTIONS] <COMMAND>` | [reference](mcp.md) |
| `lekalo mcp describe` | `lekalo mcp describe [OPTIONS]` | [reference](mcp.md) |
| `lekalo mcp serve` | `lekalo mcp serve [OPTIONS]` | [reference](mcp.md) |
| `lekalo metrics` | `lekalo metrics [OPTIONS] <COMMAND>` | [reference](metrics-export.md) |
| `lekalo metrics export` | `lekalo metrics export [OPTIONS] --evaluation <FILE> --scope <TOKEN>` | [reference](metrics-export.md) |
| `lekalo metrics status` | `lekalo metrics status [OPTIONS] --scope <TOKEN> <EXPORT_ID>` | [reference](metrics-export.md) |
| `lekalo migrate` | `lekalo migrate [OPTIONS]` | [reference](versioning.md) |
| `lekalo module` | `lekalo module [OPTIONS] <COMMAND>` | [reference](bootstrap.md) |
| `lekalo module api` | `lekalo module api [OPTIONS] <COMMAND>` | [reference](bootstrap.md) |
| `lekalo module api check` | `lekalo module api check [OPTIONS] --api <PATH>` | [reference](bootstrap.md) |
| `lekalo module api consumers` | `lekalo module api consumers [OPTIONS] --api <PATH> <MODULE>` | [reference](bootstrap.md) |
| `lekalo module api context` | `lekalo module api context [OPTIONS] --consumer <MODULE> --uses <IDS> --budget <TOKENS> <PUBLIC>` | [reference](bootstrap.md) |
| `lekalo module api diff` | `lekalo module api diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](bootstrap.md) |
| `lekalo module api export` | `lekalo module api export [OPTIONS] --api <PATH> <MODULE>` | [reference](bootstrap.md) |
| `lekalo module api lock` | `lekalo module api lock [OPTIONS] --api <PATH> --module <MODULE> --consumer <MODULE>` | [reference](bootstrap.md) |
| `lekalo module api mapping` | `lekalo module api mapping [OPTIONS] --in-process <PATH> --service <PATH> <PUBLIC>` | [reference](bootstrap.md) |
| `lekalo module new` | `lekalo module new [OPTIONS] <ID>` | [reference](bootstrap.md) |
| `lekalo native` | `lekalo native [OPTIONS] <COMMAND>` | [reference](native-gates.md) |
| `lekalo native run` | `lekalo native run [OPTIONS] <PLAN>` | [reference](native-gates.md) |
| `lekalo nfr` | `lekalo nfr [OPTIONS] <COMMAND>` | [reference](nfr.md) |
| `lekalo nfr diff` | `lekalo nfr diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](nfr.md) |
| `lekalo nfr impact` | `lekalo nfr impact [OPTIONS] --base <BASE> <PATH>` | [reference](nfr.md) |
| `lekalo nfr query` | `lekalo nfr query [OPTIONS] --as-of <DATE> <PATH> <SELECTOR>` | [reference](nfr.md) |
| `lekalo nfr report` | `lekalo nfr report [OPTIONS] --as-of <DATE> <PATH>` | [reference](nfr.md) |
| `lekalo nfr trace` | `lekalo nfr trace [OPTIONS] --as-of <DATE> <PATH>` | [reference](nfr.md) |
| `lekalo nfr validate` | `lekalo nfr validate [OPTIONS] --as-of <DATE> <PATH>` | [reference](nfr.md) |
| `lekalo observe` | `lekalo observe [OPTIONS] <COMMAND>` | [reference](observed-mode.md) |
| `lekalo observe attach` | `lekalo observe attach [OPTIONS] <SYMBOL>` | [reference](observed-mode.md) |
| `lekalo observe baseline` | `lekalo observe baseline [OPTIONS]` | [reference](observed-mode.md) |
| `lekalo observe bind` | `lekalo observe bind [OPTIONS] --path <PATH> <SYMBOL>` | [reference](observed-mode.md) |
| `lekalo observe check` | `lekalo observe check [OPTIONS]` | [reference](observed-mode.md) |
| `lekalo observe confirm` | `lekalo observe confirm [OPTIONS] <SYMBOL>` | [reference](observed-mode.md) |
| `lekalo observe impact` | `lekalo observe impact [OPTIONS] <SYMBOL>` | [reference](observed-mode.md) |
| `lekalo observe inspect` | `lekalo observe inspect [OPTIONS] <SYMBOL>` | [reference](observed-mode.md) |
| `lekalo observe promote` | `lekalo observe promote [OPTIONS]` | [reference](observed-mode.md) |
| `lekalo observe update` | `lekalo observe update [OPTIONS] --scan <FILE>` | [reference](observed-mode.md) |
| `lekalo openapi` | `lekalo openapi [OPTIONS] <COMMAND>` | [reference](openapi.md) |
| `lekalo openapi check` | `lekalo openapi check [OPTIONS] --transport <FILE> <PATH>` | [reference](openapi.md) |
| `lekalo openapi diff` | `lekalo openapi diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](openapi.md) |
| `lekalo openapi inspect` | `lekalo openapi inspect [OPTIONS] --endpoint <SYMBOL> <PATH>` | [reference](openapi.md) |
| `lekalo openapi render` | `lekalo openapi render [OPTIONS] <PATH>` | [reference](openapi.md) |
| `lekalo pilot` | `lekalo pilot [OPTIONS] <COMMAND>` | [reference](worklog-changelog.md) |
| `lekalo pilot admit` | `lekalo pilot admit [OPTIONS] <PATH> <COMPOSITION>` | [reference](worklog-changelog.md) |
| `lekalo pilot validate` | `lekalo pilot validate [OPTIONS] <PATH>` | [reference](worklog-changelog.md) |
| `lekalo portability` | `lekalo portability [OPTIONS]` | [reference](portability.md) |
| `lekalo privacy` | `lekalo privacy [OPTIONS] <COMMAND>` | [reference](privacy-runtime.md) |
| `lekalo privacy evaluate` | `lekalo privacy evaluate [OPTIONS] --decision <FILE>` | [reference](privacy-runtime.md) |
| `lekalo privacy export` | `lekalo privacy export [OPTIONS] --destination <SPEC> <ARTIFACT>` | [reference](privacy-runtime.md) |
| `lekalo privacy redact` | `lekalo privacy redact [OPTIONS] --payload <FILE>` | [reference](privacy-runtime.md) |
| `lekalo privacy subject` | `lekalo privacy subject [OPTIONS] --artifact <FILE> --destination <SPEC>` | [reference](privacy-runtime.md) |
| `lekalo provider` | `lekalo provider [OPTIONS] <COMMAND>` | [reference](provider-contract.md) |
| `lekalo provider describe` | `lekalo provider describe [OPTIONS]` | [reference](provider-contract.md) |
| `lekalo query-model` | `lekalo query-model [OPTIONS] <COMMAND>` | [reference](query-model.md) |
| `lekalo query-model diff` | `lekalo query-model diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](query-model.md) |
| `lekalo query-model validate` | `lekalo query-model validate [OPTIONS] <PATH>` | [reference](query-model.md) |
| `lekalo readiness` | `lekalo readiness [OPTIONS] --phase <PHASE>` | [reference](doctor.md) |
| `lekalo requirements` | `lekalo requirements [OPTIONS] <COMMAND>` | [reference](requirements.md) |
| `lekalo requirements query` | `lekalo requirements query [OPTIONS] <PATH> <SELECTOR>` | [reference](requirements.md) |
| `lekalo requirements report` | `lekalo requirements report [OPTIONS] <PATH>` | [reference](requirements.md) |
| `lekalo requirements trace` | `lekalo requirements trace [OPTIONS] <PATH>` | [reference](requirements.md) |
| `lekalo requirements validate` | `lekalo requirements validate [OPTIONS] <PATH>` | [reference](requirements.md) |
| `lekalo scan` | `lekalo scan [OPTIONS] --target <TARGET> [PROGRAM_ARGS]...` | [reference](observed-mode.md) |
| `lekalo search` | `lekalo search [OPTIONS] <QUERY>` | [reference](semantic-search.md) |
| `lekalo search-index` | `lekalo search-index [OPTIONS] <COMMAND>` | [reference](semantic-search.md) |
| `lekalo search-index build` | `lekalo search-index build [OPTIONS]` | [reference](semantic-search.md) |
| `lekalo search-index status` | `lekalo search-index status [OPTIONS]` | [reference](semantic-search.md) |
| `lekalo search-index update` | `lekalo search-index update [OPTIONS]` | [reference](semantic-search.md) |
| `lekalo status` | `lekalo status [OPTIONS]` | [reference](doctor.md) |
| `lekalo storage` | `lekalo storage [OPTIONS] <COMMAND>` | [reference](storage-engine.md) |
| `lekalo storage capabilities` | `lekalo storage capabilities [OPTIONS] --projection <PATH> <PATH>` | [reference](storage-engine.md) |
| `lekalo storage conformance` | `lekalo storage conformance [OPTIONS] --profile <PATH> --projection <PATH>` | [reference](storage-engine.md) |
| `lekalo storage ddl` | `lekalo storage ddl [OPTIONS] --projection <PATH> <PROFILE>` | [reference](storage-engine.md) |
| `lekalo storage diff` | `lekalo storage diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](storage-projection.md) |
| `lekalo storage drift` | `lekalo storage drift [OPTIONS] --projection <ATTACHMENT> --profile <PATH> <SCAN>` | [reference](storage-engine.md) |
| `lekalo storage input` | `lekalo storage input [OPTIONS] --projection <PATH> <PROFILE>` | [reference](storage-engine.md) |
| `lekalo storage introspect-check` | `lekalo storage introspect-check [OPTIONS] --projection <PATH> --evidence <PATH> --namespace <NAMESPACE>` | [reference](storage-introspection.md) |
| `lekalo storage laravel-plan` | `lekalo storage laravel-plan [OPTIONS] --profile <PATH> <BASE> <CANDIDATE>` | [reference](laravel-migrations.md) |
| `lekalo storage migrate-plan` | `lekalo storage migrate-plan [OPTIONS] --profile <PATH> <BASE> <CANDIDATE>` | [reference](storage-engine.md) |
| `lekalo storage plan` | `lekalo storage plan [OPTIONS] <BASE> <CANDIDATE>` | [reference](storage-projection.md) |
| `lekalo storage profile` | `lekalo storage profile [OPTIONS] --engine <ENGINE>` | [reference](storage-engine.md) |
| `lekalo storage project` | `lekalo storage project [OPTIONS] --namespace <NAMESPACE> <PATH>` | [reference](storage-projection.md) |
| `lekalo storage validate` | `lekalo storage validate [OPTIONS] <PATH>` | [reference](storage-projection.md) |
| `lekalo storage validate-engine` | `lekalo storage validate-engine [OPTIONS] <PATH>` | [reference](storage-engine.md) |
| `lekalo storage-profile` | `lekalo storage-profile [OPTIONS] <COMMAND>` | [reference](storage-engine-profile.md) |
| `lekalo storage-profile capabilities` | `lekalo storage-profile capabilities [OPTIONS] <PATH>` | [reference](storage-engine-profile.md) |
| `lekalo storage-profile diff` | `lekalo storage-profile diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](storage-engine-profile.md) |
| `lekalo storage-profile portability` | `lekalo storage-profile portability [OPTIONS] <BASE> <TARGET>` | [reference](storage-engine-profile.md) |
| `lekalo storage-profile validate` | `lekalo storage-profile validate [OPTIONS] <PATH>` | [reference](storage-engine-profile.md) |
| `lekalo task-links` | `lekalo task-links [OPTIONS] <COMMAND>` | [reference](task-external-links.md) |
| `lekalo task-links execute` | `lekalo task-links execute [OPTIONS] <PATH>` | [reference](task-external-links.md) |
| `lekalo task-links impact` | `lekalo task-links impact [OPTIONS] --attachment <PATH> <CONTRACT>` | [reference](task-external-links.md) |
| `lekalo task-links inspect` | `lekalo task-links inspect [OPTIONS] --attachment <PATH> <SYMBOL>` | [reference](task-external-links.md) |
| `lekalo task-links portability` | `lekalo task-links portability [OPTIONS] --from-profile <PATH> --to-profile <PATH> <PATH>` | [reference](task-external-links.md) |
| `lekalo task-links validate` | `lekalo task-links validate [OPTIONS] <PATH>` | [reference](task-external-links.md) |
| `lekalo trace` | `lekalo trace [OPTIONS] <COMMAND>` | [reference](trace-manifest.md) |
| `lekalo trace assess` | `lekalo trace assess [OPTIONS] --evidence <PATH> <PATH>` | [reference](trace-manifest.md) |
| `lekalo trace collect` | `lekalo trace collect [OPTIONS]` | [reference](trace-manifest.md) |
| `lekalo trace export` | `lekalo trace export [OPTIONS] <PATH>` | [reference](trace-manifest.md) |
| `lekalo trace query` | `lekalo trace query [OPTIONS] <PATH> <SELECTOR>` | [reference](trace-manifest.md) |
| `lekalo trace validate` | `lekalo trace validate [OPTIONS] <PATH>` | [reference](trace-manifest.md) |
| `lekalo transport` | `lekalo transport [OPTIONS] <COMMAND>` | [reference](transport-http.md) |
| `lekalo transport diff` | `lekalo transport diff [OPTIONS] <BASE> <CANDIDATE>` | [reference](transport-http.md) |
| `lekalo transport inspect` | `lekalo transport inspect [OPTIONS] --endpoint <SYMBOL> <PATH>` | [reference](transport-http.md) |
| `lekalo transport project` | `lekalo transport project [OPTIONS] --namespace <NAMESPACE> <PATH>` | [reference](transport-http.md) |
| `lekalo transport validate` | `lekalo transport validate [OPTIONS] <PATH>` | [reference](transport-http.md) |
| `lekalo update` | `lekalo update [OPTIONS]` | [reference](lockfile.md) |
| `lekalo validate` | `lekalo validate [OPTIONS]` | [reference](validation.md) |
| `lekalo verify` | `lekalo verify [OPTIONS] [PROGRAM_ARGS]...` | [reference](orchestration.md) |
| `lekalo waivers` | `lekalo waivers [OPTIONS] <COMMAND>` | [reference](waivers.md) |
| `lekalo waivers add` | `lekalo waivers add [OPTIONS] --id <ID> --target <TARGET> --owner <OWNER> --approver <APPROVER> --approval-ref <APPROVAL_REF> --reason <REASON> <SELECTOR>` | [reference](waivers.md) |
| `lekalo waivers audit` | `lekalo waivers audit [OPTIONS]` | [reference](waivers.md) |
| `lekalo waivers list` | `lekalo waivers list [OPTIONS]` | [reference](waivers.md) |
| `lekalo watch` | `lekalo watch [OPTIONS]` | [reference](cli.md) |
