# Lekalo typed expressions

Issue #66 makes planner conditions and field assignments
first-class, machine-checkable contract data. One closed, versioned
attachment —
[`contracts/expressions.schema.v1.0.0.json`](../contracts/expressions.schema.v1.0.0.json)
(`lekalo/expressions/v1.0.0`, identity `dev.lekalo.expressions@1.0.0`)
— binds one project to one exact Model pin and IR identity and
declares named typed-expression records: boolean **conditions** and
field **assignments** over a small deterministic language. See
[ADR-0040](adr/0040-typed-expressions.md) for the owner decisions.

The attachment is declaration plus reference evaluation only: it
never executes against project data, never reads a wall clock, a
file, or a network, and has no write surface beyond the declared
assignment target. No arbitrary call, loop, recursion, reflection,
eval, target code snippet, or hidden mutable state has any
representation in the grammar — those shapes cannot be expressed,
so they cannot slip through validation. Contract identity is
independent of the product release, of the Model/IR versions, of
the Scenario IR, and of the diagnostic registry.

## Grammar v1

- **Literals**: booleans; integers bounded to ±2^53−1 (exact in
  every target and in a JSON double); BMP-only UTF-8 strings
  (≤256 bytes); canonical UTC datetimes; integer-second durations
  (≤1000 years); homogeneous sorted set literals (≤64 members).
- **References**: `input.*`, `actor.*`, `entity.*`, `result.*`,
  declared as typed record parameters (≤64); a nullable reference
  may appear only as the operand of `is-null`/`not-null`, so null
  flows are typed, not assumed away.
- **Operators**: equality (`eq`/`ne`), comparison
  (`lt`/`le`/`gt`/`ge`, orderable types only), checked integer and
  temporal arithmetic (`add`/`sub`/`mul`/`div`/`mod`),
  null tests, set membership (`in-set`/`not-in-set`), bounded
  boolean combinators (`and`/`or`, fanout 16) and `not`.
- **Conditional**: `if` with a boolean condition and typed branches
  — no loops, no recursion, no unbounded iteration.
- **`now`**: the deterministic clock reference; the evaluation
  instant is injected per call, never read from the environment.
- **Built-ins**: fifteen target-neutral functions with versioned
  semantics (`builtinSemantics`, `1.0.0`): string
  length/concat/casing/starts-with/ends-with/contains, `int-abs`,
  the calendar accessors (`datetime-year`/`-month`/`-day`/
  `-weekday`), `duration-seconds`, and the two closed casts
  (`cast-int-to-string`, `cast-string-to-int`). Casing is
  ASCII-only by contract so every target agrees byte for byte.
- **Assignments**: one declared target field per assignment record;
  `input` and `entity` are writable, `actor` and `result` are
  read-only contexts, and an assignment's body type must equal its
  target's declared type.

Static typing is exhaustive: every operator and built-in argument
combination is decided at declaration time, and an invalid
combination is the registered
[`expression.type-invalid`](diagnostics.md) refusal — never a
runtime surprise. The grammar carries hard bounds (depth 12, 256
nodes, 10 000 records, 32 MiB canonical bytes); a computation that
exceeds them is refused with `expression.complexity-limit` and
belongs in the foreign implementation family
([ADR-0033](adr/0033-foreign-implementation-escape-hatch.md)) —
the DSL does not grow.

## Reference evaluation and cross-target fixtures

The Rust evaluator is the reference implementation: pure, total
over validated attachments, and clock-injected. Bindings are
validated against the declared references before anything runs
(`expression.binding-invalid`); every domain failure is one closed
token (`int-overflow`, `duration-overflow`, `datetime-overflow`,
`divide-by-zero`, `modulo-by-zero`, `cast-invalid`,
`concat-overflow`) identical to the token the generated targets
emit.

