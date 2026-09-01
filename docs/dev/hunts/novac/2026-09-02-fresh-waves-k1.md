<!-- SPDX-License-Identifier: CC-BY-4.0 -->
КЛЕТКА | emit_c | К1

# Охота: emit_c × К1 — свежая поверхность волн 3-пред38..41

**Трек:** novac · **Клетка:** emit_c × К1 · **Дата:** 2026-09-02
**Цель окна:** контракт-пролог `requires`, hex-литералы, вариант-как-значение и
примыкающие свежие формы чекера/лексера.
**Пробы:** `target/hunt-fresh/` (64 каталога `.nv` + один каталог C-пробы, каждая
проба — свой ПУСТОЙ каталог, файл `p.nv`, первая строка `module p`). Приняв
отчёт, окно переносит цитируемые в
`docs/dev/hunts/novac/probes/2026-09-02-fresh-waves-k1/`.

Ниже — только наблюдения с числами. Ни одна строка не закрыта, ни одна не
заведена в реестр: это делает окно.

## Чем мерилось

Признак К1 по брифу — **оба компилятора приняли, а напечатанный ответ разный**;
сюда же брифом отнесён случай «novac эмитит C, который clang не собирает» и
случай «ответы совпали, но эмиссия несёт скрытое искажение».

```sh
sh scripts/tools/novac-e1-smoke.sh target/hunt-fresh/<проба>/p.nv
```

Стороны по отдельности:

```sh
./nova-cli/target/release/nova.exe build target/hunt-fresh/<проба>/p.nv -o target/hunt-fresh/<проба>/o.exe && ./target/hunt-fresh/<проба>/o.exe; echo "(exit $?)"
export NOVA_STD_PATH=$(pwd)/std/src
./novac/target/novac.exe check target/hunt-fresh/<проба>/p.nv; echo "rc=$?"
./novac/target/novac.exe emit  target/hunt-fresh/<проба>/p.nv > target/hunt-fresh/<проба>/n.c
```

Два раннера-обёртки лежат рядом с пробами и никакой логики не содержат —
`target/hunt-fresh/run.sh <проба>` (обе стороны + смоук одной строкой) и
`target/hunt-fresh/link.sh <проба>` (линкует `n.c` argv'ем оракула из кэша
смоука и запускает: нужно там, где смоук останавливается на первом расхождении
stdout и **до сверки кодов возврата не доходит**). Коды возврата в таблицах
ниже добыты именно им.

Сырые результаты всех прогонов — `target/hunt-fresh/results.txt` (чекпоинт
писался после каждой пробы).

## Шесть классов расхождения ОТВЕТА

Каждый класс назван СВОЙСТВОМ, и под ним перечислены синтаксисы, в которых это
свойство встречается.

### Н1. Ведущий ноль: текст целого литерала переносится в C дословно, а C читает его по СВОИМ правилам

**Свойство:** решение «какое это число» принимается ДВАЖДЫ — чекер разбирает
текст литерала по правилам Nova, эмиттер отдаёт тот же текст компилятору C.
`0123` в Nova — сто двадцать три, в C — восьмеричное восемьдесят три.

| проба | оракул | novac |
|---|---|---|
| `lit_leading_zero_dec` (`println(0123)`) | `123`, exit 0 | **`83`**, exit 0 |
| `lit_leading_zero_const` (`const K = 0123`) | `123`, exit 0 | **`83`**, exit 0 |
| `lit_leading_zero_neg` (`-0123` и `0100 + 1`) | `-123` / `101`, exit 0 | **`-83`** / **`65`**, exit 0 |
| `lit_leading_zero_pattern` (`match n { 010 => 1 }`) | `1` / `0`, exit 0 | **`0`** / **`1`**, exit 0 |
| `lit_octal_invalid` (`println(08)`) | `8` / `9`, exit 0 | emit rc=0, clang: `invalid digit '8' in octal constant` |

Эмиссия `lit_leading_zero_neg` — оба числа завёрнуты, и арифметика идёт уже по
завёрнутому:

```c
static nova_unit nova_fn_main_impl(void) {
    nova_print_int(((nova_int)-0123LL));
    nova_print_newline();
    nova_int x = ((nova_int)0100LL);
    nova_print_int(nova_int_checked_add(x, ((nova_int)1LL)));
```

`lit_leading_zero_pattern` — та же подстановка ВТОРОЙ дверью, и она выбирает
другую ветвь `match`:

```c
static nova_int novac_fn_p_tag__nova_int__to_nova_int(nova_int n) {
    nova_int _novac_scr_t1 = n;
    ...
    if (!_novac_matched_t1 && ((_novac_scr_t1 == 010LL))) {
```

**Два адреса, отвечающие на один вопрос «какое число написано этим токеном»:**

* `novac/src/check/literal_rules.nv:57` — `fn int_text_fits(t str) -> bool`, и её
  собственная докстрока (`literal_rules.nv:53`) отвечает НА ЭТОТ ЖЕ ВОПРОС явно:
  «Leading zeros are stripped first: `0000009` is nine, not a nine-digit
  number». То есть чекер знает, что `0123` — десятичное сто двадцать три, и
  принимает литерал именно поэтому;
