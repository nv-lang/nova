<!-- SPDX-License-Identifier: CC-BY-4.0 -->
КЛЕТКА | check-novac-* | К7

# Охота: check-novac-* × К7 — зелёный ноль

**Трек:** guards · **Дата:** 2026-09-04 · **Модель охотника:** opus
**Пробы:** `probes/2026-09-04-check-novac-k7/` — 18 цитируемых каталогов из 40
построенных (10 стражей × 4 пробы: чистое дерево · обещанная форма · другой
синтаксис · потерянная мишень). Все 40 охотник перезапустил из ЧУЖОГО cwd
скриптом `verify.py`: `probes checked: 40, problems: 0` — сверены и код
возврата, и наличие артефакта (класс №770: молчание — не успех).
**Проверено окном запуском из дерева** (пути в `cmd.sh` переписаны на корень
репозитория, выведенный из места пробы): `p_check-novac-tyid-door_zerotarget`,
`p_check-novac-resolve-discipline_zerotarget`,
`p_check-novac-no-string-keys_othersyntax` — все три печатают `ok` с числом и
`rc=0`, как записано в их `verdict.txt`. ПОДТВЕРЖДЕНО.
**Первая охота трека** (план 278 Ф.8, слово владельца «добавь в охотника искать
ошибки в твоих стражах»); строки реестра — с меткой `(guards)`, шесть, по
классам и живым носителям, а не по восемнадцати находкам.

## Свойство класса — одно на все восемнадцать находок

Страж судит ФОРМУ через ЯКОРЬ — путь (`novac/src/sem`), имя каталога (`lex`),
имя поля (`type_id`), спеллинг (`T_INT`, `Table`, `.find(`), список файлов
(три), стиль заголовка (`## 1.`). **Ни один из десяти не проверяет, что его
якорь ещё цел.** Когда якорь уезжает — модуль переименован, поле переименовано,
запись написана в одну строку, эмиссия разрослась на четвёртый файл, спеллинг
дефолта сменился, — страж считает НОЛЬ носителей и печатает зелёное, причём в
шести случаях из десяти вместе с правдоподобным ЗАМЕРОМ («файлов 1, экспортов
fn 1, `-> Result[` во фронтенде: 0»), неотличимым от настоящей проверки. Вторая
половина того же свойства — второй синтаксис: у всех десяти есть законная
форма, выражающая ровно ту же запрещённую вещь и не попадающая в образец.

Крайний случай в дереве уже наступил: у `check-novac-resolve-discipline` три
правила из четырёх ищут спеллинг `T_INT`/`"nova_int"`, которого в `novac/src`
**ноль** строк, а живой спеллинг — `ctx.prims.int_id`.

## Находки машинной строкой

