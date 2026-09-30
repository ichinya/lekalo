# Issue #114 — Node ↔ Laravel contract/equivalence report

The written half of the removal gate. The machine half is
[`tests/fixtures/pilot/equivalence/node-laravel.scenario-comparison.json`](../../tests/fixtures/pilot/equivalence/node-laravel.scenario-comparison.json)
(emitted deterministically by `node scripts/test-php-laravel-parity.mjs --emit-evidence <path>`),
pinned by the observed-baseline manifest
(`tests/fixtures/pilot/observed-baseline/node-planner.baseline.json`)
and re-verified by `scripts/test-observed-baseline.mjs`.

## 1. What is compared, and what is not

The comparison is **neutral by construction**. Both backends execute
the *same committed scenario corpus* (six Scenario-IR documents under
`tests/fixtures/orchestration/project/lekalo/scenarios/`, digest-pinned
per file) through their committed single-file adapter artifacts. Each
generated test records one durable run record per assertion; the
records normalize to semantic row tuples:

```text
(step_id, observes, kind, outcome)
```

Runner identity, test paths, fingerprints, profiles, and durations are
excluded from the comparison by the tuple's shape. Nothing about
Node's JavaScript or Laravel's PHP reaches the compared surface — the
compared unit is the declared behavior, not the code that happens to
run it.

Not compared, deliberately: performance, storage internals, process
topology (API/queue/scheduler roles), and any wire below the port
contract. Those are implementation properties; the model owns the
contract, and the equivalence gate consumes only contract outcomes.

## 2. The corpus, per outcome

Evidence revision: corpus digests as pinned in the comparison artifact;
IR evidence `sha256:e1a2173a…` (`tests/fixtures/adapter-conformance/inputs/ir-minimal.json`).

| Scenario | Leg proved | node:test rows | laratesto rows | Equal |
| --- | --- | --- | --- | --- |
| `planner.scenario.focus_happy` | seed → invoke → result + entity state + emitted event | 3 × pass | identical | yes |
| `planner.scenario.focus_error` | the typed missing-task error, exact public payload | 1 × pass | identical | yes |
| `planner.scenario.focus_idempotent` | replay under one durable key, no duplicate emission | 1 × pass | identical | yes |
| `planner.scenario.focus_denied` | the declared deny policy refuses the bulk actor and still allows a solo actor (authorization) | 3 × pass | identical | yes |
| `planner.scenario.focus_rollback` | a failed command writes nothing: no row, no effect-ledger entry, no state change (transaction/atomicity) | 4 × pass | identical | yes |
| `planner.scenario.focus_concurrent` | concurrent focus — **declared unsupported on both backends**: the closed assertion vocabulary has no race evaluator, and the declared-unsupported row is recorded, never a silent pass | unsupported-only | identical | yes |

The shared negative control: one lying-port mutation (the emissions
capture answers empty) drives **both** suites into a recorded `fail`
row at the same semantic step (`emitted`) — a backend can never dress
a behavior change up as a runner quirk.

## 3. Which legs are target-specific, precisely

The corpus is neutral; the *enforcement proofs* differ by target and
are documented here so the comparison is not over-claimed:

- **Authorization (neutral leg).** The scenario proves the *declared
  decision*: `authorize(bulk actor, deny-bulk-focus) = denied` and
  `authorize(solo actor, …) = allowed` on both backends. The Laravel
  *HTTP enforcement* (a foreign actor sees the declared not-found, the
  policy adapter denies at the routes boundary) is pinned separately by
  the Laravel planning battery inside `scripts/test-php-laravel-routes.mjs`
  (stage: the planning battery) — it is a target leg, not part of the
  neutral comparison. The generated delegation handlers do not call the
  policy port inside the HTTP path; that honest gap is pinned by the
  battery, unchanged from #50/#53.
- **Transaction (neutral leg).** The scenario proves atomicity
  outcome-level: zero writes after a failed command. The *mechanism*
  differs (Node: in-memory store with success-only ledger writes;
  Laravel: the real database through the generated
  `TransactionPort`-wrapped handler) — the compared fact is identical
  because the ports' closed contract records the same ledger semantics.
- **Idempotency (neutral leg).** Proven through the closed port
  contract (`ctx.idempotencyKey` deduplicates, replays never re-emit).
  The Laravel routes layer additionally requires the `Idempotency-Key`
  header per the transport attachment — again pinned by the routes
  battery, a target leg.
- **Concurrent focus (declared unsupported).** No backend claims this
  leg today. It stays in the corpus as an explicit unsupported row so
  the day a race evaluator lands, both backends must answer it or
  diverge visibly.
- **Screen projection.** The Vue screen typechecks against the
  generated client and its optimistic rollback contracts are checked
  (`scripts/test-php-laravel-ui.mjs`); the Node baseline has no screen.
  Screen behavior is out of the equivalence scope by definition — the
  client contract is derived from the same join both targets answer to.

## 4. The gate (what any old-backend removal decision must show)

The Node implementation is the **observed baseline**
(`tests/fixtures/pilot/observed-baseline/`, digest-pinned) and is not
removed, shrunk, or demoted on prose. A removal decision requires all
of the following, in one reviewable state:

1. **Corpus equality.** The committed comparison artifact is fresh
   (`scripts/test-observed-baseline.mjs` green) and every scenario row
   reads `equal: true` — including the declared-unsupported rows.
2. **Negative control.** The shared lying-port mutation records the
   same fail row on both backends in the same run that produced the
   artifact (the parity gate re-proves this every run).
3. **Baseline currency.** The pinned event envelope, integration
   protocol, OpenAPI documents, and behavior rows still match the live
   bytes; a moved baseline is re-pinned consciously with a note, never
   silently.
4. **Surface accounting.** A written inventory (this report's successor)
   of what the Node backend uniquely carries — scanner, kernel,
   transport, client-SDK, zod, scenario-test, and native-gate
   *fixtures* stay target fixtures regardless; the removal claim must
   name exact adapter/application surfaces, not families.
5. **No consumer coupling.** The removed surface carries no consumer
   application naming (the privacy/fixture-provenance gates keep
   enforcing this mechanically).

If any leg is missing, the decision is "no", and the missing leg is the
next work item. Nothing in this report authorizes a big-bang removal.

## 5. Honest gaps

- The concurrent-focus leg is declared unsupported on both backends;
  equivalence over it is vacuous until a race evaluator exists.
- The neutral authorization leg proves decisions, not HTTP
  enforcement; the enforcement evidence is target-side (the Laravel
  battery) and the Node side has no equivalent HTTP battery — one more
  reason the Node baseline stays.
- The equivalence corpus covers the focus command family. The planning
  family (plan/move/unplan/reorder/pause/complete + queries) is proven
  on the Laravel target by the routes battery, but has no portable
  corpus documents yet; growing the corpus is mechanical (the six
  documents here are the template) and is the natural follow-up slice.