* `novac/src/emit_c/emit_expr.nv:241` — `@body.append(leaf_text(t))` внутри ветви
  `IntLit`: тот же текст уезжает в C без разбора. Третий адрес того же свойства —
  `novac/src/emit_c/emit_match.nv:126`, `@body.append(leaf_text(p0))` в литеральном
  паттерне (там даже без обёртки `((nova_int)...)`).

**Класс, названный конструкцией, пережил свой фикс.** Шапка
`novac/src/check/literal_rules.nv:24-30` говорит о находке охотника #810:

```
/// The MAGNITUDE of an integer literal -- the question nobody asked until
/// registry #810 (hunter, 2026-08-30). The lexer makes an IntLit of any run of
/// digits, the lattice types it `int` without looking, and the emitter prints
/// the text verbatim with an `LL` suffix: `18446744073709551616` therefore
/// overflowed a signed long long in the emitted C. That is a SILENT
/// MISCOMPILE, not a missing refusal
```

Диагноз в этой фразе назван верно — «the emitter prints the text verbatim» —
а починена была ВЕЛИЧИНА (одна конструкция: слишком большой литерал). Свойство
«текст литерала читается C по правилам C» осталось, и у него нашлись ещё пять
носителей выше.

**Вторую форму префикса искал и не нашёл:** `0X1F` и `0X8000000000000000`
(`lit_upper_hex_prefix`, `lit_upper_hex_overflow`) лексер novac отвергает по
имени — «syntax error: this is not a form of the language», — хотя оракул их
принимает (`31`, `-9223372036854775808`). Подчёркиваю, потому что путь к
обходу `int_text_fits` через ветвь `t[..2] == "0x"` был бы именно там.

**Контроль:** `ctl_dec_plain` — те же пять форм БЕЗ ведущего нуля
(`123`, `const K = 123`, `-123`, арм `10 =>`) — смоук зелёный, байт-в-байт.

### Н2. Аргумент `println`, чей вид узла не назван в `is_expr_kind`, МОЛЧА выпадает

**Свойство:** и чекер, и эмиттер перебирают детей `println` одной и той же
трёхветочной формой «строковый литерал → выражение → *иначе это пунктуация*», и
обе двери спрашивают об «иначе» ОДИН И ТОТ ЖЕ предикат. Вид узла, отсутствующий
в списке, не отвергается и не печатается — он просто не существует.

| проба | оракул | novac |
|---|---|---|
| `hex_index` (`println(v[0x1])`) | `20`, exit 0 | **пусто**, exit 0 |
| `ctl_dec_index` (`println(v[1])`) | `20`, exit 0 | **пусто**, exit 0 |
| `index_two_args` (`println("a", v[1], "b")`) | `a20b`, exit 0 | **`ab`**, exit 0 |
| `index_in_loop` (три итерации) | `10`/`20`/`30`, exit 0 | **три пустые строки**, exit 0 |
| `arg_tuple_lit` (`println("a", (1,2), "b")`) | свой codegen оракула не собрался | check rc=0, emit rc=0, кортеж выпал из эмиссии |

Эмиссия `index_two_args` — вектор построен, индекс исчез:

```c
static nova_unit nova_fn_main_impl(void) {
    Nova_Vec____nova_int* _novac_tmp_t1 = Nova_Vec____nova_int_static_new(3);
    (void)Vec____nova_int_method_push(_novac_tmp_t1, ((nova_int)10LL));
    (void)Vec____nova_int_method_push(_novac_tmp_t1, ((nova_int)20LL));
    (void)Vec____nova_int_method_push(_novac_tmp_t1, ((nova_int)30LL));
    Nova_Vec____nova_int* v = _novac_tmp_t1;
    nova_print_str(_novac_strlit_0);
    nova_print_str(_novac_strlit_1);
    nova_print_newline();
```

**Три адреса, и они складываются в один зазор:**

* `novac/src/sem/slots.nv:584-614` — `is_expr_kind`: `NodeKind.Index`,
  `TupleExpr`, `IfExpr` и `Unsafe` в списке ОТСУТСТВУЮТ. Комментарий внутри
  самого списка (`slots.nv:607-613`) называет этот класс дословно, по прошлому
  его носителю: «Counting goes through `is_arg_node`, `is_arg_node` asks here,
  and **a form missing from this list is not refused anywhere. It is simply not
  there.**»;
* `novac/src/check/typing.nv:824` — `} else if is_expr_kind(ck) {`, и хвостовая
  ветвь `typing.nv:849-852`: «Neither a literal nor an expression: the callee
  leaf and the punctuation children carry no type the emitter asks for»;
* `novac/src/emit_c/emit_expr.nv:593` — та же форма у эмиттера, и та же хвостовая
  ветвь `emit_expr.nv:620-623`: «The callee leaf and the parenthesis leaves are
  not arguments».

**Правило существует и знает только канонический путь.** Отказ на индекс написан
и назван — `novac/src/check/binds.nv:317`:

```
"outside the subset: indexing is read but not compiled yet (E2-b) -- the interop
 shell carries no `index` for this instance, so widen novac/probe/shell_probe.nv"
```

и его собственный комментарий (`binds.nv:312-316`) объясняет, ЗАЧЕМ:

