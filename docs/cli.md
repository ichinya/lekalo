# Lekalo CLI foundation

Issue #3 introduces a target-neutral Rust core and the `lekalo` command-line
front end. The workspace is edition 2021, uses Cargo resolver 2, has an exact
MSRV of Rust 1.80.0, and carries product candidate version 0.1.31. The product
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
lekalo load [--project DIR] [--spans] [--ir]
lekalo lock [--check] [--offline] [--project DIR]
lekalo update --dry-run [--offline] [--project DIR]
lekalo update --apply sha256:PLAN_ID [--offline] [--project DIR]
lekalo migrate --to model/TARGET [--dry-run] [--project DIR]
lekalo migrate --rollback PLAN_ID [--project DIR]
lekalo compatibility
lekalo validate [--project DIR] [--module MODULE] [--strict]
lekalo graph show SYMBOL [--project DIR]
lekalo graph callers SYMBOL [--transitive] [--project DIR]
lekalo graph path FROM TO [--project DIR]
lekalo graph export [--format json] [--spans] [--project DIR]
lekalo generate --check [--project DIR]
lekalo generate --clean --dry-run [--project DIR]
lekalo generate --clean --confirm sha256:PLAN_ID [--project DIR]
lekalo inspect SYMBOL [--include SECTIONS] [--project DIR]
lekalo impact SYMBOL [--depth N] [--relation KIND] [--profile default|strict] [--project DIR]
lekalo impact --changed [--base REF] [--head REF] [--worktree] [--project DIR]
lekalo context SYMBOL --budget TOKENS [--spans] [--project DIR]
lekalo context --changed SYMBOLS --budget TOKENS [--spans] [--project DIR]
```

`--version`, `load`, `lock`, `update`, `migrate`, `compatibility`,
`validate`, `graph`, `effects`, `generate`, `inspect`, `impact`, and
`context` are implemented; none remains a recognized stub. `SYMBOL` is an
opaque string at this layer, and `TOKENS` is an unsigned integer. Semantic
ID rules, validation, and graph construction bind every implemented
command. Every implemented capability is bound by
dev.lekalo.semantic-ids@0.1.0 to use the validated semantic ID verbatim as
its canonical key; the implemented loader already consumes those IDs
verbatim when it normalizes references, and the graph, effects, context,
and impact engines resolve them through the kind-qualified node identity.

`--json` is global and may appear before or after a subcommand. Both
`lekalo --json --version` and `lekalo --version --json` select JSON output.
Root and per-command help remain clap help text rather than a domain failure.

### `lekalo load`

`load` parses the fixed `.yaml` homes of a validated project with strict
spanned JSON/YAML frontends, resolves module-ID imports (direct visibility
only, cycles and duplicates rejected), normalizes compact type sugar and
short references, and prints one deterministic canonical model. See
[loader.md](loader.md) for the normative contract, reason codes, and the
hermetic fixture suite under `tests/fixtures/loader/`. `--spans` adds the
sorted `sourceMap` sibling; it never alters the model bytes.

With `--ir`, `load` additionally compiles the loaded model into the typed,
deterministic Lekalo IR (`dev.lekalo.ir@0.1.0`) and prints the canonical IR
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
normative support policy, the 0.1.0 to 1.0.0 preconditions, and the
recovery contract. Unsupported contract versions exit 5 with the shared
`versioning.unsupported-version` reason.

`compatibility` prints the embedded registry projection (families in
fixed order `model`, `ir`, `protocol`); it performs no project or
adapter discovery.

## Exit and stream contract

| Exit | Status | Stream | Meaning |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | Successful output, help, or version |
| 1 | `invalid` | stderr | Malformed command-line syntax or usage |
| 3 | `denied` | stdout | Well-formed physical/policy denial (loader, structure) |
| 4 | `unsupported` | stdout | Recognized capability unavailable in this build |
| 5 | `unsupported-version` | stderr | Model contract version outside the exact 0.1.0/1.0.0 set |

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
diagnostic is one closed wire item (`lekalo/diagnostic/v1.0.0`, registry
version `1.0.0`) whose code, category, severity, message, and data fields are
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
      "schema_version": "lekalo/diagnostic/v1.0.0",
      "registry_version": "1.0.0",
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
      "schema_version": "lekalo/diagnostic/v1.0.0",
      "registry_version": "1.0.0",
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
  "version": "0.1.31"
}
```

The corresponding human lines are
`invalid error [LEK-CLI-001] cli.usage: Malformed command-line syntax.`,
`unsupported info [LEK-DIAG-001] core.capability-unavailable: The requested
capability is not implemented yet.`, and `lekalo 0.1.31`. Human and JSON
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
wraps the structured capsule (`lekalo/context/v1.0.0`) in the success
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
profile (`dev.lekalo.estimator.chars-4@1.0.0`, the offline deterministic
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
