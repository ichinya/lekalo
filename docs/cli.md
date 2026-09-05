# Lekalo CLI foundation

Issue #3 introduces a target-neutral Rust core and the `lekalo` command-line
front end. The workspace is edition 2021, uses Cargo resolver 2, has an exact
MSRV of Rust 1.80.0, and carries product candidate version 0.1.11. The product
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
lekalo inspect SYMBOL
lekalo impact SYMBOL
lekalo context SYMBOL --budget TOKENS
```

`--version`, `load`, `lock`, `update`, `migrate`, and `compatibility` are
implemented. The remaining three subcommands are
recognized stubs: valid syntax reaches the named capability and returns
`unsupported`. `SYMBOL` is an opaque string at this layer, and `TOKENS` is an
unsigned integer. Semantic ID rules, validation, graph construction, and
real inspect, impact, or context behavior belong to later issues.
When those capabilities are implemented they are bound by
dev.lekalo.semantic-ids@0.1.0 to use the validated semantic ID verbatim as
their canonical key; the implemented loader already consumes those IDs
verbatim when it normalizes references.

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

The stubs emit only 0/1/4. The loader emits 0/1/3/5 as specified in
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

Recognized unavailable capability (using `inspect` as the example):

```json
{
  "status": "unsupported",
  "capability": "inspect",
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
  "version": "0.1.11"
}
```

The corresponding human lines are
`invalid error [LEK-CLI-001] cli.usage: Malformed command-line syntax.`,
`unsupported info [LEK-DIAG-001] core.capability-unavailable: The requested
capability is not implemented yet.`, and `lekalo 0.1.11`. Human and JSON
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
scripts/test-diagnostic-contracts.mjs, and
scripts/test-validation-contracts.mjs through NODE_PATH,
and fails the job on any install, version, or gate failure.
