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
- В Laratesto `src/Pipeline/LaravelTestInterceptor.php` возвращает `Status::Aborted` при boot/cleanup failure; `src/Pipeline/Internal/FailureResult.php` различает lifecycle skip/cancel/abort. Reporter внутри test body не увидит все эти случаи.
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

## 3. Implementation sequence: небольшие проверяемые commits

### S1 — Зафиксировать PHP port, profile и toolchain evidence

- Начать после #54 с `adapters/php-laravel/src/{ScenarioMap,LaratestoEmitter,ScenarioReporter}.php`, используя его protocol entry/kernel и dry-run/apply. Compilation только читает данные; application/vendor/port code не исполняется в compiler process.
- Версионировать `contracts/test-port.schema.*.json`: PHP logical `.php` path и явно заданная PHP callable/class surface; старые Node документы читаются по своей версии. Добавить parser/shape fixtures и обновить Node compatibility tests, не расширять опубликованный v0.4.0 молча.
- Сохранить существующие neutral exports; определить PHP `ScenarioPort` interface в `adapters/php-laravel/runtime/ScenarioPort.php`: invoke/state/fixtures/actor/clock/ids/emissions/effects/authorize/reset и optional contractCheck/fixtureDigest. Наличие export flag проверяется runtime self-test, false/absent даёт unsupported.
- Ввести bounded closed backend-run sidecar contract, например `contracts/scenario-backend-run.schema.<current-product-version>.json`, parser `core/scenario_backend_evidence.rs`, ingest `.lekalo/import/scenario-backends/` (не в папке run records: текущий scanner парсит все `.json` как RunRecord).
- Sidecar: target/profile digest, exact PHP/Laravel/Laratesto/Testo versions и Composer lock hash, source references для dev pins, capability matrix digest, expected scenario/test inventory, test fingerprints, port/config/input digests, finalized run-record digests, validated gate receipt reference. Нужны limits, logical paths, canonical serialization, authority/privacy entries; никаких env dumps и raw tool output.
- Старый neutral scenario-run сохраняет shape и runner.version=точная версия Testo; profile.digest связывает проверенный профиль. Sidecar связывается через hashes и проверяется core, не является произвольным «metadata JSON»; unknown/missing tool version блокирует strict evidence.
- Обновить affected contract registry/version gates и docs; follow текущую product-version policy, не придумывать заранее номер версии и не менять unrelated IR/Model families.

### S2 — Pure Scenario IR → Laratesto tests

- `ScenarioMap::mapScenario` проверяет identity, closed shape, bounds, exact IR/model refs, typed leaves, step reachability и symbols; сохраняет behavioral order. Реализовать parity vectors из Node mapper, не портировать его JavaScript coercions в PHP.
- `LaratestoEmitter::emitScenarioTests` создаёт strict PHP `*Test.php` с public `test*` methods, `LaravelTestCase`, stable mapping scenario ID → PHP FQN/method → semantic native-test ID. FQN с `::` не использовать как trace node ID; semantic native ID должен включать target, чтобы Node/Laravel не сливались.
- Generated tests вызывают project port и Testo assertions; поддержать result/error/entity_state/emitted/forbidden_effect/authorization/idempotency/contract_match/deterministic_fixture. Explicit unsupported и отсутствующие capabilities дают отдельные unsupported rows. Реальный scheduler/race, resource-scoped effect и weaker equivalence не подменять последовательным запуском/strict equality.
- Given entity/fixture/actor/clock/id-source и invoke с idempotencyKey/replay должны сохранять типы. Decimal/int64 не преобразовывать через float, null не смешивать с absent, refs разрешать только из объявленных предыдущих шагов; PHP escaping исключает interpolation/code injection.
- Proposed generated support home `.lekalo/generated/php-laravel/scenario-tests/`: `ScenarioTestKit.php`, recorder bridge, `{module}/{scenario}Test.php`, canonical maps. Contracted support остаётся под `.lekalo/generated/**`; tracked user tests не перегенерируются.
- `generate` возвращает sorted ownership-aware write plan; `verify` проверяет bytes/fingerprints/inputs, без запуска tests. Unsupported semantics не создают зелёную заглушку; invalid input vetoes writes.

### S3 — Generated / scaffold-once / checked ownership

