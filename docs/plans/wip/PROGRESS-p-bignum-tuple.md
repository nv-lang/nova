# PROGRESS: миграция nova-bignum на именованные кортежи (D215)

Окно: p-bignum-tuple. Модель: sonnet. Репа: `nova-bignum` (пакетная, отдельно от
компилятора). Дата: 2026-08-05.

## Итог одной строкой

**Полный откат.** `nova-bignum` в конце окна побайтово идентична `main`
(`git status` чист, HEAD = `6881276`). Миграция была выполнена технически
корректно (проходит `nova check --strict-effects` чисто), но `nova test src`
на неё не проходит — упирается в **три разных дефекта компилятора**, каждый
подтверждён минимальным репро. Ни «мигрировать только BigInt», ни «мигрировать
всю семью разом» не даёт зелёного `nova test src`. Публичный API пакета
**не изменился** — пользователей ничего не задевает.

## Что было сделано (хронология)

1. Осмотрена ветка `p-tuple` (`bfd1194`) — **непригодна для продолжения**:
   48 коммитов позади `main`, до раскладки семьи на папки-модули (план 243
   Ф.U), до появления `BigRat`/`BigFloat`, до докс-сплита. Начал заново от
   `main` в новой ветке `p-tuple-v2`.
2. Смигрировал `BigInt` (`src/bigint/core.nv`) на `type BigInt(sign Sign,
   limbs []u32)` — все ~15 сайтов конструирования переведены на позиционную
   форму (D102), `///`-документация над типом переведена на английский.
   `nova check --strict-effects` — чисто.
3. По той же схеме смигрировал `BigDecimal`+`MathContext`, `BigRat`,
   `BigFloat`+`PrecisionContext` (все четыре типа семьи присутствовали в репе,
   бриф разрешал их включать раз «уже есть, не заводить новых»). Все четыре
   файла прошли `nova check --strict-effects` чисто.
4. `nova test src` вскрыл дефект компилятора №1 (см. ниже) — 4 из 7 целей
   `CODEGEN-FAIL` с одинаковой сигнатурой. Нашёл и применил САНКЦИОНИРОВАННЫЙ
   чекером обходной путь для смежной находки (см. дефект №1, `.clone()`
   оказался сам сломан) — заменил его на `mut`-параметр приватного хелпера
   (D326 in-out), это сработало и убрало CODEGEN-FAIL.
5. После фикса — `nova test src` продвинулся дальше, но вскрыл дефект №2
   (операторный десугаринг) и дефект №3 (кросс-файловый конфликт типов,
   тот самый класс, что диагноз №271 «закрыл» СЕГОДНЯ, буквально несколькими
   часами ранее). Ни один встречный ход в коде пакета эти два не снял.
6. Проверил: работает ли «мигрировать ТОЛЬКО BigInt, остальное как было»
   (самый консервативный вариант) — **тоже нет**: дефект №3 воспроизводится
   уже на паре `BigInt(кортеж) + BigDecimal(record)`, без BigRat/BigFloat
   вообще.
7. Заключение: ни один из опробованных срезов миграции не даёт зелёного
   `nova test src`. Откатил ВСЕ четыре файла к `main` (`git checkout main --
   src/bigint/core.nv src/bigdecimal/core.nv src/bigrat/core.nv
   src/bigfloat/core.nv`). Рабочее дерево чисто, `git status` пуст.

## Вердикты прогонов — дословно (финальное состояние = main, без изменений)

`nova check src --strict-effects`:
```
===== SUMMARY =====
PASS: 8  FAIL: 0  WARN: 15
```
(все 15 warning — pre-existing unused-import в тестовых файлах, не мои).

`nova test src`:
```
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-bignum\src]
SKIP           src/bigrat/core_slow  # slow lane — requires --include-slow/--slow-only
SKIP           src/bignum  # no test blocks and no fn main() — nothing to link/run (compiled OK)
PASS           src/repro_test
PASS           src/repro_direct_test
PASS           src/bigint/core
PASS           src/bigdecimal/core
PASS           src/bigfloat/core
PASS           src/repro_parse_test
PASS           src/bigrat/core

===== SUMMARY =====
PASS: 7  FAIL: 0  SKIP: 2 (skipped)
```

`nova lint src`:
```
lint: 14 file(s), 0 finding(s)
```

Всё три вердикта — состояние **ДО** окна, не поменялось (полный откат).

## Изменения публичного API

**Нет изменений.** Пакет идентичен `main` (`6881276`). Ничего не сломано у
существующих потребителей `nova-bignum`.

## Находки — три дефекта компилятора, каждый с минимальным репро

Компилятор не трогал (по правилам окна). Номера присвоит интегратор.

### Дефект A: синтетический `.clone()` в позиции field-launder ломает кодоген ЛЮБОГО следующего instance-метода

**Контекст.** Вчерашняя волна (2026-08-01, `[M-router-handler-mut-capture-
escape-soundness]`) добавила энфорс `E_READONLY_COERCE` на `mut x = @field`,
когда тип поля не «полностью-стековый» (D246 §72). Диагностика сама
предлагает три решения, включая «(b) скопируй явно — `.clone()`».

