# Улики к задаче 120 (reg 1889)

Собрано 2026-10-25 на ветке task/120-fix-reg-1889-multi-module-carina-build,
драйвер — `nova-driver/target/release/nova.exe` (cargo build --release).
Сиды/пины: asset_sha256 `7e8264fd…e69cdb`, b1_sha256 `824bf597…6996f4`,
source_sha `3729abcc…10d5` (видны в receipt каждой сборки).
SHA-256 бинаря драйвера МЕЖДУ пересборками одного и того же исходника
различается (недетерминизм линковки) — вердикт саботажа опирается на
поведение и на пустой `git diff` после восстановления, а не на равенство SHA.

## Дословный RED/GREEN через драйвер (заказ интегратора 22:04)

- `evidence-driver-red-green.log` — один файл, три фазы подряд, команды и
  выводы дословно: GREEN на вершине ветки (== main 3b13fd3c5 по дереву
  исходников): `nova build twopkg/main.nv` → exit 0, exe печатает
  `Hello, Nova!`; RED: временная правка `(staged, repo.join("novac/src"))`
  в `carina_build.rs` (НЕ коммитилась) → тот же build → exit 1,
  `E_NOVAC_SUBSET` — `` `greet` is not a callable novac knows ``; GREEN после
  `git checkout --` и пересборки → exit 0, `Hello, Nova!`.

## Воспроизводимый регресс (доработка, круг 1)

- `scripts/tools/carina-build-crossmod.py` — страж класса №1889: driver +
  pinned B1, обе crossmod-фикстуры, ПОЛНЫЙ stdout по `expected_stdout.txt`
  (маркер EXPECT_STDOUT — substring, усечённый прогон не ловит). Oracle и
  юнит-тесты `program.rs` этот wiring не покрывают.
- `crossmod-green.log` — GREEN до саботажа: обе фикстуры PASS, 1 и 15 строк,
  rc=0.
- `crossmod-red.log` — RED на саботажном драйвере: rc=1, `E_NOVAC_SUBSET
  greet` (та же правка, что в фазе 2).
  (`crossmod-red.log` и `sabotage-*.log` содержат по одной обёртке
  PowerShell NativeCommandError — артефакт консольного редиректа `*>`,
  не вывод программ; все файлы перекодированы из UTF-16LE в UTF-8/LF.
  Дословным эталоном считать `evidence-driver-red-green.log` — он чист.)
- `crossmod-green-final.log` — GREEN после восстановления: rc=0.

## Класс «до»

- `repro-before-seed-direct.json` — прямой прогон pinned-сида Carina B1 по
  двухфайловому пакету `target/repro120/twopkg` с контрактом «до»:
  `NOVAC_SELF_PATH=<repo>/novac/src`, `NOVA_STD_PATH=<repo>/std/src`.
  Красное: `E_NOVAC_SUBSET` — `` `greet` is not a callable novac knows ``.
  Это и есть класс №1889: сиду отдавались исходники самой Карины, а не граф
  модулей программы.
- `sabotage-red.log` / `sabotage-green.log` — первая пара саботаж-проб
  (2026-10-25 21:5x), та же правка; superseded by
  `evidence-driver-red-green.log` (та же механика, один файл, дословно).

## Литеральный before vs саботажный эквивалент

Литеральный прогон «до» довоенным бинарником драйвера (коммит до e4242dfbc)
в файл тогда не писался (терминал; ключевые строки процитированы в
report.md §РЕПРО). Поведенческий эквивалент состояния «до» — фаза RED:
правка восстанавливает тот же контракт вызова сида (`NOVAC_SELF_PATH =
<repo>/novac/src` для любой сборки) при прочих равных; диагностика
дословно совпадает с `repro-before-seed-direct.json`.

## Кандидат №1924

- `pkgq2-ice-program-nv-157.log` — свежее (2026-10-25 22:1x) минимальное
  репро: package-qualified импорт СВОЕГО пакета (`import pkgq2.helper.{greet}`)
  → `E_NOVAC_ICE pipeline/program.nv:157` («belongs to module pkgq2.helper,
  which the entry's imports do not reach»). Контроль `./helper.{greet}`
  зелёный. Исходники pkgq2: `target/repro120/pkgq2/` (скрэтч).
- `pkgq-ice-program-nv-157.log` — первая зафиксированная форма той же
  находки (пакет pkgq).