```
    // Accepting the form anyway is the shape the differential guard calls the
    // worst kind of green: `check` clean, `emit` an ICE ("binding initializer is
    // neither a match, a ctor nor an expression"). So the subset refuses
```

Замерено: этот отказ срабатывает в позиции привязки (`index_bind`), в условии
`requires` (`index_in_requires`) и под бинарным оператором (`index_in_bin`) — и
не срабатывает в аргументе `println`. Там получается не «check clean, emit an
ICE», а хуже: check clean, emit clean, программа работает и печатает не то.

**Инвариант, объявленный НЕВЫРАЗИМЫМ, выразим.** `novac/src/check/typing.nv:837-843`:

```
            // The printer set is int/str/bool/f64. Every other primitive is
            // NAMEABLE (the universe holds all fifteen) but not yet
            // printable, and that boundary belongs here -- the emitter's ice
            // for a missing printer must never be the first answer. [INV-PROPERTY]
            // [INV-PROPERTY] -- the refusal returns two lines below, so the
            // emitter is not reached at all for this form; violating it would
            // require deleting the check, not merely getting it wrong.
```

Страж, который держит эту пометку, — `scripts/guards/check-invariant-discipline.sh`
(шапка, строка 32: «`[INV-PROPERTY]` — уже не инвариант: нарушение НЕВЫРАЗИМО»).
Он судит ФОРМУ: наличие подстроки `INV-PROPERTY|INV-GUARD:|INV-TODO:` в той же
добавляемой строке (`check-invariant-discipline.sh:97`). Путь, удовлетворяющий
форме и нарушающий требование, — ровно `println("a", v[1], "b")`: проверку никто
не удалял, её просто не спросили (ветвь `is_expr_kind(ck)` не взята), эмиттер
достигнут, и ответ у него не ICE, а тишина.

Второй страж этого места, `scripts/guards/check-novac-no-silent-skip.py`,
на дереве **зелёный**:

```
check-novac-no-silent-skip ok: функций прохода канала 18, выходов 86 —
у каждого решение (запись, отказ, ice или названная причина)
```

Его форма (шапка + `RE_RETURN` в теле) — «у каждого `return` в проходе канала
есть решение в окне шести строк». Ветвь `typing.nv:849-852` НЕ содержит
`return`: она дописывается до конца тела цикла и уходит на следующего ребёнка.
Мишень стража — молчаливый ВЫХОД, а здесь молчаливое ПРОДОЛЖЕНИЕ.

**Границы класса (пробы, где вид узла НАЗВАН отказом):** `arg_ifexpr` — «an `if`
in value position is not compiled yet»; `arg_unsafe` — «an `unsafe` block is read
but not compiled yet»; `arg_tuple` (тот же кортеж, но через ИМЯ — то есть узел
`Name`, который в списке есть) — «println covers int, str, bool and f64 today».
То есть три из четырёх отсутствующих видов имеют отказ на другом пути; тихо
выпадают `Index` и `TupleExpr` в литеральной форме.

### Н3. Display условия — склейка ЛИСТЬЕВ пробелами, а не срез исхода

**Свойство:** текст, который панику показывает человеку, собирается из токенов
через один пробел, поэтому любая исходная форма без пробелов (или с ними)
приезжает изменённой.

| проба | оракул | novac |
|---|---|---|
| `req_display_neg` | `... (n > -1)` | `... (n > - 1)` |
| `req_display_call` | `... (pos(n))` | `... (pos ( n ))` |
| `req_display_field` | `... (p.n > 0)` | `... (p . n > 0)` |
| `req_method_selffield` | `... (@n > 0)` | `... (@ n > 0)` |

Коды возврата совпадают — 101 у обеих сторон (проверено `link.sh` на
`req_method_selffield` и `req_violate_plain`); расходится ровно текст.

Норма, относительно которой это меряется, записана не мной:
`spec_tests/conformance/contract_exprdisplay_selfaccess_neg.nv:10-14`

```
// `Counter5` is NON-generic, so this hits the contract-display path independently
// of the generic-mono fix. No custom message → the raw contract source is the
// surfaced text (format A: "<file>:<line>: requires failed: <src>"). At count=0
// ...
// Pre-fix this message read "requires failed: assert > 0 && assert <= assert".
```

`<src>` — ИСХОДНЫЙ текст. Фикстура существует именно потому, что оракул однажды
показывал вместо него сборку из узлов; `req_method_selffield` — тот же вид
поломки на `@field`, мягче, но того же рода.

Источник — `novac/src/sem/slots.nv:840-847`, `display_of`: `for c in leaves_of(e) { if !first { s.append(" ") } ... }`.
Его собственная докстрока (`slots.nv:834-839`) границу называет: «Not the source
slice: the emitter holds no source». То есть это НЕ скрытый дефект, а названный
компромисс — но названо в нём только отсутствие среза, а не то, что от нормы
`<src>` расходятся четыре разные конструкции.

**Контроль:** `ctl_requires_plain`, `req_multi_clause`, `req_expr_body` — там,
где условие уже написано «через пробел» (`n > 0`, `a > 0 && b > 0`), совпадает
до байта; `req_andor_violate` показывает, что `&&` сам по себе тут ни при чём.

### Н4. Тот же display вставляется в C-литерал БЕЗ экранирования

