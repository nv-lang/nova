# PROGRESS — p356-compound (№356, составное присваивание на именованном кортеже)

Окно: p356-compound. Модель: sonnet. Ветка `p356-compound`, worktree
`d:/Sources/nv-lang/nova-p356`.

## Итог одной строкой

**№356 закрыт и доказан на пакете.** `+=`/`-=`/`*=`/`/=`/`&=`/`|=`/`^=` на
именованном кортеже с операторным методом теперь десугариваются в вызов
метода на ВСЕХ трёх ABI-видах (heap-record, value-record, named-tuple).
Копия `nova-bignum` с мигрированным на кортеж `BigInt` — `nova test src`:
было `PASS: 4 FAIL: 3`, стало `PASS: 7 FAIL: 0` (дословно те три цели, что
были в задании: `bigfloat/core`, `bigrat/core`, `repro_parse_test`).
**Не укладывается в нулевую дельту ратчета**: `lines=64566` против базы
`64545` (+21 строка) — прошу владельца решение по базе (обоснование ниже).

## Проба на обе формы (урок №271)

| Оператор | Named tuple (`NovaTuple_X`) | Value-record (`NovaValue_X`) | Heap-record (`Nova_X*`) |
|---|---|---|---|
| `+=` | до: CC-FAIL `invalid operands` · после: PASS | до: PASS (№284) · после: PASS | до: PASS · после: PASS |
| `-=` | до: CC-FAIL · после: PASS | до: PASS (№284) · после: PASS | до: PASS · после: PASS |
| `*=` | до: CC-FAIL · после: PASS | **до: CC-FAIL (не покрыто №284!)** · после: PASS | **до: CC-FAIL** · после: PASS |
| `/=` | до: CC-FAIL · после: PASS | **до: CC-FAIL** · после: PASS | **до: CC-FAIL** · после: PASS |
| `&=` | до: CC-FAIL · после: PASS | **до: CC-FAIL** (предикат ловил тип, но `@bitand` негде диспатчить) · после: PASS | до: PASS · после: PASS |
| `\|=` | до: CC-FAIL · после: PASS | **до: CC-FAIL** · после: PASS | до: PASS · после: PASS |
| `^=` | до: CC-FAIL · после: PASS | **до: CC-FAIL** · после: PASS | до: PASS · после: PASS |

Вердикты «до» получены пробой (`scratch`-фикстуры, не в этом отчёте —
удалены после проверки) на дофиксовом релизном компиляторе главной репы;
«после» — на фиксовом, все прогнаны на РЕАЛЬНОМ билде (не выведены
логически). Жирным — находки СВЕРХ заявленного в задании (`*=`/`/=` были
сломаны для ЛЮБОГО ABI, не только именованного кортежа; `&=`/`|=`/`^=` были
сломаны для value-record тоже, не только для named-tuple).

