<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Карта остатка самосборки novac — 2026-09-23

Замер, а не правка: код не тронут, строки реестра не заведены. Задание окна
Карины (`nova-8a`), согласовано с интегратором (`nova-e7`). Снял помощник
(`nova-93`).

## Условия замера

- Дерево `nova-wt-research` == `main`@`8f0116f41` (плюс мои docs-коммиты вне
  `novac/src`). Починка пачки `8e9de06e8` (rc=127, №1310) в дереве есть.
- `novac.exe` собран из этого же `novac/src` в 22:08; после сборки `novac/src`
  в `main` не менялся.
- Пачка — ровно как в `scripts/tools/novac-diff-corpus.sh:305`: список
  `novac/src/*/*.nv`, затем `novac/src/*.nv`; ОДИН процесс; окружение
  `NOVAC_SELF_PATH=novac/src`; каталог — корень дерева. Файл отвергнут, если хоть
  одна диагностика несёт его в поле `file`.
- Итог пачки: `rc=1`, 108 файлов, 4225 диагностик, 0 `E_NOVAC_ICE`, 0 строк не-JSON,
  0 диагностик на файл вне списка. **Принято 13 / 108, отвергнуто 95** — сходится с
  числом интегратора (95).
- Строка режима из официального прогона скрипта: `self-distance=95/108 self-mode=batch` (`sh scripts/tools/novac-diff-corpus.sh examples/flagship/aggregator/regressions/monotonic_now_bare_binding`, `EXIT=0`, 27 с). Корпус заменён ОДНИМ файлом, чтобы не ждать весь корпус примеров: на общей машине он не уложился в 9 минут и был снят таймаутом (`EXIT=124`). Самосборочная часть скрипта от корпуса не зависит — она идёт по `novac/src`. Поэтому в той строке осмысленны только поля `self-*`; числа корпуса (`contract-match=0` …) относятся к одному файлу.

Первая причина файла — его САМАЯ РАННЯЯ диагностика по смещению. Это первое,
что увидит тот, кто откроет файл, — но не обязательно единственное, что держит
файл: у большинства файлов диагностик десятки.

## Главное: у 15 файлов первая причина — ЛОЖНАЯ

