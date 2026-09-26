# Issue #54: research и план PHP/Laravel MVP target

## 1. Scope и проверенная база

- Источник требований: `gh issue view 54 --repo ichinya/lekalo`, затем комментарий владельца; прочитаны также #40, #55, #56.
- Срез: `ichinya/m5-issue-54`, исходный HEAD `e15ac298148060e55446832bec60ed00b148f640`, 2026-09-26; рабочее дерево исходно чистое.
- Это research, не реализация и не runtime acceptance: PHP/Mago/Laratesto/conformance здесь не запускались.
- MVP включает describe, strict PHP types, DTO/value objects/enums, schema/migrations, command/query/policy/endpoint scaffolds, Mago, Laratesto, manifests/drift, native gates и три ownership mode.
- Комментарий #54 дополнительно требует OpenAPI projection для Vue и greenfield `contracted` pilot. Публичное имя consumer — только **greenfield pilot**.
- Компоненты: `runtime=php-laravel`, `analysis=mago`, `testing=laratesto`, `storage=postgres-sql`, `transport=http-json`.
- #54 создаёт adapter и минимальные рабочие integrations; #55 владеет глубокой Mago graph/analysis интеграцией, #56 — полным Laratesto backend. Их зависимость от #54 не означает, что можно принять #54 без минимальных реальных diagnostics/scenarios.

## 2. Existing modules/contracts/tests: карта для implementer

Все пути ниже относительно корня; `core/` в таблице означает `crates/lekalo-core/src/`.

| Область | Файлы/символы и что переиспользовать |
|---|---|
| Process protocol | `core/target_protocol/{mod,wire,version,transport,scopes,plan}.rs`: `TargetClient`, `AdapterCommand`, `Finding`; token `lekalo.target/v1`, только protocol **0.3.2**, IR **0.2.16** (`core/ir/version.rs`). `plan-native` уже отдельная операция. |
| Confinement | `core/target_protocol/{confinement,confinement_windows}.rs`: исполняемый файл и **только первый script argument** копируются в private runtime; дополнительные assets требуют отдельного контракта. Ни sibling autoload/vendor, ни произвольный Mago binary автоматически не доступны. |
| Node reference | `adapters/node-typescript/src/kernel.mjs`: `createKernel`, `createReadView`, `createWriteView`, profile binding; `main.mjs`, `build.mjs`, `adapter.mjs`, `adapter.manifest.json`. Trusted launch profile задаёт read roots, request profile не является authority. |
| Generators | Node `src/{zod-map,zod-emit,zod-gen,generation-composite,openapi-gen,transport-extension,postgres-storage-extension}.mjs`; IR identity dispatch, pure mapping/rendering, dry-run/apply и evidence digests. Не переносить TypeScript compiler или Zod в PHP target. |
| Conformance | `core/adapter_conformance/`, `crates/lekalo-core/tests/adapter_conformance.rs`, `docs/adapter-conformance.md`, `tests/fixtures/adapter-conformance/`: 28 документированных checks, production TargetClient, default/strict, JSON/JUnit. Foreign `php-laravel` identity test использует fake Node adapter, не PHP runtime. |
| Profiles/portability | `core/target_profile/{component,resolution,portability}.rs`, `portability(source,target)`; fixture `tests/fixtures/target-profile/valid/laravel.json`. Laravel/Mago/Laratesto уже зарегистрированы; PHP typing/async и Mago types — partial. Это catalogue declarations, не результаты исполнения. |
| Ownership | `core/{artifacts,observed,bindings,contracted}/`, `docs/contracted-mode.md`, `contracts/contracted-declaration.schema.v0.4.0.json`. Contracted registry: `.lekalo/import/contracted/registry.json`; support artifacts допускаются **только `.lekalo/generated/**`**. |
| Types/expressions | `core/ir/`, `core/expressions/projection.rs`: `Target::Php`, `render_program`; PHP expression projection уже есть, полноценного PHP type/DTO adapter нет. |
| Storage | `core/storage_projection/derivation.rs::project`, `core/storage_engine/{migration,drift,postgres}/`; fixtures `storage-projection/derived/laravel.json`, `storage-engine/runtimes/php.json`. Это neutral projection/runtime binding evidence, не исполняемые Laravel migrations. |
| HTTP/OpenAPI | `core/{transport_http,openapi}/`; fixtures `transport-http/projected/laravel/laravel.expect.json`, `openapi/`; neutral contract/error identity сохраняются независимо от framework. |
| Scenarios | `core/scenario/`, `core/reference_evaluation/`, `core/scenario_evidence.rs::RunRecord::from_value`; `contracts/scenario-run.schema.v0.4.0.json`, ingest `.lekalo/import/scenario-runs`. Outcomes: pass/fail/unsupported/infrastructure/degraded; missing/unsupported не pass. |
| Node scenario proof | `adapters/node-typescript/src/scenario-{map,emit,gen}.mjs`, `scripts/test-node-scenario-{units,tests}.mjs`; fixture `tests/fixtures/orchestration/project/lekalo/scenarios/` + `src/testing/port.mjs`; compiled input `adapter-conformance/inputs/ir-minimal.json`. Четыре focus cases, concurrent case явно unsupported. |
| Planner contract slice | `tests/fixtures/contracted/planner-slice/`: maintained `src/{focus,queries,ids}.ts`, Model, initial/drift declarations, OpenAPI. Его `scenarios.yaml` — coverage definition, не достаточный executable Scenario IR. |
| Diagnostics | `core/diagnostics/provider.rs::{ProviderDiagnosticWire,normalize}`, registry/normalize/types: registered rule, logical range, namespaced original code; raw provider messages запрещены. Protocol `Finding` имеет только path/code/detail: полного provider evidence transport сейчас нет. |
| Native gates | `core/native_gate/{mod,wire,types,policy,receipt,fixture_tests}.rs`, Node `src/native-*.mjs`, `docs/native-gates.md`; immutable plan/approval/input digests. `wire.rs::MANAGERS` закрыт на pnpm/npm/yarn/bun, Composer отсутствует. |
| Execution boundary | `native_gate::production_run` по-прежнему возвращает blocked/unsupported; реальный runner только `#[cfg(test)]`. Наличие #89 confinement в target transport само по себе не включает native production execution. |
| CI/contracts | `.github/workflows/ci.yml`, `scripts/check-contract-versions.mjs`, `scripts/test-{target-protocol,diagnostic,contracted,scenario,storage-projection}-contracts.mjs`, `scripts/test-adapter-manifest-golden.mjs`. |

