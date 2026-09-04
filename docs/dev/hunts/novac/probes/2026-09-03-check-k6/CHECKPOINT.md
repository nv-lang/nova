# Охота: check x К6 (энфорс на перечислении синтаксических форм) — ЗАВЕРШЕНО

## СВОЙСТВО класса (не конструкция)

Правила модуля `check`, которые про СМЫСЛ — «имя в аргументе обязано
существовать», «слот интерполяции обязан быть печатаемого типа», «два эффектных
аргумента = отказ (F65)», «литерал массива непуст», «тело с объявленным типом
кончается значением», «столько ли аргументов, сколько параметров» — все энфорсятся
через ОДИН список видов узлов `sem/slots.nv:622 is_expr_kind` (16 имён).
Форма языка, законная и не попавшая в список, проходит правило МИМО.

**Синтаксисы носителя — ДВА, не один:**
1. `Index` — `xs[0]`;
2. `NamedArg` — `f(x, by: <expr>)`. `is_arg_node` (slots.nv:127) добавил NamedArg
   только к СЧЁТУ; walk-сайты (exprs.nv:457/516, typing.nv:902) спрашивают голый
   `is_expr_kind`, и ВНУТРЕННЕЕ выражение именованного аргумента не обходится
   вовсе.

Искал третью форму по образцу «вид узла, законный как выражение, но не в
`is_expr_kind`»: `TupleExpr` (закрыт №870 в позиции println; в свободном вызове
недостижим — параметр-кортеж отказан TUPLE_PARAM_MSG), `IfExpr` (отказан
именем), `Unsafe`/`ArraySpread` — отказаны именем. Не нашёл.

## Сайты перечисления в check/*

- `exprs.nv:201` — слоты интерполяции: `if !is_expr_kind(...) { continue }`
- `exprs.nv:457` — аргументы method-call: `if i > 2 && is_expr_kind(...)`
- `exprs.nv:516` — аргументы free-call: `if !is_expr_kind(...) { continue }`
- `typing.nv:902` — аргументы println (ЕСТЬ страховка на Branch, фикс №870)
- `rules.nv:861` — `@report_if_ordered_args` (F65)
- `tail_rules.nv:150` — allow-list хвоста
- `binds.nv:385` -> `sem/slots.nv:96 literal_elems` — элементы литерала массива
- `calls.nv:167` / `methods.nv:127` — сообщения ложных отказов
- `sem/slots.nv:127 is_arg_node`, `sem/slots.nv:622 is_expr_kind` — корень

## Пробы (19 каталогов)

| проба | novac check | оракул | вердикт |
|---|---|---|---|
| p_namedarg_unknown_name | ok (0), emit даёт C | ОТКАЗ `undefined identifier` | НАХОДКА: тихий пропуск неизвестного имени, невалидный C |
| p_interp_index_mixed | ok (0) | `first=7 last=9 done` | НАХОДКА: novac печатает `first= last= done` |
| p_index_interp | ok (0) | `9` | НАХОДКА: novac печатает пустую строку |
| p_namedarg_call | ok (0) | `30` | НАХОДКА: emit ICE `callee_of ... hole in the channel` |
| p_index_namedarg | ok (0) | — | НАХОДКА: emit ICE `expression kind outside the subset` |
| p_order_named | ok (0) | — | НАХОДКА: правило F65 не применилось; emit ICE |
| p_index_arg | ОТКАЗ «this call omits `n`» | `8` | НАХОДКА: ложный отказ, сообщение врёт |
| p_index_methodarg | ОТКАЗ «this type has no such method» | `14` | НАХОДКА: ложный отказ, сообщение врёт |
| p_index_arraylit | ОТКАЗ «an empty array literal» | `5` | НАХОДКА: ложный отказ, сообщение врёт |
| p_index_tail | ОТКАЗ «body ends without a value» | `7` | НАХОДКА: ложный отказ |
| p_order_positional | ОТКАЗ F65 | — | КОНТРОЛЬ: правило работает на позиционных |
| p_namedarg_name | ok (0), emit ok | — | КОНТРОЛЬ: named-arg сам по себе работает |
| p_index_ctorfield | честный subset-отказ (indexing) | `5` | КОНТРОЛЬ: отказ на Index СУЩЕСТВУЕТ -> в интерполяции он пропущен |
| p_namedarg_badtype | ОТКАЗ по типу | — | КОНТРОЛЬ: тип named-arg сверяется (через ключ), имя — нет |
| p_match_interp | честный subset-отказ | — | безрезультатная |
| p_interp_ifexpr | честный subset-отказ | — | безрезультатная |
| p_for_arraylit_head | честный subset-отказ | — | безрезультатная |
| p_for_paren_head | честный subset-отказ | — | безрезультатная |
| p_namedarg_arraylit | два отказа, проба грязная | — | безрезультатная |