НАХОДКА | guards | check-novac-no-naked-panic | p_check-novac-no-naked-panic_othersyntax | шапка: «ВНЕ СУДА … строки с комментарием: упоминание в прозе не вызов» → `check-novac-no-naked-panic ok: голых panic( в novac/src нет (дверь — ice() в diag)` (rc=0) при живом `panic("token table is empty") // invariant: the table is built in new()` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-no-naked-panic | p_check-novac-no-naked-panic_zerotarget | шапка: «явный инвариант идёт через дверь `ice()`, а не голым `panic(`» → `check-novac-no-naked-panic ok: голых panic( в novac/src нет (дверь — ice() в diag)` (rc=0), когда исходники лежат в `novac/lib`, а в `novac/src` ни одного .nv — проба «потеря мишени»
НАХОДКА | guards | check-novac-tyid-door | p_check-novac-tyid-door_othersyntax | шапка: «ЧТО ЛОВИТ: сравнение поля-идентификатора типа … с литералом `0` операторами `>=`, `<=`, `>`, `<`» → `check-novac-tyid-door ok: файлов .nv: 1, сравнений идентификатора типа с нулём вне двери: 0` (rc=0) при `if 0 <= args[at].type_id` и `if fd.ret_id > -1` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-tyid-door | p_check-novac-tyid-door_zerotarget | шапка: «Ровно эти три имени — они и есть поля типа `TyId` в реестрах (§10.3в)» → `check-novac-tyid-door ok: файлов .nv: 1, сравнений идентификатора типа с нулём вне двери: 0` (rc=0) при `if args[at].tyid >= 0 && fd.rettype >= 0` — проба «потеря мишени»
НАХОДКА | guards | check-novac-frontend-shape | p_check-novac-frontend-shape_othersyntax | шапка: «ошибки живут ДАННЫМИ рядом с результатом, а не альтернативой ему» → `check-novac-frontend-shape ok: файлов 2, экспортов fn 2, '-> Result[' во фронтенде: 0` (rc=0) при `export type ParseOut value { out Result[Tree, ParseError] }` + `export fn parse(src str) -> ParseOut` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-frontend-shape | p_check-novac-frontend-shape_zerotarget | шапка: «в модулях фронтенда (`lex`, `parse`, `tree`, `syntax`) ни одна ЭКСПОРТИРОВАННАЯ функция не объявляет `-> Result[`» → `check-novac-frontend-shape ok: файлов 1, экспортов fn 1, '-> Result[' во фронтенде: 0` (rc=0), когда `lex` переименован в `lexer` и нарушение уехало вместе с ним — проба «потеря мишени»
НАХОДКА | guards | check-novac-arch-invariants | p_check-novac-arch-invariants_othersyntax | шапка: «Счётчик раздела — это обещание, которое можно проверить арифметикой» → `check-novac-arch-invariants ok: счётчики на месте (2 строк счёта)` (rc=0) при разделе с пятью инвариантами прозой и строкой `Счётчик раздела: **0**` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-arch-invariants | p_check-novac-arch-invariants_zerotarget | шапка: «у каждого раздела карты есть счётчик инвариантов (274.1 §2б)» → `check-novac-arch-invariants ok: счётчики на месте (0 строк счёта)` (rc=0) на карте, где все три раздела без счёта, а нумерация записана как `## §1.` — проба «потеря мишени»
НАХОДКА | guards | check-novac-ref-field-names | p_check-novac-ref-field-names_othersyntax | шапка: «поле-ссылка называет своё пространство (конвенция П19)» → `check-novac-ref-field-names ok: полей-ссылок int в реестрах: 0 (с суффиксом 0, полиморфных 0), безымянных пространств: 0` (rc=0) при `export type FieldDef value { name str, owner int, recv int }` в одну строку (форма живая: `std/src/collections/index_map/core.nv:315`) — проба «другой синтаксис»
НАХОДКА | guards | check-novac-ref-field-names | p_check-novac-ref-field-names_zerotarget | П19 §5 дословно: «судит поля `int` в записях `sem` — единственное место, где живут реестры» → `check-novac-ref-field-names ok: судить нечего (нет …/novac/src/sem)` (rc=0), когда та же запись с `owner int` лежит в `novac/src/regs` — проба «потеря мишени» (самотест закрепляет «судить нечего» только на ПУСТОМ дереве, случай 7)
НАХОДКА | guards | check-novac-no-string-keys | p_check-novac-no-string-keys_othersyntax | шапка: «Внутри `names/` строковый ключ законен, но обязан нести `NamespaceId`: `Map[(NamespaceId, str), ...]` или `Map[NsKey, ...]`» → `check-novac-no-string-keys ok: файлов .nv: 1, строковых ключей вне закона: 0, синтезированных ключей: 0` (rc=0) при `map HashMap[str, int] /// the NamespaceId key component is nowhere: this table has none` — исключение снимается СЛОВОМ В КОММЕНТАРИИ; живой носитель `novac/src/names/names.nv:79` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-no-prelude-shadow | p_check-novac-no-prelude-shadow_othersyntax | шапка: «Собираются экспортированные имена прелюдии (`export type X` … и СВОБОДНЫЕ `export fn name(`), затем в novac/src ищутся одноимённые декларации любого вида» → `check-novac-no-prelude-shadow ok: имён прелюдии: 2, файлов novac/src: 1, теней: 0` (rc=0) при прелюдии из ТРЁХ экспортов и двух тенях, объявленных через `extern "nova" fn` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-resolve-discipline | p_check-novac-resolve-discipline_othersyntax | шапка: «1. сравнение имён (`== name`) вне `names/` — это линейный скан там, где есть `names.NameTable` с O(1)» → `check-novac-resolve-discipline ok: файлов .nv: 2, линейных сканов и тихих int-дефолтов: 0` (rc=0) при `if name == x.text` (операнды наоборот) и `if x.text == ident` (другое имя переменной) — проба «другой синтаксис»
НАХОДКА | guards | check-novac-resolve-discipline | p_check-novac-resolve-discipline_zerotarget | шапка: «2. `< 0` и следом `return T_INT` … 3. хвост-дефолт `T_INT` / `"nova_int"` последней строкой» → `check-novac-resolve-discipline ok: файлов .nv: 2, линейных сканов и тихих int-дефолтов: 0` (rc=0) при `if !is_ty(id) { return ctx.prims.int_id }` и голом хвосте `ctx.prims.int_id`; в `novac/src` спеллинга `T_INT` ноль строк — проба «потеря мишени»
НАХОДКА | guards | check-novac-no-alloc-in-lookup | p_check-novac-no-alloc-in-lookup_othersyntax | шапка: «ВНУТРИ ДВЕРИ запрещены: … сборка строки интерполяцией `${`» → `check-novac-no-alloc-in-lookup ok: файлов .nv: 1 (из них с дверями поиска: 0), аллокаций в дверях: 0` (rc=0), когда ключ строит ХЕЛПЕР `key_of`, а дверь берёт его результат — проба «другой синтаксис»
НАХОДКА | guards | check-novac-no-alloc-in-lookup | p_check-novac-no-alloc-in-lookup_zerotarget | шапка: «ДВЕРЬ — это функция, которая: либо метод таблицы (имя приёмника кончается на `Table`), либо спрашивает таблицу (`.find(`/`.lookup(`)» → `check-novac-no-alloc-in-lookup ok: файлов .nv: 1 (из них с дверями поиска: 0), аллокаций в дверях: 0` (rc=0) при `export fn NameRegistry @at(...)` с `${}`-ключом И `[]int.of(1, 2, 3)` внутри — проба «потеря мишени»
НАХОДКА | guards | check-novac-emitted-names | p_check-novac-emitted-names_othersyntax | шапка: «у каждого C-имени, которое novac ПЕЧАТАЕТ, есть объявленное пространство (конвенция П24)» → `check-novac-emitted-names ok: печатаемых имён: 4, все в объявленных пространствах` (rc=0), когда печатаемое имя лежит ДАННЫМИ (`BuiltinType { name: "i32", c_name: "int32_t" }`), а `mangle.nv` только возвращает `prim_c_name(...)`; живые носители `novac/src/builtins/builtins.nv:261-268` — проба «другой синтаксис»
НАХОДКА | guards | check-novac-emitted-names | p_check-novac-emitted-names_zerotarget | шапка: «$1 — корень; $2 — override списка файлов» при зашитом списке из трёх файлов → `check-novac-emitted-names ok: печатаемых имён: 4, все в объявленных пространствах` (rc=0), когда четвёртый файл эмиссии печатает `"my_struct_t"` и `"legacy_row"` — проба «потеря мишени»

