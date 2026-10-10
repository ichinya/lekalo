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
| `contracts/diagnostic-registry.v0.8.0.json` | Union successor: the `mcp.*` family `LEK-MCP-001..011` plus every predecessor entry, byte-preserved. |

Tool output payloads stay byte-identical with the core contracts:
inspect `0.2.16`, impact `0.2.16`, context `0.2.16`, graph `0.2.16`,
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
`mcp.input-invalid` tool result. A changed-mode `lekalo_impact` call
refuses with `mcp.backend-unavailable` unless the trusted catalog
explicitly admits Git access (`git.changedMode`).

`lekalo_trace_export` accepts only an opaque `manifestRef` granted in the
catalog; the bytes are read through the bounded no-follow reader and
re-checked after parse. Changed inputs answer `mcp.snapshot-stale`, never
a stale success.

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
wrapped in a tool envelope.

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
result. `resources/subscribe` and `resources/unsubscribe` refuse
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
name of out-of-root bytes is never served. `lekalo_target_status` spawns
`git` only when the catalog admits `git.changedMode`; without admission
it serves the
no-git projection with typed unavailable Git facts. Secret-shaped tool
or resource output refuses with `mcp.sensitive-output` rather than
redacting contract members silently: the screening pass is
spelling-canonical — it folds ASCII case and admits `_`/`-` as pre-key
separators before matching, so `PASSWORD`, `db_password`,
`client_secret`, `aws_secret_access_key`, and `AUTHORIZATION:` are one
spelling each — and matches only high-confidence shapes (private-key
blocks, credential headers, assigned credential assignments), never
prose, field names, or identifiers. The private-absolute-path needles
are deliberately broader than a leading-slash match: the POSIX `/home/`,
`/root/`, and `/users/` needles match any `home`/`root`/`users` path
segment (so benign relative content like `app/home/page.tsx` also
refuses), and the colon-less `/users/` needle is what catches the
macOS absolute profile spelling (`/Users/alice/...`) — the accepted
fail-closed trade-off, recorded here and pinned by test.

## Bounds, deadlines, cancellation

Frame 256 KiB (the reader is itself capped, so a newline-free stream
refuses before allocating past the bound), response 1 MiB (ceiling
4 MiB, enforced at the single serialization chokepoint: an over-limit
response is substituted by the typed `mcp.output-limit` denial — never a
partial frame), deadline 30 s (ceiling 120 s), four admitted concurrent
calls (a timed-out worker keeps holding its slot until it finishes, so
the bound counts real in-flight work; the sequential read loop is the
queue), 128 semantic roots, page size 100 (ceiling 500), 256 watched
identities, nesting depth 64. Caller parameters and catalog
configuration may reduce a limit, never raise one. Deadlines answer the
typed `mcp.timeout` tool error while the client is still waiting;
abandoned workers cannot deliver a late success.
`notifications/cancelled` identifies the request by `requestId` (the
legacy `id` spelling is tolerated), suppresses the late result, and
never gets a reply. A reader thread feeds the dispatcher, so a
cancellation arriving while a call runs is observed live and its reply
is suppressed; the registration covers only outstanding ids, so a
reused id is never poisoned (review r2 F6), and a timed-out or
cancelled worker keeps holding its slot until it finishes (the bound
counts real in-flight work, F14). Cooperative mid-run interruption of a
pure core traversal is deferred (see the deferral record below); the
deadline and the concurrency bound cap the abandoned work.

Duplicate JSON member names refuse the inbound frame before any map
insertion (`mcp.duplicate-key`, last-key-wins is never an admission
path), and a request other than `initialize`/`ping` before
`notifications/initialized` is refused (`mcp.lifecycle-not-initialized`).
`initialize` is single-shot: a second `initialize` after the handshake
was answered is refused (`-32600 mcp.lifecycle-already-initialized`).
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
cargo test --locked -p lekalo-core --test mcp
cargo test --locked -p lekalo-cli --test mcp
cargo test --locked -p lekalo-cli --test mcp_review
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
CI treats that as a failure, never silently accepts.

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
