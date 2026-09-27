# Issue #56 — Laratesto conformance backend: research и implementation plan

## Scope и доказательная база

- Authority: `gh issue view 56 --repo ichinya/lekalo`, прочитан первым; [issue #56](https://github.com/ichinya/lekalo/issues/56), dependencies #23/#27/#31/#54.
- Срез 2026-09-27: branch `ichinya/m5-issue-56`, исходный HEAD `aca1bb8e6b3915b05213be6eba120c046cdcf29e`, чистый worktree.
- Только research: PHP backend, Composer dependencies и Laravel runtime здесь не устанавливались/не запускались; ни один AC не объявляется выполненным.
- #54 владеет базовым PHP adapter/protocol/scanner/generation; #56 добавляет Scenario IR compiler, test ports, Laratesto execution evidence и parity. Не дублировать PHP parser или Mago integration (#55).
- Имена новых файлов/символов ниже — предлагаемый layout, согласовать с реализацией #54; существующие paths/symbols проверены в этом checkout.

## 1. Existing modules, contracts, tests

В таблице `core/` = `crates/lekalo-core/src/`, `node/` = `adapters/node-typescript/src/`.

| Seam | Файлы / символы / ограничения |
|---|---|
| Portable scenario | `core/scenario/`, `contracts/scenario-ir.schema.v0.2.16.json`, `docs/scenario-ir.md`, `crates/lekalo-core/tests/scenario.rs`, `scripts/test-scenario-contracts.mjs`. Identity `dev.lekalo.scenario-ir@0.2.16`, exact IR/model digests; given/action/assertion IDs и порядок уже определены. |
| Scenario compiler reference | `node/scenario-map.mjs::mapScenario`, `scenario-gen.mjs::{scenarioOperation,joinCheckedBindings}`, `scenario-emit.mjs::emitScenarioTests`. Generate/verify, deterministic bytes, unsupported rows, checked-binding missing/ambiguous/mismatch; checked tests не генерируются. |
| Test port | `contracts/test-port.schema.v0.4.0.json`, `tests/fixtures/orchestration/project/lekalo/test-port.json`. Closed exports: invoke, state, fixtures, actor, clock, ids, emissions, effects, authorize, contractCheck, fixtureDigest, reset. **Path ограничен `.mjs/.ts`**, PHP требует изменения контракта, не только emitter. |
| Runtime evidence | `core/scenario_evidence.rs::{RunRecord::from_value,RunSummary,trace_relations,trace_manifest_document,TraceContext}`; `contracts/scenario-run.schema.v0.4.0.json`; `.lekalo/import/scenario-runs/`. Outcomes pass/fail/unsupported/infrastructure/degraded, modes generated/scaffolded/checked; один runner id/version, нет отдельных Laravel/Testo/Laratesto versions или gate field. |
| Verify ingestion | `core/orchestration/verify.rs::{scenarios_execution_component,scenarios_execution_rollup,scenario_trace_context}`. Проверяет форму и IR digest, stale → degraded; fail и infrastructure блокируют, но причина обоих сейчас `scenario.assertion-failed`. Context не устанавливает gate, receipt публикует trace summary. |
| Trace | `core/trace/`, `contracts/trace-manifest.schema.v0.2.16.json`; `TraceContext.gate` позволяет native_test → scenario/operation и gate → test/scenario. Требуется настоящий gate context и экспорт manifest, а не только счётчик relations. |
| Ownership / source maps | `core/artifacts/`, `core/orchestration/generate.rs::{artifact_kind_for,module_path_of}`, `contracts/artifact-manifest.schema.v0.2.16.json`; ownership `.lekalo/generated/manifests/ownership.json`. `module_path_of` всегда добавляет `.ts`, test classification знает TS naming: PHP maps сейчас ошибочно свяжутся с TS. |
| Native execution | `core/native_gate/{mod,wire,policy,receipt,fixture_tests}.rs`, `docs/native-gates.md`. `wire.rs::MANAGERS` допускает только Node managers; `production_run` не запускает команды, реальный runner — test-only. Composer нельзя выдать за npm. |
| Target contract | `core/target_protocol/`, `core/adapter_conformance/`, `crates/lekalo-core/tests/adapter_conformance.rs`: `lekalo.target/v1`, protocol 0.3.2, девять operations включая plan-native. `TargetClient` confinement не переносит весь vendor tree: нужны #54 bundle/toolchain custody. |
| Profiles | `core/target_profile/component.rs` уже содержит testing=laratesto, runtime=php-laravel; testing.parallel=partial — catalogue declaration, не разрешение fiber concurrency Laravel suite. |
| Executable Planner inputs | `tests/fixtures/orchestration/project/lekalo/scenarios/planner.scenario.focus_{happy,error,idempotent,concurrent}.json`, `src/testing/{port.mjs,port.selftest.mjs}`; `tests/fixtures/adapter-conformance/inputs/ir-minimal.json`. Последний — compiled IR, который реально использует Node gate. |
| Baseline tests | `adapters/node-typescript/test/scenario-{map,emit,extension,bindings}.test.mjs`, `scripts/test-node-scenario-{units,tests}.mjs`; E2E materializes bundle, dry-run/apply, запускает tests, проверяет records, повторяемость и verify drift. Concurrent case unsupported, не успешный concurrency proof. |
| PHP starting point | `docs/m5/issue-54-research.md` — dependency plan; `adapters/php-laravel/` отсутствует. `tests/fixtures/adopt/laravel-app/` и contracted Planner coverage YAML не являются runnable Laratesto fixture. |

### Проверенный внешний контракт

- Context7 не нашёл Laratesto; Laravel 13 docs получены через `/websites/laravel_13_x`: [DB isolation](https://laravel.com/docs/13.x/database-testing), [HTTP/auth](https://laravel.com/docs/13.x/http-tests). Laravel PHPUnit traits не переносить механически в Laratesto attributes.
- Laratesto snapshot [2c7396e README](https://github.com/ichinya/laratesto/blob/2c7396efc2dc8d7af3dd2a83e89b5c0a75f73444/README.md): fresh app per test, HTTP/DB/auth/session/Artisan helpers, sequential-only Laravel suite, DB attributes, `laratesto:test --testo-only` и `--legacy-only`.
- [composer.json того же SHA](https://github.com/ichinya/laratesto/blob/2c7396efc2dc8d7af3dd2a83e89b5c0a75f73444/composer.json) требует PHP >=8.3, Laravel ^13.0, Testo ^0.10.45; README пишет ^0.10.42. Источник constraints — Composer, точный executable pin — lock, не README или HEAD.
- Этот README уже описывает bundled PHPUnit shim для fake/package assertions без установленного `phpunit/phpunit`. Это не гарантия поддержки каждого fake/API и не повод скрывать dependency: профиль должен показать native/shim/real-PHPUnit-required/unsupported для проверенного набора.
- `Laratesto/Pipeline/LaravelTestInterceptor.php` возвращает `Status::Aborted` при boot/cleanup failure; `Pipeline/Internal/FailureResult.php` различает lifecycle skip/cancel/abort. Reporter внутри test body не увидит все эти случаи.
- Testo snapshot [81b5d05 Status](https://github.com/php-testo/testo/blob/81b5d05e58fdd3afd80aea114cb17e419ddd5ff1/core/Core/Value/Status.php): Passed, Failed, Skipped, Error, Risky, Flaky, Cancelled, Aborted. `isSuccessful()` включает Flaky: его нельзя использовать как Lekalo pass predicate.
- [Testo JsonReport](https://github.com/php-testo/testo/blob/81b5d05e58fdd3afd80aea114cb17e419ddd5ff1/core/Output/Json/Internal/JsonReport.php) публикует summary и failed/error/aborted details, но успешные/skipped/risky tests сворачивает в totals. **`--json` недостаточно для per-test/per-step evidence**; нужен recorder + Testo result integration. Сырые stack/message/output содержат host paths и не должны попадать в neutral record.

## 2. Gap analysis против каждого acceptance criterion

| AC из #56 | Gap | Обязательное доказательство |
|---|---|---|
| Planner scenarios execute in Laravel fixture | PHP adapter и runnable Planner отсутствуют; есть общие inputs и Node backend. | Generated tests через настоящий Laravel kernel и persistence: happy/error/idempotent; fresh app/cleanup подтверждены sentinel tests. |
| HTTP/DB/auth/idempotency assertions work | Portable invoke не задаёт HTTP route; port должен соединять операции с native API, persisted state и policy. | HTTP status/body + DB state/count + owner/guest/foreign actor + same-key replay без дублирующей записи/эффекта; negative controls обязаны падать. |
| Node/Laravel normalize to same semantic scenario status | Neutral contract есть; Laratesto adapter результатов отсутствует; Node concurrency явно unsupported. | Один semantic corpus и IR digest, сравнение rows по scenario/step/kind/observes/outcome; missing case/row — fail. Проверить pass, fail, unsupported, infrastructure, degraded, не только зелёный happy path. |
| PHPUnit-required fake limitation visible | Старое предположение о всех fakes уже неверно для текущего shim; поддержки всех методов также нет. | Versioned capability matrix и negative fixture для unsupported/real-PHPUnit-only helper; limitation видна в evidence/verify, никогда skip-as-pass. |
| Assertion failure differs from boot/infrastructure | RunRecord различает outcomes, verify сворачивает общую причину; body-only reporter теряет boot failure. | Намеренно false assertion → fail; boot/migration/cleanup exception, timeout, corrupt report → infrastructure; пользователь видит разные причины и counters. |
| Trace scenario → Laratesto test → gate | В core есть builder, но default verify context без gate; PHP source-map association отсутствует. | Parsed trace manifest с тремя nodes, проверенными edges, hashes/source ranges; gate identity из валидированного receipt, не произвольной строки. |
| Legacy tests as additional gates | Laratesto unified command поддерживает legacy, Lekalo native manager/runner пока Node/test-only. | Отдельные Testo и Pest-or-PHPUnit receipts; каждый влияет на итог, legacy pass не заменяет scenario evidence. Проверить failing legacy и strict Testo-only profile. |

Остальные явные требования issue: generate/scaffold-once/bind-existing, session/Artisan, migrations/transactions/refresh, event/job/effect ports, clock/UUID, isolation, no production network/secrets, exact versions, ownership manifest — обязательны в шагах ниже.
