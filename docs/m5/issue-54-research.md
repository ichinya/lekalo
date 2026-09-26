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

Рекомендуется самостоятельный PHP process adapter: development sources в `adapters/php-laravel/src/`, детерминированный single-file `adapter.php` без Composer runtime dependency. Laravel/vendor нужны fixture/runtime gates, не загрузке protocol adapter. Ни Node, ни Rust library linkage не должны быть обязательны PHP consumer.

### S1 — Уточнить seams, версии и evidence custody (первый implementation commit)

1. Добавить `docs/adr/<allocated>-php-laravel-target.md` и `adapters/php-laravel/README.md`: mode matrix, PHP 8.3 minimum, component ids, output roots, отказ на unsupported и граница #54/#55/#56.
2. Добавить adapter-local toolchain manifest с exact PHP/Mago/Laratesto/Testo/Laravel versions и artifact hashes; Composer lock для fixture. Проверять tool presence/version до execution; no install/update внутри adapter или gate.
3. Зафиксировать native-contract successor: Composer manager в `native_gate/{wire,types,policy}.rs` и соответствующих `contracts/native-gate-*.schema.*.json`, composer lock/autoload/config custody вместо выдуманного tsconfig. Нельзя назвать Composer `npm-standalone` или расширить опубликованный 0.3.2 enum без version decision.
4. Вместе с #55 определить минимальный provider-evidence ingest: bounded document с `ProviderDiagnosticWire`, tool/input digests, зарегистрированный `.lekalo/import/...` home, core decoder и verify consumer. Предпочтительно отдельный neutral ingest contract вместо расширения `Finding.detail`; если меняется protocol — новый family version, validator, fixtures, Node peer negotiation и version custody вместе.
5. В `core/diagnostics/registry.rs` и published registry добавить лишь необходимые PHP/tool rules с bounded payload; сохранить оригинальный provider code в metadata. Не хранить raw tool logs в public evidence.
6. До реальной интеграции согласовать test-only runner seam с владельцами native gates: #54 не включает production native execution автоматически. В report раздельно показать plan support и executed fixture gate evidence.

### S2 — Protocol kernel, packaging и conformance

- Новые `src/{Kernel,Protocol,Profile,ReadView,WritePlan,CanonicalJson}.php`; рекомендуемые API `Kernel::handle`, `Profile::bindRequest`, `WritePlan::fromArtifacts`, `CanonicalJson::encode`.
- `build.php` объединяет проверенные source files в `adapter.php`; никаких clock/path/version-from-environment insertions. `adapter.manifest.json` покрывает shipped bytes; расширить `scripts/test-adapter-manifest-golden.mjs` на второй adapter.
- Entry принимает один bounded JSON request через stdin/request-file, отдаёт ровно один response; diagnostics не загрязняют stdout. Соблюсти duplicate-key/depth/size/unknown-field rejection, request id, negotiated IR/version и cancellation/exit discipline.
- Trusted inline launch profile аналогичен Node; связать id/target/digest/capabilities **до reads**. Проверять scopes, canonical paths, symlinks/junctions, protected Model/OpenSpec homes; отсутствие profile не превращать в чтение cwd.
- Полный strict operation set из `adapter_conformance/mod.rs::ALL_OPERATIONS` содержит **9**, включая `plan-native` (не копировать старое описание восьми операций). Реализовать describe/scan/bind/validate/generate/verify/plan-clean/clean/plan-native; staged intermediate describe-only результат не AC1.
- PHP process qualification через production `TargetClient`: PHP dynamic libraries/config/DLL lookup проверить на Linux и Windows. `php adapter.php` сохраняет script первым аргументом; не вставлять `-n` перед ним без изменения packaging contract. Недоступный runtime bundle — явный blocker, не запуск мимо confinement.
- Tool execution не делать скрытым subprocess внутри kernel: Mago/Laratesto работают через согласованный native fixture runner, adapter читает проверенное evidence. Если требуется execution внутри adapter, сначала нужен явный runtime-tool-bundle/authority contract.
- Тесты: новые `adapters/php-laravel/tests/{protocol,roots,plans,packaging}.php`, `scripts/test-php-laravel-adapter.mjs`, Rust `tests/php_laravel_adapter.rs`; fake executable и malformed requests, реальные dry-run/apply/clean, stale/consumed plan, writes вне manifest, crash, flood, cancellation, hostile profile.