**Свойство:** текст токена уезжает внутрь `"..."` C-строки как есть, поэтому
строковый литерал в условии закрывает эту строку своими кавычками.

| проба | оракул | novac |
|---|---|---|
| `req_display_quotes` (`requires s != "no", "msg"`) | `1`, exit 0 | check rc=0, emit rc=0, **clang: `expected ')'`** |
| `req_violate_str` (тот же файл, нарушающий вход) | `panic: p.nv:4: requires failed: msg (s != "no")`, exit 101 | тот же отказ clang |
| `req_display_escape` (`s != "a\nb"`) | `1`, exit 0 | тот же отказ clang |
| `req_cond_dollar` (`s != "a\$b"`) | `1`, exit 0 | тот же отказ clang |

Строка эмиссии (`req_display_quotes`) — сама себя закрывает на третьей кавычке:

```c
    if (!(((nova_int)(!Nova_str_method_equal(s, _novac_strlit_0))))) nova_contract_violation(NOVA_CONTRACT_PRE, "f", "s != "no"", "", 0, "msg");
```

**Два адреса, отвечающие на один вопрос «где литерал Nova становится текстом C»:**

* `novac/src/emit_c/shell.nv:167-168`, докстрока `emit_strlit_defs`:
  «**The only place a Nova literal becomes C text** -- which is why the escaped
  dollar is unescaped here and nowhere else», и сама работа —
  `shell.nv:180`, `out.append(unescape_dollar(raw))`;
* `novac/src/emit_c/emit_requires.nv:39` (`@body.append(display_of(rk[1]))`) и
  `emit_requires.nv:41` (`@body.append(contract_msg_c(rk))`, а внутри —
  `emit_requires.nv:57`, `return t.text`) — ВТОРОЕ место, где литерал Nova
  становится текстом C, мимо пула и мимо `unescape_dollar`.

Одна и та же программа показывает обе двери рядом (`req_msg_dollar`, эмиссия):

```c
static const uint8_t _novac_strlit_0_buf[] = "cost $5";              /* пул: backslash снят */
    ... nova_contract_violation(NOVA_CONTRACT_PRE, "f", "x > 0", "", 0, "cost \$5");   /* пролог: не снят */
```

Это и есть «ответы совпали, эмиссия несёт скрытое искажение»: clang принимает
`\$`, но с предупреждением, и печатает то же самое. Замерено отдельной C-пробой
`target/hunt-fresh/_cprobe/d.c`:

```
target/hunt-fresh/_cprobe/d.c:2:39: warning: unknown escape sequence '\$' [-Wunknown-escape-sequence]
cost $5
```

Докстрока `display_of` (`slots.nv:837-839`) переводит ответственность на
вызывающего дословно: «C-escaping is the caller's concern only insofar as Nova
string literals keep their own quotes -- the join never introduces one».
Вызывающий один, `emit_requires.nv:39`, и он не экранирует.

**Контроль/граница:** `req_msg_quotes` (`requires x > 0, "say \"hi\""`) — кавычки
внутри СООБЩЕНИЯ проходят верно (`contract_msg_c` отдаёт текст токена, а он уже
корректный C-литерал), и текст паники совпадает; `ctl_strlit_dollar`
(`println("cost \$5")`) — через пул, смоук зелёный.

### Н5. Сообщение контракта вставляется как ИСХОДНЫЙ текст: интерполяция не вычисляется

**Свойство:** `contract_msg_c` берёт текст токена и объявляет его готовым
C-литералом; всё, что Nova делает со строкой ПОСЛЕ лексера, теряется.

| проба | оракул | novac |
|---|---|---|
| `req_msg_interp` (`"got ${x}"`) | `requires failed: got -5 (x > 0)`, exit 101 | `requires failed: got ${x} (x > 0)`, exit 101 |
| `req_msg_interp_str` (`"bad ${s}"`) | `... bad tag (n > 0)` | `... bad ${s} (n > 0)` |
| `req_msg_interp_two` (`"a=${a} b=${b}"`) | `... a=-1 b=7 (a > 0)` | `... a=${a} b=${b} (a > 0)` |
| `req_msg_dollar` (`"cost \$5"`) | `... cost $5 (x > 0)` | `... cost $5 (x > 0)`, но в C `"cost \$5"` (см. Н4) |

Эмиссия `req_msg_interp` (адрес: `novac/src/emit_c/emit_requires.nv:57`):

```c
    if (!(((nova_int)((x) > (((nova_int)0LL)))))) nova_contract_violation(NOVA_CONTRACT_PRE, "f", "x > 0", "", 0, "got ${x}");
```

Норма, относительно которой это меряется:
`spec_tests/conformance/contract_msg_interp_neg.nv:3-5` —