Семейство F2 (15 файлов): novac говорит «поля нет» (`unknown field`, «record
construction names a field the type does not declare (#812)») или «имени нет»
(`unknown name`). А поле или константа в дереве объявлены — в другом файле:

- 11 файлов — в соседнем файле того же модуля;
- 4 файла — в другом модуле: у одного проверено глазами (`SourceText`), у трёх
  (`emit_c/emit_expr.nv`, `emit_c/emit_place.nv`, `pipeline/prims_test.nv`) — только
  по имени поля.

Проверено глазами, с точным типом получателя, на семи случаях:

| место отказа | что «нет» | где на деле объявлено |
|---|---|---|
| `check/exprs.nv:64` | поле `ctx` | `Checker`, `check/check.nv:73` |
| `check/diagnostics.nv:48` | поле `src` | `Checker`, `check/check.nv:72` |
| `sem/callables.nv:137` | поле `rows` | `TypeDef`, `sem/sem.nv:121` |
| `check/check_test.nv:22` | поле `text` у `SourceText { text }` | `SourceText`, `source/source.nv:33` |
| `check/run.nv:65` | поля `src`, `ctx` в конструкторе `Checker` | `check/check.nv:72-73` |
| `sem/binding_test.nv:28` | поле `name` у `ParamDef { ... }` | `ParamDef`, `sem/binding.nv:104` |
| `check/variant_rules.nv:50` | константа `VARIANT_VALUE_NEEDS_ARGS_MSG` | `check/messages.nv:140` |

Оставшиеся восемь сошлись по имени поля: поле с таким именем объявлено в дереве
у каждого, у пяти — в типе того же модуля. Тип получателя по ним не проверялся.

**Минимальная проба, обе стороны.** Модуль `m` из двух файлов, пачкой,
`NOVAC_SELF_PATH=.`:

| проба | ответ novac |
|---|---|
| тип `Box { x int }` в `a.nv`, `fn Box @get() -> int => @x` в `b.nv` | `unknown field` ровно на `x` |
| то же в одном файле (контроль) | отказов нет |
| тот же двухфайловый, БЕЗ `NOVAC_SELF_PATH` | `undeclared type name Box` (соседний файл не виден вовсе) |
| `b.nv` в одиночку, с `NOVAC_SELF_PATH` | `unknown field` на `x` (состав пачки роли не играет) |
| константа `K` в `a.nv`, `fn f() -> int => K` в `b.nv` | `unknown name` на `K` |
| то же в одном файле (контроль) | отказов нет |

Что форма законна, доказывает сам оракул: он собирает `novac.exe` из этого же
дерева, где `check/exprs.nv` читает `Checker.ctx` из соседнего `check/check.nv`.

**Что из этого следует.** Под `NOVAC_SELF_PATH` тип из другого файла виден по
ИМЕНИ (нет «undeclared type»), но его ПОЛЯ и константы модуля — нет. Для 15
файлов «первая причина» в карте — не настоящая причина: что их держит на деле,
станет видно только после починки видимости.

**Пересечение с №1284 — решать не мне.** №1284 объясняет «unknown field» на
самопроверке как КАСКАД после ложной D84-неоднозначности (методы проверяемого
файла регистрируются дважды). В минимальной пробе вызовов нет вовсе, D84
взяться неоткуда, а `unknown field` есть; и у этих 15 файлов ложная диагностика —
самая ранняя в файле, а не следствие более ранней. То есть это либо второй,
прямой механизм рядом с №1284, либо №1284 объяснил свою «unknown field» не
целиком. Сама ложная D84 из №1284 в карте тоже есть — первая причина у
`parse/expr.nv:67` (семейство F11).

## Ближе всех к принятию — одна диагностика на файл (15 файлов)

Такой файл держит ровно одна причина. Сняв её, файл перейдёт в принятые, если за ней не прячется следующая — пачка показывает только то, до чего дошла.

- `novac/src/builtins/builtins.nv:430` — outside the subset: this type form is not compiled yet (slices, tuples, fn types and quali
- `novac/src/check/calls.nv:820` — outside the subset: an `if` in value position is not compiled yet
- `novac/src/check/tail_rules.nv:321` — outside the subset: an `if` in value position is not compiled yet
- `novac/src/check/type_of.nv:522` — outside the subset: an `if` in value position is not compiled yet
- `novac/src/check/typeref_rules.nv:278` — outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b)
- `novac/src/emit_c/emit_c.nv:226` — outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b)
- `novac/src/emit_c/emit_interp.nv:246` — outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b)
- `novac/src/parse/parse.nv:183` — outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b)
- `novac/src/pipeline/handed_sum_test.nv:13` — outside the subset: the shell novac links into carries no `Vec` instance for str -- the in
- `novac/src/pipeline/prims_test.nv:22` — unknown field: this type has no field with this name
- `novac/src/resolve/resolve_test.nv:25` — outside the subset: a typed array literal is written `[]T.of(...)` -- this constructor is 
- `novac/src/sem/effects.nv:171` — outside the subset: a record constructor is compiled only as a binding initializer, a call
- `novac/src/sem/harvest.nv:300` — outside the subset: an array literal is compiled only as an initializer, a call argument, 
- `novac/src/sem/protocols.nv:127` — outside the subset: a record constructor is compiled only as a binding initializer, a call
- `novac/src/types/types.nv:171` — outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b)

Две диагностики — ещё 3: `novac/src/diag/diag.nv`, `novac/src/sem/binding_test.nv`, `novac/src/source/source.nv`.

## Группировка по первой причине — 95 отвергнутых файлов