## Пройдено без находок

Стража, прошедшего ВСЕ четыре пробы по ожиданию, среди десяти нет — поэтому
секция даёт данные по осям. Формат: `clean · promised · othersyntax · zerotarget`
(код возврата стража; ожидание — `0 · 1 · 1 · 1`).

* check-novac-no-naked-panic — 0 · 1 · **0** · **0**; оси `clean`/`promised` чисты, адрес в отказе есть.
* check-novac-tyid-door — 0 · 1 · **0** · **0**; отказ называет `файл:строка` и сам оператор.
* check-novac-frontend-shape — 0 · 1 · **0** · **0**.
* check-novac-arch-invariants — 0 · 1 · **0** · **0**; отказ называет заголовок раздела.
* check-novac-ref-field-names — 0 · 1 · **0** · **0**; отказ называет поле и строку.
* check-novac-no-string-keys — 0 · 1 · **0** · 0 — **ось «потеря мишени» ЧЕСТНАЯ**: при уехавшем `novac/src` печатает `ok: судить нечего (нет …, файлов .nv: 0)`, закреплено самотестом (случай 1). Замера не изображает.
* check-novac-no-prelude-shadow — 0 · 1 · **0** · 0 — та же честная ось: `ok: судить нечего (нет …/std/src/prelude)`, закреплено самотестом.
* check-novac-resolve-discipline — 0 · 1 · **0** · **0**.
* check-novac-no-alloc-in-lookup — 0 · 1 · **0** · **0**; главный исторический случай (ключ строкой выше внутри той же двери) ловится.
* check-novac-emitted-names — 0 · 1 · **0** · **0** — **единственный из десяти, у кого есть проверка сломанного разбора**: пустой список имён и отсутствующий файл дают КРАСНОЕ («не нашлось ни одного имени: разбор сломался (класс №519)», «нет {f}: судить нечего (класс №519)»). Инвентарь пометил его `nol-misheni: NET` — это и есть обещанная грубость верхней оценки.

## Адреса и живые носители в дереве (замерено, не додумано)