```
// in normal strings), reusing the InterpolatedStr machinery. `requires x > 0,
// ... CAPTURED value: "<file>:<line>: requires failed: got -5 (x > 0)".
```

Докстрока `contract_msg_c` (`emit_requires.nv:45-50`) говорит про слот сообщения
только одно: «With a comma the string went through the required door, so a
non-string slot arrives only with its diagnostic and never reaches emission» —
то есть проверено, что слот СТРОКА, и не проверено, какая именно строка.
Интерполированная строка приходит той же формой токена `StrLit` (замер:
`t.kind == TokenKind.StrLit` сработал, `NULL` не вернулся).

### Н6. Место нарушения не печатается: `:0:` вместо `<file>:<line>`

**Свойство:** эмиттер не держит ни имени файла, ни номеров строк, поэтому
позиция в панике — константы `""` и `0`.

| проба | оракул | novac |
|---|---|---|
| `req_violate_plain` | `panic: p.nv:4: requires failed: half wants a positive (n > 0)`, exit 101 | `panic: :0: requires failed: half wants a positive (n > 0)`, exit 101 |
| `req_msgless_violate` | `panic: p.nv:4: requires failed: n > 0` | `panic: :0: requires failed: n > 0` |
| `req_andor_violate` | `panic: p.nv:4: ... (a > 0 && b > 0)` | `panic: :0: ... (a > 0 && b > 0)` |
| `req_msg_quotes` | `panic: p.nv:4: ... say "hi" (x > 0)` | `panic: :0: ... say "hi" (x > 0)` |

**Класс НАЗВАН волной, и это надо сказать первым:** шапка
`novac/src/emit_c/emit_requires.nv:16-21` —

```
// The wave's named boundary: file/line stay EMPTY ("" and 0) -- the emitter
// holds neither a file name nor line numbers, so a violating program panics
// `:0:` where the oracle names the source line. The carriers never violate,
// the diff corpus sees no difference; a source-location channel is
// inventoried in the plan (274.5, 3-pred38) as its own wave.
```

Записываю его не как открытие, а как ЗАМЕР этой границы: расхождение
воспроизводимо, exit-коды при этом совпадают (101/101), и оно закрывает собой
все четыре пробы выше — то есть маскирует Н3 и Н5 в том же выводе, если смотреть
только на первую расходящуюся строку.

**И вот что в этой шапке проверяемо неверно.** Фраза «the carriers never
violate, the diff corpus sees no difference» — правда о СЕГОДНЯШНЕМ корпусе, и
именно она делает всю группу Н3–Н6 невидимой:

* корпус дифференциального стража — `novac/fixtures/**/pos_*.nv` плюс
  `examples/` (`scripts/guards/check-novac-differential.sh:7-13, 21-26`), и
  сравнивается ИСХОД плюс stdout/exit у принятых обеими сторонами;
* `grep -rln "requires" novac/fixtures/ examples/` даёт четыре файла, и
  РАБОТАЮЩАЯ клауза в них одна — `examples/basics/requires_gate.nv` (в
  `novac/fixtures/protocol_decl/neg_1.nv` это `neg_`-фикстура, не `pos_*`;
  в `examples/flagship/aggregator/src/main.nv` и `examples/tour/types.nv`
  слово стоит только в комментарии). Обе функции носителя вызываются
  валидными аргументами — `half(8)`, `third(9)`;
* держатель-тест — `novac/src/pipeline/subset_test.nv:655-670` — утверждает
  подстроку эмиссии для ОДНОГО условия, `n > 0`:

```
    assert(c.contains("nova_contract_violation(NOVA_CONTRACT_PRE, \"half\", \"n > 0\", \"\", 0,"))
