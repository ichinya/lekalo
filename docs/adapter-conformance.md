# Target adapter conformance suite

Issue #31 defines one shared battery of contract checks that any target
adapter must pass before it is considered compatible with an exact
protocol/IR version pair. The suite drives the adapter through the
production `TargetClient` (issue #27/#28/#29) against a hermetic embedded
fixture project; it owns no adapter catalog and trusts no fixture name.
The battery runs locally and in CI:

```sh
lekalo adapter test ./lekalo-target-node
lekalo adapter test --profile strict --report junit node adapter.mjs
```

Everything after the program path is passed to the adapter verbatim
(no shell), so adapter flags come last.

## Checks and classes

The closed catalog carries twenty-eight checks in fixed order (the
five `transport.*` rows were added by issue #70 and the five
`storage.*` rows by issue #117). Every check
has one inherent failure class; a recorded failure carries the class of
what actually failed, so a crash during a feature check is a
process-class failure and a refusal during any exchange is a
security-class one:

| Check | Class | Area |
| --- | --- | --- |
| `describe.handshake` | protocol | describe/handshake |
| `describe.negotiation` | protocol | protocol/IR version negotiation |
| `capability.declaration` | protocol | capability declaration |
| `capability.ir-declaration` | protocol | IR version backing of declared IR operations |
| `capability.surface` (strict) | protocol | complete operation surface |
| `confinement.canonical` | security | no mutation of canonical Lekalo/OpenSpec artifacts |
| `confinement.plan-scopes` | security | path confinement of declared writes |
| `input.invalid-ir` | feature | invalid input handling |
| `diagnostics.structured` | feature | structured in-envelope diagnostics |
| `determinism.repeats` | determinism | deterministic output across repeated runs |
| `generate.dry-run-plan` | feature | dry-run write plan |
| `generate.apply-plan` | feature | apply publishes exactly the plan |
| `apply.retry-discipline` | feature | crash/retry: a refused apply consumes its authority; retry replans |
| `plan.clean-cycle` | feature | plan-clean/clean cycle |
| `scenario.normalization` | feature | scenario result normalization |
| `artifact.manifest-evidence` | feature | applied bytes match declared digests |
| `redaction.evidence` | security | redaction of durable evidence |
| `process.cancellation` | process | cancellation and recovery |
| `transport.projection-parity` | feature | a declared transport generator derives its plan from the one evidence file deterministically |
| `transport.error-identity` | feature | error responses preserve `{id,code,category}`; infrastructure failures never carry declared ids |
| `transport.unsupported-capability` | feature | declared streaming/upload/download unsupported by the runtime is reported `unsupported`, never silent |
| `transport.blackbox-scenarios` | feature | fixture endpoint scenarios execute/normalize through the declared backend binding (execution stays with #47/#56/#107 owners) |
| `transport.wire-diff-block` | feature | breaking wire change in the fixture pair is classified `breaking` and blocks under `wire-consumer` |
| `storage.projection-parity` | feature | adapter's verify answer is an honest ok over the fixture and the mysql derivation holds (issue #117) |
| `storage.profile-evidence` | feature | honest `scan.schema`/`verify.schema-projection` capability declaration (issue #117) |
| `storage.introspection-checked` | security | declared scan surface answers a real read-only exchange; evidence grammar stays checked, read-only, credential-free (issue #117) |
| `storage.migration-gate` | feature | destructive diff of the fixture produces an explicitly gated plan step (issue #117) |
| `storage.collation-uniqueness` | feature | declared collation stays visible beside the derived unique index (issue #117) |

Skipped checks record a bounded reason (`operation-undeclared`,
`legacy-session`, `no-ir-operations`, `default-profile`,
`nothing-to-clean`, `no-error-observed`, `fixture-not-in-read-scopes`,
`no-repeatable-probe`, `not-run`, `no-plan`, `capability-undeclared`). A
skip is never a pass.

## Verdict, hard failures, and the badge

The verdict is a pure fold over the outcomes; hard classes dominate in
the order security → protocol → process, then any feature/determinism
failure:

| Verdict | Exit | Status | Meaning |
| --- | ---: | --- | --- |
| `pass` | 0 | `valid` | battery held |
| `feature` | 1 | `invalid` | feature/determinism failure (`adapter.check-failed`) |
| `process` | 4 | `unavailable` | exchange could not complete (`adapter.process-failure`) |
| `protocol` | 4 | `unsupported` | protocol conformance unproven (`adapter.protocol-failure`) |
| `security` | 3 | `denied` | confinement/redaction violation (`adapter.security-failure`) |

A security or protocol failure is never compensated by passing feature
tests: one `confinement.canonical` violation yields `denied` no matter
how many feature checks passed.

The verified-compatibility badge is issued only for a passing run whose
core rows are all present and passing, and it names the exact verified
versions — the negotiated protocol version and the core IR contract
version. A legacy session that negotiates 0.2.16 earns a badge for
protocol 0.2.16 exactly; no badge ever covers a declared range.

## Profiles

`--profile default` runs the full battery; optional operations the
adapter does not declare are skipped with a reason. `--profile strict`
additionally requires the complete v1 operation surface: an adapter
that omits any operation fails `capability.surface`.

`--repeats N` (2–8, default 3) bounds the determinism battery.
`--timeout-ms` (1000–600000, default 60000) bounds every child
exchange; a hung adapter is classified as a process failure within the
deadline, never hung with it.

## Fixture classes

The suite always runs against its own hermetic fixture project
(`tests/fixtures/adapter-conformance/`), one document per fixture class
of the issue:

- **minimal model** — the compiled typed IR of the accepted full-kinds
  model fixture, which also covers the **entity/command/query** class;
- **invalid references** — the same IR with one reference redirected to
  a symbol no definition declares;
- **transaction/concurrency** — a Scenario IR document (concurrent
  focus race, replay deduplicated through the idempotency key) decoded
  through the production Scenario IR module before any adapter runs;
- canonical `lekalo/` and `openspec/` homes, whose immutability is
  asserted after every exchange.

The remaining classes are adapter behaviors exercised by the suite's
regression battery against the reference fake adapter's fault
injection: **foreign implementation** (`--lekalo-adapter-identity`),
**unsupported capability** (`--lekalo-adapter-ops`), **malicious/path
traversal** (`mutate-dry`, `extra-write`), **nondeterministic**
(`nondeterministic`), and **partial failure** (`boom`).

## Reports

`--report json` prints the deterministic JSON envelope (status, report,
failure diagnostics) on stdout for every completed run; `--report
junit` prints the deterministic JUnit XML document instead — one
testcase per catalog row, `<failure type="class">` for failures,
`<skipped>` for skips, no timestamps. Both projections are
byte-identical across runs of the same adapter. Without `--report`,
the command renders the standard human/`--json` envelope; a failing
verdict still renders its envelope on the status-owned stream.

## Boundaries

The suite spawns the adapter only through the confined `TargetClient`
transport; it never runs an adapter outside confinement and never
falls back to an unconfined launch. A suite infrastructure failure
(fixture custody, temp root) is `unavailable` with
`adapter.process-failure`, never a silent pass.
The suite owns neutral fixture evidence and normalization assertions; scenario execution
backends stay with their own issues, and no persisted cross-session
plan authority exists.

## Manifest gate (issue #32)

`adapter test` resolves the launched entry through the adapter package
gate before the battery starts: the synthesized local-development
descriptor is integrity-checked, the signature policy is evaluated, and
the revocation store is consulted. The shipped adapter commits its
`adapter.manifest.json` with per-file digests, verified in CI by
`scripts/test-adapter-manifest-golden.mjs`. A gate refusal renders its
registered `adapter.*` rule and no check runs.
