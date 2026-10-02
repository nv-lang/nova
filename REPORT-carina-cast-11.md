# Отчёт облачной сессии, шаг 11: №1654 (ресиверы оболочки в Карине), №1653 (оракул), синк — ждёт публикации (ветка `p274-carina-cast`)

Вершина до шага — `b5e66a89`. Linux. `origin/main` (08ff689c) публикации ещё не несёт — синк (п. 3
задания) НЕ сделан. Оракул и Карина пересобирались после каждой правки. Строк журнала времени нет.

```
0. ОБОЛОЧКА ВЕТКИ ПЕРЕГЕНЕРИРОВАНА (коммит bb874554). Без этого №1654 нечем доказать: шаблон ветки
   держал все ресиверы по указателю, а оракул ветки (main с №1598) малый `ro @` передаёт копией —
   shell-freshness был красным. `novac-regen-shell.sh` (windows + linux), 14730 -> 14765 строк; база
   novac-emission +35 на каждом из четырёх файлов, с летописью. При синке берётся оболочка main.
/
1. №1654 — КАРИНА: РЕСИВЕР МЕТОДА ОБОЛОЧКИ (коммит c86c4231).
   Одно правило для строк программы и оболочки, с исключениями оракула (value_abi.rs):
     emit_expr.nv — `row_recv_by_pointer` для любой строки (условие `in_program` снято);
     закреплённые типы `#no_copy` / `#zero_on_move` / `#share` / `consume` — указатель при любом
       размере: `TypeDef.copy_pinned`, читается при сборе (`copy_pinned_before`, sem/value_recv.nv);
     fluent `-> @` — указатель на входе И на выходе: сигнатура `NovaValue_X*`, значение вызова
       читается через `(*...)` (`row_returns_recv_pointer`); раньше `mut @ -> @` value-записи
       программы тоже не собирался (тело возвращало указатель, сигнатура — значение);
     временный ресивер метода оболочки — тот же датированный отказ, что у строк программы
       (VALUE_RECV_TEMP_MSG, E2-b3; нового текста нет).
   СТАТИК `Path.posix`: Карина делала его тело сама (`instantiates`), но mono отмечал строку как
     `seen`, а тела выдаются только экземплярам — имя напечатано, тела нет. Теперь неуниверсальное
     handed-тело — экземпляр с пустой подстановкой (mono.nv).
   Клетки (смоук, байт в байт с оракулом): Path (носитель, через место), IoError, EnvVar,
     OpenOptions (fluent `mut @ -> @`, порознь), свои `ro @` / `mut @ -> @` / `ro -> @`. Временный
     `Path.posix(..).to_str()` — датированный отказ (neg_2).
   НЕДОСТИЖИМО ПО ДРУГОЙ ПРИЧИНЕ: Duration и итератор — экземпляр обобщённого std-метода
     (`to_nanos`, `iter`), строящий std-запись, Карина отказывает «this record construction names
     a field the type does not declare (#812)». Правило для них закреплено тестом на двери.
   Фикстуры: value_recv/pos_2, value_recv/neg_2; тест pipeline/value_recv_test (закрепления: #no_copy,
     #zero_on_move, #share после #stable, consume-тип; fluent; контроль — малый `ro @` копией).
   Проба в обе стороны: место вызова снова с `in_program` — pos_2 падает в clang (`NovaValue_Path`
     вместо `NovaValue_Path *`); закрепление и fluent выключены — оба теста красные.
/
2. №1653 — ОРАКУЛ (коммит 0776b707). КОРЕНЬ не в кодогене, а в выводе типа: `generic_variant_ctor_type`
   (types/generic_sum.rs) решал параметры по ВСЕМ полям; `u16 == int` (свой тип литерала) проваливал
   решение, `e` оставалась без типа, `match` связывал payload как `void*`. Поле без параметра в
   решение не входит — его сверяет дверь №1645 (литерал берёт `u16`, D489).
   Фикстуры: standalone/p1653_generic_sum_literal_concrete_field_ok (оба порядка полей, поле-контейнер
     `[]T`), neg/p1653_generic_sum_literal_concrete_field_neg (пины E_LIT_OUT_OF_RANGE).
   Проба в обе стороны: пропуск поля выключен — положительная фикстура CC-FAIL (`void*`); включён — PASS
   (отрицательная зелёная в обоих состояниях: литерал держит дверь №1645).
/
НАЙДЕНО ПОПУТНО (строка №TBD, 🔴 К1): оракул печатает метод `write` на результате fluent-вызова
  value-записи как ЗАПИСЬ ЧЕРЕЗ УКАЗАТЕЛЬ — `o.read(true).write(true)` (std OpenOptions) и
  `f.bump().write(5)` (своя запись) не собираются в C; `f.bump().bump()` собирается. Имя семейства
  `.write` у `*T` решает раньше типа получателя.
/
ПРИЁМКА:
  мера 0.2: 29, ICE 0 (без изменений);
  модульные тесты Карины pipeline, check, lower, lex, sem — PASS;
  крейт compiler-codegen --lib 1294/0; фильтры spec_tests --full: p1653 2/0, p1338, generic_sum 6/0,
    p1517 4/0, p1645 4/0, variant 33/0; `nova check` std/examples/novac — те же коды, что до правки;
  стражи `sh scripts/tools/novac-gate-guards.sh .` (19:16–19:39): 98 запущено; красные —
    check-novac-time-ledger (неглубокий облачный клон, как в прошлых шагах);
    check-novac-emission-size (+35 строк оболочки) — база поднята в bb874554;
    check-novac-row-fields, check-novac-resolve-discipline, check-novac-surface — исправлены в e8bd5b9b
      (поле переименовано в `copy_pinned` и вписано в §10.3в плана 274; тест ищет через двери;
      база поверхности builtins 68 -> 70, sem 341 -> 343 с летописью), повторно — ok;
    check-novac-differential — зелёный;
  флагманы: все шесть целей flagship-targets.txt собираются с --strict-effects (rc=0).
/
3. СИНК — НЕ СДЕЛАН: публикации в origin/main нет (проверено перед сдачей). План по твоему письму:
   emit_flow `@print_tested_arm` — уровень guard'а с ведущими value-`if` поверх модели `heads`;
   check/literal_rules.nv — оба импорта (`literal_digits` и `subst_type`); subset-debt.baseline — счёт
   стража на сведённом дереве; оболочка — main'овская (моя регенерация уступает); затем проба
   guard-фикстур (try_option/neg_3 -> guard_block_form/pos_1, coalesce_nested).
/
КОММИТЫ:
  bb874554 novac: shell regenerated from the branch oracle (value receivers by copy after 1598)
  c86c4231 novac: a shell method's receiver crosses the call by the oracle's rule, not always by value (1654)
  0776b707 types: a field that names no type parameter does not take part in solving a generic sum's instance (1653)
  a8a0308e docs: registry rows 1653 and 1654 fixed on the branch; ... `write` on a fluent value result
  e8bd5b9b novac: 1654's pin is `TypeDef.copy_pinned`, declared in the row-field table; ...
  (этот отчёт) docs: eleventh report of the cloud session on p274-carina-cast
/
ВОПРОСЫ ИНТЕГРАТОРУ:
  1. Синк жду публикации — скажи, когда.
  2. Строка №TBD (оракул, `write` на fluent-результате) — номер и чья.
```
