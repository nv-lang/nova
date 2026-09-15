<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# F3 (К7) — ворота набора принтеров стоят на ОДНОЙ из двух ветвей одного цикла

**Трек:** novac · **Клетка:** `emit_c` × К7 · **Найдено охотником 2026-09-16.**

**ОГОВОРКА ПЕРВОЙ СТРОКОЙ:** единственный сегодняшний носитель — `char` — УЖЕ
описан, дословно, в шапке `novac/fixtures/println_literals/pos_1.nv`. Новое здесь
не носитель, а КЛАСС: там дефект объяснён отсутствием char-принтера в
`printer_of`, то есть как дырка в таблице эмиттера. Замер показывает другое —
ворота, которые обязаны не пустить непечатаемый тип до эмиттера, **не вызываются
на литеральной ветви вовсе**. Починка `printer_of` носителя снимет, а класс
оставит: следующий непечатаемый литерал пройдёт тем же путём.

## Свойство

Аргументы `println` типизируются циклом из двух ветвей
(`novac/src/check/typing.nv:836-893`): ветвь `NodeKind.Lit` и ветвь
`is_expr_kind(ck)`. Ворота набора принтеров — `@has_printer` — стоят **только на
второй**. Первая проходит мимо них к `@record(...)` и отдаёт эмиттеру тип,
для которого принтера нет.

## Два адреса, отвечающие на один вопрос по-разному

Вопрос: **есть ли у `println` принтер для этого типа?**

* `novac/src/check/typing.nv:871-874` (ветвь выражения) — спрашивает ворота и
  отказывает по имени:

  ```
  if !@has_printer(at) {
      @report_first_leaf_of(branch_children(c), "outside the subset: println covers int, str, bool and f64 today")
      return
  }
  ```

* `novac/src/check/typing.nv:838-850` (ветвь литерала) — **не спрашивает
  ничего**: строковый литерал уходит в `@judge_interp_lit` и `continue`, любой
  другой — в `@record(c, type_of(c, @ctx, @scope, @chan))`. Слова `has_printer`
  на этой ветви нет; во всём дереве у `@has_printer` **один** вызов
  (`grep -rn "has_printer" novac/src --include=*.nv` — два попадания: объявление
  `check/operators.nv:209` и вызов `check/typing.nv:871`).

Набор ворот — `novac/src/check/operators.nv:216-217`:
`is_numeric_prim || str || bool`. `char` в него НЕ входит. То есть ворота
знают правильный ответ и для литерала — их просто не зовут.

## Утверждение, которое от этого стало ложным

`novac/src/check/typing.nv:864-870`, дословно:

> ```
> // The printer set is int/str/bool/f64. Every other primitive is
> // NAMEABLE (the universe holds all fifteen) but not yet
> // printable, and that boundary belongs here -- the emitter's ice
> // for a missing printer must never be the first answer. [INV-PROPERTY]
> // [INV-PROPERTY] -- the refusal returns two lines below, so the
> // emitter is not reached at all for this form; violating it would
> // require deleting the check, not merely getting it wrong.
> ```

`[INV-PROPERTY]` по конвенции означает «держится КОНСТРУКЦИЕЙ, стража не надо».
Конструкцией держится ровно «for this form» — для ветви выражения. Названный же
инвариант («the emitter's ice for a missing printer must never be the first
answer») нарушается соседней ветвью того же цикла, двенадцатью строками выше, и
нарушить его оказалось можно, НЕ удаляя проверку.

## Что происходит — две половины, один тип

| проба | форма | `novac check` | `novac emit` | оракул |
|---|---|---|---|---|
| `lit-char/` | `println('z')` | **0 (принял)** | **2 — ICE** | 0 |
| `expr-char/` | `ro c = 'z'` затем `println(c)` | 1 — чистый отказ | 1 | 0 |

ICE дословно:

```
{"id":"ice","code":"E_NOVAC_ICE",...,"message":"internal compiler error (novac bug, not yours): emit_c: printer_of reached a primitive family without a printer (char and unit; str and bool are answered above)"}
```

Отказ на второй половине дословно:

```
outside the subset: println covers int, str, bool and f64 today
```

Один и тот же тип `char`, один и тот же вопрос, два ответа — по тому, в какую
ветвь цикла попал аргумент.

## Что должно было бы и по какому правилу

Шапка самого `printer_of` (`novac/src/emit_c/emit_expr.nv:126-135`) называет
правило:

> `the checker's printer set (`@is_printable_carrier`) refuses first.`

Это отложенная проверка с адресом — и адрес существует, дверь открывается, но на
литеральной ветви в неё никто не стучит. Тем же словом конвенции novac П6
(«Никаких тихих дыр») и шапка фикстуры: *«a boundary is held by the checker,
never by an emitter's ice»*.

## Воспроизведение

```sh
# из корня дерева
sh docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f3-printer-gate-literal-half/lit-char/cmd.sh
sh docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f3-printer-gate-literal-half/expr-char/cmd.sh
```

## Искал вторую форму — не нашёл

Носителей литеральной ветви, кроме `char`, сегодня нет: литеральные токены — это
int, float, str, bool и char, и первые четыре в наборе ворот есть. `unit` в
`printer_of` тоже ицает, но литерала единицы в языке нет, так что этой ветвью он
недостижим. Искал грепом по `TokenKind` и по армам `printer_of`.