**Проблема 1 — `.clone()` в этой позиции не настоящий метод.** Голый
`x.clone()` на именованном кортеже (`BigInt`, вне позиции `@field.clone()`)
даёт:
```
[E7320] no field or method `clone` on named tuple `BigInt`
  note: `BigInt` has fields: sign, limbs
```
То есть `.clone()` работает ТОЛЬКО в буквальной позиции `@field.clone()` —
это текстовый паттерн-матч в чекере для конкретно этой диагностики, а не
диспетч на реальный метод.

**Проблема 2 — синтетический `.clone()` ломает кодоген следующего вызова.**
Минимальный репро (9 строк, полный проходит `nova check`, падает на
`nova test`):
```nova
module bignum.bigdecimal

import bignum.bigint.{BigInt}

export type BigDecimal(mant BigInt, scale int)

export fn BigDecimal @normalize() -> BigInt {
    ro mant = @mant.clone()
    ro (q, rem) = mant.div_rem((10).to_bigint()) ?? panic("boom")
    q
}
```
Вердикт `nova test src` дословно:
```
CODEGEN-FAIL   src/bigdecimal/core  # codegen error: [E_RECV_METHOD_MISMATCH]
`.div_rem(...)` на ресивере типа `StringBuilder` — у `StringBuilder` нет
метода `div_rem`, а single-key fallback резолвит имя в чужой тип `BigInt`
(last-wins) — вызов через чужой layout отвергнут (strict-mode, зеркало
E_UNKNOWN_TYPE_METHOD).
```
Приёмник `mant` РЕАЛЬНО имеет тип `BigInt` — сообщение об ошибке само
противоречиво (говорит про `StringBuilder`, которого в файле нет вообще —
подтверждено: ошибка воспроизводится даже без импорта `StringBuilder` в
файле). Проверено также с `.compare(...)` вместо `.div_rem(...)` — тот же
класс ошибки, значит дефект общий для ЛЮБОГО метода после `@field.clone()`,
не специфичен к `div_rem`.

**Найденный обход (САНКЦИОНИРОВАННЫЙ, не костыль):** объявить параметр
приватного хелпера как `mut` напрямую (D326 in-out), вместо лаундера поля
через локальную `mut`-переменную:
```nova
fn normalize_loop(mut mant BigInt, mut scale int) -> (BigInt, int) {
    ...
    (mant, scale)
}
export fn BigDecimal @normalize() -> BigDecimal {
    ro (mant, scale) = normalize_loop(@mant, @scale)
    BigDecimal.new(mant, scale)
}
```
Этот вариант не требует ни `.clone()`, ни лаундера через `@field` →
`mut`-локал — параметры не подпадают под D246 §72/§73 вовсе. Применил его
к `BigDecimal @normalize`/`BigFloat @normalize` — снял CODEGEN-FAIL этого
класса. (Фикс сейчас не в коде — откачен вместе со всем остальным, но
работоспособен, воспроизведён дважды.)

### Дефект B: операторный десугаринг (`==`/`*`/...) роняет `&`-адресацию, если операнд — `@field`/`other.field`, а не голая переменная

Минимальный репро (2 файла):
```nova
// bigrat/core.nv
module bignum.bigrat
import bignum.bigint.{BigInt}
export type BigRat(num BigInt, den BigInt)
export fn BigRat.new(num BigInt, den BigInt) -> BigRat => BigRat(num, den)
export fn BigRat @equal(other BigRat) -> bool {
    @num.equal(other.num) && @den.equal(other.den)
}

// main_test.nv
module bignum.main_test
import bignum.bigrat.{BigRat}
fn br(n int, d int) -> BigRat => BigRat.new(n.to_bigint(), d.to_bigint())
test "BigRat == " {
    ro a = br(1, 2)
    ro b = br(1, 2)
    assert(a == b)   // единственная строка, которая ломает сборку
}
```
Вердикт (`nova test`) дословно:
```
CC-FAIL   src/main_test  # .../main_test.c:12018:65: error: passing
'NovaTuple_BigRat' (aka 'struct NovaTuple_BigRat') to parameter of
incompatible type 'NovaTuple_BigRat *' (aka 'struct NovaTuple_BigRat *');
take the address with &
```
В сгенерированном `.c` явный `.equal()`-вызов (`a.equal(b)`) адресует ОБА
аргумента через `&`, а `==`-десугар (`a == b`) на той же паре переменных —
только приёмник, второй операнд идёт по значению, хотя сгенерированная
сигнатура `Nova_BigRat_method_equal` требует указатель. Это НЕ зависит от
числа переменных в скоупе (проверено — с двумя vs тремя переменными
одинаково) и НЕ специфично для `BigInt`-тестов (там `==` использовался
только как `a == a`/`a != b` — тот самый паттерн просто не был покрыт).

