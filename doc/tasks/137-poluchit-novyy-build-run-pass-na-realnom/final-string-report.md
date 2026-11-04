# 137 «Получить новый build+run PASS на реальном примере Carina 0.3»

## Результат

**Неизменный `examples/basics/string_literals.nv`: baseline build FAIL → локальный Carina-only build+run PASS с точным stdout. Подтверждённый прирост реальных входов: +1.** Исправлены текстовые backtick/tag без живых слотов, не весь D467/D48. CI/финальная приёмка ещё не объявлены.

Последний измеренный кандидат: [evidence/final-string-v4](evidence/final-string-v4/). Это отдельный локальный непубликуемый кандидат поверх дерева 135. Поставляемый сид, `SEED_*`, assets, теги и main не менялись. Прежние попытки и затраты сохранены в report.md/alternatives.md/string-checkpoint.md, не обнулены.

## РЕПРО

Исходный RED: `BUILD rc=1`, 19 parser fallback на том же реальном входе (135 L048 и scratch `alternatives/strings-build.*`). Весь корпус 135 не повторялся.

```text
python doc/tasks/137-poluchit-novyy-build-run-pass-na-realnom/build-string-candidate.py <scratch> <task135-preserved> green
python doc/tasks/137-poluchit-novyy-build-run-pass-na-realnom/check-string-candidate.py <scratch> <scratch>/novac137.exe <fresh-output-name>
```

Builder использует проверенный combined seed 135 и его локальный patched driver v2. На сохранённое дерево накладывается только адресный diff задачи, `ice_at`-координаты приводятся к локальным строкам. Копирование целых новых файлов main, возвращавшее чужие bootstrap-изменения, исключено. Полное дерево после backport захешировано; Oracle не используется.

Для входа кандидат выпускает C, clang компилирует его с теми же архивами runtime/GC/libuv, полученный exe запускается, весь stdout сверяется. Native-команда — `string-final-green-v4/real-native-command.json` в evidence; команда сборки компилятора — `candidate-build-command-green.json`. Это **source→C→exe→run**, не только emit.

```text
real-emit rc=0
real-native rc=0
real-run rc=0
```

Точный stdout (raw-файл сохраняет реальные окончания строк, сравнение нормализует LF/CRLF вывода):