```

`n > 0` — условие без строкового литерала, чьи листья склеиваются ровно в
исходный текст. То есть страж и держатель зелены на единственной форме, где ни
Н3, ни Н4, ни Н5 не выражаются.

## Находки машинной строкой

НАХОДКА | novac | emit_c | lit_leading_zero_dec | оба приняли (rc=0), оба exit 0; оракул `123`, novac `83` — в C `((nova_int)0123LL)`, восьмеричное | `check/literal_rules.nv:53,57` («Leading zeros are stripped first») против `emit_c/emit_expr.nv:241` `@body.append(leaf_text(t))`
НАХОДКА | novac | emit_c | lit_leading_zero_const | оба приняли, оба exit 0; оракул `123`, novac `83` — тот же перенос текста через `const` | те же два адреса, второй носитель
НАХОДКА | novac | emit_c | lit_leading_zero_neg | оба приняли, оба exit 0; оракул `-123`/`101`, novac `-83`/`65` — арифметика идёт по завёрнутому значению | те же два адреса; `nova_int_checked_add(x, 1)` над `0100LL`
НАХОДКА | novac | emit_c | lit_leading_zero_pattern | оба приняли, оба exit 0; оракул `1`/`0`, novac `0`/`1` — `match` выбирает ДРУГУЮ ветвь | третья дверь того же свойства: `emit_c/emit_match.nv:126` `@body.append(leaf_text(p0))`
НАХОДКА | novac | emit_c | lit_octal_invalid | оба приняли (check rc=0, emit rc=0); оракул `8`/`9`, clang отверг эмиссию novac: `invalid digit '8' in octal constant` | тот же `emit_expr.nv:241`; `int_text_fits("08")` снимает ведущий ноль и пропускает
НАХОДКА | novac | emit_c | hex_index | оба приняли (rc=0), оба exit 0; оракул `20`, novac печатает пустую строку — аргумент выпал | `sem/slots.nv:584-614` (в `is_expr_kind` нет `Index`), `check/typing.nv:849-852` и `emit_c/emit_expr.nv:620-623` — обе хвостовые ветви считают его пунктуацией
НАХОДКА | novac | emit_c | ctl_dec_index | оба приняли, оба exit 0; то же на ДЕСЯТИЧНОМ индексе — свойство не про hex-волну | те же три адреса
НАХОДКА | novac | emit_c | index_two_args | оба приняли, оба exit 0; оракул `a20b`, novac `ab` — аргумент выпал ИЗ СЕРЕДИНЫ списка, соседи напечатаны | те же три адреса; отказ на индекс существует и назван в `check/binds.nv:317`, но только на пути привязки
НАХОДКА | novac | emit_c | index_in_loop | оба приняли, оба exit 0; оракул `10`/`20`/`30`, novac три пустые строки | те же три адреса
НАХОДКА | novac | emit_c | arg_tuple_lit | novac принял (check rc=0, emit rc=0), кортеж выпал из эмиссии; поведение оракула не мерится — его собственный codegen не собрался | вторая форма того же свойства (`TupleExpr` тоже вне `is_expr_kind`)
НАХОДКА | novac | emit_c | req_display_quotes | оба приняли (check rc=0, emit rc=0); оракул `1` exit 0, clang отверг эмиссию novac: `expected ')'` на `"s != "no""` | `emit_c/emit_requires.nv:39` вставляет `display_of` в C-литерал без экранирования; `emit_c/shell.nv:167` называет пул «the only place a Nova literal becomes C text»
НАХОДКА | novac | emit_c | req_display_escape | оба приняли; тот же отказ clang на `"s != "a\nb""` | вторая форма (backslash-escape внутри литерала условия)
НАХОДКА | novac | emit_c | req_cond_dollar | оба приняли; тот же отказ clang на `"s != "a\$b""` | третья форма (escaped dollar внутри литерала условия)
НАХОДКА | novac | emit_c | req_display_neg | оба приняли, оба exit 101; оракул `(n > -1)`, novac `(n > - 1)` | `sem/slots.nv:840-847` `display_of` склеивает листья пробелом; норма `<src>` — `spec_tests/conformance/contract_exprdisplay_selfaccess_neg.nv:11`
НАХОДКА | novac | emit_c | req_display_call | оба приняли, оба exit 101; оракул `(pos(n))`, novac `(pos ( n ))` | тот же адрес, второй синтаксис
НАХОДКА | novac | emit_c | req_display_field | оба приняли, оба exit 101; оракул `(p.n > 0)`, novac `(p . n > 0)` | тот же адрес, третий синтаксис
НАХОДКА | novac | emit_c | req_method_selffield | оба приняли, оба exit 101; оракул `(@n > 0)`, novac `(@ n > 0)` — ровно форма, ради которой написана фикстура оракула | тот же адрес, четвёртый синтаксис
НАХОДКА | novac | emit_c | req_msg_interp | оба приняли, оба exit 101; оракул `requires failed: got -5`, novac `requires failed: got ${x}` | `emit_c/emit_requires.nv:57` `return t.text` — исходный текст литерала объявлен готовым C-литералом
НАХОДКА | novac | emit_c | req_msg_interp_str | оба приняли, оба exit 101; оракул `bad tag`, novac `bad ${s}` | тот же адрес, интерполяция str-параметра
НАХОДКА | novac | emit_c | req_msg_interp_two | оба приняли, оба exit 101; оракул `a=-1 b=7`, novac `a=${a} b=${b}` | тот же адрес, две вставки в одном сообщении
НАХОДКА | novac | emit_c | req_msg_dollar | оба приняли, ответ СОВПАЛ (`cost $5`), но в C эмитировано `"cost \$5"` — clang: `warning: unknown escape sequence '\$'` | `emit_c/shell.nv:180` применяет `unescape_dollar`, `emit_c/emit_requires.nv:41` — нет; обе строки в ОДНОЙ эмиссии
НАХОДКА | novac | emit_c | req_violate_plain | оба приняли, оба exit 101; оракул `panic: p.nv:4:`, novac `panic: :0:` | `emit_c/emit_requires.nv:38-40` пишет `"", 0`; граница НАЗВАНА в шапке файла (строки 16-21), но «the diff corpus sees no difference» держится на единственном носителе `examples/basics/requires_gate.nv`, который контракт не нарушает

## Контроли: пробы, на которых расхождения НЕТ

Без них ни одна строка выше не значит, что причина названа верно.

| проба | что показывает |
|---|---|
| `ctl_baseline` | `println("ok", 7)` — смоук зелёный; инструмент мерит, а не ломает |
| `ctl_dec_plain` | те же пять форм Н1 БЕЗ ведущего нуля (литерал, `const`, отрицание, арм `match`) — совпало. Значит Н1 — про ведущий ноль, а не про перенос текста вообще |
| `ctl_requires_plain` | `requires n > 0, "half wants a positive"`, контракт НЕ нарушен — совпало. Значит Н3/Н5/Н6 живут только на пути нарушения |
| `req_multi_clause` | две клаузы подряд, обе выполнены — совпало |
| `req_expr_body` | `requires` на теле-выражении (`=> n / 2`) — совпало |
| `req_andor_violate` | условие с `&&` — текст условия совпал до байта, разошлось только `:0:`. Значит Н3 — про склейку, а не про сложность условия |
| `req_msg_quotes` | экранированные кавычки в СООБЩЕНИИ (`"say \"hi\""`) — текст совпал. Значит Н4 — про условие, а не про слот сообщения |
| `ctl_strlit_dollar` | `println("cost \$5")` через литеральный пул — совпало. Контраст к `req_msg_dollar` в той же форме |
| `hex_pattern_match`, `ctl_hex_max`, `hex_leading_zeros`, `hex_neg`, `hex_bin_ops`, `hex_min_via_neg` | hex в паттерне `match`, `0x7FFFFFFFFFFFFFFF`, `0x00000000000000000001` и `0x0`, `-0x1`, hex в арифметике — ВСЕ совпали. Hex-волна на этих формах чистая |
| `ctl_variant_arg`, `var_scrutinee`, `var_return`, `var_tuple` | `Kind.Dot` и голый `Dash` в аргументе, вариант как скрутинант `match`, вариант в `return` и в хвосте, `(Dot, 1)` с деструктуризацией — ВСЕ совпали |
| `index_bind`, `index_in_requires`, `index_in_bin` | тот же индекс в привязке, в условии `requires` и под `Bin` — НАЗВАН отказом E2-b. Граница Н2: свойство про позицию аргумента `println`, а не про индекс вообще |
| `arg_ifexpr`, `arg_unsafe`, `arg_tuple` | `if`-значение, `unsafe`-блок и кортеж ЧЕРЕЗ ИМЯ в той же позиции — все три названы отказом. Граница Н2 с другой стороны |

## Вне клетки: сюда К1 не относится, но пробы есть

ВНЕ КЛЕТКИ | index_call_arg | `grab(v[1])` — check rc=1, но текст ВРЁТ: «this call omits `n`, a parameter with no default value». Аргумент не сосчитан, потому что `arg_count` идёт через `is_arg_node` → `is_expr_kind` (`sem/slots.nv:126-127`). Это ровно тот носитель, которым комментарий `slots.nv:607-613` объясняет добавление `RecordCtor`
ВНЕ КЛЕТКИ | index_ret | `fn head(v []int) -> int { v[0] }` — check rc=1, текст врёт так же: «fn declares a return type but its body ends without a value»
ВНЕ КЛЕТКИ | index_named_arg | `grab(n: v[1])` — check rc=0, emit rc=2, `E_NOVAC_ICE: emit: expression kind outside the subset`. Третья форма того же корня, и она даёт именно тот «worst kind of green», о котором говорит `binds.nv:312-316`
ВНЕ КЛЕТКИ | hex_overflow | `println(0x8000000000000000)` — ОРАКУЛ принимает и МОЛЧА заворачивает (`-9223372036854775808`, exit 0), novac отвергает по имени (#810). Расхождение вердикта, и в эту сторону
ВНЕ КЛЕТКИ | var_shadow | `ro Dot = 3` при `type Kind enum Dot \| Dash` — ОРАКУЛ отвергает (`E_REFUTABLE_BINDING`, D52/D59/D411), novac принимает и эмитит `nova_int Dot = ((nova_int)3LL);`. Расхождение вердикта
ВНЕ КЛЕТКИ | var_array | `[]Kind.of(Dot, Dash)` — оракул `1`/`2`, novac ICE: «mangle: a SUM as an instance argument — its in-name spelling is not measured on the shell»
ВНЕ КЛЕТКИ | var_eq | `if k == Dot` — оракул `1`, novac отвергает по имени (D46, протокольная диспетчеризация)
ВНЕ КЛЕТКИ | var_foreign2 | `Kind.Ok` при соседнем `type Res enum Ok \| No` — ОРАКУЛ принял и не собрался (`use of undeclared identifier 'Kind_Ok'`), novac отверг по имени. Проба про оракул, не про novac
ВНЕ КЛЕТКИ | arg_tuple | `println("a", t, "b")` с кортежем через имя — ОРАКУЛ не собрался (`nova_print_int` получил структуру). Проба про оракул
ВНЕ КЛЕТКИ | hex_pattern_disj, hex_const_u8, lit_underscores, lit_binary, lit_upper_hex_prefix, lit_upper_hex_overflow | отказы novac по имени либо синтаксическим отказом: `0x0A \| 0x0D` в паттерне, `expr as T`, `1_000`, `0b1010`, `0X1F`. Оставлены, чтобы границу подмножества было видно без повторного замера
ВНЕ КЛЕТКИ | req_display_dot | моя собственная ошибка в форме пробы (`type Pt record { n int }`) — обе стороны отвергли; переписана в `req_display_field`

## Противоречия — оба места дословно, выбор не мой

**П1. «Единственное место, где литерал Nova становится текстом C» — не
единственное.**

`novac/src/emit_c/shell.nv:165-178`:

```
/// Write the literal pool: one C array per string the file holds.
///
/// The only place a Nova literal becomes C text -- which is why the escaped
/// dollar is unescaped here and nowhere else.
fn emit_strlit_defs(mut out StringBuilder, lits []str) -> () {
    mut i = 0
    for raw in lits {
        // raw is the token text INCLUDING quotes. Nova's subset escape set
        // (\" \\ \n \t \r) matches C's one for one -- check refuses everything
        // else BY NAME -- with a single exception: `\$`. ... So the backslash
        // is dropped HERE, at the one place that writes a literal into C, and
        // the rest of the text is passed through untouched.
```

`novac/src/emit_c/emit_requires.nv:36-42`:

```
        @body.append("    if (!(")
        @emit_expr(rk[1])
        @body.append(")) nova_contract_violation(NOVA_CONTRACT_PRE, \"${name}\", \"")
        @body.append(display_of(rk[1]))
        @body.append("\", \"\", 0, ")
        @body.append(contract_msg_c(rk))
        @body.append(");\n")
```

Оба места пишут литерал Nova в C; первое объявляет себя единственным, второе
работает без `unescape_dollar` и без экранирования. Замер обеих строк в одной
эмиссии — `req_msg_dollar` выше.

**П2. Инвариант помечен «нарушение НЕВЫРАЗИМО», и нарушение выразимо одной
строкой программы.**

`novac/src/check/typing.nv:837-843`:

```
            // The printer set is int/str/bool/f64. Every other primitive is
            // NAMEABLE (the universe holds all fifteen) but not yet
            // printable, and that boundary belongs here -- the emitter's ice
            // for a missing printer must never be the first answer. [INV-PROPERTY]
            // [INV-PROPERTY] -- the refusal returns two lines below, so the
            // emitter is not reached at all for this form; violating it would
            // require deleting the check, not merely getting it wrong.
```

`scripts/guards/check-invariant-discipline.sh:32`:

```
#   [INV-PROPERTY]            — уже не инвариант: нарушение НЕВЫРАЗИМО (шаг 0/1).
```

Проба `index_two_args` (`println("a", v[1], "b")`) проверку не удаляет: ветвь
`is_expr_kind(ck)` просто не берётся, эмиттер достигается, ICE не случается, и
печатается `ab` вместо `a20b`. Выбирать между «пометка неверна» и «инвариант
надо переформулировать» — не моё дело.

**П3. Отказ на индекс объясняет, ЧЕГО он не допускает, и допускает худшее.**

`novac/src/check/binds.nv:312-316`:

```
    // Accepting the form anyway is the shape the differential guard calls the
    // worst kind of green: `check` clean, `emit` an ICE ("binding initializer is
    // neither a match, a ctor nor an expression"). So the subset refuses, and the
    // refusal names WHAT is missing and WHERE -- the fix is a wider probe, not a
    // change to the program being compiled.
```

`novac/src/emit_c/emit_expr.nv:620-623`:

```
        } else {
            // The callee leaf and the parenthesis leaves are not arguments;
            // the callee itself was already refused or accepted by name above.
        }
    }
    @body.append("    nova_print_newline();\n")
