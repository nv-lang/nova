# Отчёт: Карина — вариант-запись (А) и биндеры в `|` (Б)

Ветка `p-novac-patterns-ab-a25r8x`, облачная сессия, 2026-10-01.

**КОРЕНЬ:**
- **(А)** Вариант-запись отказывался ещё на входе. Парсер не читал тело `| Branch { … }` как поля, а sem не знала имён полезной нагрузки. Поэтому образец `Branch { children, .. }` и конструкция `Branch { id: … }` упирались в `@report_if_record_variant` (rules.nv), с текстом «record-form variant is not compiled yet».
- **(Б)** Дизъюнкция с именованным биндером отказывалась одной дверью в match_arms.nv («takes `_` binders only»). Правила согласования имён между альтернативами и опускания (lowering) общих имён не было.

**ФИКС:**
- `novac/src/check/record_variant.nv:38`: образец с головой разрешает каждое поле в позицию нагрузки по имени (`payload_named`).
- `novac/src/check/record_variant.nv:98`: конструкция записывается как callee-вариант со слотами.
- `novac/src/check/disj_pattern.nv:43`: альтернативы обязаны связать одни и те же имена одного типа.
- `novac/src/lower/lower_disj.nv:56`: имена объявляются один раз, а пишет их та альтернатива, которая сработала.
- Опоры:
  - `sem/collect.nv`: таблица `Ctx.payload_names`, параллельная `payload_types`; раскладка остаётся позиционной, эмиссия суммы не менялась.
  - `parse/type_decl.nv`: поля варианта.
  - `parse/expr.nv`: квалифицированная конструкция `Node.Branch {…}`.
  - `emit_c/emit_place.nv`: вызов мейкера варианта; поля вычисляются в порядке записи.

**КЛЕТКИ** (форма × позиция → до / после):

| Форма | Позиция | До | После |
|---|---|---|---|
| объявление `\| Branch { id NodeId, … }` | sum-декларация | отказ | ok |
| `Branch { id: e, kind: e, children: e }` | инициализатор, тело арма, хвост, правая часть `=` | отказ | ok |
| `Branch { … }` | аргумент или вложенная позиция | отказ | отказ (новый текст: «…constructed only where its value is placed… (E4, step 4d of 274.11 E.10)») |
| шорткат `Branch { id }` | — | отказ | ok |
| `Node.Branch { … }` | — | отказ | ok |
| `Node.Nope { … }` | — | отказ | язык: «a type name takes a variant after the dot…» |
| `Branch { children, .. }`, `Branch { id, kind, children }`, `Branch { id: p }` | match, `if`-образец | отказ | ok |
| без `..` при неполном списке | — | отказ | язык (правило D411, как у записи) |
| `Circle(r)` на варианте-записи | образец | отказ подмножества | язык |
| `Circle(r) \| Square(r)` | образец | отказ | ok |
| разные имена в альтернативах | образец | отказ | язык |
| разные типы одного имени | образец | отказ | язык; оракул так не делает, см. ВОПРОСЫ 1 |
| конструкция generic-суммы фигурной формой | — | отказ | отказ (274.7 W4) |