```text
hello Nova
one long line, written across two source lines
first
second
third
a tagged literal
cost: $ and more
shell: $HOME plain
C:\new\table
tick ` slash \ dollar ${name}
```

## ТОЧКА / ФИКС

- `lex/lex.nv`: lossless backtick-токенизация с тремя escape; незакрытый литерал не становится StrLit.
- `sem/backtick.nv`: D467 CRLF/отступ/закрывающая строка, текстовые escape, сохранение прочих пар. UTF-8 копируется участками, не однобайтовыми строковыми срезами.
- `literal_spelling.nv` / `emit_interp.nv`: полный токен отличён от фрагмента обычной интерполяции. Backtick в начале фрагмента обычной строки не выбирает raw-алфавит.
- `parse/expr.nv`: text tag — обычный Call, parts из одного сегмента, неявный пустой args; исходные токены сохранены по одному разу.
- `check/tagged_args.nv`, `calls.nv`, `binds.nv`: тип пустого args берётся из обычного конкретного vector-параметра, без дефолта `[]str`. Shell-отказ переиспользует `no_shell_vec_msg`.
- `check/strings.nv`, `literal_rules.nv`: живые слоты — адресный отказ до эмиссии, escaped `${...}` — буквальный текст.
- `emit_requires.nv`: исходный display assert/requires экранирует также LF/CR/TAB (дополнительный носитель №872).

Норма прочитана через read-spec и сверена: D467 §3, §5–7, D48. Язык не меняется, новый D-блок не применим. Остатки оформлены по решению интегратора как открытые `274.14 F1/F2`, их реализация в задачу не включалась.

## ФИКСТУРЫ / КЛАСС

`spec_tests/conformance/standalone/carina_text_backtick_tag.nv` проверяет точные значения и вызывает tag-функции: пустой текст, Windows path, три escape, literal `${}`, многострочность/отступ, UTF-8, обычную интерполяцию с backtick-фрагментами, multiline assert/requires, пустые `[]str`/`[]int`, один вызов каждого tag и порядок stdout.

На **одном последнем зелёном бинаре**:

| Проверка | Результат |
|---|---|
| Исходный string_literals | build+run PASS, весь stdout |
| Фикстура LF и её CRLF-копия | обе build+run PASS, `tag:hello\nint:world\nbacktick-ok\n` |
| Прежний str_eq | build+run PASS, `true\nfalse\ntrue\n` |
| Прежний strings | build+run PASS, четыре строки совпали целиком |
| Прежний requires_string_guard | build+run PASS, `1\n` |
| Прежний interp_display | build+run PASS, `p Pt end\nPtPt\n` |

Дополнительно машинно сопоставлены SHA генерированного C этих четырёх входов с сохранёнными логами 135 (L047, L049, L043, L020): **все четыре IDENTICAL**. Это сравнение существующих улик, без повторного запуска baseline.

Шесть отрицательных проб: live slot, tagged live slot, unclosed backtick, escaped closing delimiter, generic empty args, overloaded empty args. Проверяются rc=1, код, сообщение и точный start на **записанных байтах** файла (CRLF учтён). Live slots/generic tags законны в Nova, поэтому отказ проверяется как граница кандидата адресным runner, а не ложными общими language-negative фикстурами.

`final-refusal-comparison/before-after.json`: число диагностик соответственно **4→1, 4→1, 1→1, 3→1, 2→1, 2→1**, все шесть входов отказывали прежде. Probe `args []f32` оказался emit PASS и в негативы не засчитан.

Поиск backtick в таблице 118 нашёл ещё orm_decorators и orm_demo. Адресный emit: rc=1, 13 и 85 диагностик; первые препятствия — handler-value и derive/Deserialize. Нового PASS у них нет, живые слоты не объявлены поддержанными. Полные 118 не повторялись: +1 подтверждён, общий итог корпуса не пересчитан.

## САБОТАЖ

В отдельной локальной копии исходников правило LF в `backtick_c_text` заменено на TAB; собран отдельный Carina-only compiler-sabotage. Исходный пример и фикстуры не менялись.

Дословно RED:

```text
real-run rc=0
real BUILD+RUN FAIL stdout='hello Nova\none long line, written across two source lines\nfirst\tsecond\tthird\na tagged literal\ncost: $ and more\nshell: $HOME plain\nC:\\new\\table\ntick ` slash \\ dollar ${name}\n'
fixture-run rc=101
fixture BUILD+RUN FAIL stdout=''
fixture-crlf-run rc=101
fixture-crlf BUILD+RUN FAIL stdout=''
```

После восстановления и пересборки все семь native-проверок GREEN, шесть отказов PASS. Полные логи: `verify-run-sabotage-v4.log`, `verify-run-green-v4.log`. Четыре прежних PASS остаются зелёными также на sabotage-бинаре.

Дополнительный RED разработки: смешение целого backtick и обычного строкового фрагмента давало clang `expected '}'` на новой проверке ``"`head${n}`tail`"``; на последнем коде она проходит. Исправлена общая дверь, фикстура не ослаблена.

## PROVENANCE

Полные manifests и команды — в final evidence. Хеши относятся к **сырым** байтам:

