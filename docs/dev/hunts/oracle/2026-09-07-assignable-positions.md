<!-- SPDX-License-Identifier: CC-BY-4.0 -->
КЛЕТКА | дверь `assignable` | оракул: одно правило совместимости — много позиций, и часть о нём не знает

# Разбор: ОРАКУЛ × позиции, кладущие значение в объявленный тип мимо `assignable` (2026-09-07)

**Трек:** oracle · **Клетку назвал интегратор прозой** (у трека oracle сетки нет — так
записано в определении охотника). **Модель охотника:** opus, агент `defect-hunter`,
запущен окном nova-41. **Проб:** 51, каждая своим каталогом с `cmd.sh`; инвентарь с
дословными выводами — `CHECKPOINT.txt` рядом с пробами.

**Повод.** Два замера того же дня показали, что одно правило отвечает по-разному в
зависимости от позиции: №1008 — литерал массива принят в четырёх позициях и отвергнут
в пятой; №959 — тело функции никогда не сверялось с объявленным типом возврата, дверь
завели лишь 2026-09-05. Вопрос охоте был поставлен так: **какие ЕЩЁ позиции не знают об
этой двери?**

**Что оказалось.** Дверь — `types/mod.rs:21324`, у неё РОВНО десять вызывающих
(10920, 12245, 13670, 15896, 16906, 17017, 18119, 20662, 21929, 21967). Всё, чего в этом
списке нет, не судится вовсе. Найдено пятнадцать таких позиций плюс отдельный класс, где
последствие не «неверное число», а чтение чужой памяти и SEGFAULT.

**Заведено в реестр интегратором:** №1014 (позиции мимо двери, блокер), №1015 (спред и
голый срез переинтерпретируют память, блокер), №1016 (координата диагностики теряется
внутри интерполяции). Двадцать одна находка свёрнута в ТРИ строки по последствию и
механизму: чинится класс, а не синтаксис — греп по одной конструкции не находит
соседнюю.

## 1. Находки

