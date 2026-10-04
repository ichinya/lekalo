# Greenfield planner: Laravel backend and Vue frontend

Status: **Implemented** synthetic contracted planner slice; browser execution and automatic screen generation are **Planned**, not demonstrated. Owner: planner fixture maintainers. [#114](https://github.com/ichinya/lekalo/issues/114), [Laravel/Vue ADR](adr/0047-laravel-vue-pilot-path.md), [no screen/view IR ADR](adr/0046-no-screen-view-ir-kind.md).

The `greenfield-consumer` is fictional. Reuse the committed target-neutral planner Model and contracts, a generated/checked API client and a maintained Vue screen. No real application clone or private data is needed. Start with [minimal bootstrap](project-layout.md#minimal-project), then use the documented synthetic corpus for the complete planner slice; the empty initial Model is not silently presented as that larger fixture.

## Contract the planner (C)

Working root: [tests/fixtures/docs/contracted-module](../tests/fixtures/docs/contracted-module). Copy this synthetic tutorial variant into a disposable directory named `contract-planner` and enter that copy's root, which contains `lekalo/project.yaml`, `declarations/initial.json` and `test/native.test.mjs`. With the built `lekalo` on PATH and Node 24, execute every command in [contract one module](adoption.md#contract-one-module) from that root: update the declaration, run the two maintained-code tests, check the missing coverage, attach their IDs, then check clean conformance. The first check deliberately exits **1/stderr** with two `contracted.coverage-missing` findings; running tests alone does not attach evidence. The final check exits **0/stdout**.

The variant omits the original corpus's deliberately unimplemented count query and absent generated support artifact. Its conformance example proves declared shape/effects, exact fingerprints and attached coverage presence; it does not establish Laravel production behavior. Laravel/Eloquent/Vue concepts stay in target configuration/native artifacts.

## Inspect the planner (D)

Working root: [tests/fixtures/contracted/planner-slice](../tests/fixtures/contracted/planner-slice). Switch to a **second fresh disposable copy**, named `inspect-planner`, of this original projection corpus and enter its root containing `lekalo/project.yaml`. Execute the three commands in [projection example](architecture.md#projection-example) there. Keep the step C copy separate; do not overlay fixtures or transfer its contracted registry.

The original contains the unimplemented `planner.count_focused` query, an absent declared support artifact and no `test/native.test.mjs`. Its projections deliberately expose partial evidence; it is not a clean conformance fixture. All three displayed projections exit **0/stdout**. The documentation gate checks these two linked working-root selections against the replay registry and executes C before D in their separate copies.

## Generate, check and verify (E)

**Implemented** bounded CLI check path. In a disposable copy of `tests/fixtures/orchestration/project`, before adding a target or generated artifact:

```sh docs-example=generate-check
lekalo --json lock
lekalo --no-cache --json generate --check
lekalo --no-cache --json verify
```

Expected: exit 0/stdout. Lock records zero adapters; check returns `verdict: clean` with zero artifacts. Verify emits `lekalo/orchestration/v0.2.16`, model validation pass and explicit unsupported native/scenario execution components. This checks an empty artifact inventory and portable contracts; it does not generate files or execute a target. The next command exercises actual supported adapter generation and verification.

**Implemented** contributor reproduction. From the repository root after building the CLI, this orchestrator uses disposable fixture copies and the shared scenario corpus:

```sh docs-example=planner-chain
node scripts/test-golden-planner-e2e.mjs
```

Expected: exit 0 with passing load/IR/validation, projections, semantic-diff mutation/control, actual adapter dry-run/apply/verify scenario lane and canonical trace checks. The later scenario corpus and trace are committed shared inputs, not invented outputs of the earlier stages. Generation/check alone does not prove maintained source or a database/UI was created. Missing binary or stage is a failure.

## Laravel and Vue target evidence

Prerequisites: PHP >=8.3 with the CI-listed extensions, Composer, the committed `tests/fixtures/php-laravel/planner/composer.lock`, Node 24, externally provisioned Ajv `8.17.1`, TypeScript `5.9.3` and `@vue/compiler-sfc@3.4.38` via `NODE_PATH`. The lock pins Laravel `13.33.0`, Testo `0.10.53` and Laratesto `v0.7.3`. Provisioning occurs before verification, never inside an adapter. Use the explicit setup steps in [CI](../.github/workflows/ci.yml); do not update the lock during an example run.

**Implemented** after those prerequisites, from the repository root:

```sh docs-example=laravel-vue-native
node scripts/test-pilot-laravel-vue.mjs
```

Expected: exit 0 and all nine baseline legs pass: Model neutrality, observed baseline, operations, routes, checked client/UI, fake Mago, native-gate planning, actual scenario execution and Node/Laravel parity. A provisioned real Mago adds its real leg; CI's real-Mago job owns that proof. Missing vendor/toolchain is failure, never a skipped successful tutorial.

The maintained screen is `tests/fixtures/php-laravel/routes/ui/src/PlannerBoard.vue`. UI evidence checks client/OpenAPI agreement, SFC compilation/typechecking and scenario/E2E-binding schemas. It does **not** execute a browser or generate the screen from Model. [Client SDK](client-sdk.md), [PHP operations](php-laravel-operations.md), [target protocol](target-protocol.md), [security](security.md).

To replay exactly these displayed commands and assertions, run the documentation gate's `--lane planner` after setup. The gate requires this lane in CI; a portable-only run is not native tutorial acceptance. See [roadmap](roadmap.md) for release/version pins.
