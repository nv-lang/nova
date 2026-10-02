# REPORT-carina-sweep-10 — Serialize (в), D46 `str`, `if <вариант>`, `for` по итератору (ветка p274-carina-sweep)

**База меры: 38, ICE 0** на `6d8fad7ec` (шаг 9, после размаскирования модуля `check/`). Синка нет — по
слову интегратора работаю на своей ветке; на 19:58 шаги 8–9 в `origin/main` ещё не влиты.

## ИТОГ

| что | было → стало | путь | довод | места |
|---|---|---|---|---|
| 1. `#impl(Serialize)` | общий текст атрибута → названный отказ со сроком E2-b3 | в | ваш выбор (в); D341 | check/decl_forms.nv, check/messages.nv |
| 2. D46 `str > str` | 3 отказа → 0 | б | D46 (упорядочивание — `@compare`) | check/operators.nv, emit_c/emit_expr.nv, builtins |
| 3а. `if <вариант> = ..` | 1 → 0 (класс: и `while`) | б | D486 s4 | check/cond_forms.nv, lower/lowering.nv |
| 3б. `for` по итератору | 1 → 0 | б | D58 | check/for_iter.nv (новый), lower/ir.nv, lower/lowering.nv, emit_c/emit_place.nv |

**Мера 0.2: 38 → 34, ICE 0.** Ушли 3 (D46) + 1 (`if <pattern>`) + 1 (`for`) = 5; добавился 1 — после
снятия `if DefFn(row) = def.target` тело метода в check/type_of.nv типизируется дальше и показало
второй «приёмник `value`-записи по указателю» (:710) — это №1654, у cast. Текстов отказов
подмножества: 140 → 139 (страж `subset-debt-dated` ok; новый текст Serialize +1, два текста
`if`/`while`-границы −2).

**Поправка после полного раннера:** `check-novac-deps` нашёл ребро `lower -> builtins`, которого нет в
таблице §3 (`@variant_test` брал `OPTION_SOME_VARIANT`). Ребро не добавлено — вопрос поднят в `sem`
(`is_option_some_pattern`, поверхность sem 344 → 345 с записью в базе), коммит `9ccdf97b2`; поведение
не изменилось (мера 34, смоук байт-в-байт, `nova test novac/src` 10/10).

## 1. `#impl(Serialize)` — вариант (в)

Отказ остаётся, но назван: «`#impl` of this protocol is derived only for `Display` and `Equal` --
the derived methods of the others (`Serialize`, D341) and their callers dispatch through protocol
bounds, not compiled yet (E2-b3)». Та же одна дверь атрибута — текст выбирается по атрибуту (прочие
атрибуты, `#cfg` и т. п., говорят прежним текстом).

## 2. D46 на `str` — сравнения строк

Расширена та же дверь `==` (`@record_operator_callee`): `<`/`<=`/`>`/`>=` на двух `str` ищут строку
`compare` тем же поиском, что `==` на `str`, и проходят тот же вопрос к оболочке
(`@accept_equality_row` теперь спрашивает метод СТРОКИ, а не только `equal`); эмиттер берёт метод из
записанной строки и печатает `compare(a, b) <op> 0` — как оракул. `COMPARE_METHOD` в builtins.

**Находка про оракула (№TBD в 221.1):** `"a" < 1` проходит `nova check`, а C не собирается — диспатч
упорядочивания на `@compare` не сверяет правый операнд с параметром.

## 3а. `if <вариант> = ..` и `while <вариант> = ..` — любой вариант любой суммы

Носитель: `if DefFn(decl_row) = def.target` (check/type_of.nv:703). Граница «только `Some(name)`» снята
для `if` и `while` (класс): вариант судит общая дверь образца `@bind_pattern` (рукава `match`),
инициализатор обязан быть суммой (Option, Result, объявленная сумма) — иначе отказ по имени.
Lowering: `@variant_test` — `Some` над Option сохраняет `Cond.IsSome` (C всех прежних носителей не
изменился), любой другой вариант — проверка головы рукава `Cond.Holds(PatTest)`; `while` по такому
варианту кладёт тело в then-ветку проверки и выходит из else (у `Holds` нет отрицания).
**Тесты, закреплявшие старую границу, переведены на новое правило — не ослаблены:** check_test.nv
(№1259: квалифицированная и голая головы читаются одинаково, `int`-инициализатор отвергается;
№1380: голый вариант без данных в условии читается), subset_test.nv и subset_pattern_test.nv
(текст для не-суммы; голова, не называющая вариант суммы, — отказ двери образца).
**Находка про оракула (№TBD):** `if Dot(x) = <int>` проходит `nova check`, а C не собирается.