НАХОДКА | К1 | `types/mod.rs:8573` (арм `RecordLit` не зовёт `assignable`) против `:13670` (`ro` зовёт) | p01-record-field-ctor | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p01-record-field-ctor/cmd.sh | `Boxy { b: n }` при `n int = 300`, поле `b u8` — принято, печатает `field=44` | `E_IMPLICIT_NARROWING`: D54 — implicit numeric coercion нет НИГДЕ (`spec/decisions/02-types.md:16977`)
НАХОДКА | К1 | тот же арм, литерал вместо переменной | p52-field-lit-out-of-range | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p52-field-lit-out-of-range/cmd.sh | `Boxy { b: 300 }` — принято, печатает `field_lit=44` | `E_LIT_OUT_OF_RANGE`: D55 п.4 НАЗЫВАЕТ поля record-литерала дословно (`02-types.md:1298-1306`)
НАХОДКА | К1 | тот же арм, чужой тип | p28-field-wrong-record | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p28-field-wrong-record/cmd.sh | `Holder { slot: alpha }` при `slot Beta` — принято, программа умирает `nova: out of memory`, `exit=127` | `E7301`, как на контроле `p29-control-local-wrong-record` — та же пара типов через `ro` отвергается
НАХОДКА | К1 | сокращённая инициализация поля `{ b }` — ВТОРОЙ синтаксис того же свойства | p44-record-shorthand | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p44-record-shorthand/cmd.sh | `fn Boxy.make(b int) -> Boxy => { b }` при поле `b u8` — принято, `shorthand=44` | тот же отказ, что у полной формы; греп по `Type { field: … }` эту форму не находит
НАХОДКА | К1 | `types/mod.rs:9875` — гейт присваивания стоит на ФОРМЕ ЦЕЛИ (`if let ExprKind::Ident`) | p02-field-assign | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p02-field-assign/cmd.sh | присваивание в поле — `field_assign=44`; в элемент по индексу (`p05-index-assign`) — `idx=44` | сужение отвергается независимо от формы цели; контроль `neg/n_reassign.nv` на форме `Ident` красный
НАХОДКА | К1 | элемент кортежа в объявленном возврате; в `assignable` нет арма `TupleLit` | p03-tuple-elem-return | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p03-tuple-elem-return/cmd.sh | `fn pair(n int) -> (u8, u8) => (n, 1)` — принято, `tuple_ret=44` | дверь возврата (`:20641`) обязана спускаться в элементы кортежа, как спускается в `ArrayLit` (`:21921`)
НАХОДКА | К1 | правая часть `??` | p08-coalesce-rhs | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p08-coalesce-rhs/cmd.sh | `coalesce=44`; с чужим типом (`p32`) — `nova: out of memory`, `exit=127` | та же дверь, что судит `ro b u8 = n`
НАХОДКА | К1 | аргумент на вызове ЗАМЫКАНИЯ-ЛИТЕРАЛА против значения объявленного fn-типа | p47-fn-value-two-forms | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p47-fn-value-two-forms/cmd.sh | в одном файле две строки задают один вопрос: строка 16 отвергнута (`E_IMPLICIT_NARROWING`), строка 17 — нет | обе формы судятся одинаково; это НЕ остаток №959 (там ТЕЛО замыкания против возврата, здесь АРГУМЕНТ)
НАХОДКА | К1 | payload `Some`/`Ok`: арм ctor-payload (`:21958`) срабатывает только на голом литерале | p11-option-payload | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p11-option-payload/cmd.sh | `payload=44`; `p18-result-payload` — `okpayload=44`; с чужим типом отказ даёт СИ (`p33`) | переменная в payload судится так же, как литерал
НАХОДКА | К1 | модульная `const`, инициализированная другой `const` | p35-module-const-narrowing | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p35-module-const-narrowing/cmd.sh | `const WIDE int = 300; const NARROW u8 = WIDE` — принято, `const_u8_from_int=44` | как `const K u8 = 300` (`p53`), который отвергается корректно — одна дверь, два ответа
НАХОДКА | К1 | значение и ключ map-литерала: `assignable` рекурсирует по `ArrayLit` и не рекурсирует по `MapLit` | p13-map-literal-value | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p13-map-literal-value/cmd.sh | `mapval=44`; ключ (`p14`) — запись 300 легла под ключ 44 | поэлементная проверка, как у массивного литерала (`p04` отвергается)
НАХОДКА | К1 | тело операции effect-обработчика против объявленного `-> T` | p25-handler-op-return | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p25-handler-op-return/cmd.sh | `with Meter = effect Meter { read() -> u8 => n }` — `handler_ret=44`; чужой тип (`p31`) — `exit=127` | дверь возврата обязана доходить до тела handler-литерала
НАХОДКА | К1 | значение по умолчанию у параметра | p37-default-param-value | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p37-default-param-value/cmd.sh | `fn take(x u8 = WIDE)` при `WIDE int = 300` — `take()` печатает `default=44` | то же значение, переданное ЯВНО, отвергается (`neg/n_fnarg.nv`)
НАХОДКА | К1 | элемент вариадического параметра | p38-variadic-arg | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p38-variadic-arg/cmd.sh | `fn total(...xs []u8)`, вызов `total(n, 1)` — `variadic=45` | обычный параметр `u8` то же значение отвергает
НАХОДКА | К1 | аргумент под ЯВНЫМ turbofish | p39-turbofish-generic | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p39-turbofish-generic/cmd.sh | `pass[u8](n)` — принято, `turbofish=44` | тип пришпилен программистом, то есть позиция «с явно ожидаемым типом» по D55 (`02-types.md:1363`)
НАХОДКА | К1 | аннотация типа у переменной цикла `for`; одно имя читается двумя способами | p41-for-binding-two-readings | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p41-for-binding-two-readings/cmd.sh | `for x u8 in src` при `src []int = [300]` — в ОДНОМ теле `x as int` даёт 300, а `sink(x)` даёт 44 | либо отказ на несовпадении, либо одно значение; сегодня чекер считает `x` за `u8`, а хранит 300
НАХОДКА | К1 | вторая ФОРМА свойства, не int→u8: `f64` в поле `f32` | p27-field-f64-into-f32 | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p27-field-f64-into-f32/cmd.sh | `Boxy { b: d }` при `d f64 = 1.0e300`, поле `b f32` — принято, печатает `f32field=inf` | явный `as`: `spec/conversions.ru.md:88` (`f64 → f32 | as | IEEE rounding`)
НАХОДКА | К1 | спред массива переинтерпретирует ПАМЯТЬ (`:18200-18202` — «slips past `assignable`») | p20-array-spread-reinterpret | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p20-array-spread-reinterpret/cmd.sh | `[...src]` из `[]int` в слот `[]u8` — `len=4 1 0 0 0` (сырые байты); обратно (`p21`) — `67305985 = 0x04030201` и чтение за границей; `[]int` в `[]str` (`p22`) — SEGFAULT, `exit=139` | отказ той же двери, что даёт `E_ARG_ELEM_TYPE_MISMATCH`, чей текст сам называет вред: «would reinterpret its contents»
НАХОДКА | К1 | та же дыра в позиции АРГУМЕНТА и в поле — то есть строгая дверь обходится спредом | p23-spread-at-arg | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p23-spread-at-arg/cmd.sh | `show([...src])` при `fn show(t []u8)` — `arg_spread=2 1 0`; в поле (`p24`) то же | позиция аргумента судится и для спреда
НАХОДКА | К1 | голый срез без спреда: локаль, поле, возврат | p48-bare-slice-local | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p48-bare-slice-local/cmd.sh | `ro a []u8 = src` при `src []int` — `len=2 1 0`; поле (`p50`) и возврат (`p51`) — то же | как в аргументе (`p49`), где `E_ARG_ELEM_TYPE_MISMATCH` отвергает
НАХОДКА | К2 | координата диагностики внутри строковой интерполяции | p42-interp-diag-coordinate | sh docs/dev/hunts/oracle/probes/2026-09-07-assignable-positions/p42-interp-diag-coordinate/cmd.sh | один и тот же отвергаемый вызов на строках 15 и 16: первый отказ указывает `probe.nv:1:6` (строка 1 — комментарий) | координата подвыражения интерполяции; пин `nova:expect` — единственная машинная проверка этого

