# PROGRESS: p-bignum-t2 — nova-bignum на именованные кортежи, второй заход

Окно: p-bignum-t2. Модель: sonnet. Репа: `nova-bignum`, ветка `p-tuple2` от
`main` (компилятор НЕ трогал; собран заново из `nova/nova-cli` перед началом).
Дата: 2026-08-06.

## Итог одной строкой

**Полная миграция удалась.** Все четыре типа семьи (`BigInt`,
`BigDecimal`+`MathContext`, `BigRat`, `BigFloat`+`PrecisionContext`) переведены
на именованные кортежи (D215). Блокеры первого захода (№271/№361/№362/№356)
действительно сняты — `nova check --strict-effects` и `nova test src`
держат канон БЕЗ ЕДИНОГО отклонения на каждом из четырёх чекпоинтов
(check 8/0/15, test 7/0/2 SKIP, lint 0 findings — идентично состоянию ДО
миграции). Четыре коммита в ветке `p-tuple2`, НЕ запушены (пуш — за
владельцем, как оговорено заданием).

## Разведка перед кодом (важно для приёмки)

Прочитал ВСЕ артефакты предыдущих окон, включая записи бэклога, которых НЕ
было в брифе:
- `docs/plans/wip/PROGRESS-p-bignum-tuple.md` (первый заход, откат) —
  структура пакета там устарела (единый `src/bigint.nv`), сейчас
  folder-module (`src/bigint/core.nv` и т.д., план 243 Ф.U) — механику
  переносил, файлы не переиспользовал (их и не было, как и предупреждал бриф).
- `docs/plans/221.1-bug-sweep.md` записи №271/№356/№361/№362/№369/№370 —
  **дословно, не по памяти**, включая ДВЕ находки, которых НЕ было в брифе:
  - **№369** (уже упомянут в брифе): `#impl(Clone)` на именованном кортеже
    ломает кодоген. В семье НЕТ ни одного `#impl(Clone)` (грепнул) — не
    актуально для этой миграции, отмечаю для полноты.
  - **№370 (НЕ было в брифе, важно)**: «санкционированный» в тексте №362
    D326 mut-параметр-хелпер (`fn helper(mut mant BigInt) -> BigInt`,
    вызванный как `helper(@mant)` без `mut @`) НЕ даёт независимую копию
    heap-backed полей — тихо алиасит хранилище получателя (K1,
    silent-corruption, ОТКРЫТ). Это прямо касалось `@normalize` в
    `BigDecimal`/`BigFloat` — в первом заходе именно этот хелпер был
    «санкционированным обходом» для той же функции. Я его **не
    использовал вообще**: `mut mant = @mant` (field-launder локальной
    `mut`-переменной, НЕ передача `@field` в отдельную функцию с
    `mut`-параметром) скомпилировался и исполнился чисто без всякого
    обхода — сам класс дефекта №370 в этой миграции не встретился, потому
    что не понадобился воркэраунд вовсе (первопричина, ради которой он был
    придуман — дефект чекера в `infer_expr_type`'s `Member`-арм для
    `NamedTuple` — закрыта фиксом №362).

## Ход миграции (4 чекпоинта, по типу)

1. **`BigInt`** (`src/bigint/core.nv`) — `type BigInt value {sign, limbs}` →
   `type BigInt(sign Sign, limbs []u32)`. ~15 сайтов конструирования
   (`.zero()/.one()/.new_raw()`, арифметика `@plus/@neg/@abs/@times/@shl/@shr`,
   конверсии `to_bigint`) — позиционная форма. Коммит `cd121d0`.
2. **`BigDecimal`+`MathContext`** (`src/bigdecimal/core.nv`) —
   `type BigDecimal value {mant, scale}` / `type MathContext value {precision, rm}`
   → `BigDecimal(mant BigInt, scale int)` / `MathContext(precision int, rm RoundingMode)`.
   `@normalize` (field-launder `mut mant = @mant` на BigInt-поле, ТОТ САМЫЙ
   паттерн, что был спорным в первом заходе) — скомпилировался и прошёл
   тесты БЕЗ диагностики, БЕЗ обхода. Коммит `8c5593d`.
3. **`BigRat`** (`src/bigrat/core.nv`) — `type BigRat value {num, den}` →
   `BigRat(num BigInt, den BigInt)`. `normalize_rat`, `zero/one`,
   `@neg/@abs` (поле-punning `@den` как позиционный аргумент), мосты
   `BigDecimal @to_bigrat`/`BigFloat @to_bigrat` — все на позиционную форму.
   Коммит `1986cef`.
