# Задача 135 — объединённый локальный сид Carina (130+131+134) на origin/main 0b372f664: пробы, замер 118+19, сверка с 128

Дата: 2026-11-01. Исполнитель: nova.worker (ветка `task/135-proverit-obedinennye-ispravleniya-0-3-na`).
Заказчик: nova.integrator. Слово владельца «делай супер быстро» (2026-10-31) учтено: без полного `nova test`/mega-CU, без Oracle, тяжёлые прогоны через машинную очередь.

## РЕПРО (три фикса одним бинарём)

Подтверждено. ОДИН бинарь объединённого сида (`novac.exe` из `nova-carina-0.2.2-135combined-windows-x86_64.zip`) прошёл 6/6 проб GREEN:

| # | проба | фикс | ожидание | факт |
|---|-------|------|----------|------|
| 1 | анонимный конструктор `{ x, y }` в bind | 130 | печать 9 | GREEN `9` |
| 2 | анонимный конструктор в аргументе вызова | 130 | ok | GREEN `ok` |
| 3 | `extern "C" fn` прототип + вызов | 131 | печать 5 | GREEN `5` |
| 4 | marker_order_pos_1: канонический порядок маркеров | 134 | 7 + closed | GREEN |
| 5 | bare_block_pos_1: голый блок | 134 | печать 42 | GREEN `42` |
| 6 | for_inclusive_pos_1: включительный диапазон | 134 | печать 10 | GREEN `10` |

Контроль ванильным B1 (0.2.1, официальный): те же 6 проб — 6/6 RED (130 — ICE `type_of.nv:632`; 131/134 — E_NOVAC_SUBSET/отказы форм). Улики: `evidence/probes-green.txt`, `evidence/probes-red.txt` (пути обезличены `$WT`).

## ТОЧКА (идентичность сида)

- **Исходник:** `3729abcce42ec2d94efc898ec6eafe91ff6210d5` (B1, = источник замера 128) + бэкпорты принятых фиксов 130 (коммиты `efb2af8c4..f7b2202a1`), 131 (`71d8a35bd`/`88d4684f1`/`2121eb5bc`), 134 (классы A/B из `5aabbb4b4..c57409eab`). Дерево: 218 файлов `novac/src` — 173 base + 26 чистых с HEAD + 7 бэкпорт-улик + 12 слитых `git merge-file` (current=base, ancestor=preFix, other=HEAD — слияние само вырезало код задачи 92). `verify_tree.py`: ALL CLEAN (ни маркеров 92, ни срыва пинов; sha бэкпорта сходятся). По-файловый provenance + sha256: `evidence/seed-combined/MANIFEST.md`.
- **Хеши:** C `9bac3159d15b3b159bdcdc5a357a7c453dfc6d6fa335d2131d07a6673ea26a76`; exe `63ef8c6cc0996264ae41a964abaa41fb3d4d6b2a82a788a1fec2f9344c981327`; ZIP `c727288f301b4b8849bffe4d6e75b7bc9f55589472cfec97029d70521d16fb69`. Опора идентичности — C sha (exe машинно-зависим через PE-debug path).
- **Маркировка:** asset `nova-carina-0.2.2-135combined-windows-x86_64.zip`, SOURCE-SHA `3729abcc…+task135-combined-130-131-134`, stage `carina-b`, provenance «LOCAL undeliverable candidate». Кэш сидов: установлен под своим именем, 0.2.1 и 0.2.2-130fix не тронуты. Release/assets НЕ публиковались, теги НЕ ставились, main НЕ двигался, официальный дескриптор и SEED_* на main НЕ менялись.

## Замер 118 примеров (порядок и входы — байт-в-байт из evidence 128)

**Оба замера ниже — patched-scratch-driver, не поставляемый путь. Pristine-driver на объединённом сиде НЕ ЗАМЕРЕН.** Разделение контуров, оба набора таблиц и воспроизводимый diff: [evidence/README.md](evidence/README.md), [driver-contours.tsv](evidence/driver-contours.tsv). 62 PASS не являются утверждением о поставляемом драйвере.

Контур: scratch-драйвер (SEED_* → 0.2.2-135combined, unpacked-путь, sidecar-фикс стейджинга), install135 (std = HEAD == std замера 128, diff пуст), clang, Boehm/libuv из vendored подмодулей.

