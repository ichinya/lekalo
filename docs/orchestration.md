# The generate and verify orchestration (issue #91)

Two unified core commands turn the accepted seams into the generation and
verification pipelines: `lekalo generate` validates the Model, binds the
exact lock and inputs, resolves the target profile and adapter
capabilities, computes the affected scope, requests adapter write plans
through the published `lekalo.target/v1` protocol, validates paths and
ownership, executes only on an explicit command, verifies the actual
writes against the plan, and maintains the artifact manifest and
evidence atomically. `lekalo verify` aggregates the read-only phases —
core validation, drift, per-target adapter validation, and the optional
binding, scenario, and trace summaries — and never writes. No target
logic lives in the Rust core: the adapter owns every target decision,
and the receipts are one model projected to both renderers. The design
decision is [ADR-0038](adr/0038-generate-verify-orchestration.md).

## Receipt contract

Every success emits one closed receipt
([`contracts/orchestration-report.schema.v1.0.0.json`](../contracts/orchestration-report.schema.v1.0.0.json),
discriminator `lekalo/orchestration/v1.0.0`, identity
`dev.lekalo.orchestration-report@1.0.0`) — independent of the product
release and of every other contract family. Both renderers project the
same receipt; the human line is a one-line summary of the same data.

- `inputs` pins the canonical Model and typed-IR payload digests and the
  revision digest over both, exactly the #21 binding domain.
- `irEvidence` names the canonical IR cache file
  (`.lekalo/cache/ir/<project>.json`) and its digest: the only bytes an
  adapter may read as input. Generate maintains the file (runtime
  cache, atomic replace, read-back check); verify consumes it and
  refuses a stale or absent file as `lock.stale` instead of writing.
- `scope` records the attribution scope: every module, or one module.
- Generate receipts carry one isolated row per target (`planned`,
  `applied`, or `failed`) with the protocol plan id, the sorted write
  actions with their exact content digests, and — for an apply — the
  updated ownership-manifest digest. Target failures stay isolated
  rows; the aggregate envelope preserves every failure diagnostic.
- Verify receipts carry one row per executed or declared-absent
  component with `required` true or false, the closed state, and the
  registered reason token. The verdict and every component state ride
  in the receipt; partial success is never spelled as full success.
- `verdict` is `ready`, `degraded`, or `blocked`. The exit class
  follows the accepted envelope: ready 0, blocked 1 (or the failure's
  own class 3/4/5), degraded 4, version refusals 5, policy denials 3.

## The catalog seam: `lekalo lock --adapter`