| Артефакт | SHA256 |
|---|---|
| Неизменный реальный `.nv` (рабочая копия) | `2fc215846972f602772e95ab7aa60b863fdb63b425bd395ee19ecb2959fdd1fe` |
| C компилятора | `902436b8bcc8ae217bc2eb21d25e842214eb67def1f545bec075f06e806fcfd2` |
| Компилятор exe | `abc58e46628439a79b68606821f905856b2dec30ebccb1b33f09d5536e22d58a` |
| C реального примера | `3d627c775faa52a2c849b60eff0ac2aa9519a1ea42428cf3a81f6fae48473ac9` |
| Exe реального примера | `d45c9fe4c0a38198a344ca99537227344475373607d27b2bae66e66ccf131a6b` |

Бинарники/C/сырые логи сохранены в scratch; compiler и sabotage сохранены под отдельными именами. Публикуемые улики обезличены: только prefixes `$WT/$SCRATCH/$HOME/$CLANG`, JSON пересериализован. `normalization.json` содержит raw/published hashes; source/C/exe SHA не пересчитываются по обезличенному тексту. Полные std-логи — в scratch; опубликованы команды, сводка с хешем полного сравнения и свидетелями новых сообщений.

## std/src

Сохранённый seed CLI не принимает каталог: до и после `check <std/src>` — `rc=2`, `novac: cannot read ...std\src`. Это не проверка std и не PASS. Выполнен поддерживаемый batch по всем **333 `.nv`**, включая тесты, без Oracle:

```text
std before FAIL rc=1 diagnostics=5064
std after FAIL rc=1 diagnostics=5033
std diagnostic delta: added=6 removed=37
```

Все шесть добавленных сообщений — F1 в data/sql_test.nv; у каждого есть прежний fallback **на том же source start**. Свидетели в `std-batch-comparison/summary.json`. Общий std остаётся вне подмножества, зелёным не объявлен.

## Инвариант / реестр / база / CI

Новый структурный случай: leafless ArrayLit только для неявного пустого args тега; явный `[]` содержит токен. Это позволяет переиспользовать обычный вызов без выдуманных токенов и второй системы вызовов. Lossless проверен native probe на четырёх входах: `roundtrip RUN rc=0 stdout=b'roundtrip-ok\r\n'`. Двери lexer/parser/tree после этого прогона не менялись.

№1948 и №1950 остаются OPEN; №1089 не исправлялся. Backtick/callback не продублированы, display-носитель связан с №872. Новых E_*/W_* нет. Измерения собственного анализатора стража: refusals **145→147**, undated **52→52**, undated_by_door **67→67**. Два текста заменяют fallback. **Пересмотр базы утверждён интегратором 2026-11-02** (ответ на qmvyb1r6yeyed), записан с летописью и ссылкой на измерения; `check-novac-subset-debt-dated` — PASS, оба бессрочных счётчика сохранены.

В ветку включён main `a4d03b307be91b4bb220bc66200dfa333275bf17` с 136 (merge `e909cb960`). Main не двигался; полные локальные гейты не запускались. CI/integrate/t137 и LANDED пока отсутствуют.

## Критерии / ЧТО ОСТАЛОСЬ

Дословно: «Существующий вход с baseline FAIL становится build+run PASS с ожидаемым поведением; минимальный регрессионный тест и саботаж RED/GREEN; provenance бинаря из окончательного кода; результаты других носителей и сохранность прежних PASS; если не достигнуто — честная незавершённость, смена текста диагностики не приёмка».

Локальные содержательные критерии выполнены в text-only классе. Приёмка глазами, потому что граница подмножества, std-delta и достаточность адресного набора требуют независимого суждения. Остаются CI и штатная приёмка; без них задача не объявляется принятой. Учёт времени — `[M-137-time-budget-accounting]`, harness не менялся.

## ВЕТКА/КОММИТ

Ветка `task/137-poluchit-novyy-build-run-pass-na-realnom`. Полный SHA коммита передаётся со штатной сдачей; проверенный интеграционный SHA фиксируется приёмщиком в precheck. Сборочные source/C/exe SHA выше относятся к последним исходникам реализации, не к прежнему WIP-кандидату.