| семейство | файлов | где |
|---|---|---|
| F4 оболочка interop: нет тела/экземпляра метода | 17 | `novac/src/check/assign_rules.nv:28`, `novac/src/check/check.nv:166`, `novac/src/check/scope.nv:85`, `novac/src/check/strings.nv:38`, `novac/src/emit_c/emit_requires.nv:34`, `novac/src/names/names.nv:95`, `novac/src/parse/decls.nv:34`, `novac/src/parse/expect.nv:48`, `novac/src/parse/handler.nv:51`, `novac/src/parse/type_decl.nv:35`, `novac/src/pipeline/handed_sum_test.nv:13`, `novac/src/sem/defs.nv:72`, `novac/src/sem/interop.nv:49`, `novac/src/sem/litpool.nv:48`, `novac/src/sem/node_questions.nv:32`, `novac/src/sem/sem.nv:134`, `novac/src/sem/typeref.nv:51` |
| F2 ЛОЖНОЕ «нет такого»: поле или константа из другого файла (см. ниже) | 15 | `novac/src/check/check_test.nv:22`, `novac/src/check/diagnostics.nv:48`, `novac/src/check/exprs.nv:64`, `novac/src/check/option_rules.nv:52`, `novac/src/check/return_rules.nv:33`, `novac/src/check/run.nv:65`, `novac/src/check/variant_rules.nv:50`, `novac/src/emit_c/emit_decls.nv:36`, `novac/src/emit_c/emit_expr.nv:69`, `novac/src/emit_c/emit_match.nv:39`, `novac/src/emit_c/emit_place.nv:45`, `novac/src/pipeline/prims_test.nv:22`, `novac/src/sem/binding_test.nv:28`, `novac/src/sem/callables.nv:137`, `novac/src/sem/pattern.nv:45` |
| F1 чужой модуль: тела не компилируются в единицу (E2-b2) | 14 | `novac/src/check/binds.nv:44`, `novac/src/check/casts.nv:40`, `novac/src/check/destructure.nv:73`, `novac/src/check/handler.nv:59`, `novac/src/check/match_arms.nv:53`, `novac/src/check/params.nv:44`, `novac/src/check/rules.nv:52`, `novac/src/check/with_typing.nv:28`, `novac/src/emit_c/emit_option.nv:41`, `novac/src/lower/lower_match.nv:70`, `novac/src/lower/lowering.nv:45`, `novac/src/pipeline/pipeline.nv:37`, `novac/src/pipeline/unit_test.nv:59`, `novac/src/sem/type_shape.nv:42` |
| F3 срез `x[lo..hi]` (E2-b) | 13 | `novac/src/check/literal_rules.nv:78`, `novac/src/check/typeref_rules.nv:278`, `novac/src/emit_c/emit_c.nv:226`, `novac/src/emit_c/emit_interp.nv:246`, `novac/src/emit_c/shell.nv:86`, `novac/src/lex/lex.nv:601`, `novac/src/main.nv:71`, `novac/src/parse/parse.nv:183`, `novac/src/sem/mangle.nv:522`, `novac/src/sem/slots_decl.nv:397`, `novac/src/source/source.nv:40`, `novac/src/source/source_test.nv:13`, `novac/src/types/types.nv:171` |
| F6 позиция конструктора/литерала массива | 12 | `novac/src/lower/ir.nv:497`, `novac/src/lower/lower_test.nv:13`, `novac/src/parse/type_ref.nv:36`, `novac/src/pipeline/with_test.nv:84`, `novac/src/resolve/resolve_test.nv:25`, `novac/src/sem/channel.nv:246`, `novac/src/sem/coerce.nv:263`, `novac/src/sem/collect.nv:160`, `novac/src/sem/effects.nv:171`, `novac/src/sem/harvest.nv:300`, `novac/src/sem/mangle_test.nv:24`, `novac/src/sem/protocols.nv:127` |
| F5 экранирование в строке (E2-b) | 7 | `novac/src/parse/parse_test.nv:78`, `novac/src/pipeline/decls_test.nv:23`, `novac/src/pipeline/interp_test.nv:25`, `novac/src/pipeline/overload_test.nv:23`, `novac/src/pipeline/pipeline_test.nv:183`, `novac/src/pipeline/refusal_test.nv:34`, `novac/src/pipeline/subset_test.nv:31` |
| F7 `if` значением (E2-b) | 4 | `novac/src/check/calls.nv:820`, `novac/src/check/tail_rules.nv:321`, `novac/src/check/type_of.nv:522`, `novac/src/lex/lex_test.nv:183` |
| прочее | 4 | `novac/src/check/typing.nv:71`, `novac/src/diag/diag.nv:52`, `novac/src/emit_c/emit_flow.nv:47`, `novac/src/tree/tree.nv:358` |
| F9 форма типа (срез/кортеж/fn-тип, E2) | 3 | `novac/src/builtins/builtins.nv:430`, `novac/src/mono/mono.nv:67`, `novac/src/sem/slots.nv:176` |
| F8 индексация (E2-b) | 3 | `novac/src/check/methods.nv:51`, `novac/src/check/operators.nv:36`, `novac/src/sem/binding.nv:239` |
| F10 вызов на ТИПЕ `Type.method(...)` | 2 | `novac/src/resolve/resolve.nv:86`, `novac/src/sem/file_shape.nv:47` |
| F11 ложная D84-неоднозначность (№1284) | 1 | `novac/src/parse/expr.nv:67` |