- Расширить #54 `ScenarioGeneration` handler: generated файлы полностью managed; scaffolded тест один раз создаётся в `tests/Feature/Lekalo/`, затем user-owned; checked binding только на существующий native test.
- Для scaffold-once хранить ownership transition и исходный fingerprint; повторный generate сохраняет пользовательские правки, missing scaffold выдаёт actionable diagnostic, а не молча восстанавливает файл. Cleanup не удаляет scaffolded/checked tests.
- `ScenarioBindingResolver` использует #54 observed scan index и stable ID annotation/sidecar, проверяет ровно одного владельца, FQN, fingerprint, scenario/version/IR binding. Missing/duplicate/stale identity — fail/degraded как определено custody, никогда автоматический substitute generated test.
- Checked/scaffolded tests подключают `ScenarioRecorder` и регистрируют покрытые assertion IDs явно; успешный старый test без per-step evidence не доказывает coverage всех then steps.
- В `core/orchestration/generate.rs` изменить `artifact_kind_for` и `module_path_of`: explicit validated artifact path из map либо target-aware suffix mapping, сохранив Node default. Включить PHP test/support/data classification и exact byte source ranges в ownership manifest.
- Tests: generation twice byte-identical; modified generated file refuses overwrite; scaffold edit survives regenerate/clean; checked emits no replacement; ambiguous/missing binding rejects; PHP map points to `.php`, traversal/symlink/out-of-root paths reject.

### S4 — Laravel harness, isolation и target test ports