### S3 — Strict type mapping и deterministic support generation

- Новые `src/{TypeMap,DtoEmitter,EnumEmitter,ScaffoldEmitter,GenerationComposite}.php`; `TypeMap::map`, `DtoEmitter::emit` чистые функции над проверенным IR. Не читать Laravel conventions из core Model.
- Mapping покрывает весь `ir/mod.rs::ScalarBase`: string→string, boolean→bool, number→validated finite int/float с проверкой representable range/precision, date/datetime→validated wrappers, uuid/uri→validated string wrappers. Constraint-aware integers проверять по range; не добавлять в IR выдуманный decimal primitive: exact decimal storage/scenario values передавать canonical string/value object, без float conversion.
- Opaque IDs — final readonly wrappers; enums — backed enum с проверенными значениями; value objects/DTO — final readonly explicit properties/constructors. Потеря number precision должна давать unsupported, не молчаливое округление.
- Required key и nullable value независимы: deserializer различает absent и null (`array_key_exists`, presence representation), включая все четыре комбинации. Collections проверять рекурсивно, включая keys/element types; PHPDoc недостаточен runtime contract.
- Date/datetime — отдельные validated representations с сохранением neutral calendar/timezone precision semantics; arbitrary object/mixed/magic conversion не fallback. Неизвестные scalar/union/recursive construct — typed unsupported до выдачи успешного generation plan.
- Name resolution: deterministic namespaces/imports, reserved keywords, case-insensitive PHP collisions, cross-module refs и stable source-map semantic ids. `declare(strict_types=1)` во всех generated PHP; no dynamic props, `__get/__set`, facades/service locator в portable domain.
- Output по умолчанию `.lekalo/generated/php-laravel/{types,scaffolds,migrations,openapi,tests}/`; fixture Composer autoload явно подключает generated types. Один composite dispatch по document identity, deterministic LF/sorting/escaping, fixed headers, no wall-clock filenames.
- Scaffolds: command/query interfaces и handler templates, policy interface, HTTP controller/route registration templates. Unimplemented body явно бросает typed unsupported, не возвращает фальшивый success; maintained bodies реализуются отдельно в fixture.
- Fixtures: `tests/fixtures/php-laravel/types/{valid,invalid,golden}/`: presence/null, enum, ID, nested collections, decimal boundary, date/offset, integer overflow, case collisions, unsupported reference. Проверять generated code через PHP lint и runtime roundtrip/negative inputs.

### S4 — Schema/migrations, HTTP/OpenAPI, ownership и drift

- `src/{StorageEmitter,TransportEmitter,OpenApiEmitter,ArtifactManifest,ContractDeclaration}.php`: использовать neutral storage/HTTP/OpenAPI evidence и semantic ids; не заново интерпретировать SQL types из PHP naming.
- PostgreSQL MVP: tables/columns/null/default/PK/FK/indexes, unique focused task rule и transactions; Laravel migration wrappers над deterministic DDL/projection. Stable ordered migration identifiers из input digest, append-only applied migration history; destructive changes дают approval-required plan, никакого auto-migrate.
- Verification сравнивает emitted projection с read-only DB snapshot; `scan.schema`/`verify.schema-projection` объявлять только после реализации. SQLite может проверять boot, но не доказывает PostgreSQL constraints/locking parity.
- OpenAPI output нужен для Vue consumer: request/response DTO, HTTP error `{id,code,category}`, nullable/required semantics и stable operation ids. Сверять с `core/openapi` projection/goldens и реально обслуженным endpoint; frontend generation не scope #54.
- Mode matrix: observed только bounded scan/evidence/binding proposal; contracted проверяет maintained code и генерирует только support в `.lekalo/generated/**`; limited managed управляет исключительно явно перечисленными generated artifacts.
- Scaffold-once: сгенерировать templates как support, перенести в maintained source явным действием при fixture setup; последующие generate/clean никогда не меняют перенесённые handlers/policies/tests. Не смешивать generated и custom code в одном файле.
- Manifest использует core ownership/source-map semantics: owner symbol, target, kind/lifecycle, input/output hashes, generator version. Changed generated file — drift/refusal до overwrite; cleanup только exact manifest paths с проверкой fingerprints.
- Fixtures: `ownership/{observed,contracted,managed}`, mutation sentinels для `app/`, `routes/`, maintained tests; `storage/` valid/destructive/golden, `http/` success/declared-error/infrastructure, `openapi/` parity golden.

