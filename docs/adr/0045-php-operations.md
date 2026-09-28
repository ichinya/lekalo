# ADR 0045: explicit PHP Laravel operations (commands, queries, policies)

Issue #59. The research plan is `docs/m5/issue-59-research.md`; this ADR
records the decisions that bind the implementation.

## Decision

1. **New closed contract family at v0.4.0** (the product version of the
   landing commit, per `docs/versioning.md`):
   `php-operations-input` (the authored document under
   `lekalo/operations/<project>.operations.json`),
   `php-operations-map` (the derived custody sidecar),
   `php-operations-evidence` (the observed-handler document the checked
   join consumes, under `.lekalo/import/observed/operations-evidence.json`).

2. **Operations consume compiled IR through the core evidence path**
   (`.lekalo/cache/ir/<project>.json`) exactly like the #58 types family.
   The input pins the exact `irDigest` plus the digest of the types input
   it reuses (`lekalo/types/<project>.types.json`); the types family is a
   required dependency, never re-derived or re-emitted by operations.

3. **Core-side join authority.** `crates/lekalo-core/src/php_operations/`
   validates the authored join — compiled IR definitions, the embedded
   #62 error-registry binding (declared errors are the binding set, exact
   equality), the referenced types input, and the closed recipe grammar —
   before any adapter exchange. The adapter re-validates its input
   defensively, but the core join is the acceptance authority. A missing
   context is a finding; absence is never "no policy", "pure query" or
   "no transaction".

4. **Closed recipe vocabulary (managed mode), v0.4.0:**
   `port-delegation` (one typed port call carries the business body; the
   body is maintained and labeled custom in the map) and
   `single-entity-update` (read one entity by key through a generated
   narrow repository interface, check declared preconditions with typed
   equality, rebuild the immutable entity DTO with every constructor
   argument spelled explicitly — assignments and kept fields — save, and
   dispatch declared events through a generated per-operation event port).
   Operands are closed: `{fromInput}`, `{fromEntity}`, `{enumCase}`,
   `{literal}`. No PHP, SQL, eval, callbacks, loops, clock or ID operands
   in v0.4.0: unsupported inputs refuse with a bounded reason before any
   write. Queries accept only `port-delegation`; any query that could
   write refuses (`operations.query-write`).

5. **Modes and custody.** Per-operation modes: `managed` (generated,
   regenerable, under `.lekalo/generated/php-laravel/operations/`),
   `scaffold-once` (emitted once under the closed consumer root
   `app/lekalo-operations`, then user-owned; the sidecar
   `operations.map.json` is the marker), `checked` (no writes; the
   declared FQN/method/path and constructor shape join against the
   observed evidence document), and `custom` (maintained body, same shape
   join, no business-body rewriting). `app/lekalo-operations` classifies
   `scaffolded` in the core lifecycle classifier (closed constant shared
   with the adapter, like `app/lekalo-types`).

6. **No hidden logic.** Portable handlers take constructor-injected typed
   dependencies, expose exactly one public business entrypoint `handle`,
   and never import portable-core facade aliases or service-locator calls.
   Errors are the declared #62 binding set as typed domain classes
   (infrastructure-category errors stay infrastructure). Policy is an
   injected per-operation port; the transaction is a declared
   required/forbidden binding through a generated `TransactionPort`
   (commands only). The emitted classes are syntax-checked and loaded
   under `php -n`; the Laravel boundary (connection, queue, auth
   principal binding) stays maintained application code.

7. **Capability registration.** `generate.operations` and
   `verify.operations` join the `dev.lekalo.target-capabilities@0.4.0`
   generation (same reviewed in-generation extension the registry records
   for `generate.types`). The adapter declares both as `partial`: the
   recipe vocabulary is a bounded subset, and verify covers shape/binding/
   declared-effect reconciliation but not scenario execution.

## Consequences

- Deterministic emission: identical inputs and pins produce byte-identical
  files across roots; any relevant attachment change (IR, types input,
  operations input) changes the map digests and invalidates custody.
- The observed-evidence checked join is an identity/shape join over a
  validated bounded document; trusted producer execution (pinned Mago)
  is a separate lane and its absence is a finding, never a pass.
- Runtime behavior (PostgreSQL transactions, queue visibility, actor
  leakage) requires the provisioned Laravel lane and is not claimed by
  this contract family.
