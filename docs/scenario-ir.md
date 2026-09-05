# The Scenario IR (issue #23)

One portable behavioral scenario, written once and referenced by the
native test backend of every target. The Scenario IR is a derived test
contract — never canonical Model source, never runtime evidence — with
its own closed wire schema
([`contracts/scenario-ir.schema.v1.0.0.json`](../contracts/scenario-ir.schema.v1.0.0.json),
discriminator `lekalo/scenario-ir/v1.0.0`, identity
`dev.lekalo.scenario-ir@1.0.0`), independent of the product release, of
the Model/IR/graph/protocol contract versions, of the diagnostic
registry, and of every execution backend.

## Identity

- The scenario carries a stable `scenarioId` in the inherited
  [semantic-ID grammar](semantic-ids.md) (two or three segments, for
  example `planner.scenario.switch_focus`) plus an explicit
  `scenarioVersion`. The project is named by a one-segment root id.
- Every step carries a stable scenario-local `stepId` (one lowercase
  segment, at most 64 bytes). Identity is `scenario_id + step_id`;
  array position and text hashes are never identity, and the
  occurrence ordinal exists only for source and reference
  disambiguation.
- Every document binds the exact compiled inputs: `irRef` (the one
  accepted IR contract `dev.lekalo.ir@0.1.0` plus the canonical IR
  digest) and `modelRef` (the accepted Model version plus the source
  Model digest).

## Shape

- `given` (ordered, at most 256) establishes data and controls only:
  typed entity state (selector plus field values), an opaque fixture
  reference with its own version and capability set, a typed actor
  identity, a deterministic clock, or a deterministic ID source. No
  expressions, code, SQL, network fixtures, or target names.
- `when` (ordered, at least one, at most 256) is exactly one closed
  action, `invoke`: a qualified command or query, typed input fields,
  an optional actor, an optional clock control, and an explicit
  idempotency key where applicable. Optional bounded `replay`
  metadata points at a prior `when` step. Writes, effects,
  authorization, retries, and error categories are never inferred from
  operation names.
- `then` (ordered, at least one, at most 512) carries exactly one
  closed assertion observing a reachable action output or prior step:
  `result`, `error`, `entity_state`, `emitted`, `forbidden_effect`,
  `authorization`, `idempotency`, `contract_match`,
  `deterministic_fixture`, and the explicit `unsupported` expectation
  (a known-absent capability is declared, never silently passed).
  Assertions are separate from metrics and telemetry; a metric or
  timing observation can never satisfy an assertion.
- `bindings` (sorted set, at most 32) reference backends as data: the
  closed backend kind (`fake-reference` or `native`), a stable runner,
  independent runner/protocol/profile version pins, a capability-set
  digest, the stable native-test or fake-evaluator identity, and
  optional mode and evidence digests. Native tests stay
  source-native-owned; nothing is copied, inlined, or executed.
- `tags` (sorted set, at most 64) and dotted-namespace `metadata`
  (at most 64 entries, 64 KiB canonical bytes) are bounded and closed.
- The optional `sourceMapRef` pins the digest-bound source-map sidecar
  (identity `dev.lekalo.scenario-sourcemap@1.0.0`, at most 8192
  entries) that maps scenario, step, role, member path, reference
  role, and occurrence ordinal onto accepted logical paths and exact
  byte/line/column ranges. The sidecar never carries a physical root,
  source snippet, fallback zero span, or a request to reparse.

## Typed values and references

`TypedValue` is a closed recursive union with an explicit type tag:
null, boolean, 64-bit integer, bounded string, canonical decimal
(no exponent, no leading zero, no trailing zero fraction, no `-0`),
calendar date, canonical UTC datetime (offsets and leap seconds are
refused), canonical lowercase UUID, bounded credential-free URI, list,
and field-name-keyed object. Floats, arbitrary JSON, unbounded maps,
implicit coercion, and locale- or timezone-dependent spellings do not
exist. `Ref` is a closed union (semantic symbol, operation, entity,
field, event, job, effect, error, requirement, fixture, actor, step
output, given value, clock, ID source) with a qualified stable
identifier, an optional bounded member path, and an optional expected
type. Generic resolution and visibility stay with #12; error-union
membership with #62; effect meaning with #14; authorization with #25.

## Validation, reachability, and coverage

Normalization fails closed before semantic processing: unknown fields,
wrong identities, malformed identifiers or digests, bounds, and shape
violations each return one registered diagnostic and no partial IR.
The scenario-local data-flow pass rejects duplicate step identities,
empty scenarios, dangling observations, forward step-output
references, use of an unestablished given value or control, a clock or
ID-source reference to a step of the wrong kind, two state setups
claiming the same row with overlapping fields, an idempotency
assertion with no key or replay control, an actor reference with no
declared actor, and a `then` step that no action reaches. Command and
query existence, operation kinds, and input/output types remain #12
checks.

A valid scenario derives a stable [`CoverageVector`]: the scenario,
step, action, and assertion identities, the covered operations, the
expected outcome kind, and the optional typed error, effect, and
policy references. It is plain rebuildable data offered to the #13
graph — it never creates graph nodes, write edges, or
declared-versus-detected comparisons, and it never preempts execution.

## Determinism

Canonical bytes are compact UTF-8 JSON with byte-sorted object keys;
set-like collections (tags, binding capabilities and order, projection
paths, bindings) are sorted; behavioral order (`given`, `when`,
`then`, selectors) is preserved exactly. The same validated scenario
produces byte-identical bytes — and therefore the same digest — on
every host, independent of map insertion order, locale, timezone, cwd,
and line endings. Canonical payloads are bounded at 1 MiB; every bound
([`version.rs`](../crates/lekalo-core/src/scenario/version.rs)) rejects
with a typed diagnostic instead of truncating.

## Boundaries

Pure construction and validation perform no source, model, `.lekalo`,
cache, report, network, process, or target access. Execution belongs
to #107 (fake reference), #47/#56 (native runners); neutral evidence
production to #31/#107; aggregation to #91; reporting to #103;
transactions to #24; process and protocol conformance to #27/#31.
Diagnostics reuse the accepted #11 infrastructure rules
(`graph.input-invalid`, `graph.traversal-limit`, `graph.export-limit`)
with fixed detail tokens and bounded echoes; the registry file is
unchanged. There is no CLI command in v1; vectors run through the
contract gates (`scripts/test-scenario-contracts.mjs` with exact
Ajv 8.17.1 on Node 18 and 24, plus
`crates/lekalo-core/tests/scenario.rs`).

## References

- [ADR-0014](adr/0014-scenario-ir.md) — owner decisions.
- [docs/semantic-ids.md](semantic-ids.md) — the inherited ID grammar.
- [docs/effect-graph.md](effect-graph.md) — the typed seams this
  contract references.
