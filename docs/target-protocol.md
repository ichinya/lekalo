# Target adapter process protocol

`lekalo.target/v1`, contract 1.0.0, connects core to separate executables.
Adapters may be written in any language; core loads no native plugin ABI.
The operations are describe, scan, bind, validate, generate, verify,
plan-clean and clean. Product 0.2.0, Model 1.0.0, IR 0.1.0 and diagnostic
registry 1.10.0 remain independent version lines.

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

`wire::request_id` computes the canonical envelope digest. Operational
client IDs additionally bind the private project identity, target/profile,
IR path, exact readable input bytes, output before-state and negotiated
capability digest. Absolute paths are not added to public wire envelopes.
Equivalent operations in different projects intentionally have different
IDs. Adapters echo the supplied request ID instead of recomputing it.

Describe is mandatory. It negotiates adapter identity/version/digest,
protocol versions, operations, transports, targets, profiles, read/write
scopes and optional structured progress. Every refresh revokes the old
handshake and pending plan before any fallible work. A protocol mismatch
remains `unsupported-version`/exit 5; missing capability remains
`unsupported`/exit 4.

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
expectations. The pinned Ajv 8.17.1 gate checks the raw schema for duplicate
keys before parsing; schema acceptance is reported separately from semantic
runtime rejection. Neither a filename whitelist nor a fixture name is a
runtime proof.

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

Apply can write only its staged output areas. After the process tree exits,
core verifies the whole staged view, the exact echoed plan and output
hashes, then rechecks real inputs and before-state before publishing.
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
read-only system runtime roots, and staged writable mounts. The product does
not install it; CI provisions it explicitly. macOS requires
`/usr/bin/sandbox-exec` with a deny-by-default profile. Projects inside an
allowed system runtime tree are refused. Missing or unsupported confinement
fails closed, with no ambient fallback. Linux/macOS behavioral qualification
comes from the exact-candidate hosted gates, never from Windows tests.

The executable and a first-argument script are copied into a private runtime.
Additional external runtime assets/packages need a future explicit bundle
contract and receive no implicit host grant. Node's preserve-symlinks flags
avoid reading host ancestors of the already link-free copied script. The
reference adapter is a standalone Node script; Go/PHP/Rust target packages
and the future generation CLI are not qualified by these tests.

## Failure classification and integration

| Failure | Public status / exit |
| --- | --- |
| Invalid local request or plan mismatch | invalid / 1 |
| Adapter operation error | invalid / 1 |
| Scope/protected-home policy violation | denied / 3 |
| Unsupported capability | unsupported / 4 |
| Spawn, deadline, cancellation, crash, malformed output, output cap | unavailable / 4 |
| Protocol/version mismatch | unsupported-version / 5 |

Public diagnostics use opaque path subjects and fixed adapter error codes.
`adapter-error-partial` preserves an adapter's partial-error claim without
publishing its arbitrary code/message. Neither partial nor an error envelope
proves that an ambient filesystem was unchanged: preservation comes from
isolation and refusal to publish its stage.

The integration surface is `TargetClient::describe` / `TargetClient::call`.
This issue provides no adapter catalog, native target package, generation CLI
or persisted cross-session plan authority. `transport::run` is explicitly a
raw process primitive for callers owning its command; it does not implement
scope policy and is never an unconfined fallback for TargetClient.
