# Target adapter process protocol

Status: **Implemented** host protocol `lekalo.target/v1` with bootstrap contract `0.3.2` and explicitly negotiated successors `0.6.4` (lint), `0.8.0` (watch selection) `0.9.0` (ordinary public scan custody) and `0.9.1` (complete native plans), product `1.0.0`. Owner: adapter-host maintainers. [ADR-0025](adr/0025-target-protocol.md). Source-bound Model/IR pairs are `0.2.16` and `0.8.0`; ordinary operations retain their predecessor admission, and watch requires the explicit pins below. The ordinary diagnostic registry is `0.6.4`; watch emits the merged `0.8.0` union. Supported operations are describe, scan, bind, validate, generate, verify, plan-clean, clean and the read-only native-plan extension. Operation availability depends on the adapter and profile; declaration is not successful execution.

The exact [current schema](../contracts/target-protocol.schema.v0.3.2.json) and production decoder govern this checkout. Historical base/extension paragraphs below describe negotiation history; they do not admit a historical contract absent from the current registry. [Workflow provider discovery](provider-contract.md) is separate. [Native gates](native-gates.md) own read-only planning and the explicitly approved Windows production execution profile. [Architecture](architecture.md), [security](security.md), [Node adapter](../adapters/node-typescript/README.md), [PHP adapter](../adapters/php-laravel/README.md).

## Version negotiation and capability discovery

This checkout probes at **exactly `0.3.2`** (`BASE_VERSION`/`VERSION`) and upgrades only to a supported version the adapter explicitly declares and proves through a second describe at that exact version. The ascending `SUPPORTED_VERSIONS` set in `crates/lekalo-core/src/target_protocol/version.rs` admits 0.3.2, 0.6.4, 0.8.0, 0.9.0 and 0.9.1. The frozen predecessor schemas retain their exact members and bytes; arbitrary newer/older strings are refused.

Protocol `0.9.0` retains the `0.8.0` wire surface and adds exact
source/profile custody for successful scans, with scan detail capacity of
2048 Unicode code points. Other detail limits remain 128. Optional watch work still requires the adapter to declare that capability. The
[public scan recipe](public-typescript-scan.md) describes required
`source_digest`, `scan_profile_digest`, typed refusals and the qualifying
manifested standalone/workspace subset.

Describe reports exact adapter identity/digest, protocol versions, operations, transports, target/profile choices, read/write scopes and capabilities. The closed capability object includes exact `ir_versions`, a bounded map of named support states (`full`, `partial`, `unsupported`, `unknown`) and optional declared constraints. Names must resolve in the embedded capability-definition registry. Missing or unknown evidence does not mean supported.

Discovery sends only describe, without project IR or write authority. An ordinary IR-carrying operation requires explicit compatibility with the core's `0.2.16` IR; a selected watch operation instead requires the exact captured IR version plus an explicit accepted source Model version. Incompatible candidates are filtered before disclosure. A declared digest is distinct from verified entry bytes; capability provenance distinguishes declared, read-only probed and planned/applied verified evidence.

Selection filters IR compatibility before required capability support. Partial support requires explicit `allow_partial`; unknown support cannot satisfy a required capability. Survivors and refusal reasons are deterministic. Cache/lock custody includes adapter identity, actual bytes, negotiated protocol, IR and capability-definition/profile digests; changes invalidate the snapshot. [Target profiles](target-profile.md), [lock](lockfile.md), [adapter conformance](adapter-conformance.md).

## Read-only watch selection (0.8.0)

The [watch protocol successor](../contracts/target-protocol.schema.v0.8.0.json)
adds optional `capabilities.watch: {selection: true, model_versions: ["0.2.16", "0.8.0"]}`
on the upgraded describe. `model_versions` is a nonempty sorted exact set;
its admitted pairs are Model/IR `0.2.16` and `0.8.0`, and `ir_versions` must
independently declare the requested IR. Neither a protocol upgrade nor scan
support implies successor Model support. Only that declaration, at exactly
0.8.0, admits `watch_request` on scan/validate. Predecessor Models may use a
visible complete full-scope legacy fallback; successor Models never downgrade.
The closed request carries `source_revision`, `pins` (`model_version`,
`ir_version`, `ir_digest`), `scope` (`module` or `global`), the module
for module scope, sorted owned `symbols` (old and current, including deletions)
and sorted `changed_paths`. Global scope has no module or symbols and runs only
mandatory project-level obligations. Full captured IR is resolution context,
not permission to revalidate unrelated module partitions.

