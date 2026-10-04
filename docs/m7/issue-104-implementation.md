# Issue #104: подготовка релиза 0.6.5

Проверено 2026-10-04 в `D:/Projects/lekalo`, ветка `main`.
Основание: [issue #104](https://github.com/ichinya/lekalo/issues/104) и
задание `fix-104.txt`. Этот документ входит в один локальный release-prep
коммит вместе с реализацией. Полное принятие #104 и публикация не заявляются.

## Обнаруженный дефект

Опубликованный [GitHub Release 0.6.4](https://github.com/ichinya/lekalo/releases/tag/v0.6.4)
имеет тег `v0.6.4`, указывающий на коммит
`9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`. В его `Cargo.toml`
`workspace.package.version` равна `0.6.3`; оба Cargo packages наследуют
эту версию. Локальный Windows release-бинарник, собранный из этого коммита
до смены версии, также выводит `lekalo 0.6.3`.

Это подтверждает расхождение версии опубликованного релиза/тега с версией
его исходников и соответствующего бинарника. На момент проверки REST API
показывает у релиза `0.6.4` пустой список assets: отдельный бинарник к нему
ещё не прикреплён.

Доказательства: `git show v0.6.4:Cargo.toml`,
`git rev-parse 'v0.6.4^{commit}'`,
`target/x86_64-pc-windows-msvc/release/lekalo.exe --version` и
`gh api repos/ichinya/lekalo/releases/latest`.

## Решение о версии 0.6.5

`0.6.5` — следующий patch после уже опубликованного `0.6.4`. Новый выпуск
позволяет согласовать версии и включить release workflow в релизный коммит,
сохранив существующие публичные теги и историю.

- [Cargo.toml](../../Cargo.toml) задаёт единую версию `0.6.5`;
  [Cargo.lock](../../Cargo.lock) фиксирует её для `lekalo-cli` и `lekalo-core`.
- CLI берёт версию из `CARGO_PKG_VERSION`; собранный executable выводит
  `lekalo 0.6.5`.
- Обновлены version probes в CLI tests и версия продукта в
  [документации](../versioning.md).
- Через настоящий CLI перегенерированы contract-only lock и зависимые
  receipts orchestration. Смена продукта меняет `resolver.request_digest`,
  lock digest и ownership-manifest digest. Обновлены sidecar и Rust pins;
  входные Model/IR contracts и содержимое генерируемых файлов сохраняют
  свои прежние digest/version identities.

## Проверки и workflow

[scripts/release.mjs](../../scripts/release.mjs) реализует следующие этапы:

1. `check-version` проверяет, что `v<VERSION>` совпадает с версиями обоих
   Cargo packages, полученными через `cargo metadata --locked`.
2. `prepare` проверяет опубликованный release, его название (`<VERSION>`
   или `v<VERSION>`), Cargo versions и чистоту tagged checkout, затем
   передаёт SHA исходников в build jobs.
3. `package` требует соответствующие ОС и архитектуру, закреплённый SHA
   и чистые исходники. Он проверяет `--version` и `--help`, создаёт архив
   только с executable и README, распаковывает его, сравнивает binary SHA-256
   и запускает `--version`, `--help`, `compatibility` вне checkout.
4. `collect` требует весь набор из пяти архивов и пяти `.build.json`,
   проверяет версии, targets, source commits и SHA-256, затем создаёт
   `SHA256SUMS.txt`. Лишние файлы, неполный набор и смешанные builds отклоняются.
5. `publish` повторно проверяет release и SHA тега, загружает точный список
   assets, скачивает их обратно и сравнивает с локальными файлами.

В [обычном CI](../../.github/workflows/ci.yml) добавлен job
`Release tag and Cargo version parity` для push `v`-тегов. Regression suite
запускается также в существующем OS matrix CI. На этом checkout
`check-version` принимает `v0.6.5`, а `v0.6.4` отклоняет с exit code `1`.

[Release workflow](../../.github/workflows/release.yml) запускается по
`release: published` или вручную для существующего опубликованного тега.
Он содержит native build jobs для Windows x64, Linux x64/ARM64 и macOS
Intel/Apple Silicon. Все исходники собираются из SHA, полученного из тега,
с `cargo build --release --locked`. Publish job зависит от всех build jobs.
При ручном запуске checkout скриптов отделён от checkout исходников, чтобы
можно было собирать старые согласованные теги.

`.build.json` содержит source/workflow commit, target, product version,
Rust compiler details, SHA-256 Cargo.lock, executable и архива, ссылку на
Actions run. Это unsigned build metadata; signed attestation и проверяемая
побайтовая воспроизводимость пока не реализованы.

## Проверки перед коммитом

Все обязательные команды из задания выполнены на текущих исходниках:

| Проверка | Результат |
| --- | --- |
| `cargo build --workspace --locked` | PASS |
| `target/debug/lekalo.exe --version` | PASS: `lekalo 0.6.5` |
| `cargo test -p lekalo-cli --locked --test cli --test lock` | PASS: 6 CLI + 8 lock tests |
| `cargo test -p lekalo-core --locked --test lockfile` | PASS: 21 tests |
| `node --test scripts/release.test.mjs` | PASS: 8 tests |
| `git diff --check` | PASS: whitespace errors отсутствуют |

Node suite проверяет настоящий Cargo workspace и отказ при неверном теге,
несовпадение release title/Cargo versions, обязательность обеих packages,
неполный набор платформ, повреждённые архивы, лишние файлы, смешанные commits,
ZIP round trip на текущей Windows ОС и несовпадение скачанных файлов.
Fixtures полного набора assets в этих тестах синтетические; они не являются
доказательством реальной сборки всех пяти targets или загрузки на GitHub.

## Evidence по acceptance criteria #104

| Acceptance criterion | Реализация и доказательства | Статус |
| --- | --- | --- |
| Binaries run on supported OS matrix | Пять native targets в release workflow; локальная Windows-сборка `0.6.5` и CLI tests прошли. После extraction workflow запускает executable на его native runner. | Частично: hosted Linux/macOS/ARM64 builds и полная release matrix ещё не выполнены. |
| Version parity test blocks bad release | `validatePackageVersions`, `validateRelease`, executable version probes; `check-version` отвергает неверный тег; 8 Node tests прошли; guard включён в tag CI и release preparation. | Локально подтверждено; hosted CI ещё не запускался для нового коммита. |
| Every archive has checksum and provenance metadata | `package` пишет `.build.json`, `collect` создаёт SHA-256 для архивов и records; отрицательные collection/download проверки проходят. | Механизм подготовлен; опубликованных архивов `0.6.5` нет, signed provenance отсутствует. |
| Installation examples verify checksum before extraction | [docs/releases.md](../releases.md) содержит PowerShell, Linux и macOS команды с SHA-256 проверкой перед распаковкой. | Документировано; установка из реально опубликованного `0.6.5` ещё не проверена. |
| Old compatible releases remain downloadable | Workflow работает только с указанным release и одноимёнными assets; старые теги/releases не удаляются. Существующий `0.6.4` проверен через API. | Сохранение истории предусмотрено; скачивание и совместимость старых бинарных архивов не проверены. |
| Self-update never runs automatically and supports `--check` | Установка/обновление/откат в документации выполняются вручную с checksum verification. | Автоматическое обновление не добавлено; команда self-update с `--check` не реализована. |
| Release notes list Model/IR/protocol compatibility ranges | `target/debug/lekalo.exe --json compatibility` подтверждает ranges ниже; неизменённые contract versions отделены от product version. | Данные подготовлены; release notes для `0.6.5` ещё не опубликованы. |

Текущие ranges из production CLI и
[version registry](../../crates/lekalo-core/src/versioning/contracts/version-registry.v0.2.16.json):

| Family | Current | Min | Max |
| --- | --- | --- | --- |
| Model | `0.2.16` | `0.2.16` | `0.2.16` |
| IR | `0.2.16` | `0.2.16` | `0.2.16` |
| Target protocol | `0.3.2` | `0.3.2` | `0.3.2` |

## Непубликованное и оставшаяся работа

На момент подготовки коммита локальный и удалённый тег `v0.6.5` отсутствуют;
GitHub Release `0.6.5` также отсутствует. Release binaries, архивы,
`SHA256SUMS.txt` и build records для `0.6.5` не опубликованы. Workflow
ещё не доставлен на GitHub и не запускался там для этого изменения.

Полное завершение #104 требует hosted OS matrix evidence, реальной загрузки
и проверки скачанных assets, SBOM, signed provenance/attestation, schema/protocol
bundle, решения о signature policy и release notes с compatibility ranges.
Эти пункты выходят за пределы данного задания на локальную release preparation.
В этом задании выполняется только один коммит на `main`; tags, push и
публикация релиза запрещены заданием и не выполняются.