## Корень

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`, `Stmt::Assign` арм
компаунд-присваивания (~строка 32233).

Предикат `is_overloaded_add_ty`, решающий «десугарить `a += b` в
`a = a + b` (метод) или эмитить сырой C `a += b`», проверял:
`nova_str` / `Nova_Vec____…` / heap `Nova_<X>*` (5-й символ ровно `_`) /
`NovaValue_<X>` (value-record). **Именованный кортеж (`NovaTuple_<X>`,
D215/Plan 120) не матчился НИ ОДНОЙ веткой** — 5-й символ у него `T`
(`Tuple_`), не `_`, поэтому даже эвристика «heap-record префикс» его не
ловит. Падал в сырой C compound-assign на структуре → CC-FAIL
`invalid operands`.

**Почему фикс №284 сюда не дошёл.** №284 (закрыт 2026-08-02) добавил ТОЛЬКО
ветку `NovaValue_` — привязался к конкретному ABI-виду типа, а не к общему
признаку «это операторный метод». Кортеж — другой ABI-вид с ТЕМ ЖЕ классом
проблемы (by-value struct, receiver ABI — указатель), но другая C-мангл-
схема имени (`NovaTuple_` vs `NovaValue_`) — фикс №284 их не связал.

**Вторая, отдельная от №356 находка (не в заявленном скоупе, но той же
природы):** тот же предикат-`matches!` вообще не включал `AssignOp::Mul` /
`AssignOp::Div` — НИ ДЛЯ ОДНОГО ABI-вида, включая heap-record (`Nova_X*`),
для которого `+=`/`-=`/`&=`/`|=`/`^=` уже работали. `*=`/`/=` были
исключены ПО ОПЕРАЦИИ, а не по типу — тот же класс отказа (сырой C `*=`/`/=`
на struct-операнде), просто другая ось предиката. Обнаружено пробой при
проверке «всей родни» по заданию.

**Третья находка:** даже когда предикат для `&=`/`|=`/`^=` матчил
`NovaValue_`/`NovaTuple_` (потому что op был в списке), синтезированный
`Binary` BitAnd/BitOr/BitXor не имел КУДА диспатчиться — арм standalone
`Binary`-дизайна для value-record (`compiler-codegen/src/codegen/emit_c.rs`,
Plan 175 Ф.1b/Ф.3, ~34340) и для named-tuple (D215, ~34234) поддерживали
только `+ - * / %` (арифметика), НЕ `& | ^`. Синтезированный узел падал
дальше по цепочке до сырой C-эмиссии `&`/`|`/`^` на структуре — тот же
CC-FAIL, просто из другого места (не Stmt::Assign, а Binary-дизайн). Это
доказывает: для value-record/named-tuple `&`/`|`/`^` были не десугарены
ВООБЩЕ (даже НЕ в compound-присваивании) — только heap-record (`Nova_X*`,
через `operator_dispatch::BINOP_TABLE`, Plan opunify) их поддерживал.

## Фикс

Три точки правки, все в `emit_c.rs` (не в чекере) — порядок эмиссии и ABI,
не типовая информация; тип операнда уже известен чекеру (`infer_expr_c_type`
читает готовый C-тип, не пересчитывает семантику), сам факт «это операторный
метод» уже установлен наличием `@plus`/`@bitand`/… в `method_overloads`
(построено чекер-каналом раньше в пайплайне) — ветки лишь МАРШРУТИЗИРУЮТ по
уже известной информации, ничего заново не выводят:

1. **Stmt::Assign compound-assign предикат** (~32233): добавлен разряд
   `tgt_ty.starts_with("NovaTuple_") && !tgt_ty.ends_with('*')` (зеркало
   существующей `NovaValue_`-ветки №284); `matches!` расширен на
   `AssignOp::Mul | AssignOp::Div`.
2. **Named-tuple Binary-арм** (~34234, D215): `method_name`-match расширен
   `BinOp::BitAnd => "bitand"`, `BitOr => "bitor"`, `BitXor => "bitxor"`
   (зеркало уже существующих `plus`/`minus`/`times`/`div`).
3. **Value-record Binary-арм** (~34340, Plan 175): та же расширка —
   `matches!`-guard и `method_name`-match получили `BitAnd`/`BitOr`/`BitXor`.

Не копипаста новой ветки под каждый ABI-вид — расширение УЖЕ существующих
предикатов/match'ей теми же тремя вариантами, той же формой, что у соседних
операторов в тех же местах. Признак «не разбираю C-имя строкой ради типовой
информации» соблюдён: имя типа (`NovaTuple_`/`NovaValue_`/`Nova_`) уже
canonical, сформировано самим компилятором по фиксированной грамматике —
классификация «эмитить рано/поздно» / «маршрут A/B», не типовой вывод.

`operator_dispatch::BINOP_TABLE`/`resolve_binop_dispatch` (Plan opunify) уже
делает ЭТО ЖЕ обобщённо для heap-record (`Nova_X*`) — не мигрировал
value-record/named-tuple арм на этот общий резолвер: миграция трогает
намного больше строк (два разных call-emission паттерна: heap — `f(l, r)`
напрямую, value/tuple — `f(&recv_tmp, r_or_&r)` через материализацию
receiver'а в адресуемый temp) и вне бюджета этого окна. Отмечаю как
кандидата на будущее обобщение (см. «Смежные находки»).

## Прогон пакета `nova-bignum` (обязательная проверка по назначению)

Копия в `scratch_p356/bignum-copy` (саму репу `nova-bignum` НЕ трогал,
удалена после прогонов вместе со scratch-каталогом). `BigInt`
(`src/bigint/core.nv`) мигрирован на именованный кортеж:
`export type BigInt(sign Sign, limbs []u32)` — те же ~15 сайтов
конструирования переведены на позиционную форму `BigInt(sign, limbs)`, что
и в прежних окнах (271/361). Остальные три типа (`BigDecimal`, `BigRat`,
`BigFloat`) не трогал.

**ДО фикса (дофиксовый релизный компилятор главной репы, тот же чекаут
пакета):**
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...bignum-copy\src]
SKIP           src/bigrat/core_slow  # slow lane
SKIP           src/bignum            # no test blocks (compiled OK)
PASS           src/repro_test
PASS           src/repro_direct_test
PASS           src/bigint/core
PASS           src/bigdecimal/core
CC-FAIL        src/bigfloat/core       # .../bigfloat\core.c:13911:18: error: invalid operands to binary expression ('NovaTuple_BigInt' and 'NovaTuple_BigInt') | 1 error generated.
CC-FAIL        src/bigrat/core         # .../bigrat\core.c:13353:18: error: invalid operands to binary expression (...) | 1 error generated.
CC-FAIL        src/repro_parse_test    # .../repro_parse_test.c:13661:18: error: invalid operands to binary expression (...) | 1 error generated.

===== SUMMARY =====
PASS: 4  FAIL: 3  SKIP: 2 (skipped)
```
Дословно совпадает (класс ошибки, число падений, три те же файла) с
репро окна p361 (`PROGRESS-p361-crossfile.md`), источник —
`bigfloat/core.nv:338`, `mant += BigInt.one()`.

