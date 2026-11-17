# lekalo-python

Языковой runtime Python для процесса `lekalo.target/v1`. `src/` содержит чтение профиля, статическое сканирование и языковые символы; правила регистрации FastAPI принадлежат отдельному `lekalo-python-fastapi`.

```sh
cargo +1.97.0 test --locked
cargo +1.97.0 build --locked --config 'profile.dev.debug=0'
node --test tests/scan-protocol.test.mjs tests/fastapi.test.mjs
```

Сборка самостоятельна: соседний checkout ядра не нужен. `vendor/lekalo-adapter-sdk-rust` — закреплённый протокольный SDK из ядра (`sdk-source.lock.json`), `vendor/lekalo-python-fastapi` — закреплённый пакет правил (`framework-source.lock.json`). Эти копии обновляются из исходных репозиториев, не редактируются здесь. Целевая версия Python и версия хостового Rust — разные настройки.

Пока FastAPI подключается как библиотека на этапе сборки, а разрешённые providers профиля ограничены прежним протоколом. Обновление закреплённого FastAPI: после commit в его репозитории выполните `node scripts/sync-framework.mjs`, затем `node scripts/check-sources.mjs` и тесты.

Произвольный repo/ref и загрузка правил по компонентам требуют последующего resolver/provider API: [lekalo#239](https://github.com/ichinya/lekalo/issues/239), [lekalo#197](https://github.com/ichinya/lekalo/issues/197). Межпроцессные тесты с CLI ядра остаются host-conformance в основном репозитории.

`ci/package-workflow.yml` — неактивный шаблон CI. Для автоматического запуска его нужно установить в `.github/workflows/` учётными данными с правом `workflow`; текущая публикация проверена локально.

## Распознавание проектов для `init --adopt`

`crates/discovery` владеет языковыми правилами обнаружения, перенесёнными из
`lekalo-core/src/init/detect.rs`. Эта небольшая Rust-библиотека принимает
байты/значения; обход каталогов, no-follow чтение, проверка путей и ограничения
остаются в хосте. Команды build/test являются предложениями и не исполняются.

```sh
cargo test --locked --manifest-path crates/discovery/Cargo.toml
```

Ядро использует проверяемую копию в `packages/` с точным commit и SHA-256.
Это переходный способ поставки host-side discovery, а не новый процессный
runtime или resolver пакетов. Формат существующего adoption receipt сохраняется.
