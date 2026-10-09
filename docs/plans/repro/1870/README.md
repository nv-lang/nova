<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №1870 — область тела операции обработчика, задача #53

## РЕПРО

`repro.nv.txt`: ожидается `4207`, исходный компилятор печатает `4201`.
`repro3.nv.txt`: те же операции переставлены — исходный компилятор печатает `4207`.
`repro2.nv.txt`: соседняя операция читает локал без одноимённого параметра —
name-resolution правильно отказывает `undefined identifier \`leaked\``.
То есть утечка — в суждении о типе и материализации, не в разрешении имени.

```sh
cp docs/plans/repro/1870/repro.nv.txt <session-scratch>/repro1870.nv
nova-cli/target/release/nova build <session-scratch>/repro1870.nv -o <session-scratch>/repro1870.exe
<session-scratch>/repro1870.exe
```

`<session-scratch>` — scratchpad текущей сессии, не каталог исходников.
Для контрольных улик `repro2.nv.txt`/`repro3.nv.txt` применяется тот же шаг
копирования в scratchpad под суффиксом `.nv`. Постоянные исполняемые
фикстуры остаются в spec_tests, улики не подхватываются раннером.

До фикса C: `tag(nova_make_Signal_Other((nova_int)((Nova_Signal*)s)->tag))`.
После фикса: значение передаётся напрямую, без лишнего конструктора и as-cast.

## ТОЧКА / ФИКС / КЛАСС

`compiler-codegen/src/types/mod.rs`: `MapLitAnnotator::walk_expr_shape`,
ветка `HandlerLit | ProtocolLit`. `walk_stmt(Let)` добавлял `s:int` в
`var_types`, `walk_block_to` не восстанавливал её; следующая операция не
регистрировала параметры. `try_wrap_leaf` читал stale-тип и сплайсил sum-lift.
Это post-check проход `annotate_map_literals`, а не решение C-эмиттера.

Общая дверь `handler_method_scope` строит новый scope от внешней области
литерала, затеняет параметры (в том числе без аннотации) и вводит известные
типы параметров. Не добавляется новый режим или исключение для `Signal`.

| Место, где тела операций обходятся подряд | Что было / что сделано |
|---|---|
| `TypeCheckCtx::f1_expr_inner` | Общий scope, параметры отсутствовали. `f1_block` возвращал локалы блока, но имя параметра могло взять внешний тип; теперь отдельный `handler_method_scope` для каждой операции. |
| `BoundCtx::walk_expr` | Общий scope, параметры отсутствовали. `walk_block` восстанавливал локалы, но параметры не затеняли внешние; теперь та же дверь scope. ProtocolLit здесь проверяет структурное соответствие без обхода тел. |
| `MapLitAnnotator::walk_expr_shape` | Прямая утечка локалов предыдущей операции. Обмен карты на op-scope и восстановление после тела, обе формы тела (Expr/Block). |
| `TypeCheckCtx::walk_expr` (arity/type-reference walk) | Не ведёт изменяемого окружения локальных типов; синтаксический обход, нечему протекать. |
| `NameResCtx::walk_expr` | Уже отдельный frame параметров push/pop на каждую операцию; probe `repro2.nv.txt` подтверждает отказ соседнему локалу. |
| `consume_walk_expr` | Уже `consume_walk_isolated_expr/block` на каждую операцию; тип/consume-параметры передаются отдельно. |
| `MapLitCtx::walk_expr` | Не ведёт карту типов локалов: ожидаемые типы и синтаксические формы, нет наследования локала операции. |
| Проверки деклараций, never, effects/fail, capture, default-handler | Нет последовательного изменяемого scope локальных типов; capture-скан начинает каждую операцию со своих параметров, а проверки деклараций используют schema/ret_ty. |
| `CEmitter::emit_handler_lit` (вне чекера) | Параметры регистрируются заново для каждой impl-функции; boxing/ref-контекст изолирован. Именно sum-lift в AST вызывал обнаруженную порчу, правки emit_c нет. |
| Карина `novac/src/check/handler.nv::type_handler_op` | `@scope.mark()` перед параметрами и `@scope.release(pmark)` после Block/Expr и на ранних выходах; соседняя операция не наследует параметры/локалы. Тот же класс по коду не найден; исправлений Карины и новой строки №TBD нет. Это приёмка глазами, не заявление о прогоне Карины. |