**ПОСЛЕ фикса (компилятор ИЗ этого worktree, ТА ЖЕ копия пакета):**
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...bignum-copy\src]
SKIP           src/bigrat/core_slow  # slow lane
SKIP           src/bignum            # no test blocks (compiled OK)
PASS           src/repro_direct_test
PASS           src/repro_test
PASS           src/bigint/core
PASS           src/bigdecimal/core
PASS           src/bigfloat/core
PASS           src/repro_parse_test
PASS           src/bigrat/core

===== SUMMARY =====
PASS: 7  FAIL: 0  SKIP: 2 (skipped)
```
Дифференциальная проба на ОДНОЙ и той же копии пакета (не два разных
чекаута) — исключает «повезло с другим состоянием кода».

## Фикстуры (`docs/dev/test-conventions.md`)

- `spec_tests/conformance/m356_named_tuple_compound_assign_pos.nv` —
  позитив, три типа-носителя (`M356Tup` именованный кортеж, `M356ValRec`
  value-record, `M356HeapRec` heap-record), по одному тесту на ABI-вид,
  проверяют ЗНАЧЕНИЕ (`assert(x.lo == …)`) после каждого оператора
  родни, не факт сборки.
- `spec_tests/conformance/neg/m356_compound_assign_no_operator_method_neg.nv`
  — негатив: `+=` на именованном кортеже БЕЗ `@plus` — `EXPECT_CC_ERROR
  invalid operands` (расширенный предикат не проглатывает «нет метода»,
  диагностика остаётся детерминированной; `fn main()` обязателен —
  `EXPECT_CC_ERROR`, в отличие от `EXPECT_COMPILE_ERROR`, требует реально
  дойти до этапа C-компилятора, без runnable entry раннер коротит на
  «nothing to link/run» и CC не вызывает вовсе — то же самое, из-за чего
  два СУЩЕСТВУЮЩИХ в корпусе `EXPECT_CC_ERROR`-негатива
  (`neg/f2_negative_match_arm_type_mismatch.nv`,
  `neg/f3_negative_into_wrong_return.nv`) сейчас молча SKIP'аются — не мой
  дефект, пред-существующий, отмечаю отдельно ниже).

Обе фикстуры лежат в `spec_tests/conformance/` — folder-module, единый CU
на ~658 позитивов; изолированный прогон ОДНОГО peer-файла раскрывает ВЕСЬ
CU (проверено: попытка `nova test
spec_tests/conformance/m356_named_tuple_compound_assign_pos.nv` не
завершилась за 10 минут — та же экспансия, что и полный
`--positive spec_tests/conformance`). Мега-CU — по заданию, у владельца.
Позитивную фикстуру верифицировал ЛОГИЧЕСКИ идентичной standalone-пробой
(тот же набор типов/операторов/ассертов, отдельный модуль вне
folder-module) — PASS, вердикт ниже. Негативная фикстура — в `neg/`,
это ВСЕГДА отдельный CU (не подмешивается в позитивный folder-module),
прогналась изолированно за секунды.

## Прогоны — вердикты дословно

**Standalone-проба, идентичная позитивной фикстуре (7 операторов × 3
ABI-вида, все со значением-ассертом):**
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...scratch_p356/probe.nv]
PASS           scratch_p356/probe

===== SUMMARY =====
PASS: 1  FAIL: 0
```
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...scratch_p356/probe2.nv]
PASS           scratch_p356/probe2