* `novac/src/names/names.nv:79` — `    map HashMap[str, int] /// the NamespaceId key component is @ns, one field up`: плоский строковый ключ, проходящий стража по слову в КОММЕНТАРИИ.
* `novac/src/builtins/builtins.nv:261-268` — печатаемые C-имена `int8_t`, `int16_t`, `int32_t`, `int64_t`, `uint16_t`, `uint32_t`, `uint64_t`: ни одной объявленной приставки (`Nova_`, `nova_`, `novac_`, `NOVAC_`, `_novac_`, `_NovaTuple`) и ни одного поимённого исключения. Путь до C: `builtins.prim_c_name` ← `novac/src/sem/mangle.nv:231`, `:455`, `:651`.
* `T_INT` в `novac/src`: 0 строк; `"nova_int"` голым хвостом: 0 строк (единственная строка `builtins.nv:256` — поле записи, под образец хвоста не подходит). Живой спеллинг — `ctx.prims.int_id` (`check/type_of.nv:160`, `:310`, `:319`).
* `check-novac-emitted-names` на живом дереве с расширенным списком файлов краснеет (rc=1): `abc`, `anonymous`, `struct`, `tuple`. Оговорка честности: три из четырёх — слова в КОММЕНТАРИЯХ (`emit_c/emit_expr.nv:387`, `:467`), четвёртое — сравнение с ключевым словом C в `emit_c/shell.nv:87`. Это ложняки расширения списка, а не находки; находка — что список не может заметить переезд эмиссии.
* Форма «запись в одну строку» живая: `std/src/collections/index_map/core.nv:315` — `type IndexMapIter[K, V] { map IndexMap[K, V], pos int }`.
* Форма `extern "nova" fn` живая: `std/src/prelude/concurrency.nv:61,68,76,81,89` (в прелюдии — только методы и ассоциированные; свободного extern-экспорта в прелюдии сегодня нет, и в novac externов нет — зазор назван, носителя в дереве нет).

## Команды воспроизведения

```sh
P=docs/dev/hunts/guards/probes/2026-09-04-check-novac-k7   # из корня репозитория
sh "$P/p_check-novac-emitted-names_zerotarget/cmd.sh"
sh "$P/p_check-novac-resolve-discipline_zerotarget/cmd.sh"
sh "$P/p_check-novac-no-string-keys_othersyntax/cmd.sh"
sh "$P/p_check-novac-no-alloc-in-lookup_zerotarget/cmd.sh"
# каждый cmd.sh выводит корень репозитория из своего места в дереве и печатает
# вердикт стража и rc=N; ожидаемый вывод — в verdict.txt рядом

# живой замер, только чтение
python scripts/guards/check-novac-emitted-names.py .
python scripts/guards/check-novac-emitted-names.py . "$(ls novac/src/emit_c/*.nv novac/src/sem/mangle.nv | tr '\n' ' ')"
```

## Что обошёл и почему

* **72 стража из 82 не открывал.** Взято 10 — все с инъекцией входа (argv-шов ROOT/каталог/файл) и небольшим корпусом. Правило брифа «стража без инъекции входа не пробовать» снимает `check-novac-differential.sh`, `-build-clean.sh`, `-oracle-fresh.sh`, `-fuzz-zero-panic.sh`, `-lint.sh`, `-batch.sh`, `-emission-size.sh`, `-iteration-cost.sh`, `-smoke-wrapper.sh`, `-local-only-work.sh`, `-commit-donor.sh`, `-shell-freshness.sh`, `-mangle-fixed-point.sh` — они читают своё дерево, свой git или гоняют бинарь.
* **Групповые самотесты и `check-novac-selftest-proves-red.sh` не запускал** — прямой запрет брифа. Гейт, `nova test`, мега-CU не запускал.
* **Ни одного стража не правил и не подменял**; в рабочем дереве не создано и не изменено ни одного файла, в главную репу не заходил.
* **Не проверял компилятором, что пробные `.nv` законны как Nova.** Формы подобраны по ЖИВЫМ носителям в дереве (запись в одну строку — из std, `extern "nova" fn` — из std/прелюдии, `ctx.prims.int_id` — из check), но `novac check` не гонял: бинарь делится с гейтом. Для стража это неважно — он читает текст, — но окну при триаже знать полезно.
* **Вторую форму искал грепом по другому синтаксису, но не исчерпывающе.** Где не нашёл — говорю прямо: свободный `export extern "nova" fn` в прелюдии (искал `^export extern "nova" fn [a-z_]*(` по `std/src`), многострочная сигнатура во фронтенде (искал `^export fn .*($` по `novac/src` — 0), заголовок типа с комментарием после `{` (искал `^type .*{ *//` — 0).
* **`check/check.nv:76 gap_at int` — подозрение без вердикта.** Поле хранит ссылку («The FIRST gap seen in this file»), суффикса пространства не несёт и лежит ВНЕ `sem`, то есть подпадает под находку про перенос мишени. Законно ли оно как «смещение в тексте» (П19 §5 освобождает `lo`/`hi`, `start`/`end` в `source`/`lex`/`tree`, но не `check`) — решать не охотнику.
* **Мера «сколько ещё стражей того же класса» не считалась.** Инвентарь даёт 44 строки `nol-misheni: NET`; на десяти проверенных верхняя оценка подтвердилась девять раз из десяти. Экстраполяция на оставшиеся 34 — не вердикт охотника.