## 2. Что обошёл и почему

- **№959 и №1008 не переоткрывались**: позиции, названные в них, проверены только как
  КОНТРОЛИ (`p00`, `p04`, `p16`, `p17`) — чтобы знать, где дверь работает.
- **`try`/`catch` как позиции в языке нет** (ошибки идут через эффект `Fail` и `Result`),
  поэтому «тело catch» из брифа прочитано как тело операции обработчика и покрыто выше.
- **Локальная аннотация кортежа `ro t (u8, u8) = …` невыразима**: парсер читает такую
  форму как рефутабельный паттерн (`E_REFUTABLE_BINDING`), поэтому позиция кортежа
  измерена через возврат.
- **Хвост ветви `if`/`match` в объявленный тип переменной — искали, дефекта нет**
  (`p06`, `p07` отвергнуты корректно). Это ЗАМЕР, а не отсутствие: хвост ветви в
  позициях возврата, поля и аргумента не проверялся — следующая клетка.
- **Не мерилось:** методы `Vec`/`IndexMap` кроме `push`; каналы (у двери есть вызывающий
  `:10920`, пробы нет); `#coerce`-позиции D429; граница `extern "C"`; цепочки `with_*`;
  поля вложенных записей глубже одного уровня; сумма с payload объявленного типа.
  Пробел назван пробелом.
- **Число носителей каждого класса в корпусе не считалось** — это работа окна: без счёта
  носителей приоритет не назначить.

## 3. Противоречия

**П1. Комментарий компилятора против поведения компилятора на той же форме.**
`types/mod.rs:18200-18202` утверждает: «Scalar coercion (`int`→`u32` OUTSIDE a generic,
e.g. `push(int)` into a `Vec[u32]`) is a value-level truncation and is intentionally NOT
flagged». Замер на ровно этой форме (`p16-vec-push-arg`): `probe.nv:7:12: error:
[E_IMPLICIT_NARROWING] cannot pass 'int' as argument 'v' of narrower type 'u8'`. Одно из
двух устарело — выбор не охотника. (Интегратор: это ЧЕТВЁРТЫЙ за день комментарий,
обещающий не то, что делает код; записано в №1014.)

**П2. D55 называет позицию поля record-литерала нормативно, реализация её не судит.**
Место A — `02-types.md:1298-1306` п.4: «…распространяется на аргументы функций …, let с
аннотацией, **поля record-литерала**. Выход за диапазон — compile error
`E_LIT_OUT_OF_RANGE`». Место B — там же, блок «Статус реализации (2026-05-15)»,
строки 1359-1380: в перечне позиций поля record-литерала не упомянуты ВОВСЕ — ни как
сделанные, ни как отложенные. По таблице нельзя понять, дефект это или объявленный
пробел. Замер: `p52-field-lit-out-of-range` печатает `field_lit=44`.
