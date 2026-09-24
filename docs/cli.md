# Lekalo CLI foundation

Issue #3 introduces a target-neutral Rust core and the `lekalo` command-line
front end. The workspace is edition 2021, uses Cargo resolver 2, has an exact
MSRV of Rust 1.80.0, and carries product candidate version 0.2.16. The product
version is independent of every contract or model schema version.

The core crate owns the result contracts, the issue #7 loader
(YAML/JSON frontends, imports, normalization, canonical output), and the
issue #8 typed IR. The CLI crate
owns syntax and presentation through clap and serde_json. The foundation
stubs neither read the filesystem nor build IR, invoke a target, or contact
a provider; the loader and IR are pure: the loader is the one capability
with filesystem access and it never writes.

## Commands

```text
lekalo --version
lekalo init [--project-id ID] [--module MODULE] [--frontend yaml|json] [--target TARGET [--profile PROFILE]]
            [--editor-hints] [--project DIR] [--dry-run]
lekalo module new ID [--frontend yaml|json] [--project DIR] [--dry-run]
lekalo init --adopt [--target TARGET [--profile PROFILE]] [--project-id ID] [--project DIR] [--dry-run]
lekalo scan --target TARGET [--profile PROFILE] [--timeout-ms MS] [--project DIR] PROGRAM [ARGS]...
lekalo bindings list [--project DIR]
lekalo bindings propose [--project DIR]
lekalo bindings confirm PROPOSAL [--candidate NATIVE] [--project DIR]
lekalo bindings confirm --batch (--preview | --confirm sha256:PLAN_ID) [--project DIR]
lekalo bindings audit [--project DIR]
lekalo load [--project DIR] [--spans] [--ir]
lekalo lock [--check] [--offline] [--project DIR]
lekalo update --dry-run [--offline] [--project DIR]
lekalo update --apply sha256:PLAN_ID [--offline] [--project DIR]
lekalo migrate --to model/TARGET [--dry-run] [--project DIR]
lekalo migrate --rollback PLAN_ID [--project DIR]
lekalo compatibility
lekalo validate [--project DIR] [--module MODULE] [--strict]
lekalo expressions validate PATH [--builtin-support FILE]
lekalo expressions eval PATH --vectors FILE [--builtin-support FILE]
lekalo expressions render PATH --target node|php|go [--builtin-support FILE]
lekalo expressions diff BASE CANDIDATE
lekalo transport validate PATH [--project DIR] [--errors FILE] [--query-model FILE] [--strict]
lekalo transport inspect PATH --endpoint SYMBOL [--project DIR]
lekalo transport project PATH --namespace node|laravel|go|rust [--project DIR] [--errors FILE] [--query-model FILE]
lekalo transport diff BASE CANDIDATE
lekalo storage validate PATH [--project DIR]
lekalo storage project PATH --namespace postgres|laravel|mysql|mariadb
lekalo storage diff BASE CANDIDATE
lekalo storage plan BASE CANDIDATE [--confirm PLAN_ID]
lekalo storage introspect-check --projection PATH --evidence PATH --namespace postgres|laravel|mysql|mariadb
lekalo storage-profile validate PATH
lekalo storage-profile capabilities PATH
lekalo storage-profile portability BASE TARGET [--postgres-divergences]
lekalo storage-profile diff BASE CANDIDATE
lekalo openapi render PATH [--project DIR] [--errors FILE] [--query-model FILE]
             [--version 3.1|3.0] [--mode full|fragments]
lekalo openapi check PATH --transport ATTACHMENT [--project DIR] [--errors FILE]
             [--query-model FILE] [--ownership FILE]
lekalo openapi inspect PATH --endpoint SYMBOL [--project DIR] [--errors FILE] [--query-model FILE]
lekalo openapi diff BASE CANDIDATE [--project DIR] [--version 3.1|3.0]
lekalo graph show SYMBOL [--project DIR]
lekalo graph callers SYMBOL [--transitive] [--project DIR]
lekalo graph path FROM TO [--project DIR]
lekalo graph export [--format json] [--spans] [--project DIR]
lekalo generate --check [--locked] [--project DIR]
lekalo generate --clean --dry-run [--project DIR]
lekalo generate --clean --confirm sha256:PLAN_ID [--project DIR]
lekalo generate [--target TARGET]... [--module MODULE] [--dry-run] [--locked]
                -- PROGRAM [ARGS...] [--project DIR]
lekalo verify [--target TARGET]... [--module MODULE] [--changed] [--locked]
              [--trace PATH] [-- PROGRAM [ARGS...]] [--project DIR]
lekalo inspect SYMBOL [--include SECTIONS] [--project DIR]
lekalo impact SYMBOL [--depth N] [--relation KIND] [--profile default|strict] [--project DIR]
lekalo impact --changed [--base REF] [--head REF] [--worktree] [--project DIR]
lekalo context SYMBOL --budget TOKENS [--spans] [--project DIR]
lekalo context --changed SYMBOLS --budget TOKENS [--spans] [--project DIR]
lekalo contract update --declaration FILE [--project DIR]
lekalo contract check [--module MODULE] [--project DIR]
lekalo contract attach SYMBOL [--native-test IDS] [--gate IDS] [--project DIR]
lekalo contract support SYMBOL --kind KIND --path PATH [--digest SHA256] [--lifecycle LC]
    [--project DIR]
lekalo cache status [--project DIR]
lekalo doctor [--project DIR] [--trace PATH]... [--fix]
lekalo status [--project DIR]
lekalo readiness --phase model|implement|generate|verify|release [--project DIR] [--trace PATH]...
```