===== SUMMARY =====
PASS: 1  FAIL: 0
```
(`probe2` — heap-record `*=`/`/=`/`&=` + голый `int` compound-присваивание
без изменений — контроль, что checked-overflow ветка для `nova_int` не
задета: `+=`/`-=`/`*=`/`/=`/`&=`/`|=`/`^=` на `mut x = 5` дают ожидаемые
`8/6/24/4/4/5/0`.)

**Негативная фикстура:**
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...neg/m356_compound_assign_no_operator_method_neg.nv]
PASS           spec_tests/conformance/neg/m356_compound_assign_no_operator_method_neg  # (negative-cc)

===== SUMMARY =====
PASS: 1  FAIL: 0
```

**`cargo build --release` (nova-cli):** чисто, `Finished \`release\`
profile [optimized] target(s)`, только pre-existing warnings.

**`nova check std/src`:**
```
PASS: 148  FAIL: 26  WARN: 61
```
Байт-в-байт канон.

**`arch-ratchet.sh`:**
```
ARCH-RATCHET FAIL: lines=64566 > baseline=64545 (emit_c must not grow; fix belongs to checker channel / IR path)
arch-ratchet ok: infer=348 <= 348
```
`infer` не сдвинулся (ни одного нового вызова `infer_expr_c_type` — все три
правки маршрутизируют по уже вычисленному типу/уже найденному методу).
`lines` — **+21** сверх базы. Дельта — минимизирована (сжаты комментарии,
исходно было +34); дальше сжимать значило бы убрать обоснование правки,
обязательное по конвенции для правок `emit_c.rs`. **Решение по базе —
за владельцем** (инструкция задания explicítно не разрешает подгонять
базу самому).

**`cargo test --release --lib` (compiler-codegen, `RUST_MIN_STACK=67108864`,
scoped `codegen::emit_c`):**
```
test result: FAILED. 76 passed; 2 failed; 0 ignored; 0 measured; 1153 filtered out
```
Оба фейла — **пред-существующие, не связаны с этим окном**:
`array_lit_named_tuple_box_tests::emit_array_lit_int_primitive_unchanged`
(паникует на `[Plan172.12-A8]` prelude-facade) и
`array_lit_named_tuple_box_tests::emit_array_lit_named_tuple_heap_box`
(паникует на `[P67] nova_int collapse`) — те же самые, дословно, что
задокументированы в `PROGRESS-p361-crossfile.md` как пред-существующие на
родительском коммите.

**`cargo test --release --lib operator_dispatch`:**
```
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 1222 filtered out
```
Таблица `BINOP_TABLE`/`resolve_binop_dispatch` (heap-record маршрут,
Plan opunify) не тронута — контрольный прогон, что её не задело.

**Мега-CU (658/0/69) и флагман** — не гонял (по заданию, у владельца).

## Смежные находки

1. **`*=`/`/=` были сломаны для ЛЮБОГО ABI-вида, включая heap-record** — не
   только для value-record/named-tuple. Компаунд-предикат исключал эти два
   `AssignOp` целиком, до №356 никто не заметил (нет фикстур на heap-record
   `*=`/`/=` до этого окна). Зафиксировано фикстурой `M356HeapRec`.