Пути Rust: литерал в let/return, литерал в binding `with`, готовый `with X = h`,
обработчик с состоянием (внешние захваты), `#default_handler`-фабрика —
все приходят в те же HandlerLit-ветки. Новые runtime-фикстуры вызывают обе
операции и проверяют полученные значения.

Спека: `spec/decisions/04-effects.md`, D474 §0: контекст один на обработчик,
переменные захватываются из области, **где литерал написан**. Общий внешний
`current_ms` не разрезается. Формы общего блока локалов между телами операций
не найдено; D431/D474 не дают такой видимости. Язык не меняется, D-блок не нужен.

Инвариант: область операции не зависит от порядка соседних операций и не
меняет область места записи литерала. Без сохранения/восстановления карты
это неверно; переименование носителя в std инвариант не обеспечивает.

## ФИКСТУРЫ

Все в `spec_tests/conformance/standalone/`:

- `p1870_handler_op_local_leak_pos.nv`: `run=4207 kill=1 ok`, Block и Expr,
  payload и unit-вариант суммы проходят в отдельную функцию без порчи.
- `p1870_handler_op_same_type_pos.nv`: `run=42018 ok`; локал и параметр
  одного типа с разными значениями. Контроль законного соседнего случая,
  сам по себе до фикса зелёный — это не саботажный свидетель.
- `p1870_handler_precomputed_pos.nv`: `4207`, готовый обработчик `with X = h`,
  параметр Signal затеняет также внешнюю переменную `s int`, внешний `s`
  после литерала по-прежнему имеет значение 123.
- `p1870_handler_state_pos.nv`: `4107 state=42`, общее внешнее состояние
  остаётся общим, а параметр соседней операции не перетипизируется.
- `p1870_handler_default_pos.nv`: `4207`, фабрика `#default_handler`.

`compiler-codegen/src/types/handler_scope_tests.rs`: отдельный scope,
затенение нетипизированным параметром, annotator без каналов чекера —
законный int sum-lift остаётся, Signal-параметр и внешний Signal-захват не lift-ятся.

## Критерии приёмки (дословно из задания)

1. Регрессионная фикстура spec_tests/conformance/standalone/ (EXPECT_STDOUT): две операции, в первой `mut s = 0`, во второй параметр `s Signal` уходит в функцию — значение доходит неискажённым; до фикса RUN-FAIL, после — PASS; откат фикса -> красная (оба вывода дословно).
2. Вторая фикстура: локал и параметр одного типа с разными значениями.
3. Поиск класса: в отчёте перечислены все места чекера, где окружение тела операции наследуется от предыдущей операции, каждое закрыто или объяснено.
4. Строка №1870 в docs/plans/221.1-bug-sweep.md: если #50 ещё не влита и строки в main нет — завести самому (в main её может принести #50: при слиянии оставить одну строку), пометить закрытой с адресом фикса; проба docs/plans/repro/1870/.
5. Точечно: nova test spec_tests --filter на свои фикстуры и подкаталог handlers/effects; cargo test в compiler-codegen по затронутому модулю (через crew_watch machine: true); nova lint --deny на новых фикстурах. Вердикт — CI через land-task.

Критерий 3 — приёмка глазами, потому что полнота инвентаря обходов не
доказывается запуском одного носителя; таблица выше называет все найденные
пути и наличие/отсутствие изменяемого окружения типов.

## САБОТАЖ (реальное снятие фикса и пересборка)

Сняты все три подключения области операций (F1, BoundCtx, MapLitAnnotator),
пересобран nova-cli. Команды через `crew_watch {machine: true}`:

```sh
cargo build --release --manifest-path nova-cli/Cargo.toml
nova-cli/target/release/nova.exe check std/src --format short
nova-cli/target/release/nova.exe test spec_tests --filter p1870
cargo test --manifest-path compiler-codegen/Cargo.toml --lib types::handler_scope_tests
```