ИТОГИ ПЕРВОГО ПРОГОНА (run118, 21 мин): **62 PASS (60 rc=0 + 2 намеренные rc=101), FAIL 56** — дословно расклад 128 по числам. Гистограмма классов:

| класс | 128 | 135 |
|-------|-----|-----|
| PASS | 62 | 62 |
| E_NOVAC_SUBSET | 48 | 44 |
| E_NOVAC_LANG | 3 | 8 |
| E_NOVAC_ICE | 1 | 0 |
| compile/link (no main) | 4 | 4 |

Изменившиеся строки (6), каждая разобрана по дословным логам:

1. **`examples/tour/modules_tour.nv`: E_NOVAC_ICE → E_NOVAC_SUBSET.** Носитель №1924 в объединённом сиде больше НЕ падает ICE — первая диагностика честная («in the instance of `to_uppercase`: []T.new(...) takes no arguments…»). Сам ICE program.nv:157 как класс жив (воспроизведён на этом же дереве голым относительным entry — отдельная запись №TBD).
2. **`examples/tour/consume_tour.nv`: SUBSET → LANG.** Ожидаемый эффект 134: `defer` отказан по имени (D90, долг E2-горизонта, №1938) вместо ложного «record constructor».
3. **`examples/os/agent_runner.nv`, `examples/os/pty_session.nv`: SUBSET → LANG («break… no loop around this one»).** Разбор: base молча читал `loop {` как record-ctor (безусловный `Name {` до 130) и умирал внутри него, не доходя до `break`; дверь `named_ctor_ahead` (130) дала честный разбор, и обход дошёл до `break` в match-арме. Сам `loop` subset не читает (зарезервированное слово, лексится Ident; класс keyword-parity уже назван в №1089/№1938). Оба контура пример отвергают — сменилась только первая диагностика на уже падающем входе. Новых PASS это не дало.
4. **`examples/tls/echo_client.nv`, `echo_server.nv`: SUBSET → LANG (E_EMBED_NOT_FOUND).** Улика контура, не сида: extern-фикс 131 пустил tls-деп дальше, до чтения embed `certs/localhost_cert.pem`, который стейджинг драйвера не копировал (дефект №TBD, stage_program теряет не-.nv). После рекурсивного sidecar-фикса scratch-драйвера проба echo_client: E_EMBED_NOT_FOUND снят, первая диагностика содержательная SUBSET. Перепрогон всей таблицы на исправленном контуре — ниже.

ПЕРЕПРОГОН (run118b, драйвер с рекурсивным sidecar), завершён: **62 PASS, 46 E_NOVAC_SUBSET, 6 E_NOVAC_LANG, 4 compile/link, 0 ICE**. Оба TLS-входа вновь SUBSET, E_EMBED_NOT_FOUND снят. Относительно 128 изменились классы только четырёх строк: agent_runner, pty_session, consume_tour, modules_tour (разбор выше). Новых PASS нет. Два намеренных rc=101 сверены с принятыми кодами этих же входов в 128; остальные 60 PASS имеют rc=0.

Таблица: `evidence/table-118.tsv` (финальная), сохранённые логи — `evidence/logs-118/`; пакетные логи — `evidence/logs-packages/`. Пути дерева обезличены `$WT`.

## Замер 19 пакетных входов (входы и HEAD'ы спутников — из 128, сверены)

Первый прогон: 18/19 класс-в-класс с 128 (все E_NOVAC_SUBSET); nova-sdl — та же субстанция (remote-not-found, репозиторий недоступен), в 135 причина записана текстом, в 128 поле было пустым. Перепрогон на исправленном контуре подтвердил тот же итог: **18 SUBSET + 1 remote-not-found**, все build rc=1, запусков не было.

5 библиотечных пакетов (bignum, compress, http, socks, tls) — **НЕ ПРОВЕРЕНО** (нет runnable entry, как и в 128; package gate — область 0.4).

## №1929 — три носителя, нормативная сверка

