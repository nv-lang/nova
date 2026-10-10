<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# #55 «Карина №1875: хвостовые значения блоков match и полная самосборка»

Статус: ремонт прошёл штатную локальную A→B→C, targeted проверки, саботаж
сравнений, CI-discovery и причинную пробу одним диагностическим бинарём.
Приёмочный CI ещё не выполнен. №1875 открыта до приёмки, 0.2 не объявлена.

## РЕПРО

Исходная task-ветка `t55-karina-1875-hvostovye-znacheniya-blokov` оказалась
на локальной голове владельца; её точный идентификатор и роль сохранены в
[журнале исторического provenance](task55-historical-provenance.json).
Она оставлена нетронутой. В том же дереве создана `t55-fix-1875` от свежего
`origin/main` [`6a3275d0add28a9fe207fac9464baa3380f97f8d`](https://github.com/nv-lang/nova/commit/6a3275d0add28a9fe207fac9464baa3380f97f8d) — «chore(crew): release accepted task slots», 2026-10-09. Обычный cherry-pick
диагностической улики #54 дал commit
[`81ca672d17d61ddda08b302561275d1f9079d52c`](https://github.com/nv-lang/nova/commit/81ca672d17d61ddda08b302561275d1f9079d52c) — «docs(novac): localize bootstrap record mangling failure», 2026-10-10;
исходный private commit расследования указан в provenance-файле. Дерево
расследования и улики #54 не изменялись.

Дерево задачи:
`D:/Sources/nv-lang/worktrees/nova-opencode-55-karina-1875-hvostovye-znacheniya-blokov`.
Временное и полные логи — собственный scratchpad
`C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv2e4l8pqx6d4rj6mu`,
его проверенный 8.3 alias `C:/Users/B7E3~1/AppData/Local/Temp/opencode/SE78CE~1`.
Далее `S` обозначает этот каталог.

До создания junction оба пути отсутствовали. `target` указывает на `S/target`,
`nova-cli/target` — на `S/cargo`; `CARGO_TARGET_DIR=S/cargo`,
`TMPDIR=TMP=TEMP=S/tmp`. Rust-оракул собран из исходников своего дерева.
Штатная дверь поиска оракула видит именно эту приватную сборку.
GC заимствован штатной `novac_borrow_main_gc` из главной копии (библиотека и
заголовки); исходники главной копии не использованы как база задачи.

Первый watch `1791636661781-kca28e` построил оракул, но сборку Карины остановил
clang: `unable to make temporary file: no such file or directory`.
Лог `S/novac-before-build.log` сохранён. Повтор через ASCII alias того же scratch,
watch `1791637119492-41ezqr`, построил приватную Карину до фикса.
Первый неудачный smoke без бинаря (rc127/rc1) не считается доказательством класса.

Дословный поведенческий замер до фикса (`S/probes-before-retry.log`):

```text
novac-e1-smoke: FAIL — stdout расходится (< оракул, > novac): 1c1
< p1875 if 7 -3 value heap record_value record_heap
---
> p1875 if 0 0
rc=1
```

У строки после `>` ещё четыре конечных пробела: её точное представление JSON —
`"> p1875 if 0 0    "`. Сырые логи до/саботажа и измеренный source patch
сохранены lossless в [task55-lossless-evidence.json](task55-lossless-evidence.json):
`text_utf8.encode("utf-8")` восстанавливает исходные байты, размеры и SHA256
записаны рядом; roundtrip всех вложений проверен (включены полный
original-family, след lower и итог Г17). Это сохраняет значимые
пробелы и переводы строк без исключения из whitespace-проверки репозитория.

Оба отрицательных файла `p1875_match_block_{tail,iflet}_type_neg.nv`:
оракул rc1 с E7301 на строке 10, Карина до фикса rc0 с пустым выводом.

## ТОЧКА И ФИКС

`check/match_arms.nv`, `type_arm_block_value`: прежнее условие
`i == last && is_expr_kind(c.kind_of())` не признавало `IfStmt`, `IfLet`,
`TupleExpr` хвостовыми значениями. `lower/lower_match.nv` правильно читает
отсутствие записанного типа блока, но вследствие неверного ответа checker
вызывает statement-lowering и оставляет нулевой match-local.

`type_arm_block_value` теперь вызывает общий с `type_branch_value` обход
`type_block_value`. Ожидаемый тип получает только последний branch-узел блока;
предшествующие инструкции проверяются без ожидания результата блока.
Общий `type_block_value_tail` выбирает существующие value-двери для обычного
и паттернового `if`, обычных выражений, tuple и nested match. После внутреннего
отказа, включая выход литерала за диапазон, запрос типа не производится.
Обязательный хвост ветви `if` и необязательный хвост match-arm различаются
явным параметром `required`; условие без `else` в необязательном плече остаётся
инструкцией. Выходы через `return`/управление циклом судятся существующим
termination-путём. `lower` получает тип из прежнего канала; `lower_else_value`
принимает также цепочку `else if` с паттерном и передаёт её общей `lower_place`.
Mangle не изменён.

Причинный след в настоящем B подтверждён чтением замороженного B.c:
`instance_arg_spelling` начинается на строке 137979; в TkRecord обе строки
назначаются `_novac_l4` (137995, 138002), затем `_novac_l1 = _novac_l4`
(138004), вместо прежних отброшенных `(void)`. Соседний TkApp также передаёт
хвост через `_novac_l1 = _novac_l5` (138018). Это два носителя общей двери,
не специальные изменения record/Vec mangle; полный C затем скомпилирован и
побайтно совпал с B.c.

Норма: D23 (`spec/decisions/03-syntax.md:695`, цитата лично сверена), D34/D40,
D478/D489, D275. Изменение языка/D-блок: **не применимо**, восстанавливается
существующее значение хвоста. Новый архитектурный инвариант не вводится:
прежний «checker решает, lower читает канал» остаётся тем же; устранены разные
списки допустимых хвостов у двух проверяющих обходов.

## ФИКСТУРЫ И КЛАСС

- `standalone/p1875_match_block_if.nv` — прямой носитель silent-wrong-value:
  обе ветви, int, str, интерполяция. После первого фикса e1-smoke: stdout
  байт-в-байт с оракулом, exit0 (`S/after.log`, watch `1791637537754-ain1hg`).
- `standalone/p1875_match_block_tails.nv` — expression-arm/plain-block controls,
  nested if/match, IfLet в плече и в ветви if, tuple, ранние возвраты, числовое
  ожидание u8, statement-позиции; test-блоки вызывают функции и проверяют значения.
- `neg/p1875_match_block_tail_type_neg.nv` и
  `neg/p1875_match_block_iflet_type_neg.nv` — line-pinned E7301 оракула;
  Карина после фикса rc1 с существующими сообщениями E_NOVAC_SUBSET о
  несовместимых типах. Новый код диагностики не введён.
- `standalone/p1875_match_block_branch_exits.nv` — цепочка обычного и
  паттернового условия, вложенный if с двумя return, соседняя value-ветвь.
  После checker-правки, до сопряжённой правки lower: оба check rc0, но
  `novac emit` дал `internal compiler error at novac/src/lower/lower_branch.nv:30
  (novac bug, not yours): lower: the else of a value if is neither a block nor
  an else if -- check refuses that`. Точная строка с backticks и rc1 сохранена
  в `S/chains-before-lower.log`, watch `1791638195756-zfuvmx`.
  Lower-диспетчер теперь пропускает IfLet к той же `lower_place`, что обычный if.
- `standalone/p1875_match_block_expected.nv` — ожидание u8 из аннотированного
  биндинга, аргумента и поля записи, Option[int] с голым None.
- `neg/p1875_match_block_tail_range_neg.nv` — хвост 256 в ожидаемом u8.

Проверены существующие решения `type_branch_value`, `type_if_value`,
`type_if_let`, `lower_block_stmts_value`, `lower_block_value`, `tail_places_value`
и входы function-tail/arm-expression. Два checker-обхода сведены к одному;
понижение уже умеет размещать эти значения и не требует нового вывода типов.

### Независимые находки оракула

1. `oracle-tuple-tail-type-gap.nv`: объявленный возврат `(int, str)`, хвост
   `(1, 2)` в одном плече и `(3, "other")` в другом принимаются `nova check`
   с rc0. Это носитель уже открытой №1014 («элемент кортежа в объявленном
   возврате»), не новый номер. Проба сохранена как диагностическая, а не как
   якобы зелёный negative. Первичный лог `S/probes-before.log:40–45`.
2. №1876, `oracle-match-loop-exits.nv.txt`: значение match с break/continue-плечами
   становится `nova_unit` в C оракула. Полная первая семейная проба имела
   `nova check` PASS, сборка и targeted test дали:

   ```text
   error: invalid operands to binary expression ('nova_int' (aka 'long long') and 'nova_unit')
    8280 |         out += n;
   ```

   Полный лог `S/p1875_match_block_tails-oracle-build.log`, сохранённый C
   `S/tmp/nova_tests-38116/build-47ce0ace9d8b/p1875_match_block_tails.c`.
   Это ошибка сгенерированного C после успешного check, не ошибка среды и не
   доказательство неправильного значения у исправленной Карины.

Интегратор разрешил разделить новую, ещё не зафиксированную диагностическую
пробу: существующие тесты не менялись. Полный исходник **до разделения**, включая
`loop_exits` и `assert(loop_exits() == 43)`, сохранён без изменения как
`tail-family-original.nv.txt` и `S/tail-family-original.nv`. SHA256 исходного
файла и обеих копий:
`04c69b724f0fb35e54bc9df0f0d24d6a90b9ea09b7fceae970adec048fb7a334`.
Дословный clang-лог приложен как `tail-family-oracle-cc-fail.log`.
Независимые формы остаются в conformance со всеми своими assertions;
main вызывает проверяющие функции, поэтому assertions исполняются и при
`nova build`/прямой сборке Кариной (одни test-блоки рядом с main этого не гарантируют).
Оригинальный loop-carrier проверяется отдельно Кариной по точному ожидаемому
выводу; сравнение с оракулом на нём не заявляется.

Прямой прогон исходника с хешем `04c69b…` исправленной Кариной завершён
watch `1791638576913-app89y` за 5 с после очереди. Сырые C/obj/exe/stdout/stderr
лежат в `S/direct-attempt1/family-first/`, лог `S/direct-family-first.log`.
Все восемь ожидаемых строк совпали полностью, в том числе:

```text
p1875 option 17 -17 18 -18
p1875 tuple 21 pair 22 tuple
p1875 exits 31 32 33 34 35 36
p1875 statements 41 0 42 0 43
DIRECT PASS: exact observable output
```

№1876: оракульный маршрут 274.10, не доказанный блокер 0.2. Прямой счёт реестра
после добавления: rows1787, max1876, дублей0. По явному разрешению интегратора
изменены только соответствующие rows/max в `registry-rows.baseline`; gaps не менялись.

## САБОТАЖ

Допуск необязательного хвоста временно возвращён к прежнему
`is_expr_kind(c.kind_of())`: тело `block_tail_can_place_value` заменено этой
формулой. Фикстура прямого wrong-value при саботаже не менялась. Собран отдельный
`S/novac-sabotage.exe`; watch `1791637748791-aq6q4l` (31 с исполнения после
18 мин очереди). Полные лог и diff: `S/sabotage.log`, `S/sabotage-source.diff`.
Дословно:

```text
sabotage-build rc=0
novac-e1-smoke: FAIL — stdout расходится (< оракул, > novac): 1c1
< p1875 if 7 -3 value heap record_value record_heap
---
> p1875 if 0 0
SABOTAGE_BEHAVIOR_RC=1
SABOTAGE_NEGATIVE_RC=0
SABOTAGE detected: behavior mismatch and invalid tail accepted
```

Предикат восстановлен. Первый восстановленный прогон —
`S/validate.sh`, watch `1791639017815-98mbgb`, rc0 (16 мин исполнения).
Восстановленная зелёная сторона, дословно:

```text
novac-e1-smoke ok: spec_tests/conformance/standalone/p1875_match_block_if.nv — поведение идентично оракулу (stdout байт-в-байт, exit 0; оракул из кэша)
expected: ['p1875 if 7 -3 value heap record_value record_heap']
actual:   ['p1875 if 7 -3 value heap record_value record_heap']
DIRECT PASS: exact observable output
novac-negative rc=1
```

Полный сохранённый прогон — [tail-values-validation.txt](tail-values-validation.txt),
красная сторона — `tail-values-sabotage.txt` в lossless bundle выше;
четыре конечных пробела строки `>` представлены так же, как в разделе РЕПРО.
Приватные компиляторы этого прогона, SHA256:

- oracle: `28e49f0d39ac813e44813671e9d551ce56235e235885811bb7c903c98e1a9801`;
- novac-final (сохранён как `S/novac-final-attempt1.exe`):
  `bda7bee0a96a42362d3582c20df3d323bf514191d6841232aa9de083babb02e0`.

Команды и вердикты:

```text
nova test spec_tests/conformance --filter p1875 --full --keep-artifacts
PASS: 7  FAIL: 0
check-novac-diag-schema ok: фикстур 269, диагностика валидна (id,code,severity,primary,message)
check-novac-no-cascade ok: фикстур 269, на каждой ровно один диагностик severity=error
check-novac-file-size ok: файлов 214, все не длиннее 1000 строк
check-novac-line-length ok: строк .nv: 64283, длиннее 120 символов вне трёх исключений: 0
VALIDATE_FAIL=0
```

`--full` здесь выбирает все виды EXPECT у **семи отфильтрованных** файлов,
это не полный прогон nova test. Все четыре новых позитивных файла проверены
обоими check, e1-smoke и прямой сборкой/точным выводом Карины; три негатива
дали rc1 у обоих компиляторов. Дополнительно e1-smoke зелёный на неизменённых
`novac/fixtures/{tail_if_terminating,tail_if_let_value,tail_match_none,ctor_if_tail}/pos_1.nv`.
Полный исходный loop-carrier снова проверен novac-final: DIRECT PASS, включая 43.

Оговорка свежести: имя типа в новом expected-position позитиве изменено
`ByteBox` → `P1875ByteBox` после его первого direct-прогона для изоляции имени.
Ровно этот файл повторно проверен watch `1791640136683-v1fmre`:
оба check, e1-smoke, exact direct и targeted test зелёные (`PASS: 1  FAIL: 0`).
Исходники компилятора после сборки novac-final до этого прогона не менялись.
Тот же watch прямо собрал неизменный original-family **испорченной** Кариной:
`SABOTAGE_ORIGINAL_FAMILY_RC=2`, `E_NOVAC_ICE` с сообщением
`sem: no type recorded for this node — a hole in the checker's channel`.
Восстановленный original-family выше имеет exact DIRECT PASS, включая 43.

## std/src

Один свежий приватный Rust-оракул, `nova check std/src` до и после:

```text
PASS: 162  FAIL: 26  WARN: 67
PASS: 162  FAIL: 26  WARN: 67
```

Оба rc1. Сырые строки с ANSI сохранены в `S/std-before-retry.log` и
`S/std-after.log`, повтор — `S/std-final.log`; выше удалены только терминальные цвета. Это одинаковый
исход baseline, а не зелёная проверка std. Rust/std исходники не изменялись.

## ШТАТНАЯ A → B → C

Поставлена watch `1791639246507-0i5ky0`: неизменённый `scripts/tools/double-build.sh`,
без `DOUBLE_BUILD_A` и `GC_DONT_GC`, приватный fresh A; реальные B/C, relink,
сырые сравнения и красно-зелёная порча сравниваемого текста и бинаря.
Команда-обёртка `S/bootstrap.sh`, полный xtrace с argv и временем шагов —
`S/bootstrap.log`. Обёртка отказывается запускаться поверх уже существующего
`target/double-build`, чтобы штатное удаление W не уничтожило предыдущие улики.
Исходный diff сохраняется перед запуском как `S/bootstrap-source.diff`,
после завершения снимаются SHA256 всех артефактов.

Первая штатная попытка завершилась за 27 с после 13 мин очереди:

```text
S5 PRECONDITION NOT MET: A (novac by the oracle) check accepts only 212/214 of its own source; emit+build+compare not attempted
diagnostic HEAD + uncommitted novac/src
BOOTSTRAP_RC=1
```

Это **не успешная самосборка**: A есть, B/C и сравнения не запускались.
Полный HEAD [`81ca672d17d61ddda08b302561275d1f9079d52c`](https://github.com/nv-lang/nova/commit/81ca672d17d61ddda08b302561275d1f9079d52c) — «docs(novac): localize bootstrap record mangling failure», 2026-10-10 — только предок
незакоммиченного фикса. SHA256 измеренного source patch:
`df6e3e7a849a1605c4a97210a542dea4f783033666e30382fa5d42a2236af069`.
A.exe SHA256: `76a91b1f8550d832ea5b1b20de7463809f653300361e412f6c1ed4b1125a98f6`.
Неизменённая копия исходников novac, всех артефактов, trace, patch и хешей
заморожена в `S/bootstrap-attempt1/`. Последующий postcheck явно написал
`COMPARE SABOTAGE NOT RUN: baseline B/C files are absent or differ`, rc9;
это не засчитанный саботаж сравнения. Два self-check отказа диагностируются
на том же A/source с сохранением полного JSON и argv.

Повтор exact self-batch (`S/selfcheck-attempt1-command.json`, watch
`1791640483795-alkwm1`, 16 с) назвал оба отказа: новые вызовы
`@type_block_value(b, required: false/true)` использовали именованный аргумент
у параметра без default. Карина отказала по D102: `parameter required has no
default value, so it is passed positionally, not by name`; Rust-check эту
ошибку вызова не обнаружил. Оба вызова исправлены на позиционные `false/true`;
условие допуска хвоста и диагностики не ослаблены.

Повтор штатной цепочки: watch `1791640716115-c4dn8n`. Прежний W перенесён
в `S/bootstrap-attempt1/original-artifacts`, поэтому новая попытка не затирает
старую. Прямые артефакты первого validation также перенесены в
`S/direct-attempt1`; окончательная повторная проверка нового исходника —
watch `1791640791408-xfbzb2`, `S/validate2.log`.

Этот окончательный прогон завершён rc0 (14 мин); полный лог
[tail-values-validation-final.txt](tail-values-validation-final.txt).
Повторены fresh private build, 7/7 targeted tests, check/e1/direct четырёх
positive, отказы трёх negative, четыре прежние tail-регрессии, прямой original-family
и оба diagnostic guards на 269 файлах. `std/src` снова 162/26/67.
Отдельные проверки оформления/статуса/маршрута реестра —
[task55-registry-checks.txt](task55-registry-checks.txt), rc0.

### Успешная штатная попытка

Watch `1791640716115-c4dn8n`, rc0, около 2 мин. A заново построена оракулом
за 26 с, проверяет 214/214 собственных файлов; A emit B — 25 с, B emit C — 12 с.
Это полные настоящие B/C executable, не переименованные A и не частичный линк.
Штатный скрипт неизменён, `DOUBLE_BUILD_A` и `GC_DONT_GC` сняты из окружения.

```text
B ≡ C: B/novac.exe and C/novac.exe byte-identical (3877888 bytes), B.c and C.c byte-identical (13250302 bytes)
BOOTSTRAP_RC=0
```

Три полные строки IDENTICAL — [task55-bootstrap-summary.txt](task55-bootstrap-summary.txt):
повторный link B.o, сырые B.c/C.c и executable B/C. Никакой нормализации C
или бинарей для сравнения не было. Артефакты:

| Артефакт | Байты | SHA256 |
|---|---:|---|
| A.exe | 4387840 | `b2c14c1a4d9bc7133626fa3a926381111fadaa89361249b5e095ecce5976e005` |
| B.c и C.c | 13250302 | `8ea9b067c5ba8ebaa47828652d05d616bfddc8b68c3976b28c4acf670b74e996` |
| B/novac.exe, B.relink/novac.exe, C/novac.exe | 3877888 | `9e87fdc2af7ff264ad8d67bd8d102f95221fe7bef9896eb1128eadbf478087d9` |

**Provenance:** HEAD [`81ca672d17d61ddda08b302561275d1f9079d52c`](https://github.com/nv-lang/nova/commit/81ca672d17d61ddda08b302561275d1f9079d52c) — «docs(novac): localize bootstrap record mangling failure», 2026-10-10 — **плюс dirty source**,
не SHA готового фикса. Полный измеренный patch —
`task55-measured-source.patch` в [lossless bundle](task55-lossless-evidence.json), SHA256
`afcbc3bde1882f97dfbd8828482d680080a4409453be7585d19c7e28bcbc5f61`.
Перед заморозкой текущий `git diff -- novac/src` побайтно сверен с этим patch.
Штатный `double-build.sh` SHA256:
`15c20ace42515a7619d1449fe8b3086cc2af0021b21967042c1578990dfee102`.

248 файлов заморожены в `S/bootstrap-success`: все исходники novac, приватный
oracle, A/B/C, raw C, obj, relink, argv/PCH, stdout/stderr/clang/link логи,
verdict, полный xtrace и source patch. Их размеры/хеши —
[task55-bootstrap-manifest.json](task55-bootstrap-manifest.json);
отдельная штатная выборка — [task55-bootstrap-artifacts.sha256](task55-bootstrap-artifacts.sha256).
Исходные файлы в `target/double-build` также сохранены.

Саботаж сравнения завершён отдельным watch `1791640939611-dgy64i`, rc0, 3 с,
на копиях C.c и C executable: та же штатная `double-build.sh --compare`.
Текст, байт 6625152: DIFFER/rc1 → восстановление IDENTICAL/rc0.
Executable, байт 1938945: DIFFER/rc1 → восстановление IDENTICAL/rc0.
Оригинальные SHA256 проверены неизменными. Полные строки с именами/размерами/
позициями: [task55-compare-sabotage.txt](task55-compare-sabotage.txt).

```text
SABOTAGE c-text rc=1
RESTORED c-text rc=0
SABOTAGE c-exe rc=1
RESTORED c-exe rc=0
COMPARE SABOTAGE PASS: both red and green; original artifacts unchanged
```

**Это ещё не ступень 0.2:** по поручению интегратора её окончательное подтверждение
после #55/#57 требует fresh bootstrap и full CI на одном FINAL clean SHA.
Текущий диагностический dirty-source замер не переносится на будущий merge молча.

## Штатное CI-подключение conformance

Интегратор расширил scope #55 на discovery существующего
`scripts/guards/check-novac-differential.sh`, один manifest, его связанный selftest
и эту узкую документацию. Новый `scripts/guards/novac-conformance.list` —
единственный список дополнительных canonical relative paths: четыре positive,
три negative; сами программы остаются только в conformance.
По умолчанию эти файлы **добавляются** к прежним `novac/fixtures/**/pos_*.nv`.
Старые allow, корпус, бюджет и храповик не изменены. Этап 1 сравнивает исходы
обоих компиляторов; этап 2 использует прежний e1 для принятых positive.
Зарегистрированный `EXPECT_COMPILE_ERROR` не собирается/не запускается даже
при ошибочном принятии обоими check (D89); собственный oracle CI судит EXPECT.
Равенство кодов/позиций диагностик differential не обещает — его прежняя граница.

Manifest обязателен. Пустой дополнительный manifest сохраняет старый набор
в default-режиме (контроль Г6); `--conformance-only` с пустым manifest красный,
потому что итоговая выборка пуста. Комментарии `#` и пустые строки допускаются;
в записи нет whitespace, backslash, пустых/`.`/`..` сегментов; она начинается
`spec_tests/conformance/`, заканчивается `.nv`, разрешается в файл внутри root.
Missing/invalid/duplicate записи краснят настоящий discovery. Проверка одним
Python-проходом, без нового текстового процесса на каждую запись (Г2).
Классификация negative берёт маркер из первых 30 строк, как конвенция EXPECT;
пул читает готовое множество имён средствами shell.

Локальная команда:

```sh
NOVAC_BIN="$S/novac-final.exe" NOVAC_SMOKE_CACHE="$S/wiring-cache" \
  bash scripts/guards/check-novac-differential.sh . "$S/novac-final.exe" --conformance-only
```

Третий CLI-аргумент исключает legacy и корпус, явно печатает
`scope=registered-conformance`, `SAMPLE` и `legacy fixtures and corpus NOT checked`.
Это не переменная окружения: gate (его `par_add`/`par_run`) этот аргумент не
передаёт; выборка не может незаметно попасть в полный ярус через окружение.
Неизвестный scope-аргумент отвергается.

**Новый проверочный инвариант:** явно зарегистрированный вход существует,
каноничен и не посчитан дважды. Без него удаление/ошибка пути превращает
регрессию в незапущенный тест при зелёном старом наборе. Обойтись без manifest
можно было бы только дублированием тестов в legacy-корпусе или запуском всей
conformance неподдерживаемым подмножеством; оба варианта не нужны для этой задачи.
Это усиление discovery, не новая языковая норма и не второй дифф-раннер.

Связанный `selftest/test-check-novac-differential.sh` сохраняет прежние проверки
(baseline до правки: 26 ok/0 FAIL). В fake-root добавлен зарегистрированный
negative-контроль, поэтому pool-сводка теперь считает 8 вместо 7, с прежними
6 runtime-совпадениями и дополнительным outcome-only negative. Новые проверки
меняют вход одного и того же guard: missing/empty/invalid/duplicate manifest,
исход negative, неверный ответ positive и восстановления; default реально
запускает и legacy, и registered positive. Прогоны настоящих saved bad/fixed
компиляторов через **новый discovery** отдельно показывают registered coverage;
они не объявлены самостоятельным доказательством Г17 только по разным бинарям.

### Результат discovery и Г6

Итоговый связанный selftest: **51 ok, 0 FAIL**
([лог](task55-discovery-selftest.txt)). Прежние corpus-selftest **71/71**
([лог](task55-corpus-selftest.txt)) и binary-guards **14/14**
([лог](task55-binary-selftest.txt)) также прошли. В этих двух fake-root добавлены
только обязательный manifest и согласованный positive, assertions не менялись.
Первичный wiring watch `1791644781085-5dkp4z`: saved bad compiler принял все
три negative вместо отказа, guard rc1; fixed compiler — rc0, 7 совпавших исходов,
4/7 e1-совпадений и 3 поимённо названных outcome-only negative. Это покрытие,
а причинная проба компилятора — следующий раздел.

Г6, watch `1791646586848-zwa97j`, rc0: старая HEAD-версия guard и новая версия
запущены с **одинаковым пустым extra manifest** на четырёх реальных legacy
tail-контролях, чьи копии побайтно сверены с живыми файлами. Оба исполнили
все четыре e1. После отдельного прогрева кэша stdout совпал байт-в-байт,
кроме измеренных секунд стены; stderr совпал полностью. Сырые стороны
[old](task55-g6-old.txt)/[new](task55-g6-new.txt),
[все argv/хеши/списки](task55-g6-evidence.json),
[итог](task55-g6-summary.txt).

Полный живой legacy-список также измерен: **278 прежних путей без изменения**,
manifest добавляет **7 непересекающихся путей**, итог **285**; из новых 3 —
negative без поведения. Это предусмотренная delta discovery. Сквозное исполнение
здесь — четыре названных legacy-контроля и registered seven; весь широкий корпус
локально не запускался и остаётся за CI. Allow/corpus/budget не менялись.

### Г17: один диагностический бинарь

Watch `1791646280577-n09gyv`, rc0. В **копию** замороженного B.c добавлены только
четыре строки внутри `block_tail_can_place_value`: при наличии test-only
`NOVA_TEST_1875_OLD_TAIL` вернуть прежний `is_expr_kind(k)`, иначе выполнить
новый допуск. Побайтовая проверка удаления этой вставки восстановила исходный B.c
целиком. Nova-исходники, штатные A/B/C и release API не менялись.
Копия собрана с сохранёнными argv/PCH штатной цепочки, один раз; затем между
старым и новым режимом менялось только окружение запуска **того же файла**.

- Diagnostic C SHA256: `cdacc1f6bd1267528490700e27b269ae3db40eeed3fc3c216125e95b7ea94284`.
- Единственный diagnostic executable SHA256:
  `1b7ffda1286ecd5f23fa77175c2c89c50da11ece16132d521fec67b9c0e8defc`;
  проверен неизменным после обеих сторон.
- Артефакты, точная вставка, команды и raw stdout: `S/causal-c/`;
  полный лог `task55-g17-proof.log` в lossless bundle.

```text
G17 diagnostic C: only the four-line admission switch added; all other bytes identical
SAME_BINARY_OLD_DISCOVERY_RC=1
SAME_BINARY_OLD_BEHAVIOR_RC=1
SAME_BINARY_RESTORED_DISCOVERY_RC=0
SAME_BINARY_RESTORED_BEHAVIOR_RC=0
G17 SAME BINARY PASS: only old/new admission selected; production sources and frozen ABC untouched
```

Старый режим через новый discovery даёт три negative-расхождения; прямой e1
даёт прежние `0 0` и пустые строки. Новый режим того же бинаря проходит все
семь исходов и четыре e1, затем отдельно core e1. Полный восстановленный
discovery — [task55-wiring-restored.txt](task55-wiring-restored.txt).
Это причинная проба допуска хвоста, **не** дополнительный bootstrap compiler.

## Критерии приёмки задачи

| Критерий | Доказательство / граница |
|---|---|
| criteria: критерии приёмки задачи названы дословно и выполнены | «хвостовые значения блоков match и полная самосборка»: общий checker/lower, семейство и отказы, старое условие краснит; fresh A→B→C/relink/raw IDENTICAL и саботаж сравнения доказаны; CI остаётся приёмщику. |
| fixture: фикстура утверждает наблюдаемое поведение и зовёт проверяемое | 4 positive с точным stdout и исполняемыми assertions, 3 line-pinned negative; unit-проверки вызываются из main, не только объявлены. |
| sabotage: фикс сломан — красный вывод, возвращён — зелёный | Дословные красный/зелёный выводы выше и приложенные логи; full original также красный/зелёный. |
| class: фикс класса, а не носителя | Единый обход block-tail для match/if, IfStmt/IfLet/tuple/match, ожидание типа, выходы и lower цепочки; mangle не менялся. |
| registry: найденный дефект заведён тем же слиянием | №1875 уточняется этим ремонтом; №1876 заведена, tuple-carrier привязан к существующей №1014. |
| spec: язык меняется — D-блок | Не применимо: существующие D23/D34/D40/D275/D478/D489; язык не меняется. |
| std-check: nova check std/src до и после | Обе строки 162/26/67 приведены дословно без ANSI-цвета; исходная краснота не объявлена PASS. |
| invariant: новый инвариант, почему нужен | В компиляторе восстановлен прежний канал checker → lower; новый discovery-инвариант регистрации, необходимость и selftest описаны отдельно выше. |
| gate / ci: CI на кандидате зелёный | Делает приёмщик на кандидате integrate/t55; локальные gate/mega-CU не запускались. LANDED/CI пока нет. |
| commits: English, DCO, --only по именам | Код/отчёт фиксируются scoped English DCO commit; полный SHA передаётся приёмщику. Диагностический cherry-pick сохранён. |
| report: РЕПРО, ТОЧКА, ФИКС, ФИКСТУРЫ, САБОТАЖ, std/src, ВЕТКА/КОММИТ, ЧТО НЕ СДЕЛАНО | Разделы этого отчёта и приложенные machine logs. |

**Приёмка глазами, потому что** полноту границы класса и отсутствие новой языковой
политики нельзя вывести из одного PASS: сравнить три source-файла, нормативные
ссылки, матрицу форм и сохранённый source patch. Отдельная граница автоматизации:
новые conformance-фикстуры исполняются oracle CI и добавленным manifest-путём
novac-differential; локально прогоняется только явно названный registered scope.
Полный корпус/CI делегирован приёмщику, локальная выборка его не подменяет.
Allow/baselines ради зелени не расширялись.

## ВЕТКА/КОММИТ И ЧТО НЕ СДЕЛАНО

Ветка `t55-fix-1875`; диагностический предок [`81ca672d17d61ddda08b302561275d1f9079d52c`](https://github.com/nv-lang/nova/commit/81ca672d17d61ddda08b302561275d1f9079d52c) — «docs(novac): localize bootstrap record mangling failure», 2026-10-10. Финальный SHA будет
передан приёмщику с точным списком проверок. Для уборки нужны обе task-ветки.
CI кандидата выполняет приёмщик; локальные gate/mega-CU/full nova test не запускались.
`accept` должен указывать полный SHA кандидата, исходная plugin-ветка не его предок.

Остаются scoped commit/сдача и приёмочный clean combined candidate/CI.
Финальные targeted мета-проверки seams/guard-registry/registry rows, формы,
статуса и маршрутов зелёные: [task55-final-meta.txt](task55-final-meta.txt),
watch `1791647658567-9sp0rx`. `git diff --cached --check` также чист.

### Доработка приёмки, круг 1: canonical coalesce

Предварительный candidate, его точный SHA и причина отклонения сохранены как
исторические данные в [журнале provenance](task55-historical-provenance.json).
Он получил W_MANUAL_COALESCE в CI nova-lint (run `38067267936`): helper
`option_number` вручную раскрывал Option. Его тело заменено на каноническое
`o ?? -1`. Носители дефекта в `optional_tail` и expected-position match,
assertions, EXPECT_STDOUT и семь manifest entries сохранены без изменений.

Watch `1791649629256-erif5r`, rc0: targeted `lint --deny` — 1 файл,
0 findings/denied; oracle и fixed Carina `check` — PASS; targeted oracle
runtime — 1/1; e1 — stdout побайтно одинаков, exit0; прямой Carina runtime —
точно `p1875 positions 204 206 208 -1 71`.
Лог: [task55-review1.txt](task55-review1.txt).

Предварительный candidate не включал #57 и не является финальной проверкой.
Свежая штатная A→B→C и полный CI остаются обязательными на одном чистом
combined SHA после #57. №1875 остаётся OPEN до полной приёмки, 0.2 не объявлена.

### Доработка приёмки, круг 2: четыре блокера novac-gate

Приёмщик сообщил красный `nova-gate` на отклонённом candidate, чей exact SHA и
роль сохранены в [журнале provenance](task55-historical-provenance.json), run
`38074299290`:
устаревшие живые строки плана, отсутствие ledger за 2026-10-10, два адреса
`ice_at` после сдвига строк и нераспознанный делегирующий выход из типизации.
Это не успешная финальная приёмка; для manual run `38074337950` приёмщик
отправил отмену. Прежние доказательства не переносятся на новый final SHA.

- Две строки плана от 2026-10-07 дополнены текущим состоянием #55 и датой
  2026-10-10; история сохранена. В ledger добавлена оценка активной работы
  этой сессии `~0.50`, явно **НЕ ЗАМЕРЕНО**, без ожидания CI и работы приёмщика.
- `match_arms.nv` label теперь указывает на вызов `ice_at` строки 779,
  `lower_branch.nv` — строки 32. Тексты причин и ветвление сохранены.
- Возврат `@type_block_value_tail(b)` в `@type_branch_value` — делегирование
  для IfStmt/IfExpr/IfLet, не пропуск типизации. Helper вызывает
  `@type_if_value`/`@type_if_let(as_value: true)`: они записывают тип пути,
  который продолжается, либо сообщают отказ. Если обе ветви завершаются,
  значение не требуется; parser recovery уже имеет диагностику. Добавлен
  `SILENT-OK` с этой причиной. Новый отказ/ICE здесь изменил бы поведение,
  а повторный отчёт после внутреннего отказа дал бы каскад.

Watch `1791657718110-2b8li5`, rc0: все четыре адресных guard зелёные;
liveline отстаёт максимум на 1 день при пределе 2; ledger — 229 строк,
56 дат, максимальная сумма за дату 1.00; ICE — 255 вызовов;
no-silent-skip — 60 функций и 299 выходов. Size/line-length зелёные,
oracle `nova check novac/src/main.nv` — PASS, `git diff --check` чист.
Полный лог: [task55-review2.txt](task55-review2.txt).

Исполняемый алгоритм фикса и семь регрессий не менялись. После смены исходников
нужны новый чистый combined candidate, свежая штатная A→B→C с обеими сторонами
пробы сравнения и полный required CI/manual на **одном** окончательном SHA.
№1875 остаётся OPEN; 0.2 не объявлена.
Критерии 0.2 не изменены, 274.11 не закрывается.

## Запись приёмки кандидата

Первый опубликованный кандидат (exact SHA и причина отклонения сохранены в
[журнале provenance](task55-historical-provenance.json)) на ветке
`integrate/t55` был ошибочно собран от #56, а не от #57. База #56 была commit
[`ec4e6a23531100177f0e2a258ce44478d5595dd9`](https://github.com/nv-lang/nova/commit/ec4e6a23531100177f0e2a258ce44478d5595dd9) — «docs: centralize acceptance and merge-lock rules», 2026-10-10.
Он отклонён и не является precheck: `nova-lint` run `38067267936` завершился
с W_MANUAL_COALESCE в `p1875_match_block_expected.nv:17`; исправление helper
внесено отдельным commit
[`99b8176f81efbef15b3fd5114fe2bb4eaeac5734`](https://github.com/nv-lang/nova/commit/99b8176f81efbef15b3fd5114fe2bb4eaeac5734) — «test(novac): use canonical coalesce in block-tail fixture», 2026-10-10.

После обнаружения неверной базы остановлены активные workflow: `38067267975`
(`windows-process-acceptance`), `38067267961` (`crate-tests`), `38067267970`
(`nova-gate`); GitHub подтвердил для всех `completed/cancelled`. Watch
`1791649283067-zqdlmm` был отменён ранее, но он останавливал только наблюдателя,
не сами workflow. Наблюдение о lint `1791649869205-ylznfv` также не подтверждало
код процесса. Отдельная проба `1791650217818-rten6r` сняла точный результат:
argv `nova-cli/target/release/nova.exe lint --deny
spec_tests/conformance/standalone/p1875_match_block_expected.nv`, binary SHA256
`28e49f0d39ac813e44813671e9d551ce56235e235885811bb7c903c98e1a9801`, фактический
process rc=0, stderr пустой. Строка `(--deny, exit 1)` в stdout — вводящая в
заблуждение часть summary, не exit status.

Другие workflow прежней неверной базы: `38067267912` (`nova-doc`),
`38067267986` (`contracts-z3`), `38067267968` (`contracts-crosscheck`) и
`38067268005` (`nova-test-regression`) завершились success; эти результаты
также не считаются полным CI кандидата. `#57` достигла `origin/main` на
[`121c39883d7b39f6d1c80e23c2d3a704fa061a37`](https://github.com/nv-lang/nova/commit/121c39883d7b39f6d1c80e23c2d3a704fa061a37) — «docs(registry): make the example carrier caveat explicit», 2026-10-10; свежий `integrate/t55-final`
готовится на этой базе. Финальные A→B→C, оба-way сравнения, обязательный CI и
manual full должны пройти на одном clean SHA; до этого приёмка CI pending,
№1875 остаётся открытой, 0.2 не объявлена.