## 3б. `for` по итератору (D58)

Носитель: `for _ in name.chars()` (check/decl_forms.nv:127). По D58 `for x in c` — это `iter()`, затем
`next()` в цикле; у итератора `iter()` — он сам, и оракул зовёт `next` прямо:
`Nova_CharsIter_method_next(&it)`. Сделано так же: голова, у типа которой есть `next() -> Option[T]`,
обходится вызовом `next` каждый оборот (новый `Rvalue.CallNext`, `@lower_for_iter`; приёмник — по
указателю, где строка его так берёт, D488). Голова-коллекция, которой нужен `iter()` (не `[]T`), —
честный отказ со сроком (274.7 B12), текст называет оба вида голов.

**Для cast (№1654):** явный `it.next()` на `value`-записи из std у Карины печатается как `next(it)`
вместо `next(&it)` — clang падает (замерено на пробе `while Some(_) = it.next()`). В моём пути
`for` указатель ставится по `row_recv_by_pointer` без ограничения `in_program`, как и требует ABI
оболочки; у общего вызова метода ограничение `in_program` остаётся — это ваш №1654.

## ФИКСТУРЫ И ТЕСТЫ

- `str_ordering/pos_1` (четыре оператора, отличие в байте/длине/равенство, пустая строка) — байт-в-байт;
  `neg_1` — `"a" < 1`.
- `iflet_variants/pos_1` (вариант своей суммы с данными и без, значением и инструкцией, `Ok`/`Err`
  `Result` из std `read`, `None`, `while` по варианту) — байт-в-байт; `neg_1` — вариант над `int`.
- `for_iter/pos_1` (`str.chars()` с `_` и с привязкой, пустая строка, свой итератор с `mut @next`,
  `break`) — байт-в-байт; `neg_1` — голова-вызов, возвращающий `int` (оракул отказывает на кодогенерации).
- Модульные тесты: str_ordering_test.nv, for_iter_test.nv (новые); `nova test novac/src`: 10/10.
- **В обе стороны:** D46 — судья без пары `str` даёт pos_1 5 отказов и краснит тест, печать без
  `<op> 0` даёт неверные строки; `if <вариант>` — граница `Some` возвращена → 6 отказов, всё через
  `IsSome` → ICE; `for` — дверь итератора закрыта → 4 отказа и красный тест, приёмник по значению →
  clang падает.

## СТРАЖИ

Полный раннер (`scripts/tools/novac-gate-guards.sh`) на `e570cc6f2`: **99 прогнано, 3 красных, все не от
шага** — `shell-freshness` (оболочка против эмиссии оракула; красна и на main), `time-ledger` (неглубокий
клон облака), `no-panic` (снят пределом 240 с, вердикта нет).

Первый полный прогон нашёл четыре мои недоделки — все исправлены, поведение не менялось:
`deps` (ребро `lower -> builtins`, коммит `9ccdf97b2`, см. поправку выше), `import-exists` (`no_subst`
из чужого модуля, лишний `OPTION_SOME_VARIANT`), `type-field-docs` (без дока `@call_next`),
`mangling-one-way` (тест сверял ABI-приставку `Nova_str_method_`) — коммит `e570cc6f2`. После них:
мера 34, ICE 0; `nova test novac/src` 10/10; три `pos_1` байт-в-байт.

## РЕЕСТР

Две строки №TBD (после №1643): оракул пропускает `str < int` (К2) и `if <вариант> = <не сумма>` (К2);
обе — «чекер не сверяет тип в позиции», родня №906, БЛОКИРУЕТ ТЕГ: НЕТ (отказ громкий, на сборке C).

## КОММИТЫ

```
dbf4aa366 novac: `#impl(Serialize)` is refused by its cause and its stage (E2-b3)
cad5e24f7 novac: an ordering of two `str` calls `str @compare` (D46)
dbae7932a novac: `if <variant> = ..` and `while <variant> = ..` read any variant of any sum
2eba34b39 novac: a `for` over an iterator calls its `next` each turn (D58)
9ccdf97b2 novac: lower asks sem whether a pattern is `Some` over an Option
e570cc6f2 novac: sweep 10's leftovers the full guard run named
```
плюс коммит отчёта и двух строк реестра.

## ВОПРОСЫ ИНТЕГРАТОРУ

1. Номера двум строкам №TBD.
2. Очередь пуста — что дальше? (Синк сделаю, когда шаги 8–9 появятся в main.)
