# Read-only MCP server (issue #33)

Lekalo ships a read-only MCP (Model Context Protocol) server that gives
AI agents programmatic access to the semantic layer without parsing human
CLI output and without repository-wide filesystem access. The transport
is stdio-first, pinned to the **2025-11-25** protocol profile, and
read-only unconditionally: write and generate tools do not exist on this
surface and require a separately authorized design stage.

```
lekalo mcp describe --json
lekalo mcp serve --stdio --project DIR
lekalo mcp serve --stdio --project DIR --catalog-config mcp.local.json --group planner-review
lekalo mcp serve --stdio --project DIR --timeout-ms 30000 --max-response-bytes 1048576
```

`lekalo mcp describe` emits the embedded deterministic capability
manifest (`lekalo/mcp-capabilities/v0.8.0`) without reading a project or
starting a server. `lekalo mcp serve` requires explicit project
selection, owns stdout exclusively as JSON-RPC (one UTF-8 frame per LF),
and rejects `--json` with the ordinary pre-server usage diagnostics. The
`--catalog-config FILE` operand is a trusted launcher input, never a
remotely supplied tool argument.

## Contracts

New artifacts take the product version of the implementation commit;
every preserved upstream contract keeps its frozen version.

| Artifact | Identity / purpose |
| --- | --- |
| `contracts/mcp-capabilities.schema.v0.8.0.json` | `dev.lekalo.mcp-capabilities@0.8.0`, `lekalo/mcp-capabilities/v0.8.0`; the seven tools, five resource families, protocol pin, limits. |
| `contracts/mcp-server-config.schema.v0.8.0.json` | `dev.lekalo.mcp-server-config@0.8.0`; trusted catalog (projects/groups/manifests/git admission/limits). |
| `contracts/mcp-tool-<name>-input.schema.v0.8.0.json` | Seven closed request grammars (`inspect`, `impact`, `context`, `graph`, `validate`, `target-status`, `trace-export`). |
| `contracts/mcp-tool-<name>-output.schema.v0.8.0.json` | Seven closed result envelopes: `status: valid` carries the preserved core receipt, every other status a typed `failure` branch. |
| `contracts/mcp-resource.schema.v0.8.0.json` | `dev.lekalo.mcp-resource@0.8.0`; the five resource families. |
| `contracts/mcp-graph-query.schema.v0.8.0.json` | Describing schema for the frozen `graph show`/`callers`/`path` receipts. |
| `contracts/diagnostic-registry.v0.8.0.json` | M8 grand union: every landed M8 family (`format.*`, `adoption.*`, `portability.*`, `worklog-changelog.*`, `managed.*`, `module-api.*`, `tasklink.*`) plus `mcp.*` (`LEK-MCP-001..011`) and every frozen predecessor entry; all family Rust accessors resolve this same 0.8.0 identity, and the merge-time union guard (`scripts/check-registry-union.mjs`) refuses a registry that drops a landed family. |