Красный вывод дословно:

```text
NEG-WRONG-STDOUT spec_tests/conformance/standalone/p1870_handler_default_pos  # expected stdout pattern '4207' not found in: 4201
NEG-WRONG-STDOUT spec_tests/conformance/standalone/p1870_handler_op_local_leak_pos  # expected stdout pattern 'run=4207 kill=1 ok' not found in: run=4201 kill=0 ok
NEG-WRONG-STDOUT spec_tests/conformance/standalone/p1870_handler_precomputed_pos  # expected stdout pattern '4207' not found in: 4201
NEG-WRONG-STDOUT spec_tests/conformance/standalone/p1870_handler_state_pos  # expected stdout pattern '4107 state=42' not found in: 4101 state=42
PASS: 1  FAIL: 4

pass must pass the Signal unchanged
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 1295 filtered out; finished in 0.00s
```

Классификация раннера здесь `NEG-WRONG-STDOUT`, не буквальный `RUN-FAIL`:
скомпилированный бинарь исполнился с неверным stdout — тот же требуемый
наблюдаемый отказ. Фикстуры, EXPECT-маркеры и значения не ослаблялись.
После саботажа восстановлены все три подключения и выполнена новая сборка.

Зелёный вывод повторного прогона после восстановления, дословно:

```text
PASS           spec_tests/conformance/standalone/p1870_handler_precomputed_pos  # (stdout/stderr)
PASS           spec_tests/conformance/standalone/p1870_handler_default_pos  # (stdout/stderr)
PASS           spec_tests/conformance/standalone/p1870_handler_op_same_type_pos  # (stdout/stderr)
PASS           spec_tests/conformance/standalone/p1870_handler_op_local_leak_pos  # (stdout/stderr)
PASS           spec_tests/conformance/standalone/p1870_handler_state_pos  # (stdout/stderr)
PASS: 5  FAIL: 0

test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 1169 filtered out; finished in 0.37s
```

Итог целевых проверок:

| Команда (все тяжёлые через crew_watch machine:true) | Вывод |
|---|---|
| `RUST_MIN_STACK=134217728 cargo test --manifest-path compiler-codegen/Cargo.toml --lib types::` | Строка выше, 129/0. Размер стека взят из `scripts/guards/check-crate-tests.sh:113`: без него расширенный прогон оборвался `STATUS_STACK_OVERFLOW` на существующем `e2e_impl_param_mode_mismatch_rejected`, до новых тестов. |
| `cargo build --release --manifest-path nova-cli/Cargo.toml` | Успех; предупреждения существующие, не подавлялись. |
| `nova test spec_tests --filter p1870` | `PASS: 5  FAIL: 0` |
| `nova test spec_tests --filter handler --full` | `PASS: 22  FAIL: 0` |
| `nova test spec_tests --filter effect --full` | `PASS: 27  FAIL: 0` |
| `nova lint --deny spec_tests/conformance/standalone/p1870*.nv` | `lint: 5 file(s), 0 finding(s), 0 denied (--deny, exit 1)` — это буквальная строка CLI; фактический exit=0. |

Подкаталогов handlers/effects в данном дереве spec_tests нет: использованы
целевые фильтры `handler`/`effect`, с `--full`, чтобы не пропустить негативы.

## std/src

До фикса (снятый фикс, новый бинарь), ANSI-цвета убраны, текст дословно:

```text
PASS: 158  FAIL: 26  WARN: 67
```

26 отказов — существующие негативные пробы std, не 26 новых регрессий.
После восстановления сравнивается та же команда `nova check std/src --format short`.

После фикса, дословно (совпадает с ДО):

```text
PASS: 158  FAIL: 26  WARN: 67
```

## Приёмочные шаги проекта

- **criteria:** пять критериев дословно выше; запуск и поиск класса выполнены,
  окончательный CI/land-task принадлежит приёмщику.
- **fixture:** пять фикстур действительно зовут проверяемые операции и
  утверждают payload, unit-вариант, значения параметров и общий счётчик.
- **sabotage:** четыре наблюдаемых провала и один провал Rust-test при
  снятии фикса, повторный зелёный после восстановления; оба вывода выше.
