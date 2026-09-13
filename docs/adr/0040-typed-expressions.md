# ADR-0040: The bounded typed-expression language

Date: 2026-09-13
Status: accepted for issue #66

## Context

Issue #66 asks for typical planner preconditions, filters, and field
assignments to be expressible as data. Before this issue, conditions
and computed field values had no portable home: every target invented
its own predicate code, and no artifact could state, type-check, or
diff the actual semantics of a condition. The goal is deliberately
narrow — a small deterministic expression subset, not a programming
language. The predecessor #64 filter grammar already names the seam:
richer conditions were left to "the future #66 typed-expression
family by opaque reference".

## Decision

### 1. One independent attachment family, not a Model or IR change

Expressions live in one closed, versioned attachment
(`lekalo/expressions/v1.0.0`, identity
`dev.lekalo.expressions@1.0.0`) following the established
attachment pattern: bound to one project, one exact Model pin, and
one exact IR identity plus digests, closed under
`additionalProperties: false`, identity independent of the product
release and of every other contract family. The Model and IR
contracts stay untouched; Model-bound field-type resolution remains
with #107 — v1 declares its references' types explicitly on the
record.

### 2. A closed grammar with hard denials, and the foreign escape

The v1 grammar is exactly: typed literals (booleans, bounded
integers, BMP-only strings, canonical datetimes, integer-second
durations, homogeneous sorted sets); typed references
(`input.*`/`actor.*`/`entity.*`/`result.*`, nullable only under
`is-null`/`not-null`); eleven binary operators (equality,
comparison, checked arithmetic); null tests; set membership;
bounded `and`/`or`/`not`; the loop-free conditional; `now`; and
fifteen registered built-ins with a versioned semantics pin. There
is no representation — no node kind, no wire spelling, no escape
hatch inside the grammar — for arbitrary function calls, loops,
recursion, reflection, eval, filesystem or network access, target
code snippets, or hidden mutable state. Those shapes cannot be
expressed, so they cannot be validated into existence. Computations
that exceed the closed grammar (depth 12, 256 nodes, fanout 16,
64 set members, 64 parameters, 10 000 records per attachment) are
refused with `expression.complexity-limit` and belong in the
foreign implementation family (ADR-0033, issue #30). The DSL does
not grow; the escape hatch absorbs growth.

### 3. Static typing is exhaustive and evaluation is total

Every operator combination is decided at declaration time by the
reference typer: an invalid type or operator combination is a
registered `expression.type-invalid` refusal, never a runtime
surprise. The reference evaluator is pure and total over validated
attachments: the only ambient input is the injected clock
(`now` evaluates to exactly the caller-provided instant, which is
how tests stay deterministic), every arithmetic result is
range-checked against ±2^53−1 (exact in Rust i64, PHP/Go 64-bit
integers, ECMAScript BigInt, and any JSON double a reader may
interpose), and every domain failure (division or modulo by zero,
overflow, bad cast, concat overflow) is one closed error token
identical to the token the generated targets emit. Bindings are
validated against the declared references before anything runs.

### 4. Cross-target equivalence is structural, then proven

One compiler projects a validated attachment into one complete,
self-contained program per target (Node with BigInt, PHP, Go) that
reads the shared evaluation-vector document on stdin and writes
results to stdout. Datetimes are integer seconds under one
civil-calendar algorithm, casing is ASCII-only by contract, strings
are BMP-only so code-point order is byte order, and division
truncates toward zero identically everywhere — equivalence holds by
construction, and the shared fixtures prove it by execution. No
generated program reads anything but stdin, writes anything but
stdout, or calls anything but its own prelude.

### 5. Managed mode is capability-tokened; the snapshot gates it

Every grammar feature carries a capability token (`expression.core`,
one `expression.builtin/<name>` per built-in). A target or adapter
declares its snapshot; validating or generating against a snapshot
that lacks a required token blocks managed mode with
`expression.builtin-unsupported`, and the escape hatch is the
foreign family — never a silent partial generation.

### 6. Canonical bytes, semantic diff, and explainable refusals

Canonical form is compact UTF-8 JSON with byte-sorted keys and
sorted duplicate-free set literals; the SHA-256 of the canonical
bytes is the stable identity consumed by diff and impact. The pure
semantic diff classifies every changed path as breaking,
non-breaking, or policy-change (a changed body reshapes behavior
while keeping the declaration), so expression changes are visible in
every semantic-diff/impact consumer. Every refusal carries its
declared source span and bounded fixed tokens — never raw input,
paths, or digests.

## Consequences

The diagnostic registry advances 1.24.0 → 1.25.0 additively with the
nine `expression.*` rules (LEK-EXPR-001..009). The product carries
prospective version 0.2.15. The Node release gate
`scripts/test-expressions-contracts.mjs` is the independent second
canonical implementation of the wire contracts, joined to the pinned
Ajv 8.17.1 CI sequence. Runtime enforcement (where conditions
actually gate), scenario execution, Model-bound field resolution,
and code-generation integration stay with #24/#23/#107 and the
target adapters; this family supplies the declared, typed,
evaluable, and renderable contract they consume.

## Boundaries

No execution scheduling, adapter registry, authorization decision,
transaction semantics, report/trace surface, or filesystem/network
access belongs here. Tests are hermetic; fixtures live under
[`tests/fixtures/expressions/`](../../tests/fixtures/expressions/).