```

Первое место говорит «поэтому подмножество отказывает»; второе на том же виде
узла не отказывает, а тихо печатает программу, дающую другой ответ. Оба
отвечают на вопрос «что делать с индексом, который эмиттер не умеет».

## Что обошёл и почему

* **`0X`-префикс** (`lit_upper_hex_prefix`, `lit_upper_hex_overflow`) —
  единственный путь, которым можно было бы обойти хексовую ветвь
  `int_text_fits` (`literal_rules.nv:58`, тест `t[..2] == "0x"` регистрозависим),
  лексер novac закрывает синтаксическим отказом. Носителя нет — так и записываю.
* **Плавающие литералы** (`emit_expr.nv:246`, та же подстановка `leaf_text`)
  под Н1 не отстреляны: у C и Nova десятичная форма float совпадает, а формы
  вроде `1e3`/`.5` я не мерил. Гипотеза, пробы нет.
* **`Unsafe` и `IfExpr` в аргументе `println`** названы отказом на другом пути и
  потому в Н2 не считаются носителями; я НЕ проверял, есть ли у них третья
  позиция, где отказа нет (например, поле конструктора).
* **`ArraySpread`** — четвёртый вид, отсутствующий в `is_expr_kind`, — не
  отстрелян: он живёт внутри литерала массива, а не в позиции аргумента, и
  подобрать форму, где он приходит аргументом, я не пробовал.
* **`ensures`/`invariant`** (вторая половина контрактов) в подмножестве novac не
  искал вовсе: цель названа прологом `requires`.
* **Взаимодействие Н3 и Н6** оставлено как наблюдение: `:0:` печатается ПЕРВЫМ,
  поэтому смоук на всякой нарушающей пробе показывает сначала его. Если Н6
  починят раньше, все пробы Н3/Н5 останутся красными — это стоит знать, но
  порядок починки не мой.
* **Обратная проба со снятием рычага поодиночке** невозможна без правки дерева
  (`is_expr_kind`, `int_text_fits`, `display_of` — по одной за раз), а брифом
  запрещено и править, и пересобирать компиляторы. Вместо неё сделаны контроли
  «свойство отсутствует» — по одному на класс, перечислены выше.
* **`std`, мега-CU и полный `nova test`** не гонял ни разу: только 64 своих
  пробы, смоук по каждой и один запуск `check-novac-no-silent-skip.py`.

Ни одна строка не закрыта, ни одна не заведена в реестр: это делает окно.