### S5 — Минимальные Mago и Laratesto integrations (#55/#56 seams)

- `src/{MagoEvidence,DiagnosticMapper,ScanEvidence}.php`: consume pinned Mago process JSON/SARIF через S1 ingest; rule/source span/original code → registered Lekalo diagnostic + semantic source map. Не писать собственный PHP parser; неподтверждённый relation/route/effect — uncertainty, не proved binding.
- Реальные non-mutating recipes: `mago analyze --reporting-format json`, `mago lint --reporting-format json`, `mago guard --reporting-format json` после capability probe выбранного pin; никогда `--fix`. Использовать controlled config/read roots и bounded stdout/stderr/deadline.
- Missing executable, malformed output, timeout/crash, unsupported version и genuine lint/analysis findings должны давать разные статусы. Captured synthetic JSON/SARIF fixtures + fake analyzer покрывают это независимо от установленного Mago.
- Strict fixtures: missing strict_types, non-readonly/finality profile, dynamic properties/variable variables, facade dependency, magic state, missing array shape, generated drift; один real positive и несколько real negative gates обязательны.
- `src/{ScenarioMap,LaratestoEmitter,ScenarioReporter}.php`: Scenario IR → generated Laratesto tests; `tests/fixtures/php-laravel/planner/tests/Support/PlannerPort.php` реализует invoke/query/actor/clock/UUID/effect capture. Reporter пишет canonical scenario-run v0.4.0 с per-assertion outcome, stable scenario/test/gate links, IR digest и runner version.
- Generated PHP tests используют Laratesto public plugin/HTTP/DB/auth API в fresh app per test, последовательный запуск. Pin Testo JSON output samples; различать failed assertion, skipped/unsupported и boot/infrastructure exception; stdout не парсить как human prose.
- Для traceability сохранить scenario → generated test/source range → operation → native gate и hashes. Additional Laravel/Testo/PHP versions хранить в разрешённом toolchain evidence, не добавлять неизвестные поля в closed scenario-run contract.
- Нельзя заявлять эффект/policy conformance лишь по сохранённой declaration: runtime denial, rollback, emitted effects и idempotency проверяются через test ports. PHPUnit shim limitation отражать по фактическому pin, отдельные legacy gates не подменяют Laratesto execution.

### S6 — Laravel Planner и cross-target semantic parity

- Новая полноценная fixture `tests/fixtures/php-laravel/planner/`: `composer.json`, `composer.lock`, `artisan`, `bootstrap/app.php`, controlled `config/`, `routes/api.php`, `testo.php`, `mago.toml`, target/profile/toolchain evidence и synthetic-only test environment.
- Maintained `app/Domain/Planner/{FocusTask,CountFocused,PlannerPolicy}.php`, `app/Infrastructure/PlannerRepository.php`, `app/Http/PlannerController.php`; typed generated DTOs не Eloquent magic domain objects. Framework container/DB adapters остаются снаружи domain boundary.
- Источник executable scenarios — четыре `tests/fixtures/orchestration/project/lekalo/scenarios/planner.scenario.focus_*.json` и тот же compiled `ir-minimal.json`, что использует Node gate. Не объявлять contracted slice coverage YAML executable test.
- В новом `tests/fixtures/php-laravel/parity/` зафиксировать canonical semantic inputs/digests; target bindings/profile отделены. Если для HTTP/DB/auth нужны новые cases, добавить одинаковые neutral scenarios и Node port bindings, а не менять semantics только для Laravel.
- `scripts/test-php-laravel-parity.mjs` материализует два disposable runtime roots, запускает Node generated tests и Laravel tests, проверяет real run receipts и нормализует к scenario/step/assertion/outcome + canonical return/error/state/effect values. Runner-specific paths, durations и native test names не сравнивать; отсутствие case/row — failure.
- Обязательные executed cases: happy focus, replay с тем же idempotency key, declared error/rollback, owner vs guest/foreign actor, query/DB count, HTTP JSON/error identity. Negative control: сломать handler/policy в disposable copy — parity gate обязан упасть.
- Concurrent focus case у текущего Node runner unsupported: в MVP честно одинаковый unsupported с portability limitation; не принимать его как passed race proof. Для обещания race semantics потребуется реальный deterministic concurrent harness обоих backends.

