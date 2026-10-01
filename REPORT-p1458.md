# REPORT-p1458 — `ro []T` в двери чтения типа Карины (№1458)

Облачная сессия, 2026-10-01. Ветка `p1458-novac-ro-slice-ret` (она же
`p1458-novac-ro-slice-ret-cyw6vx` — ветка, назначенная средой облачной сессии;
обе указывают на один коммит). Основание — `origin/integrate`, в конце слит
`bf2c8ba`.

```
КОРЕНЬ: двойной. (1) `ty_of_type_ref` (novac/src/sem/typeref.nv) — одна дверь
  для типа возврата, поля, параметра и компонента кортежа — не знала ведущего
  `ro` и отвечала `no_ty()`; `ret_ty_of_decl` читает с индекса 1, то есть
  ровно с `ro`. (2) `@report_if_return_form` (check/rules.nv) считал `KwRo`
  «формой» и отказывал целиком — так отказывали и методам, хотя harvest.nv
  уже умел перешагнуть `ro` (и при этом выбрасывал его: вид терял «только
  чтение»). Права «только чтение» не хранил никто.

ФИКС:
  sem/typeref.nv:89 — `ro T` читается как терм `T` (D176: тот же C, нулевая
    цена); `ret_is_ro` / `decl_ret_is_ro` — дверь «возврат — вид».
  sem/typeref.nv:98 — элемент `[]Elem` читается той же дверью: `[][]T`,
    `[]Vec[T]` стали типами (вложенная клетка №1458).
  sem/callables.nv `FnDef.ret_ro`, sem/sem.nv `FieldDef.ro_view`,
    resolve/resolve.nv `FieldIndex.field_is_ro` — право записи живёт на строке.
  check/rules.nv `@report_if_return_form` — форма судится по типу ПОСЛЕ `ro`
    (`-> ro *T` по-прежнему отказ указателя); `past_field_ro` — поле `f ro T`.
  check/type_of.nv `typed_arg_class` — вызов, отдающий вид, — класс `ro`
    (mut-получатель и mut-параметр не находят кандидата).
  check/assign_rules.nv — `view[i] = x` / `view.f = x` → E_READONLY_CONTENT,
    `mut x = view()` → E_READONLY_COERCE.
  check/rules.nv `@report_typed_local` — `ro x ro T` → E_REDUNDANT_TYPE_MODIFIER
    у второго `ro` (как у оракула).
  check/return_rules.nv — `ro`-параметр через `-> ro T` больше не «launder».

КЛЕТКИ (позиция × T × чтение/запись -> до / после; оракул в скобках):
  возврат fn `=>`      × int       × чтение          -> TYPE_FORM / ок (ок)
  возврат fn блок      × int       × чтение (for)    -> TYPE_FORM / ок (ок)
  возврат метода       × str       × чтение          -> TYPE_FORM / ок (ок)
  возврат метода       × запись    × чтение поля     -> TYPE_FORM / ок (ок)  (`-> ro Spot`)
  возврат fn           × [][]int   × подпись         -> TYPE_FORM / ок (ок): `fn grid(rs [][]int)
                         -> ro [][]int => rs` чист. Программу, ИСПОЛЬЗУЮЩУЮ вложенный вектор,
                         Карина отвергает шеллом («carries no `len` for Vec[Vec[int]]»,
                         давний названный отказ интеропа); оракул не парсит `[][]int.of(...)`
                         в выражении — исполняемой фикстуры нет
  возврат fn           × int       × `f()[i] = x`    -> TYPE_FORM / E_READONLY_CONTENT (то же)
  возврат fn           × int       × `mut a = f()`   -> TYPE_FORM / E_READONLY_COERCE (то же)
  возврат fn           × int       × `ro a=f(); a[i]=x` -> TYPE_FORM / E_READONLY_FIELD (оракул:
                         E_READONLY_CONTENT — давнее расхождение имени кода на ro-связи, не этот класс)
  возврат fn           × int       × `f().push(x)`   -> TYPE_FORM / «no such method» (оракул МОЛЧИТ)
  возврат метода       × int       × `mut v = o.m()` -> TYPE_FORM / E_READONLY_COERCE (оракул МОЛЧИТ)
  аннотация `ro x ro []T`                            -> «нет типа `ro`» / E_REDUNDANT_TYPE_MODIFIER (то же)
  аннотация `mut x ro []T`                           -> «нет типа `ro`» / отказ типизированного
                         локала E2-b3 без ложного «нет типа» (оракул: ок)
  поле `f ro []int`    × int       × чтение          -> TYPE_FORM / ок (ок)
  поле `f ro []int`    × int       × `w.f[i] = x`    -> TYPE_FORM / E_READONLY_CONTENT (то же)
  поле `f ro []int`    × int       × `mut x = w.f`   -> TYPE_FORM / E_READONLY_COERCE (то же)
  поле                 × `@f[i] = x` в методе        -> TYPE_FORM / E_READONLY_CONTENT (то же)
  параметр `xs ro []int`                             -> qualifier + TYPE_FORM / так же
                         (оракул E_REDUNDANT_PARAM_RO; двойной отказ — давний, не этот класс)
  T = своя запись в векторе (`[]Spot`) — Карина отказывает: в интероп-шелле
    нет инстанса Vec[Spot] (существующий названный отказ, не этот класс).

МЕРА 0.2 (один процесс: NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src novac check
  novac/src/*/*.nv novac/src/*.nv, порядок и пути как в novac-diff-corpus.sh;
  GC_DONT_GC=1 — см. ОТКРЫТО п.1):
  на исходниках integrate ed90b40, старый бинарь -> новый:
    файлов 105/128 -> 99/128, диагностик 240 -> 728, TYPE_FORM 15 -> 0;
  на слитом дереве (integrate bf2c8ba + ветка), бинарь integrate -> бинарь ветки:
    файлов 104/128 -> 99/128, диагностик 241 -> 728, TYPE_FORM 16 -> 0
    (16-й — новая подпись самой ветки `past_field_ro -> ro []Node`).
  Оставшихся TYPE_FORM нет; кортежей и `fn`-типов среди них не было — все 15
  были `ro`: `-> ro []T` 10 (channel 6, mono 3, slots 1), `-> ro Node` 5
  (slots 2, parse_test 3).
  РОСТ ЧИСЛА — СНЯТАЯ МАСКА, НЕ РЕГРЕСС: отказ обхода в одном файле модуля
  гасит типизацию всего модуля. Доказано на старом бинаре: модуль parse/
  без parse_test.nv даёт те же 445 диагностик, что новый бинарь со всем
  модулем (+12 своих у parse_test); mono/ — 46 вместо 3. Принятых файлов
  меньше на 6: parse/* (7 файлов) впервые типизируются и отказывают,
  sem/slots.nv впервые принят. Ни одной диагностики E_READONLY_* /
  «ro view» в самосборке — новые запреты не задели собственный код Карины.

ФИКСТУРЫ: novac/fixtures/ro_slice_ret/
  pos_1 (fn `-> ro []int`, `=>` и блок с `return`, `.len()`, индекс, `for`),
  pos_2 (методы `-> ro []str` и `-> ro Spot`, `for` по виду) —
  смоук байт-в-байт: ДА, `sh scripts/tools/novac-e1-smoke.sh` на Linux
  (NOVAC_BIN=novac/target/novac, GC_DONT_GC=1); все 64 pos_* фикстуры зелёные
  до и после слияния integrate.
  neg_1 (`f()[0] = 9`), neg_2 (`mut a = f()`), neg_3 (`ro a ro []int`),
  neg_4 (`w.xs[0] = 9` при `xs ro []int`) — по одному диагностику
  (check-novac-no-cascade ok, 73 фикстуры), NOVAC_EXPECT совпадает; оракул
  отказывает каждой тем же кодом.

ОБЕ СТОРОНЫ: на одном дереве снята ветка `ro` в двери и шаг через `ro` в
  правиле формы возврата -> pos_1, pos_2, neg_1, neg_2 краснеют (TYPE_FORM);
  возвращено -> зелёные. Запреты записи: бинарь integrate на neg_1..4 даёт
  другой первый отказ (TYPE_FORM / «unknown field»), check-novac-fixture-expect
  на нём красный по всем четырём.

САМОСБОРКА: да — `nova build novac/src/main.nv -o novac/target/novac` на
  оракуле этой ветки (до и после слияния). Модульные тесты: check_test,
  binding_test, mangle_test, resolve_test, parse_test, types_test, lower_test —
  PASS; pipeline (typeref_test тянет весь модуль) — те же два падения, что на
  чистом integrate (subset_pattern_test.nv:449, subset_test.nv:775), без новых.
  Стражи novac: lint, no-cascade, diag-schema, row-fields (поля внесены в
  274 §10.3в), subset-debt-dated, surface (resolve 15 -> 16, `field_is_ro`) —
  зелёные по моим правкам; красные на чистом integrate остаются красными
  (branch-complete check.nv:817-818, module-donor check/slice.nv, surface
  builtins/sem, fixture-expect pattern_depth/neg_3 и slice_vs_range/neg_1,
  time-ledger — мелкий клон).

КОММИТЫ: 9f530f3 фикс + реестр №1458 + фикстуры + леджер + §10.3в + база
  поверхности; 4d5f261 слияние integrate bf2c8ba (реестр — union);
  2481e70 элемент среза через дверь типа (`[][]T`); последним — этот отчёт.

ОТКРЫТО:
  1. Бинарь Карины, собранный на Linux, недетерминирован под Boehm GC: мера 0.2
     на одном бинаре гуляет 208…242 (новый — 341…880), смоук display_synth/pos_2
     (давняя фикстура) то зелёный, то «no such method». С GC_DONT_GC=1 всё
     повторяется до числа. Это класс №1461 (заведён интегратором при слиянии
     p1442) — подтверждение на Linux, отдельной строки не заводил.
  2. Дефекты ОРАКУЛА (не чинил): он молчит на запись через `ro`-вид в формах
     `f().push(x)`, `grow(f())` при `mut`-параметре, `x = f()` в `mut`-связь,
     `mut v = o.m()` при `-> ro T` у МЕТОДА (у свободной функции отказ есть),
     `bx().n = 2` и `bx().xs[0] = 2` через `ro`-вид записи. Карина первые
     две и две последние отвергает по спеке (D176), `x = f()` — нет (см. п.3).
  3. Отмывание вида через связь не закрыто: `ro a = f(); mut b = a` и
     `mut a = ...; a = f()` Карина пропускает. Это давний класс
     [M-ro-launder-via-mut-binding] (тот же пропуск и для обычной `ro`-связи:
     `ro a = []int.of(1); mut b = a` оракул отвергает, Карина молчит), и в
     собственном коде Карины есть носитель: `recv_kids = branch_children(c)`
     (sem/harvest.nv:239) — закрытие добавит самосборке отказ.
  4. Код на индексной записи через `ro`-СВЯЗЬ: оракул E_READONLY_CONTENT,
     Карина E_READONLY_FIELD — давнее расхождение, не этот класс.
  5. Вектор своей записи в фикстуре недоступен: интероп-шелл не несёт
     Vec[UserRecord]; pos_2 проверяет вид над записью через `-> ro Spot`.

ВОПРОСЫ ИНТЕГРАТОРУ:
  1. Номер для дефектов оракула из ОТКРЫТО п.2 (одна строка, класс «запрет
     записи через `ro`-вид в оракуле неполон: судится только инициализатор
     `mut`-связи от свободной функции и индексная запись»). Рекомендация:
     одна строка К2, маршрут — оракул; Карина уже ведёт себя по спеке, так
     что дифференциал разойдётся только на отвергнутых программах.
  2. Закрывать ли п.3 (отмывание через связь) в Карине сейчас. Варианты:
     (а) отдельной задачей с правкой harvest.nv:239 — рекомендую: класс
     шире №1458 и касается обычных `ro`-связей; (б) оставить до оракула.
  3. Ветка: среда облачной сессии назначила `p1458-novac-ro-slice-ret-cyw6vx`;
     тот же коммит запушен и под именем из задачи `p1458-novac-ro-slice-ret`
     (если пуш под ним прошёл — см. ответ в треде). Сливать любую.
  4. Авторство коммитов — `Claude <noreply@anthropic.com>` (git config среды;
     user.* по AGENTS.md не трогал), check-commit-hygiene на это красный —
     так же, как у прежних облачных веток (p1338, p1343).
```