- **class:** три прохода используют одну дверь scope, остальные пути
  перечислены; инвентарь — приёмка глазами по причине, названной выше.
- **registry:** №1870 добавлен этим же коммитом (в этой базе строки #50 нет),
  при слиянии с #50 оставить одну строку. Новых E_*/W_* нет. База rows/max
  поднята по факту добавления строки. При синхронизации с origin/main
  `e4436993a` итоговое множество пересчитано: rows=1779, max=1871, дублей 0.
  №1869 и №1870 сняты с gaps; заполненные номера 1858/1859/1865 также не
  восстановлены как дыры. №1868 (#50, ConPTY) остаётся временной дырой до
  приезда её строки; назначения подтверждены интегратором.
- **spec:** не применимо — язык не меняется, исправлено окружение типов.
- **std-check:** обе строки PASS/FAIL/WARN выше, без новой дельты.
- **invariant:** локалы операции живут только в её scope; без него тип
  параметра зависит от порядка операций. Новый страж не добавлялся;
  воспроизводимый инвариант держат фикстуры и модульные тесты.
- **gate/ci:** НЕ заявлены локально; финальное доказательство — CI кандидата
  и строка `LANDED task=#53 main=…` приёмщика.
- **commits:** один commit на английском, DCO, явный список файлов.
- **report:** РЕПРО, ТОЧКА, ФИКС, ФИКСТУРЫ, САБОТАЖ, std/src здесь;
  ВЕТКА/КОММИТ и ЧТО НЕ СДЕЛАНО — в итоговом письме интегратору.

## ЧТО НЕ СДЕЛАНО

- `std/src/os`, shell.tpl.c и PTY/proc-код не менялись; обход переименованием
  параметра не снимался (границы #44/#50).
- Карина не исправлялась; новый E_*/W_* и синтаксис не вводились.
- Мега-CU, полный nova test и локальные gate.sh/gate-novac.sh не запускались.
- CI/land-task и итоговый LANDED выполняет приёмщик на кандидате integrate/t53.
- Хеш единственного коммита передаётся интегратору вместе с отчётом; реестр
  ссылается на эту задачу и функцию фикса, чтобы не вписывать хеш в себя.

## Доработка после CI (круг 1)

CI кандидата `nova-gate` run 37872268078 остановился на двух текстовых
требованиях, не на поведении фикса:

1. Комментарий `must never retype` теперь помечен `[INV-PROPERTY]`:
   новая карта области операции не может содержать локалы соседней операции.
   Рядом названы существующий тест
   `handler_scope_tests::annotator_does_not_sum_lift_sibling_parameter_or_outer_capture`
   и `standalone/p1870_handler_op_local_leak_pos.nv`; их краснота при снятии
   механизма и зелёный повтор приведены выше. Формулировка инварианта сохранена.
2. Три архивные улики переименованы в `.nv.txt`, ссылки и команда
   воспроизведения обновлены (копирование в scratchpad). Пять живых фикстур
   в spec_tests не менялись. База `repro-evidence-suffix` не ослаблялась.

Проверки отдельно, без запуска локальных гейтов:

```sh
bash scripts/guards/check-invariant-discipline.sh . origin/main
bash scripts/guards/check-repro-evidence-suffix.sh .
```

Страж инвариантов проверяет также уже закоммиченные добавления относительно
базы; поэтому исправление его вывода подтверждается после коммита, не только
по незакоммиченному diff. CI/land-task после синхронизации — за приёмщиком.

## Синхронизация после #44

Влита `origin/main ef30238a36d2552a01bba12f81ccbaaa3449e9e7`. Правка №1874
в `record_bare_variant_ctor` и emit_c приехала из main без конфликта с
областями операций №1870. №1870 осталась одной строкой; baseline пересчитан
по итоговому множеству: rows=1783, max=1874, дублей 0. Заполненные номера
1862/1863/1866 не восстановлены как gaps; 1870 также не gap. Сохранены ещё
отсутствующие в main 1868 и 1872–1873. Точечные проверки после новой сборки
передаются приёмщику с полным SHA; предыдущий прогон не выдаётся за этот.
