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
expressed. Structural violations are JSON-RPC `-32602`; cross-field
semantic violations are the typed `mcp.input-invalid` tool result.
A changed-mode `lekalo_impact` call refuses with `mcp.backend-unavailable`
unless the trusted catalog explicitly admits Git access
(`git.changedMode`).

`lekalo_trace_export` accepts only an opaque `manifestRef` granted in the
catalog; the bytes are read through the bounded no-follow reader and
re-checked after parse. Changed inputs answer `mcp.snapshot-stale`, never
a stale success.

## Resources

`lekalo://PROJECT/project-summary`, `lekalo://PROJECT/modules`,
`lekalo://PROJECT/symbol/{symbol}`, `lekalo://PROJECT/targets`,
`lekalo://PROJECT/diagnostics` — plus group-qualified identities
`lekalo://PROJECT/g/GROUP/...`. Resource content is `application/json`
carrying the same closed versioned projections the tools emit. Raw
source, dotfiles, lock bytes, and absolute private roots are never
resource content.

## Authorization, scope and privacy

Effective authorization is **launcher catalog grants ∩ core read policy
∩ group allow-list**. Roots resolve once at startup; requests refer only
to opaque identities. A group is a server-local module allow-list — a
disclosure constraint, not a claim that module filtering alone limits
reads: any operation whose required module closure leaves the allow-list
refuses before reading (`mcp.scope-denied`), and full-scope operations
(graph export, validate, target status, changed impact, trace export)
refuse under a group instead of fabricating a narrowed result.

The pure policy refuses traversal, encoded traversal, drive/UNC/URL
spellings, NUL/ADS, dot segments, and known secret locations before any
open; the filesystem seam adds symlink/junction/reparse-point checks and
bounded reads. Secrets in output refuse with `mcp.sensitive-output`
rather than redacting contract members silently.

## Bounds, deadlines, cancellation

Frame 256 KiB, response 1 MiB (ceiling 4 MiB), deadline 30 s (ceiling
120 s), four admitted concurrent calls, 16 queued requests, 128 semantic
roots, page size 100 (ceiling 500), 256 watched identities, nesting depth
64. Caller parameters and catalog configuration may reduce a limit,
never raise one. An over-limit response answers with the typed
`mcp.output-limit` denial — never a partial frame. Deadlines answer the
typed `mcp.timeout` tool error while the client is still waiting;
abandoned workers cannot deliver a late success. `notifications/cancelled`
suppresses the late result and never gets a reply.

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
```

The stdio gate drives the real binary over a real pipe and validates
every `structuredContent` with pinned Ajv against the closed output
schemas, then proves the served root byte-identical. The host gate
qualifies a real MCP host/SDK connection; when no qualified host is
provisioned it reports `unavailable` (exit 2) and CI treats that as a
failure, never silently accepts.

## Connecting an AI Factory extension

`examples/mcp/ai-factory-extension.json` is the registration template:
it declares the stdio command (`lekalo mcp serve`), the pinned protocol
profile, and the project grant. Registration is a separate opt-in change
of the host's own extension manifest — Lekalo never installs, edits, or
negotiates with an extension on its own. A missing, disabled, or
crashed server never affects the ordinary CLI: no `lekalo` command
startup path touches MCP.