4. **`BigFloat`+`PrecisionContext`** (`src/bigfloat/core.nv`) —
   `type BigFloat value {mant, exp}` / `type PrecisionContext value {prec, rm}`
   → `BigFloat(mant BigInt, exp int)` / `PrecisionContext(prec int, rm RoundingMode)`.
   `@normalize` — тот же field-launder паттерн, снова без обхода. Коммит
   `e2c7ee1`.

На каждом шаге — полный `nova check src --strict-effects` + `nova test src`
ЦЕЛИКОМ (не только затронутый файл — весь пакет, чтобы ловить межфайловые
регрессии сразу).

## Вердикты — дословно, канон на каждом чекпоинте

Канон ДО миграции (проверено первым делом):
```
nova check src --strict-effects:  PASS: 8  FAIL: 0  WARN: 15
nova test src:                    PASS: 7  FAIL: 0  SKIP: 2 (skipped)
```

После КАЖДОГО из четырёх чекпоинтов (BigInt / BigDecimal / BigRat / BigFloat)
— идентично, без единого отклонения:
```
nova check src --strict-effects:  PASS: 8  FAIL: 0  WARN: 15
nova test src:                    PASS: 7  FAIL: 0  SKIP: 2 (skipped)
```
Все 15 warning — pre-existing unused-import в тестовых файлах (не мои,
подтверждено построчно — те же имена/файлы на каждом шаге).

Финальный `nova lint src`:
```
lint: 14 file(s), 0 finding(s)
```
Канон (было 0 до миграции).

## Изменения публичного API

**Форма конструирования меняется** — теперь у ВСЕХ шести типов семьи
(`BigInt`, `BigDecimal`, `MathContext`, `BigRat`, `BigFloat`,
`PrecisionContext`) конструктор — ТОЛЬКО позиционный (D102), фигурный
record-литерал (`Type { field: val, ... }`) для этих типов больше НЕ
существует:
- `BigInt(sign Sign, limbs []u32)` — было `BigInt { sign, limbs }`
- `BigDecimal(mant BigInt, scale int)` — было `BigDecimal { mant, scale }`
- `MathContext(precision int, rm RoundingMode)` — было `MathContext { precision, rm }`
- `BigRat(num BigInt, den BigInt)` — было `BigRat { num, den }`
- `BigFloat(mant BigInt, exp int)` — было `BigFloat { mant, exp }`
- `PrecisionContext(prec int, rm RoundingMode)` — было `PrecisionContext { prec, rm }`

**Позиционная деструктуризация запрещена** (D215) — код, который бы
попытался `ro (sign, limbs) = some_bigint`, не скомпилируется; только
`.sign`/`.limbs` (свойства-методы, уже так и были, не поменялись).

**Фактического влияния на потребителей — нет.** Проверил: ни один файл
внутри `nova-bignum` (кроме собственных модулей типов) не конструировал эти
типы через record-литерал напрямую — везде `.new()`/`.zero()`/`.one()`
фабрики, чья СИГНАТУРА не изменилась. Проверил также остальные репы
монорепо (`nova-http`, `nova-polaris`, `www`, `nova-tls`) — `bignum` нигде
не используется. Значит миграция ABI-breaking по факту буквы (форма
конструирования типа сменилась), но НЕ breaking по факту практики (нет ни
одного затронутого потребителя ни внутри пакета, ни снаружи).

## Компиляторные дефекты — не встретил ни одного нового

Ни `nova check`, ни `nova test` не дали ни одной ошибки/регрессии ни на
одном из четырёх чекпоинтов. Блокеры первого захода (№271, №361, №362,
№356) — все закрытые фиксы подтвердились на РЕАЛЬНОЙ, дословно той же
форме миграции, что их и вскрыла. Новых №TBD-находок в этом окне нет.

## Смежная находка — НЕ моя, вне периметра, зафиксирована в коммите

`nova test src --include-slow` даёт `CODEGEN-FAIL` на `src/bigrat/core`:
`fn br(n int, d int) -> BigRat` продублирована буквально в
`src/bigrat/core_test.nv:17` и `src/bigrat/core_slow.nv:15` (идентичная
сигнатура, folder-module конфликт имён). Оба файла НЕ входили в периметр
этой миграции (я их не трогал вообще — только `core.nv`), воспроизводится
идентично на `main` без единого изменения от меня. Дефолтный (не-slow)
`nova test src` эту пару не затрагивает (`core_slow` — SKIP по умолчанию),
поэтому канон 7/0/2 не искажён. Оставляю владельцу решить про номер/чистку
(дубликат хелпера в двух co-equal файлах модуля, не компиляторный дефект —
конфликт имён в пакетном коде).

## Модель

sonnet.
