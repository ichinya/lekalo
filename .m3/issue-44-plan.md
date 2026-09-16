# Issue #44 — план TypeScript symbol scanner и semantic bindings

## Статус исследования и решение

Дата: 2026-09-15. Исследован checkout `ichinya/M3`, HEAD `1eb3d30` (merge #43), tag `v0.3.0`; исходный `git status --short` показывал только существующий untracked каталог `.m3/`. Прочитаны live [#44](https://github.com/ichinya/lekalo/issues/44) и закрытый [#42](https://github.com/ichinya/lekalo/issues/42), [план #43](issue-43-plan.md), accepted review из sibling worktree [`m3-issue-43/.m3/issue-43-review.md`](../../m3-issue-43/.m3/issue-43-review.md), kernel, consumers, contracts, fixtures и release reserve. Этот документ — единственное изменение данного исследования; implementation, reserve, commit, branch и contract edits не выполнялись.

**Рекомендуемый технический путь:** новый adapter-local build package с закреплённым TypeScript compiler API; детерминированный committed single-file `adapter.mjs` с встроенными compiler code и standard declarations; scanner через существующий `createKernel` registry; read-only host, стабильный внутренний индекс, явная uncertainty и ограниченная проекция в существующий scan.

**Ограничение завершения #44:** при неизменных contracts `0.2.16` нельзя честно закрыть все acceptance criteria через текущую CLI-цепочку. `observed-scan` уже умеет `evidence.signature` и `evidence.references`, но процессный scan и его consumer не передают их; новые references не попадут в общий dependency graph. Internal retention достаточен для source spans/JSDoc/детального provenance, но не заменяет AC5 «Imports/references дополняют dependency graph». Ниже дан исполнимый план совместимой части и минимальное отдельное изменение транспорта для полного завершения. При жёстком freeze contracts версия продукта остаётся планируемой `0.3.1`, AC5 остаётся открытым; не обходить это расширением JSON-in-detail под прежней версией.

### Краткое резюме для координатора — 10 строк

1. Резервировать product/adapter `0.3.1`; неизменённые contracts и resolver оставить `0.2.16`.
2. Закрепить TypeScript `5.9.3` и esbuild `0.25.12`; выпускать один bundle с compiler и lib declarations.
3. Не использовать TypeScript из проекта, `NODE_PATH`, sibling runtime imports или package-manager execution при scan.
4. Перед подключением scanner исправить F1 и F2–F8; request resolution сверять с независимым trusted snapshot.
5. Передавать root-bearing profile как ограниченный явный launch input; wire profile не является источником root authority.
6. Индексировать через Program/TypeChecker; aliases объединять по declaration identity, overload sets хранить целиком.
7. Stable native ID отделить от signature digest, source location, export aliases и compiler object IDs.
8. Uncertainty, framework provenance и полные spans сохранять внутри; неполную wire-проекцию отклонять.
9. Для AC5 нужен отдельный транспортный путь к уже существующему observed evidence; contracts freeze не позволяет назвать #44 полностью готовым.
10. При scan-only ожидаются default 7 pass/11 skip и strict 7 pass/10 skip/1 fail; нужны собственные semantic/integration probes.

## 1. Compiler API, упаковка и фактический запуск

### Проверенная граница TargetClient

Источники: [`target_protocol/confinement.rs`](../crates/lekalo-core/src/target_protocol/confinement.rs), `Sandbox::new`, `command`, `run_linux`, `run_macos`; [`transport.rs`](../crates/lekalo-core/src/target_protocol/transport.rs), `run_private`/`run_impl`; [`confinement_windows.rs`](../crates/lekalo-core/src/target_protocol/confinement_windows.rs); [`target_protocol/mod.rs`](../crates/lekalo-core/src/target_protocol/mod.rs), describe/call; [`docs/target-protocol.md`](../docs/target-protocol.md).

- Describe стартует с пустым private project view; нельзя во время handshake читать package.json/tsconfig, чтобы решить, какие roots объявить.
- Core разрешает executable на стороне родителя, копирует Node в private runtime и копирует **только первый аргумент**, если это существующий script file. Остальные argv передаются, но не дают файловых разрешений. Поэтому entry должен оставаться первым аргументом `AdapterCommand.args`.
- Production vector: `program: node`, `args: [absolute_adapter_mjs, ...explicit_adapter_options]`; документированный базовый запуск остаётся `node adapters/node-typescript/adapter.mjs`. Core сам добавляет Node `--preserve-symlinks`/`--preserve-symlinks-main` после копирования, а для file transport — `--lekalo-request-file` с private runtime path.
- `cwd` child — private `project`, `request.project_root` — `.`. Источники туда копируются только по разрешённым `read_scopes`; это не cwd исходного checkout.
- Linux/macOS используют очищенную среду (`env_clear`; Linux также bwrap `--clearenv`). Windows передаёт только `APPDATA`, `LOCALAPPDATA`, `SystemDrive`, `SystemRoot`, `TEMP`, `TMP`, `USERPROFILE`, `windir`, причём пользовательские директории указывают на private root. `NODE_PATH`, `NODE_OPTIONS`, пользовательские package stores и исходный PATH не являются доступным runtime input.
- Staging ограничен суммарно 64 MiB, отдельный file read — 4 MiB; request 1 MiB, response 8 MiB. Размер bundle отдельно проверить в confinement: копирование script не является переносом package directory.

### Выбор зависимости

| Вариант | Решение и причина |
| --- | --- |
| Peer dependency: target project's `typescript` | Отклонить: package не переносится в runtime, версия меняется от проекта, pnpm links конфликтуют с no-follow политикой, загрузка project dependency выполняет недоверенный JS. |
| Новый обычный runtime `import 'typescript'` | Отклонить для release artifact: Node ищет package относительно скопированного script; sibling `node_modules` отсутствует. |
| Сырой compiler source, вручную вставленный в kernel | Возможен, но усложняет обновления и custody; не выбирать ручное редактирование upstream code. |
| **Build dependency + vendored single-file output** | **Выбрать:** точный package lock, compiler и lib data входят в committed bundle; deployment/runtime не требуют npm или project install. |

Закрепить `typescript: 5.9.3` и `esbuild: 0.25.12` без `^`/`~`. Это deliberate compatibility baseline для классического JS Compiler API и Node 18/24, не утверждение о latest. У TypeScript 5.9.3 entry `lib/typescript.js`, engine Node `>=14.17`; у esbuild 0.25.12 — Node `>=18`. Проверено по [TS package metadata](https://raw.githubusercontent.com/microsoft/TypeScript/v5.9.3/package.json) и [esbuild metadata](https://raw.githubusercontent.com/evanw/esbuild/v0.25.12/npm/esbuild/package.json). Текущая [официальная Compiler API guide](https://github.com/microsoft/TypeScript/wiki/Using-the-Compiler-API) предупреждает об изменении API в 7.1; результаты Context7 по `main` нельзя механически переносить на выбранный pin.

### Предлагаемый layout (создавать только при implementation)

```text
adapters/node-typescript/
  package.json                 private build package, exact devDependencies
  package-lock.json            exact compiler/bundler/platform optional dependency custody
  build.mjs                    deterministic bundle + --check, no fixture commands
  src/kernel.mjs               extracted #43 kernel, preserved exported test APIs
  src/main.mjs                 strict argv/transport, trusted launch profile, registry
  src/scanner.mjs              Program index and internal incremental session
  src/compiler-host.mjs        restricted VFS, resolution, embedded standard libs
  src/symbol-identity.mjs      canonical identity/signature/alias graph
  src/semantic-bindings.mjs    proposed IR compatibility + conservative projection
  adapter.mjs                  committed generated single-file runnable artifact
  THIRD_PARTY_NOTICES.md       compiler/bundler provenance and licenses
  test/*.test.mjs              existing suites + scanner/host/bindings/bundle suites
scripts/test-node-typescript-scanner.mjs
scripts/reserve-031-regen.mjs
tests/fixtures/node-typescript-scanner/**
crates/lekalo-core/tests/node_typescript_scanner.rs
```

`build.mjs`: esbuild API `bundle:true`, `platform:'node'`, `format:'esm'`, `target:'node18'`, `splitting:false`, `sourcemap:false`, fixed working directory and source order. Не externalize `typescript`. Встроить все необходимые `lib*.d.ts` из **того же** pin как sorted string map virtual module; TS default-lib и transitive `/// <reference lib>` читаются из этой map, не из filesystem. Legal notices включить и в bundle comment, чтобы single-file distribution сохранила attribution. Не включать абсолютные build paths, timestamps и nondeterministic metadata.

CJS compatibility upstream compiler требует отдельного smoke: при нужде ESM banner с `createRequire(import.meta.url)` для известных Node built-ins и module-local `__filename`/`__dirname` compatibility shim. Запретить внешнюю package fallback загрузку, включая optional `source-map-support`; unused optional modules отклонять/отключать явно на build seam. Проверять фактический import/require graph и startup side effects выбранного compiler. `createRequire` здесь не разрешение искать проектные packages. Kernel остаётся логически на built-ins; **целый scanner bundle уже содержит third-party compiler**, README «zero runtime dependencies» уточнить как «no external runtime installation».

Rebuild в двух разных внешних checkout/temp paths должен дать одинаковые bytes/hash. `build.mjs --check` собирает в памяти и сравнивает committed artifact, не переписывает его. Bundle relocation test копирует **только** `adapter.mjs`, запускает Node без соседних assets/NODE_PATH и проверяет primitive/lib types, JSX, imports и реальные symbols. Identity digest — hash запущенного bundle, а не `src/main.mjs`, compiler version отдельно внутри metadata. Исправить `entryPath()` для import tests: `process.argv[1]` может быть test runner; модульный URL — источник идентичности artifact.

## 2. Root authority и точная extension seam

### Что есть сейчас

[`adapter.mjs`](../adapters/node-typescript/adapter.mjs) `main()` вызывает пустой `createKernel()`. `ResolvedProjectProfile` принимается только in-process и имеет закрытые поля `id,mode,target,readRoots,exclusions,targetResolution,provenance,localReference`. Root — `{kind:'file'|'tree',path}`, validator вычисляет `scope`; `targetResolution` — `{digest,capabilities:[{id,support}]}`. Это отличается от core `ResolvedProfile`, где read roots вообще нет. [`scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs) сегодня посылает `profile_resolution: None` и `ir_path: None`.

### Предлагаемый production bootstrap без новых wire members

Добавить adapter option `--lekalo-project-profile-json <compact-json>` **после entry**. Это новый bounded explicit launch input от владельца `AdapterCommand`, не поле request и не чтение пути из env/project. Deserialize через тот же strict decoder, максимум 16 KiB JSON (с учётом Windows argv лимита), закрытая shape и immutable копия. Не поддерживать JS-module/plugin path. Только `ResolvedProjectProfile` и минимальная отдельная launcher configuration для списка tsconfig entrypoints/явных semantic mapping inputs, если они потребуются; не прятать машинную конфигурацию в `localReference`.

1. Trusted caller заранее выбирает roots и profile; describe может проверить их **лексически**, объявить scope/profile и registry без source reads. Physical checks выполняются при operation, когда private view уже материализован.
2. Без этого input bare entry продолжает безопасно описывать kernel-only surface. Не изобретать `src/**`/`packages/**` fallback. Поэтому **scan-enabled conformance и CLI примеры обязаны передать этот option**; bare команда сама по себе не является доказательством scanner.
3. Один launch input — один profile. `describe.profiles` должен стать `[profile.id]` вместо константного `standalone`; `targets` соответствует `profile.target`. `read_scopes` — canonical unique scopes из trusted roots, максимум wire 64; `write_scopes:[]`, `progress:false`.
4. Для snapshot-bound profile core orchestration/caller обязан передать уже полученный `wire::ProfileResolution` в существующий `CallRequest.profile_resolution`; расширить внутренний `ScanRequest` optional trusted resolution, где этот источник реально имеется. Не вычислять «trusted» snapshot из request и не копировать digest из входного wire в profile. Existing `lekalo scan` без resolved source работает только с unbound profile. Это осознанная граница UX, не скрытое обходное поведение F1.
5. Scanner dispatch требует явного `request.profile === trustedProfile.id`, в том числе для unbound standalone; CLI использует `lekalo scan --profile standalone` при таком id. Отсутствующий profile — structured refusal, хотя общий wire scan разрешает его отсутствие. При `targetResolution` дополнительно обязательны обе resolution части. Request.target при наличии равен `node-typescript`; bind, если когда-либо включён, всегда требует target/profile. Это не меняет generic wire grammar, а задаёт precondition конкретной реализации.

**Read-scope caveat:** core fingerprint/staging читает source bytes раньше extension. Kernel exclusions не исключают эти bytes из broad `packages/**` snapshot. Для #44 deterministic fixtures передавать точные source/config file roots (≤64), исключив vendor/generated/node_modules до выдачи scopes. Для larger project допустимы только проверенные отдельные source trees, не содержащие исключённых/чувствительных descendants; иначе fail closed. Автоматическая безопасная подготовка больших manifests — отдельный trusted host resolver/internal read-policy integration: перечислять paths без content reads, вычесть exclusions **до** snapshot/copy и использовать тот же narrowed policy в request identity и after-check. Не обещать «никогда не читает excluded» при одной лишь scanner-side фильтрации. Не менять общий TargetClient так, чтобы он стал угадывать TS layout.

### Регистрация

```javascript
// Proposed source wiring; descriptor fields already exist in #43.
const scanner = {
  id: 'typescript-symbol-scanner',
  version: '0.3.1',
  operations: ['scan'],
  namedCapabilities: { 'scan.symbols': 'full' },
  acceptedIrVersions: ['0.2.16'],
  invoke: context => scanProject(context),
};
const kernel = createKernel({
  resolvedProjectProfile: trustedProfile,
  extensionRegistry: [scanner],
  localEvidenceSink,
});
```

`invoke` сейчас **синхронный**; не вернуть Promise в существующий `normalizeExtensionOutcome`. `context` ровно `{operation,request,profile,readView,cancellation,limits}`. `scan.symbols:full` означает complete enumeration в объявленном module/entity scope, не доказанную semantic compatibility любого TS pattern; при неполноте request завершается error. Пока scanner покрывает только часть заявленного scope, объявлять `partial` и учитывать, что текущий strict selection `lekalo scan` отвергнет adapter. Не объявлять `full` только ради selection.

**Bind не регистрировать в #44 базовой реализации.** Wire `result.bindings` — только `{module,target,profile}`; это target binding, не native semantic mapping. `lekalo bindings propose/confirm/audit` работает с observed registry после scan и не вызывает adapter `bind`. Native mapping/signature algorithm — внутренний helper scanner. Не возвращать fake bind success. Позднее полноценный `bind` должен иметь собственный operation-aware projector; сегодняшний `projectResult` умеет только entries.

### Restricted CompilerHost

Сегодня `createReadView` даёт `roots`, `canRead`, `readFile`, `counters`; **нет directory enumeration/stat API**, поэтому обычный `ts.sys`/default `createCompilerHost` использовать нельзя. Добавить bounded `listDirectory`, `fileExists`, `directoryExists`, logical `realpath`/canonical mapping; каждое действие проходит scope/exclusion/component checks. Missing allowed path отличать от denied path: TS resolver может пробовать соседние расширения, но denied/config dependency нельзя молча превращать в «не существует» без uncertainty.

Построить VFS inventory из разрешённых paths; compiler видит стабильный virtual root, а не host/temp path. Config parsing (`readConfigFile`, `parseJsonConfigFileContent`) и `resolveModuleName`/`resolveTypeReferenceDirective` получают только этот host. Реализовать compiler `getSourceFile`, `readFile`, `fileExists`, `directoryExists`, `getDirectories`, `readDirectory`, `getCurrentDirectory`, `getCanonicalFileName`, `useCaseSensitiveFileNames`, `getNewLine`, `getDefaultLibFileName`, resolution hooks. Все standard libs — отдельная read-only virtual asset namespace; источники — readView. Filesystem `writeFile`, emit на диск, build/watch/install hooks запрещены; единственное исключение для declaration emit — ограниченный in-memory callback из §5. Не вызывать default host fallback через spread, оставив забытый `ts.sys.readDirectory`.

Read budget принадлежит kernel и не может увеличиваться extension через аргумент `readFile`. Content cache считает каждый actual read, а не повторный compiler lookup; file cap 4096/bytes 4 MiB — начальный bound #43, для embedded libraries отдельный заранее известный budget. File bytes читать bounded до allocation; UTF-8 — fatal. Enumeration/file size/depth/diagnostics/total output тоже ограничены. Передача cancellation проверяется между traversal/typecheck units; для синхронного compiler core timeout + kill остаётся жёсткой внешней границей.

## 3. Обязательные исправления accepted review F1–F8

Все исправления — **до первого production scanner dispatch**. Не изменять #43 accepted verdict задним числом.

| Finding | Изменение | Обязательное доказательство |
| --- | --- | --- |
| **F1 major** | Единый pre-dispatch `validateProfileBinding(request,profile)`: обязательное profile id equality и target equality при наличии target; при trusted targetResolution exact digest equality и canonical deep equality всех sorted capability `{id,support}`; нельзя принять subset/superset/другой support. Если snapshot есть, а pair отсутствует — refusal; если pair есть, а trusted snapshot отсутствует — refusal. Unbound standalone принимает только отсутствие pair и явный совпадающий profile id. | Spy invocation и read counters равны нулю для missing/wrong id, target, digest, capability value/order/set, missing/extra binding, stale snapshot; success только на exact match. Both-or-neither syntax tests сохранить. |
| F2 | Удалить `entries.slice(0,10000)`; reject 10001+, overlong detail/kind/path/total bytes и явно incomplete data. Проверить каждый entry без потери полей; не обрезать diagnostics без признака incompleteness. | 10000/10001 boundary и long details; никакого `status:ok` на потерянном индексе. Core отдельно отвергает `result.truncated:true` и отсутствующий expected result до merge. |
| F3 | Валидировать exclusions по scope grammar: literal file или `dir/**`; явно определить shorthand bare directory, лучше требовать `/**`. Prune subtree до descent, включая fileExists/config resolver. | `vendor/**`, nested generated/node_modules, sibling-prefix и точный file exclusion; zero content reads. |
| F4 | Удалить несуществующие `Stats.fileAttributes*`/`attributes` ветки. Описать реальные гарантии lstat/realpath; junction проверять как symlink. | Windows junction + Unix symlink tests; не заявлять универсальное распознавание всех reparse tags через отсутствующее API. |
| F5 | Пройти lstat по каждому компоненту от trusted root до root/read path, reject и inside-pointing, и outside-pointing links; realpath containment на чтении/перечислении, а не только на регистрации root. | Intermediate link в root и в descendant file, замена после resolve, dangling link; отсутствие чтения outside. |
| F6 | Одинаковый predicate «extension установлен и validated profile существует» для describe/dispatch. При отсутствии profile — structured unsupported/refusal до dereference/sink. | Extension-without-profile не даёт TypeError/exit-only error. |
| F7 | `describe.ir_versions` = sorted unique union validated `acceptedIrVersions` только enabled descriptors; проверить точный supported set. | Scan selection для 0.2.16 проходит только при реальной совместимости; несовместимые версии не попадают в union. Scan не становится requires_ir из-за этого поля. |
| F8 | stdin читать chunks до `MAX_REQUEST_BYTES+1`, прекратить на excess; fstat pipe size не является bound. File transport тоже открыть/проверить/read bounded, а не полный readFileSync. | Pipe без EOF и поток >1 MiB не заставляют целиком буферизовать input; ровно limit/limit+1, malformed bytes. |

При extraction kernel заодно локально исправить review nits F9/F10: `--version-json` только в точной argv форме; reject duplicate descriptor IDs/duplicate operation claims вместо first-registered-wins; удалить constant-true helper. Это не повод расширять feature scope.

## 4. Symbol indexing: compiler mechanisms и модель индекса

Compiler plan проверен через Context7 `/microsoft/typescript` (затем pin `/microsoft/typescript/v5.9.3`) и официальный [Compiler API guide](https://github.com/microsoft/TypeScript/wiki/Using-the-Compiler-API). Использовать public API выбранного pin; конкретные hook signatures проверить по его `lib/typescript.d.ts` при реализации. Не брать новые `spawnAPI`/remote snapshot examples из main.

| Индексируемое | Механизм и правила |
| --- | --- |
| Package/workspace exports | Читать разрешённые package.json как data, обрабатывать `name`, `type`, `exports`/`imports`, `types`, entry conditions; package roots задаёт trusted profile. `Program.getSourceFiles`, module `Symbol`, `checker.getExportsOfModule`, `checker.getAliasedSymbol`; отдельные public export edges к одной declaration identity. Package manifest и compiler resolution должны согласоваться. |
| Interfaces/type aliases/enums | AST kind guards + `getSymbolAtLocation`; `getDeclaredTypeOfSymbol`, `getTypeAtLocation`, properties, type arguments/constraints, enum constants через checker. Type-only exports сохранять; merged interface/namespace declarations — один symbol с несколькими spans. |
| Classes/methods/properties | Named class Symbol, declared instance type и constructor type; `getPropertiesOfType`, call/construct signatures; class member AST metadata (static/instance/private/protected/readonly/optional/accessors). Inherited member edge отделять от собственной declaration. |
| Functions/overloads | Function/arrow/function-expression symbol + `getTypeOfSymbolAtLocation`, `getSignaturesOfType`, `getSignatureFromDeclaration`, `getReturnTypeOfSignature`; все visible overloads и отдельно implementation signature. Не создавать duplicate native symbol на каждый overload или alias. |
| Routes/handlers | Только plugin rule с verified imported callee symbol/module и статическими аргументами/методом/path, alias-aware resolution. Связать handler symbol с endpoint candidate; namespace/name совпадение само по себе не evidence framework. Dynamic route/unknown wrapper — unknown, confidence/provenance обязательны. |
| Test definitions | Compiler-resolved imported `test`/`it`/`describe`-подобный DSL из явно поддержанного declaration source; static names, nested suites, each/parameterized cases только при статически перечислимых значениях. Referenced callable symbols в test body дают candidates, не доказательство проверки поведения. Не искать вхождение имени regex. |
| Imports/references | ImportDeclaration/ImportEquals/ExportDeclaration/type-only AST плюс symbol resolution каждого relevant identifier/property access; calls через `getResolvedSignature`, references через `getSymbolAtLocation`. Роли call/reference, source→target native edges; external/unresolved edge хранит uncertainty. Не приписывать semantic read/write/effects без доказательства. |
| Source ranges/JSDoc | `node.getStart(sourceFile)`, `getEnd`, `sourceFile.getLineAndCharacterOfPosition`; offsets UTF-16, start inclusive/end exclusive, line/column 1-based в internal records. `symbol.getDocumentationComment(checker)`, `getJsDocTags` — supporting metadata, не executable instructions и не semantic authority. |

Внутренний результат: sorted `packages`, `projects`, `symbols`, `exports`, `references`, `routes`, `tests`, `diagnostics`, `inputManifest`, compiler/profile/policy digests. Полные source spans, display names, original localReference, overload signatures и provenance остаются здесь. Canonical order по UTF-8 keys, не `localeCompare` и не fs traversal order. Вызовы compiler не выполняют исследуемый код.

### Native identity и signature identity

Не использовать `Symbol.id`, `Node.id`, номер строки, absolute/temp path, tsbuildinfo internals или один `checker.typeToString()` как identity. Предлагаемая domain-separated схема:

```text
identityTuple = ["lekalo.ts.native.v1", packageLocator,
                 declarationModuleRelativePath, lexicalQualifiedName,
                 declarationFamily, staticOrInstance]
native = "ts1-" + sha256(canonicalJson(identityTuple))
signature = "sha256:" + sha256(canonicalJson(normalizedSignatureGraph))
```

`packageLocator` = explicit package name + workspace-relative package root; для непакетного project — его explicit logical project key. Display spelling полного tuple хранится отдельно; native 68 ASCII chars подходит stableKey≤256. Коллизии hash проверять по tuple, не overwrite. Имя module/declaration не меняется от whitespace/добавления строк; смена signature оставляет native тем же. Move/rename даёт новый ID, если нет явной relocation mapping; не обещать автоматическое отслеживание произвольного переименования. Export alias/re-export module не входит в declaration identity; aliases имеют отдельные edges. Merged declarations используют canonical representative + проверенный набор declarations; два разных declarations с одинаковым lexical tuple — ambiguity, а не ordinal по обходу. Anonymous/dynamic identities без устойчивого anchor помечать unstable/unknown.

Signature graph: параметры в исходном порядке, optional/rest/this, normalized type refs, nullability, return/async Promise policy, generic arity/constraints, relevant modifiers, полный **упорядоченный** overload set (порядок может менять resolution). Members/set-like properties сортировать; recursion через stable type-reference anchors. Hash algorithm/domain/policy и compiler pin участвуют в scanner metadata/cache key. Один display string не заменяет структурное сравнение.

## 5. TS correctness, declarations, uncertainty и incremental

### Project configurations и resolution

1. Trusted roots включают exact tsconfig entry files и разрешённые extends/reference dependencies. Parse JSONC через TS API, а package.json через bounded strict JSON. Out-of-root `extends`/references/path aliases — refusal/unknown, не silent default compilerOptions.
2. Построить DAG project references; detect cycles/missing configs/overlapping ownership. Один Program на referenced project с его options, `projectReferences`, host и `oldProgram` где допустимо; не объединять всё в один Program с последним tsconfig.
3. Solution config с пустым `files` не означает empty project. Пройти references топологически. `createProgram` не выполняет `tsc -b`: clean checkout без generated .d.ts проверить отдельно. **Выбран public-API путь через virtual in-memory declarations** referenced project: после проверки source Program получить declaration-only output вызовом `program.emit(undefined, boundedCapture, undefined, true)` с analysis-only options `declaration:true`, `emitDeclarationOnly:true`, `noEmit:false`; callback принимает только declaration assets и ничего не пишет в filesystem. Feed эти outputs в VFS downstream Program, сохранив mapping их exports к исходным native identities/spans из upstream index. Не индексировать virtual generated declarations как новые native symbols и не читать stale dist. Emit diagnostics/пропущенные declaration outputs дают uncertainty. В 5.9.3 `CompilerHost.useSourceOfProjectReferenceRedirect` помечен **@internal**, поэтому не использовать его как публичную гарантию ([pinned source](https://raw.githubusercontent.com/microsoft/TypeScript/v5.9.3/src/compiler/types.ts)). Для конфигураций, которые не допускают безопасную declaration projection, — honest diagnostic; synthetic success fixture должна быть чистым source-only reference graph.
4. ESM/CJS resolution следует `module`/`moduleResolution`, package `type`, conditional exports/imports, import vs require usage, `.mts/.cts` при заявленной поддержке. Test fixtures CJS могут быть `.cts`/`export =` и `.ts` CommonJS; `.cjs/.js` только если отдельно объявлен allowJs/checkJs режим. `.ts/.tsx` обязательны.
5. `paths`/`baseUrl`, relative `.js` import→TS source, aliases/re-exports, package entry conditions и workspace package graph разрешать checker/TS module resolution с host hooks. Не превращать bare package import в первый совпавший package name. В synthetic pnpm layout использовать разрешённые source mappings/project references, а не symlinked store и не автоматическое install.

### Declaration/exclusion policy

- `.ts/.tsx` — indexed project source; `.d.ts/.d.mts/.d.cts` внутри явно разрешённых project roots — declaration evidence, **не доказательство реализации**. Встроенные TypeScript standard libs только type context, никогда project symbols/bindings.
- `node_modules`, `.pnpm`, vendor, generated, dist, build, coverage, caches и sensitive dot paths не индексируются; default в #44 также не читать их как type context. Project-local разрешённые authored declarations могут описывать framework fixture. Реальные внешние typings, отсутствующие по policy, дают unresolved uncertainty; broad `node_modules/**` для удобства не добавлять.
- `tsconfig.exclude` влияет на root selection, но import может вернуть файл в Program; enforce exclusions на **каждом host lookup/read**, а не только initial glob. Generated output определяется и fixed dirs, и tsconfig outDir/declarationDir, и explicit profile exclusions. Config не может отменить более строгую host policy.
- Текущая logical path grammar lowercase-only: `Foo.ts`, `[id].tsx`, `@scope` в directory path и иные неподдержанные spellings не переименовывать/не lower-case. Framework/package names в data могут быть такими, но source paths на wire — нет. Отказ с объяснением этой portability границы; поддержка произвольных repository paths — отдельное изменение соответствующих path contracts, не «незаметный fix» #44.

### Uncertainty

Собирать config/options/global/syntactic/semantic diagnostics через Program APIs. На relevant binding surface явно выявлять TypeFlags.Any (включая nested/generic contamination), unresolved imports/types, unknown/unsafe assertions, computed exports/accesses, dynamic require/import, dynamic route/test values, unsupported advanced types и incomplete reference graph. `skipLibCheck` не превращает неизвестный тип в проверенный. Отличать «точно известно, что native declaration есть» от «доказана совместимость с semantic ID».

Каждая inferred framework relation: rule id/version, resolved callee identity, declaring source span, source digest, confidence из `exact/high/medium/low/unknown`, origin и причины/alternative candidates. `exact` observation location не делает mapping user-confirmed. Ambiguity сохраняет все candidates. Полностью собранный inventory с неоднозначным mapping допустим только при честной candidate projection; если важное uncertainty/provenance не представимо, operation state `partial`/`unknown`, wire error, registry не обновлять. `q:'unknown'` отдельно не делает запись `state:unknown`: current consumer с location+fingerprint считает её current. Поэтому не выдавать успешный low-confidence binding как замену диагностике.

### Incremental design

Internal `ScannerSession` хранит content-addressed SourceFiles/Programs, module-resolution cache и dependency/reverse-dependency graph; key включает compiler/policy version, trusted profile digest/root manifest, все config/package bytes, options, source hashes и reference topology. Reuse `oldProgram` или public incremental builder с memory-only host, без `.tsbuildinfo`/watchers/writes. Invalidation по изменённому файлу + importing/type-dependent projects; изменение global declaration/options/paths/package exports вызывает wider rebuild. Удалённые files/exports удаляются из next index, не остаются из cache.

Cold/full и incremental output должны совпасть byte-for-byte, включая diagnostics, aliases, signatures и graph. Перестановка directory enumeration не меняет результат. Edit body без signature change сохраняет native/signature; edit signature сохраняет native, меняет signature; source fingerprint всё равно меняется.

**Process boundary:** TargetClient запускает новый child на каждый exchange, а `scan` не имеет cache request/result members. In-memory incremental reuse доступен embedder session и внутри project graph одного процесса; текущий `lekalo scan` между invocations будет cold scan. Не обещать persistent CLI acceleration и не писать cache из read-only scan. Если AC требует именно межпроцессный incremental, нужен отдельно спроектированный parent-owned cache transport с freshness validation; это дополнительный acceptance gap при freeze.

## 6. Lekalo IR compatibility и #42 binding registry

Исходники: [`ir/mod.rs`](../crates/lekalo-core/src/ir/mod.rs) `TypeRef`, `Field`, `CommandDef`, `QueryDef`, `TargetBindingDef`; [`observed/types.rs`](../crates/lekalo-core/src/observed/types.rs) `Evidence`, `Provenance`, `SymbolRecord`; [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs); [`observed/index.rs`](../crates/lekalo-core/src/observed/index.rs); [`observed/bindings.rs`](../crates/lekalo-core/src/observed/bindings.rs); [`docs/bindings.md`](../docs/bindings.md), [ADR-0035](../docs/adr/0035-bindings-registry.md). Исторические номера версий в docs/ADR не использовать вместо текущего versioning.md и Rust constants.

**#42 не выпустил TypeScript callable-signature schema.** В observed-scan/index `evidence.signature` — optional sha256; `references` — semantic target/role/confidence, `fields/base/values` — ограниченное type evidence. Core `bind_explicit` сегодня даже записывает file fingerprint в `evidence.signature`; это не structural TS signature. IR `TargetBindingDef` содержит `id/common/target`, а не native callable. Не путать его с semantic binding registry или scenario/native backend binding.

Предлагаемая compatibility policy version 1:

- TypeRef — только Ref/List/Optional, scalar bases: string/number/boolean/date/datetime/uuid/uri; map scalar aliases по resolved semantic definition. TS string не доказывает uuid/uri/date refinements; unresolved branding/refinement — unknown.
- Entity/value-object fields сравниваются структурно: required/optional/nullability, recursive refs, arrays, enums/literal domains. General unions/intersections/conditional/mapped types поддерживать только после concrete checker resolution; unsupported remainder → unknown.
- Для command нужен явно выбранный binding convention «один input object» с IR input fields; для query — явно определённая callable shape и returns. Не выводить effects, authorization, reads/writes или business semantics из совпадения TS signature. Promise return unwrap только при объявленной async convention; разные overloads должны проверяться полностью, не first-match.
- Внутренний verdict `compatible|incompatible|unknown` с reason list и всеми candidates. Совместимость type shape не подтверждает implements/exposes/verifies автоматически. Semantic ID/kind берутся из явного mapping/IR либо остаются proposals; function≠command и class≠entity по одному синтаксису.
- Current scan request IR не передаёт. Реализация внутреннего matcher тестируется против parsed v0.2.16 IR fixture; production comparison только если caller явно дал `ir_path`, объявленный в read scopes, и adapter проверил exact IR version/shape. Для `lekalo scan` добавить опциональное предоставление **существующего** compiled IR через CallRequest, без автоматической записи compilation output. Без IR verdict unknown, не «совместим».

Изменение signature: проверить два независимых сценария (а) scan → confirm → source edit → `bindings audit` до rescan: file truth даёт stale; (б) scan → confirm → signature edit → rescan → audit. Сегодня merge может обновить fingerprint confirmed record и вернуть current даже при изменённой signature; plan включает запрет такого автоматического refresh без compatibility evidence. Для frozen wire консервативно сохранять старый fingerprint/user-owned mapping на изменении bytes, пока нет повторного подтверждения/доказанной совместимости. Не подменять новую signature старым file hash. Для rich path сравнить прежний signature digest + native identity до обновления; изменение semantic shape требует stale/reconfirmation. Explicit priority и provenance preservation остаются обязательными.

## 7. Protocol mapping и решение по contract freeze

Проверены [`target-protocol.schema.v0.2.16.json`](../contracts/target-protocol.schema.v0.2.16.json), [`observed-scan.schema.v0.2.16.json`](../contracts/observed-scan.schema.v0.2.16.json), [`observed-index.schema.v0.2.16.json`](../contracts/observed-index.schema.v0.2.16.json), Rust `wire.rs` и `scan_service.rs`; также fixtures [`scan-response.json`](../tests/fixtures/target-protocol/valid/scan-response.json), [`bind-response.json`](../tests/fixtures/target-protocol/valid/bind-response.json). Старые имена `v1_1`/`v1_2` fixture не означают поддержку другой текущей версии.

| Данные | Существующее место / предел | План |
| --- | --- | --- |
| Adapter identity | Wire Evidence `{adapter,plan_id?}` | Echo exact bundle id/version/digest; не дописывать arbitrary evidence fields. |
| Scan inventory | `entries:[{path,kind,detail?}]`, ≤10000; detail≤128 Unicode code points | Complete semantic projection либо refusal; `kind` в consumer — закрытые Lekalo scalar/enum/value-object/entity/command/query/event/policy/effect/endpoint, **не** TS class/function/interface. |
| Semantic/native/location | Detail строго `{s,n,l?,q?,t?}`, **s и n оба required** | Stable native hash и компактный semantic proposal; 1-based declaration line. |
| Confidence | Detail `q` → observed mapping confidence | Всегда передавать явно; default consumer medium нельзя использовать для неизвестного. |
| Ambiguous mappings | Несколько entries с одним s → candidate set | Дедуп native+semantic перед wire; distinct candidates сохранить; тест same semantic/different kind без group[0]-выбора. |
| Native tests | `t` → core testBindings | Полный detail часто не помещается; отказ, а не discard имени/файла или второй fake candidate. |
| Signature, references | Уже есть в observed evidence, **нет** в accepted detail parser/scan transport mapping | Internal retention; общий graph не обновится без следующего integration change. |
| Full spans/JSDoc/framework rule provenance | Нет закрытого wire/observed span/rule object | Только internal outcome/local-only sink, документировать отсутствие durable cross-process round-trip. |
| Freshness/revision | Core сам fingerprints real source и hash `[adapter identity,request_id]` | Не доверять scanner `current`; internal revision дополнительно binds actual input/type graph. |
| Native semantic bind | Wire bind только `{module,target,profile}` | Не использовать; registry получает semantic evidence через scan/update. |

Пример допустимой **формы**, не готовой семантической истины: `detail = JSON.stringify({s:'demo.item',n:'ts1-'+hash,q:'unknown'})`; полная длина считается перед serialization. Даже short hash + длинный semantic ID + line/test может превысить 128. Поля нельзя обрезать, хешировать semantic ID с потерей связи с IR или раскладывать один факт по псевдоentries. Нельзя использовать result.findings/progress/error.detail как неописанный data channel для graph: core их не объединяет, а public semantics не такие.

Core guard при merge: требовать result.entries, `truncated !== true`, полную representability, candidate/path coverage и type validity до записи index. Signature/ref fingerprints вычислять/проверять от того snapshot, который был scanned; изменение реального source между call и build_document не должно получить fresh fingerprint без rescan. **Live finding:** TargetClient повторно проверяет real input snapshot только в apply/publish ветке, не после read-only scan; `build_document` затем заново читает real tree. Добавить внутренний snapshot custody result для scan consumer (без wire members), сравнить текущие source bytes с scanned digests перед merge, либо вернуть drift/refusal; registry fingerprint относится к scanned bytes, а subsequent audit выявляет дальнейшее изменение. Не присваивать старым symbol facts fingerprint нового файла.

### Что можно выпустить при contracts 0.2.16

Bundled real scanner + F1–F8 + restricted host + internal complete native/reference/type index + precise local uncertainty + bounded existing registry projection; tests действительно находят pnpm-shaped exports и предотвращают aliases/overload duplicates. Полные spans/provenance допустимо держать внутри, как #43. Это честная стадия, но **не полное закрытие #44**: общий graph AC5 и полная signature transport semantics остаются неподключёнными. One-shot localEvidenceSink не становится durable artifact автоматически; нельзя утверждать, что пользователь после child exit может его получить.

### Минимальный отдельный change для полного AC5 (только после снятия freeze)

Предпочтение: один typed optional member у `ScanEntry`, например `evidence`, повторно использующий **существующие** observed evidence fields `signature/references/fields/base/values/identityFields`; дополнительно typed semantic/native/line/confidence/test fields вместо 128-character compact bottleneck, если требуется общий, а не ограниченный scanner. Для самого AC5 минимально достаточно typed `references` (и желательно `signature`) + consumer projection. Не расширять верхний Wire Evidence arbitrary JSON, не переносить raw spans/local references наружу. Existing observed-scan/index schema могут остаться 0.2.16, если их shapes/semantics не меняются; endpoint rich routing также требует своего явно typed mapping, не обещать его от одного signature field.

Per [`docs/versioning.md`](../docs/versioning.md) изменённый **target protocol** получает текущую product version на commit: `0.3.1`, если это всё ещё issue candidate, либо реально зарезервированную более позднюю версию follow-up. Обновить schema filename/$id/discriminators/exact protocol negotiation/constants/registry hashes/fixtures/consumers, явно определить compatibility set; не присваивать изменённому контракту `0.2.17` и не менять 0.2.16 in place. IR и observed contracts не bump без изменения. Current research ничего из этого не выполняет.

Это minimal design alternative, конфликтующая с текущим требованием «contracts stay 0.2.16»; implementer должен оставить AC5 открытым при сохранении freeze, а не молча реализовать альтернативу. Public arbitrary filenames/full spans/rule provenance и persistent cache, если потребуются, увеличат минимальный change и требуют отдельного scope решения.

## 8. Fixtures без package-manager execution

Разместить под `tests/fixtures/node-typescript-scanner/` с README `public-fixture`, synthetic provenance, manifest разрешённых файлов/хешей и expected internal index + bounded wire projection. Existing `node-typescript-kernel` и `bindings/ts-scanner.mjs` сохранить как независимые regression fixtures.

| Каталог | Содержание и oracle |
| --- | --- |
| `esm/` | package type module, tsconfig, named/default/type exports, alias chains/export-star conflicts, function overloads, merged interfaces, methods/properties, JSX и authored declarations. |
| `cjs/` | CommonJS tsconfig, `.cts`, import-equals/export-equals, статический exports assignment там, где checker подтверждает; dynamic exports negative case. |
| `project-references/` | solution→base→app, composite configs, разные compiler options, clean source-only state без dist, separate cycle/missing reference/changed dependency fixtures. |
| `path-aliases/` | baseUrl/paths, extended config, `.js` import→TS, type aliases/re-exports; denied outside-root alias и duplicate resolution candidates. |
| `pnpm-workspace/` | inert `pnpm-workspace.yaml`, package.json `workspaces`/`workspace:*`, packages/a и packages/b с names/exports/types и references; source mappings заданы явно. Ни lock install, ни реальный `.pnpm` store для success case не нужны. |
| `framework-evidence/` | Синтетические локальные declarations/imports известного rule vocabulary, static route→handler, static tests→referenced symbol; одноимённая несвязанная функция, dynamic path/name, shadowed import дают uncertainty. Не выдавать synthetic stub за conformance с реальным framework. |
| `uncertainty/` | nested any, unknown, unresolved dependency, dynamic require/import, nonliteral computed property, generic/conditional remainder, unresolved global lib. |
| `excluded/` | Poison text в node_modules/vendor/dist/build/coverage/generated и sensitive paths; manifests не дают scopes на эти файлы. Отдельный internal tree traversal тест проверяет pruning. |
| `incremental/` | edit scripts как inert data: whitespace insertion, body change, signature edit, deletion, config/exports edit, global declaration edit; cold-vs-warm oracle. |
| `protocol/` | raw request bytes, trusted profiles, mismatch matrix F1, expected response/errors; long detail, >10000 entries и >64 scopes negative vectors. |

Все package scripts — poison sentinels. Fixture runner запускает только разрешённые Node entry/lekalo test binaries, не `npm`, `pnpm`, `yarn`, `tsc`, framework runners или project hooks. Install **build dependencies адаптера** — отдельный development/CI provisioning step, никогда часть scan/fixture execution. Junction/symlink cases создавать только в test-owned внешнем temp directory, platform capability фиксировать явно; skip unsupported host setup не считать проверкой no-follow. Case-sensitive и uppercase path negatives обязательны.

## 9. Reserve product 0.3.1

Повторить смысл commit [`3e4159d`](https://github.com/ichinya/lekalo/commit/3e4159dc666058936827f968869dc2a51638c37a) и [`scripts/reserve-030-regen.mjs`](../scripts/reserve-030-regen.mjs), не делать blanket replace `0.3.0`/`0.2.16`.

- `Cargo.toml` workspace.package.version и две local package entries `Cargo.lock` → 0.3.1; adapter/package metadata → 0.3.1.
- Product literals: `crates/lekalo-cli/tests/{authorization,cli,context,diff,effects,graph,impact,inspect,requirements}.rs`; version-sensitive assertions `crates/lekalo-core/tests/lockfile.rs`, kernel integration tests и adapter tests.
- Новый `scripts/reserve-031-regen.mjs` на основе accepted recipe: assert binary 0.3.1, unique external temp root, contract-only lock generation; `core.version` 0.3.1, `contracts.target_protocol.version`/`resolver.version` 0.2.16.
- Regenerate `tests/fixtures/lockfile/valid/contract-only.lock.json`, его `.expect.json` (оба `lockDigest` и `payloadSha256`), `GOLDEN_DIGEST` в core lockfile/CLI lock tests.
- Actual copied-fixture `lock → generate --dry-run → generate → verify` receipts: `tests/fixtures/orchestration/{generate.dry-run,generate.apply,verify.full}.golden.json`. Проверять **`receipt.verdict === 'ready'`**, нормализовать только planId, как accepted script. Не переносить ошибочный старый плановый `status:'valid'` predicate.
- Не менять contracts, diagnostic/capability registry/resolver versions или unrelated synthetic adapter fixture versions ради product reserve. Fresh checker должен показать product 0.3.1 и 58 unchanged artifact families.

## 10. Test plan и точные команды

Команды ниже — для будущей implementation, не отчёт об их выполнении. Имена новых files/commands являются частью плана. Dependency provisioning требует lockfile, создаваемого implementer; fixtures никогда сами его не выполняют.

```powershell
# Build provisioning, separate from scanner/test runtime; no target-project install.
npm ci --prefix adapters/node-typescript --ignore-scripts --no-audit --no-fund
node adapters/node-typescript/build.mjs
node adapters/node-typescript/build.mjs --check
node --check adapters/node-typescript/adapter.mjs
node scripts/test-node-typescript-kernel.mjs
node scripts/test-node-typescript-scanner.mjs

cargo build --workspace --locked
cargo run --locked -p lekalo-cli -- --version --json
# Execute only after precise reserve edits and a fresh 0.3.1 binary.
node scripts/reserve-031-regen.mjs
cargo test --locked -p lekalo-core --test node_typescript_kernel --test node_typescript_scanner
cargo test --locked -p lekalo-core --test bindings --test observed --test target_protocol --test target_protocol_boundaries --test target_discovery --test target_profiles --test adapter_conformance
cargo test --locked -p lekalo-cli --test bindings --test observed --test adapter_test --test generate_orchestrate --test lock
cargo test --locked -p lekalo-core --test lockfile
cargo test --locked -p lekalo-cli --test authorization --test cli --test context --test diff --test effects --test graph --test impact --test inspect --test requirements
cargo test -p lekalo-core target_protocol_conformance --locked
```

`--ignore-scripts` esbuild provisioning: platform optional binary должен присутствовать; build проверяет exact esbuild version и отказывает при отсутствии binary, не запускает fallback install/download. Если `CARGO_TARGET_DIR` внешний, перед reserve recipe задать `LEKALO_BIN` точным path freshly built executable.

### Conformance commands

```powershell
# Bare entry keeps #43 no-profile behavior; retain these regression commands.
cargo run --locked -p lekalo-cli -- adapter test --profile default --report junit -- node adapters/node-typescript/adapter.mjs
cargo run --locked -p lekalo-cli -- adapter test --profile strict --report junit -- node adapters/node-typescript/adapter.mjs

# Proposed committed synthetic launch profile, read as text by the caller, not child.
$scannerProfileJson = (Get-Content -Raw tests/fixtures/node-typescript-scanner/protocol/conformance.profile.json | ConvertFrom-Json | ConvertTo-Json -Compress -Depth 16)
cargo run --locked -p lekalo-cli -- adapter test --profile default --report junit -- node adapters/node-typescript/adapter.mjs --lekalo-project-profile-json $scannerProfileJson
cargo run --locked -p lekalo-cli -- adapter test --profile strict --report junit -- node adapters/node-typescript/adapter.mjs --lekalo-project-profile-json $scannerProfileJson

# Existing independent complete-surface reference stays green.
cargo run --locked -p lekalo-cli -- adapter test --profile strict --report junit --timeout-ms 60000 -- node tests/fixtures/target-protocol/fake-adapter.mjs --lekalo-adapter-variant fluent
```

Создать conformance.profile с unbound standalone profile и roots, согласованными с реальным embedded conformance fixture; этот запуск не подменяет semantic fixture suite. На Windows передачу JSON как одного argv проверить process test; если shell quoting конкретного host ломает JSON, запускать тот же argv через test harness `spawnSync` array, не исполнять string-built shell command.

### Contract/regression gates

```powershell
# Ajv 8.17.1 pre-provisioned outside checkout, as in CI.
$env:NODE_PATH = $env:LEKALO_AJV_NODE_PATH
node scripts/check-contract-versions.mjs
node scripts/test-contract-versions.mjs
node scripts/test-versioning-contracts.mjs
node scripts/test-lockfile-contracts.mjs
node scripts/test-target-protocol-contracts.mjs
node scripts/test-target-profile-contracts.mjs
node scripts/test-bindings-contracts.mjs
node scripts/test-observed-contracts.mjs
node scripts/test-graph-contracts.mjs
node scripts/test-orchestration-contracts.mjs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
git diff --check
git diff --exit-code -- contracts
```

В CI сохранить все существующие privacy/authority/structure/schema gates, Node 18/24, Rust MSRV 1.80 и OS confinement matrix; добавить offline bundle check и scanner suites. Hosted `check-contract-versions --base HEAD^` запускать на фактическом commit baseline. Увеличивать timeout только после измерения compiler startup/typecheck; infrastructure failure sandbox backend не считать scanner failure или PASS.

### Обязательные новые probes

1. F1–F8 before-invoke guards, no-profile parity, duplicate descriptor/operation rejection, exact identity после bundle relocation.
2. Compiler standard lib resolution без filesystem assets; no ts.sys/project package/global environment fallback; bounds до allocation.
3. Все symbol categories таблицы, exact export sets, overload order, renamed imports/export-star conflicts, project-reference source redirect, declaration-only versus implementation.
4. Compiler resolves primitives/object members/generics, а test oracle не повторяет regex implementation; source text с ложными `export` в comment/string не создаёт symbols.
5. Local file/body/signature/config/reference edits: identity стабильна где обещано, signature hash меняется по правилам, incremental=cold, удалённые rows исчезают.
6. Registry public chain scan→propose→confirm→edit→audit; отдельно edit→rescan→audit, explicit priority, ambiguity refusal, provenance retention. Refusal не записывает частичный index.
7. Signature/reference graph round-trip через public APIs при **разрешённом** rich transport; при freeze тест/отчёт явно фиксирует AC5 incomplete, не green empty graph.
8. Adversarial nested exclusions/symlinks, no real PM/native scripts, source/config before/after hash, malformed UTF-8/JSON, response caps, unknown kinds, detail overflow, truncated flag.
9. Exact-SHA evidence: adapter bundle digest, compiler pin, input manifest, command/exit, fresh consumer results. Existing #43 61 Node/4 Rust counts не выдавать за новые #44 test counts.

## 11. Strict-conformance: точные ожидаемые изменения

Основание — [`adapter_conformance/mod.rs`](../crates/lekalo-core/src/adapter_conformance/mod.rs), `capability_phase`, `read_determinism_phase`, `process_phase`, `settle_determinism`, и closed [check catalog](../crates/lekalo-core/src/adapter_conformance/check.rs). Каталог содержит **18** checks. `requires_ir()` true только для validate/generate/verify, не scan/bind.

| Check(s) | #43 bare | #44 configured scan-only |
| --- | --- | --- |
| describe.handshake, describe.negotiation, capability.declaration | pass (3) | pass (3) |
| confinement.canonical, confinement.plan-scopes, redaction.evidence | pass (3) | pass (3) |
| process.cancellation | skip | **pass**: scan — первый cancellation candidate, fresh describe recovery |
| capability.ir-declaration | skip | skip: scan не requires_ir, несмотря на declared accepted IR version |
| determinism.repeats | skip | **skip**: read probes только validate/verify; recovery добавляет describe-digest один раз, count<2 |
| input.invalid-ir, diagnostics.structured | skip (2) | skip (2): validate не объявлен |
| generate.dry-run-plan, generate.apply-plan, apply.retry-discipline, artifact.manifest-evidence | skip (4) | skip (4) |
| plan.clean-cycle, scenario.normalization | skip (2) | skip (2) |
| capability.surface (default / strict) | skip / fail | skip / fail: отсутствуют bind/validate/generate/verify/clean/plan-clean |

**Ожидание при успешных prerequisite exchanges:** bare default = 6 pass/12 skip/0 fail (exit 0), bare strict = 6 pass/11 skip/1 fail (exit 4). Configured scan default = **7 pass/11 skip/0 fail** (exit 0), strict = **7 pass/10 skip/1 fail** (exit 4). Единственный обязательный skip→pass — `process.cancellation`; skip→fail новых cases не ожидается. Если появится иной fail, это actionable regression, а не «strict и так красный».

В cancellation передан уже выставленный flag: scan body обычно **не запускается**. Даже 7/11 не доказывает symbol scanning, ESM/CJS или deterministic scan output. Собственные Node/Rust probes обязаны вызвать production scanner на настоящем synthetic source и повторить запрос. Не расширять shared conformance checks/semantics без отдельного versioned ownership; отсутствие полного badge остаётся честным. Если bind позже добавлен без IR-consuming operations, цифры сами по себе не улучшатся.

## 12. Порядок implementation и связь с AC #44

Нумерация AC соответствует live issue: AC1 pnpm exports; AC2 overload/alias/re-export duplicates; AC3 any/dynamic uncertainty; AC4 signature invalidation; AC5 dependency graph; AC6 no node_modules/build index; AC7 fixture matrix.

| Шаг | Работа и exit condition | Acceptance |
| --- | --- | --- |
| 1 | Зафиксировать ограничения contract freeze/root authority/incremental process boundary в README; не объявлять полный #44 done без AC5. Выбрать explicit launch-profile path и trusted snapshot source. | Prerequisite всех AC |
| 2 | Reserve product 0.3.1 по §9, focused custody gates; untouched contracts 0.2.16. | Lifecycle |
| 3 | Extract kernel source + F1–F8/nits, baseline Node/Rust tests; scanner ещё не включён. | Safety prerequisite AC1–AC7 |
| 4 | Build dependency lock, compiler+lib single-file bundle, reproducibility/relocation/offline tests. | Real compiler prerequisite |
| 5 | Launch profile bootstrap, descriptor projection, restricted host/enumeration/VFS/config graph; zero excluded reads. | AC1, AC6, AC7 |
| 6 | Synthetic fixtures ESM/CJS/references/aliases/pnpm, public Program/Checker extraction, stable IDs/overload/alias canonicalization. | AC1, AC2, AC7 |
| 7 | Structured types/signatures, uncertainty classification, framework/test supporting evidence, internal IR matcher. | AC3, AC4; scope requirements |
| 8 | Incremental session + cold parity; edits/deletions/config/ref invalidation. | Incremental requirement, AC4 |
| 9 | Existing bounded scan projector и core no-truncation/no-empty/no-silent-refresh guards; full public binding lifecycle probes. | AC1–AC4, AC6; AC5 ещё не закрыт |
| 10 | При сохранении freeze: закончить совместимый milestone и оставить AC5 открытым; при отдельно разрешённом contract change: typed reference/signature transport→observed evidence→graph/impact, versioning и round-trip gates. | **AC5** только после реального consumer round-trip |
| 11 | All affected gates/full regression, default+strict counts, fresh exact-SHA evidence и независимый review candidate. | Final acceptance |

Eventual completion report должен перечислять actual pass/fail/skip, versions и artifact digest, реализованные и оставшиеся AC, отличие internal index от persisted registry, source preservation, no package execution и unresolved type/project cases. Не считать schema-valid response, empty graph, test-only scanner injection или default conformance полноценным acceptance #44.

## Research validation

- Live `gh issue view 44/42 --repo ichinya/lekalo`, HEAD/tag/status и reserve `3e4159d` просмотрены read-only.
- Context7 использован для TypeScript Compiler API и esbuild; pinned package metadata и официальная API guide дополнительно проверены напрямую. API/build recipe здесь — проектные решения, не запущенный прототип.
- `node scripts/check-contract-versions.mjs` выполнен: `{ok:true,product:'0.3.0',contractArtifacts:58,base:'HEAD'}`.
- Проверены 25 относительных ссылок документа: все разрешаются; UTF-8 без replacement characters/trailing whitespace. `git diff --check` и `git diff --exit-code -- contracts` успешны; tracked files не изменены, единственный созданный файл — этот план в существующем untracked `.m3/`.
- Implementation/semantic tests, reserve recipe, dependency installation и compiler bundle build в этом исследовании не запускались; будущие conformance числа выведены из live suite source, не представлены как измеренный результат кандидата.