## По файлам — первая причина (самая ранняя диагностика в файле)

| файл:строка | код | диагностик в файле | первые 120 знаков сообщения |
|---|---|---|---|
| `novac/src/builtins/builtins.nv:430` | E_NOVAC_SUBSET | 1 | outside the subset: this type form is not compiled yet (slices, tuples, fn types and qualifiers arrive with generics, E2 |
| `novac/src/check/assign_rules.nv:28` | E_NOVAC_SUBSET | 39 | outside the subset: the shell novac links into carries no `kind_of` for Node -- novac read the signature but has no body |
| `novac/src/check/binds.nv:44` | E_NOVAC_SUBSET | 161 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/check/calls.nv:820` | E_NOVAC_SUBSET | 1 | outside the subset: an `if` in value position is not compiled yet |
| `novac/src/check/casts.nv:40` | E_NOVAC_SUBSET | 25 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/check/check.nv:166` | E_NOVAC_SUBSET | 141 | outside the subset: the shell novac links into carries no `report_if_error_token` for Checker -- novac read the signatur |
| `novac/src/check/check_test.nv:22` | E_NOVAC_SUBSET | 17 | this record construction names a field the type does not declare (#812) |
| `novac/src/check/destructure.nv:73` | E_NOVAC_SUBSET | 28 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/check/diagnostics.nv:48` | E_NOVAC_SUBSET | 17 | unknown field: this type has no field with this name |
| `novac/src/check/exprs.nv:64` | E_NOVAC_SUBSET | 273 | unknown field: this type has no field with this name |
| `novac/src/check/handler.nv:59` | E_NOVAC_SUBSET | 84 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/check/literal_rules.nv:78` | E_NOVAC_SUBSET | 14 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/check/match_arms.nv:53` | E_NOVAC_SUBSET | 159 | outside the subset: `is_ty` is declared in module `novac.types`, whose bodies novac does not compile into this unit yet  |
| `novac/src/check/methods.nv:51` | E_NOVAC_SUBSET | 151 | outside the subset: indexing is read but not compiled yet (E2-b) -- the interop shell carries no `index` for this instan |
| `novac/src/check/operators.nv:36` | E_NOVAC_SUBSET | 81 | outside the subset: indexing is read but not compiled yet (E2-b) -- the interop shell carries no `index` for this instan |
| `novac/src/check/option_rules.nv:52` | E_NOVAC_SUBSET | 75 | unknown field: this type has no field with this name |
| `novac/src/check/params.nv:44` | E_NOVAC_SUBSET | 31 | outside the subset: `param_type_at` is declared in module `novac.sem`, whose bodies novac does not compile into this uni |
| `novac/src/check/return_rules.nv:33` | E_NOVAC_SUBSET | 70 | unknown field: this type has no field with this name |
| `novac/src/check/rules.nv:52` | E_NOVAC_SUBSET | 154 | outside the subset: `param_type_at` is declared in module `novac.sem`, whose bodies novac does not compile into this uni |
| `novac/src/check/run.nv:65` | E_NOVAC_SUBSET | 40 | this record construction names a field the type does not declare (#812) |
| `novac/src/check/scope.nv:85` | E_NOVAC_SUBSET | 21 | outside the subset: this type has no such method in the declarations novac was handed |
| `novac/src/check/strings.nv:38` | E_NOVAC_SUBSET | 30 | outside the subset: the shell novac links into carries no `kind_of` for Node -- novac read the signature but has no body |
| `novac/src/check/tail_rules.nv:321` | E_NOVAC_SUBSET | 1 | outside the subset: an `if` in value position is not compiled yet |
| `novac/src/check/type_of.nv:522` | E_NOVAC_SUBSET | 1 | outside the subset: an `if` in value position is not compiled yet |
| `novac/src/check/typeref_rules.nv:278` | E_NOVAC_SUBSET | 1 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/check/typing.nv:71` | E_NOVAC_SUBSET | 208 | outside the subset: this operator on these operands dispatches through a protocol (D46: @plus / @compare / @equal), and  |
| `novac/src/check/variant_rules.nv:50` | E_NOVAC_SUBSET | 23 | unknown name: nothing with this name is bound at this point |
| `novac/src/check/with_typing.nv:28` | E_NOVAC_SUBSET | 35 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/diag/diag.nv:52` | E_NOVAC_SUBSET | 2 | outside the subset: an effect attribute is read but not compiled yet |
| `novac/src/emit_c/emit_c.nv:226` | E_NOVAC_SUBSET | 1 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/emit_c/emit_decls.nv:36` | E_NOVAC_SUBSET | 114 | unknown field: this type has no field with this name |
| `novac/src/emit_c/emit_expr.nv:69` | E_NOVAC_SUBSET | 261 | unknown field: this type has no field with this name |
| `novac/src/emit_c/emit_flow.nv:47` | E_NOVAC_SUBSET | 63 | outside the subset: a `match` on an applied sum (`Option[IfCtx]`) is read but not compiled yet -- its tags belong to the |
| `novac/src/emit_c/emit_interp.nv:246` | E_NOVAC_SUBSET | 1 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/emit_c/emit_match.nv:39` | E_NOVAC_SUBSET | 16 | unknown field: this type has no field with this name |
| `novac/src/emit_c/emit_option.nv:41` | E_NOVAC_SUBSET | 18 | outside the subset: `is_ty` is declared in module `novac.types`, whose bodies novac does not compile into this unit yet  |
| `novac/src/emit_c/emit_place.nv:45` | E_NOVAC_SUBSET | 51 | unknown field: this type has no field with this name |
| `novac/src/emit_c/emit_requires.nv:34` | E_NOVAC_SUBSET | 20 | outside the subset: the shell novac links into carries no `kind_of` for Node -- novac read the signature but has no body |
| `novac/src/emit_c/shell.nv:86` | E_NOVAC_SUBSET | 3 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/lex/lex.nv:601` | E_NOVAC_SUBSET | 55 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/lex/lex_test.nv:183` | E_NOVAC_SUBSET | 4 | outside the subset: an `if` in value position is not compiled yet |
| `novac/src/lower/ir.nv:497` | E_NOVAC_SUBSET | 8 | outside the subset: a record constructor is compiled only as a binding initializer, a call argument or the body of a mat |
| `novac/src/lower/lower_match.nv:70` | E_NOVAC_SUBSET | 64 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/lower/lower_test.nv:13` | E_NOVAC_SUBSET | 3 | outside the subset: a type in call position constructs a value, and novac compiles that only for a newtype (`type Row in |
| `novac/src/lower/lowering.nv:45` | E_NOVAC_SUBSET | 193 | outside the subset: `branch_children` is declared in module `novac.sem`, whose bodies novac does not compile into this u |
| `novac/src/main.nv:71` | E_NOVAC_SUBSET | 7 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/mono/mono.nv:67` | E_NOVAC_SUBSET | 6 | outside the subset: this type form is not compiled yet (slices, tuples, fn types and qualifiers arrive with generics, E2 |
| `novac/src/names/names.nv:95` | E_NOVAC_SUBSET | 3 | outside the subset: this type has no such method in the declarations novac was handed |
| `novac/src/parse/decls.nv:34` | E_NOVAC_SUBSET | 78 | outside the subset: the shell novac links into carries no `take` for Cursor -- novac read the signature but has no body  |
| `novac/src/parse/expect.nv:48` | E_NOVAC_SUBSET | 8 | outside the subset: the shell novac links into carries no `peek` for Cursor -- novac read the signature but has no body  |
| `novac/src/parse/expr.nv:67` | E_NOVAC_SUBSET | 354 | this call fits more than one method and none is more specific -- an ambiguity is refused, never chosen (D84) |
| `novac/src/parse/handler.nv:51` | E_NOVAC_SUBSET | 46 | outside the subset: the shell novac links into carries no `take` for Cursor -- novac read the signature but has no body  |
| `novac/src/parse/parse.nv:183` | E_NOVAC_SUBSET | 1 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/parse/parse_test.nv:78` | E_NOVAC_SUBSET | 4 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/parse/type_decl.nv:35` | E_NOVAC_SUBSET | 134 | outside the subset: the shell novac links into carries no `take` for Cursor -- novac read the signature but has no body  |
| `novac/src/parse/type_ref.nv:36` | E_NOVAC_SUBSET | 44 | outside the subset: a typed array literal is written `[]T.of(...)` -- this constructor is not the literal form |
| `novac/src/pipeline/decls_test.nv:23` | E_NOVAC_SUBSET | 6 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/handed_sum_test.nv:13` | E_NOVAC_SUBSET | 1 | outside the subset: the shell novac links into carries no `Vec` instance for str -- the interop surface is the probe pro |
| `novac/src/pipeline/interp_test.nv:25` | E_NOVAC_SUBSET | 5 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/overload_test.nv:23` | E_NOVAC_SUBSET | 5 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/pipeline.nv:37` | E_NOVAC_SUBSET | 66 | outside the subset: `lex` is declared in module `novac.lex`, whose bodies novac does not compile into this unit yet (E2- |
| `novac/src/pipeline/pipeline_test.nv:183` | E_NOVAC_SUBSET | 40 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/prims_test.nv:22` | E_NOVAC_SUBSET | 1 | unknown field: this type has no field with this name |
| `novac/src/pipeline/refusal_test.nv:34` | E_NOVAC_SUBSET | 7 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/subset_test.nv:31` | E_NOVAC_SUBSET | 69 | outside the subset: this string escape is not compiled yet (the subset knows only \" \\ \n \t \r) |
| `novac/src/pipeline/unit_test.nv:59` | E_NOVAC_SUBSET | 14 | outside the subset: `matches_at` is declared in module `novac.pipeline`, whose bodies novac does not compile into this u |
| `novac/src/pipeline/with_test.nv:84` | E_NOVAC_SUBSET | 8 | outside the subset: a typed array literal is written `[]T.of(...)` -- this constructor is not the literal form |
| `novac/src/resolve/resolve.nv:86` | E_NOVAC_SUBSET | 124 | outside the subset: a call on a TYPE (`Type.method(...)`) is not compiled yet -- no position accepts it (274.7, the E2 h |
| `novac/src/resolve/resolve_test.nv:25` | E_NOVAC_SUBSET | 1 | outside the subset: a typed array literal is written `[]T.of(...)` -- this constructor is not the literal form |
| `novac/src/sem/binding.nv:239` | E_NOVAC_SUBSET | 26 | outside the subset: indexing is read but not compiled yet (E2-b) -- the interop shell carries no `index` for this instan |
| `novac/src/sem/binding_test.nv:28` | E_NOVAC_SUBSET | 2 | this record construction names a field the type does not declare (#812) |
| `novac/src/sem/callables.nv:137` | E_NOVAC_SUBSET | 57 | unknown field: this type has no field with this name |
| `novac/src/sem/channel.nv:246` | E_NOVAC_SUBSET | 11 | outside the subset: a record constructor is compiled only as a binding initializer, a call argument or the body of a mat |
| `novac/src/sem/coerce.nv:263` | E_NOVAC_SUBSET | 3 | outside the subset: an array literal is compiled only as an initializer, a call argument, a constructor field or the bod |
| `novac/src/sem/collect.nv:160` | E_NOVAC_SUBSET | 5 | outside the subset: a record constructor is compiled only as a binding initializer, a call argument or the body of a mat |
| `novac/src/sem/defs.nv:72` | E_NOVAC_SUBSET | 15 | outside the subset: the shell novac links into carries no `len` for Vec[ConstDef] -- the interop surface is the probe pr |
| `novac/src/sem/effects.nv:171` | E_NOVAC_SUBSET | 1 | outside the subset: a record constructor is compiled only as a binding initializer, a call argument or the body of a mat |
| `novac/src/sem/file_shape.nv:47` | E_NOVAC_SUBSET | 7 | outside the subset: a call on a TYPE (`Type.method(...)`) is not compiled yet -- no position accepts it (274.7, the E2 h |
| `novac/src/sem/harvest.nv:300` | E_NOVAC_SUBSET | 1 | outside the subset: an array literal is compiled only as an initializer, a call argument, a constructor field or the bod |
| `novac/src/sem/interop.nv:49` | E_NOVAC_SUBSET | 8 | outside the subset: the shell novac links into carries no `find` for NameTable -- novac read the signature but has no bo |
| `novac/src/sem/litpool.nv:48` | E_NOVAC_SUBSET | 5 | outside the subset: the shell novac links into carries no `find` for NameTable -- novac read the signature but has no bo |
| `novac/src/sem/mangle.nv:522` | E_NOVAC_SUBSET | 6 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/sem/mangle_test.nv:24` | E_NOVAC_SUBSET | 11 | outside the subset: a typed array literal is written `[]T.of(...)` -- this constructor is not the literal form |
| `novac/src/sem/node_questions.nv:32` | E_NOVAC_SUBSET | 27 | outside the subset: the shell novac links into carries no `kind_of` for Node -- novac read the signature but has no body |
| `novac/src/sem/pattern.nv:45` | E_NOVAC_SUBSET | 20 | unknown field: this type has no field with this name |
| `novac/src/sem/protocols.nv:127` | E_NOVAC_SUBSET | 1 | outside the subset: a record constructor is compiled only as a binding initializer, a call argument or the body of a mat |
| `novac/src/sem/sem.nv:134` | E_NOVAC_SUBSET | 58 | outside the subset: the shell novac links into carries no `id_of` for Node -- novac read the signature but has no body t |
| `novac/src/sem/slots.nv:176` | E_NOVAC_SUBSET | 4 | outside the subset: this type form is not compiled yet (slices, tuples, fn types and qualifiers arrive with generics, E2 |
| `novac/src/sem/slots_decl.nv:397` | E_NOVAC_SUBSET | 3 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/sem/type_shape.nv:42` | E_NOVAC_SUBSET | 45 | outside the subset: `is_ty` is declared in module `novac.types`, whose bodies novac does not compile into this unit yet  |
| `novac/src/sem/typeref.nv:51` | E_NOVAC_SUBSET | 108 | outside the subset: the shell novac links into carries no `len` for Vec[Node] -- the interop surface is the probe progra |
| `novac/src/source/source.nv:40` | E_NOVAC_SUBSET | 2 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/source/source_test.nv:13` | E_NOVAC_SUBSET | 4 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
| `novac/src/tree/tree.nv:358` | E_NOVAC_SUBSET | 3 | outside the subset: a record-form variant is not compiled yet |
| `novac/src/types/types.nv:171` | E_NOVAC_SUBSET | 1 | outside the subset: a slice `x[lo..hi]` is read but not compiled yet (E2-b) |
