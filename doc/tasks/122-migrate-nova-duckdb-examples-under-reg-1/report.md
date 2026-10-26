# Доклад: задача 122 «migrate nova-duckdb examples under reg 1637»

Satellite-пакет `nova-duckdb` (D:\Sources\nv-lang\nova-duckdb) мигрирован под №1637
(исчерпывающие match по существу), Oracle чистый; `nova build` примеров через Карину
упирается во внешний блокер (pinned B1 без модулей зависимостей) — задокументирован.

## РЕПРО

В `nova-duckdb` — `E_MATCH_NON_EXHAUSTIVE` в `src/sqltag.nv:40` (`value_of`):
№1637 вырастил сумму `SqlValue` std с 6 вариантов до 14 (по одному на числовой
тип), а match знал домиграционные 6 поимённых веток (`I`/`F`/`S`/`B`/`Bytes`/`Null`)
— задача 118 замерила это как единственное падение `nova test src` пакета.
Дополнительно компилятор текущей эпохи отвергал примеры: `int_at` в
`examples/01-hello/hello_duckdb.nv` и `examples/02-appender-history/appender_history.nv`
(ветка `_ => 0 - 1` типа `int` против рукава `i64` — E_MATCH_ARM_WIDTH_MISMATCH,
D491) и мёртвые импорты `duckdb.types.{Value}` в `examples/03-with-tls/with_tls.nv`
и `src/encryption_test.nv` (unused-import: использование только в паттерн-позиции).
Проверено «до»: `nova check src` FAIL на sqltag — дословно в
`evidence/oracle-check-src-before.txt`.

## ТОЧКА

- `src/sqltag.nv` `value_of`: match по 6 старым вариантам `SqlValue`; никакого
  catch-all там не было — диагностика требовала поимённо покрыть новые 14.
- `examples/01-hello`/`02-appender-history`: `(0 - 1)` типа `int` в fallback-ветке
  `int_at`, где соседний рукав — `i64`; конструктор `Value.Int` ждёт `i64`
  (D491-граница подтверждена пробником: `int`→`i64` = E_IMPLICIT_NARROWING,
  дословно в `evidence/oracle-check-src-d491-narrowing.txt`).

## ФИКС

Ветка `task/122-migrate-under-reg-1637` в satellite (не запушена — пуш интегратору):

- `0f82981` — `sqltag.nv`: match в `value_of` переписан на 14 поимённых веток по
  D-блокам маппинга (без `_`): `Int/I8..I64 → Value.Int(n as i64)`,
  `U8..U32 → Value.UInt(n as u64)` (симметрия с read-путём `duckdb.nv`),
  `F32 → Value.Real(x as f64)`; каждая конверсия lossless. Новая фикстура-пир
  `src/sqltag_test.nv`.
- `a59a073` — `examples/01-hello`/`02-appender-history`: `(0 - 1) as i64`;
  `examples/03-with-tls/with_tls.nv` и `src/encryption_test.nv`: удалены мёртвые
  импорты `Value` (использовался только в паттерн-позиции — резолв идёт по типу
  скрутини, замерено пробником `patprobe` в обе стороны).

## ФИКСТУРЫ

- `src/sqltag_test.nv` (satellite, пир по конвенции std): блоки `test "…"` с
  `assert` через обёртки `expect_int/expect_uint/expect_real` — каждый из 14
  вариантов поимённо, значения переносятся без потерь (включая верх диапазона
  `I32`/`U32`), порядок списка в `values_of`; входит в `nova test src`
  (PASS 5 FAIL 0 SKIP 7). Маркеров EXPECT_* нет — это пир-тест пакета, не
  spec_tests.
- №1928 — пробник-evidence `evidence/1928-enum-payload-array-literal.txt`
  (bytesprobe, дословный вывод). Фикстурой spec_tests НЕ оформлен: фикс — чужая
  задача (OPEN reg 1015).

## САБОТАЖ (в обе стороны, оба вида)

Саботаж ВЕТКИ (исчерпаемость компилятора):
- Убрана ветка `SqlValue.U32` из match → `nova check src` красный:
  `E_MATCH_NON_EXHAUSTIVE … не покрывает вариант: U32` — дословно в
  `evidence/sabotage-arm-red.txt`.
- Ветка возвращена → зелёный (`evidence/oracle-check-src.txt`, те же финальные
  байты, `git diff` пуст).

Саботаж ЗНАЧЕНИЯ (фикстура ловит неверный маппинг, не только компилятор):
- `SqlValue.U32(n) => Value.UInt((n as u64) + 1)` → `nova test src --filter sqltag`
  красный: `RUN-FAIL src/sqltag_test … FAIL: unsigned integers land in Value.UInt
  … — sqltag_test.nv:32: assert failed: n == want`, PASS 0 FAIL 1 SKIP 1, rc=1 —
  дословно в `evidence/sabotage-value-red.txt`.
- Восстановлено `Value.UInt(n as u64)` → зелёный, rc=0 — дословно в
  `evidence/sabotage-value-green.txt`; `git diff` satellite пуст.

## Гейты

