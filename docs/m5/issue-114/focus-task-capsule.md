# `planner.focus_task` context capsule (issue #114)

One self-contained bundle for an agent working on the focus operation:
everything below is either embedded or digest-pinned. An agent should
never need to open the consumer repository — the pilot fixtures carry
no consumer-application naming, only the neutral `planner` module —
and never needs to re-derive the facts from more than this document
plus the digest-pinned evidence files.

Sources of truth, in authority order:

1. the semantic Model (language-independent, no framework concepts —
   audited by `scripts/test-planner-model-neutrality.mjs`, evidence
   `tests/fixtures/php-laravel/routes/evidence/model-neutrality.audit.json`);
2. the transport attachment (`tests/fixtures/php-laravel/routes/inputs/transport.json`)
   and the #62 error registry — wire truth, single-sourced from the
   same validated join as the Model;
3. the generated client contract (`tests/fixtures/php-laravel/routes/ui/generated/planner.client.ts`,
   byte-verified by `scripts/test-php-laravel-ui.mjs`).

## 1. The machine capsule (verbatim)

`lekalo context planner.focus_task` over the operations-fixture IR
(`dev.lekalo.context@0.2.16`, complete, 248 estimated characters,
12/12 candidates included, evidence:
`tests/fixtures/php-laravel/routes/evidence/focus-task.context.json`):

```json
{"status":"valid","context":{"budget":{"estimated":248,"fits":true,"limit":4000,"minimumRequired":248},"complete":true,"coverage":{"candidates":12,"excluded":0,"included":12},"estimator":{"digest":"sha256:602e648c2ff7c58cace92876f6c834c5c1735ce594e3ba565d7b012f752fb0be","identity":"dev.lekalo.estimator.chars-4@0.2.16","version":"0.2.16"},"gaps":[{"gap":"detected-effects-absent"},{"gap":"error-contracts-unrepresentable"},{"gap":"no-scenario-coverage"}],"identity":"dev.lekalo.context@0.2.16","irDigest":"sha256:c32eda1c4ab769dc636e29ce1cdfe0d032b691eb4f4e3e0d90b2e7708f7a41dd","manifest":{"excluded":[],"included":[{"id":"operation:planner.focus_task","section":"symbol","tokens":35},{"id":"policy:planner.deny_bulk_focus","section":"policies","tokens":21},{"id":"effect:emit-event:operation:planner.focus_task->event:planner.task_focused#0","section":"effects","tokens":20},{"id":"effect:update:operation:planner.focus_task->canonical:planner.task#0","section":"effects","tokens":18},{"id":"edge:accepts:operation:planner.focus_task->type:planner.task_id#0","section":"dependencies","tokens":18},{"id":"edge:derived_from:operation:planner.focus_task->requirement:PLANNER-REQ-001#0","section":"dependencies","tokens":21},{"id":"edge:references:operation:planner.focus_task->effect:planner.update_task#0","section":"dependencies","tokens":20},{"id":"edge:exposes:endpoint:planner.endpoint_focus_task_by_id->operation:planner.focus_task#0","section":"public-impact","tokens":23},{"id":"edge:exposes:endpoint:planner.endpoint_tasks_focus->operation:planner.focus_task#0","section":"public-impact","tokens":22},{"id":"type:planner.task_id","section":"types","tokens":14},{"id":"edge:emits:effect:planner.update_task->event:planner.task_focused#0","section":"closure","tokens":18},{"id":"edge:references:effect:planner.update_task->entity:planner.task#0","section":"closure","tokens":18}]},"mode":"symbol","modelVersion":"0.2.16","project":"planner","roots":["operation:planner.focus_task"],"schemaVersion":"lekalo/context/v0.2.16","sections":{"closure":[{"confidence":"canonical","from":"effect:planner.update_task","occurrence":0,"relation":"emits","to":"event:planner.task_focused"},{"confidence":"canonical","from":"effect:planner.update_task","occurrence":0,"relation":"references","to":"entity:planner.task"}],"dependencies":[{"confidence":"canonical","from":"operation:planner.focus_task","occurrence":0,"relation":"accepts","to":"type:planner.task_id"},{"confidence":"canonical","from":"operation:planner.focus_task","occurrence":0,"relation":"derived_from","to":"requirement:PLANNER-REQ-001"},{"confidence":"canonical","from":"operation:planner.focus_task","occurrence":0,"relation":"references","to":"effect:planner.update_task"}],"effects":[{"action":null,"confidence":"canonical","field":null,"kind":"emit-event","occurrence":0,"operation":"operation:planner.focus_task","resource":{"id":"planner.task_focused","kind":"event"}},{"action":null,"confidence":"canonical","field":null,"kind":"update","occurrence":0,"operation":"operation:planner.focus_task","resource":{"id":"planner.task","kind":"canonical"}}],"policies":[{"appliesTo":["planner.focus_task"],"decision":"deny","description":"Bulk focus is out of scope","id":"policy:planner.deny_bulk_focus"}],"public-impact":[{"confidence":"canonical","from":"endpoint:planner.endpoint_focus_task_by_id","occurrence":0,"relation":"exposes","to":"operation:planner.focus_task"},{"confidence":"canonical","from":"endpoint:planner.endpoint_tasks_focus","occurrence":0,"relation":"exposes","to":"operation:planner.focus_task"}],"symbol":[{"contract":{"effects":["planner.update_task"],"input":[{"name":"task_id","required":true,"type":{"ref":"planner.task_id"}}]},"derivedFrom":["PLANNER-REQ-001"],"description":"Focus one task","id":"operation:planner.focus_task","kind":"operation","module":"planner","subkind":"command","version":1}],"types":[{"contract":{"base":"uuid"},"description":"Stable task identifier","id":"type:planner.task_id","kind":"type","module":null,"version":1}]}}}
```