Диагностики носителей в 135 **дословно совпали с 128** (коды, позиции, тексты; `carriers_1929.py`). Нормативная сверка через read-spec (spec-reader): по всем трём носителям норма требует ПРИЁМА — прав корпус, неверен сид: `() as int` нелегален как форма, но операнд не unit (std Duration `@millis -> i64`) — сбой резолва; коллизия вариантов — D478 п.2 (ожидаемый тип), сам D478 (`02-types.md:1002-1005`) заранее назвал отказ Карины частично неверным; ссылка сида на D406 — не тот уровень нормы. Язык и корпус задачей 135 не правились (решение — владельца/интегратора); №1929 остаётся OPEN и блокирует тег.

## Новые дефекты (№1945–1947, номера назначены интегратором, внесены в docs/plans/221.1-bug-sweep.md этим же мержем)

- **№1945 / A (К1, P1):** nova-driver `stage_program` теряет embed-sidecar (D412) — два носителя (self-build Карины `shell.tpl.c`; tls `certs/` подкаталогом). Заплатка только в scratch-драйвере задачи, на main не перенесена.
- **№1946 / B (К2, P2):** ICE `program.nv` при `novac emit` с относительным entry без префикса self-root — воспроизводится на чистом pinned-дереве; обход — префиксный спеллинг (форма MANIFEST-130). Родня №1924.
- **№1947 / C (К3):** sha `binds.nv` в MANIFEST-130 посчитан по CRLF-байтам рабочей копии, а не по blob — цепочка provenance машинно не сверяется.

## ФИКС / ФИКСТУРЫ / САБОТАЖ

Задача измерительная: кода языка/компилятора не меняет. САБОТАЖ-контроль прогона: ванильный B1 на тех же 6 пробах — 6/6 RED (выше). Дерево объединения проверено `verify_tree.py` (маркеры 92 отсутствуют, sha бэкпортов сходятся, wiring 131/134 на месте).

## std/src

ДО == ПОСЛЕ: std не тронут (install135 использует std HEAD, идентичный std замера 128 — diff пуст).

## ВЕТКА/КОММИТ

`task/135-proverit-obedinennye-ispravleniya-0-3-na` — коммит с этим отчётом, MANIFEST, уликами проб и записями №TBD (хеш — по факту коммита).

## ЧТО НЕ СДЕЛАНО

- 5 библиотечных пакетов — НЕ ПРОВЕРЕНО (нет runnable entry).
- Полный `nova test`, mega-CU, локальные gate.sh/gate-novac.sh — не гонялись (запрет задачи; прерогатива интегратора).
- Oracle-судья — не участвовал (запрет задачи).
- Заплатки scratch-драйвера (sidecar, SEED_*, unpacked) — НЕ перенесены на main; перенос embed-sidecar фикса — отдельная задача по записи A.
- exe sha256 машинно-зависим (PE-debug path); опора идентичности — sha сгенерированного C.

## Ограничения сопоставимости и окончательная поставка улик

- **Pristine-driver + объединённый сид: NOT-MEASURED.** Результат не реконструируется из scratch-прогонов.
- Оба снятых набора — patched-scratch-driver-v1 и v2; обе пары таблиц сохранены и помечены driver_contour. Diff v2 и точная база — [scratch-driver.patch](evidence/scratch-driver.patch), [scratch-driver-base.txt](evidence/scratch-driver-base.txt); `git apply --check` прошёл.
- 137 финальных логов (118+19) и таблицы проверены через `git ls-files`; локальные ссылки отчёта и evidence/README.md разрешаются.
- **Polaris-lock дрейфовал:** 12 examples/nova.lock.toml изменены (ограничение http с >=0.1,<0.2 на >=0.2,<0.3). Поэтому утверждение «те же закреплённые зависимости, что в 128» НЕ доказано и для polaris не применяется. Совпадение HEAD спутника и имени входа не доказывает совпадения разрешённого графа зависимостей. Это ограничение сопоставимости пакетного замера, а не доказанный регресс/прогресс сида. В duckdb созданы три untracked lock. Полный status/diff — [satellite-lock-status.md](evidence/satellite-lock-status.md). Файлы не откатывались и не включались из спутников в коммит.
- Следующие стены — **корзины первых диагностик, НЕ доказанные классы дефектов**: parser fallback — 18 входов; tail-without-value — 16; effect attribute — 4; static-method ambiguity — 4; link-no-main — 4. Полное ранжирование с носителями: [next-walls.md](evidence/next-walls.md). Новый sweep не запускался.