- Добавить полноценную fixture `tests/fixtures/php-laravel/planner/`: `composer.json`, committed `composer.lock`, `artisan`, `bootstrap/app.php`, `config/`, `routes/api.php`, `testo.php`, migrations, `tests/Support/PlannerPort.php`, test-only providers. Переиспользовать maintained handlers/repository из #54, не создавать отдельную fake business implementation для green tests.
- `PlannerPort::invoke` связывает operation IDs с HTTP kernel endpoints или явно объявленными command/query bindings; state читает реальную DB. Body/status/error mapping проверяется против transport contract, не выводится из имени операции. Actor binding использует actingAs/actingAsGuest и реальный guard/model.
- Добавить target adapter helpers для HTTP request/response, DB has/missing/count, session has/missing/errors, Artisan exit/output. Session/Artisan checks — target-native port self-tests/fixtures; не добавлять Laravel-specific assertions или arbitrary Artisan/SQL в portable Scenario IR.
- `testo.php` регистрирует `LaravelPlugin`, discovery и reporter plugin; Laravel tests строго sequential, fiber plugin выключен. Один fresh application на **scenario test**, все when/replay steps внутри него; между tests сбрасывать auth, session, cookies, facades, clock, UUID sequence, captured effects.
- Установить test clock и UUID bindings через test provider до выполнения handler; ledger event/job/effect capture реализовать project test ports, без реальной mail/queue/network delivery. Доказывать emitted/forbidden/duplicate effects наблюдением, а не только declarations.
- DB lifecycle через Laratesto `RefreshDatabase`, `DatabaseMigrations`, `DatabaseTransactions` attributes, не PHPUnit traits. Проверить миграции, rollback, cleanup failure и порядок boot → DB → user setup → test → user teardown → DB teardown → static cleanup.
- Disposable runtime root с synthetic `.env.testing`, отдельной DB на run (SQLite fast lane; PostgreSQL lane для #54 storage semantics), local array/sync drivers, fixed timezone/locale. Preflight **до boot/migrate** проверяет разрешённую DB, отсутствие production env/config cache и egress boundary; APP_ENV=testing сам по себе недостаточен.
- Dependency installation отдельно на CI provisioning step по lock; runtime не вызывает Composer install/update/scripts и не наследует credentials/host env. Runtime allowlist environment, isolated temp/cache/session paths, process timeout/output caps/descendant cleanup; network denied либо evidence blocked, не обещать sandbox там, где он недоступен.
- Capability probes: native HTTP/DB/auth, shim-backed selected fake assertions, real-PHPUnit-only/unimplemented helper. Strict `--testo-only` запрещает legacy runner, но не запрещает bundled shim автоматически; отражать shim version/coverage явно, unsupported helper не трактовать как semantic failure приложения.

### S5 — Testo results → neutral evidence + complete trace

- `runtime/{ScenarioRecorder,ScenarioEvidencePlugin}.php`: recorder собирает per-step outcomes; plugin обрабатывает terminal `TestResult` через проверенный API выбранного Testo pin, включая boot/cleanup вне body. Не опираться на CLI JSON totals или парсинг human output.
- `src/ScenarioReporter.php::normalizeResult` и supervisor `finalizeRun`: reconcile planned inventory, per-step recorder и terminal results; final evidence публикуется atomic rename только после teardown. Boot crash до plugin, missing/truncated report, timeout/nonterminal run → synthesized scenario-level infrastructure row по expected inventory, не старый pass.
- Перед запуском создать пустой run-specific staging home; импортировать только records этого run с matching inputs/test/port/config digests. Duplicate/unknown IDs, stale fingerprints, extra/missing assertion rows и malformed sidecars fail closed; один scenario в двух targets не перезаписывает файл другого (flat filenames включают target/test digest).

| Testo / runner observation | Neutral outcome / правило |
|---|---|
| Passed + все ожидаемые assertions подтверждены + cleanup successful | pass; totals/exit 0 без row inventory недостаточны |
| Failed assertion / expectation | fail с соответствующим step; typed domain error, ожидаемый сценарием, проверяется assertion и может дать pass |
| Aborted/Error, boot/migration/cleanup/spawn/timeout/report failure | infrastructure; unexpected throwable не выдавать за ожидаемый domain error |
| Skipped с доказанной unsupported capability | unsupported с bounded capability/reason |
| Обычный skip, Risky, Flaky | degraded; retries не превращают conformance в pass |
| Cancelled / unfinished expected test | infrastructure; отсутствующий run вообще — not-run/unsupported component, не pass |

- В `core/orchestration/verify.rs` сохранить отдельные fail/infrastructure counters и причины в rollup/receipt; новый diagnostic регистрировать в registry, если нет подходящего. Missing expected suite/test inventory не должен стать успешным частичным run.
- `scenario_backend_evidence` валидирует toolchain/input/test/gate custody до trace export; построить contexts на каждый подтверждённый backend gate. Нельзя прикреплять все records к одному произвольному gate.
- `trace_manifest_document` → `TraceManifest::parse` → durable exported manifest: native_test verifies scenario/operations; gate evidences native_test/scenario. Проверить source ranges/ownership/test hash, current model/IR digest и отсутствие cross-target ID collisions. Legacy gates не получают semantic edges без bindings.
- Приватность: bounded fixed reason tokens, logical paths; stdout, exception text/stack, SQL, env и secrets не копируются в neutral artifacts. Raw synthetic diagnostic samples хранятся только как reviewed test fixtures.

### S6 — Native gates / legacy coexistence

- С #54 расширить native gate contract successor и `core/native_gate/{types,wire,policy,receipt,fixture_tests}.rs` для Composer/PHP tools, точного lock/tool/config/argv fingerprint; не ослаблять shell/lifecycle-hook restrictions. #56 подключает scenario/test recipes, не включает production runner обходным флагом.
- В trusted fixture harness две отдельные argv recipes: `php artisan laratesto:test --testo-only` и, при наличии legacy runner, `php artisan laratesto:test --legacy-only`; legacy chooses Pest иначе PHPUnit, не оба. Unified `php artisan laratesto:test` проверить отдельно как consumer compatibility, не смешивать его stdout в scenario evidence.
- Strict post-migration profile исполняет только Testo и явно показывает excluded legacy gate; mixed profile запускает оба gates даже при failure первого, если не задан fail-fast. Нельзя объявлять missing required legacy binary successful optional gate.
- Production native run остаётся blocked/unsupported до отдельной подтверждённой execution capability; AC доказывается реальным disposable fixture harness. Состояния gate failed/infrastructure/unsupported/blocked не смешивать с neutral assertion status.

### S7 — Acceptance fixtures, parity, CI и документация

- `tests/fixtures/php-laravel/parity/` связывает общий semantic corpus и compiled IR с target bindings; исходные четыре focus JSON использовать из общей fixture, не поддерживать divergent PHP copies. Добавить neutral owner/guest/foreign-actor, DB-count/query, duplicate-effect cases вместе с Node port support; raw HTTP transport extras проверять отдельно.
- Новые `scripts/test-php-laravel-{scenario-units,scenario-tests,parity}.mjs`: unit contracts/goldens, materialize adapter → dry-run/apply → port selftest → real Testo execution → neutral schema/custody/trace validation → verify drift. Parity запускает оба backends и сравнивает semantic row tuple и canonical result/error/state/effect observations, исключая runner/path/duration.
- Оба backend должны иметь реально executed happy/error/idempotent/auth/state cases; одинаковое unsupported не является доказательством реализации. Concurrency остаётся явно unsupported до настоящего scheduler/DB concurrency proof.
- Negative controls в disposable copies: wrong HTTP body/status, disabled policy, wrong DB count, duplicate idempotency effect → fail; broken bootstrap/migration/cleanup, killed process → infrastructure. Дополнительно missing port export, unsupported fake, spontaneous skip, flaky retry, no assertions, malformed/stale/forged/oversized evidence, wrong gate hash → не pass.
- `tests/fixtures/php-laravel/evidence/` хранит reviewed Testo result samples и expected neutral records для всех statuses; `ownership/`, `bindings/`, `isolation/`, `legacy/` — fixtures из S3–S6. Isolation sentinel меняет clock/auth/session/DB в первом тесте, второй доказывает отсутствие утечки; обратный порядок и rerun дают тот же semantic result.
- Добавить PHP cases в unit tests `core/scenario_evidence.rs`, `core/orchestration/{generate,verify}.rs`, `core/trace/`; schema positive/negative tests для port/backend sidecar; PHP adapter conformance запускает реальный shipped artifact, не fake target identity.
- CI `.github/workflows/ci.yml`: pinned PHP 8.3 baseline + supported newer PHP lane, locked Laravel 13/Testo/Laratesto, SQLite lane и disposable PostgreSQL integration lane, required PHP runtime job без silent missing-tool skip. Install по lock отдельно, runtime offline; publish sanitized evidence + manifests. Не считать Node gate заменой PHP acceptance.
- Обновить `docs/scenario-ir.md`, `docs/native-gates.md`, добавить `docs/php-laravel-scenarios.md`: modes, port surface, commands, evidence fields, unsupported/fake matrix, strict migration profile, tested versions и execution boundary.

## 4. Commands и критерий завершения implementation

Из repo root, после соответствующих S1–S7; новые scripts до реализации не существуют:

```text
node scripts/test-scenario-contracts.mjs
node scripts/test-node-scenario-units.mjs
node scripts/test-node-scenario-tests.mjs
cargo test -p lekalo-core --test scenario
cargo test -p lekalo-core scenario_evidence
cargo test -p lekalo-core execution_rollup
cargo test -p lekalo-core --test adapter_conformance
node scripts/test-target-protocol-contracts.mjs
node scripts/test-native-gate-contracts.mjs
node scripts/check-contract-versions.mjs
node scripts/test-php-laravel-scenario-units.mjs
node scripts/test-php-laravel-scenario-tests.mjs
node scripts/test-php-laravel-parity.mjs
git diff --check
```

- В disposable Planner root: provisioning `composer install --no-interaction --prefer-dist --no-scripts`, затем controlled fixture setup; `php vendor/bin/testo run --suite=Laravel` и recipes S6. Composer scripts/application boot допускаются только отдельным reviewed fixture setup, не при compiler validation.
- Conformance CLI invocation брать из `docs/adapter-conformance.md` с shipped PHP entry/profile после #54; strict surface включает все девять operations. Сохранять report, skips и реальные unsupported capabilities, не делать фиктивную «полную» реализацию.
- Done: все семь AC доказаны runtime fixtures, все issue modes и дополнительные capabilities проверены; Node regression green; ownership/trace/evidence contracts validated; corruption/isolation/negative controls краснеют ожидаемым outcome. Research commit сам по себе ни один runtime AC не закрывает.

## 5. Риски и рекомендуемые решения

| Риск / неоднозначность | Решение |
|---|---|
| #54 отсутствует как реализация в этом checkout | Первый implementation шаг — сверить dependency branch и переиспользовать adapter skeleton, bundle confinement, fixture и source-map контракт. Отдельный backend entry не изобретать. |
| Upstream HEAD ≠ выпущенный совместимый пакет; README/Testo range расходятся | Выбрать совместимый released pin/immutable dev reference, commit lock, снять actual versions из установленного runtime; fixtures Testo API/results закрепить на этом pin. Изменения внешнего Laratesto требуют отдельного cross-project PR. |
| Shim расширяет fake support, issue требует показать PHPUnit limitation | Capability matrix на exact pin + unsupported negative probe. Не объявлять все fakes unsupported и не обещать полную PHPUnit compatibility. |
| Testo public events могут не покрывать всякий shutdown | Spike reporter на chosen pin с boot/teardown failures; recorder + outer supervisor inventory гарантируют negative terminal evidence. Не брать internal JSON report как стабильный полный result API. |
| Single scenario-run runner.version не вмещает весь toolchain | Validated digest-linked backend sidecar, без неразрешённых полей в v0.4.0; format adoption и version migration делаются вместе с Rust/Node readers. |
| Node enum/status parity может скрыть разные наблюдения | Общие semantic inputs и typed observation comparison; negative mutations плюс обязательный nonempty executed subset. Node known unsupported не «исправлять» ложным PHP pass. |
| Laravel globals / DB transactions маскируют stale state | Sequential fresh-app tests, cleanup sentinels и separate PostgreSQL integration; SQLite не доказывает postgres locking/races. |
| Generic verify сейчас сворачивает infrastructure и gate trace | Изменить rollup и validated context join явно; AC требует observable classification и parsed exported manifest, не только внутренний helper test. |
| Network/env denial отсутствует в части native runner platforms | Fixture harness в проверенной изоляции; production remains blocked. Ни APP_ENV, ни disposable directory отдельно не являются security boundary. |

## Research validation

- Проверены authoritative issue, перечисленные local contracts/symbols и upstream pinned source snapshots; repository/runtime implementation не менялись.
- Для этого documentation-only task: `git diff --check`, лимит ≤300 строк, staged-path audit и commit на dedicated branch. Runtime tests перечислены как будущая acceptance strategy, не как выполненные проверки.