Selected responses must echo `evidence.watch_digest`, SHA-256 over compact
UTF-8 JSON of the selection with recursively byte-sorted object keys and no
trailing newline. Unknown members, explicit nulls, unsupported versions,
unsafe paths, unknown or mixed Model/IR pins, wrong/missing digests and scan
entries outside the selected module are refused. Scan and validate both name
the captured `ir_path`; before either child launch, the host verifies the exact
IR bytes against `ir_digest`, its `contract` discriminator and `modelVersion`.
A stale pin refuses before disclosure, not as a full-scope fallback. The
response digest covers the pins as well as the scope and source revision. Every request is still bound by the ordinary request ID,
verified adapter/capability identity, read-only confinement and deadline. The
host retains untouched diagnostic partitions and invalidates deleted ones;
[watch delivery](cli.md#lekalo-watch-experimental-issue-95) records actual
executed/reused partition provenance. Captured successor runs use exact
Model/IR/graph/impact `0.8.0` pins, including typed relations and secondary
uniqueness; predecessor canonical bytes and pins remain unchanged. Lock
schema digests, compiled IR identity pins and read-only artifact checks follow
the captured source version. The complete successor result and six pre-spawn
refusal cases are pinned under `tests/fixtures/watch/golden/`.

Protocol `0.9.1` retains scan/watch/lint custody and adds the full immutable
native plan to successful `plan-native` responses. Its digest, adapter identity
and optional summary counts must agree. Earlier summary-only responses cannot
authorize execution. [Native gates](native-gates.md) own the outer approval and
source/control/runtime custody.

## Scan entry evidence

The current schema can carry optional closed entry evidence: structural signature digest and at most eight outbound references with typed role (`read`, `create`, `update`, `delete`, `emit`, `call`, `reference`) and confidence (`exact`, `high`, `medium`, `low`, `unknown`). Missing evidence stays unknown. Core records it in observed evidence; signature changes can stale a binding instead of silently refreshing its mapping.

Rich adapter-local evidence need not fit this closed projection. A lossy scan must refuse; [the brownfield tutorial](tutorial-brownfield-typescript.md) labels its separate contributor fallback **Experimental**. Successful direct-kernel evidence does not qualify the public wire exchange.

## Resolved profile request

An operation carrying a profile token may also carry `profile_digest` and a bounded id-sorted `profile_capabilities` list. The paired members are admitted together according to the exact schema. They bind the resolved configuration and support claims; they do not carry arbitrary readable roots or grant permission expansion. The adapter's injected declared project view and host confinement remain separate boundaries.

## Requests and identities

Core sends a closed compact JSON envelope containing protocol,
protocol_version, operation, request_id and project_root (`.`). The latter
names a fresh private project view, never an ambient grant to the real root.

| Operation | ir_path | target | profile | dry_run | plan_id |
| --- | --- | --- | --- | --- | --- |
| describe | forbidden | forbidden | forbidden | forbidden | forbidden |
| scan | optional | optional | optional | forbidden | forbidden |
| bind | optional | required | required | forbidden | forbidden |
| validate | required | optional | optional | forbidden | forbidden |
| generate | required | required | optional | required | apply only |
| verify | required | optional | optional | forbidden | forbidden |
| plan-clean | optional | optional | optional | forbidden | forbidden |
| clean | optional | optional | optional | forbidden | required |
| plan-native | forbidden | optional | optional | forbidden | forbidden |

The read-only plan-native exchange (issue #48) carries the closed
`native_request`/`native_plan` members; see [native gates](native-gates.md).

`wire::request_id` computes the canonical envelope digest. Operational
client IDs additionally bind the private project identity, target/profile,
IR path, exact readable input bytes, output before-state and negotiated
capability digest. Absolute paths are not added to public wire envelopes.
Equivalent operations in different projects intentionally have different
IDs. Adapters echo the supplied request ID instead of recomputing it.

Describe is mandatory. It negotiates adapter identity/version/digest,
protocol versions, operations, transports, targets, profiles, read/write
scopes and optional structured progress. Every refresh revokes the old
handshake and pending plan before any fallible work, including successful,
failed and cancelled calls through `describe_with_cancel`. An adapter wire token,
exact version or declared-version negotiation mismatch returns `unsupported`,
exit 4/stdout, before generation; a missing capability has the same status,
exit and stream with its distinct reason code. Registry publication and versioning
preflight refusals retain `unsupported-version`, exit 5/stderr.

## Transport and decoding

The executable is launched from an argv vector without a command interpreter.
Requests use stdin or an exclusively created bounded request file, passed as
`--lekalo-request-file PATH`. Owned request resources are cleaned on every
return path, including spawn failure; Unix request files use mode 0600.
The response is one JSON envelope on stdout. Stderr is captured under a
64 KiB cap and never parsed as protocol or projected into public diagnostics.

Default limits are a 600-second child deadline, 8 MiB stdout and 1 MiB
request. Timeout, cancellation and output overflow terminate the owned
process tree. Windows uses a kill-on-close job; Linux uses a PID namespace;
the raw Unix transport uses its own process group. Reader joins never wait
unconditionally on a pipe retained by an escaped descendant. The macOS
confined profile prohibits descendant creation.

Runtime decoding rejects duplicate decoded keys (including escaped keys),
explicit null optionals, unknown fields, invalid tokens/versions, collection
bounds and operation-specific result shapes. All semantic fixtures execute
against production Rust decoding and validation with exact rule/detail
expectations. The pinned Ajv 8.20.0 gate checks the raw schema for duplicate
keys before parsing; schema acceptance is reported separately from semantic
runtime rejection. Neither a filename whitelist nor a fixture name is a
runtime proof.

Operation error codes contain 1–128 Unicode characters, counted as code
points rather than UTF-8 bytes or grapheme clusters. The production decoder
enforces this bound once; the client retains the validated internal code
without byte truncation. Public diagnostics still use fixed redacted codes.
Shared ASCII, Cyrillic, astral and combining-character vectors exercise the
decoder, client and exact Ajv gate at and beyond the boundary.

## Scopes and plans

Paths are canonical lowercase portable segments, at most 64 bytes per
segment and 512 bytes overall. Dot/traversal segments, trailing-dot aliases,
DOS devices, absolute paths and backslashes are rejected. Scopes optionally
end with `/**`; the unbounded root `**` is forbidden. Protected write homes
are `lekalo/`, `lekalo.lock`, `.lekalo/{ir,cache,import,privacy,consumer}/`
and `openspec/`.

An IR path must exist as a readable regular file and be covered by a read
scope. An empty read-scope list grants no input access. Only read-scope bytes
enter the private view. Existing write-scope files expose their shape as
empty placeholders unless also covered by a read scope; write authority
alone never exports existing user content.

The PHP/Laravel adapter `0.8.0` declares cache reads through the finite
`ir`, `transport`, `openapi` and `client-sdk` evidence homes. This lets
snapshots run while Windows holds the controller's exclusive
`.lekalo/cache/writer.lock`. The published package binds those same
capability scopes and grants; generation retains its normal ownership
and confirmation checks.

Describe, read operations and planning use a read-only private view enforced
by the OS. An adapter crashing on a denied write is classified as a crash;
core does not claim that a mutation occurred or silently turn it into a
successful dry run. Successful generate dry-run or plan-clean produces a
pending plan bound to the operation, full context, handshake and exact
before-state. The caller must echo core's opaque `plan_id`. Every apply
attempt consumes that authority, including failed/partial attempts.

All binding, context and before-state checks precede child launch. Create
requires absence; replace/delete require an existing regular file; clean
plans contain only deletions. Files over 4 MiB or otherwise unreadable refuse
verification. Snapshots include empty directories and reject links/special
entries. The scoped private view is bounded to 4096 entries and 64 MiB of
copied input, with a 64-directory depth limit and bounded directory enumeration. Unknown bytes never prove equality.

Apply writes only inside its private staged view. After the process tree exits,
core verifies the whole staged view, the exact echoed plan and output
hashes, then rechecks real inputs and before-state before publishing.
Exact file scopes support creation, replacement and deletion, including files
at the project root. Linux grants their parent directory inside the private
stage because unlink changes a directory entry; an individual writable file
bind mount cannot provide that authority. This grants no additional real
project access or readable input bytes, and changes to undeclared staged
siblings or protected inputs still fail whole-stage verification.
Undeclared writes, malformed replies, missing echoes and adapter errors
publish nothing. Atomic replacements do not modify hard-linked targets in
place. Ordinary publication I/O failures attempt rollback and explicitly
report incomplete rollback as partial. Multi-file publication is not a
crash-atomic filesystem transaction; concurrent privileged host mutation is
outside this process-confinement boundary.

## Platform boundary

Windows uses a fresh LPAC profile with the `registryRead` capability needed
by Node/libuv startup, an explicit inherited-handle list, and a job assigned
before resuming the suspended process. ACLs and integrity labels change only
on owned private staging. Projects with shared LPAC grants, callback ACLs,
reparse entries or an uninspectable/over-100000-entry tree are refused before
execution. LPAC retains OS-granted system access and registry reads: the
promise is confinement of project data, not zero operating-system access.
See Microsoft's [AppContainer launch guide](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer).

Linux requires `/usr/bin/bwrap`: separate user/mount/PID/network namespaces,
read-only system runtime roots and mount ancestors, and staged writable mounts. The product does
not install it; CI provisions it explicitly. Hosts restricting unprivileged
user namespaces through AppArmor also need an administrator-provided bwrap
launcher profile. The [AppArmor bwrap policy](https://gitlab.com/apparmor/apparmor/-/blob/apparmor-4.1/profiles/apparmor/profiles/extras/bwrap-userns-restrict)
permits namespace setup while stripping capabilities from executed children.
CI loads its dedicated variant into the ephemeral runner's kernel and removes
it afterward; it neither installs persistent policy files nor disables the
global namespace restriction. Core never changes host policy or retries with
weaker isolation when the backend is denied. macOS requires
`/usr/bin/sandbox-exec` with a deny-by-default profile. The private view and
all copied runtime paths use their canonical spelling. Its loader can read
the exact filesystem root directory, without access to descendant paths;
project reads remain confined to the private view. Projects inside an
allowed system runtime tree are refused. Missing or unsupported confinement
fails closed, with no ambient fallback. Linux/macOS behavioral qualification
comes from the exact-candidate hosted gates, never from Windows tests.

### Issue #95 delivery evidence boundary

The committed [platform qualification golden](../tests/fixtures/watch/golden/platform-qualification.json)
retains the original delivery snapshot, including its local WSL/macOS
qualification gaps. It is a historical fixture, not the current host verdict.

The [2026-10-24 CI run](https://github.com/ichinya/lekalo/actions/runs/40067697049)
at `715156dad7c22b7a5a137e33f1f062ec8c04b5ab` passed the watch gates on native
Windows, Linux and macOS runners. The matrix executes contracts, native/poll
platform behavior, boundaries, clean-command equivalence, fault/recovery,
successor Model pins and consumer/backpressure checks. This supplies native
macOS execution evidence; a cross-compile or a Windows/Linux pass alone does
not. Reproduce through the same three-OS CI matrix when changing the lane.
The earlier local WSL instability is not evidence of a current CI failure,
and this run does not qualify other kernels, every adapter or production hosts.

macOS admission still requires deny-default seatbelt, process-fork denial and
working sampled resident-memory supervision. That supervision is degraded,
not a hard allocation/RSS ceiling; absent or failed enforcement refuses or
terminates/reaps the owned process, with no ambient fallback. Successful CI
execution does not strengthen sampled supervision into a hard resource cap.

The executable and a first-argument script are copied into a private runtime.
Additional external runtime assets/packages need an explicit bundle
contract (**Planned**) and receive no implicit host grant. Node's preserve-symlinks flags
avoid reading host ancestors of the already link-free copied script. The
reference adapter is a standalone Node script; these host tests do not qualify every target package. The PHP adapter and native tutorial have separate checks and explicit execution limits.

## The Node/TypeScript kernel adapter (issue #43)

`adapters/node-typescript/adapter.mjs` is the concrete observed MVP
target adapter (`lekalo-target-node-typescript`, adapter release 0.4.0):
a self-contained Node bundle with vendored compiler/evidence assets. The bare kernel advertises describe only; configured scanner, native-plan, transport and dedicated generation bundles supply additional surfaces according to the resolved profile. Read/write authority stays explicit. See the adapter README for exact bundles and capability declarations. A scan can refuse when rich evidence cannot be represented by the closed public wire; the experimental tutorial records that refusal separately from its direct-kernel fallback.

Its limits are contractual, not incidental: read roots never come from
the wire (the base resolved profile carries digest/capability pairs,
not roots) but from an injected internal resolved project profile whose
roots are validated lexically and physically — links, junctions, and
escapes included — before any extension callback runs. Extension
evidence is preserved completely in the internal envelope and a
local-only sink; the closed public wire carries only what it can
represent, and refuses lossy projections instead of truncating. Node
runtime versions are reported by a local `--version-json` probe, not by
the handshake, which has no slot for them. The strict conformance
profile therefore fails `capability.surface` for this adapter by
design; the applicable default-profile rows pass. Adding operations or
wire members remains separately owned contract work.

The issue #70 transport extension adds one configured deployment
surface: when the bound resolved profile reads the transport evidence
home (`.lekalo/cache/transport/**`, written by `lekalo generate` from
the canonical `lekalo/transport.yaml`), the kernel advertises the
`generate` operation with `generate.transport-http: partial` and
`generate.openapi: unsupported`, a declared `src/routes/**` write
scope, and a deterministic route-layer plan derived from the single
evidence file — explicit `unsupported` notes for declared
streaming/upload/download capabilities it does not implement, never a
silent downgrade. Profiles that do not read the evidence home never
see the generator; the bare kernel keeps its `describe`-only surface.

## Failure classification and integration

| Failure | Public status / exit / stream |
| --- | --- |
| Invalid local request or plan mismatch | invalid / 1 / stderr |
| Adapter operation error | invalid / 1 / stderr |
| Scope/protected-home policy violation | denied / 3 / stdout |
| Unsupported capability (`target.capability-unsupported`) | unsupported / 4 / stdout |
| Spawn, deadline, cancellation, crash, malformed output, output cap | unavailable / 4 / stdout |
| Adapter wire token/version/negotiation mismatch (`target.protocol-mismatch`) | unsupported / 4 / stdout |
| Registry protocol unpublished or unsupported preflight version (`versioning.*`) | unsupported-version / 5 / stderr |

Public diagnostics use opaque path subjects and fixed adapter error codes.
`adapter-error-partial` preserves an adapter's partial-error claim without
publishing its arbitrary code/message. Neither partial nor an error envelope
proves that an ambient filesystem was unchanged: preservation comes from
isolation and refusal to publish its stage.

The integration surface is `TargetClient::describe` / `TargetClient::call`
plus the issue #28 discovery, capability-definition, and selection
modules. Pending plans remain session-bound; this protocol grants no persisted cross-session plan authority. Adapter installation and generation CLI behavior have separate owners.
`transport::run` is explicitly a raw process primitive for callers
owning its command; it does not implement scope policy and is never an
unconfined fallback for TargetClient.