`adapters/php-laravel/` отсутствует. `tests/fixtures/adopt/laravel-app/` — adoption source fixture, не установленный Laravel Planner application.

## 3. Gap analysis: каждый acceptance criterion

| # | Критерий #54 | Текущее состояние / обязательное доказательство |
|---|---|---|
| AC1 | Adapter проходит conformance suite | Есть общий harness, PHP adapter отсутствует. Нужен запуск shipped entry через TargetClient, strict report без проваленных обязательных checks; skips перечислить, fake identity test не засчитывать. |
| AC2 | Planner Model реализуется в Laravel fixture | Есть Node contract/runtime inputs, Laravel projections и adoption snippets. Нужен загрузившийся Laravel с maintained Planner handlers, policy, HTTP и PostgreSQL persistence; runtime assertions должны проверять результат и DB. |
| AC3 | Node/Laravel scenarios эквивалентны | Есть Node generated scenario runner и neutral run records. Нужен один набор semantic Scenario IR, два реально выполненных target backend и сравнение per-assertion outcomes плюс return/error/state/effect observations. Одинаковые skips не доказательство реализованного поведения. |
| AC4 | Mago/Laratesto diagnostics нормализованы | Есть provider normalizer и scenario run contract, нет integrations; узкий protocol Finding теряет range/original code. Нужны реальный tool sample, version pin, transport/ingest custody и негативные fake-provider tests. |
| AC5 | Artifacts воспроизводимы | Core manifest/drift и Node patterns есть. Нужны два чистых PHP build/generate, повторный apply без изменений, LF/sorted outputs, digests и безопасный clean. |
| AC6 | Limitations в portability report | Static PHP partial capabilities уже есть; не отражают конкретные backend gaps. Нужны capability/evidence mapping и golden Node→Laravel report, где видны unsupported assertions, async/typing и platform/tool gaps. |
| AC7 | Core Model не меняется ради Laravel без ADR/evidence | На этом срезе менять Model не требуется. Проверить неизменность Model/IR schemas, identifiers и semantic definitions; versioned protocol/native evidence extensions обосновать отдельно, не выдавать за изменение domain Model. |

## 4. External compatibility evidence и решения

- Context7: `/laravel/docs` подтверждает Laravel 13 с PHP >=8.3; рекомендуемый fixture baseline — Laravel 13, PHP 8.3 и 8.5 CI, exact Composer lock. [Release policy](https://github.com/laravel/docs/blob/13.x/releases.md).
- Laravel `make:migration` создаёт timestamped filename; generator должен сам вычислять стабильные имена, не вызывать этот command при генерации. [Migrations](https://github.com/laravel/docs/blob/13.x/migrations.md).
- Context7: `/carthage-software/mago`, команды `analyze`, `lint`, `guard`; брать JSON/SARIF из pinned executable, без internal Rust crates и без fix. Точный format/exit contract подтвердить `--help` и captured positive/negative samples выбранной версии. [Reporting](https://github.com/carthage-software/mago/blob/main/docs/content/en/fundamentals/shared-reporting-options.md), [Guard](https://github.com/carthage-software/mago/blob/main/docs/content/en/tools/guard/command-reference.md).
- Context7 не нашёл Laratesto; прочитан upstream README через GitHub API, HEAD `2c7396efc2dc8d7af3dd2a83e89b5c0a75f73444`. Он указывает PHP 8.3/Laravel 13, Testo `^0.10.42`, sequential tests и `php artisan laratesto:test --testo-only`; exact tested versions ещё надо зафиксировать lock. [Pinned README](https://github.com/ichinya/laratesto/blob/2c7396efc2dc8d7af3dd2a83e89b5c0a75f73444/README.md).
- В этом README Laravel fake assertions используют bundled PHPUnit shim без установленного phpunit package. Поэтому ограничение #56 нельзя описывать как абсолютное «fakes требуют PHPUnit»: записывать конкретный pin, shim use и реально unsupported operations; обычные PHPUnit/Pest gates учитывать отдельно.

## 5. План реализации

Будет дополнен отдельным логическим шагом после фиксации inventory/gaps.
