# Issue #33: read-only MCP server research

Research only, 2026-10-06. Acceptance authority: [live issue #33](https://github.com/ichinya/lekalo/issues/33), read using `gh issue view 33 --repo ichinya/lekalo --json title,body,labels`. Its parent [#1](https://github.com/ichinya/lekalo/issues/1) and dependencies [#11](https://github.com/ichinya/lekalo/issues/11), [#13](https://github.com/ichinya/lekalo/issues/13), [#14](https://github.com/ichinya/lekalo/issues/14), [#15](https://github.com/ichinya/lekalo/issues/15), [#16](https://github.com/ichinya/lekalo/issues/16), [#17](https://github.com/ichinya/lekalo/issues/17) were read for context; all six dependencies are closed. Their historical example syntax does not replace current contracts.

Checkout: branch `ichinya/m8-issue-33`, initial HEAD `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, supplied fresh `ichinya/M8` base, initially clean. `Cargo.toml` still records product `0.6.5` and Rust MSRV `1.80.0`; the assigned implementation target is **0.8.0**. This documentation commit does not bump Cargo or publish contracts. House format references: `docs/m7/issue-34-research.md`, `issue-35-research.md`, and `issue-103-research.md`; version authority: `docs/versioning.md`.

**Finding:** semantic read services already exist; the missing work is the MCP transport, explicit authorization/scoping, safe resource projections, bounded execution, and host qualification. Wrapping every existing CLI invocation without examining its read/write behavior would not satisfy #33. In particular, cached graph/validate paths can write runtime cache, and `run_trace` currently reads a supplied path using `std::fs::read` without project confinement.

All new paths, APIs, flags, codes, schema members and gates below are **proposals**. Existing-source facts are labeled separately. Implementation, MCP host acceptance and external changes remain future work.

## Existing-surface inventory

| Existing owner / inspected anchors | Current behavior | Consequence for #33 |
| --- | --- | --- |
| `crates/lekalo-cli/src/main.rs`, `Commands`, `ImpactArgs`, `GraphCommands`, `TraceCommands` | Inspect, impact, context, validate, graph, doctor/status and trace export exist; no MCP command or server. Global `--json` selects structured rendering, global `--no-cache` bypasses cache reads/writes/locks. | Add a separate server entrypoint and reuse typed services/result construction; never parse human output. |
| `crates/lekalo-core/src/inspect/{mod.rs,version.rs}`: `InspectRequest`, `Include`, `validate_selector`, `run` | Pure projection over compilation, dependency graph and effects; exact/short-name resolution and mandatory sections; `lekalo/inspect/v0.2.16`. | Preserve unknown-vs-ambiguous diagnostics and optional bindings/scenarios. |
| `crates/lekalo-core/src/impact/{analyzer.rs,request.rs,version.rs}`, CLI `git_input.rs` | Typed impact analysis and bounded requests; CLI owns changed-input Git resolution; `lekalo/impact/v0.2.16`. | Symbol and changed modes need distinct validated operands; a Git selector is not a semantic-ID list. |
| `crates/lekalo-core/src/context/{mod.rs,version.rs}`: `CapsuleScope`, `plan`, `Capsule` | Single-symbol or explicitly supplied changed IDs; structured capsule, included/excluded items, `fits`, minimum required tokens; `lekalo/context/v0.2.16`. | One tool call returns the actual capsule, including insufficient-budget metadata; a digest alone is insufficient. |
| `crates/lekalo-core/src/graph/{mod.rs,slice.rs,canonical.rs}` | Dependency graph and bounded slices; CLI show/callers/path receipts differ from the full graph export schema; canonical export is `lekalo/graph/v0.2.16`. | Publish exact describing schemas for query receipts; do not validate show/path as full exports or truncate graph JSON. |
| `crates/lekalo-core/src/validator/`, `cache/`, CLI `run_validate`, `with_graph` | Validation profile can report mandatory cross-module diagnostics; cached compilation/session records are ordinary CLI side effects. Success receipt has describing schema `contracts/validation-report.schema.v0.6.4.json`. | Server executes the uncached path. Module selection does not restrict all reads to that module. |
| `crates/lekalo-core/src/doctor/{mod.rs,version.rs}`, `target_profile/`, `lockfile/`, CLI `doctor_git.rs` | Read-only readiness report, lock/inventory/profile observations; `lekalo/doctor/v0.3.2`. A report can have status valid and verdict blocked. | Use typed relevant checks for target/profile status; unavailable or stale is not passed. There is no existing `lekalo target status` command. |
| `crates/lekalo-core/src/trace/`, CLI `run_trace`, `trace_export` | `TraceManifest::parse` validates supplied bytes, canonicalizes and hashes `lekalo/trace-manifest/v0.2.16`; export does not construct a trace from the project. `trace collect` persists evidence. | Admit a registered manifest reference through a bounded no-follow reader before parsing. Do not expose collect as a read tool. |
| `crates/lekalo-core/src/{project_fs.rs,loader/}` | Validated-root filesystem capability, logical paths, no-follow/link/reparse-point checks and bounded source loading; loader never uses `.lekalo/**` as model input. | Reuse these policies and strengthen read admission for MCP; a root prefix comparison alone is insufficient. |
| `crates/lekalo-core/src/target_protocol/{scopes.rs,confinement.rs,confinement_windows.rs}` | Adapter-declared scopes, protected write homes and platform confinement; governed dot homes can be legitimate internal inputs. | MCP permission cannot exceed core/adapter policy. No adapter subprocess is needed for the initial seven tools. |
| `crates/lekalo-core/src/{result.rs,diagnostics/}` | `DomainResult` owns status/exit/stream; original diagnostics, normalized ordering and derived reasonCodes. Diagnostic wire `0.2.16`; embedded registry `0.6.4` plus supported successor `0.6.5`. | MCP preserves domain status and codes while putting protocol responses on stdout; it does not inherit CLI stderr placement. |
| `docs/{provider-contract.md,authority.md,privacy.md}`, `privacy/{refs.rs,context.rs}` | Provider discovery is frozen `dev.lekalo.workflow-provider@0.6.4`; artifact ownership and disclosure policy are independent. | MCP has an independent contract family. No discovery-triggered install, init, generation, QA persistence or new artifact custody. |

Read-only external inventory: `D:/Projects/aifhub/aifhub-extension`, HEAD `9b9db326865c342761467d2238895a53a341f7cf`, clean at inspection. Its `extension.json` has an `mcpServers` template for `ai-factory aifhub-mcp`, with instructions describing AI Factory 2.16+ runtime configuration rendering. This proves an extension registration seam exists, **not** that a Lekalo server is configured or supported by every installed host. No external files were changed or external tests executed.

Protocol documentation was fetched through Context7, resolving `Model Context Protocol` to official `/modelcontextprotocol/modelcontextprotocol`, then querying the version-specific `/websites/modelcontextprotocol_io_specification_2025-11-25`. The first lookup also returned a 2026-07-28 profile with materially different per-request metadata/lifecycle. **Proposal:** pin `2025-11-25` for the first qualified stdio host profile; do not claim it is the latest profile or silently mix lifecycle rules from the two dates. Host compatibility remains a measured acceptance gate.

## Goal decomposition and implementable increments

| Increment | Proposed implementation and ownership | Observable completion |
| --- | --- | --- |
| I1: publish the read boundary | New core `mcp/{mod.rs,version.rs,schemas.rs,policy.rs}` for pure request/response/policy types; explicit catalog of project IDs, named groups, admitted inputs and trace references. Add contracts and documentation before transport wiring. | Exact seven-tool catalog, closed root-object schemas, fixture parity, scope narrowing tests; no filesystem/process work in pure schema discovery. |
| I2: authorize every read | New CLI `mcp/{catalog.rs,read.rs,snapshot.rs}` using existing loader/project_fs ownership. Extract a policy-aware loader/service seam where needed, rather than copying parsers. Register the finite read set before any file open. | Reads stay within approved project/core policy; imports, trace links and status evidence cannot broaden it. No cache/runtime writes or adapter execution. |
| I3: add stdio lifecycle | New CLI `mcp/{mod.rs,stdio.rs,dispatch.rs,execution.rs}`, isolated from normal command rendering. Add `McpCommands::{Describe,Serve}` in `main.rs`; dependencies and feature choices must satisfy existing MSRV. | Initialize/initialized, ping, list/read/call, bounded frame parsing and error mapping work in a real subprocess. CLI builds and works without serving MCP. |
| I4: expose all resources/tools | New pure core `mcp/{tools.rs,resources.rs}` maps typed services. Five resource families and all seven P1 tool names; schema-pinned payloads, deterministic ordering. | Same admitted inputs produce semantically identical CLI/core/MCP payloads; context needs exactly one tools/call request. |
| I5: bound lifetime and refresh | Request supervisor, deadlines, cancellation, input identity recheck; bounded watch of admitted model inputs and resource subscriptions. | Cancel/timeouts leave no worker/process leak, no stale success; atomic model updates invalidate memory before change notifications. |
| I6: qualify host integration | New Lekalo `docs/mcp.md`, `examples/mcp/ai-factory-extension.json`; proposed separate extension registration/test change, runtime-specific configuration examples. | AI Factory extension renders a local read-only Lekalo template, launches the built/released server and completes real resource/tool calls. |
| I7: gate acceptance | Rust boundary tests, Node schema/stdio gates, synthetic fixture provenance/catalog and cross-OS CI wiring. | Every issue acceptance row below has positive and negative evidence, with no required live host test hidden behind a skip. |

Order: I1 -> I2 -> I3 -> I4 -> I5 -> I6/I7. Standalone read functionality remains usable while external host qualification is pending. None of these increments includes generation/write tools; their independent authorization design is a later opt-in stage.

## Proposed CLI and tool surface

### Server entrypoints

```text
lekalo mcp describe --json
lekalo mcp serve --stdio --project DIR
lekalo mcp serve --stdio --project DIR --group planner-review
lekalo mcp serve --stdio --project DIR --catalog-config mcp.local.json --timeout-ms 30000 --max-response-bytes 1048576
```

All four entrypoints are **proposed**. `describe` emits an embedded deterministic schema/capability manifest without reading a project or starting a server. `serve` requires explicit project selection, has read-only authorization unconditionally, and owns stdout exclusively as MCP JSON-RPC. Reject `--json` for serve and unknown transport/write flags with normal pre-server CLI usage diagnostics. The catalog-config path is a trusted launcher operand, never a remotely supplied tool argument; an explicit config is optional for single-project startup. New `--group` narrows a group already defined in that trusted catalog; it cannot invent memberships.

Proposed bounds: incoming JSON-RPC frame 256 KiB, response 1 MiB by default with maximum 4 MiB, timeout 30 seconds with maximum 120 seconds, four admitted concurrent calls, 16 queued requests, 128 semantic roots, page size 100 with maximum 500, 256 watched/subscribed resource identities, nesting depth 64. Effective limits are the minimum of server configuration, host profile and core family bounds. Caller parameters may reduce a limit, never raise it. The response limit counts the entire encoded JSON-RPC frame, including text and structured JSON duplication. Reserve a small fixed error budget; an over-limit request produces a bounded error, never a partial JSON frame.

### Seven exact tool names and request grammar

Every tool has required `projectId` and optional `groupId` from the server catalog, plus optional `timeoutMs` within server limits. No tool accepts root paths, executable names, arbitrary argv, environment variables, export destinations or write permissions. Semantic IDs and enumerated selectors are validated before loading.

| Required tool | Proposed additional arguments / exclusivity | Existing CLI comparison or typed owner |
| --- | --- | --- |
| `lekalo_inspect` | `symbol`; `include` array of `bindings` and/or `scenarios` | `lekalo inspect planner.focus_task --include bindings,scenarios --project DIR --json --no-cache`; `inspect::run`. |
| `lekalo_impact` | `mode: symbol|changed`; symbol mode requires `symbol`; changed requires `baseRef` and exactly one of `headRef` or `worktree:true`; optional `depth`, `module`, `target`, `relations`, `profile` | `lekalo impact planner.focus_task --depth 3 --project DIR --json --no-cache`; changed comparison: `lekalo impact --changed --base BASE --worktree --project DIR --json --no-cache`. Resolve refs to pinned commits before analysis; no guessed HEAD/base. |
| `lekalo_context` | `mode: symbol|changed`; exactly `symbol` or nonempty `changedSymbols` array; `budget` required, optionally `spans:false|true` | `lekalo context planner.focus_task --budget 5000 --project DIR --json --no-cache`; changed: `lekalo context --changed planner.focus_task,planner.list_tasks --budget 12000 --project DIR --json --no-cache`. Changed IDs are a typed handoff, not a Git mode. |
| `lekalo_graph` | `mode: show|callers|path|export`; mode-specific `symbol` or `from`/`to`; callers supports `transitive`; export only within full authorized project scope | `lekalo graph show planner.focus_task --project DIR --json --no-cache`; `graph callers SYMBOL --transitive`; `graph path FROM TO`; `graph export --format json`. Reuse exact core query/export builders; unsupported group export refuses instead of fabricating a full graph. |
| `lekalo_validate` | Optional `module` and `strict:false|true`; report-file/CI-output arguments forbidden | `lekalo validate --module planner --project DIR --json --no-cache`; validate full required semantic closure, preserve mandatory cross-module findings. |
| `lekalo_target_status` | Optional `targetId`, `profileId` admitted in the project catalog | Typed doctor lock/inventory/capability-profile checks, with target-specific selection from existing profile/lock services. CLI comparison is `lekalo doctor --project DIR --json --no-cache`, not an invented existing target subcommand. New selection/result projection belongs to MCP contracts. |
| `lekalo_trace_export` | `manifestRef` required, an opaque catalog ID for an approved trace input | CLI semantic comparison: `lekalo trace export PATH --json --no-cache`; MCP replaces its path reader with bounded authorized bytes then calls `TraceManifest::parse`. No manifest auto-collection or construction. |

These CLI examples are comparison recipes, not instructions to spawn unrestricted subprocesses from tool arguments. Prefer typed core dispatch after authorized loading. Git operations, where necessary, use reviewed argv arrays, `shell:false`, a fixed approved cwd, minimal environment and no external diff/filter/helper execution. Changed-mode Git access requires explicit catalog admission; without it return unsupported, never scan the repository to infer changes. No protocol request can switch projects by changing cwd or `LEKALO_PROJECT`.

### Resources, groups and updates

Proposed logical URIs, with bounded percent-encoding of semantic components:

| Resource family | URI form | Content |
| --- | --- | --- |
| Project summary | `lekalo://PROJECT/summary` | Project identity, schema pins, admitted scope and model/IR revision digest; aggregate module/symbol counts and explicit unknown states. |
| Module list | `lekalo://PROJECT/modules` | Deterministic bounded page of authorized module metadata; opaque cursor bound to snapshot/scope. |
| Symbol metadata | `lekalo://PROJECT/symbol/SYMBOL` | The authorized inspect projection, never raw source or a repository file resource. |
| Target/profile status | `lekalo://PROJECT/targets` | Selected lock/profile/inventory observations; available/unsupported/stale/unknown remain separate. |
| Diagnostics/evidence summary | `lekalo://PROJECT/diagnostics` | Safe stable diagnostics and scoped evidence counts/references/completeness; no raw tool logs or local history. |

Resources/read returns application/json text containing the same closed versioned projection types used by tools. Groups use the same families with a catalog-defined group qualifier; resources/list and templates disclose only authorized identities. No `file://` resources, arbitrary glob/read-file tool, recursive repository listing or discoverable private absolute roots.

**Group decision:** no canonical Model group field was found in the inspected schema/IR. Introduce a server-local named allow-list of modules within one approved project, not a change to Model/IR or a cross-project wildcard. The trusted catalog grants a finite canonical input closure independently of the response group. A group is a semantic disclosure constraint, not a claim that CLI `--module` limits filesystem reads. Any dependency outside its admitted read closure is denied before reading; any response requiring unauthorized semantic facts is refused before serialization. Do not drop protected context facts or mandatory diagnostics and label the result complete. Initially refuse an operation whose full core result cannot fit the authorized group; a later scoped projection needs its own explicit schema/completeness rules.

Watch only admitted model/config/lock/evidence inputs, never a recursive repository root. Debounce and hash changed admitted bytes, validate a replacement snapshot, then atomically replace it. Invalid model changes invalidate the old success snapshot and expose current diagnostics. Subscribed resource content changes emit `notifications/resources/updated`; resource enumeration changes emit `notifications/resources/list_changed`. Tool-list changes need a real catalog/capability change, not every model edit. Bound/coalesce notifications; stale cursors refuse. No notification contains paths, contents or another group's symbol identities.

## Proposed versioned contracts and MCP compatibility

Follow the product-version rule: **new/changed contracts in implementation take `0.8.0`; unchanged contracts retain both bytes and versions**. Do not re-version Model/IR, inspect/impact/context/graph/trace/diagnostic `0.2.16`, doctor/target-protocol `0.3.2`, validation/provider `0.6.4`, or unrelated successor contracts merely because MCP is added. Outer MCP protocol date is independent of Lekalo contract version.

| Proposed contract artifact | Identity / purpose |
| --- | --- |
| `contracts/mcp-capabilities.schema.v0.8.0.json` | `dev.lekalo.mcp-capabilities@0.8.0`, `lekalo/mcp-capabilities/v0.8.0`; seven exact tools, five resource families, protocol profile, schema/digest pins, permissions and limits. |
| `contracts/mcp-server-config.schema.v0.8.0.json` | `dev.lekalo.mcp-server-config@0.8.0`; trusted catalog, project/group/input/manifest grants and bounded limits. Physical launcher roots never appear in discovery or result payloads. Config is not a newly authorized persisted canonical artifact. |
| `contracts/mcp-tool-<NAME>-input.schema.v0.8.0.json` | Seven closed root-object schemas for NAME `inspect`, `impact`, `context`, `graph`, `validate`, `target-status`, `trace-export`; exact matching `dev.lekalo.mcp-tool-<NAME>-input@0.8.0` identities. |
| `contracts/mcp-tool-<NAME>-output.schema.v0.8.0.json` | Seven closed root-object schemas, matching `dev.lekalo.mcp-tool-<NAME>-output@0.8.0`; outer discriminator `lekalo/mcp-tool-<NAME>-output/v0.8.0`. Describes preserved core receipt/payload or a typed MCP refusal. |
| `contracts/mcp-resource.schema.v0.8.0.json` | `dev.lekalo.mcp-resource@0.8.0`, `lekalo/mcp-resource/v0.8.0`; closed resource identity, scope, revision, data and bounded-page metadata. Per-kind branches are nested. |
| `contracts/mcp-graph-query.schema.v0.8.0.json` | Describing schema `dev.lekalo.mcp-graph-query@0.8.0` for current show/callers/path receipts; no discriminator injected into frozen core query payloads. |
| `contracts/diagnostic-registry.v0.8.0.json` and `diagnostic-registry.schema.v0.8.0.json` | Registry successor for the new MCP diagnostic family below; preserve all predecessors/tombstones and typed vocabulary. Diagnostic item wire remains frozen unless its actual shape changes. |

The seven NAME substitutions above enumerate fourteen artifacts, not arbitrary extension names. Proposed common output members: `schemaVersion`, `identity`, `tool`, `scope`, `snapshotRef`, `status`, `contractPins`, `diagnostics`, and mode-specific `result` on a completed core call. Failure branch records `failureClass` and bounded closed detail, without fake core payload. Tool-specific nested branches pin the exact upstream schema, including validation failure envelopes and doctor verdict semantics. State-dependent required/prohibited fields are enforced by nested schema rules and semantic validation. Resources use the same validated result types instead of a second semantic vocabulary.

Each input/output schema uses top-level `type:"object"`, `properties`, `required`, `additionalProperties:false`; no root `oneOf`, `anyOf`, `allOf`, `not`, bare `$ref` or top-level union. Flatten the listing schemas deterministically from published artifacts, with definitions/references bundled locally and no remote schema fetches. Nested discriminated branches remain possible, but host validators must actually parse them. Closed enums, unique arrays, maximum string/array/number/depth limits, mode exclusivity and cross-field scope/status/revision coherence require Rust and Ajv parity checks. Do not replace typed outputs with an arbitrary JSON string just to satisfy a host.

Return each completed tool result in `structuredContent` and a text content block containing the same serialized JSON for compatible clients; validate both against the tool output schema. All domain refusal/failure statuses set `isError:true`; a valid but blocked doctor report or `fits:false` capsule keeps its original semantic state explicitly, without being promoted to ready/complete. Incompatible outer call syntax/unknown tool gives MCP protocol error; a known tool's semantic failure gives a typed tool result. Core diagnostics survive verbatim when disclosure-safe; otherwise refuse disclosure with an MCP diagnostic rather than corrupting the core diagnostic contract.

Pinned protocol behavior: `initialize` negotiates the supported date; after `notifications/initialized`, allow ping, tools/list, tools/call, resources/list, resources/templates/list, resources/read, resources/subscribe and resources/unsubscribe. Advertise only implemented capabilities: tools and resource subscribe/listChanged; no prompts, sampling, elicitation, tasks, HTTP transport or client-driven root expansion. Stdio is one UTF-8 JSON-RPC message per LF-delimited frame, with only protocol frames on stdout and sanitized bounded logging on stderr. Unknown methods use `-32601`, unknown tools/invalid call parameters `-32602`, malformed JSON `-32700`; do not echo private operands. Serialize writes so notifications and concurrent replies cannot interleave bytes.

Official references: [2025-11-25 lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle), [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports), [tools and structured content](https://modelcontextprotocol.io/specification/2025-11-25/server/tools), [resources and subscriptions](https://modelcontextprotocol.io/specification/2025-11-25/server/resources), [cancellation](https://modelcontextprotocol.io/specification/2025-11-25/basic/utilities/cancellation). Newer profiles require deliberate negotiation adapters and qualification; host advertising a different date is not permission to guess compatibility.

## Authorization, privacy, cancellation and determinism

Effective authorization is **launcher catalog grants intersect core read policy intersect semantic group permissions**, and adapter policy as well if adapter access is ever added. Client roots, URI fields and tool annotations are untrusted metadata, never a filesystem grant. Root selection is fixed at startup; requests refer only to opaque project/group/manifest identities. Authorize file enumeration, imports, each read, Git access, evidence loading and each result independently. Reuse existing capability/no-follow semantics; do not weaken core or Windows confinement checks to make MCP work.

Separate internal reads from disclosure: governed `.lekalo/**` evidence/lock metadata and Git object/index inputs may be explicitly needed internally, but dotfiles/raw contents and absolute private roots are never MCP resource content. Deny secrets/config directories such as `.env`, `.git/config`, key stores and user-home paths as arbitrary inputs. Block traversal, encoded traversal, drive/UNC/file URLs, NUL/ADS, mixed separators, symlink/junction/reparse-point/alias and replacement races. Check hardlink exposure according to existing owner policy, with hostile probes; lexical containment alone is insufficient. Unknown safe authorization on a platform is unavailable, without an unrestricted fallback.

Current project loading/status inspection is not yet parameterized by this MCP catalog. I2 therefore needs an explicit authorized-read abstraction at the existing loader/doctor/trace seams. Policy checks happen at enumeration/open boundaries, not a single post-read prefix check. A subprocess fallback is acceptable only when the approved confinement recipe proves the same finite read set; it cannot substitute for the policy-aware typed path. No external adapter scan, native gate execution, trace collect, run-history record, cache creation, report-file output or install/repair is reachable from the default tool dispatch.

Use existing privacy/classification owners plus a closed output allow-list. Do not assume model descriptions, default values, diagnostic data or provenance strings are secret-free simply because paths are relative. Check synthetic canaries in all serialized strings and all error/log paths; unsafe content yields `mcp.sensitive-output` with no offending text. Keep mandatory contract fields intact; if policy-safe redaction changes a payload's contract meaning, refuse rather than silently deleting fields. Semantic facts and names can remain local-private; a content digest is neither anonymization nor export authorization. Stdio response delivery does not authorize durable QA/public export or remote hosting.

Supervise work with per-request cancellation token and monotonic deadline. Handle `notifications/cancelled` by matching the live request ID, stop queued work or terminate the bounded worker, and suppress late success. Do not cancel initialization via this notification. Existing pure traversals/load/serialization need cooperative checkpoints or isolation in a supervised read-only worker; a timer around a blocking thread does not stop its execution. If a process is used, terminate descendants using approved platform behavior and bound cleanup. EOF shuts down outstanding work; one cancelled/timed-out request leaves the session available. Cancellation notifications themselves have no reply; do not claim the client must consume a late cancellation diagnostic. Timeout can return a typed bounded tool error while the client is still waiting; neither timeout nor cancellation is model validation failure.

Resolve and pin input identities once per request; recheck admitted model/config/lock/profile/trace bytes before serialization. Input changes discard the candidate result with stale-snapshot diagnostics; never answer with the old successful model after an invalid edit. Pin core payload identities/hashes using their existing digest domains, not one new hash algorithm for every family. MCP snapshot identity is a separate domain over sorted admitted logical identities and exact content digests; roots, clocks, environment and host names do not enter deterministic outputs. Request IDs and notification arrival order are transport facts, not semantic goldens.

## Diagnostic-family and registry implications

No `LEK-MCP-*` allocation was found in the inspected diagnostic registries. Proposed allocation below is additive; implementation must recheck collisions against the M8 integration registry. MCP transport diagnostics supplement original core/adapter diagnostics and never rename them. Preserve severity/category/status independence and original namespaced provider codes.

| Proposed code / rule ID | Category; status | Meaning |
| --- | --- | --- |
| `LEK-MCP-001` / `mcp.input-invalid` | infrastructure; invalid | Known-tool semantic request/config invariant invalid; bounded closed field/reason, no input echo. Malformed JSON-RPC syntax remains a protocol error. |
| `LEK-MCP-002` / `mcp.scope-denied` | security; denied | Unregistered project/group/input, attempted widening or forbidden path read/disclosure. |
| `LEK-MCP-003` / `mcp.sensitive-output` | security; denied | Output or log cannot be safely disclosed while preserving its contract. |
| `LEK-MCP-004` / `mcp.request-limit` | infrastructure; denied | Admission queue/concurrency/frame/nesting/root bound exceeded. |
| `LEK-MCP-005` / `mcp.output-limit` | infrastructure; denied | Complete encoded response exceeds its effective cap; no truncated payload. |
| `LEK-MCP-006` / `mcp.timeout` | infrastructure; unavailable | Request deadline expired; bounded cleanup and no late success. |
| `LEK-MCP-007` / `mcp.cancelled` | infrastructure; unavailable | Internal cancellation outcome; no forced response to a cancelled request. |
| `LEK-MCP-008` / `mcp.contract-unsupported` | compatibility; unsupported-version | Unknown exact tool/result/protocol pin; no automatic schema downgrade. |
| `LEK-MCP-009` / `mcp.snapshot-stale` | infrastructure; unavailable | Authorized input bytes changed during evaluation or cursor revision is stale. |
| `LEK-MCP-010` / `mcp.backend-unavailable` | infrastructure; unavailable | Required safe read/confinement/host capability unavailable; no broad fallback. |
| `LEK-MCP-011` / `mcp.result-invalid` | infrastructure; invalid | Typed backend output violates selected schema/status/pin invariants. |

Publish the `0.8.0` registry successor and companion schema together, update `diagnostics/{registry.rs,version.rs}`, message/typed data owners, applicable validation profiles and fixture coverage inventory. Preserve predecessor entries, codes, default severities, lifecycle/tombstones and allowed statuses; do not overwrite frozen registry files. Existing domain diagnostics retain their accepted registry pins; a transport diagnostic uses the new pin. A normal tool success does not imply diagnostic absence, and registered error severity alone does not rewrite core status. No new semantic diagnostic family for validation/impact/context/graph is needed.

The workflow-provider manifest need not change: MCP discovery is separate. If a later provider manifest or privacy/authority admission actually changes, publish that changed family at `0.8.0`, update all consumers/pins/hash gates deliberately, and keep untouched upstream contracts frozen. This research grants no new persisted artifact home.

## Fixtures, gates and acceptance mapping

Proposed synthetic family `tests/fixtures/mcp/`: `catalog/`, `projects/{single,groups,multi-root}/`, `schemas/{valid,invalid}/`, `tools/`, `resources/`, `sensitive/`, `limits/`, `cancellation/`, `updates/`, `host/`, `golden/`. Reuse existing inspect/impact/context/graph/trace/doctor/validation inputs and expected core outputs by pinned fixture references; do not duplicate whole consumer repositories. Hostile absolute paths/secrets are synthetic recipes materialized in temporary test roots, not real user data. Register the family in `tests/fixtures/fixture-provenance.json` and the existing suite catalog/diagnostic-pair inventory; actual baseline catalog path must be confirmed when wiring implementation.

| Fixture/gate family | Proposed checks |
| --- | --- |
| Contract/schema compatibility | Seven input/output root objects, exact schema/version/pins; unsupported date, extra fields, wrong types, nested exclusivity, duplicate keys, deep/large JSON and semantic invariants. Ajv plus actual qualified host/SDK validators. Structured/text outputs agree. |
| Scope and path admission | Project A/B, same symbol names, authorized group, unknown/mixed group, cross-module closure, scope attempt in URI/cursor/tool args, malicious imports; Linux links and Windows junction/UNC/ADS/aliases/races. Instrument reads as well as compare writes. |
| Core parity | All seven tool payloads and five resources; unknown/ambiguous symbols, invalid/warning/strict validation, incomplete impact, insufficient context budget, blocked/unknown target status, partial trace and wrong manifest digest. Compare full typed payloads and exact domain diagnostics against core/CLI. |
| Read-only and privacy | Full before/after file inventory/hash in disposable roots; no cache/lock/history/reports/adapter execution; secrets in model strings/errors/evidence, raw log/private absolute path leakage and dotfile enumeration. Both triggering and clean controls required. |
| Transport, limits and lifecycle | Real stdio child; initialize order, unsupported negotiation, unknown request, multiple frames, UTF-8/chunk boundaries, stdout cleanliness, atomic writes, request-ID isolation, max+1 bounds and queue saturation. |
| Cancellation/timeouts/updates | Queued/running/serialization cancellation, CPU and I/O stall, worker descendants, late result suppression, EOF cleanup, subsequent ping/tool health; valid/invalid model replacement and subscribed/list updates, stale cursor and no notification leaks. |
| Host integration and CLI independence | Real AI Factory extension render/launch/resource/context flow on selected runtime; missing server/unsupported protocol/optional disabled config cannot break normal CLI. Server absent/feature-disabled build retains ordinary CLI commands. |

Acceptance wording below is copied **verbatim** from the issue, retaining all seven criteria. This is a future implementation/acceptance map; no row is claimed passed by this research.

| Acceptance criterion from #33 | Proposed implementation loci | Evidence required |
| --- | --- | --- |
| MCP read tools не расширяют filesystem scope core/adapter policy. | I1/I2 catalog/read grants, loader/project_fs seams, request/output authorization; existing adapter policy. | Instrument admitted reads and denied opens, hostile imports/trace refs/roots/groups/Git helpers, platform confinement negatives; prove no raw source/adapter spawn/fallback and zero writes. |
| Context capsule доступна одним tool call. | `lekalo_context`, `context::plan`, pinned context output schema. | One `tools/call` returns the full structured capsule and matching text JSON, no required chained resource/impact call; fits and minimum-required cases preserve all mandatory facts. |
| Validate/impact возвращают stable diagnostics. | Typed validation/impact services, original `DomainResult`/diagnostic normalization, MCP result adapter. | Same admitted input/profile across repeat runs and locales yields original code/id/severity/category and deterministic ordering; semantic failure differs from timeout/unsupported; warning/strict and incomplete-graph controls. |
| Write operations недоступны по умолчанию. | Fixed seven-tool dispatch and schema catalog; uncached reads; independent future write stage. | tools/list has exactly seven read tools; generate/write/collect/install/migrate/scan/report/history and smuggled write parameters refuse; snapshots verify no filesystem mutations. Config/annotations cannot activate a hidden write branch. |
| Server можно подключить через AI Factory extension. | `docs/mcp.md`, `examples/mcp/ai-factory-extension.json`; external extension `extension.json` registration seam and its own tests. | Separate extension commit renders a Lekalo template for a declared supported runtime and launches actual server; initialize/list/resource/one-call context complete with captured version/pin/config/commit evidence. Mock server/config JSON alone is insufficient. |
| Tests покрывают schema compatibility, scopes и sensitive paths. | `crates/lekalo-core/tests/mcp.rs`, `crates/lekalo-cli/tests/mcp.rs`, new script gates and synthetic family. | Root-schema/host-parser matrix, allowed/denied project/group table, Windows/Linux/macOS path attacks and synthetic leak controls; a missing required test or required host lane fails CI. |
| MCP unavailable не делает обычный CLI unusable. | Isolated serve lifecycle, ordinary CLI command dispatch; optional host registration. | Run inspect/impact/context/graph/validate/provider describe with missing/disabled/crashed/incompatible MCP; normal CLI semantics remain usable. No CLI startup attempts MCP negotiation/install/network access. |

Requirements traceability beyond acceptance: stdio-first -> I3 transport gate; versioned host-compatible schemas -> I1 schema/real-validator matrix; project/group -> I1/I2/I4 scope table; read-only and later explicit write opt-in -> I2/I4 dispatch/writes probes and deferred write boundary; secrets/dotfiles/absolute paths -> I2/I4 leak/read-admission corpus; response bound -> I3/I5 full-frame tests; cancellation/timeouts -> I5 process/checkpoint tests; change notifications -> I5 subscription/invalid-update tests; core JSON output -> I4 full parity. Five resource families and all seven names are required, not optional shortcuts.

### Gate and CI plan (proposal)

New gates: `scripts/test-mcp-contracts.mjs` (schema/catalog/core pin parity), `scripts/test-mcp-stdio.mjs` (real binary lifecycle/parity/limits/scopes/privacy/cleanup/updates), `scripts/test-mcp-host.mjs` (qualified host connection; external extension qualification has separate custody), and `scripts/test-mcp-boundaries.mjs` (read-only/CLI independence and source dependency boundaries). New core tests own pure policy/projection semantics; CLI subprocess tests own filesystem/process/stream behavior.

```text
cargo test --locked -p lekalo-core --test mcp
cargo test --locked -p lekalo-cli --test mcp
cargo build --locked -p lekalo-cli
node scripts/test-mcp-contracts.mjs
node scripts/test-mcp-stdio.mjs
node scripts/test-mcp-boundaries.mjs
node scripts/test-mcp-host.mjs
node scripts/test-diagnostic-contracts.mjs
node scripts/test-inspect-contracts.mjs
node scripts/test-impact-contracts.mjs
node scripts/test-context-contracts.mjs
node scripts/test-graph-contracts.mjs
node scripts/test-doctor-contracts.mjs
node scripts/test-trace-contracts.mjs
node scripts/check-contract-versions.mjs --base <implementation-base>
cargo fmt --all -- --check
```

All new commands/tests above are future work. Extend `.github/workflows/ci.yml` schema gates on existing Node 18/24 lanes with pinned Ajv, and binary subprocess gates on Ubuntu/Windows/macOS **after** building that job's CLI binary. Provision Ajv outside the source tree and set both `LEKALO_AJV_NODE_PATH` and `NODE_PATH` as current CI does. Run existing structure/privacy/authority/provenance/suite gates after adding their new inventories. Add normal workspace Clippy/MSRV checks; do not assume an MCP SDK supports Rust 1.80.0 without an actual dependency audit/build.

Host profile qualification records exact client/runtime/protocol/schema validator versions and input/output bounds. A missing required host or incompatible parser fails its qualification gate; it may be reported unavailable locally but cannot be relabeled accepted. Linux/macOS/Windows subprocess and confinement evidence, schema-only fixtures and external host acceptance are separate deliverables.

## Risks and non-goals

- **Protocol drift:** 2025-11-25 initialization/subscription rules and 2026-07-28 per-request rules are different. Implement one pinned profile first, then explicitly qualify others; no universal host-compatibility promise.
- **Read scope versus semantic group:** module-filtered CLI validation still loads the full semantic project and reports cross-module problems. Admit its finite canonical closure explicitly or refuse; neither output filtering nor client roots proves safe reads.
- **Missing policy seam:** trace's raw file reader and doctor/loader reads need policy-aware entrypoints. An unrestricted CLI proxy would inherit these gaps and cache effects.
- **Disclosure versus contract parity:** required context facts or diagnostics can contain sensitive strings. Default refusal is safer and more faithful than deleting contract members and reporting complete success. Local-private delivery is distinct from public export.
- **Blocking work and stale snapshots:** a timeout wrapper does not interrupt blocking parsing/traversal. Add checkpoints or supervised isolation and pre/post identity checks; invalid edits must not leave old successes current.
- **MSRV and host evidence:** SDK dependencies may require a newer Rust version; JSON-schema validity alone does not prove a host accepts root shapes or launches the server. Make both gates explicit.
- **External ownership:** extension registration is a separate repository change. This research's local artifact can be complete while #33's real host criterion remains unimplemented.
- **Bounds and completeness:** core exports can exceed host caps. Count text duplication/frame overhead, refuse unsafe overflows, and preserve `fits:false`/partial states rather than truncating away mandatory facts.

Non-goals: write/generate tools or a hidden write flag; adapter/native test execution; HTTP/remote multi-tenant serving/auth; a generic repository browser; raw source retrieval; Model/IR group-schema changes; LSP (#80); replacement of workflow-provider (#34), HLV bridge (#35) or cross-project AI Workspace workflows (#37); public QA/evidence publication; installation/release/tag/issue closure. Generation/write requires a new separately authorized design and implementation stage.

## Research validation and completion boundary

Research evidence is live issue/context retrieval, current local source/contract inventories, official version-specific MCP documentation through Context7, and read-only extension registration inspection. No MCP server, new schema, diagnostic allocation, fixture/gate, external registration, Cargo version bump or release was implemented. No runtime/host/full-suite acceptance is claimed.

Document validation: check exact seven acceptance strings and seven tool names, balanced fences and contract-version distinctions; run `git diff --check`; inspect staged paths; commit only `docs/m8/issue-33-research.md` with `docs(m8): research issue-33`; verify that the file is present in the HEAD commit and no task changes exist outside `docs/m8/`. No push or external message publication is part of this task; worker completion is reported through the assigned Orca dispatch.