**Побочное наблюдение (не проверено до конца, но согласуется):** в
`BigDecimal @times` (`@mant * other.mant` — `*` на `BigInt`-полях,
project-полях, не голых переменных) тест `div: инвариант
a.div(b,mc).times(b) == a` падал НЕ на строгой ошибке компиляции, а на
`RUN-FAIL` (неверное значение времени выполнения): `q.times(b).equal(a)`
даёт `false` вместо `true`. Изолированный ручной прогон того же
`to_str()`/parse-пути (без остальной test-suite в CU) даёт ПРАВИЛЬНОЕ
значение — то есть сама логика парсинга/форматирования не сломана, порча
проявляется только внутри полного compile-unit'а с другими тестами. Это
согласуется с тем же классом дефекта Б (пропуск `&` на операнде-проекции
поля), только здесь C-компилятор ошибку не ловит (типы совпадают
достаточно, чтобы скомпилироваться, но значение читается неверно) —
тихая порча вместо явного отказа сборки.

### Дефект C: `Nova_BigInt_method_equal` conflicting types — кросс-файловый, №271 закрыт СЕГОДНЯ не покрывает этот случай

**№271** (`docs/plans/221.1-bug-sweep.md`, окно `p271-mangle`) закрыт СЕГОДНЯ,
буквально за несколько часов до этого окна, с формулировкой «миграция
`nova-bignum` на кортежи РАЗБЛОКИРОВАНА». Диагноз №271: `@equal` рядом с
методом, возвращающим `Result[(Self,Self), E]` (`@div_rem`), эмитится
слишком поздно — С-компилятор видит implicit-декларацию раньше настоящего
прототипа. Фикс — предикат `debt_is_late_emitted_value_payload` расширен
на позиционный кортеж. Проверка по назначению в записи №271: «реально
мигрированный `bigint.nv` собирается и исполняется» — но эта проверка была
**standalone** (только сам `bigint.nv`), не в связке с остальным пакетом.

Мой репро показывает: тот же класс ошибки воспроизводится СНОВА, если
`BigInt`-кортеж используется файлом-потребителем (`BigDecimal`, ещё
value-record, ничего в нём не менял):
```
nova test src/bigint src/bigdecimal
```
Вердикт дословно:
```
CC-FAIL   src/bigdecimal/core  # .../bigdecimal\core.c:2304:18: error:
conflicting types for 'Nova_BigInt_method_equal' | 1 error generated.
```
В сгенерированном `.c` `Nova_BigInt_method_equal` первый раз ИСПОЛЬЗУЕТСЯ на
строке 565 (внутри авто-производной структурной `@equal`-функции самого
`BigDecimal` — `record`, не кортеж), а НАСТОЯЩАЯ декларация метода стоит на
строке 2304 — С видит implicit-декларацию на 565 и настоящий прототип на
2304 конфликтуют. Тот же общий класс, что диагностировал №271, но
триггер — КРОСС-ФАЙЛОВЫЙ (потребитель типа, не сам тип), фикс №271 (по
позиционному кортежу внутри ОДНОГО файла) его не покрывает.

**Важно для интегратора:** это значит, что закрытие №271 не разблокировало
миграцию `nova-bignum` целиком — только узкий standalone-случай. Реальная
миграция пакета (где `BigInt` используется остальными тремя файлами семьи)
упирается в этот кросс-файловый вариант того же класса СРАЗУ, на самом
маленьком возможном срезе (`BigInt`-кортеж + `BigDecimal`-record, без
BigRat/BigFloat вообще).

## Проверенные варианты среза миграции — ни один не даёт зелёного `nova test src`

| Срез | `nova check` | `nova test src` |
|---|---|---|
| Вся семья (BigInt+BigDecimal+BigRat+BigFloat) в кортежи | чисто | Дефект A (снят обходом) → Дефект B (CC-FAIL на BigRat, RUN-FAIL на BigDecimal/BigFloat) |
| Только `BigInt` в кортеж, остальное как было | чисто | Дефект C (CC-FAIL на BigDecimal уже при паре `bigint+bigdecimal`) |

## Что осталось (на будущее окно, после фиксов компилятора)

- Сама механика миграции (позиционные конструкторы вместо record-литералов,
  D102-энфорс, `///`-доки на английском) — отработана и подтверждена рабочей
  на уровне `nova check --strict-effects` для ВСЕХ четырёх типов семьи плюс
  `MathContext`/`PrecisionContext`. Как только дефекты A/B/C закрыты —
  повторная миграция должна пройти быстро (тот же приём, что делали дважды
  до этого — механическая правка, а не заново придуманный подход).
- Обход дефекта A (`mut`-параметр вместо `mut x = @field.clone()`) стоит
  занести в шпаргалку для следующего окна — рабочий, ничего не ломает.
- Известный отдельный пробел `BigDecimal.to_str()` в интерполяции (упомянут
  в задании) — НЕ проверялся отдельно в этом окне: дефекты A/B/C блокировали
  раньше, чем дошло бы до него.
- `[M-named-tuple-cu-recv-method-misresolution]` он же класс дефекта B
  (StringBuilder receiver mismatch) и кросс-файловый вариант дефекта C
  (Nova_BigInt_method_equal conflicting types) — оба требуют присвоения
  номеров интегратором; тексты выше готовы к копированию в
  `docs/plans/221.1-bug-sweep.md`.

## Модель

sonnet.
