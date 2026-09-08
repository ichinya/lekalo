# ADR-0025: Target protocol

Status: accepted for issue #27.

## Context

Target adapters turn the target-neutral IR into real project artifacts. The
M1 baseline deliberately left them undefined: the versioning registry ships
an empty `protocol` family, the lockfile refuses executable components, and
the compatibility preflight refuses every adapter
(`versioning.protocol-unpublished`). Two dead ends were rejected up front:
in-process Rust plugins (dynamic ABIs are unstable, coupling is total, and
non-Rust targets become second-class) and internal core dependencies (no
third party could ever ship a target). The remaining design space is a
process protocol: adapters as separate executables in any language.

## Decision

1. One closed wire contract, `lekalo.target/v1`, exact contract version
   1.0.0, identity `dev.lekalo.protocol@1.0.0`, published in the version
   registry's `protocol` family with the `v1` selector alias. Publication is
   the semantic act of this issue: it turns on the protocol pins in the
   lockfile, the adapter compatibility preflight, and the artifact adapter
   binding, all of which were built against this moment.
2. Eight operations in v1 — `describe`, `scan`, `bind`, `validate`,
   `generate`, `verify`, `plan-clean`, `clean` — over a request/response
   envelope pair. `describe` is the mandatory handshake: capability
   negotiation before any destructive operation.
3. Transport without shells: direct argv spawn in a private scoped project view, request
   over stdin (or a bounded temporary file for adapters that declare only
   `file`), one response envelope on stdout, stderr as bounded diagnostics
   evidence only. Deadline, cancellation, request and output caps are
   enforced by the client, and every refusal is classified infrastructure.
4. Deterministic identifiers bind canonical request/plan bytes and private
   project identity, operation, parameters, input/output before-state and
   negotiated capabilities. The public IDs are opaque digests. Adapters echo
   the supplied IDs; every apply attempt consumes its authority, and every
   handshake refresh revokes previous authority before attempting a launch.
5. Scopes and protected homes. Adapters declare read/write scopes in
   `describe`; the grammar is portable lowercase segments with an optional
   trailing `**`. Canonical Lekalo and OpenSpec homes are unwritable by
   declaration and by plan, whatever the adapter claims.
6. Dry-run write plans are mandatory before generation and clean. OS
   confinement makes describe/read/planning views read-only. An apply
   changes only its private staged output; core validates the full staged
   tree and exact plan echo before publishing any real project changes.
   Create/replace/delete preconditions and context are checked before child
   launch and again before publication. Ordinary I/O failures attempt
   rollback; incomplete rollback is partial. Publication across files is
   not a crash-atomic transaction.
7. Closed error classification: infrastructure (spawn/timeout/crash/invalid
   JSON/output cap — exit 4), protocol mismatch (exit 5), capability
   (exit 4), policy (exit 3), operation errors (exit 1, including partial
   results flagged by the adapter). Fifteen registered `target.*` rules
   (`LEK-TGT-001..015`) as the diagnostic registry's v1.10.0
   wire-shape-preserving minor increment.
8. The committed dependency-free Node.js adapter exercises the full
   handshake through real confinement. Native Go/PHP/Rust packages are not
   claimed tested. Ajv validates schema shape; Rust executes the semantic
   vectors through the production decoder and validators.
9. No CLI surface in this issue. The generation owner (#91) integrates
   `TargetClient`; this issue ships the client, the protocol, and the tests
   that exercise real cross-process transport.

## Consequences

- The version-registry bytes change, so every committed lock that pins the
  registry digest had to move to the published world (`contract-only.lock.json`
  now pins `dev.lekalo.protocol@1.0.0`); the publication-gate tests stay
  alive against synthetic unpublished registries.
- With the protocol published, artifact manifests whose entries carry no
  adapter ref are unbound when the lock names adapters; locks that name no
  adapters (all locks creatable before the #91 catalog) keep the v1
  byte-drift semantics of the #21 gate.
- `versioning.protocol-unpublished` remains a live refusal path for
  registries that do not publish the family (synthetic, older, or custom);
  it is no longer the answer the embedded registry gives.
- Adapter authors target a frozen 1.0.0 wire: closed shapes mean every
  future member is a reviewed protocol version, never a silent extension.

## References

- [Target protocol](../target-protocol.md)
- [Versioning and migrations](../versioning.md)
- [Diagnostics](../diagnostics.md)
- Issue #27; prerequisites #8 (IR), #9 (versioning), #10 (lockfile),
  #11 (diagnostics); consumer #91 (generation execution).
