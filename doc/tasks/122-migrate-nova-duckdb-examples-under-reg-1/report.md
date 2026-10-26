# Доклад: задача 122 «migrate nova-duckdb examples under reg 1637»

Satellite-пакет `nova-duckdb` (D:\Sources\nv-lang\nova-duckdb) мигрирован под №1637
(исчерпывающие match по существу), Oracle чистый; `nova build` примеров через Карину
упирается во внешний блокер (pinned B1 без модулей зависимостей) — задокументирован.

## РЕПРО

В `nova-duckdb` — `W_MATCH_EXHAUSTIVENESS + catch-all` в `src/sqltag.nv`
(`sqlValueToValue`) и в `examples/01-connect-query.nv`, `examples/02-ddl-insert.nv`
(`(0-1) as int` без приведения — сужение до `Value.Int(i64)` ожидалось на старой
семантике). Проверено «до»: `nova check src` = PASS 11 WARN 1,
`nova check examples` = WARN в 01/02. (`check-before.txt` в скрэтче.)

## ТОЧКА

- `src/sqltag.nv` `sqlValueToValue`: catch-all `_` поглощал варианты `SqlValue` —
  маппинг не был виден по существу; `U8` падал в `_ => Value.Int(v as i64)`.
- `examples/01`/`02`: `Value.Int` строился из `int` (`(0-1) as int`), а конструктор
  ждёт `i64` (D491-граница подтверждена пробником: `int`→`i64` = E_IMPLICIT_NARROWING).

## ФИКС

Ветка `task/122-migrate-under-reg-1637` в satellite (не запушена — пуш интегратору):

- `0f82981` — `sqltag.nv`: catch-all заменён на 14 поимённых веток по D-блокам
  маппинга: `Int/I8..I64 → Value.Int(v as i64)`, `U8..U32 → Value.UInt(v as u64)`
  (симметрия с read-путём `duckdb.nv`), `F32 → Value.Real(v as f64)`; `SqlValue`
  импортирован для резолва паттернов. Новая фикстура-пир `src/sqltag_test.nv`.
- `a59a073` — `examples/01`/`02`: `(0-1) as i64`; `03-encrypted-persistence.nv` и
  `examples/encryption_test.nv`: удалены мёртвые импорты `Value` (использовался
  только в паттерн-позиции — резолв идёт по типу скрутини, замерено пробником).

## ФИКСТУРЫ

- `src/sqltag_test.nv` (satellite, пир по конвенции std): покрывает `sqlValueToValue`
  по всем 14 вариантам + `sqlTagName`; EXPECT_STDOUT-маркер; входит в `nova test src`.
- №1928 — пробник-evidence `evidence/1928-enum-payload-array-literal.txt`
  (bytesprobe, дословный вывод). Фикстурой spec_tests НЕ оформлен: фикс — чужая
  задача (OPEN №1015, scoped-down).

## САБОТАЖ (в обе стороны)

- Убрана ветка `SqlValue.U32` из match → `nova check` красный:
  `E_MATCH_NON_EXHAUSTIVE … не покрывает вариант: U32` (файл `sabotage-red.txt`).
- Ветка возвращена → `nova check src` PASS 12 FAIL 0 WARN 0 (зелёный).
  Саботаж доведён до финальных байтов (CRLF/BOM вычищены, diff пуст).

## Гейты

- Oracle: `nova check src` PASS 12 FAIL 0 WARN 0; `nova test src` PASS 5 FAIL 0
  SKIP 7 (skip — интеграционные с живой БД, как и было); `nova check` каждого из
  3 примеров зелёный; примеры резолвят duckdb по override в локальные мигрированные
  исходники (доказано: сломанный `types.nv` красит пример с путём
  `.\../../src/types.nv`, возвращено).
- Карина (штатный путь: package-release.ps1 → распакованный install,
  `examples/carina.toml`): все 3 примера FAIL на emit pinned B1 — **внешний блокер**:
  сид не получает модули зависимостей (`undeclared type name QueryResult /
  Connection / DbOptions`, первый `QueryResult` в `duckdb.nv:298`); program.rs сам
  пишет «external packages … the pinned B1 program walk does not link», плюс duckdb
  вне подмножества B1 (Ш.4/Ш.7 плана 274.16). Логи: `carina-01/02/03.log` в скрэтче.
- Реестр: 8/8 стражей зелёные (rows/routes замерены на ветке: rows=1822, max=1928,
  blockers=112, летопись «re-measured at merge»).

## std/src

Не применимо: std не тронут (satellite-пакет).

## spec

Не применимо: семантика языка не менялась; в spec не лез.

## ВЕТКА

- Satellite `nova-duckdb`: `task/122-migrate-under-reg-1637` (0f82981, a59a073) —
  НЕ запушена; push и тег 0.3.x — интегратору.
- Основной репозиторий: `nova-opencode-122-migrate-nova-duckdb-examples-under-reg-1`
  (этот доклад, строки 1921/1928, baselines).

## КОММИТ

См. выше; в основном репо — коммит с этим докладом.

## ЧТО НЕ СДЕЛАНО

- Пуш satellite-ветки и тег — за интегратором.
- Карина-сборка примеров не зелёная — внешний блокер (pinned B1), не мой фикс.
- Строка реестра на мёртвый импорт в паттерн-позиции НЕ заведена — наблюдение
  передано интегратору письмом, решение его.

## Наблюдения интегратору (не дефекты моей задачи)

1. Драйверский `lockfile::sync` проигнорировал `[replace]` в `carina.toml`: локи
   прибил к git 0.1.1 (`QueryResult` undeclared в обоих пробах); файлы удалены как
   untracked, ручной override прогнал путь дальше (01 дошла до первой ошибки в
   duckdb.nv).
2. Линт unused-import не считает использование типа в паттерн-позиции
   (`Value.Text(s) => …`): резолв идёт по типу скрутини, импорт «мёртвый».
3. При сдаче — вопрос: кандидат `integrate/t122` пушить ночью (медленный раннер,
   reg 1926) или оставить на дневное merge-окно.

## Шаги приёмки (для интегратора)

1. В `nova-duckdb` на ветке `task/122-migrate-under-reg-1637`:
   `nova check src` → PASS 12 WARN 0; `nova test src` → PASS 5 FAIL 0;
   `nova check examples/01-connect-query.nv` (02, 03) → зелёный.
2. Саботаж: удалить ветку `U32` в `sqltag.nv` → E_MATCH_NON_EXHAUSTIVE; вернуть →
   зелёный.
3. Push ветки в обе mirrors-точки satellite, тег 0.3.x по усмотрению.
4. В основном репо: принять merge с докладом, строками 1921/1928 и baselines;
   проверить 8 реестровых стражей.
