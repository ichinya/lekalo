# Issue #48 — pnpm workspaces, native gates и targeted execution: план реализации

Дата: 2026-09-16. Режим: RESEARCH ONLY. Исследован `ichinya/M3`, HEAD `047f0e8539ae5b1a366074d299e4cbae0b979e17`, product `0.3.1`; исходный status — только untracked `.m3/`. Этот документ не является реализацией или acceptance report. Tracked-файлы, версии и контракты в ходе исследования не менялись; commit не создавался.

## 1. Решение и доказательная база

Рекомендуется **read-only planner в bundled Node adapter + независимая проверка immutable plan в lekalo-core + отдельный fixture-only runner, отсутствующий в production build**. Выбор пакетов не делегируется `pnpm --filter`, а выполнение не делегируется `pnpm run`: planner строит точный список команд и причин, runner принимает только заранее одобренный digest. Existing private/untrusted repositories получают plan-only и blocked receipt до #89. Никакой флаг в production CLI не включает fixture runner.

**Contract verdict: protocol 0.3.1 недостаточен для выбранного end-to-end seam.** Нужен новый `target-protocol.schema.v0.3.2.json` с одной read-only операцией `plan-native`, typed request/result и registry capability; дополнительно нужны отдельные closed contracts policy/plan/run/evidence. `verify`, `generate`, `plan-clean`, `WriteEntry` и их `plan_id` не переопределяются. Run — отдельный host/test lifecycle, не новый исполняющий adapter RPC. Старый 0.3.1 остаётся неизменённым; новый current exact version и consumers переводятся согласованно по `docs/versioning.md`.

