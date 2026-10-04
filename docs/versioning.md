# Версии контрактов и миграции

Текущая версия продукта — `0.6.4`, заданная в
`workspace.package.version` в `Cargo.toml`. Контракты сохраняют собственные
принятые версии: выпуск продукта не меняет байты и версии неизменённых
контрактов. Старые схемы, переходы между ними и история миграций storage
projections удалены. Поддержки старых данных нет.

При изменении контракта его версия становится текущей версией проекта
в коммите с этим изменением. Меняются имя файла, идентификаторы контракта,
discriminator wire-формата, встроенные константы и ссылки потребителей.
Хеши пересчитываются по указанному контрактом алгоритму. Версия
неизменённого контракта при выпуске новой версии продукта сохраняется.

Issue #34 adds the workflow-provider discovery family
(`dev.lekalo.workflow-provider`): the AIFHub-consumable
`lekalo provider describe` handshake emitted as
`lekalo/workflow-provider/v0.6.4`. Like doctor, it is an independent
family — separate from the `lekalo.target/v1` adapter protocol, from
every Model/IR/graph/effect/lock contract, and from the diagnostic
registry — but unlike the per-command contracts it takes the product
version of its implementation commit rather than a family-local
version, and its successor therefore follows the product-version rule
above. The manifest pins the exact upstream contract versions it
carries; those pins never drift with a product release.

Проверка `node scripts/check-contract-versions.mjs` сравнивает рабочее дерево
с HEAD. В CI используется `--base HEAD^`: изменённый контракт должен иметь
текущую версию продукта. Проверки схем и runtime отдельно контролируют
идентификаторы, хеши и поведение.

## Новые миграции

Механизм планирования, dry-run, атомарного применения и rollback сохранён.
Его каталог сейчас пуст: исторического перехода Model 0.1.0 → 1.0.0
больше нет. `lekalo compatibility` показывает только текущие Model, IR
и protocol контракты; устаревшие версии и aliases не зарегистрированы.

При следующем изменении Model контракт получает версию проекта этого
коммита. При необходимости переноса данных добавляются явный шаг миграции,
его регистрация и тесты переноса из предыдущей принятой версии. Старые
миграции не восстанавливаются автоматически. IR пересобирается из Model.

Миграции запускаются явно через `lekalo migrate --to model/<version>`.
`--dry-run` строит план без записи; применение и rollback используют
проверки исходных байтов, ограниченные пути, журнал и резервные копии.
Неизвестная версия или незарегистрированный переход не угадываются.

Версии принимаются только в канонической форме SemVer. Поддержка определяется
точной записью в реестре, а не попаданием в числовой диапазон.