### S7 — Composer/Laravel native gates, portability и CI

- `src/{ComposerWorkspace,NativePlan}.php`: read-only composer.json/lock parsing, exact package/tool/config digests, confirmed command recipes, affected paths и source/test mapping. Сначала standalone Composer package; path-repository graph и shell scripts либо явно реализовать, либо unsupported.
- Планировать direct argv `php -l <file>`, pinned Mago и `php artisan laratesto:test --testo-only --testo-arg=--json`; Composer metadata ≠ разрешение `composer run-script`. Install/update/plugins/lifecycle hooks не запускаются. Native recipe options и subprocess descendants проверяются отдельно от protocol entry.
- Расширить `native_gate/fixture_tests.rs` и trusted synthetic catalog на PHP fixture после S1 contract change: exact digest approval, disposable copy, no original writes, controlled env/DB, bounded process tree, stale plan zero spawns, cleanup evidence. Production `native run` сохраняет честный blocked/unsupported, пока отдельная интеграция executor не реализована.
- `src/PortabilityEvidence.php` и tests над `target_profile::portability`: named limitations для PHP typing/async, unmapped types, Mago unavailable, Laratesto assertions/shim/concurrency, unsupported Composer topology и platform confinement. Новые capability names требуют versioned component catalogue decision; не писать free-form fields в closed report.
- `.github/workflows/ci.yml`: PHP 8.3/8.5 fixture jobs, Node 24 parity baseline (existing TypeScript test execution требует подходящий Node), pinned Mago, PostgreSQL service, явный setup dependencies, deterministic bundle/manifest gate, production TargetClient conformance, diagnostics + parity + drift tests.
- Dependency provisioning — отдельный видимый CI/bootstrap step с lock, `--no-scripts --no-plugins`; нужный Laravel package discovery включить явно controlled recipe. Не коммитить vendor, реальные credentials или private consumer names.

## 6. Команды и test strategy

Новые scripts ниже — deliverables плана, сейчас их нет. Все fixture mutation/execution scripts должны сами создавать disposable roots и очищать только их; команда из repo root не должна менять committed goldens без явного update режима.

```text
php adapters/php-laravel/build.php --check
node scripts/test-php-laravel-adapter.mjs
node scripts/test-php-laravel-adapter.mjs --conformance
node scripts/test-php-laravel-generation.mjs
node scripts/test-php-laravel-diagnostics.mjs
node scripts/test-php-laravel-planner.mjs
node scripts/test-php-laravel-parity.mjs
cargo test -p lekalo-core --test php_laravel_adapter
cargo test -p lekalo-core --test adapter_conformance
cargo test -p lekalo-core --test target_protocol_security
cargo test -p lekalo-core native_gate
cargo test -p lekalo-core scenario_evidence
node scripts/check-contract-versions.mjs
node scripts/test-adapter-manifest-golden.mjs
node scripts/test-node-scenario-units.mjs
node scripts/test-node-scenario-tests.mjs
```

