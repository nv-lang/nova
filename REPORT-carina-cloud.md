# Отчёт облачной сессии — ветка `p274-carina-cloud` (2026-10-01)

СЛИЯНИЕ ice-expr: один конфликт — `docs/dev/novac-time-ledger.md`. Обе стороны дописали строку в одно место: облачная сессия — строку «вариант-запись», помощник — строку `never`. Сохранены обе, в этом порядке. Остальное слилось чисто (`b6115dc4a`) /

СТРАЖИ: 98 запущено, 7 тяжёлых пропущено раннером. Итог на вершине: красный только известный `check-novac-commit-no-simplification` («.» вместо файла сообщения, Errno 21). `check-novac-local-only-work` зелёный после пуша ветки. По ходу починено пять красных:
- `check-novac-no-silent-skip` — `tail_rules.nv:218`, выход ветки `never` из ice-expr. Страж не считает `@record` решением, поэтому оба выхода получили `// SILENT-OK:` с причиной;
- `check-novac-surface` — sem 321 -> 322, новый экспорт `is_never_ty` из ice-expr. База поднята с причиной;
- `check-novac-tuple-no-second-door` — 96 > 95, красный уже на `b8d501817`. Причина — текст отказа №1514 («a tuple» в `COMPOSITE_REPR_MSG`). Текст теперь называет форму `(A, B)`; тест проверяет «over a composite type», и это осталось;
- `check-novac-time-ledger` — среда: неглубокий клон. После `git fetch --unshallow` зелёный, код не правился;
- `check-novac-table-one-filler` — 38 > 36, появился при №1518: две новые записи `harvest_consts` (`consts.add`, `defs.add`), обе в sem. База поднята с причиной, отдельный коммит /

№1518: скрипт применён, все якоря нашлись, но патч в исходном виде НЕ РАБОТАЛ — тест красный, «unknown name» на LIMIT и WORD. Причина: парсер оборачивает каждый литерал в ветвь `Lit` (`parse/expr.nv`), и `is_literal_leaf` не срабатывал ни разу. Исправлено по замыслу патча, а не в обход:
- `handed_literal` берёт `Lit` с литералом (число, в том числе отрицательное; строка; символ) и ПЕРЕСОБИРАЕТ его с `no_node()`. У листьев id нет, поэтому чужой id в канал файла не попадает;
- `emit_c/emit_expr.nv`: строковый литерал без идентичности не читает `@lit_tmp` — иначе выход за границу массива с индексом -1;
- проба в обе стороны: да (без вызова `harvest_consts` тест красный, с вызовом зелёный) /

№1519: скрипт применён, все якоря нашлись. Одна правка руками: патч передавал `generic_linear(fk)` седьмым позиционным аргументом в `fns.add` (`collect.nv`) — это слот `handed_from str`. Строка теперь помечается отдельным вызовом `fns.mark_linear(generic_linear(fk))`, той дверью, которую патч сам завёл. Проба в обе стороны: да (`linear_dominates` всегда `false` — тест красный) /

МЕРА 0.2 (Linux, команда из задания; при `GC_DONT_GC=1` то же самое):
- на дереве `b8d501817` — 181, ICE 0;
- после ice — 160, ICE 0;
- после №1518 — 149;
- после патчей — 145, ICE 0.

Итого −36:
- группа «`ice` is not a callable» 21 -> 0 (на её месте осталось 6 других «not a callable»);
- «unknown name» 13 -> 4 (оставшиеся 4 — локальное `name` в `resolve.nv`);
- «more than one method» 5 -> 1 (`out.to_str()` в `pipeline.nv`);
- две одиночные ушли попутно.

double-build: 120/142 на `b8d501817` -> 121 после ice -> **122/142** после патчей /

ГРУППЫ (топ-10 после):
```
27 the operands are different numeric types -- no implicit widening (D405)
24 outside the subset: a cast `X` is not compiled yet (E2-b, numeric family)
24 outside the subset: a `X` arm on a string or char literal is read but not compiled yet (E2-b, string family)
12 outside the subset: a `X` on an applied sum (`X`) is read but not compiled yet (E2-b2)
 6 outside the subset: this type has no such method in the declarations novac was handed
 6 outside the subset: `X` is not a callable novac knows in an expression (E2-b3)
 6 a bare `X` here has no type to take (E2-b3)
 4 unknown name: nothing with this name is bound at this point
 4 the pattern'X's declaration -- match every payload field (D59)
 3 parameter `X` needs a mutable place (P14)
```
/

МОДУЛЬНЫЕ ТЕСТЫ: `novac/src/pipeline` — прошло (23 файла в одном CU, 1/1), `novac/src/sem` — прошло, `novac/src/resolve` — прошло, `novac/src/check` — прошло /

КОММИТЫ:
- `b6115dc4a` — слияние ice-expr;
- `a827761f0` — стражи после слияния;
- `2849c200a` — №1518 + строка реестра + 1517 в `gaps=`;
- `a66b24138` — №1519 + строка реестра;
- `c0bc64335` — база table-fillers для №1518;
- этот отчёт /

ОТКРЫТО:
- переданная константа с НЕлитеральным инициализатором не регистрируется (заявлено в Simplifications №1518);
- правила 2 и 6 амендмента D84 (must-consume `T`; обобщённый вызывающий с `[T consume]`) этим исправлением не затронуты и не проверены;
- строка в `novac-time-ledger.md` за эту сессию НЕ добавлена: страж показывает сумму долей за 2026-10-01 ровно 1.00 при потолке 1.00 /

ВОПРОСЫ ИНТЕГРАТОРУ:
1. **Мера 0.2 на `b8d501817`: 109 у Карины против 181 здесь, на Linux.** Команда та же, ICE 0, double-build совпадает (120/142). Лишнее сидит в трёх группах: 27 «different numeric types», 24 «cast», 24 «string/char arm»; 67 мест — в `lex/lex.nv` (`b == (' ' as u8)`, `b == B_TAB`). Варианты:
   - (а) Карина считала другой командой или на другом дереве;
   - (б) novac ведёт себя по-разному на Windows и Linux.

   Рекомендация: окну Карины прогнать ту же команду на `b8d501817` у себя и сравнить списки групп. Если различие подтвердится, это платформенно-зависимый ответ компилятора: строка реестра К1 и вопрос до ступени 0.2, потому что мера должна быть одной на всех машинах.
2. **Две правки сверх присланных патчей — на ревью окну Карины:** `handed_literal` + проверка в `emit_expr` (№1518) и `mark_linear` в `collect.nv` (№1519). Рекомендация: принять. Обе держат собственный инвариант патча (никакого чужого id в канале; линейность через дверь `@mark_linear`) и подтверждены пробой.
3. **Текст отказа №1514 изменён ради стража `tuple`** (`a tuple` -> `` `(A, B)` ``). Если владелец предпочитает слово, нужна не правка текста, а поднятая база `tuple_words` с его согласия. Рекомендация: оставить как есть — форма названа точнее слова.
