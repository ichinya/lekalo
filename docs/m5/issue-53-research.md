# Issue #53 — validating the planner UI/API projection on the planning screen

## Scope and evidence

- Authority: `gh issue view 53 --repo ichinya/lekalo`, read first on
  2026-09-29; [issue #53](https://github.com/ichinya/lekalo/issues/53),
  dependencies #50/#60/#72. Milestone M5, pilot: greenfield.
- Inspected branch `ichinya/m5-issue-53`, baseline `fb066fc0` (the #50
  planning vertical plus the #59/#60 machinery); initial worktree clean;
  workspace version `0.4.0`.
- Evidence run on the baseline:
  `node scripts/test-php-laravel-routes.mjs` — 9/9 stages (with the
  provisioned vendor tree);
  `cargo test --locked -p lekalo-core --lib` — 913 passed, 0 failed.
- Paths below describe this checkout; **new** marks proposed work, not
  an existing API. Public fixtures and evidence name only `planner` —
  no consumer-application name appears anywhere.

## 1. What the pilot had to answer

The issue's research question: is
`Query + Command + Endpoint + Scenario` enough for a real client
screen, or does Lekalo need a future `screen/view` definition kind in
core? The acceptance bar: a Vue screen implementable from
generated/checked API types, browser E2E bound to scenarios and
requirements, every required screen state traceable to API symbols,
accessibility kept without a UI DSL, and the screen never becoming P0
core without evidence.

## 2. Existing machinery the pilot reuses (measured, not assumed)

| Need | Existing surface | Reuse verdict |
| --- | --- | --- |
| Typed client | `client_sdk::project` join (transport + IR + #62 registry + #64 query-model), node-typescript adapter `generate.client-sdk` | The projection carried every planner operation after growing the model; the TS/Go clients render deterministically from one evidence document. |
| Wire contract | `contracts/transport-http.schema.v0.4.0.json` (closed endpoint bindings; method/path single-sourced in the Model) | Idempotency keys, correlation headers, auth schemes, and the full error table arrive per operation with exact #62 identities. |
| Optimistic concurrency | `planner.reorder_version` scalar + `reorder_stale` (LEK-ERR-009, `conditional(idempotency-key)`) | The compare-and-swap is machine-readable end to end; the client union exposes the retry authorization. |
| Screen states | Declared queries, declared errors, closed categories | Every screen state maps to a declared symbol; nothing needed inventing. |
| Requirements/scenarios | `requirements.attachment.json` + openspec provider tree; Scenario IR (`dev.lekalo.scenario-ir@0.2.16`) with typed `native` backend bindings | Scenarios are Model symbols (`kind: scenario`), the transport joins them per endpoint, and the trace walks requirement→symbol. |
| Screen fixture | `tests/fixtures/client-sdk/vue-consumer/` harness (pinned TS + `@vue/compiler-sfc`) | The same provisioning pattern typeschecks the screen SFC strict. |

## 3. Gaps the pilot exposed (all closed in this slice)

1. **No completed family.** The #50 model had today/backlog/carry-over
   and pause but no complete command, completed query, or completion
   stamp — the issue's completed bucket and complete command had no
   projection to validate. Closed in the model: `planner.complete_planning`,
   `planner.completed_planning`, `planner.task_completed`,
   `planner.user_task_planning.completed_at`, two endpoint bindings, the
   `deny_foreign_planning` gate, and the two #62 registry bindings
   (additive, canonical, 13 total).
2. **No query-model home for the routes project.** The #72 SDK evidence
   derivation skips honestly without `lekalo/query-model.yaml`. Closed:
   the six query plans (filters, sorts with identity tie-breakers,
   consistency) committed in the model home.
3. **Scalar projection lied for numbers.** The #72 TypeScript/Go
   renderers spelled every named scalar `string`/`string`, typing
   `position` and `reorder_version` — the optimistic-concurrency field —
   as strings. Closed in the renderer: the declared scalar mapping
   projects (`number`→`number`/`float64`, `boolean`→`boolean`/`bool`;
   dates stay ISO strings). Golden-neutral for the #72 fixture (no
   number scalars there); the adapter bundle and manifest are rebuilt
   and re-pinned.
4. **Go render was not gofmt-stable.** Struct tags aligned by hand in
   the #72 fixture passed `gofmt -l` only because that fixture's field
   widths never forced realignment. Closed by making the renderer align
   the name/type columns gofmt-style and committing the gofmt-normalized
   bytes as the fixture form (gofmt is deterministic, so the checked
   join still detects every wire change).
5. **No UI-side projection or E2E bindings.** Closed without core IR:
   `ui-projection.json` (machine-readable DTOs, capability-gated
   actions, states, timezone rules, accessibility expectations) checked
   against the same join by `scripts/test-php-laravel-ui.mjs`; two
   canonical browser-E2E scenario documents with `native` bindings to
   `web.runners/browser-e2e` under the model home's `lekalo/scenarios/`.

## 4. The research question, answered from the pilot

**Query + Command + Endpoint + Scenario suffices; a screen/view
definition kind is not justified today.** The evidence:

- The screen needed exactly four things — buckets (queries), mutations
  (commands), wire truth (endpoints), and behavior bindings
  (scenarios) — and every one already had a closed home. The only
  screen-shaped facts that surfaced (DTO field selection, action
  availability, state mapping, a11y expectations) are *projections* of
  those four, checked against them, and add no new semantic decisions.
- The one genuinely new predicate — action availability — derives
  completely from existing contracts: the policy `applies_to` list, the
  registry binding's error table, and DTO field nullability. A
  `screen/view` IR kind would have hardcoded what derivation already
  produces machine-readably, violating the issue's own constraint.
- The cost side is honest: the checked join (`test-php-laravel-ui.mjs`)
  is fixture-local glue (~400 lines, no core), and if a second screen
  needs the same glue the reusable part is a *checker*, not an IR kind.
- Documented gaps a future `screen` kind would have to earn: none of
  the acceptance criteria failed for lack of one. Revisit only if (a) a
  second pilot needs cross-screen composition semantics, or (b) impact
  analysis must answer "which screens break" without a consumer
  declaration — the `ClientArtifactIndex` consumer binding already
  covers the honest version of (b).

## 5. Decisions

1. The completed family enters the model as ordinary
   commands/queries/endpoints/events under the existing deny gate; no
   screen semantics leak into core.
2. The UI projection stays a checked fixture artifact
   (`lekalo/ui-projection/v0.1.0`, closed vocabulary, validated by the
   gate against IR + transport + client contract + requirements), never
   a core contract without a second pilot's evidence.
3. Browser E2E binds through the existing Scenario IR `native` backend;
   the runner (`web.runners/browser-e2e`) and the maintained Vue screen
   are the pilot surface, kept minimal (two bound scenarios).
4. Accessibility expectations ride OpenSpec requirements bound to the
   screen scenarios; no UI DSL is introduced.

## 6. Verification

```sh
node scripts/test-php-laravel-ui.mjs      # new gate: 6 stages
node scripts/test-php-laravel-routes.mjs  # 9/9 (incl. the S10 completed battery)
node scripts/test-php-laravel-operations.mjs
cargo test --locked -p lekalo-core --lib  # registry + client_sdk + scenario
cargo run -p lekalo-core --example regen-planner-routes -- .
cargo run -p lekalo-core --example canonicalize-screen-scenarios -- .
NODE_PATH=… node scripts/test-client-sdk-runtime.mjs
NODE_PATH=… node scripts/test-client-sdk-contracts.mjs
node scripts/test-node-client-sdk.mjs
node adapters/node-typescript/build.mjs --check
node scripts/regen-adapter-manifest.mjs
```