## Противоречия — оба места дословно, выбор не охотника

**(а) `check-novac-no-naked-panic.py` спорит сам с собой.** Шапка (строки 10-11): «ВНЕ СУДА: сама дверь (`diag/diag.nv`) — там `panic` и живёт, — и строки с комментарием: упоминание в прозе не вызов.» Тело (строки 52-53): `if "// " in line:` / `continue`. На вопрос «судится ли строка, которая ЗОВЁТ `panic` и несёт хвостовой комментарий?» шапка отвечает «да, это вызов, а не упоминание», код отвечает «нет». Самотест закрепил только чистый комментарий («panic( в комментарии — не находка»).

**(б) правило строкового ключа и код двери имён.** Страж (`check-novac-no-string-keys.py`, строки 7-8): «Внутри `names/` строковый ключ законен, но обязан нести `NamespaceId`: `Map[(NamespaceId, str), ...]` или `Map[NsKey, ...]`». Дерево (`novac/src/names/names.nv:77-79`): `export type NameTable {` / `    ns NamespaceId /// which namespace these names belong to` / `    map HashMap[str, int] /// the NamespaceId key component is @ns, one field up`. На вопрос «где живёт компонент `NamespaceId` ключа» два места отвечают по-разному — В КЛЮЧЕ (страж) и В СОСЕДНЕМ ПОЛЕ (код), — а зелёным сегодня всё держится словом `NamespaceId` в доккомментарии той же строки.

**(в) П24 и таблица builtins.** Шапка стража: «у каждого C-имени, которое novac ПЕЧАТАЕТ, есть объявленное пространство … Исключения названы поимённо: C-ключевое `void`, подстановочник `_`, метод `equal`, `fmod`, слоты шаблона оболочки и константа рантайма NOVA_UNIT». Дерево (`novac/src/builtins/builtins.nv:261`): `    BuiltinType { name: "i8", c_name: "int8_t" },` (и ещё шесть таких же). На вопрос «есть ли у печатаемого `int32_t` объявленное пространство» конвенция отвечает «обязано быть, иначе красное», дерево — «нет, это спеллинг stdint.h». Пространство ли это третьей стороны, которое надо объявить, или исключение, которое надо назвать поимённо, — решать не охотнику.

## Что решило окно, принимая отчёт (2026-09-04, окно 274)

* Противоречие (в): спеллинги `stdint.h` — пространство ТРЕТЬЕЙ СТОРОНЫ, и оно объявляется в шапке стража поимённо, как `void` и `fmod`; страж этот принадлежит окну 274, решение — его. Записано строкой реестра.
* Противоречия (а) и (б) — строки реестра с двумя адресами; фикс (а) очевиден (суд по строке с отрезанным хвостовым комментарием), фикс (б) — выбор между составным ключом и переписанным правилом, обе стороны названы в строке.
* Класс К7 целиком — мета-страж: каждый `check-novac-*` с инъекцией входа гоняется на ПУСТОМ корне, и `ok` с числом там — красное. Это первый пункт следующей волны стражей, не этого отчёта.
* Подозрение про `check/check.nv:76 gap_at int` — прочитано: это БАЙТОВОЕ СМЕЩЕНИЕ в тексте файла («a refusal whose run has zero width», `gap_at < 0` = не видано), не ссылка в пространство реестра, и суффикс пространства ему не положен. Но охотник прав в другом: П19 §5 освобождает такие поля ПО ИМЕНИ И МОДУЛЮ (`lo`/`hi`, `start`/`end` в `source`/`lex`/`tree`), а не по смыслу — поле со смыслом смещения под другим именем в `check` страж `check-novac-ref-field-names` сегодня не судит только потому, что судит лишь `sem`. Записано как ширина стража, не как дефект поля; если суд расширится на `check`, поле либо переименовывается в `gap_lo`, либо освобождается явно.
* Строка №911 (класс якоря) и №912 (второй синтаксис) приняты интегратором с приёмкой уровня класса; номера 911–916 присвоены им же тем же днём.
