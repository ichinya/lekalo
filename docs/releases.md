# Установка и выпуск Lekalo CLI

При публикации GitHub Release workflow `Release binaries` собирает CLI из
коммита указанного тега с `cargo build --release --locked`. После завершения
всех пяти сборок архивы прикрепляются к этому релизу:

| Платформа | Target | Архив |
| --- | --- | --- |
| Windows x64 | `x86_64-pc-windows-msvc` | `.zip` с `lekalo.exe` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `.tar.gz` с `lekalo` |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | `.tar.gz` с `lekalo` |
| macOS Intel | `x86_64-apple-darwin` | `.tar.gz` с `lekalo` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `.tar.gz` с `lekalo` |

Имена имеют вид `lekalo-v<VERSION>-<TARGET>.<EXTENSION>`. В каждом архиве
одна папка с исполняемым файлом и README. Для Linux используются native
Ubuntu 22.04 runners (glibc, не musl); macOS проверяется на macOS 14 ARM64
и macOS 15 Intel; Windows — на Windows Server 2022 x64.

Релиз также содержит `SHA256SUMS.txt` и отдельный `.build.json` для каждого
архива: commit исходников, target, версию Rust, SHA-256 lockfile, бинарника
и архива, а также ссылку на запуск GitHub Actions. Эти записи являются
метаданными сборки; криптографическая подпись и notarization не добавляются
этим workflow.

## Проверка и установка

Скачайте соответствующий архив и `SHA256SUMS.txt` из **одного** релиза.
Подставьте его версию в примеры. Команды проверяют checksum до распаковки.

### Windows (PowerShell)

```powershell
$version = '0.6.5'
$archive = "lekalo-v$version-x86_64-pc-windows-msvc.zip"
$entries = @(Get-Content -LiteralPath SHA256SUMS.txt | Where-Object { $_.EndsWith("  $archive") })
if ($entries.Count -ne 1) { throw 'Checksum entry missing or duplicated' }
$expected = ($entries[0] -split '  ', 2)[0]
$actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw 'SHA-256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath .
& ".\lekalo-v$version-x86_64-pc-windows-msvc\lekalo.exe" --version
```

Перенесите `lekalo.exe` в каталог из `PATH`, если нужен вызов `lekalo`
из любого рабочего каталога.

### Linux (bash)

```bash
set -euo pipefail
version=0.6.5
target=x86_64-unknown-linux-gnu # aarch64-unknown-linux-gnu для ARM64
archive="lekalo-v${version}-${target}.tar.gz"
awk -v name="$archive" '$2 == name { print; found++ } END { if (found != 1) exit 1 }' SHA256SUMS.txt > selected-checksum.txt
sha256sum --check --strict selected-checksum.txt
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "lekalo-v${version}-${target}/lekalo" "$HOME/.local/bin/lekalo"
"$HOME/.local/bin/lekalo" --version
```

Добавьте `$HOME/.local/bin` в `PATH`, если этого каталога там ещё нет.

### macOS (bash)

```bash
set -euo pipefail
version=0.6.5
target=aarch64-apple-darwin # x86_64-apple-darwin для Intel
archive="lekalo-v${version}-${target}.tar.gz"
awk -v name="$archive" '$2 == name { print; found++ } END { if (found != 1) exit 1 }' SHA256SUMS.txt > selected-checksum.txt
shasum -a 256 --check selected-checksum.txt
tar -xzf "$archive"
mkdir -p "$HOME/.local/bin"
install -m 755 "lekalo-v${version}-${target}/lekalo" "$HOME/.local/bin/lekalo"
"$HOME/.local/bin/lekalo" --version
```

## Создание релиза

1. Убедитесь, что обычный CI прошёл для релизного коммита.
2. Создайте тег `v<VERSION>` на нужном коммите. Версии `lekalo-cli` и
   `lekalo-core` в Cargo должны совпадать с `<VERSION>`. Дождитесь CI
   для этого тега: job `Release tag and Cargo version parity` блокирует
   несовпадение до создания релизных артефактов.
3. Опубликуйте GitHub Release для этого тега с названием `<VERSION>`
   или `v<VERSION>`. Событие `release: published` запускает сборки,
   включая prerelease.
4. Дождитесь успешного завершения `Release binaries`: каждый бинарник
   проверяется через `--version` и `--help` на своей ОС, затем ещё раз
   запускается после распаковки архива. Команда `compatibility` проверяет
   встроенный реестр контрактов вне checkout. Ошибка любой сборки блокирует
   загрузку всего набора.

Workflow должен уже находиться в коммите нового релизного тега. Для старого
релиза, чей тег не содержит workflow, используйте ручной запуск:
**Actions → Release binaries → Run workflow**, выберите ветку с workflow
и передайте существующий опубликованный тег в поле `tag`.
Исходники будут взяты из тега, а скрипты сборки — из коммита workflow.
Если версия Cargo в старом теге отличается от версии релиза, ручной запуск
тоже завершится отказом: нужен новый релиз с согласованными версиями.

Проверки блокируют несовпадение версии тега, названия релиза, Cargo packages
или `lekalo --version`, грязные исходники, изменение тега во время сборки,
неполный набор платформ и неверные SHA-256. После загрузки workflow скачивает
свои release assets и сравнивает их с локальными файлами. Повторный запуск
заменяет только файлы с теми же именами в указанном релизе; остальные релизы
и их assets сохраняются. Обновление установленного CLI выполняется вручную
с такой же проверкой checksum; предыдущую версию можно скачать для отката.