The capsule's own declared gaps are honest and bounded: the operations
IR it was rendered from carries no detected-runtime effects, no #62
error contracts, and no scenario coverage. The pilot join below closes
exactly those three gaps for the Laravel target.

## 2. The command contract (semantic, target-free)

```text
command  planner.focus_task  "Focus one task"
input    task_id : planner.task_id (uuid, required)
effects  planner.update_task (update planner.task, emits planner.task_focused)
policy   planner.deny_bulk_focus (deny; "Bulk focus is out of scope")
derived  PLANNER-REQ-001
```

The command is declared once; the Laravel pilot binds it, the Node
baseline keeps observing it, and neither target's vocabulary appears
in the definition (see the neutrality audit above).

## 3. Wire truth (the two focus endpoints of the pilot join)

Both endpoints are one projection of the same join
(`tests/fixtures/php-laravel/routes/inputs/transport.json`,
digest-bound to the committed IR `tests/fixtures/php-laravel/routes/inputs/ir/planner.ir.json`):

| | `planner.endpoint_focus_task_by_id` | `planner.endpoint_tasks_focus` |
| --- | --- | --- |
| Method/path | `POST /tasks/{task_id}/focus` | `POST /api/tasks/{task_id}/focus` |
| Success | `202` | `202` |
| Auth | `user_bearer` scheme, actor `identity.user`, policy `planner.deny_bulk_focus` | same |
| Body | explicit `task_id` (from the path) | whole-input JSON |
| Idempotency | `Idempotency-Key`, required | `Idempotency-Key`, required |
| Correlation | `X-Correlation-Id`, `X-Request-Id` | same |
| Scenario | `planner.screen_focus_flow` | — |

Declared error table (identical for both; identities from the #62
registry, statuses from the declared error defaults):

| Error identity | Registry id | Status | Category |
| --- | --- | --- | --- |
| `planner.focus_conflict` | LEK-ERR-001 | 409 | conflict |
| `planner.focus_denied` | LEK-ERR-002 | 403 | auth |
| `planner.input_invalid` | LEK-ERR-003 | 400 | validation |
| `planner.store_unavailable` | LEK-ERR-004 | 503 | infrastructure |
| `planner.task_not_found` | LEK-ERR-005 | 404 | not-found |

## 4. The generated client surface (the screen's only API vocabulary)

From the byte-verified `planner.client.ts` (a fresh adapter render of
the committed SDK evidence equals the committed bytes — checked by
`scripts/test-php-laravel-ui.mjs`, stage 1):

```ts
plannerEndpointFocusTaskById(taskId: string,
  input: { "task_id": taskId },
  idempotencyKey: string,
  xCorrelationId?: string, xRequestId?: string,
): Promise<LekaloResult<void>>

plannerEndpointTasksFocus(taskId: string,
  input: focusTaskInput,
  idempotencyKey: string,
  xCorrelationId?: string, xRequestId?: string,
): Promise<LekaloResult<void>>
```

Every call carries the required `Idempotency-Key`; the result union is
the declared error table above plus success. The client never retries
by itself; retries follow the declared error contracts.

## 5. Behavior evidence (portable scenarios covering the command)

`planner.focus_task` is the invoked operation of the whole committed
corpus (`tests/fixtures/orchestration/project/lekalo/scenarios/`),
executed on both backends under their real runners:

| Scenario | Leg | Both backends |
| --- | --- | --- |
| `planner.scenario.focus_happy` | seed → focus → result/state/emitted | equal, pass |
| `planner.scenario.focus_error` | unknown id answers `planner.error.task_missing` | equal, pass |
| `planner.scenario.focus_idempotent` | same key replays without a duplicate | equal, pass |
| `planner.scenario.focus_denied` | `planner.policies/deny-bulk-focus` denies the bulk actor, allows a solo actor | equal, pass |
| `planner.scenario.focus_rollback` | a failed focus writes no row, no effect-ledger entry, no state change | equal, pass |
| `planner.scenario.focus_concurrent` | the race case is declared unsupported (no race evaluator in the closed assertion vocabulary) — recorded, never a silent pass | equal, unsupported |

Durable comparison: `tests/fixtures/pilot/equivalence/node-laravel.scenario-comparison.json`.
The scenario-vocabulary policy spelling `planner.policies/deny-bulk-focus`
maps to the IR symbol `planner.deny_bulk_focus` in the maintained
fixture ports (the closed Scenario IR namespacedId grammar carries
slash-form ids only).

## 6. Maintenance rules for this capsule

- Any wire change re-runs the validated join; this document's sections
  3–5 must be re-checked against the re-pinned evidence (the gates:
  `scripts/test-php-laravel-routes.mjs`, `scripts/test-php-laravel-ui.mjs`,
  `scripts/test-php-laravel-parity.mjs`).
- The machine capsule is derived (`lekalo context planner.focus_task`);
  re-render it after Model changes and update section 1 verbatim.
- Everything here names `planner` and the two targets — no consumer
  application is named anywhere in the capsule or its evidence.