Tool output payloads stay byte-identical with the core contracts:
inspect `0.8.0` (the merged #52 successor), impact `0.2.16`, context `0.2.16`, graph `0.2.16`,
validation report `0.6.4`, doctor `0.3.2`, trace manifest `0.2.16`. Each
`result` member splices the exact core receipt under its pinned
discriminator; graph receipts ride under an explicit `mode` tag with no
injected discriminator.

## Tools

Exactly seven read tools exist; the catalog is fixed at compile time and
`tools/list` is the authoritative surface:

`lekalo_inspect`, `lekalo_impact`, `lekalo_context`, `lekalo_graph`,
`lekalo_validate`, `lekalo_target_status`, `lekalo_trace_export`.

Every tool requires `projectId` (an opaque catalog id) and accepts an
optional `groupId` and `timeoutMs`. Operands are semantic ids and
enumerated selectors only: no root path, executable name, argv,
environment variable, export destination, or write flag can be
expressed. A comma is never part of a symbol operand — the changed-list
separator is a CLI spelling, so `changedSymbols` is a typed array whose
members cannot re-split into an unchecked set. Structural violations are
JSON-RPC `-32602`; cross-field semantic violations are the typed
`mcp.input-invalid` tool result. Catalog/module/target identifiers are at
most 63 ASCII characters; `baseRef`/`headRef` are bounded to 128 Unicode
code points (the same length language as the input schemas). A changed-mode `lekalo_impact` call
refuses with `mcp.backend-unavailable` unless the trusted catalog
explicitly admits Git access (`git.changedMode`).

`lekalo_trace_export` accepts only an opaque `manifestRef` granted in the
catalog; the bytes are read through the bounded no-follow reader and
re-checked after parse. Changed inputs answer `mcp.snapshot-stale`, never
a stale success. `lekalo_target_status` uses the status report for the
lock selector and the real doctor-kind report for inventory, trust and
profiles; it never fabricates an empty `ready` panel (every selected
receipt retains at least one check, as the pinned output schema requires).

## Resources

`lekalo://PROJECT/summary`, `lekalo://PROJECT/modules`,
`lekalo://PROJECT/symbol/{symbol}`, `lekalo://PROJECT/targets`,
`lekalo://PROJECT/diagnostics` — plus group-qualified identities
`lekalo://PROJECT/g/GROUP/...`. Resource content is `application/json`
carrying the same closed versioned projections the tools emit, and every
envelope's `kind` member uses the contract enum spelling
(`project-summary`, `module-list`, `symbol-metadata`, `target-status`,
`diagnostics`); the URI family keys are the served spellings above
(`summary`, never the contract kind `project-summary` — the parser
accepts the alias, but `resources/list` and every emitted `uri` use the
canonical key). Raw source, dotfiles, lock bytes, and absolute private
roots are never resource content. `resources/list` pages through an
opaque snapshot-bound cursor: a cursor from a different enumeration
snapshot is refused on the method's own error channel (`-32602
mcp.snapshot-stale`), never answered from a mixed snapshot and never
wrapped in a tool envelope. `resources/read` refusals likewise use
that method's own JSON-RPC error channel with a registered `mcp.*`
token; a resource failure never claims to be a `lekalo_inspect` tool
result or carries a foreign tool schema identity.

## Authorization, scope and privacy

Effective authorization is **launcher catalog grants ∩ core read policy
∩ group allow-list ∩ launch group**. Roots resolve once at startup;
requests refer only to opaque identities. The launcher's `--group` is
the enforced disclosure ceiling of both surfaces: a client-supplied
`groupId` or a group-qualified resource URI must name exactly the launch
group — anything else is a typed widening refusal, never a silent
substitution — and an unqualified identity resolves under it. A group is
a server-local module allow-list — a disclosure constraint, not a claim
that module filtering alone limits reads: any operation whose required
module closure leaves the allow-list refuses before reading
(`mcp.scope-denied`), and full-scope operations (project summaries,
module lists, graph export, validate, target status, changed impact,
trace export) refuse under a group instead of fabricating a narrowed
result. The post-run closure checks the complete failure diagnostics
and reason codes as well as successful payloads: a domain-invalid
response cannot disclose an excluded module's identity by failing.
`resources/subscribe` and `resources/unsubscribe` refuse
(`-32601 subscriptions-not-implemented`): no watcher ships, so an inert
registration — one the server could never honor — is never acknowledged,
whatever the identity (review r2 F3/codex 13).

The pure policy refuses traversal, encoded traversal, drive/UNC/URL
spellings, NUL/ADS, dot segments, and known secret locations before any
open; the filesystem seam adds a component-wise link walk (ancestor
symlinks/junctions/reparse points refuse at every depth, `O_NOFOLLOW` on
the final Unix open) plus a re-verified chain after the read (TOCTOU
guard), and bounded reads — and hardlink aliases refuse on both
platforms through the link count (Unix `nlink`; Windows
`GetFileInformationByHandle`'s `nNumberOfLinks`), so an in-root second
name of out-of-root bytes is never served. The same alias policy also
applies to the model loader (`project_fs::read_file_opt`) and the
`lekalo.lock` snapshot leg, not only trace manifests; governed cache,
bindings and artifact inputs are identity-pinned before/after a status
projection. The finite doctor dependencies
`.lekalo/adapters/inventory.json` and
`.lekalo/adapters/evidence/revocations.json` use the same bounded,
no-follow, single-owner admission: presence, exact content and physical
file identity enter the pre/post snapshot, and the doctor consumes the
admitted bytes instead of reopening their paths. `lekalo_target_status`
spawns `git` only when the catalog admits `git.changedMode`; without
admission it serves typed unavailable Git facts. Both Git-read adapters
force `GIT_OPTIONAL_LOCKS=0`, overriding launcher settings: a read grant
never permits an implicit `.git/index` refresh write. The bounded Git
seam clears inherited Git configuration/environment, disables fsmonitor,
hooks, pagers, external diff/textconv, submodule recursion and lazy fetch,
and denies all transport protocols. It discovers filter names through
Git's non-executing config reader and disables clean/smudge/process
filters before a fact read; repository-configured helpers cannot execute.
The admitted snapshot includes the exact Git state, commit and dirty
facts consumed by status/doctor and the targets resource. Their post-read
recheck refuses drift as `mcp.snapshot-stale`; without a Git grant these
pins are typed unavailable and no Git process starts. Secret-shaped tool
or resource output refuses with `mcp.sensitive-output` rather than
redacting contract members silently: the screening pass is
spelling-canonical — it folds ASCII case and a closed set of common
Cyrillic/Greek lookalikes, removes `_`/`-` symmetrically, and recognizes
plain, single and JSON-escaped quoted assignments. Value delimiters
must match the opening quote's kind and escape context; neither a leading
opposite quote nor an empty quoted prefix followed by material can
disguise a nonempty credential as empty. Credential stems
anywhere in assigned identifier keys refuse, including plurals and
unlisted compounds (`secrets`, `mysecret`, `secret_value`,
`aws_access_key_id`) and compound `key` names in snake/kebab/camel and
unseparated spellings. Non-empty arrays/objects on a credential key
refuse just like scalar values. Empty strings/arrays/objects carry no
material; the quoted JSON `"tokens": <unsigned integer>` budget member
is the only numeric-count exception, not numeric passwords or
string-valued tokens. The explicit negated pseudo-key `NotAPassword=x`
and bare JSON `key` member remain benign; other benign compounds may
refuse conservatively. This is not exhaustive Unicode normalization or
a promise to identify every possible secret encoding.

The pass also matches private-key blocks, credential headers, and bare
`Bearer <token>` schemes. Drive-letter (`D:/...`), UNC
(`\\server\share\...`, `//server/share/...`), home-relative (`~/...`),
and boundary-rooted multi-segment POSIX paths refuse by shape, not a
five-segment vocabulary. Numeric-index JSON pointers such as
`/definitions/20` stay available, but absolute API routes may refuse
because their bytes are indistinguishable from host paths. Common
private dotfile spellings (`.env`, `.ssh`, `.aws`, `.gnupg`, `.config`,
`.netrc`, `.npmrc`, `.pypirc`, `.git-credentials`, `.keystore`)
refuse anywhere, including Windows separator spellings. Hidden-file
components also refuse as a class, including `.pgpass`, `.my.cnf`,
`.kube/`, `.docker/`, and unlisted peers; dots within ordinary identifiers,
filenames and versions stay benign. UNC and POSIX matchers treat every
non-segment byte as a boundary, including code spans, angle brackets and
pipes. Filesystem schemes (`file:/...`, `file:///...`) are not benign URL
exceptions. Decoded JSON strings are screened too: escaped newlines/tabs
cannot hide a path.
Relative project paths and benign URLs (`https://...`, `lekalo://...`)
remain available except for the
established broader `/home/`, `/root/`, `/users/`, and dotfile needles:
`app/home/page.tsx` can refuse too. These are accepted fail-closed
availability trade-offs, recorded here and pinned by tests. The entire
bounded output is scanned so long keys/paths and UTF-8 spellings cannot
straddle a fixed-overlap chunk boundary.

## Bounds, deadlines, cancellation

Frame 256 KiB (the reader is itself capped, so a newline-free stream
refuses before allocating past the bound), response 1 MiB (ceiling
4 MiB, enforced at the single serialization chokepoint: an over-limit
response is substituted by the typed `mcp.output-limit` denial — never a
partial frame), deadline 30 s (ceiling 120 s), four admitted concurrent
calls (a timed-out worker keeps holding its slot until it finishes, so
the bound counts real in-flight work), **16 queued requests** (excess
frames refuse with `mcp.request-limit` on their own ids while cancel and
EOF retain priority), 128 semantic roots, page size 100 (ceiling 500), 256 watched
identities, nesting depth 64. Request ids are limited to 512 **encoded**
JSON bytes and must reserve room for a correlated output-limit refusal
inside the effective response cap. An unadmitted id gets a bounded null-id
`-32600 mcp.request-limit` refusal before execution; the session remains
usable unless even that compact response cannot fit. Integral numeric
spellings such as `5000.0` and `3e0` follow the input schemas' integer
value semantics; fractional/out-of-range values refuse. Caller parameters
and catalog configuration may reduce a limit, never raise one. Deadlines answer the
typed `mcp.timeout` tool error while the client is still waiting;
abandoned workers cannot deliver a late success.
`notifications/cancelled` identifies the request by `requestId` (the
legacy `id` spelling is tolerated), suppresses the late result, and
never gets a reply. A reader thread feeds the dispatcher, so a
cancellation arriving while a call runs is observed live and its reply
is suppressed; a queued call is cancellable too, before it executes.
Only valid JSON-RPC 2.0 notification envelopes may mutate cancellation
or handshake state; malformed cancellation frames refuse as invalid
requests and never act. Unknown and completed ids are not registered,
so a reused id is never poisoned. A timed-out/cancelled worker holds its
slot until it finishes (the bound counts real in-flight work); its
cooperative guard stops new admitted reads and Git child spawns after
abandonment. Both doctor and changed-impact Git children use the same
five-second bounded wait, concurrent pipe draining (4 MiB per stream),
and process-tree cleanup (Unix process groups / Windows jobs) on
cancellation, deadline or output overflow. Children are terminated and
reaped before the worker releases its slot. Pure core traversal
mid-computation remains deferred (below).

Duplicate JSON member names refuse the inbound frame before any map
insertion (`mcp.duplicate-key`, last-key-wins is never an admission
path), and a request other than `initialize`/`ping` before
`notifications/initialized` is refused (`mcp.lifecycle-not-initialized`).
`initialize` requires object params with a protocol version, capabilities
object, and client name/version before it can advance the handshake;
garbage or missing params refuse with `-32602`. It is single-shot: a
second valid `initialize` after the handshake was answered is refused
(`-32600 mcp.lifecycle-already-initialized`).
Only implemented capabilities are advertised: the initialize answer
reports `resources: {subscribe: false, listChanged: false}`.

### Deferred against the issue #33 requirement

- **Change notifications** ("change notifications after model update"):
  the bounded admitted-input watcher, invalidation, and coalesced
  `notifications/resources/updated` / `list_changed` emissions are
  deferred. The capability metadata is truthful (`subscribe: false`,
  `listChanged: false`), `resources/subscribe` refuses instead of
  acknowledging inert registrations, and the resource `revision` already
  binds to the admitted-input identity so a later watcher can diff it.
- **Cooperative mid-run cancellation**: a cancellation is observed live
  and its reply suppressed, but a pure core traversal is not interrupted
  mid-computation; the deadline and the concurrency bound cap the
  abandoned work (the timed-out worker keeps its slot until completion).
  At CLI read/spawn checkpoints, cancelled or timed-out workers stop
  starting new reads/processes; already-running Git children and their
  supervised descendants are polled, terminated and reaped by both Git
  adapters. This is not full process isolation
  or guaranteed interruption of arbitrary pure core traversal.

## Diagnostics

The registered `mcp.*` family (`LEK-MCP-001..011` in
`dev.lekalo.diagnostic-registry@0.8.0`): input-invalid, scope-denied,
sensitive-output, request-limit, output-limit, timeout, cancelled,
contract-unsupported, snapshot-stale, backend-unavailable,
result-invalid. Transport diagnostics supplement original core
diagnostics and never rename them.

## Gates

```text
node scripts/test-mcp-contracts.mjs
node scripts/test-mcp-stdio.mjs
node scripts/test-mcp-boundaries.mjs
node scripts/test-mcp-host.mjs
node scripts/test-mcp-union.mjs
node scripts/test-classification-contracts.mjs
cargo test --locked -p lekalo-core --test mcp
cargo test --locked -p lekalo-cli --test mcp
cargo test --locked -p lekalo-cli --test mcp_review
cargo test --locked -p lekalo-cli --test mcp_r5
cargo test --locked -p lekalo-cli --test mcp_r7
cargo test --locked -p lekalo-cli --test mcp_r8
cargo test --locked -p lekalo-cli --bin lekalo mcp::work::tests
```

The stdio gate drives the real binary over a real pipe and validates
every `structuredContent` with pinned Ajv against the closed output
schemas, then proves the served root byte-identical, Ajv-validates a
live resource envelope per family, and exercises the launch-group
ceiling. The review gate (`mcp_review`) drives the closed review-round
findings as wire behavior: duplicate keys, ancestor links, the output-
limit substitution, subscribe authorization, and the launch-group
narrowing. The host gate qualifies a real MCP host/SDK connection; when
no qualified host is provisioned it reports `unavailable` (exit 2) and
CI treats that as a failure, never silently accepts. The r7 wire tests
also inventory a Git-enabled session (including index bytes/mtime),
refuse aliased adapter state, and pin trust-state changes; the worker
unit tests prove child/descendant termination and bounded pipe capture.
The r8 tests prove hostile-helper controls really execute outside MCP but
not inside it, pin Git-only changes, refuse boundary/dotfile disclosures
on both surfaces, and exercise numeric spellings and request-id budgets.
A deterministic evaluation checkpoint also proves mid-read Git drift
returns snapshot-stale rather than a mixed candidate. Unix no-follow flags
come from the target-aware rustix constants with a symlink-open regression.
The union matrix removes each landed family against all eleven family
gates, and the classification gate resolves every local schema reference,
including otherwise-unused definitions.

## Connecting an AI Factory extension

`examples/mcp/ai-factory-extension.json` is the registration template:
it declares the stdio command, the pinned protocol profile, and the
project grant. The launch shape is qualified end-to-end by
`scripts/test-mcp-host.mjs` — a built `lekalo` binary, `mcp serve
--stdio --project <workspace-relative project directory>` with the
server working directory at the workspace root and `LEKALO_PROJECT`
cleared completes initialize, `tools/list`, one `tools/call
lekalo_context`, and one `resources/read` over the official SDK.
The project selector must stay **relative to the server's working
directory** (`.` for a workspace-root project, `path/to/project` for a
nested one): the loader's selection policy refuses absolute selectors,
so a host that substitutes an absolute `${workspaceRoot}` into
`--project` gets `structure.selection-absolute` on the first call. Host
operators whose extension runtime cannot set a working directory should
register with the workspace root as cwd and the relative project
spelling. Registration is a separate opt-in change of the host's own
extension manifest — Lekalo never installs, edits, or negotiates with an
extension on its own. A missing, disabled, or crashed server never
affects the ordinary CLI: no `lekalo` command startup path touches MCP.
