# ADR-0038: the generate and verify orchestration

Status: accepted (issue #91). Builds on ADR-0003 (structure), ADR-0009
(lockfile), ADR-0011 (validation), ADR-0014 (scenario IR), ADR-0015
(artifact ownership), ADR-0016 (cache), ADR-0021 (authorization),
ADR-0025 (target protocol), ADR-0029 (target profiles), ADR-0030
(adapter conformance), ADR-0032 (doctor readiness), and ADR-0035
(bindings registry). Consumers: #103 (reports), #107 (fake-reference
execution), #47/#56 (native runners).

## Context

The issue #1 milestone list closes the M2 arc with the orchestration
commands. Every seam exists in accepted form: the target protocol with
planned-and-applied exchanges and process confinement (#27), capability
discovery and selection (#28), composable profiles (#29), the ownership
manifest and drift gate (#21), the lock and its required-verification
preflight (#10), semantic validation (#12), the scenario IR (#23), and
the binding registry (#42). What is missing is the unified core
pipeline and its result wire: nothing binds a concrete invocation to
the exact lock, inputs, adapter, plan, and manifest, and nothing
aggregates the verification phases into one deterministic receipt.

## Decision

1. **Two core services, one receipt family.** `orchestration::generate`
   and `orchestration::verify` own the pipelines; the CLI selects,
   renders, and maps exits. Both emit closed receipts published as
   `dev.lekalo.orchestration-report@1.0.0`
   (`lekalo/orchestration/v1.0.0`), independent of every other version
   line, with one gate (`scripts/test-orchestration-contracts.mjs`)
   validating the schema, the pinned goldens, and the cross-language
   invariants with exact Ajv 8.17.1.

2. **The catalog seam lives in the lock command.** Adapters are
   operator-supplied programs, never installed by core.
   `lekalo lock --adapter PROGRAM [-- ARGS]` discovers through the safe
   describe handshake, builds the sealed `#10` candidate supply from the
   discovery outcome, and resolves the lock through a request that names
   the discovered adapter id. Single-entry project adapters pin one
   digest three ways (package, source snapshot, launched entry). The
   accepted compatibility preflight runs unchanged; declared ranges are
   never widened, so a legacy-only adapter refuses at resolution as
   `versioning.adapter-incompatible`. Verification of a present lock
   merges the lock's own component identities into the verification
   request, so a catalog lock stays request-current for callers that
   name no components. `lekalo update` keeps the empty supply.

3. **Generation is plan-first and never trust-based.** Every apply is
   the protocol's planned-and-applied exchange: the dry-run plan binds
   the plan id, context, and before-state; the apply consumes that
   authority; core re-verifies the published bytes against the plan and
   replaces the ownership manifest atomically only after the protocol
   published the staged bytes. Ownership is declared by the run: the
   reserved generation anchor `<project>.generated` with the scope's
   sorted definition ids as `input_refs` — never inferred from names. A
   stale manifest may be replaced only by a full-scope run whose plan
   covers every recorded generated entry; a scoped run refuses with
   `lock.stale` rather than silently dropping ownership. Plans that
   touch the manifest bookkeeping or a never-overwritten lifecycle are
   ownership-policy denials (exit 3).

4. **The canonical IR evidence is cache, not artifact.** Generate
   writes the canonical typed-IR bytes to
   `.lekalo/cache/ir/<project>.json` (runtime cache home, atomic
   replace, read-back check); the file must be covered by the adapter's
   declared read scopes or the run refuses with
   `target.capability-unsupported`. Verify never writes: it requires
   the evidence to exist and match the current inputs revision
   (`lock.stale` otherwise). The evidence is never a manifest artifact
   and never the only copy of a semantic decision.

5. **Isolation and aggregation rules.** Target failures stay isolated
   rows and the remaining targets complete; the aggregate envelope
   preserves every failure diagnostic with deterministic worst-class
   precedence (unsupported-version > invalid > denied > unsupported >
   unavailable). Verify distinguishes required components (model
   validation, drift, adapter validation for an explicit target) from
   optional ones (bindings, portable scenarios, trace, and the two
   declared permanent absences for scenario execution and native
   gates). Any failure blocks; a required non-pass or any degraded
   findings degrade (exit 4); declared optional absences are
   exit-neutral and reported in the receipt — never silently skipped,
   never counted as execution.

6. **No new diagnostic rules.** The registry stays at the accepted
   integrated version: orchestration failures map onto the registered
   `lock.*`, `structure.*`, `target.*`, `adapter.*`, `semantic.*`,
   `observed.*`, `validate.*`, and `loader.*` families with bounded
   tokens. The reserved 1.23.0 registry identity stays unconsumed.

## Alternatives considered

- **Supply through a package catalog.** Rejected: the provider/catalog
  milestones (#32+) own candidate supply from remote sources; the
  describe-based bridge keeps the same sealed-resolution guarantees
  without inventing an install surface.
- **Filtering adapter plans by module.** Rejected: the wire protocol
  does not transport a scope filter, and core filtering would break the
  plan binding and the dry-run/apply cross-check. Module scoping is
  attribution and reporting scope.
- **Inferring per-symbol ownership.** Rejected: the protocol does not
  transport per-write ownership and inference from names violates the
  semantic-ID authority. The declared anchor plus `input_refs` records
  honest provenance.
- **Writing IR evidence during verify.** Rejected: verify never writes
  anywhere; a stale evidence file is a `lock.stale` refusal, and
  `lekalo generate` is the recovery.

## Consequences

- #103 can project the receipts into report bundles; #107 and #47/#56
  replace the two declared absences with real execution components
  without a wire change; #42 binding freshness stays exit-neutral for
  projects that never adopted observed mode.
- The ownership manifest grammar continues to forbid runtime-home
  paths for entries; adapters that generate into the source tree
  (the shipped convention) record claims there, while
  `.lekalo/generated/` stays the v1 orphan scan root.
- `lekalo lock --check` remains the headless CI gate for catalog locks
  without any adapter supply on the invocation, because verification
  merges the lock's own identities.
- Every committed fixture that pins adapter bytes (the lock, the
  golden receipts) must be regenerated together when the fixture
  adapter bytes change; the gate documents the sequence.

## References

- [Orchestration](../orchestration.md)
- [Lockfile](../lockfile.md), [Target protocol](../target-protocol.md),
  [Target profiles](../target-profile.md),
  [Artifact manifest](../artifact-manifest.md)
- Issue #91; prerequisites #10, #11, #12, #21, #23, #27, #28, #29.