- `--conformance` wrapper должен собирать `lekalo`, передавать shipped PHP entry **через `lekalo adapter test --profile strict --report json`**, trusted inline launch profile fixture и сохранять JSON/JUnit receipts. Direct PHP unit tests не заменяют этот запуск.
- Existing Ajv gates после S1: `test-target-protocol-contracts.mjs`, `test-diagnostic-contracts.mjs`, `test-target-profile-contracts.mjs`, `test-contracted-contracts.mjs`, `test-scenario-contracts.mjs`, `test-storage-projection-contracts.mjs`, `test-openapi-contracts.mjs`, `test-artifact-manifest-contracts.mjs`; использовать pinned Ajv 8.17.1 setup из CI.
- Обновить/добавить native-contract schema tests и frozen predecessor vectors вместе с successor, regenerate affected custody fixtures; изменения версии protocol требуют проверки Node adapter, не blanket replacement всех 0.2.16/0.3.2.
- Determinism: два fresh builds + два fresh generation roots, shuffled input file enumeration, different temp absolute paths, одинаковые manifest/source-map/OpenAPI/migration/test bytes; clean/regenerate и repeat apply без diff.
- Adversarial: malformed/oversized provider JSON, unsafe SARIF URI, duplicate/colliding PHP identifiers, changed Model/input/tool hashes, tampered generated file, symlink output, source path outside roots, missing tools, failing assertion и boot crash, no implicit install sentinel.
- Gate acceptance bundle: exact commit/toolchain hashes, strict conformance report, executed Planner/parity receipts, normalized diagnostics samples, deterministic artifact hashes, portability golden и unchanged canonical Model evidence. Ни один missing/skipped/unsupported gate не считать выполненным acceptance.

## 7. Риски, ambiguities и рекомендуемое разрешение

| Риск/неясность | Рекомендуемое решение |
|---|---|
| #54 зависит от будущих #55/#56 integrations | Сначала S1/S2 bootstrap и fake seams; затем реальные минимальные slices S5/S6, coordinated ownership. Завершать #54 только после integration evidence, даже если глубокие graph/test features остаются #55/#56. |
| Native gate contract пока Node-only | Versioned Composer successor до S7; production execution — отдельный явно названный gap, fixture proof допустим только как fixture proof. Если требуется live pilot execution внутри Lekalo, это дополнительная обязательная работа executor, не solved by plan. |
| PHP packaging и tool assets не помещаются в existing sandbox | Проверить single-file PHP first; для runtime DLL/config/tool assets нужен reviewed explicit bundle contract. Не расширять ambient filesystem/PATH или silently bypass TargetClient. |
| Mago diagnostics не переносятся через Finding | S1 neutral provider ingest + registered authority/privacy family; preserve original code/range; никакого JSON, спрятанного в `detail`. |
| Анализатор не выдаёт нужный symbol/reference format | Pin и capability probe вместе с #55; использовать supported process evidence, отказ/partial вместо собственного regex parser и fabricated confidence. |
| PHP generics/decimal/date/presence слабее Model | Runtime validators/value wrappers, shared boundary fixtures и явная portability partial/unsupported; не ослаблять domain contract. |
| Scaffolds против contracted maintained ownership | Templates только generated support, explicit one-time adoption; handlers SQL/services/custom tests never overwrite/clean. |
| Один «Planner» существует в нескольких fixtures | S6 выбирает executable Scenario IR + compiled IR из Node gate; contract slice отдельно проверяет ownership. Каждый receipt фиксирует input digests, не только имя Planner. |
| Static profile support обещает больше backend | Component catalogue не evidence; проверять actual tools/ports и dynamic result states, недоказанные возможности ограничивать portability report. |
| PHP fake/PHPUnit limitation изменилась upstream | Не копировать старое ограничение буквально: pinned shim smoke + explicit unsupported list; upgrade совместно с diagnostics/parity golden review. |
| Laravel global state и DB concurrency | Sequential fresh app, isolated PostgreSQL DB/reset, controlled clock/UUID; concurrent scenario не эмулировать serial test. |

Implementation commits: S1 contracts/ADR; S2 kernel/conformance; S3 types; S4 projections/ownership; S5 tool bridges; S6 Planner/parity; S7 native/CI. Каждый шаг завершается своим focused gate и conventional commit; integration failures не откладывать под общий «MVP done».