`init`, `init --adopt`, `module new`, `--version`, `load`, `lock`, `update`, `migrate`, `compatibility`,
`validate`, `graph`, `effects`, `generate`, `inspect`, `impact`, `context`, `cache`,
`doctor`, `status`, `readiness`, and `contract` are implemented; none remains a recognized stub. `SYMBOL` is an
opaque string at this layer, and `TOKENS` is an unsigned integer. Semantic
ID rules, validation, and graph construction bind every implemented
command. Every implemented capability is bound by
dev.lekalo.semantic-ids@0.2.16 to use the validated semantic ID verbatim as
its canonical key; the implemented loader already consumes those IDs
verbatim when it normalizes references, and the graph, effects, context,
and impact engines resolve them through the kind-qualified node identity.

`--json` is global and may appear before or after a subcommand. Both
`lekalo --json --version` and `lekalo --version --json` select JSON output.
Root and per-command help remain clap help text rather than a domain failure.

### `lekalo init`

`init` bootstraps a new greenfield Lekalo project (issue #97) in the
invocation directory: the minimal canonical skeleton (`lekalo/project.yaml`,
the first empty module `lekalo/modules/<module>/module.yaml`,
`lekalo/targets/<id>.yaml` only for an explicit `--target`, the managed
`/.lekalo/` `.gitignore` line, and `.vscode/settings.json` only for the
opt-in `--editor-hints`) — no application code, no installation, no
adapter execution, no lock. `--module` names the first module
(default `app`), `--frontend yaml|json` selects the document syntax
(default block YAML), and `--project-id` overrides the sanitized
directory-name derivation. `--dry-run` prints the machine-readable plan
without writing; a repeated init is idempotent and re-runs report
unchanged/added/conflicting artifacts; the `.gitignore` line merges
into user content, every other artifact never overwrites. The applied
skeleton passes the normal load-and-validate gate before the receipt.
The normative contract is [bootstrap.md](bootstrap.md) and
[ADR-0039](adr/0039-greenfield-init-bootstrap.md).

### `lekalo init --adopt`

`init --adopt` connects Lekalo to an existing repository (issue #38):
read-only detection with provenance and confidence, the minimal canonical
skeleton (`lekalo/project.yaml`, plus `lekalo/targets/<id>.yaml` only for
an explicit `--target`), atomic no-overwrite writes with journal and
rollback, and an in-process load+validate gate over the result. An
explicit `--profile` (requires `--target`, #28 token grammar) is
recorded in the target document and the receipt's `adapterProfile`;
never executed or checked against an adapter. `--dry-run`
prints the full plan and writes nothing; a repeated init is idempotent.
Observed modules stay observations in the receipt — the Model contract has
no module-mode field, so none is emitted. The normative
contract is [adopt.md](adopt.md) and [ADR-0028](adr/0028-init-adopt.md).

### `lekalo module new`

`module new ID` creates one additional empty module
(`lekalo/modules/<ID>/module.yaml`) in an existing project (issue #97),
sharing the bootstrap semantics: the closed one-segment id grammar,
`--dry-run`, no-overwrite conflicts, identical-bytes skip, the journaled
writer, and the post-write load-and-validate gate. Outside a Lekalo
project it is the normal root-not-found failure and nothing is written.

### `lekalo load`

`load` parses the fixed `.yaml` homes of a validated project with strict
spanned JSON/YAML frontends, resolves module-ID imports (direct visibility
only, cycles and duplicates rejected), normalizes compact type sugar and
short references, and prints one deterministic canonical model. See
[loader.md](loader.md) for the normative contract, reason codes, and the
hermetic fixture suite under `tests/fixtures/loader/`. `--spans` adds the
sorted `sourceMap` sibling; it never alters the model bytes.

With `--ir`, `load` additionally compiles the loaded model into the typed,
deterministic Lekalo IR (`dev.lekalo.ir@0.2.16`) and prints the canonical IR
bytes in place of the preserved model; `--spans` then appends the IR source
map. IR decode failures are `invalid` (exit 1, stderr) with the closed
`ir.*` reason codes. See [ir.md](ir.md) for the normative IR contract and
the fixture suite under `tests/fixtures/ir/`.

### `lekalo validate`

`validate` runs the issue #12 semantic validator over the compiled typed
IR: load, IR compile, then the phase-ordered semantic rules
(`lekalo validate` accepts `--project DIR`, `--module MODULE`, and
`--strict`). Loader, IR, and versioning failures pass through untouched;
semantic invalidity is `invalid` (exit 1, stderr) and a valid project is
`valid` (exit 0, stdout) carrying only warning/info diagnostics. The wire
envelope embeds the fixed-order `validation` object (profile, profile
version, pinned diagnostic registry release, optional module scope, enabled
rule count, severity counts). Rules, severities, and the two built-in
profiles are governed by [validation.md](validation.md) and
[ADR-0011](adr/0011-semantic-validation.md); `--strict` selects the strict
built-in profile, and `--module` scopes the report to one module while
keeping every error anywhere in the project.

Under `--strict`, `validate` additionally runs the issue #25
authorization review after semantic validation succeeds. Reference
integrity failures (unknown operations, fields, capabilities, roles,
compositions, or mappings; implication or composition cycles; unknown
fields or actors) are `invalid` (exit 1, stderr) in both profiles. A
protected effect without allow coverage, a stale `model_ref` digest, or
adapter mapping evidence below `full` is `denied` (exit 3, stdout) —
including a project whose protected effects have no
`lekalo/authorization.yaml` at all. The default profile is advisory and
never blocks; the closed vocabulary, evaluation semantics, and limits
are documented in [authorization.md](authorization.md) and
[ADR-0021](adr/0021-authorization.md).

### `lekalo lock` and `lekalo update`

`lock` creates the committed `lekalo.lock` for a validated project, or
checks an existing one and never updates it; `lock --check` is the headless
CI gate that refuses a missing lock. `update --dry-run` prints the
deterministic plan (`planId`, sorted add/remove/change entries) without
writing anything; `update --apply sha256:PLAN_ID` applies exactly that plan
under a byte-level compare-and-swap. Digest mismatches are integrity
denials (exit 3), distinct from validation failures (exit 1), unavailability
(exit 4), and unsupported versions (exit 5). See [lockfile.md](lockfile.md)
for the normative wire, digest domains, resolution order, and the
transaction contract, and [ADR-0009](adr/0009-lockfile.md) for the recorded
owner decisions.
### `lekalo migrate` and `lekalo compatibility`

`migrate` moves a project to a registered Model contract version
(`--to model/<canonical-version>` or a declared alias such as
`model/v1`) or rolls back one recorded migration (`--rollback`).
`--dry-run` prints the plan, semantic diff, and declared losses without
writing. Apply and rollback are journaled, verified transactions with
immutable backups under `.lekalo/cache/migrations/`; readers fail closed
while a journal exists. See [versioning.md](versioning.md) for the
normative support policy, the 0.2.16 to 0.2.16 preconditions, and the
recovery contract. Unsupported contract versions exit 5 with the shared
`versioning.unsupported-version` reason.

`compatibility` prints the embedded registry projection (families in
fixed order `model`, `ir`, `protocol`); it performs no project or
adapter discovery.


### `lekalo adapter test`

`adapter test` runs the issue #31 conformance battery against one
adapter executable through the confined target-protocol client: the
describe handshake and negotiation, capability declaration, deterministic
repeats, the dry-run plan and its apply, confinement, cancellation,
invalid-input handling, structured diagnostics, scenario
normalization, artifact evidence, and redaction. `--profile strict`
additionally requires the complete v1 operation surface; `--report
json|junit` prints the deterministic report document on stdout for
every completed run while the exit code stays verdict-owned
(0 pass, 1 feature failure, 3 security, 4 process/protocol). A
security or protocol failure is never compensated by passing feature
tests, and the verified badge names the exact protocol/IR versions
only. The normative contract is
[adapter-conformance.md](adapter-conformance.md) and
[ADR-0030](adr/0030-adapter-conformance.md).

## Storage (issue #69)

The `lekalo storage` group projects the storage-engine family over
the #65 storage projection. The core owns every decision; the binary
selects, renders, and maps exits on the accepted envelope.

- `lekalo storage profile --engine postgres [--version V]` — the
  owner-published version matrix, or one version's capability
  answers. An unpublished major answers `unsupported-version`.
- `lekalo storage validate PATH` — normalize one engine profile and
  print its canonical bytes.
- `lekalo storage ddl PROFILE --projection PATH` — the deterministic
  DDL document; the profile's `projectionRef` digest must bind the
  projection.
- `lekalo storage migrate-plan BASE CANDIDATE --profile PATH
  [--confirm PLAN_ID]` — the gated migration plan; a destructive
  plan answers the denied envelope (`LEK-SEN-009`) until the exact
  `planId` is named.
- `lekalo storage drift SCAN --projection PATH --profile PATH` —
  the declared-versus-observed comparison; the verdict stays data.
- `lekalo storage input PROFILE --projection PATH` — the one
  runtime-neutral engine input document every runtime consumer
  receives.
- `lekalo storage capabilities PROFILE --projection PATH
  [--requirements PATH] [--profile strict|permissive]` — the engine
  capability snapshot, optionally mapped against declared
  transaction-concurrency requirements.
- `lekalo storage conformance --profile PATH --projection PATH
  [--scan PATH] [--drifted PATH] [--input PATH] [--runtime PATH]…`
  — the closed fourteen-check battery; a skip is never a pass.

See [docs/storage-engine.md](storage-engine.md) and
[ADR-0042](adr/0042-postgres-storage-engine.md).

## Exit and stream contract

| Exit | Status | Stream | Meaning |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | Successful output, help, or version |
| 1 | `invalid` | stderr | Malformed command-line syntax or usage |
| 3 | `denied` | stdout | Well-formed physical/policy denial (loader, structure) |
| 4 | `unsupported` | stdout | Recognized capability unavailable in this build |
| 5 | `unsupported-version` | stderr | Model contract version outside the exact 0.2.16/0.2.16 set |

No subcommand remains a stub. The loader emits 0/1/3/5 as specified in
[loader.md](loader.md).

Loader failures are pretty-printed with two-space indentation and carry
structured reasons: `{"code", "path"?, "span"?, "data"?}` where `path` is a
logical project-relative POSIX path and spans point into the original
document bytes. Success stays one compact line.

clap parsing always uses its non-terminating parse path. Syntax failures are
mapped to exit 1, so clap's default exit 2 is never exposed. The stable failure
does not echo raw arguments or include localized parser detail.

## JSON wire shapes

JSON is UTF-8, pretty-printed with two-space indentation, and followed by
exactly one LF. Fields are emitted in the order shown. Output contains no ANSI
escapes, timestamps, absolute paths, current-directory values, or raw argv.

Since issue #11 the diagnostics array is authoritative and `reasonCodes` is
derived from it: the unique diagnostic ids in normalized order. Every
diagnostic is one closed wire item (`lekalo/diagnostic/v0.2.16`, registry
version `0.2.16`) whose code, category, severity, message, and data fields are
resolved from the embedded rule registry; see
[Diagnostics](diagnostics.md) and
[ADR-0010](adr/0010-diagnostics.md). Severity and category never compute the
exit; `DomainResult` alone owns status, stream, and exit.

Malformed syntax:

```json
{
  "status": "invalid",
  "diagnostics": [
    {
      "schema_version": "lekalo/diagnostic/v0.2.16",
      "registry_version": "0.2.16",
      "id": "cli.usage",
      "code": "LEK-CLI-001",
      "severity": "error",
      "category": "infrastructure",
      "message_id": "cli.usage",
      "message": "Malformed command-line syntax.",
      "data": {},
      "related_locations": [],
      "causes": [],
      "fixes": [],
      "metadata": {}
    }
  ],
  "reasonCodes": [
    "cli.usage"
  ]
}
```

Recognized unavailable capability (using `impact` as the example):

```json
{
  "status": "unsupported",
  "capability": "impact",
  "diagnostics": [
    {
      "schema_version": "lekalo/diagnostic/v0.2.16",
      "registry_version": "0.2.16",
      "id": "core.capability-unavailable",
      "code": "LEK-DIAG-001",
      "severity": "info",
      "category": "infrastructure",
      "message_id": "core.capability-unavailable",
      "message": "The requested capability is not implemented yet.",
      "data": {},
      "related_locations": [],
      "causes": [],
      "fixes": [],
      "metadata": {}
    }
  ],
  "reasonCodes": [
    "core.capability-unavailable"
  ]
}
```

Version:

```json
{
  "status": "valid",
  "version": "0.2.16"
}
```

The corresponding human lines are
`invalid error [LEK-CLI-001] cli.usage: Malformed command-line syntax.`,
`unsupported info [LEK-DIAG-001] core.capability-unavailable: The requested
capability is not implemented yet.`, and `lekalo 0.2.16`. Human and JSON
renderers consume the same `DomainResult`.

## Graph

Issue #13 projects the deterministic dependency graph of semantic symbols
through four thin subcommands. The core owns every decision (construction,
traversal, cycles, slices, limits); the binary only selects, renders, and
maps exits onto the accepted 0/1 envelope. Successes exit 0 on stdout;
unknown nodes (`graph.unknown-node`, `LEK-GRAPH-007`), unknown relation
filters (`graph.unknown-relation`), missing paths (`graph.path-not-found`),
bound exhaustion (`graph.traversal-limit`), fatal input
(`graph.input-invalid`), and forbidden cycles (`graph.cycle-forbidden`)
exit 1 on stderr.

```sh
lekalo graph show planner.focus_task
# entity planner.focus_task
#   dependency accepts operation:planner.focus_task -> type:planner.task_id (canonical)

lekalo graph callers planner.task_focused            # direct reverse view
lekalo graph callers planner.task --transitive       # bounded reverse closure
lekalo graph path planner.api_focus planner.task_focused
# path endpoint:planner.api_focus -> event:planner.task_focused (2 hops, canonical)

lekalo graph export --format json                    # canonical graph bytes
lekalo graph export --spans                          # + declaration-span sidecar
```

`graph export --json` wraps the canonical graph bytes in the success
envelope: `{"status":"valid","graph":{...}}`. The bytes are byte-identical
for the same IR across reruns, frontends, and platforms. `--spans` appends
the `spans` sidecar (declaration path and range per node, resolved through
the #8 source map) without touching the semantic bytes. The contract,
guarantees, and limits are documented in [docs/graph.md](graph.md) and
[ADR-0012](adr/0012-dependency-graph.md).

## Effects

Issue #14 projects the deterministic effect graph of operations through
three thin subcommands. The core owns every decision (declared
projection, evidence attachment, comparison, conflicts, limits); the
binary only selects, renders, and maps exits onto the accepted 0/1
envelope. Successes exit 0 on stdout; unknown operations or selectors
(`graph.unknown-node`, `LEK-GRAPH-007`), bound exhaustion
(`graph.traversal-limit`), and fatal input (`graph.input-invalid`) exit 1
on stderr.

```sh
lekalo effects show planner.focus_task
# effects operation:planner.focus_task : 2 edges
#   effect create operation:planner.focus_task -> canonical:planner.task (canonical)
#   effect emit-event operation:planner.focus_task -> canonical:planner.task_focused (canonical)

lekalo effects writers planner.task                   # reverse writers of an entity
lekalo effects writers planner.task --readers         # reverse readers instead
lekalo effects writers planner.task.title             # exact field scope
lekalo effects writers cache:vendor.app.key           # typed adapter resource

lekalo effects conflicts --changed planner.edit_task_cmd,planner.archive_task_cmd
# conflicts for 2 changed operations : N conflicts
#   delete-overlap planner.archive_task_cmd x planner.edit_task_cmd on canonical:planner.task
```

Success envelopes wrap the payload as `{"status":"valid","effects":{...}}`.
The change set of `conflicts --changed` is a typed handoff in the command
line; the tool never parses Git or infers changed symbols. The contract,
guarantees, and limits are documented in [docs/effect-graph.md](effect-graph.md)
and [ADR-0013](adr/0013-effect-graph.md).

## Diff

```sh
lekalo diff old/ new/ --profile source-consumer,wire-consumer
lekalo diff --base old/ new/
lekalo diff --format json --project old/ new/
```

`diff` compares two accepted, normalized project selections by meaning,
never by lines: both sides load through the accepted selection policy,
compile to the typed IR, and are projected into canonical semantic
projections keyed by stable semantic IDs and member keys. The answer is
one closed wire document with the equality verdict, the ordered change
records, direct reason codes, the affected-seed set, per-profile
decisions, and non-executable migration hints. Renames resolve only
through a declared `renamed_from` claim plus a matching same-identity
history edge (multi-hop chains within the recorded bound); replacements
and deletions resolve through their tombstones; a claim without its
registry edge classifies as conflicting, never as a guessed alias. Old
IDs are never silently reusable.

Exactly one base and one candidate are required: two positionals, or
`--base OLD` plus one positional. `--profiles` (repeatable terms,
comma-separated) selects among the five closed built-in profiles
(`source-consumer`, `wire-consumer`, `storage-consumer`,
`target-consumer`, `advisory`); without it only the profile-independent
facts are reported. `--format json` is the only v1 format and renders
the canonical bytes; the envelope adds exactly one trailing newline.
The diff verdict never computes an exit: a successful comparison is
exit `0`, invalid inputs are exit `1` with the registered `diff.*`
diagnostics. The command never parses Git (the `--base` selector is a
project directory; Git change detection belongs to #16), loads adapters,
or writes.

Success envelopes wrap the payload as `{"status":"valid","diff":{...}}`.
The contract, taxonomy, profiles, limits, and guarantees are documented
in [docs/semantic-diff.md](semantic-diff.md) and
[ADR-0019](adr/0019-semantic-diff.md).

## Context

Issue #17 projects the bounded context capsule for one symbol or one
explicitly supplied change set. The core owns every decision
(selection, estimation, truncation, projection); the binary only
selects, renders, and maps exits onto the accepted 0/1 envelope. The
human stream carries the agent-facing Markdown of the capsule; `--json`
wraps the structured capsule (`lekalo/context/v0.2.16`) in the success
envelope. Successes exit 0 on stdout; unknown symbols
(`graph.unknown-node`, `LEK-GRAPH-007`), out-of-range budgets and
over-bound scopes (`graph.input-invalid`), and manifest bound exhaustion
(`graph.traversal-limit`) exit 1 on stderr. An exhausted but in-range
budget is never an error: the capsule is emitted with exact truncation
metadata (`fits`, `minimumRequired`, per-fact manifest rows).

```sh
lekalo context planner.focus_task --budget 5000
lekalo context planner.focus_task --budget 5000 --spans
lekalo context --changed planner.focus_task,planner.edit_task_cmd --budget 12000 --json
```

Protected semantic facts (the root contract, its policies, effects,
direct dependencies, scenarios, public impact, and bindings) are typed
records, never collapsed into ambiguous prose; supporting context (type
cards, bounded closure) is ranked and may be excluded. The estimator
profile (`dev.lekalo.estimator.chars-4@0.2.16`, the offline deterministic
fallback) pins its identity, version, and digest into every capsule.
`--spans` attaches the declaration-span sidecar (logical
project-relative paths only) as the restricted raw-source evidence path;
the default never carries source bytes, secrets, `.env` content, or
absolute paths. The contract, guarantees, and limits are documented in
[docs/context.md](context.md) and [ADR-0018](adr/0018-context-capsules.md).

## Trace

Issue #22 validates, exports, and queries the neutral trace manifest
through three thin subcommands. The core owns every decision (wire and
semantics validation, the completeness policy, canonical bytes,
queries); the binary only reads the document, selects, renders, and maps
exits onto the accepted 0/1 envelope. Successes exit 0 on stdout;
wire/semantics violations (`graph.input-invalid`, `LEK-GRAPH-003`),
unknown query subjects (`graph.unknown-node`, `LEK-GRAPH-007`), and
unreadable files (`loader.io`) exit 1 on stderr.

```sh
lekalo trace validate tests/fixtures/trace/full.trace.json
# trace manifest planner-trace-full
#   completeness full (requirement-to-gate)
#   nodes 9; relations 9; gaps 0; uncovered sinks 0

lekalo trace export tests/fixtures/trace/full.trace.json > manifest.json
# human export is exactly the canonical bytes + LF

lekalo trace query tests/fixtures/trace/full.trace.json requirements-for:planner.focus_task
# requirement PLANNER-REQ-001 implements requirements.focus_task confirmed/exact
# requirement PLANNER-REQ-002 implements requirements.focus_task_archive confirmed/exact

lekalo trace query tests/fixtures/trace/partial.trace.json gaps
# gap missing-gate candidate anchor=symbol:planner.archive_task expected=hlv.gate.archive
# gap stale-revision stale anchor=symbol:planner.archive_task
```

`export --json` embeds the canonical bytes plus the `manifestDigest`
(`sha256:` over exactly those bytes). The closed query selectors are
`requirements-for:ID`, `symbols-for:ID`, `artifacts-for:ID`,
`tests-for:ID`, `gates-for:ID`, `diagnostics-for:ID`, and `gaps`. The
contract, guarantees, and limits are documented in
[docs/trace-manifest.md](trace-manifest.md) and
[ADR-0014](adr/0014-trace-manifest.md).

## Inspect

Issue #15 answers the single-symbol question through one thin subcommand.
The core owns every decision — selector grammar and resolution, the #6
alias registry, ambiguity bounds, section states, limits — and projects
one normalized object into the human and JSON views; the binary only
selects, renders, and maps exits onto the accepted 0/1 envelope.
Successes exit 0 on stdout; unknown symbols
(`inspect.symbol-unknown`/`inspect.short-name-unknown`, `LEK-INS-001` /
`LEK-INS-002`), ambiguity (`inspect.short-name-ambiguous`,
`LEK-INS-003`), and output-bound exhaustion (`inspect.output-limit`,
`LEK-INS-004`) exit 1 on stderr. A selector that violates the grammar
(`cli.usage`) never echoes the rejected input.

```sh
lekalo inspect planner.focus_task
lekalo inspect focus_task --json                     # safe short name
lekalo inspect planner.task --include bindings,scenarios
```

Success envelopes wrap the payload as
`{"status":"valid","inspect":{...}}` with the fixed section order
identity, contract, invariants, policies, effects, dependencies,
dependents, scenarios, bindings, ownership, portability, trace,
completeness, diagnostics. The contract, guarantees, and limits are
documented in [docs/inspect.md](inspect.md) and
[ADR-0014](adr/0014-inspect.md).

## Generate

`lekalo generate --check` verifies the derived ownership manifest at
`.lekalo/generated/manifests/ownership.json` against the exact lock
revision, the current Model/IR inputs, the locked adapter identity, the
exact artifact bytes, and the declared managed root. It is strictly
read-only and never spawns adapters. A clean or report-only check exits
0; generated drift, staleness, or absence and any orphan exit 1 on
stderr; integrity and path-policy refusals exit 3; a future manifest
discriminator exits 5.

```sh
lekalo generate --check
valid generate check clean manifest sha256:9d1f... lock sha256:2c40... \
  (artifacts 1, clean 1, stale 0, manual-drift 0, missing 0, orphan 0, reported 0)
```

`--clean --dry-run` prints the deterministic clean plan (orphan paths,
content digests, sizes) and its `planId` without touching a byte.
`--clean --confirm sha256:PLAN_ID` applies exactly that plan after full
revalidation, deleting only unchanged orphans inside
`.lekalo/generated/`; any change between preview and apply yields
`lock.source-changed` with zero deletes. A mutating clean without a
bound plan identity is refused with `lock.preview-required`. The
contract, verdict table, and clean rules are documented in
[docs/artifact-manifest.md](artifact-manifest.md) and
[ADR-0015](adr/0015-artifact-ownership-manifest.md).

```sh
lekalo generate --clean --dry-run
lekalo generate --clean --confirm sha256:973d6dd3ef84df5e286622a796e542f9dac20974047f21ec0a1a095501949734
generate applied plan sha256:973d... (-1)
```

### Zod schemas for the Node/TypeScript target (issue #45)

With the node-typescript adapter supplied, `generate` emits deterministic
Zod modules from the compiled IR — schema constants plus inferred types,
a shared runtime, a sorted barrel, and one canonical `.map.json` sidecar
per module (field path → semantic id, declaration byte ranges). The
pipeline is plan-first: `--dry-run` lists the whole write set and writes
nothing; the apply consumes the echoed plan id. The ownership manifest
records the modules as kind `schema`, the sidecars as kind `data`, and
the sidecar declaration ranges as source maps bound to the exact inputs
revision, so the `--check` gate above detects any tampering, staleness,
or orphaning of the generated set.

```sh
lekalo lock -- node adapters/node-typescript/adapter-zod.mjs \
  --lekalo-project-profile-json '{"id":"generate", …}'
lekalo generate --dry-run --target node-typescript -- node adapters/node-typescript/adapter-zod.mjs --lekalo-project-profile-json '{…}'
lekalo generate --target node-typescript -- node adapters/node-typescript/adapter-zod.mjs --lekalo-project-profile-json '{…}'
lekalo generate --check
```

The type-mapping table, the optional/nullable orthogonality rules, the
codegen policy document, and the unsupported-construct behaviour are
specified in [docs/zod-generation.md](zod-generation.md). Note the
generation artifact (`adapter-zod.mjs`) is a dedicated self-contained
script: the full compiler bundle exceeds the protocol's entry-digest
bound and is scanner-only.

## Requirements

The `lekalo requirements` handoff resolves one requirements attachment
(`lekalo/requirements/v0.2.16`) against its project: the read-only OpenSpec
provider walks `specs/**` and `changes/**`, projects the effective
requirement set, and pins every reference to an exact body revision. All
decisions live in the core; the binary selects, renders, and maps exits,
and nothing is ever written.

```sh
lekalo requirements validate tests/fixtures/requirements/planner/requirements.attachment.json --project tests/fixtures/requirements/planner
# requirements planner
#   requirements 3; references 3; fresh 3; stale 0; missing 0; conflict 0; coverage gaps 0; conflicts 0

lekalo requirements report ... > report.json     # canonical report bytes
lekalo requirements query ... coverage-gaps      # requirements no symbol links
lekalo requirements query ... impact             # changed/removed/renamed/conflict rows
lekalo requirements query ... symbol:planner.focus_task
lekalo requirements query ... requirement:openspec:planner.REQ-focus-task
lekalo requirements trace ... > trace.json       # neutral #22 trace projection
```

Exit protocol: `0` valid (validate: every reference fresh, no conflict),
`1` malformed attachment, unknown symbol or source, invalid provider tree,
unknown query selector or subject; `3` denied — stale, missing, or
conflicted references, any conflict in a resolved tree, or a Model
pin/project custody mismatch; `4` an absent provider tree that references
depend on. Human and JSON are projections of the same result; the report
and trace exports emit canonical bytes with pinned digests. See
[docs/requirements.md](requirements.md) and
[ADR-0026](adr/0026-requirements-traceability.md).

## NFR (issue #85)

The `lekalo nfr` handoff resolves NFR constraints
(`lekalo/nfr/v0.4.0`) against their measured evidence
(`lekalo/nfr-evidence/v0.4.0`): the gate, the derived report, the
closed queries, the neutral trace projection, the semantic diff, and
the impact synthesis. All decisions live in the core; nothing is ever
written. See [docs/nfr.md](nfr.md) and
[ADR-0042](adr/0042-nfr-constraints.md).

```sh
lekalo nfr validate tests/fixtures/nfr/planner/nfr.attachment.json \
  --evidence tests/fixtures/nfr/planner/nfr-evidence.staging-eu.json \
  --as-of 2026-09-30 --project tests/fixtures/nfr/planner
# nfr planner
#   constraints 4; satisfied 3; violated 0; unverified 1; stale 0; ...

lekalo nfr validate ... --strict                    # advisory failures deny too
lekalo nfr report ... > report.json                 # canonical report bytes + digest
lekalo nfr query ... unverified                     # constraints without current evidence
lekalo nfr query ... constraint:planner.nfr.api-focus-p95
lekalo nfr trace ... > trace.json                   # neutral #22 trace projection
lekalo nfr diff BASE CANDIDATE                      # breaking / non-breaking / policy-change
lekalo nfr impact CANDIDATE --base BASE             # changed constraints through impact
```

Exit protocol: `0` pass, `1` invalid (malformed attachment or evidence,
unknown selector or subject, malformed as-of date), `3` denied — a
mandatory constraint violated, unverified, stale, unsupported, or
conflicted (with `--strict` advisory violated/unverified/stale escalate
into the denied set), `4` an evidence file absent or unreadable. The
as-of date is required: expiry and validity evaluation stay
deterministic and clock-free.

## Impact

Issue #16 answers the change-radius question through one command with two
modes. The core owns every decision (typed changed-input handoff
validation, bounded reverse traversal, the depth-free mandatory-public
closure, risks, gates, canonical bytes); the binary only selects, renders,
and maps exits onto the accepted 0/1/3 envelope. Strict-profile denials
(`impact.gate-blocked`, `LEK-IMPACT-010`) exit 3 on stdout; selector,
changed-input, traversal, and output faults exit 1 on stderr.

```sh
lekalo impact planner.task --depth 3
# impact symbol (entity:planner.task)
#   direct 6 transitive 6 mandatory-public 0
#   risks 6
#   gates 9
#   evidence incomplete completeness incomplete

lekalo impact --changed --worktree                      # index/worktree vs HEAD
lekalo impact --changed --base main                     # committed base vs HEAD
lekalo impact --changed --base main --head feature      # two committed revisions
```

Success envelopes wrap the payload as `{"status":"valid","impact":{...}}`
with the canonical, digest-carrying impact object; `--profile strict`
denies with `{"status":"denied",...}` when a required gate rests on
unknown or stale evidence. The contract, guarantees, limits, and the
closed risk/gate vocabularies are documented in
[docs/impact.md](impact.md) and [ADR-0017](adr/0017-impact.md).
`--fix` renders the closed safe-fix recipe preview (advice only, nothing is
executed) and the optional `--trace PATH` manifests supply OpenSpec/HLV/
AI Factory gate evidence; unsupplied evidence degrades. The report is the
product: it exits 0 on stdout whenever it was produced, whatever verdict it
records. The contract, the closed check vocabulary, the verdict rule, and
the safe-fix recipes are documented in [docs/doctor.md](doctor.md) and
[ADR-0032](adr/0032-doctor-readiness.md).

```sh
lekalo doctor
# doctor degraded : 13 checks (12 ok, 1 degraded, 0 blocked)

lekalo status
# status ready : lock fresh, cache missing, bindings none-required, artifacts clean

lekalo readiness --phase generate
# readiness generate blocked : 13 checks (10 ok, 2 degraded, 1 blocked)
```

## Contract (contracted mode)

Issue #40 records, verifies, and governs AI-written implementation. The
model is primary for the public contract, effects, and invariants; the
target source is maintained code; the adapter checks conformance and may
generate support artifacts only. The thin subcommands hand every
decision to the core contracted engine; receipts are pretty two-space
JSON with fixed key order, and failures carry the registered
`contracted.*` rules:

```sh
lekalo contract update --declaration adapter-declaration.json
lekalo contract check --module planner
lekalo contract attach planner.focus_task --native-test "npm test -- focusTask"
lekalo contract support planner.focus_task --kind openapi --path .lekalo/generated/openapi/planner.json --digest sha256:<64 hex>
```

`contract update` merges one adapter declaration into the derived
registry at `.lekalo/import/contracted/registry.json`; bindings pin the
maintained source location and fingerprint, the typed signature claim,
and the declared-effect claim. `contract check` is the conformance
gate: it re-fingerprints the sources, recomputes the canonical
signatures and declared effects from the typed IR, and re-digests every
fingerprinted support artifact (exit 0 clean, exit 1 with registered
findings). The mode semantics, guarantees, and limits live in
[docs/contracted-mode.md](contracted-mode.md) and
[ADR-0034](adr/0034-contracted-mode.md).

## Scan and bindings (issue #42)

Issue #42 fills the observed registry on demand and adds the binding
workflow. The thin subcommands hand every decision to the core; the
contract, the ambiguity policy, and the freshness rules live in
[bindings.md](bindings.md) and [ADR-0035](adr/0035-bindings-registry.md):

```sh
lekalo scan --target node-typescript node tests/fixtures/bindings/ts-scanner.mjs
lekalo bindings list
lekalo bindings propose
lekalo bindings confirm prop-<64 lowercase hex>
lekalo bindings confirm prop-<64 lowercase hex> --candidate src/tasks.ts#createTask
lekalo bindings confirm --batch --preview
lekalo bindings confirm --batch --confirm sha256:<64 lowercase hex>
lekalo bindings audit
```

`scan` discovers and selects the adapter through the #28 seam, runs the
read-only `scan` exchange in the confined sandbox, and merges through the
#39 merge rules; a refused scan never writes the registry.
`bindings propose` derives one deterministic proposal per inferred
binding; an ambiguous proposal lists every candidate and picks none until
`--candidate` names one. `bindings confirm --batch` is the planned and
confirmed preview of every unambiguous proposal. `bindings audit` is the
staleness gate over symbols and native test bindings (exit 0 current,
exit 1 with `observed.stale-binding` per finding).

## Expressions (issue #66)

Issue #66 adds the typed-expression handoff: the thin
validate/eval/render/diff subcommands over the closed
`dev.lekalo.expressions@0.2.16` family. The core owns every decision
(static typing, reference evaluation with the injected clock,
projection, classification); the binary only selects, renders, and
maps exits. The contract, the closed grammar, the managed-mode
capability snapshots, and the cross-target fixtures live in
[expressions.md](expressions.md) and
[ADR-0040](adr/0040-typed-expressions.md):

```sh
lekalo expressions validate planner.json
lekalo expressions validate planner.json --builtin-support support.json
lekalo expressions eval planner.json --vectors vectors.json
lekalo expressions render planner.json --target node
lekalo expressions diff base.json candidate.json
```

`validate` emits the hermetic validity envelope with the capability
summary and the canonical SHA-256 digest. `eval` runs the shared
evaluation vectors through the deterministic reference evaluator;
every vector of a `now`-reading expression injects its clock.
`render` emits one complete self-contained Node, PHP, or Go program
that recomputes the vectors; a capability snapshot missing a required
token blocks managed mode (`expression.builtin-unsupported`).
`diff` classifies every changed path as breaking, non-breaking, or
policy-change; the verdict stays data, never an exit code.

 ## Development checks
```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

CI also checks the exact Rust 1.80.0 toolchain, builds and tests on Linux,
Windows, and macOS, and runs every accepted Node contract checker and suite on
Node.js 18 and 24. On both Node majors the contracts job additionally
provisions exact Ajv 8.17.1 under the runner temp directory, outside the
checkout, exposes it to scripts/test-model-ajv.mjs, scripts/test-lockfile-ajv.mjs,
scripts/test-diagnostic-contracts.mjs,
scripts/test-validation-contracts.mjs,
scripts/test-graph-contracts.mjs,
scripts/test-effect-graph-contracts.mjs,
scripts/test-artifact-manifest-contracts.mjs,
scripts/test-cache-contracts.mjs,
scripts/test-context-contracts.mjs,
scripts/test-semantic-diff-contracts.mjs,
and scripts/test-authorization-contracts.mjs through NODE_PATH,
and fails the job on any install, version, or gate failure.

## Storage and storage profile (issue #117)

The storage commands are thin, read-only handoffs to the core
storage-projection, storage-engine-profile, and storage-introspection
families. The documents are read at the given paths; every decision —
wire validation, semantic self-check, derivation, comparison, and the
drift check — lives in the core. Nothing is ever written and no
database connection flag exists anywhere: the introspection evidence is
produced by the runtime adapter, and the core only consumes it.

```text
lekalo storage validate PATH
lekalo storage project PATH --namespace postgres|laravel|mysql|mariadb
lekalo storage diff BASE CANDIDATE
lekalo storage introspect-check --projection PATH --evidence PATH --namespace NS
lekalo storage-profile validate PATH
lekalo storage-profile capabilities PATH
lekalo storage-profile portability BASE TARGET [--postgres-divergences]
lekalo storage-profile diff BASE CANDIDATE
```

Exit-code discipline matches `query-model diff`: success and typed
refusals stay on the accepted 0/1/3/4/5 envelope, and every verdict is
data in the JSON envelope, never a guessed repair.

## OpenAPI (issue #46)

The `openapi` commands project the #70 transport attachment into a
deterministic, validator-clean OpenAPI document; every decision lives
in `lekalo_core::openapi` (see [docs/openapi.md](openapi.md)).

`render` prints the canonical document bytes, their digest, and the
projection findings as warnings:

```json
{"status":"valid","openapi":{"projectId":"planner","openapiVersion":"3.1.0",
"mode":"full","canonicalDigest":"sha256:…","endpoints":6,"document":{…}}}
```

`check` is the checked mode: it binds a maintained document
(`x-lekalo-endpoint` first, `operationId` second), recomputes the
fragments, and reports per-pointer drift, manual collisions, and the
unbound-manual inventory. A conformant document returns
`{"status":"valid","openapiCheck":{"conformant":true,…}}`; drift
travels as `openapi.drift`, unresolved anchors as
`openapi.binding-unresolved`, and collisions as
`openapi.merge-conflict`.

`diff` renders the pointer-level view of the transport wire
compatibility classes over two same-family attachments:

```json
{"status":"valid","openapiDiff":{"equal":false,"breaking":1,"nonBreaking":0,
"policyChange":0,"wireConsumerBlocked":true,
"paths":[{"path":"endpoints/…/params","class":"breaking",
"pointers":["/paths/…/parameters"]}]}}
```

`inspect` returns one endpoint's rendered operation with its JSON
pointer. The generation composite of the node-typescript adapter claims
`generate.openapi` (partial) on `generate`, writes the document plus
the `ownership`/`map` sidecars under the policy path (default
`docs/openapi.yaml`), and verifies them on `verify`; `lekalo generate`
writes the canonical render evidence under
`.lekalo/cache/openapi/<project>.json`.