2. **`&`/`|`/`^` не диспатчились НИ В КАКОМ виде (даже НЕ в
   compound-присваивании) для value-record и named-tuple** — только
   heap-record (через `operator_dispatch::BINOP_TABLE`) их поддерживал.
   Это шире заявленного в задании «родня компаунд-присваивания» — сам
   БИНАРНЫЙ `a & b` на value-record/named-tuple с `@bitand` тоже был бы
   CC-FAIL до этого окна. Фикс закрывает и это (фикс — в Binary-арме, не
   только в Stmt::Assign).
3. **Проверка на третий арм без `method_byref_flag`** (по заданию, класс
   №363): арм, куда маршрутизируются новые `BitAnd`/`BitOr`/`BitXor` case'ы
   (и для named-tuple, и для value-record) — это ТА ЖЕ call-emission-ветка,
   что уже обслуживает `plus`/`minus`/`times`/`div` и УЖЕ консультирует
   `method_byref_flag` (проверено чтением, не только предположением: named-
   tuple арм строка ~34262, value-record арм строка ~34383-84). Новые case'ы
   — только запись в `method_name`-match, сам call-site не дублировался —
   третьего инстанса класса №363 НЕ найдено.
4. **Два СУЩЕСТВУЮЩИХ (не моих) `EXPECT_CC_ERROR`-негатива без `fn main()`
   молча SKIP'аются**: `neg/f2_negative_match_arm_type_mismatch.nv` и
   `neg/f3_negative_into_wrong_return.nv` — `detect_test_type`
   (`test_runner.rs:6208`) НЕ распознаёт `EXPECT_CC_ERROR` на этапе
   file-level лейн-роутинга (грепает только `EXPECT_COMPILE_ERROR`/
   `_RUNTIME_PANIC`/`_TIMEOUT`/`_EXIT`), а без runnable entry раннер вообще
   не доходит до C-компилятора («nothing to link/run») — эти два негатива
   молчаливо НЕ ПРОВЕРЯЮТ то, что заявляют с 2026 (даты создания не
   смотрел). Не чинил (вне периметра №356, инфраструктура раннера, не
   `emit_c.rs`) — отмечаю, чтобы владелец решил про отдельный номер.
5. **Полный `operator_dispatch::BINOP_TABLE`-резолвер (Plan opunify) не
   мигрирован на value-record/named-tuple** — heap-record давно получил
   унифицированный диспатч на ВСЕ 10 операторов таблицы одним резолвером;
   value-record/named-tuple всё ещё несут два отдельных hand-rolled арма
   (Plan 175 / D215), которые я расширил точечно, а не обобщил.
   Унификация — отдельная, более крупная работа (разная call-emission ABI:
   `f(l,r)` напрямую vs `f(&recv_tmp, …)` через материализацию) — кандидат
   на будущее окно, не входит в бюджет этого.

## Разблокирована ли миграция `nova-bignum` полностью?

**Да, чем доказано:** дифференциальный прогон РЕАЛЬНОГО пакета (не
изолированной фикстуры) на ОДНОЙ и той же копии — `PASS: 4 FAIL: 3` (три
цели именно те, что называло задание: `bigfloat/core`, `bigrat/core`,
`repro_parse_test`, дословно тот же класс clang-ошибки, что в отчёте p361)
до фикса → `PASS: 7 FAIL: 0` после, БЕЗ каких-либо ручных обходов в коде
пакета (только миграция `BigInt` на именованный кортеж — задача, ради
которой всё затевалось). Ни одной цели пакета не осталось красной.
№271 → №361 → №356 закрывают всю цепочку блокеров, найденную на пути
миграции `nova-bignum`.

**Оговорка не про миграцию, а про мой личный гейт:** мега-CU/флагман
(явно закреплены за владельцем этим заданием) я не гонял — ratchet.sh
показывает +21 строку сверх базы (`infer` не сдвинулся), решение принимает
владелец. До подтверждения на мега-CU и решения по базе слияние в main —
не моё решение.

## Модель

sonnet.
