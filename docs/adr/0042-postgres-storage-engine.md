# ADR-0042: PostgreSQL storage engine profile, checked-mode
# introspection, and gated migration plans

- Status: accepted
- Date: 2026-09-19
- Issue: #69 (M4)
- Depends on: #24 (transaction concurrency), #29 (target profiles),
  #31 (adapter conformance), #64 (query model), #65 (storage
  projection), #66 (typed expressions)

## Context

Issue #65 ships the domain-vs-storage boundary and a `postgres`
namespace projection — pure declaration, never SQL. #29 registers the
`postgres-sql` component. What #69 adds is the engine layer: an exact
engine version pin with per-version capability answers, the
deterministic DDL/migration emitters, adapter-produced checked-mode
introspection and drift comparison, the #24 capability mapping, and
the shared component conformance battery. #117 (MySQL) is implemented
in parallel and must reuse the same component shape.

## Decisions

1. **Three small families, not one.** Profile = declared policy,
   introspection = adapter-produced evidence, migration plan =
   computed proposal. The native-gates precedent (four families for
   one issue) supports the split; one family with optional members
   would mix declaration, observation, and computation. Each family
   publishes at `0.4.0`; the frozen predecessors stay byte-identical.
2. **The storage-projection family bumps to 0.4.0 additively.**
   `default` (closed literal/now/uuid_generate/sequence grammar — no
   free expressions), `enum`/`array` domain types, `index.where`, and
   `table.checks` are projection facts every namespace needs and the
   diff must classify, so they live in the attachment, not the
   profile. The Laravel namespace answers `enum→string(64)`,
   `array→json`, and refuses `where`/`checks` explicitly
   (`mapping-unsupported`) — never silent coercion. Enum members are
   an inline bounded sorted list: the standalone choice, resolvable
   without Model resolution machinery.
3. **Computed columns refuse to render.** The 0.4.0 member carries no
   expression; emitting SQL would invent semantics. Explicit
   `render-unsupported`; a bounded expression reference to #66 is a
   future additive member, not designed here.
4. **Introspection rides `scan`'s typed evidence member** under the
   new `scan.storage-schema` capability — no target-protocol schema
   bump. Migration plans are core-produced and applied by adapters
   under `plan.storage-migration`/`generate.storage-ddl`; the plan's
   `planId` is the apply authority. Plan homes stay stdout-only in
   v1 — no authority-matrix bump.
5. **Honest versioned answers.** Supported majors 15–18 (14 leaves
   upstream support in November 2026). `isolation.snapshot` answers
   `partial` (REPEATABLE READ is snapshot isolation, but the #24
   owner relation keeps `snapshot` unordered on purpose);
   `lock.range` answers `partial` (no key-range locks below
   SERIALIZABLE); `temporal_constraint` is full on 18 only. A pin
   outside the matrix refuses, never clamps.
6. **Always-quote identifiers.** Emitted SQL quotes every identifier
   as `"name"`: the storage-name grammar never escapes itself, so
   quoting is unconditional and deterministic. Unnamed indexes derive
   `idx_<table>_<cols>[_uq]`; owned sequences derive
   `seq_<table>_<column>`; RLS policies derive `pol_<table>_tenant`;
   foreign keys derive `fk_<table>_<column>` — the emitted name is
   always present in the document.
7. **Registry code prefix `LEK-SEN`.** The `storage-engine.*` rules
   are a separate family from the #65 `storage.*` rules, so a new
   prefix is cleaner than continuing `LEK-STO-008+`. Fourteen rules,
   LEK-SEN-001..014; `extension-unsupported` is a warning-severity
   reporting rule; `migration-gated` and `conformance-failed` are
   denied-class rules.
8. **A dedicated conformance catalog.** `storage_engine::conformance`
   carries the fourteen component checks; #31's 18-check protocol
   battery stays frozen, and #117 reuses every check unchanged.
   Skips carry bounded reasons and are never passes.

## Consequences

- The Node/PHP/Go runtimes consume one canonical engine input
  document (`lekalo storage input`) — byte-identical goldens pinned
  by test (acceptance criterion 1).
- Planner guarantees are checked on PostgreSQL through the #24
  strict/permissive mapping over the embedded snapshot (criterion 2).
- Drift is detected against checked-mode evidence only; unchecked
  observation is unrepresentable (criterion 3).
- Semantic diffs become safe plans with visible risk and the exact
  digest custody gate (criterion 4).
- Unsupported extensions/types surface verbatim in evidence and
  findings (criterion 5).
- The test lifecycle is isolated and production access is
  const-forbidden (criterion 6).
- The adapter passes the conformance battery (criterion 7).