Все дословные логи — `doc/tasks/122-migrate-nova-duckdb-examples-under-reg-1/evidence/`;
команды, cwd, переменные окружения, хеши бинарей/исходников — `evidence/README.md`.

- Oracle: `nova check src` PASS 12 FAIL 0 WARN 0 (`evidence/oracle-check-src.txt`);
  `nova test src` PASS 5 FAIL 0 SKIP 7 (`evidence/oracle-test-src.txt`; все 7
  SKIP — модули без test-блоков и `fn main()`, скомпилированы OK; интеграционные
  `encryption_test`/`db_real_test`/`duckdb_test` реально PASS); `nova check` каждого из 3 примеров
  зелёный (`evidence/oracle-check-example-01-hello.txt`,
  `-02-appender-history.txt`, `-03-with-tls.txt`). Примеры резолвят duckdb в
  ЛОКАЛЬНЫЕ мигрированные исходники, не git 0.1.1: сломанный `src/types.nv`
  красит пример 01 с путём `.\../..\src\types.nv:257:1`
  (`evidence/resolution-proof-oracle.txt`), восстановлен — зелёный
  (`evidence/resolution-proof-restored.txt`).
- Карина (штатный путь: package-release.ps1 → распакованный install в скрэтче):
  все 3 примера rc=1 на emit pinned B1 — **внешний блокер**: сид не получает
  модули зависимостей вовсе (`undeclared type name QueryResult / Connection /
  DbOptions … in the declarations handed to novac`); program.rs сам пишет
  «external packages … the pinned B1 program walk does not link», плюс исходники
  duckdb вне подмножества B1 (Ш.4/Ш.7 плана 274.16). Логи:
  `evidence/carina-build-01-hello.txt`, `-02-appender-history.txt`,
  `-03-with-tls.txt`. Сломанный `types.nv` НЕ меняет вывод Карины
  (`evidence/resolution-proof-carina.txt`) — сид не читает источники зависимости
  ни локальные, ни git; это и есть блокер, расхождение Oracle(зелёный)/
  Carina(красный) зафиксировано, не подогнано.
- Реестр: 9/9 стражей зелёные, переснято в круге доработки 1 (rows=1822,
  max=1928, blockers=112, летопись «re-measured at merge»).

## std/src

Не применимо: std не тронут (satellite-пакет).

## spec

Не применимо: семантика языка не менялась; в spec не лез.

## ВЕТКА

- Satellite `nova-duckdb`: `task/122-migrate-under-reg-1637` (0f82981, a59a073) —
  НЕ запушена; push и тег 0.3.x — интегратору.
- Основной репозиторий: `nova-opencode-122-migrate-nova-duckdb-examples-under-reg-1`
  (этот доклад, evidence/, строки 1921/1928, baselines).

## КОММИТ

См. выше; в основном репо — коммиты задачи (последний — с этим докладом и evidence).

## ЧТО НЕ СДЕЛАНО

- Пуш satellite-ветки и тег — за интегратором.
- Карина-сборка примеров не зелёная — внешний блокер (pinned B1), не мой фикс.
- Строка реестра на мёртвый импорт в паттерн-позиции НЕ заведена — наблюдение
  передано интегратору письмом, решение его.
- Критерий V3 нарушен в первом круге: 01:48→02:57 = 69 минут без контрольной
  точки (норма 30). Зафиксировано в log.md честно, задним числом отметки не
  восстанавливались; со второго круга точки каждые ≤30 мин.

## Наблюдения интегратору (не дефекты моей задачи)

1. `lockfile::sync` игнорирует `[replace]` из `nova.override.toml`:
   `examples/*/nova.lock.toml` пересоздаются каждым прогоном, прибитые к
   `git … version = "0.1.1" commit = "ba7b8a1…"`. На прогоны не влияет (Oracle
   резолвит по override — `evidence/resolution-proof-oracle.txt`; Карина источники
   зависимости не читает вовсе), файлы оставлены untracked.
2. Линт unused-import не считает использование типа в паттерн-позиции
   (`Value.Text(s) => …`): резолв идёт по типу скрутини, импорт «мёртвый».
3. При сдаче — вопрос: кандидат `integrate/t122` пушить ночью (медленный раннер,
   reg 1926) или оставить на дневное merge-окно.

## Шаги приёмки (для интегратора)

1. В `nova-duckdb` на ветке `task/122-migrate-under-reg-1637`:
   `nova check src` → PASS 12 WARN 0; `nova test src` → PASS 5 FAIL 0;
   `nova check` в каждом из трёх пакетов примеров (`examples/01-hello`,
   `examples/02-appender-history`, `examples/03-with-tls`) → зелёный. Эталонные
   выводы и точные команды/окружение — `evidence/README.md`.
2. Саботаж: удалить ветку `U32` в `sqltag.nv` → E_MATCH_NON_EXHAUSTIVE; вернуть →
   зелёный. Саботаж значения: `U32 → Value.UInt((n as u64) + 1)` → `nova test src
   --filter sqltag` красный (assert n == want); вернуть → зелёный.
3. Push ветки в обе mirrors-точки satellite, тег 0.3.x по усмотрению.
4. В основном репо: принять merge с докладом, строками 1921/1928 и baselines;
   проверить реестровых стражей.
