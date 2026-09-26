# NFR: performance, reliability, and operational constraints with evidence (issue #85)

One independent, closed, versioned attachment — `lekalo/nfr/v0.4.0`,
identity `dev.lekalo.nfr@0.4.0`, contract
[contracts/nfr.schema.v0.4.0.json](../contracts/nfr.schema.v0.4.0.json) —
declares non-functional requirements as versioned semantic constraints;
one volatile measured-evidence document — `lekalo/nfr-evidence/v0.4.0`,
identity `dev.lekalo.nfr-evidence@0.4.0`,
[contracts/nfr-evidence.schema.v0.4.0.json](../contracts/nfr-evidence.schema.v0.4.0.json) —
carries measured results pinned to the exact constraint revision and
environment; one derived, read-only report — `lekalo/nfr-report/v0.4.0`,
identity `dev.lekalo.nfr-report@0.4.0`,
[contracts/nfr-report.schema.v0.4.0.json](../contracts/nfr-report.schema.v0.4.0.json) —
resolves every constraint into per-environment rows, first-class
statuses, and the gate verdict. Owner decisions are recorded in
[ADR-0042](adr/0042-nfr-constraints.md).

## Authority and boundaries

NFRs are Lekalo-owned semantic data. Functional invariants live in the
invariant-transition family
([docs/invariant-transition.md](invariant-transition.md)); a metric or
timing observation can never satisfy a scenario assertion
([docs/scenario-ir.md](scenario-ir.md)); external provider traceability
stays in the requirements family
([docs/requirements.md](requirements.md)) — the constraint's
`sourceRequirement` links back as plain data, and #36 is deliberately
not extended. Evidence arrives from the outside; the core never runs a
benchmark, never invents a value, and never merges results across
environments. The performance gate itself is executed by a backend
(native gate plans, HLV gates, CI checks); Lekalo only consumes its
evidence — `gateRef` is an opaque namespaced reference in the owner's
namespace.

## The constraint model

A constraint is a **claim**. It carries a stable `constraintId` (dotted
lowercase segments), a dimension (`runtime` or `ai-budget`), a closed
kind, a scope (`project`/`module`/`operation`/`endpoint` over semantic
ids), the declared `requirement`, `enforcement` (`mandatory` or
`advisory`), the declared `measurement` (method, optional `gateRef`,
optional evidence kinds), the accepted environments, required target
capabilities, a validity window with revision, and an optional
`sourceRequirement`.

Closed kind vocabulary, partitioned by dimension — the two dimensions
can never share a row (runtime: latency p50/p95/p99, throughput,
concurrency, availability, durability, timeout, retry-budget,
resource-limit, retention, consistency, freshness, rpo, rto, deployment,
runtime-constraint, accessibility-ref, security-ref, privacy-ref;
ai-budget: ai-cost, ai-token, ai-compute). **No universal thresholds**:
every comparator/value/unit is declared per constraint; the core only
evaluates `measurement ⊀ requirement` under the declared comparator and
never invents a default. Bound-less kinds (retry-budget, consistency,
deployment, runtime-constraint, the three reference kinds) carry no
numeric claim: the owner's pass receipt is the verdict.

`method: declaration` is the no-measurement spelling. It is legal only
under `enforcement: advisory` and reports unverified, never satisfied —
unverifiable prose becomes an explicit advisory row or a registered
`openQuestion` (bounded id, text-free by design), never false proof.

## Environment identity

One environment is the exact measured-conditions record: env id, the
owner-held target-profile and adapter snapshot digests, runtime,
platform, sorted labels. Two environments are compatible exactly when
all of these are equal — the same runtime in a different region is a
different environment. Evidence under a non-accepted environment stays
visible in the report's foreign section under its own key, with the
fixed mismatch reason, and never satisfies a bound. An empty accepted
set accepts the first distinct environment as the constraint's single
environment; every further distinct environment is foreign.

## The status engine

Per constraint, over current evidence (exact constraint revision, not
expired against the injected as-of date, compatible environment):
a `fail` receipt is `violated`; `skipped`/`unknown` receipts are
`unverified`; a `pass` receipt compares the measurement under the
declared comparator (numeric kinds) or is the owner's verdict
(bound-less kinds). Contradictory receipts at one environment and
revision are an explicit `conflict`, never an average. Aggregated
statuses, in precedence order: `unsupported` (a required capability
absent or below minimum in the resolved profile), `open-question`
(advisory declaration, by design), `stale` (revision mismatch, expiry,
or validity window ended before the as-of date), `conflict`,
`violated`, `satisfied`, `foreign-environment`, `unverified` — computed,
never declared; no wire spelling lets a document assert satisfaction.

Gate verdict (default): a `mandatory` constraint in `violated`,
`unverified`, `stale`, `unsupported`, or `conflict` denies the gate
(exit 3). Advisory rows are always visible and never block.
`--strict` escalates advisory `violated`/`unverified`/`stale` into the
denied set (the `impact --profile strict` precedent). The runtime and
ai-budget sections render disjoint.

## Determinism and denial

The resolution is a pure function of the attachment, the supplied
evidence sets, the resolved capability snapshot (from the committed
project lock when present), and the injected as-of date — never a
clock. Custody first: the attachment binds exactly one project, Model
state (load-envelope digest), and IR state (canonical IR digest);
`nfr.model-ref-mismatch` denies before any work. Every bound violation
rejects with a registered `nfr.*` diagnostic (LEK-NFR-001..013) and no
partial result.

## CLI

- `lekalo nfr validate ATTACHMENT [--evidence FILE ...] [--strict]
  --as-of DATE --project DIR` — the gate: exit 0 pass, 1 invalid,
  3 denied, 4 unavailable.
- `lekalo nfr report ATTACHMENT [--evidence FILE ...] --as-of DATE
  --project DIR` — the canonical report bytes plus digest.
- `lekalo nfr query ATTACHMENT SELECTOR [--evidence FILE ...]
  --as-of DATE --project DIR` — the closed selectors: `report`,
  status words, `foreign-environment`, `open-questions`,
  `coverage-gaps`, `constraint:ID`, `symbol:SEMANTIC-ID`.
- `lekalo nfr trace ATTACHMENT [--evidence FILE ...] --as-of DATE
  --project DIR` — the neutral #22 trace projection.
- `lekalo nfr diff BASE CANDIDATE` — the semantic diff; the verdict
  stays data.
- `lekalo nfr impact CANDIDATE --base BASE [--depth N] --project DIR`
  — the changed constraints through the accepted impact engine.

## HLV/AIFHub consumption

The evidence document is the neutral wire: valueState-typed
measurements, exact environment identity, owner-held artifact digests —
a runner of any origin can emit it, and the trace projection
([docs/trace-manifest.md](trace-manifest.md)) carries the
constraint→symbol→gate chain into the same neutral manifest every
external system already consumes.