Adapters are separate executables; the lock pins their identity, and
nothing installs them. `lekalo lock --adapter PROGRAM [-- ARGS]` binds
one invocation-supplied program: the safe describe handshake negotiates
the session, and the sealed `#10` candidate supply is built from the
discovery outcome (single-entry project adapters: the package digest,
the source-snapshot digest, and the launched-entry digest are the same
SHA-256 over the committed script's exact bytes). The request names the
discovered adapter id, and the resolver's accepted compatibility
preflight runs unchanged — an adapter whose declared protocol or IR
range cannot cover the requested contract versions refuses as
`versioning.adapter-incompatible` (exit 5), never with a widened
declaration. The adapter entry must live inside the project root so the
lock carries a `project` source coordinate and never an absolute path.
`lekalo lock --check` (and every verification of a present lock) stays
request-current by merging the component identities the lock itself
pins into the verification request. `lekalo update` keeps the empty
supply; regeneration through a new `lekalo lock` after removing the
file is the documented recovery.

## Generation: `lekalo generate`

```text
lekalo generate --check [--locked] [--project DIR]
lekalo generate --clean (--dry-run | --confirm sha256:PLAN_ID) [--project DIR]
lekalo generate [--target TARGET]... [--module MODULE] [--dry-run]
                [--locked] -- PROGRAM [ARGS...] [--project DIR]
```

The preflight runs the accepted pipeline (root discovery, structure,
load, compile, exact lock, manifest) and, under `--locked`, the
`LockRequirement::Required` inventory verification — with the supply
entry as the declared local artifact, or an empty inventory, which
passes only contract-only locks. The requested targets (or every target
the discovered adapter declares) are resolved per target:

1. the target must be declared by the adapter, else the isolated row
   fails with `target.capability-unsupported`;
2. the IR evidence must be covered by the adapter's declared read
   scopes, else `target.capability-unsupported`;
3. the dry-run exchange produces the plan and its opaque protocol plan
   id; the dry-run lists every intended file action;
4. ownership validation refuses a plan that touches the manifest
   bookkeeping or any never-overwritten lifecycle
   (`target.protected-path`, exit 3);
5. the explicit apply consumes the bound plan authority, and the
   protocol client verifies the staged bytes, the echoed plan, the
   before-state, and publishes atomically with rollback;
6. the pipeline re-verifies the published bytes against the plan
   (`target.plan-mismatch` otherwise) and replaces the ownership
   manifest atomically: entries are upserted with the run's adapter
   reference, the generated lifecycle, and the scope's definition ids
   as `input_refs`; a stale manifest may be replaced only by a
   full-scope run that regenerates every recorded generated entry — a
   scoped run refuses with `lock.stale` instead of dropping ownership
   it did not regenerate.

`--module M` scopes the attribution (the manifest anchor and
`input_refs`) and the reported scope; the adapter always plans and
applies its own declared scope, because the wire protocol does not
transport a module filter and core never filters an adapter's plan.

### Ownership attribution

Every generated artifact is owned by the reserved generation anchor
`<project>.generated` (a two-segment spelling in the reserved
generation namespace), with `input_refs` carrying the sorted unique
definition ids of the run's scope. Ownership is declared by the run,
never inferred from names.

## Verification: `lekalo verify`

```text
lekalo verify [--target TARGET]... [--module MODULE] [--changed]
              [--locked] [--trace PATH] [-- PROGRAM [ARGS...]]
              [--project DIR]
```

`--changed` resolves the affected scope through the accepted Git handoff
(the working tree) and records the affected modules in the receipt;
mandatory cross-module errors are never hidden. The components, in
fixed id order:

| Component | Required | Notes |
| --- | --- | --- |
| `artifact.drift` | yes | the #21 read-only drift gate with verdict counts |
| `adapter.<target>` | yes (explicit `--target`) | describe + binding + the read-only `verify` operation; the IR evidence must exist and match the inputs revision |
| `bindings.registry` | no | freshness of the #42 registry; a never-recorded index is absent |
| `model.validation` | yes | the #12 semantic validation with severity counts |
| `native.gates` | no | declared absence (`core.capability-unavailable`) until the native gates land |
| `scenarios.execution` | no | declared absence until the #107/#47/#56 backends land |
| `scenarios.portable` | no | the portable scenario coverage summary |
| `trace.summary` | no | the neutral trace summary when `--trace` names a manifest |

The exit class distinguishes the acceptance classes: any component
failure is invalid (exit 1), a required component that is unsupported
or absent degrades to exit 4, optional degraded findings degrade to
exit 4, and the declared permanent absences stay exit-neutral —
reported in the receipt, never silently skipped, and never counted as
execution. Verify writes nothing anywhere; the drift gate and the
binding registry are read, never repaired.

## Diagnostics and exits

The registry is closed at the accepted integrated version and this
surface adds no rules: every failure maps onto one registered rule of an
accepted family (`lock.*`, `structure.*`, `target.*`, `adapter.*`,
`semantic.*`, `observed.*`, `validate.*`, `loader.*`,
`versioning.*`). Status — and therefore the exit class — is owned by the
mapping, never by severity; the aggregate envelope preserves every
isolated target diagnostic with the deterministic worst-class
precedence (5 > 1 > 3 > 4).

## Gate

- `cargo test --workspace --locked` — the core pipeline, the CLI
  behavior, and the exit classes.
- `node scripts/test-orchestration-contracts.mjs` — the receipt schema,
  the pinned golden receipts (regenerate with the documented fixture
  sequence when the fixture project bytes change), and the
  cross-language invariants, with exact Ajv 8.17.1 on Node 18 and 24.