One compiler projects a validated attachment into one complete,
self-contained program per target — Node (BigInt), PHP, and Go —
that reads the shared
[evaluation-vector document](../contracts/expressions-vectors.schema.v1.0.0.json)
(`lekalo/expressions/vectors/v1.0.0`) on stdin and writes the
computed results to stdout. The clock is a per-vector field: a
vector that omits it reads the shared epoch default
(`1970-01-01T00:00:00Z`) in the reference and in every generated
target, while a present-but-malformed clock refuses with
`clock-invalid`, so the
shared fixtures prove semantic equivalence by execution:
[`tests/fixtures/expressions/vectors.json`](../tests/fixtures/expressions/vectors.json)
drives the reference evaluator, the CI assertions, and every
generated program over the same cases. Before evaluating any row,
every generated program runs the same eager binding validation as
the reference: scope/field shape, unknown scopes and fields,
presence of every declared reference (nullable included),
nullability, exact JSON scalar types, the family value-domain
bounds, canonical datetimes, and set size/sorting/duplicates —
including values on branches the body never takes. Binding-stage
refusals beyond those vectors and clock validation cannot appear in
that document; the executed
projection gate (`tests/expressions_projection.rs`) runs every
generated program against those exact shapes and requires the same
closed tokens the reference refuses with.

## Managed mode: capability snapshots

Every feature carries a capability token: `expression.core` for the
closed grammar, `expression.builtin/<name>` per built-in. A target
or adapter declares its snapshot
([`contracts/expressions-builtin-support.schema.v1.0.0.json`](../contracts/expressions-builtin-support.schema.v1.0.0.json));
validating or generating against a snapshot that lacks a required
token blocks managed mode with `expression.builtin-unsupported`.
The escape hatch is the foreign implementation family — never a
silent partial generation.

## Canonical form, diff, and impact

Canonical bytes are compact UTF-8 JSON with byte-sorted keys;
set literals normalize to sorted duplicate-free form and
combinators keep their declared operand order, so the SHA-256 of
the canonical bytes is a stable attachment identity. The pure
semantic diff over same-family attachments classifies every changed
path as **breaking** (a removal, a result/target type change, a
removed or narrowed reference, a moved semantics pin),
**non-breaking** (an addition, a description, an added reference),
or **policy-change** (a body change, a target move, a widened
reference — still declared, computes differently), so expression
changes are visible in every semantic-diff and impact consumer.

## CLI

```sh
lekalo expressions validate tests/fixtures/expressions/valid/planner.json
lekalo expressions validate planner.json --builtin-support support.json
lekalo expressions eval planner.json --vectors vectors.json
lekalo expressions render planner.json --target node
lekalo expressions diff base.json candidate.json
```

The binary only selects, renders, and maps exits; every decision
lives in the core. Failure envelopes stream to stderr with exit 1;
success envelopes stream to stdout with the capability summary,
canonical digest, evaluation results, rendered program, or diff
classification.

## Contract custody

- [`contracts/expressions.schema.v1.0.0.json`](../contracts/expressions.schema.v1.0.0.json) — the attachment contract,
- [`contracts/expressions-vectors.schema.v1.0.0.json`](../contracts/expressions-vectors.schema.v1.0.0.json) — the shared evaluation-vector contract,
- [`contracts/expressions-builtin-support.schema.v1.0.0.json`](../contracts/expressions-builtin-support.schema.v1.0.0.json) — the capability-snapshot contract,
- [`tests/fixtures/expressions/`](../tests/fixtures/expressions/) — the shared fixtures (valid planner, 87 vectors, 28 invalid refusals, capability snapshots, diff pair),
- `scripts/test-expressions-contracts.mjs` — the independent Node release gate (pinned Ajv 8.17.1).

Failures emit the accepted #11 diagnostic contract with the
registry minor 1.24.0 → 1.25.0 (additions only):
`expression.input-invalid` (LEK-EXPR-001),
`expression.contract-invalid` (LEK-EXPR-002),
`expression.type-invalid` (LEK-EXPR-003),
`expression.complexity-limit` (LEK-EXPR-004),
`expression.builtin-unsupported` (LEK-EXPR-005),
`expression.eval-invalid` (LEK-EXPR-006),
`expression.binding-invalid` (LEK-EXPR-007),
`expression.diff-invalid` (LEK-EXPR-008), and
`expression.export-limit` (LEK-EXPR-009), category `semantic`,
with bounded fixed tokens and the declared source span on every
record-level refusal. See [docs/diagnostics.md](diagnostics.md).

## Boundaries

No runtime enforcement of conditions (that stays with #24), no
scenario execution (#23), no Model-bound field-type resolution
(#107 declares explicit types in v1), no authorization decisions
(#25), no adapter registry or code-generation integration — the
renderer supplies the shared cross-target fixtures the adapters
confirm. Expression *references* from the #63 `ExpressionRef`
grammar and the #64 filter grammar point into this family by
opaque identity.
