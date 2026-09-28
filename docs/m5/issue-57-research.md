# Issue #57 — Laravel migrations из storage projection: research и implementation plan

## 1. Scope и evidence

- Authority: `gh issue view 57 --repo ichinya/lekalo`, прочитано первым, 2026-09-27; [issue](https://github.com/ichinya/lekalo/issues/57).
- Branch: `ichinya/m5-issue-57`, исходный HEAD `aca1bb8e6b3915b05213be6eba120c046cdcf29e`; рабочее дерево исходно чистое.
- Это research-only: изменён только этот документ; реализация, PHP/DB/Laratesto и acceptance suite не запускались.
- Scope включает все возможности и семь AC issue; dependencies: #18/#21/#24/#29/#54/#69, с интеграцией scenario backend #56.
- В этом checkout уже есть PHP protocol kernel #54. Старый `docs/m5/issue-54-research.md` описывает состояние до его реализации и не является текущим inventory.
- Рекомендуемый путь: использовать PostgreSQL projection/DDL/planner в core, а PHP adapter превращает проверенный план в immutable Laravel migration artifacts; не писать второй SQL planner на PHP.

## 2. Existing modules/contracts/tests

Пути от корня; `core/` означает `crates/lekalo-core/src/`. Символы ниже существуют, новые обозначены в §4.

| Область | Где смотреть и что использовать |
|---|---|
| Storage declaration | `core/storage_projection/{mod,entity,projection,wire,validate}.rs`; `StorageProjectionAttachment`, `DataRisk`; closed contract `contracts/storage-projection.schema.v0.4.0.json`. Domain entities и target storage facts разделены. |
| Derivation | `storage_projection/derivation.rs::project`, `DerivedColumn`, `DerivedProjection`; namespaces postgres/laravel/mysql/mariadb. Laravel namespace — данные, не PHP migration emitter. |
| Storage impact | `storage_projection/diff.rs::compare` даёт layer/class/risk per path; `plan.rs::migration_plan` — non-executable path plan, gates destructive **и backfill_required**. CLI `storage diff`, `storage plan`. |
| Semantic history | `core/diff/mod.rs::{compare,detect_renames}`, `diff/identity.rs::Subject`, `ir::RenameHistoryEntry`; stable symbol IDs, `renamed_from` и registry history. Нет готового переноса field/physical-column rename history в storage planner. |
| PostgreSQL profile | `core/storage_engine/{mod,wire,validate}.rs`, `postgres/{version_matrix,snapshot,types,quoting}.rs`; exact engine version, policies, #24 capabilities, production-forbidden test lifecycle. Не путать с MySQL-oriented `storage_engine_profile/`. |
| SQL | `storage_engine/postgres/ddl.rs::{render,DdlDocument,DdlStatement}`: typed defaults, indexes/predicates, foreign keys, sequences, checks/RLS. `storage_engine/input.rs::engine_input` связывает profile с projection и выдаёт именно PostgreSQL namespace для любого runtime. |
| Executable proposal | `storage_engine/migration.rs::{plan,MigrationPlan,Step}` / export `storage_engine::plan_migration`: SQL statement, kind, risk, requires, exact planId; gates только destructive. Table/join rename есть, `rename_column` нет. Нет rollback classification или custom-step reference. |
| Engine contracts | `contracts/{storage-engine,storage-observation,storage-migration-plan}.schema.v0.4.0.json`; plan schema допускает `introspectionDigest`, но Rust `MigrationPlan` его не хранит/не выпускает. Observed schema и migration ledger — разные evidence. |
| Checked schema | `storage_engine/{introspection,drift}.rs::compare_drift`, CLI `storage drift`; missing/extra/divergent/unsupported, extensions, columns/defaults/constraints/indexes. Drift verdict — данные, сам по себе не generation blocker. |
| PHP adapter | `adapters/php-laravel/src/kernel.php::{deterministic_generation,generate_response,apply_writes,plan_clean_response,clean_response,describe_capabilities}`; сейчас один support `kernel.php`, не migration generator. `plan_native_response` unsupported. |
| Packaging/security | `adapters/php-laravel/{build.php,adapter.php,adapter.manifest.json}`: single-file artifact, PHP >=8.3, read `.lekalo/{ir,cache}/**`, write `.lekalo/generated/php-laravel/**`, no children/network. Protocol 0.3.2, IR 0.2.16; нельзя молча подменить IR storage JSON. |
| Ownership | `core/artifacts/{mod,clean}.rs`, `GenerateService::{check,clean_plan,clean_apply}`, `core/{contracted,observed,bindings}/`; hash/lock custody есть, applied-migration semantics нет. Contracted support только `.lekalo/generated/**`. |
| Native/target seams | `core/target_protocol/`, `core/native_gate/`, `core/target_profile/component.rs`; Node `postgres-storage-extension.mjs::createPostgresStorageExtension` только injected apply seam, не Laravel runner и не достаточная проверка approval. |
| Existing tests | `crates/lekalo-core/tests/{storage_projection,storage_engine,storage_engine_profile,diff,php_laravel_kernel}.rs`; `crates/lekalo-cli/tests/{storage,diff}.rs`; PHP `tests/{protocol,process}.php`. |
| Existing gates | `scripts/test-{storage-projection,storage-engine,semantic-diff}-contracts.mjs`, `test-php-laravel-adapter.mjs`, `test-adapter-manifest-golden.mjs`, `.github/workflows/ci.yml`. Ajv contract scripts требуют 8.17.1 вне checkout. |
| Fixtures | `tests/fixtures/storage-projection/{valid/planner-storage.json,derived/laravel.json,diff/}`, `storage-engine/{valid,derived,runtimes/php.json}`, `invariant-transition/valid/planner-invariants.json`, `transaction-concurrency/valid/planner-focus-race.json`. PHP runtime JSON — не выполненная migration. |

Полезные regression anchors в `tests/storage_engine.rs`: `an_additive_change_plans_ready_without_destructive_steps`,
`an_added_not_null_column_plans_nullable_backfill_then_the_constraint`, `a_renamed_table_keeps_its_column_diff`,
`a_renamed_join_renames_and_rederives_its_foreign_keys`, `a_referenced_pk_swap_takes_its_dependent_keys_down_and_back`,
`a_destructive_plan_is_gated_and_blocked_until_the_plan_id_is_named`.

## 3. Gap analysis: каждый acceptance criterion

| AC | Текущее состояние / недостающее доказательство |
|---|---|
| 1. Planner schema → valid Laravel migration | Core SQL и PHP kernel есть; отсутствуют migration emitter, Composer-installed Planner fixture и выполненный up. Нужны PHP lint + Laravel boot + PostgreSQL schema equality. |
| 2. Unique one-focus корректна либо unsupported | PostgreSQL renderer умеет partial unique index; существующий Planner storage fixture содержит partial **non-unique** index due_date, не one-focus. Invariant `planner.invariant.focus_one_active`: максимум один focused task **per user**, predicate focused_at not_null; нужен явный storage mapping или typed unsupported. |
| 3. Additive/destructive разные gates | Два core plan API уже различают риски, но backfill gate расходится; adapter пока не consumes их. Нужны единая generation policy, bound approval, stale-plan rejection и нулевые writes до разрешения. |
| 4. Applied migration не меняется | Generic manifest/hash custody есть; нет applied ledger, append-only migration ownership и защиты от clean. Нужны ledger + file digest evidence, отказ даже при generic clean approval. |
| 5. Rename history предотвращает loss | Model symbol rename поддержан; engine table rename определяется stable entity key и физическим именем, column diff name-based. Нужен отдельный validated storage rename mapping и populated-DB preservation test; similarity не evidence. |
| 6. Fixture DB и Laratesto scenarios | Есть declarative storage/conformance tests и PHP kernel suite; нет установленного Laravel Planner DB fixture/этих сценариев. Нужен реальный PostgreSQL + Laratesto run, без зачёта skip/unsupported как pass. |
| 7. Semantic diff показывает storage impact до generation | `storage diff` уже показывает риски; основной Model diff не заменяет storage comparison. Нужен pre-generation report с обоими digests, rename/custom/rollback consequences и gate decisions; dry-run не пишет и не выполняет SQL. |

## 4. Concrete implementation plan (последовательные небольшие commits)

Все новые имена ниже — предлагаемый API, не доступные сейчас команды. Первая реализация: S1 contract/input custody,
затем S2 rename/policy, S3 emitter, S4 checked/immutability, S5 integration; не создавать второй PHP kernel.

### S1 — Bound migration input и version decision

- Добавить ADR `docs/adr/<allocated>-laravel-storage-migrations.md` и `docs/laravel-migrations.md`: SQL-backed Laravel migrations,
  runtime=php-laravel/storage=postgres-sql, PostgreSQL-first support matrix, generation != DB execution.
- Новый `core/storage_engine/laravel_input.rs::build_laravel_input` собирает validated base/candidate projection, engine profile,
  semantic/history evidence, storage diff, forward plan/initial DDL и target/profile/lock provenance в bounded canonical document.
- Опубликовать новый closed schema `contracts/laravel-migration-input.schema.v<release>.json` и output schema
  `contracts/laravel-migration-artifacts.schema.v<release>.json`; sync Rust/PHP validators, unknown-member/size/depth tests.
- Input pins: base/candidate/model/IR/profile/lock/history/observation/ledger/custom-step digests, exact PostgreSQL version,
  adapter/generator version, timestamp policy, ordered operations, effective gate, rollback classification. Нет credentials/URLs.
- Размещать input как explicit digest-bound sidecar под `.lekalo/ir/**`, approved observation/ledger snapshot под `.lekalo/cache/**`.
  Добавить typed discovery/reference в existing generation input pipeline и CLI; не искать произвольные соседние файлы и не класть JSON в `Finding.detail`.
- Прочитать request через production TargetClient: если existing envelope не выражает sidecar reference, согласованно version
  protocol family + validators/fixtures/negotiation; не переобъявлять storage input как `dev.lekalo.ir@0.2.16`.
- Добавить CLI `storage laravel-plan BASE CANDIDATE --profile PATH --history PATH --ledger PATH --timestamp-base VALUE`
  (новый surface в `crates/lekalo-cli/src/`, отдельный module `storage_laravel.rs`), `--json`, `--confirm PLAN_ID`, checked observation input.
  Для initial schema — отдельный `--initial` mode через `ddl::render`, не фиктивный пустой invalid Model.
- Версию назначить по текущей release policy через `scripts/check-contract-versions.mjs`; изменяемые published families обновлять согласованно.

### S2 — Rename, effective policy, dependencies и rollback

- Новый `core/storage_engine/history.rs::{StorageRenameMap,validate_rename_history}` и versioned history schema:
  project/base/candidate pins, entity/member identity, namespace, old/new physical name, semantic history reference.
- Переиспользовать validated symbol history `diff::compare`; переименование domain symbol само по себе **не** переименовывает таблицу.
  Для fields, не имеющих stable member ID/history сегодня, требовать explicit reviewed storage mapping; не выводить mapping из одинаковых типов.
- В `migration.rs` добавить `plan_with_history`, сохранив existing `plan` для callers без history; matching columns после validation
  генерирует `ALTER TABLE ... RENAME COLUMN`, далее type/null/default diff уже по новому имени. Добавить closed step kind/schema support.
- Проверять bijection, old exists/new unoccupied, project/pins, chains, conflicts/cycles; ambiguous history блокирует generation.
  Rename + type change/FK/index/default/sequence/predicate change должен сохранять все операции и корректные references.
  Без evidence remove+add остаётся явным destructive proposal и никогда не проходит как additive rename.
- Effective gate — union storage semantic risk и SQL step risk: destructive **и backfill_required** требуют exact bound plan approval.
  Existing `migration::plan` ставит gate только destructive; не использовать его `ready` как универсальное разрешение Laravel generation.
- Plan identity включает immutable inputs/operations/custom content/timestamp/ledger/observation; acknowledgement/status не включать
  в self-referential hash. Проверять approval при writes; изменение любого input отменяет approval, engine confirmation не заменяет target confirmation.
- Не наследовать автоматически zero-value backfill planner: для бизнес-значений требовать declared default или custom-step reference
  с reviewed digest, pre/postconditions и rollback policy. Планировать nullable add → backfill → NOT NULL; отсутствующий backfill блокирует.
- Новый `rollback.rs::{classify,build_reverse_plan}`: reversible schema, data-loss-on-rollback, custom-required, irreversible.
  Не строить down простым reverse строк или `plan(candidate,base)`: recreating column не возвращает данные. Для unsafe down — fail-before-any-DDL;
  reversible down выполняет обратную dependency order; create-table up имеет data-loss down и требует явного разрешения rollback execution.
- Расширить Step typed operation metadata там, где нужны inverse/mapping; SQL не парсить regex. Topological sort `requires` со stable tie-break,
  проверки missing edges/cycles; FK циклы разрешать create tables → add FK, reverse — drop FK → drop tables.

### S3 — PHP Laravel emitter и deterministic IDs

- Добавить `adapters/php-laravel/src/{storage_input,migration_emit,migration_ledger}.php`, функции
  `decode_storage_input`, `emit_laravel_migrations`, `assert_append_only`; подключить в `kernel.php::deterministic_generation`/`generate_response`.
- `build.php` расширить fixed-order concatenation; обновить shipped `adapter.php`, package hashes/manifest и `describe_capabilities`.
  Заявлять storage capabilities только для реализованной subset; preserve all existing protocol/confinement tests.
- Предпочтительно anonymous class extends `Migration`, strict types, `up(): void` с последовательными `DB::statement` из проверенного
  core SQL; `down(): void` из typed reverse plan либо явный refusal до изменения DB. Это valid Laravel migration, Blueprint не обязателен issue.
- Такой backend сохраняет PostgreSQL precision/null/default semantics, quoted identifiers, partial indexes, sequence/RLS и timestamp timezone,
  вместо ошибочного преобразования `laravel datetime` в PostgreSQL instant. Не смешивать postgres/laravel namespace table names.
- Если позже нужен Blueprint emitter: explicit names для PK/unique/index/FK, точные modifiers при `change`, `renameColumn`,
  jsonb/timestampTz/decimal precision; неподдерживаемое mapping помечать unsupported, не использовать framework defaults.
- PHP literal escaping должен сохранять SQL bytes (quotes, backslashes, dollar signs, Unicode); no eval/raw user PHP/raw Model SQL.
  Custom steps — allowlisted reference + content digest to maintained implementation; не вставлять arbitrary source в neutral Model.
- Не вызывать `make:migration` (clock timestamp). Filename: declared UTC base + append-only ordinal → Laravel timestamp prefix,
  stable sequence/short plan digest suffix; full digest в ledger. Проверять timestamp monotonicity/collision against existing names.
- Один migration batch на bound transition; ordinal/filename закрепляется ledger и не пересчитывается при regenerating history.
  Два чистых запуска с одинаковыми inputs дают одинаковые bytes/path/manifest; no-op повтор не создаёт файл.
- Contracted output `.lekalo/generated/php-laravel/<target>/migrations/`; maintained fixture provider регистрирует этот путь через Laravel
  `loadMigrationsFrom`. Не расширять write scope на `database/migrations/**` автоматически; existing application migrations только checked.
- `generate` всегда только artifacts: нет artisan/subprocess/network/DB side effects; SQL dry-run report доступен до approval.

### S4 — Applied immutability и checked mode

- Новый ledger contract: target/profile/database-scope token, ordered migration IDs/filenames/content hashes, plan/source digests,
  published/applied evidence и custom-step digests. DB migration table даёт names/batches, но не original file hashes: нужен отдельный immutable ledger.
- Самый безопасный default: **все опубликованные** migrations append-only, не только applied; исправление только новым forward migration.
  Byte-identical regenerate — no-op; changed/missing published file, unknown applied ID, same filename/different plan — отказ.
- External fixture/checked scanner читает read-only Laravel migrations table + schema observation; связывает evidence с opaque connection,
  exact server version, profile/base и scope. Missing/stale/incomplete observation => checked unknown/blocked, не empty database.
- До publication проверить core `compare_drift`, ledger/file digest consistency и partial-index predicate equivalence; unknown/manual migration
  не исполнять для анализа. Existing custom/scaffolded/checked файлы никогда не перезаписывать.
- Изменить `core/artifacts/clean.rs` и PHP clean planning так, чтобы published migration/ledger не удалялся как orphan даже с generic confirmation.
  Нужен explicit artifact retention metadata/version decision или отдельный retained migration manifest; path-name heuristic недостаточен.
- Apply выполняется в staged view с repeat custody validation, atomic publication manifest+files/ledger и recovery от interrupted write;
  конкурентный generator со stale ledger теряет authority. Checked generation не соединяется с production автоматически.

### S5 — Planner fixture, scenarios и CI

- Добавить `tests/fixtures/laravel-migrations/{inputs,golden,invalid}/` и установленное приложение `tests/fixtures/laravel-migrations/app/`
  с `composer.json`, exact `composer.lock`, migrations provider, Testo/Laratesto config и maintained Planner persistence handlers.
- Baseline: Laravel 13/PHP >=8.3 (совместим с kernel minimum); конкретные framework/Laratesto/PostgreSQL image versions и digest
  зафиксировать при реализации, не считать старый fixture engineVersion 16.4.0 автоматически версией нового сервера.
- Fixture projection обязана согласовать real Model/IR/invariant IDs: existing storage и invariant examples используют разные subject IDs.
  Добавить user_id NOT NULL, focused_at nullable и declared partial unique index on user_id WHERE focused_at IS NOT NULL;
  это per-user one-focus, не UNIQUE(task_id) и не global UNIQUE(focused_at). Если mapping нельзя доказать — AC2 explicit unsupported.
- Проверить timestamps/decimal/default/null/PK/unique/index/FK/join tables, а также explicit optimistic version column + CAS update scenario;
  наличие `row_etag` или profile declaration само по себе не доказывает concurrency behavior.
- `app/tests/MigrationScenarios.php`: fresh up/schema comparison, populated additive, rename with sentinel rows, destructive refusal/approved proposal,
  same-user focus conflict, different-user success, two-connection race, stale version CAS, FK delete behavior, relation-table uniqueness,
  safe down/up и refused irreversible down. Sequential test runner может запускать внутри одного race case две DB connections.
- `scripts/test-php-laravel-migrations.mjs` (новый) orchestration: disposable PostgreSQL/schema, verified test-only credentials outside reports,
  migrate/seed/scenarios/introspection/cleanup; positive evidence server version + input/artifact digests + discovered/executed assertion counts.
- Добавить CI job в `.github/workflows/ci.yml`: locked Composer install как отдельный setup, Linux PostgreSQL service, PHP lint,
  Laratesto + contract/golden/immutability gates; сохранить Windows/Linux deterministic packaging tests. Production DB execution запрещён.
- Fixture runner не должен обходить production native-gate restrictions и заявлять production execution support: #54 `plan-native` unsupported,
  core production native runner не реализует Composer. Согласовать #56 execution/evidence seam; отсутствующий runtime/dependency = blocked, не pass.

## 5. Test strategy и команды implementer

Existing focused regressions (в research не запускались):

```powershell
cargo test -p lekalo-core --locked --test storage_projection --test storage_engine --test diff
cargo test -p lekalo-cli --locked --test storage --test diff
cargo test -p lekalo-core --locked --test php_laravel_kernel
node scripts/test-storage-projection-contracts.mjs
node scripts/test-storage-engine-contracts.mjs
node scripts/test-semantic-diff-contracts.mjs
php adapters/php-laravel/build.php --check
node scripts/test-php-laravel-adapter.mjs
node scripts/test-adapter-manifest-golden.mjs
```

Ajv 8.17.1 provisioning/NODE_PATH брать из CI; не добавлять npm install artifacts в корень. Rust build output при необходимости вне worktree.
New tests: `crates/lekalo-core/tests/laravel_migrations.rs`, `crates/lekalo-cli/tests/laravel_migrations.rs`,
`adapters/php-laravel/tests/migrations.php`, `scripts/test-laravel-migration-contracts.mjs`, `scripts/test-php-laravel-migrations.mjs`.

| Gate | Минимальные vectors и assertions |
|---|---|
| Contracts/input | Unknown nested fields, mismatched project/model/IR/profile/engine pin, untrusted SQL/custom source, wrong digests, excessive sizes; no read/write outside scopes. |
| Rename | Valid direct/chain; missing/conflicting/cyclic history; rename+type/FK/index change; stable symbol rename without physical rename; populated rows unchanged. |
| Safety | Add nullable allowed; drop/type narrowing/backfill blocked; exact approval accepted, stale approval rejected; blocked generation writes zero bytes. |
| Mapping | All scalar/default/precision/null variants; bytea/UUID/JSONB/enum/array capability refusal; named keys/indexes/checks/FK cycles/joins, timestamp/version semantics. |
| Custody | Re-run no-op, applied file tamper/delete, unknown applied migration, timestamp collision, ledger race, interrupted publication, symlink/traversal, clean cannot delete history. |
| DB/rollback | Fresh/populated up; actual catalog equals projection; data preservation on rename; reverse dependency order; irreversible down refuses before first statement. |
| Determinism | Two roots, reordered JSON keys, timezone/clock changes give equal bytes; report pins all inputs; change input invalidates plan/approval. |

Fixture commands after S5, from `tests/fixtures/laravel-migrations/app` with isolated test environment:

```text
composer install --no-interaction --prefer-dist
php artisan migrate --pretend
php artisan migrate
php artisan laratesto:test --testo-only
```

Laratesto command/version must be confirmed against the chosen locked package before CI publication; existing #54 research records this command,
but there is no installed fixture in this checkout. New Node harness executes equivalent commands and asserts nonzero scenario discovery.
`migrate --pretend` is a fixture SQL preview, not offline generation proof or permission to connect to production.
Final implementation gates: above focused suites, new real-DB suite, `cargo test --workspace --locked`, contract-version check and `git diff --check`.
Report separately protocol pass/platform skip, semantic tests, actual DB scenarios, unsupported strategies; issue acceptance requires executed DB evidence.

## 6. Risks / recommended resolutions

- **Scope creep:** full ORM/DTO/Mago/backend implementation belongs to siblings; #57 owns migration artifacts, safety/evidence and minimal executable scenarios.
- **Namespace ambiguity:** PHP runtime does not imply Laravel storage namespace; use declared PostgreSQL physical projection consistently, fail on conflicting bindings.
- **Rename under-specified:** field names are not stable IDs; version a storage history attachment instead of adding Laravel syntax to Core Model.
- **Approval vs backfill:** engine ready is weaker than issue safety needs; effective Laravel gate covers both destructive and backfill, no implicit zero business data.
- **Rollback:** schema reversibility is not data recovery; classify separately and require custom recovery reference when needed.
- **Checked provenance:** hashes alone cannot prove a live production schema; use bounded trusted observation and record freshness/custody, otherwise block checked verdict.
- **Partial unique/CAS:** explicit partition/predicate plus DB race proof; unsupported is honest AC2 outcome, but must be visible in capability/portability report.
- **Confinement:** single-file PHP distribution must remain runnable without sibling/vendor includes; Windows copied PHP DLL limitation is existing, not DB acceptance.
- **Framework API:** Context7 `/websites/laravel_13_x` confirms anonymous migrations, `up/down`, `renameColumn`, explicit FK naming and `migrate --pretend`:
  [Laravel 13 migrations](https://laravel.com/docs/13.x/migrations). SQL-backed emitter avoids accidental Schema Builder type/default changes.
  Official API additionally confirms [loadMigrationsFrom](https://api.laravel.com/docs/13.x/Illuminate/Support/ServiceProvider.html#method_loadMigrationsFrom)
  and [DB::statement](https://laravel.com/docs/13.x/database#running-a-general-statement); publishing via `publishesMigrations` changes timestamps,
  so register the generated directory directly instead of republishing immutable filenames.
- **Unproven runtime:** current Rust SQL tests and PHP kernel CI do not establish real PostgreSQL/Laratesto conformance; S5 is mandatory, not a follow-up after acceptance.

## 7. Research delivery validation

Only this file is intended to change; validate line count <=300 and `git diff --check`, then commit `docs(m5): research Laravel migration generation for issue 57`.
No implementation or runtime acceptance is claimed by this research commit.
