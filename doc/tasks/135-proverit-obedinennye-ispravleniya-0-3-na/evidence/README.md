# Контуры измерения 135

**62 PASS — результат patched-scratch-driver, НЕ поставляемого pristine-driver.** Полного замера pristine-driver на объединённом сиде нет; его SEED_* остаются официальными и не выбирают объединённый сид. Исторический замер 128 использует другой сид и не заменяет такой замер.

Сводка: [driver-contours.tsv](driver-contours.tsv).

| Контур | Таблица 118 | Таблица пакетов | Изменения драйвера |
|---|---|---|---|
| pristine-driver + объединённый сид | НЕ ЗАМЕРЕНО | НЕ ЗАМЕРЕНО | официальные исходники не менялись |
| patched-scratch-driver-v1 | [таблица](patched-scratch-driver-v1/table-118.tsv) | [пакеты](patched-scratch-driver-v1/packages.tsv) | локальные SEED_* / unpacked-путь, копирование sibling sidecar |
| patched-scratch-driver-v2 | [таблица](table-118.tsv) | [пакеты](packages.tsv) | те же пины, рекурсивное sidecar-копирование |

Финальные [118 логов](logs-118/) и [19 пакетных логов](logs-packages/) относятся **только к v2**. Первый набор итоговых таблиц сохранён отдельно; его полные логи остаются в scratch `run118/logs` и `runpk/logs`, не подменены логами v2.

Воспроизводимый [diff scratch-драйвера v2](scratch-driver.patch) к точной [базе](scratch-driver-base.txt) `01b960b3a312b76d3f096c95deea17eea3ebad76`: `git apply --check` и `git apply` из корня отдельной копии этой базы; затем `cargo build --release --manifest-path nova-driver/Cargo.toml`. ZIP локального сида нужно положить в локальный seed cache: URL в diff — идентификатор непубликованного кандидата, по сети его нет. Эта заплатка — улика контура, не принятый production-фикс (ошибки копирования в ней подавляются; полная приёмка класса не выполнена).

[Состояние и diff lock-файлов спутников](satellite-lock-status.md). Только чтение, ничего не откатывалось и не включалось из спутников в коммит задачи.

[Ранжирование следующих стен](next-walls.md) — по первой диагностике сохранённых логов v2, без нового sweep.

Пять библиотек не проверены: bignum, compress, http, socks, tls.