Прочитаны live issues [#48](https://github.com/ichinya/lekalo/issues/48), [#120](https://github.com/ichinya/lekalo/issues/120) (CLOSED), [#89](https://github.com/ichinya/lekalo/issues/89) (OPEN), sibling `.m3/issue-43-{report,review}.md`, `.m3/issue-44-report.md`, `.m3/issue-44-review-r2.md`. Последний файл содержит сначала fix-required r2, **затем final verification R-1 с отсутствием blocking findings на `44c5938`**: нельзя цитировать его первый verdict как состояние merged #44. Шиппед README и текущий код использованы для проверки исторических утверждений отчётов.

| Проверенный исходник | Что действительно существует; значение для #48 |
| --- | --- |
| `adapters/node-typescript/src/kernel.mjs`, `README.md`, `build.mjs` | Explicit extension registry, trusted launch profile, bounded readView, profile-binding до dispatch, single-file compiler bundle. Никакого discovery/import project plugin или запуска scripts. |
| `src/scanner.mjs`, `discoverPackagesAndProjects`, `packageLocatorFor`, `ScannerSession`, `scanOperation` | Internal package roots, locators `{kind,name,root}`, modules, references, config metadata; pnpm-shaped fixture ещё не означает pnpm workspace parser. Компилятор 5.9.3 уже vendored. |
| `crates/lekalo-core/src/target_protocol/wire.rs` | Closed `Operation`, `RequestEnvelope`, `OperationResult`; `plan_id` разрешён только generate apply/clean. Result содержит ok/truncated/entries/findings/bindings; response Evidence — adapter и optional write plan id. Полей native plan/run нет. |
| `target_protocol/plan.rs` | SHA256 write-plan, scope snapshots, before-state checks; это output generation plan, не command authorization. Snapshot bounds/path machinery полезны, semantics write plan повторно не используются. |
| `target_protocol/transport.rs` | Bounded capture/deadline/cancellation, process-group cleanup; **public `run` не очищает env**, в отличие от `run_private`. Raw transport не является безопасным runner. |
| `target_protocol/confinement.rs`, `confinement_windows.rs` | Existing private stage; Linux bwrap `--unshare-all`, macOS sandbox profile, Windows LPAC + kill-on-close Job Object, suspended launch до Job assignment. Наличие кода не доказывает работоспособность на данной машине; scope расширения для package cwd/env ещё надо проверить. |
| `observed/{scan_service,index,bindings,types,view}.rs` | 0.3.1 typed scan signature/references доходят до observed Evidence. Package locator не отдельное wire field: semantic/native ids и source paths нельзя трактовать как сериализованный full scanner index. Observed impact прямо остаётся incomplete. |
| `impact/gate.rs` | Neutral gate selection; модуль не исполняет команды и не является native scheduler. |
| `target_protocol/capability.rs` | Closed registry `dev.lekalo.target-capabilities@0.3.1`; `verify.scenarios` не означает native gates; неизвестный capability id отвергается. |
| `contracts/authority-matrix.v0.2.16.json`, `privacy-policy.v0.2.16.json` | Exact authority/privacy refs; labels не дают execution authority. `diagnostics.raw-tool-output` имеет других allowed writers, а `ai-factory.execution-plan` — другого owner; их нельзя присвоить Lekalo runner только по похожему названию. |

Документация pnpm получена через Context7 (`resolve-library-id`, затем `/pnpm/pnpm.io` `query-docs`) и уточнена по официальным versioned pages. Непривязанный к версии сайт уже описывает 12.x; implementation должен объявить проверенный compatibility subset и exact fixture pin, а не обещать все версии pnpm.

- `pnpm-workspace.yaml` определяет membership; root включён всегда, отсутствие `packages` в текущей 10.x documentation означает root-only; patterns допускают inclusion/exclusion. Это discovery membership, не разрешение выполнять root scripts. [pnpm 10.x workspace config](https://pnpm.io/10.x/pnpm-workspace_yaml).
- `workspace:` означает local workspace resolution; aliases и relative references существуют. Обычный semver dependency с совпавшим именем сам по себе не доказывает local link. [pnpm 10.x workspace protocol](https://pnpm.io/10.x/workspaces).
- Выбор dependents идёт по обратным edges (`...foo`), dependencies — по прямым; #48 использует собственный bounded traversal и сохраняет объяснения. [pnpm filtering](https://pnpm.io/filtering).
- `pnpm run` имеет `scriptShell`/`shellEmulator`, а pre/post execution управляется настройкой; даже direct spawn pnpm не гарантирует shell-free gate. Поэтому lifecycle dispatch не используется вообще. [pnpm 10.x run](https://pnpm.io/10.x/cli/run).

## 2. Архитектура и файловая карта

Ниже новые имена — предложения для implementation, не уже существующие APIs.

| Файл/модуль | Ответственность |
| --- | --- |
| `adapters/node-typescript/src/workspace.mjs` (new) | Только чтение bounded inventory: pnpm membership, package manifests, package graph, manager capability path, package-local config resolution. Не читает ambient parent dirs, home, npmrc credentials, node_modules или global pnpm config. |
| `src/native-plan.mjs` (new) | Pure join scanner evidence→packages, affected closure, confirmed gate resolution, stable command order, proposed immutable plan. Ни `child_process`, ни shell/PM/network, ни writes. |
| `src/native-contract.mjs` (new) | Closed shape/enum/bounds validators и canonical digest implementation, общие Node conformance vectors с Rust. |
| `src/scanner.mjs` | Экспорт ограниченного internal metadata API поверх существующего inventory/session; сохраняет native identity и semantic signature domain. Package-specific config interpretation переиспользует vendored TS parser, а не запускает tsc. |
| `src/kernel.mjs` | v0.3.2 decoding, `plan-native` routing, typed result projection, registration-based capabilities. Trusted profile/target/digest binding проверяется до любых reads. `describe` по-прежнему не читает project. |
| `build.mjs`, `adapter.mjs`, `THIRD_PARTY_NOTICES.md`, package manifests | Bundle planner и parser dependencies в один artifact. Exact development pins, notices, deterministic rebuild. Если нужен YAML parser — bounded bundled parser с запретом duplicate keys/tags/alias expansion, а не regex и не runtime install. Выбор/pin parser фиксируется implementation commit после отдельной проверки его docs. |
| `crates/lekalo-core/src/native_gate/{mod,types,wire,version,policy,plan,digest,selection,evidence,diagnostic}.rs` (new) | Host API, strict independent validation, checked policy, snapshot custody, recomputation digest, plan-only execution refusal, normalized receipts, composite observed projection. Никакого `Command::spawn` в этом production module. |
| `crates/lekalo-core/src/lib.rs` | Export pure native_gate API. Не добавлять новый production runner crate. |
| `target_protocol/{wire,version,mod,capability,discovery,selection}.rs` | New read-only op/result, exact negotiation and custody. Native plan не попадает в pending generation PlanBinding и не разрешает publish. |
| `observed/view.rs` + `native_gate/evidence.rs` | Read-only join package/gate receipts к current observed records по native id + symbol + fingerprint + revision. Existing observed JSON не расширять скрыто; новый composite `NativeObservedView` имеет собственный contract. |
| `crates/lekalo-cli/src/main.rs` | Proposed `native plan` + JSON output; отдельный `native run --plan ... --approve-digest ...` в product возвращает typed blocked/unsupported receipt без запуска. Никаких prompts/fallback в CI; explicit checked-in policy обязателен. |
| `crates/lekalo-core/tests/support/native_gate_runner.rs` (new) | Real synthetic-fixture harness: trusted catalog, approval, stage, backend preflight, argv launch, capture, mutation comparison, cleanup. Компилируется только в test build. |
| `native_gate/fixture_tests.rs` (new, `#[cfg(test)]`) | Внутренние runtime tests для доступа к существующим crate-private confinement primitives. Integration tests pure plan/API вынести в `tests/native_gate.rs`. Test-only narrow backend adapter в `target_protocol/confinement.rs` при необходимости; production launch authority не расширять. |
| `tests/fixtures/node-native-gates/**`, `adapters/node-typescript/test/{workspace,native-plan,native-plan-process}.test.mjs`, `scripts/test-node-native-gates.mjs`, `scripts/test-native-gate-contracts.mjs` | Synthetic inputs/goldens, unit/process/contract tests; runner tests отдельно в Rust test-only harness. |
| `crates/lekalo-core/src/adapter_conformance/{check,fixture}.rs`, `crates/lekalo-core/src/target_protocol/conformance.rs`, `.github/workflows/ci.yml` | Read-only native-plan conformance rows, truthful capability surface и per-platform fixture evidence. Не делать execution условием describe/scan conformance. |
| `docs/native-gates.md` (new), `docs/target-protocol.md`, `docs/observed-mode.md`, adapter README | Scope, exact version/support table, plan/run boundary, result vocabulary, honest platform gaps. Исправить противоречивую historical prose про «two exact versions» в current protocol docs. |

Fixture harness переиспользует audited isolation backend, но **не** протокольный `generate/apply`, `Sandbox::publish` и не public raw transport. Нельзя обойти stage/backend mismatch запуском unconfined Node. Existing confinement содержит переписывание executable/first-script paths и добавление Node flags; fixture launcher должен либо заранее включить эти преобразования в одобренную launch recipe, либо предоставить узкий test-only launch adapter, который не меняет argv. Любая неучтённая добавка flags/argv запрещена.

## 3. Closed contracts и versioning

### 3.1 Необходимые новые/изменяемые families

| Family / proposed artifact | Решение, exact изменение |
| --- | --- |
| `contracts/target-protocol.schema.v0.3.2.json` | New operation `plan-native`; optional request member `native_request`, required только у этой операции; optional result `native_plan`, required при её успешном результате. Closed refs на native types; в остальных ops эти поля запрещены. `plan-native` требует target/profile и bound resolution; IR optional для contracted bindings, scan evidence/snapshot mandatory по new request. `dry_run`, write `plan_id`, `writes` запрещены. |
| `contracts/native-gate-policy.schema.v0.3.2.json` | Exact confirmation, allowed tools/gates, release fallback, env recipe, limits, output/write scopes, trust restriction и refs. Execution policy отличается от privacy `policyRef`. |
| `contracts/native-gate-plan.schema.v0.3.2.json` | Workspace/packages/edges, scan+input custody, changes, affected reasons, commands, capabilities/gaps, policy+privacy metadata, plan digest. Общие `$defs` для command/tool/ref/measurement. |
| `contracts/native-gate-run.schema.v0.3.2.json` | Closed run request и terminal receipt через discriminated `oneOf`, с `$defs` run approval, per-command result, mutation/output/cleanup. Нельзя передать alternative argv/cwd через run request. |
| `contracts/native-gate-view.schema.v0.3.2.json` | Composite observed view: symbol/package links, required/selected gate refs, exact result refs, freshness/completeness и graph evidence. Не меняет observed-index или impact JSON в обход их schemas. |
| Embedded target capability definitions | Registry successor `dev.lekalo.target-capabilities@0.3.2`, новый `plan.native-gates`; existing ids сохраняют смысл. Backend/PM statuses внутри plan — typed native capability table, **не** runtime promises в adapter describe. Определения и digests обновить в resolver/lock consumers. |
| `contracts/diagnostic-registry.v0.3.2.json` | New `native-gate.*` stable codes. Общая diagnostic schema остаётся 0.2.16, если closed shape не меняется; registry constants/hash/embedded copy и fixtures переходят согласованно. |
| Authority/privacy successors | Для durable native policy/plan/run/output нужны legitimate registered kinds, см. §3.4; exact reviewed authority и policy successor 0.3.2, новые sidecars/manifests/classification custody. Closed privacy schema не менять, если она уже принимает эти exact refs и kinds через registry; если constants/enums schema меняются, публиковать её successor 0.3.2, не patch 0.2.16. |
| Model/IR/observed-index/observed-scan/graph/impact/target-profile/lock/orchestration payload schemas | Сохраняются прежние версии: #48 не добавляет их wire fields. Existing lock data обновляется при смене protocol/capability refs, что не равно изменению lock schema. Native view/selection не выдаётся за новый canonical impact schema. |

`native_request` содержит bounded `changes: {files:[{path,change,before_digest?,after_digest?}], symbols:[]}`, `scan_ref:{digest,revision,adapter}`, `observed_ref`, `execution_policy_ref`, `input_manifest_digest`, `tool_catalog_ref`, `capability_snapshot_ref`; сами документы поставляются в explicit read scope private view с repository-relative refs и digest. Core до adapter call проверяет происхождение/форму и фиксирует bytes. Adapter не может сам повысить trust, одобрить план или проверить installed host tools через spawning. Input refs — content references, не команды и не абсолютные file URLs.

0.3.1 годится для старого scan и existing semantic references, но не переносит package graph/confirmed command/plan authority. Local-only sidecar без protocol successor не решает end-to-end передачу из отдельного bundled adapter в core; JSON внутри `Finding.detail` или `ScanEntry.detail` был бы обходом closed contract. Альтернатива «весь pnpm planner в Rust, scanner только через 0.3.1» технически возможна, но дублирует TS config/package rules и internal scanner metadata. Здесь она **не** выбрана.

Current-only successor следует current registry policy: product/adapter/protocol 0.3.2 для новой реализации, Model/IR 0.2.16. Не обещать dual-version negotiation, если он не реализован отдельными frozen validators. 0.3.1 peer получает explicit unsupported до plan dispatch; сохранённые schema bytes используются для negative compatibility vectors. `versioning/contracts/version-registry.v0.2.16.json` — embedded registry location, изучить его loader: при изменении собственных registry semantics/identity создать successor, иначе обновить только protocol family publication по принятому #44 механизму. Не делать blanket replacement строк версий.

### 3.2 Состав immutable plan

Обязательные fields (snake_case в native contracts, inherited privacy objects сохраняют свои принятые имена):

- `schema_version`, `kind: "native-plan"`, `plan_digest`, `adapter:{id,version,digest}`, `planner_version`, `canonicalization_version`.
- `repository_role` — stable alias, `trust:{mode,fixture_ref?,provenance_ref}`, exact `classification_ref`, `policyRef`, `authorityRef`; disposition никогда не является единственным proof synthetic trust.
- `execution_policy_ref:{id,version,digest}`, `profile_ref:{id,digest}`, `input_manifest_digest`, `scan_ref`, `observed_ref`, `tool_catalog_digest`, `capability_snapshot_digest`.
- `workspace:{manager,declared_version,compatibility_path,root,manifest_digest,lock_digest_state,packages,edges,completeness,uncertainties}`. Paths normalized relative; root `.` — явный специальный случай, не wildcard grant.
- `changes`, `affected:[{package_id,reasons:[{kind,source_ref,edge_path,policy_rule_ref?}]}]`, `excluded:[{package_id,reason}]`, `selection_mode: targeted|release-full`, `fallback_rule_ref?`.
- `commands:[{id,package_id,gate,script_name,script_digest,confirmation_ref,cwd,tool_ref,argv,env,tsconfig_ref?,depends_on,affected_reason_refs,read_manifest_ref,allowed_writes,limits,provenance}]`.
- `env:{allowed_names,bindings}`. Binding только `literal` из проверенной nonsecret policy или typed relocation token (`execution-temp`, `execution-home`, `platform-system-root`); значения любых inherited variables не подставляются. Exact names **и** binding values/recipes входят в digest.
- `tools:[{id,name,version,artifact_digest,entry_digest?,provenance,platform}]`; manager declaration отдельно от фактически исполняемого tool. Пустая/непроверенная версия = unknown, никогда не успешный executable plan.
- `required_capabilities`, `capabilities:[{id,definition_version,state,source,evidence_ref}]`, `run_eligibility:{state,reason_codes}`, `limits` whole-run, `write_policy`.

Начальные hard maxima: 1 MiB request/policy, 8 MiB response/plan; 4 MiB single input, 64 MiB total inputs, 10,000 entries, 1,024 packages, 8,192 edges, 256 workspace patterns, 128 commands, 64 argv entries по 1,024 UTF-8 bytes, total argv ≤16 KiB; 32 env entries; depth 64 JSON, depth 8 config extends, 32 hops explanation, bounded traversals с visited set. Prefix limits применяются во время чтения/парсинга, не после allocation. UTF-8 fatal, duplicate JSON/YAML keys refused, cycles/unknown types reported. Превышение не превращается в truncated successful plan; меньшее ограничение kernel/backend всегда приоритетно. Windows total command line также проверяется после UTF-16 quoting и relocation.

### 3.3 Digest, approval, run receipt

`plan_digest = sha256(domain || canonical(plan без plan_digest))`, domain/version фиксируются contract. Canonicalization: UTF-8 JSON, ключи сортируются bytewise, только конечные integers в заданных диапазонах, без null вместо absent, arrays canonical order для sets, argv/commands сохраняют semantic order; LF не входит. Rust/JS shared golden byte vectors включают Unicode и key-order adversaries. Digest не включает timestamp/temp absolute root/PID/duration. Все decision-relevant inputs, snapshot, policy, argv/env recipe, tool digests/versions, capabilities и limits включены.

Run request: `schema_version`, `kind:"native-run-request"`, `plan_ref`, `approved_plan_digest`, `approval:{mode:explicit|checked-in-policy,subject_ref,policy_digest,rule_ref}`, `fixture_catalog_ref`. Approval лежит **вне** хешируемого plan: plan не может одобрить сам себя. Checked-in policy не autorun любого будущего digest: конкретный run request обязан назвать конкретный digest, проверенный policy evaluator. Command overrides, ambient additions и mutable plan recomputation при run запрещены.

Runner re-decodes approved bytes, independently recomputes digest, verifies live fixture catalog identity/source bytes, all input/tool/policy/config/script digests, stage identity, capability evidence. Drift → `plan-stale` до первого gate spawn; нужен новый plan и approval. Для sequence commands предусмотренные writes предыдущих gates не считаются случайным source drift, но изменение protected config/source/policy/tool запрещено; generated prerequisites перечислены как plan dependency outputs.

Receipt: `kind:"native-run-result"`, `plan_digest`, `execution_policy_ref`, privacy/authority refs, `outcome`, `reason_codes`, `commands:[{command_id,package_id,cwd,tool_ref,argv,env_names,env_recipe_digest,tool_version,exit,duration_ms,outcome,reason_codes,output_ref}]`, `mutation_summary`, `original_verification`, `cleanup`, `capability_evidence`, `provenance`. Exit/duration/count measurements используют #120 `valueState`: never-executed имеет unknown/unsupported/withheld, а не exit=0/duration=0. Реальное нулевое значение — known zero. `argv` переносится только после проверки отсутствия secrets/host paths; приватный original plan не публикуется автоматически.

Closed outcomes: `passed`, `failed` (обычный nonzero native gate), `missing` (confirmed script/tool отсутствует), `blocked` (policy/trust/approval/network requirement), `unsupported` (manager/platform/shape capability), `infrastructure` (spawn/crash/deadline/cancel/flood/cleanup), `security` (unexpected write/original mutation/env escape). Reason codes различают `timeout`, `cancelled`, `output-limit`, `process-tree-cleanup`, `cleanup-failed`; terminal result фиксируется ровно один раз. Security/cleanup defects не маскируются первоначальным gate exit; secondary causes сохраняются.

### 3.4 Privacy/authority closure — обязательный preflight

Не называть native plan `ai-factory.execution-plan`, не писать runner output от имени владельца `hlv` и не вводить shadow kind вне #2. Рекомендуемые новые stable kinds: `lekalo.native-gate-policy` (canonical), `lekalo.native-execution-plan` (derived), `lekalo.native-gate-result` (direct-evidence), `lekalo.native-gate-output` (direct-evidence). Successor authority определяет writers/readers/exact `.lekalo/native-gates/{policies,plans,runs,output}/**` boundaries и classification; privacy successor даёт каждому один default, по умолчанию consumer-repository-only для checked policy и local-private для plans/results/output. Synthetic fixture evidence получает public-fixture **только через допустимую exact provenance/classification**, без снижения sensitivity.

В implementation сначала проверить full authority+privacy evaluator vectors для новых kinds и refs; accepted vocabulary/precedence не менять. Policy document, manifest, classification document, authorizing evidence/subject profile и все exact-reference consumers пересобрать там, где сменился authority/policy digest. Это существенная, но необходимая contract addition для durable artifacts, а не реализация runtime export pipeline #119. Если successor taxonomy не принят, persist/export новых artifacts BLOCKED; это настоящий acceptance blocker, не повод присвоить похожий existing kind.

Raw streams — только bounded local memory/temp sink с exact classification; public JSON содержит redacted output artifact reference/digest либо withheld valueState. Redaction создаёт derived artifact с source refs и новой evaluation; замена абсолютного cwd не делает весь stderr публичным. Для private plan никакие скрипты, usernames, package identities или raw argv не уходят в public report без policy decision. Absolute executable/stage paths и private root binding живут в local-only sidecar; portable evidence содержит repository-relative cwd, argv paths и role alias.

## 4. Workspace/package graph и affected selection

1. **Read authority.** Project root/profile/read roots приходят по accepted trusted launch seam; `pnpm-workspace.yaml`, relevant package manifests/configs и fixture scripts должны быть явно в inventory. Никакого подъёма до ближайшего parent workspace. Link/junction/reparse, escapes, case-colliding logical paths, special files, excluded dirs и oversized entries отвергаются до content reads. Existing scanner excludes сохраняются; #48 не начинает читать node_modules.
2. **Membership.** Parse YAML `packages` в объявленном subset: canonical relative literals, `*`, `**`, `?`, leading `!`; dot dirs только explicit. Root included при valid root manifest. Pattern syntax сверх subset → capability partial/unsupported с причиной, не silently approximate. Traversal в bounded in-scope inventory, не glob ambient FS. Empty/missing `packages` behavior фиксируется compatibility fixture. Дубликаты roots/names — ambiguity, не first-wins. YAML tags/anchors/env expansion/hooks не исполняются.
3. **Manager selection.** Exact confirmed `packageManager`/profile выбирает route; lockfile только evidence, не команда. Conflicting pnpm/yarn/npm/bun markers → ambiguous manager. Declared pnpm version и compatibility rule включены в план; отсутствующий tool не скачивается через corepack. Для unknown future version можно показать plan с unsupported semantics, но не runnable assurance.
4. **Edges.** Package id domain содержит relative root и package name, соответствует locator #44; stable native ids не пересчитываются новым алгоритмом. Собрать typed edges consumer→dependency из dependencies/devDependencies/optionalDependencies/peerDependencies с condition/scope/provenance, TS references, scanner imports/calls и explicit target bindings. `workspace:*`, `workspace:^`, `workspace:~`, bare/alias/relative формы либо поддержаны и tested для version rule, либо явно refused. Relative spec нормализуется внутри workspace до canonical root; `..` допустим в **input dependency syntax**, но не в public resolved path/cwd. Missing target/version mismatch не игнорируется.
5. **Resolution confidence.** Plain semver name match, `file:`/`link:`, catalogs, exports/paths и peers не считать доказанным local edge без lock/config/scan evidence, достаточного для declared support path. Unknown external relation сохраняется как uncertainty. Optional/platform predicates фиксируются в plan. Cycles выделяются SCC; reachability может быть известна, но build ordering в цикле требует explicit policy или блокирует sequence.
6. **Join scanner→workspace.** Internal locator root/name и module file проверяются против inventory digest. На core стороне observed source location/stable native key дают independent cross-check, `evidence.references` направлены на semantic ids и разрешаются через binding index. Нет декодирования хешированного `ts1-*` в package name. Несовпадение revision/profile/manifest/signature→stale refusal. Inferred/low confidence — evidence, не confirmation command. Неизвестный reference не превращается в отсутствие dependency.
7. **Changed roots.** Changed files/symbols передаёт caller; planner не запускает git. Deleted/renamed packages требуют old snapshot/before digest; без него selection incomplete. Root config/lock/shared file имеет explicit ownership/policy reason; нельзя автоматически выбрать весь repo. Packages отсутствующего/ambiguous owner оставляют plan blocked либо применяют разрешённую release fallback.
8. **Affected closure.** Starting packages + reverse dependency transitive closure + graph-bound tests. Selected build prerequisites добавляются по forward edges отдельно с reason `build-prerequisite`; это не разрешает unrelated tests. `planner → api consumer → cli consumer` выбирает эти пакеты; unrelated web/integration без edge или policy reason остаются excluded. Deterministic sorted traversal, bounded paths, SCC-aware ordering, одинаковые cold/warm results.
9. **Completeness.** #44 capped reference set и observed incomplete не дают доказательство closed-world graph. Полный fixture catalog плюс package graph/config edges может доказать completeness **в пределах synthetic fixture**. Для arbitrary repo план показывает uncertainty; без достаточного coverage targeted-run eligibility blocked. Unknown не означает empty affected set.
10. **Full fallback.** Только `release-full` при exact checked-in release rule, trigger/change scope/explicit gate set и recorded rule digest; не auto fallback на missing tool, failed test, unsupported manager, network gap или untrusted repo. Такой plan имеет другой digest и отдельно одобряется. Fallback не расширяет trust/permissions.

Package graph входит в `NativePlan.workspace.edges` и `NativeObservedView` как impact evidence с причина→path→package→command linkage. Existing `impact/gate.rs` получает read-only association helper, если полезно, но его нейтральные GateItems и контракт `impact.schema.v0.2.16` не становятся native receipts. Если впоследствии потребуется добавить package edges именно в canonical graph/impact wire, это отдельные justified successors 0.3.2; в данном плане такой scope не выбран.

## 5. Confirmed scripts, tools и manager paths

Policy разрешает только gate kinds build/typecheck/lint/test и exact package+script+manifest hash+tool recipe. Существование script с именем `test` не является confirmation. Entry содержит source profile/config reference, rule version/digest, exact script string digest, explicit argv и tool digest/version. CI использует только checked-in confirmed policy; отсутствие/изменение policy блокирует, не вызывает prompt.

Для M3 принимается узкая shell-free форма existing scripts, например `node gates/typecheck.mjs --project tsconfig.json`: literals без interpolation, metacharacters/redirections/pipes, command substitution, assignment prefixes, response files, shell executables, `.cmd`/`.bat` или nested PM invocation. Parser сравнивает **полную** строку с подтверждённой argv recipe, а не ищет известный prefix. Quoting либо поддерживается собственным строго определённым literal subset с parity vectors, либо отказ. Если script требует shell, structured unsupported: его нельзя «улучшить» изменением package.json или приблизительной разбивкой по пробелам.

Confirmed direct tool (`tsc`, linter/test entry) допускается только через заранее provisioned immutable artifact catalog; Node запускает конкретный JS entry, минуя bin shims. Config JS/plugins — executable code: в M3 допустимы только synthetic catalog hashes, никакого импорта arbitrary project config в planner. `node --eval`, `--require`, `--import`, `--run`, loaders, option injection и произвольные Node flags не получают разрешение через имя tool. Fixture canary spawning нужен для process-tree tests, но является отдельным заранее reviewed fixture artifact, не разрешением scripts запускать произвольные tools.

Package-local tsconfig выбирается из confirmation (`tsconfig_ref`) либо однозначного package-local `tsconfig.json`, никогда не global/default workspace config для всех packages. JSONC/extends/project references разбираются через vendored TS read-only host и bounded config chain; missing/escaping/package-resolved configs в excluded node_modules дают unsupported/unknown. Каждый command ссылается на точный config/extends manifest digest. Current scanner объединяет options для своего Program; это **не** авторитетный готовый per-package tsc invocation. Отдельно получить config каждого package. В verified direct `tsc` recipe обязателен exact `--project` или проверенный build-reference список; `.tsbuildinfo`/emit/cache outputs включены в write policy.

| Capability path | M3 обещание |
| --- | --- |
| `pnpm-workspace` | Bounded workspace detection/graph/selection; real fixture execution direct pinned tools. pnpm CLI не запускается. Unsupported layout/lock semantics остаются gap. |
| `npm-standalone` | Separate one-package detection+policy/config route; synthetic standalone fixture реально исполняет confirmed direct Node/tool command, без `npm run`/install. Это требуемый AC13. |
| `npm-workspaces` | Отдельное unsupported/partial capability, если parser/fixtures не включены явно; не притворяться pnpm. |
| `yarn` | Explicit detection + plan-only unsupported recipe при PnP/plugins/unimplemented workspace semantics; не запускать `.pnp.cjs`. Full Yarn support не acceptance claim M3. |
| `bun` | Explicit detection + plan-only unsupported runtime/lock path до независимых fixtures; не подменять Bun Node-ом. |

Tool versions в execution receipt — реально использованные Node/tool artifacts; pnpm manifest version — declared manager metadata с provenance, не «запущенная версия». Probing произвольного `tool --version` также execution: planner не делает его. Fixture catalog versions проверяются в trusted provisioning и при runner preflight внутри того же restricted backend; changed binary digest/version блокирует approved plan. Runtime dependencies provisioning — отдельный developer/CI setup до run, не часть plan/runner; missing dependency не вызывает install/update.

## 6. Fixture-only execution, platform capability и cleanup

### 6.1 Отдельные состояния

`plan → validate → approve exact digest → fixture trust check → capability preflight → disposable copy → launch → reap all descendants → compare writes/original → redact reference → cleanup → terminal receipt`.

В production `native run` заканчивается до fixture execution: private/untrusted → blocked `confinement-required`; известный synthetic plan → unsupported `fixture-runner-not-shipped`. Реальный harness compiled только `#[cfg(test)]` и принимает trusted catalog entry, не произвольный root/`--trust public-fixture`. Caller-controlled marker/README/public repository недостаточны. Требуются #120 disposition, synthetic origin, provenance, exact fixture catalog/source digest и accepted refs. Explicit permission/license не заменяет synthetic origin в этой более узкой M3 execution policy.

### 6.2 Disposable root и custody

- Новый owned temp root вне original workspace; bounded manifest-driven byte copy обычных файлов/директорий, никакого hardlink/reflink-by-default/shared package store. Reject symlinks/junctions/reparse/alternate streams, special files и case aliases; OS-backed reads и повторная identity verification закрывают path substitution. В runner нет recursion по caller-supplied absolute deletion path.
- Before-copy original snapshot фиксирует **весь synthetic fixture root**, включая source/config и hidden ordinary files, не только tracked/git diff. Source snapshot после copy должен совпасть; иначе stale. В stage отдельно immutable inputs/tools/policy и exact allowed output dirs. Секреты/ambient caches/home не копируются.
- Physical cwd — strict descendant stage root, полученный из `command.cwd`; original root никогда не cwd и не mounted writable. `.` разрешён только standalone/root package, mapped на stage. Local sidecar связывает approved logical command с фактическим OS executable/cwd; public evidence не содержит absolute root.
- Executable staged по content digest в trusted runtime root; argv уже окончательный literal vector, относительные script/config args проверяются от approved cwd. Stage-only temp/home/cache bindings разрешаются по recipe до launch и проверяются против digest-bound relocation semantics. Никакого PATH lookup, Corepack shim, automatic Node flag addition или shell wrapping.
- Plan write policy перечисляет exact relative files либо bounded output roots + allowed actions/count/bytes; она не обещает заранее content hash nondeterministic log/build output. Receipt записывает фактические created/modified/deleted paths+digests. Не использовать generation WriteEntry, которому нужен expected output content hash, для dynamic build logs.
- After run **после завершения всех descendants** сравнить whole-stage tree и original snapshot. Outside-allowlist write — security even при exit 0; original drift — security/unverifiable, never passed, без автоматического revert original. Original snapshot read failure означает unverifiable, не unchanged. Проверка фиксирует наблюдаемое состояние; она сама по себе не доказательство sandbox от adversarial transient writes — #89 остаётся владельцем общего confinement.

### 6.3 Env/network/processes

Env начинается пустым. Deny sensitive variables, PATH/NODE_PATH/NODE_OPTIONS, npm config/token, proxy credentials, SSH/cloud/provider vars независимо от ambient machine. Windows required system bindings берутся из trusted platform resolver, versioned recipe содержит имена; home/temp перенаправлены в stage. Unknown env names или case-insensitive дубли на Windows отвергаются. Approved recipe фиксирует nonsecret literal values и relocation bindings; «только имена» недостаточно против TOCTOU.

Network denial required=true для fixture run, даже если тесту сеть не нужна. Отсутствие npm install, env proxies, DNS failure, offline flag или удачный отрицательный request сами по себе не доказывают denial. Перед launch нужен доступный enforcement backend, зафиксированный capability receipt и negative canary в том же boundary. Probe failure/unknown → blocked `network-denial-unavailable` до любого native gate. Не добавлять network allowlists в M3.

| Платформа | База/обязанность #48 | Gap behavior |
| --- | --- | --- |
| Linux | Reuse bwrap network/pid isolation из существующего backend. Проверить user namespaces/bwrap mount/cwd/env support в fixture job. Parent-death containment + namespace/process ownership, bounded group kill/reap; process group один не является доказательством containment daemonized descendants. | Нет нужного enforcement → capability unsupported и blocked run. Не заменять на обычный spawn/setsid. |
| Windows | Reuse LPAC no-network capability setup + explicit handle/env list; CREATE_SUSPENDED → Job assignment → resume, kill-on-close и запрет breakaway; дождаться отсутствия descendants. Validate/CreateProcess quoting parity для backslashes/quotes/Unicode; `.bat/.cmd` запрещены. | LPAC/AppContainer/ACL/Job setup либо required write/cwd support недоступны → blocked. Job Object сам по себе не network denial. |
| macOS/other POSIX | Existing macOS sandbox-exec route требует фактического preflight; нельзя считать все POSIX одинаковыми. Reuse only proven backend for exact fixture recipe. | Отсутствующий/неприменимый backend → blocked/unsupported, без обещания полноценной cross-platform реализации #89. |

No new general sandbox, containers, seccomp policy engine, untrusted filesystem/network/resource confinement или install service. Если existing backend не способен выполнить exact fixture recipe, это фиксируется как gap; AC7/9 остаются открытыми до compatible fixture/backend evidence, а не закрываются skip-ами.

### 6.4 Bounded lifecycle

Initial defaults: 30 s/command, 120 s/run, 5 s terminate/reap budget; stdout 64 KiB, stderr 64 KiB, combined 128 KiB/command, 1 MiB/run. Policy может сузить; widening не превышает contract ceiling и входит в new approval. Capture одновременно drains оба streams bounded chunks; binary/invalid UTF-8 не ломает transport, public decoder использует safe redacted representation. Никакого unbounded read-to-end после kill или ожидания pipe EOF от оставшегося grandchild.

Cancellation до launch→zero spawn; during run→terminate owned tree→bounded reap→mutation audit→cleanup. Timeout, flood, child crash, spawn failure и orphan descendants — distinct reason codes. Parent exit 0 с живым child не terminal success. Cleanup вызывается после success/failure/cancel/deadline/flood и observable crash; temp handle ownership и cleanup outcome фиксируются. Crash recovery использует только owned run journal/identity и живость tree, не удаляет произвольные old temp dirs. Power loss/unobservable abrupt death не может честно обещать синхронную cleanup; следующий harness preflight доказывает cleanup либо возвращает unknown/failed. Этот предел явно записать в docs.

## 7. Test matrix и acceptance mapping

Все перечисленные тесты — план работ, **NOT RUN в этом research**. New fixtures invented/public-fixture, без private consumers. Real runtime vectors не заменяются mocked runner receipts.

`tests/fixtures/node-native-gates/pnpm-monorepo/`: root + `packages/planner`, `packages/api`, `packages/cli`, `packages/web`, `packages/integration`, confirmed literal scripts, local tsconfigs/extends, fixture policy/catalog. api depends on planner, cli on api; web/integration независимы. Gate commands создают per-package markers только в разрешённых stage outputs. Отдельная npm-standalone fixture. Fixture gate implements real deterministic checks (например Node tests и проверка fixture source/config), а не безусловный exit 0 с именем typecheck; для claim real TypeScript typecheck — provisioned pinned compiler entry и намеренно invalid TS negative vector.

| AC из live #48 (порядок checkbox) | Code seam | Проверка / условие закрытия |
| --- | --- | --- |
| 1. Planner change selects affected consumers/tests | `workspace.mjs`, `native-plan.mjs`, core `selection.rs` | Planner-only source change → planner/api/cli и graph-bound tests; reverse transitive edges, TS refs, renamed/deleted old snapshot, SCC vectors; markers фактического исполнения соответствуют plan. |
| 2. Unrelated web/integration not run without reason | selection + runner | No marker для обоих; добавить реальную edge/policy rule и доказать появление единственного justified command. Poison scripts в excluded packages никогда не запускаются. |
| 3. Full fallback only explicit release policy | `policy.rs`, plan digest | Отсутствующая/не-release/mismatched rule → blocked; точная checked release rule → new full plan с recorded reasons+digest; runtime failure/network gap не trigger fallback. |
| 4. Approved digest equals cwd/argv/env/tools | `digest.rs`, runner preflight | Tamper каждый field, script, config, lock, executable и policy после preview; каждый stale run zero-spawn. Cross-language byte vectors; record actual launch mapping/tool hashes/env recipe и сравнить с approval. |
| 5. Direct argv, no shell injection | runner + literal parser | Shell metachar/string substitution rejected; literal hostile-looking arg никогда не исполняет второй command. Windows quote/backslash/Unicode parity; sh/cmd/powershell/PM/bin-shim deny; source/bundle import audit. |
| 6. Unconfirmed/lifecycle/install fail closed | `policy.rs`, gate resolver | Script-name only, changed hash, pre/post hooks, prepare/install/update/corepack/npx/dlx, shell config/plugins → no execution. Root+excluded poison marker absent. |
| 7. Real pnpm synthetic disposable run, original unchanged | fixture runner/backend | Capability-supported OS: реальные commands, writes только stage, whole source/config before/after digest equal, temp removed. **Нельзя закрыть, если все jobs blocked/skipped.** |
| 8. Private/untrusted plan-only | production guard + harness catalog | Valid plan даже с caller `public-fixture` marker не проходит catalog/trust; zero gate launches. Production binary не содержит runner escape flag. |
| 9. Timeout/flood/process tree classified+cleaned | lifecycle support | Infinite wait, simultaneous stdout/stderr flood, parent exits while grandchild holds pipe, crash/cancel, detached child probe; bounded elapsed, no survivor, cleanup proof. **Каждая заявленная OS требует live result**, unavailable platform остаётся gap. |
| 10. No sensitive env; honest network capability | env builder/backend preflight | Parent seeded token/NODE_OPTIONS/proxy/SSH canaries отсутствуют в child; no output leak. Enforced network negative fixture + backend absent vector: second reports blocked, zero gate launches. Denial signal без backend не считается proof. |
| 11. Distinct missing/failed/blocked/unsupported | diagnostic registry + receipt | Missing confirmed script, real nonzero gate, policy denial, Yarn/Bun unsupported path дают разные stable code/outcome; not-run exit state never 0. |
| 12. JSON evidence relative cwd/argv/exit/duration/digest/policy/mutation/redacted ref | `wire.rs`, `evidence.rs`, schemas | Ajv+Rust parity, absolute/UNC/URI/home/env secret output probes, redaction derived refs, malformed/unknown/duplicate fields refused, null/nonfinite rejected, mutation and cleanup failure propagate. |
| 13. pnpm monorepo + standalone npm fixtures | fixtures + manager routing | Both independent positive plans and real direct commands under supported backend; no pnpm-as-npm fallback. Real npm-standalone behavior required, detection-only недостаточно. |

Дополнительные обязательные vectors:

- Invalid YAML/JSONC/duplicate keys, huge packages/globs/references, missing workspace config, excluded root attempts, symlink/junction during copy/read, hardlink source alias, package-name collision и missing dependency.
- Shared config/lock change, scanner partial/over-8 reference refusal, stale binding/current byte mismatch, unresolvable native→semantic ref, no changes, empty affected plan (not a passed full gate), cold/warm and two roots digest parity.
- Stage mutation outside allowlist, deletion of input, original concurrent change, cleanup failure and file held open, child survives parent, invalid UTF-8 output. No original restoration masking failure.
- Protocol 0.3.1 rejects new operation/fields; 0.3.2 strict correct op coupling, no verify plan_id, no write publish from native plan; unknown capability ids refuse. Adapter declare-only without trusted profile stays honest.
- Approval confusion: valid digest alone without authorizing approval, approval to different source root with identical public plan, stale local binding, reused run journal, changed capability snapshot. Private local root binding не раскрывается в public digest/evidence.

**M3 closure limit.** Ни один checkbox не закрыт этим документом. Все 13 достижимы в указанном synthetic scope при наличии хотя бы одного реально работоспособного backend и честного platform matrix; AC7/9/13 требуют реального исполнения, AC4/10 — фактической launch/env/isolation evidence. Все-platform arbitrary repo execution, full Yarn/Bun behavior и #89 confinement нельзя объявить delivered. Если scope «support npm/yarn/bun» трактуется как full execution всех manager semantics, это не закрывается данным M3: нужен отдельный согласованный расширенный capability implementation и fixtures, не optimistic support flag.

## 8. Последовательность implementation и проверки

1. **Reserve product 0.3.2 отдельно**, regenerate custody и пройти reserve gates (§9).
2. Принять contract design: schemas, exact operation/field bounds, authority/privacy kinds+refs, diagnostic/capability definitions. Add positive/negative vectors до использования protocol. Никаких existing contract edits in place.
3. Workspace detection/package/config graph, pure selection и confirmed command parser; Node fixture unit tests, independent Rust validation и canonical digest parity. No runner на этой стадии.
4. `plan-native` bundled seam через real TargetClient + CLI plan-only, observed composite evidence; test snapshot stale/refusal and original read-only behavior. Single-file relocation/build reproducibility gates.
5. Test-only runner с trusted catalog, explicit approved plan, disposable copy, existing enforcement preflight; сначала successful offline fixtures, затем timeout/flood/env/tree/mutation cases. Unavailable backend — blocked recorded, не unconfined retry.
6. End-to-end AC evidence, docs/support matrix, review exact candidate SHA. Fixes проходят новую independent verification; green goldens не заменяют runtime adversarial tests.

Proposed focused commands после реализации (development provisioning заранее, отдельным разрешённым setup; здесь не запускались):

```text
node adapters/node-typescript/build.mjs --check
node scripts/test-node-typescript-kernel.mjs
node scripts/test-node-typescript-scanner.mjs
node scripts/test-node-native-gates.mjs
node scripts/test-native-gate-contracts.mjs
cargo test --locked -p lekalo-core --test native_gate
cargo test --locked -p lekalo-core native_gate::fixture_tests
cargo test --locked -p lekalo-core --test node_typescript_kernel --test node_typescript_scanner --test target_protocol --test target_protocol_boundaries --test observed
cargo test --locked -p lekalo-cli --test native_gate
node scripts/test-target-protocol-contracts.mjs
node scripts/check-contract-versions.mjs
node scripts/test-contract-versions.mjs
node scripts/test-versioning-contracts.mjs
node scripts/check-authority.mjs
node scripts/check-privacy.mjs
node scripts/test-authority-contracts.mjs
node scripts/test-privacy-contracts.mjs
node scripts/test-privacy-schema-parity.mjs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
git diff --check
```

Также default/strict adapter conformance real bundle + configured profile и strict reference fake adapter. Existing strict `capability.surface` expected failure из-за неполной generation/scenario surface остаётся ожидаемым; не регистрировать fake verify/generate ради PASS. `.github/workflows/ci.yml` добавить pure Node/contract/Rust suites и explicit platform fixture jobs; stdout receipt содержит PASS/BLOCKED/UNSUPPORTED per capability. В required executable job absence backend является непрошедшим runtime gate, а не обычным skip. Validation report хранит exact SHA, tool versions, approvals/digests, per-AC results и local/public artifact distinction.

## 9. Reserve commit 0.3.2

Повторить смысл accepted `3e4159d`/`scripts/reserve-030-regen.mjs` и `86330ef`/`scripts/reserve-031-regen.mjs`, не их устаревшие literal assumptions.

- `Cargo.toml [workspace.package].version`, локальные `lekalo-core`/`lekalo-cli` entries `Cargo.lock` → 0.3.2.
- Adapter `ADAPTER_VERSION`, `package.json`/`package-lock.json` own metadata и version assertions → 0.3.2; protocol VERSION на reserve этапе **ещё 0.3.1**. Rebuild generated bundle, не ручной patch artifact.
- Product-sensitive tests `crates/lekalo-cli/tests/{cli,authorization,context,diff,effects,graph,impact,inspect,requirements,lock}.rs`, `crates/lekalo-core/tests/lockfile.rs`, kernel/scanner tests обновляются только там, где literal является product/adapter version.
- Добавить `scripts/reserve-032-regen.mjs`: assert fresh binary 0.3.2 (`LEKALO_BIN` поддержать), новые external disposable copies, regenerate `tests/fixtures/lockfile/valid/contract-only.lock.json`, `.expect.json` (оба lockDigest/payloadSha256), `GOLDEN_DIGEST` в core lockfile и CLI lock tests, orchestration generate dry-run/apply/verify golden receipts.
- В reserve fixture `core.version=0.3.2`, protocol contract=0.3.1, resolver=0.2.16, unchanged family versions неизменны. Проверить LF, final newline и canonical hash domain; fixture normalization только утверждённых nondeterministic fields.
- Reserve commit **без contract changes**. Затем feature contract commit вводит необходимые successors 0.3.2 и coherent consumers/registry/cache keys/lock receipts; повторная regeneration уже должна ожидать protocol=0.3.2. Не переписывать первоначальный reserve evidence как будто оба шага были одним.
- `node scripts/check-contract-versions.mjs --base HEAD^` при каждом commit и checker против accepted base в final branch review; семантические schema tests обязательны, checker сам по себе не доказывает отсутствие незаконного contract drift. Existing 0.3.1/0.2.16 artifacts не редактировать; superseded files можно вывести из active references по принятой repository convention, history/байты не переопределять.

## 10. Риски, implementation decisions и out of scope

1. **Главный delivery риск — protocol/privacy/authority closure.** Execution artifacts не вмещаются ни в текущий wire, ни в похожие чужие artifact kinds. Preflight contract stage обязателен, иначе реализация может быть только internal experiment без закрытого AC12. Concrete successor proposal дан выше; не прятать вопрос под opaque metadata.
2. **Existing sandbox reuse может оказаться уже, чем gate recipe.** Backend сейчас заточен под protocol process, fixed project cwd и limited staged runtime. Сначала подтвердить test-only adaptation на Windows/Linux; не переписывать #89 в рамках #48. Неудача означает capability gap и незакрытый real-execution AC.
3. **Package-local config не равно объединённым scanner Program options.** Требуется distinct per-package config manifest. Paths/exports/TS refs/external deps без sufficient evidence остаются uncertainty; ни npm install, ни fallback whole workspace не исправляют её автоматически.
4. **Toolchain custody.** Согласовать exact synthetic fixture Node/TS/manager metadata pins с CI matrix; provisioned binary должен работать в existing LPAC/bwrap backend. Не выбирать «latest» во время run. Package manager может быть declared but not executed; это явно видно в receipt.
5. **Неполная поддержка pnpm syntax и Yarn/Bun.** Pin compatibility table и перечислить accepted glob/lock/specifier forms в docs. Если full manager compatibility требуется для issue closure, coordinator должен расширить capability scope/fixtures; без этого отчёт должен назвать gaps.
6. **TOCTOU и normalization.** Digest binds recipe, а absolute physical root локален. Нужны independent host validation, private root binding и actual-launch evidence; serializing candidate plan и потом заново resolving PATH/scripts нарушает invariant.
7. **Platform-specific limitations.** Process-group kill не полный sandbox, Job Object не network denial; negative network probe без enforcement не доказательство. Неизвестное состояние cleanup/original snapshot не маркируется known zero/unchanged.
8. **Research verification scope.** Выполнены read-only repository/issue/source/docs inspection и проверка существования predecessor reports; runtime tests/CI/build не выполнялись. Итоговый файл под `.m3/` остаётся untracked. Historical review test counts не являются свежими gates #48.

Явно вне scope: #89 general confinement и private/untrusted execution; arbitrary shell/scripts/plugin configs; install/update/lifecycle hooks/Corepack downloads; source/config generation, mutation или script replacement; package publication/network allowlists; persistent cross-session scanner cache; full run history #121; export/redaction platform #119 и public aggregates #102; расширение canonical Model/IR; автоматическое закрытие AC по результатам этого research.
