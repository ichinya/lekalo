# Target protocol (issue #27)

Status: normative for issue #27. This document defines the process/JSON
protocol `lekalo.target/v1` that connects the Lekalo core to external target
adapters. The design decision is [ADR-0025](adr/0025-target-protocol.md).

Target adapters are separate executables written in any language — Rust, Go,
Node.js, PHP, or anything else that can read stdin and write stdout JSON.
They are never Rust dynamic plugins, never `cdylib` extensions, and never
internal core dependencies: the only coupling between core and adapter is
this protocol. Core never loads a library, ABI, or plugin.

## Contract identity

- Wire token: `lekalo.target/v1`, exact contract version `1.0.0`
  (`dev.lekalo.protocol@1.0.0` in the embedded version registry, selector
  alias `protocol/v1`).
- Wire schema: `contracts/target-protocol.schema.v1.0.0.json`, closed
  (`additionalProperties: false` everywhere), draft 2020-12.
- Registry family: `protocol`, published by this issue. While it stayed
  unpublished, no external adapter could be compatible
  (`versioning.protocol-unpublished`); publication is what turns every
  downstream seam on (`lekalo lock` protocol pins, adapter compatibility
  preflight, artifact adapter binding).

## Operations v1

```text
describe    capability negotiation; the mandatory handshake
scan        read-only survey of target applicability
bind        module/target/profile binding proposal
validate    IR/target compatibility findings
generate    dry-run plan first, then a plan-bound apply
verify      post-generation verification findings
plan-clean  deterministic deletion plan (dry by definition)
clean       exact execution of a plan-clean plan
```

## Request envelope

Sent by core, serialized to compact canonical JSON (sorted keys, no
whitespace). The request identifier is derived, never random:
`request_id = "req-" ++ hex(sha256(canonical(request without request_id)))`,
so identical inputs produce identical identifiers across runs and hosts, and
every response echo binds to exactly one request.

```json
{
  "protocol": "lekalo.target/v1",
  "protocol_version": "1.0.0",
  "operation": "generate",
  "request_id": "req-…",
  "project_root": ".",
  "ir_path": ".lekalo/ir/planner.json",
  "target": "node-typescript",
  "profile": "default",
  "dry_run": true,
  "limits": { "timeout_ms": 600000, "max_output_bytes": 8388608 },
  "plan_id": "plan-…"
}
```

Closed per-operation member table enforced by the client:

| operation   | ir_path  | target  | profile | dry_run        | plan_id |
| ----------- | -------- | ------- | ------- | -------------- | ------- |
| describe    | —        | —       | —       | forbidden      | forbidden |
| scan        | optional | optional | optional | forbidden     | forbidden |
| bind        | optional | required | required | forbidden    | forbidden |
| validate    | required | optional | optional | forbidden    | forbidden |
| generate    | required | required | optional | required     | apply only |
| verify      | required | optional | optional | forbidden    | forbidden |
| plan-clean  | optional | optional | optional | forbidden    | forbidden |
| clean       | optional | optional | optional | forbidden    | required |

## Transport

- The adapter executable is spawned directly from an argv vector — no shell,
  no interpolation, no environment trust — in the project root.
- The request travels over stdin. An adapter that declares only the `file`
  transport receives the request through a bounded temporary file outside
  the project, delivered by appending `--lekalo-request-file <PATH>` to its
  argv. The file is deleted when the child exits.
- The response is one JSON envelope on stdout. stderr is diagnostics and
  logs only: captured under a 64 KiB cap as evidence, never parsed.
- Hard bounds: deadline (default 600 s), stdout cap (default 8 MiB), request
  cap (1 MiB). Exceeding the deadline or the caps kills the child and
  classifies as infrastructure. A caller cancel flag kills the child too.

## Response envelope and handshake

The response echoes `protocol`, `protocol_version`, `operation`, and
`request_id`, and always carries `evidence` binding the adapter identity
(`id`, `version`, package `digest`) and, for write-carrying exchanges, the
`plan_id`. `status` is `ok` or `error`; an error envelope carries the closed
error taxonomy (`class`: `invalid|unsupported|infrastructure|conflict`,
adapter `code`, bounded `message`, `retryable`/`partial` flags, `detail`).
`describe` additionally returns the capability map: protocol versions,
operations, transports, targets, profiles, read/write scopes, and whether
structured progress is emitted (progress is optional and legal only from an
adapter that declared it).

A protocol token or exact-version deviation anywhere is a protocol mismatch
and is refused as `unsupported-version` before any generation can start.
The negotiated version must appear in the adapter's declared
`protocol_versions`.

## Scopes, protected homes, write plans

- Scopes are declared in `describe`: portable lowercase segments with an
  optional trailing `**`; traversal (`..`), absolute paths, and backslashes
  never parse.
- Canonical homes can never be covered by a write scope and never appear in
  a write plan: `lekalo/`, `lekalo.lock`, `.lekalo/ir/`, `.lekalo/cache/`,
  `.lekalo/import/`, `.lekalo/privacy/`, `.lekalo/consumer/`, and
  `openspec/`. An adapter cannot silently mutate canonical Lekalo/OpenSpec
  files — the refusal is a denial (exit 3) even when the adapter declares
  the scope.
- Every `ir_path` the client sends must sit inside a declared read scope;
  every planned path must sit inside a declared write scope.
- `generate` requires `dry_run: true` first. The dry run returns the
  declared output plan — exact logical paths, `create`/`replace`/`delete`,
  and the SHA-256 of the exact resulting bytes — and must change nothing;
  the core snapshots the write scopes before and after and refuses any
  mutation as a denial.
- The apply echoes the deterministic `plan_id`
  (`"plan-" ++ hex(sha256(canonical(plan)))`), must reproduce the identical
  plan (deterministic adapters make plans reproducible; nothing persists
  between the two child processes), and after the child exits the core
  verifies the observed project state: every declared write matches the
  declared digest, every declared deletion is gone, and nothing else inside
  the declared write scopes changed. Violations are `target.plan-mismatch`.

## Error classification

Closed, with registered `target.*` diagnostics (registry v1.10.0,
`LEK-TGT-001..015`):

- infrastructure (exit 4, unavailable): spawn failure, request-write
  failure, deadline exceeded, caller cancellation, output-limit violation,
  crash (non-zero exit without a usable error envelope), invalid JSON,
  malformed envelope.
- protocol (exit 5, unsupported-version): foreign token, unsupported exact
  version, failed negotiation — refused before any generation.
- adapter capability (exit 4, unsupported): operation/target/profile not
  offered.
- policy (exit 3, denied): scope grammar/protection violations, dry-run
  mutation.
- operation (exit 1, invalid): malformed local requests, adapter-reported
  in-envelope errors, plan mismatches.

## Language neutrality

The fake adapter (`tests/fixtures/target-protocol/fake-adapter.mjs`) is a
dependency-free Node.js script implementing the full handshake — describe,
all eight operations, deterministic plans, and fault injection for the
contract tests (`--lekalo-fault wrong-token|wrong-version|garbage|crash|
hang|noise|bad-echo|boom|mutate-dry|extra-write`). It doubles as the
reference implementation an adapter author in any other language can port:
stdin, stdout, canonical JSON, SHA-256 — nothing else.

## Boundaries

Pure protocol definition and client transport: no generation execution, no
adapter catalog, no CLI surface (the `generate` owner integrates the client),
no persistence of plans between processes, and no network access. The thin
handoff for the generation owner (#91) is `TargetClient::describe` /
`TargetClient::call` in `lekalo_core::target_protocol`.