Тексты отказов Карины и оракула:
- Позиционный образец на варианте-записи.
  - Карина: «this variant is declared in RECORD form and is destructured by position -- write its fields by name, `Circle { radius }` (D54; the oracle's E_RECORD_VARIANT_POSITIONAL_PATTERN, registry 719)».
  - Оракул: `[E_RECORD_VARIANT_POSITIONAL_PATTERN] вариант `Shape.Circle` объявлен RECORD-формой … а разбирается ПОЗИЦИОННО`.
- Разные имена в альтернативах.
  - Карина: «every alternative of a `|` pattern must bind the same names -- … (E_OR_PATTERN_BINDING_MISMATCH, registry 1009) …».
  - Оракул: `[E_OR_PATTERN_BINDING_MISMATCH] альтернативы дизъюнкции обязаны вводить ОДИН И ТОТ ЖЕ набор биндеров…`.
- Разные типы одного имени.
  - Карина: «a name every alternative of a `|` pattern binds must have one type in all of them…».
  - Оракул: принимает; на прогоне читает чужую раскладку или падает (заведено в реестр).

**МЕРА 0.2:** всего 316 → 335; «record-form variant» 6 → 0; «`|` binders» 5 → 0; ICE 0 (GC_DONT_GC=1, один процесс).

Открылось +35 более глубоких диагностик, по группам (не чинил):
- поверхность оболочки:
  - методы `Vec[T]`: +13, из них 10 в моём новом коде парсера;
  - экземпляр `Vec`: +1;
- `Option[T]`: +3;
- unknown field (Token): +5;
- голый `None`: +5;
- payload count disagrees (mono.nv:319): +3;
- `ice` не вызываем: +3;
- вызов на ТИПЕ: +1;
- элемент не читается: +1.

Кроме 11 целевых закрылись ещё 5: undeclared `id` 1, did-not-parse 1, заголовок `for` 3.

**ДОЛГ ПОДМНОЖЕСТВА:**
- `undated` 61 → 59: база опущена, строка хроники в `scripts/guards/subset-debt.baseline`.
- `undated_by_door` 76 → 71: опущена.
- `refusals` 140 → 140.

**ФИКСТУРЫ:**
- `novac/fixtures/record_variant/pos_1.nv`: `Node.Branch`, шорткат, поля в другом порядке, оба образца, доступ к полю после связывания.
- `novac/fixtures/record_variant/neg_1.nv`: `NOVAC_EXPECT destructured by position`.
- `novac/fixtures/or_pattern_binder/pos_1.nv`.
- `novac/fixtures/or_pattern_binder/neg_1.nv`: `NOVAC_EXPECT must bind the same names`.
- Смоук байт-в-байт: **да**, обе pos (`novac-e1-smoke.sh`, GC_DONT_GC=1). Остальные pos-фикстуры (72) тоже зелёные; страж пинов: 39/39.

**ОБЕ СТОРОНЫ:** на одном дереве собраны два бинаря Карины, с отключёнными дверями и с включёнными.
- Двери отключены: обе pos красные; neg дают старые тексты «record-form variant is not compiled yet» и «takes `_` binders only», пины краснеют.
- Двери включены: обе pos ok, neg совпадают с пинами.

**СТРАЖИ КАРИНЫ:** `novac-gate-guards.sh`: 95 ok / 3 fail, 7 тяжёлых пропущено. Все три красных — окружение, не ветка:
- `check-novac-time-ledger.py`: мелкий клон, истории нет.
- `check-novac-commit-no-simplification.py`: в гейте ему не дают файл сообщения. На моих сообщениях с файлом он ok.
- `check-novac-shell-freshness.sh`: `shell.tpl.c` расходится с эмиссией оракула. Входы стража (`novac/src/emit_c/shell.tpl.c`, `novac/probe/`) ветка не трогала: diff от 9712b30 пуст.

Сознательно подняты две базы, каждая со строкой хроники:
- sem-поверхность 300 → 303: `payload_named`, `variant_is_record`, `ctor_head_variant`;
- наполнители таблиц 35 → 36: `payload_names`, наполняет только sem.

Линт по 18 изменённым файлам: 0 находок.

**МОДУЛЬНЫЕ ТЕСТЫ:** `check-novac-module-tests.sh`: 9 PASS / 1 FAIL. Падает `novac/src/pipeline/subset_pattern_test.nv:52`: тест пришпиливает старый отказ «takes `_` binders only», который задание и снимает. Файл в запретном каталоге `pipeline/`, я его не трогал. Патч в ВОПРОСАХ 3; с ним 10/0, проверено и откачено.

**САМОСБОРКА:** да. Оракул собирает Карину (`nova build novac/src/main.nv`); мера 0.2 по своим исходникам проходит без ICE.

**КОММИТЫ:**
- `8f2ab60`: novac: record-form sum variants and `|` patterns with binders.
- `0eca5c5`: novac: Carina gate guards on the record-variant and `|`-binder change.
- последний: этот отчёт.

Слияние `origin/integrate` (9712b30): Already up to date.

**ОТКРЫТО:**
- Вариант-запись в позиции аргумента или вложенно отказывается: нужна эмиссия 4d в `emit_c.nv` и `emit_expr.nv`, файлы запретные.
- Фигурная конструкция варианта generic-суммы отказывается (274.7 W4).
- `|` с альтернативой не-вариантом отказывается (E2-b).
- В реестр 221.1 заведено 5 строк №TBD (страж формы краснеет на TBD, как и ожидалось):
  1. главная строка;
  2. хвостовой `match` с биндером в блоке терял тип, и функция возвращала 0; починено в ветке;
  3. оракул читает биндеры `|` по позициям первой альтернативы: молча неверное значение (12 вместо 21) или segfault при разных типах;
  4. оракул не проверяет вариант-запись: принимает дубль поля, неисчерпывающий match печатает мусор, неизвестное поле в образце даёт ошибку clang;
  5. потеря нагрузки у handed-суммы.

**ВОПРОСЫ ИНТЕГРАТОРУ:**

1. **Правило согласования биндеров в `|` не записано ни в одном D-блоке.** Его форму определяют только `spec/open-questions.ru.md:8450`, раздел OPEN («все альтернативы биндят **одинаковый** набор имён (bootstrap берёт биндинги из первой альтернативы)»), и код оракула (`E_OR_PATTERN_BINDING_MISMATCH`, №1009). Про ТИПЫ спека молчит. Оракул разные типы принимает и на прогоне падает.
   - Задание велело при пробеле в спеке остановиться. Записка координатора велела действовать по заданию и отметить развилку. Я реализовал, отмечаю здесь.
   - Варианты:
     - (а) амендмент к D486 §2: одинаковые имена И один тип, связывание ПО ИМЕНИ; Карина уже так делает;
     - (б) только имена, как оракул;
     - (в) откатить проверку типов в Карине.
   - Рекомендую (а). Без проверки типов ветка читает чужую раскладку; у оракула это уже дефект с segfault. Номер D-блока нужен от вас.
2. **Квалифицированная КОНСТРУКЦИЯ `Node.Branch { … }` в spec/ не записана.**
   - В образцах `qual-head` есть: `spec/decisions/03-syntax.md:13729`, `record-pat = qual-head? '{' field-pat … '}'`. Для выражения-конструкции такой формы нет.
   - Оракул её принимает, задание её требует. Сделана по аналогии с образцом.
   - Рекомендую амендмент к D54: голова конструкции записи — тот же `qual-head`. Иначе форма живёт только в двух компиляторах.
3. **Запретный тест `novac/src/pipeline/subset_pattern_test.nv` держит снятый отказ**, отсюда модульные тесты 9/1. Нужно применить патч в ветке, где `pipeline/` разрешён:
   ```diff
   -test "pattern alternatives OR their tag tests; a named binder refuses (disjunction wave)" {
   +test "pattern alternatives OR their tag tests; a named binder binds (disjunction wave)" {
   ...
   -    // A named binder inside a disjunction refuses by name: alternatives
   -    // would have to agree on the binder, and no carrier needs that.
   +    // A named binder inside a disjunction BINDS since 2026-10-01 (D486 s2):
   +    // every alternative binds `x` with one type, so the arm reads one `x`.
   +    // Five carriers in types.nv needed it; the refusal is gone.
   ...
   -    assert(d2.len() >= 1)
   -    assert(d2[0].message.contains("takes `_` binders only"))
   +    assert(d2.len() == 0, messages(d2))
   ```
   Рекомендую влить вместе с этой веткой: контракт модуля сменился сознательно, и тест должен ехать за ним в том же слиянии.
4. **Вариант-запись в позиции аргумента** (`f(Branch {…})`) отказывается до шага 4d. Варианты:
   - (а) оставить как упрощение подмножества: носителей среди 11 нет;
   - (б) разрешить правку `emit_c.nv` и `emit_expr.nv`.
   Рекомендую (а) до волны 4d.
