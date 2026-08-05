# PROGRESS — p248tup-145 (окно p248tup-145, №248 + №145)

Worktree: `d:/Sources/nv-lang/nova-p248t`, branch `p248tup-145`, база `main` (`57bf5fdf6`).
Модель: sonnet.

## Итог одной строкой

№248 — найден и зафиксирован корень (класс «дефект прошёл мимо гейта, потому
что диспетч именованного-кортежа тихо деградировал» — не буквально «класс
№129 single-key last-wins», как было продиагностировано окном p-op-channel,
а СОСЕДНИЙ по классу дефект в семействе «C-строка вместо канала»). №145 —
три дыры закрыты частично (2 из 3 в компиляторе, 1 требует D-амендмента,
который эта волна НЕ пишет — зона интегратора).

**Приёмка от 2026-08-05 потребовала перевода фиксов №248/№145(3) на
чекер-канал (arch-ratchet был красным).** Перевод сделан. `arch-ratchet`
зелёный (`lines=64545<=64545`, `infer=348<=348`), фикстуры целы, мега-CU
**657/0/68** (не ниже достигнутого ранее), `nova check std/src` = 148/26/61
байт-в-байт. По дороге канальный перевод вскрыл и заставил зачинить ТРИ
смежных дефекта в самом канале — раздел «Ход канального перевода» ниже
разбирает каждый честно (что сломалось, как нашёл, как починил).

---

## №248 [M-named-tuple-cu-recv-method-misresolution]

### Ревизия диагноза окна p-op-channel

Окно p-op-channel (2026-08-02) продиагностировало корень как «класс №129
single-key `method_receivers` last-wins» на самом вызове `.div_rem()`.
Это окно НЕ подтвердило эту гипотезу эмпирически:

- Инструментировал `emit_c.rs` (временный `eprintln!` перед веткой
  `method_receivers.get(method)` single-key fallback, ~emit_c.rs:43929) —
  собрал debug-компилятор, прогнал ТРИ независимых репро (изолированный
  `Frac`+`StringBuilder`, `Frac`+co-compiled `Widget` с ОДНОИМЁННЫМ методом
  `div_rem`, и РЕАЛЬНУЮ миграцию `nova-bignum`'s `BigInt` на named tuple —
  см. ниже) — печать НИ РАЗУ не сработала. Вызов `.div_rem()` на named-tuple
  ресивере ВСЕГДА резолвится через раннюю, специфичную по типу ветку
  (`emit_call`'s «5. User-defined method call», ~emit_c.rs:42112+,
  `debt_strip_recv_c_prefix` уже корректно снимает `NovaTuple_`), single-key
  fallback до него не доходит.
- Т.е. диагноз «резолв `.div_rem()` уезжает в чужой тип» ОПРОВЕРГНУТ —
  сам вызов `.div_rem()` резолвится верно во всех проверенных формах.

### Настоящий корень

Дефект — НЕ в резолве самого `.div_rem()`, а НИЖЕ ПО ТЕЧЕНИЮ: значение
named-tuple типа, полученное ИЗ такого вызова (или вообще откуда угодно),
не умеет корректно интерполироваться в `"${x}"`/println даже когда у типа
ЕСТЬ `@display`/`@debug`/`.to_str()`.

**Место (было):** `compiler-codegen/src/codegen/emit_c.rs`, функция
`debt_strip_value_prefix_or_nova_trim_start` — до фикса только
`NovaValue_`/`Nova_`. `"NovaTuple_X"` не матчит НИ ОДИН из двух префиксов
(`"NovaTuple_"`'s 5-й байт — `T`, не `_`), поэтому helper возвращал СЫРУЮ,
нестрипнутую строку `"NovaTuple_X"`. Эта функция использовалась В ДВУХ
местах `emit_interpolated_str` (резолв `@display`/`@debug`; D410
`to_str()`-фоллбэк) как КЛЮЧ поиска в `all_methods`/`method_overloads`,
которые зарегистрированы под ГОЛЫМ Nova-именем типа (`"X"`, не
`"NovaTuple_X"`) — промах лукапа ВСЕГДА, даже когда тип реально объявляет
метод. Итог: `${x}` для named-tuple значения падало в LAST-RESORT
numeric-cast фоллбэк (`nova_int_to_str((nova_int)(v))`) — хард CC-FAIL для
by-value структуры (`operand of type 'NovaTuple_X' ... where arithmetic or
pointer type is required`), а НЕ silent garbage (в отличие от закрытого
`[M-interp-numeric-fallback-silent-garbage]` — там были heap-record'ы,
`Nova_X*`, указатель молча кастуется в число).

Чекер-сторона (`interp_display_via_str_from_or_to_str`/`find_method_decl`,
types/mod.rs) УЖЕ корректно резолвит named-tuple тип по имени и пропускает
`${x}` (clean `nova check`) — дефект был ЧИСТО в кодогене, редундантном
пере-резолве той же информации из C-строки вместо чтения готового резолва.

### Бисекция оригинального репро — восстановлена, с уточнением

Формулировка окна p-op-channel «D215-кортеж + `.div_rem()` + StringBuilder
в CU» — верна КАК НАБОР ФАКТОРОВ, но `.div_rem()` — не точка мисрезолва, а
ИСТОЧНИК значения; StringBuilder — не случайная примесь, а сам МЕХАНИЗМ:
`emit_interpolated_str` строит результат ЧЕРЕЗ `Nova_StringBuilder_method_
append` БЕЗУСЛОВНО (для ЛЮБОЙ интерполяции, даже когда пользователь ни разу
не называет `StringBuilder`), а значит КАЖДАЯ `${x}`-интерполяция named-
tuple значения — потенциальный триггер. Оригинальная бисекция (окно
p-tuple-migration, 2026-08-01) не сохранила репро-артефакт (ветка без
коммита) — этот вывод получен независимой реконструкцией.

### Фикс — ФИНАЛЬНАЯ канальная форма

**Чекер** (`compiler-codegen/src/types/mod.rs`, `f1_expr`'s
`ExprKind::InterpolatedStr` арм): для каждого `InterpStrPart::Expr` после
`check_interp_no_display`/`check_interp_no_debug` (которые УЖЕ вызывают
`resolve_interp_user_value_type(e, gs, scope)` для своей диагностики) —
тот же резолв ПОВТОРНО вызывается и результат (бare Nova-имя типа, когда
это non-generic Record/Sum/NamedTuple/Newtype) пишется в существующий
канал `resolved_types_buf[e.id]` как `ResolvedType::Named{name, module:
vec![], args: vec![]}` (только если запись ещё не занята — `.entry().
or_insert(...)`, не перетирает более специфичный резолв).

**Кодоген** (`emit_c.rs`): новый helper `channel_named_type(&self, id) ->
Option<String>` читает `self.resolved_types[id]`, разворачивает
`ResolvedType::Named{name, args, ..}` **только если `args.is_empty()`**
(см. «Дефект B» ниже — почему это критично), и **валидирует результат
против `self.record_schemas.contains_key(name)`** (см. «Дефект A» —
почему валидация обязательна). Оба места `emit_interpolated_str`
(display/debug-резолв и D410 to_str-фоллбэк) теперь читают
`self.channel_named_type(e.id).unwrap_or_else(|| Self::debt_strip_value_
prefix_or_nova_trim_start(&arg_ty))` — канал первым приоритетом, старый
C-строковый хелпер (**вернул в ИСХОДНУЙ вид, без добавленной `NovaTuple_`
ветки** — она больше не нужна) остаётся fallback'ом байт-в-байт на всё,
что канал не покрывает.

### Ход канального перевода — три смежных дефекта, вскрытых по дороге

Задание интегратора прямо предупреждало: «если канал не заполняется —
это и есть настоящий корень, чини его». По дороге канал СНАЧАЛА
заполнился НЕПРАВИЛЬНО трижды — каждый раз чужим (не относящимся к
данному вызову) значением, и КАЖДЫЙ раз пришлось разбираться, почему.

**Дефект A — `Channel.new()` заражает канал.** Первая версия фикса №145
п.3 (см. ниже) сразу дала ICE (`nova: internal error ... [P67-LEGACY]
method call \`.send\` return type unknown`) на существующей фикстуре
`neg/channel_elem_turbofish_str_payload_neg.nv` (`ro { tx, rx } =
Channel[str].new(4)`). Инструментировал: `resolved_types_buf[d.value.id]`
для ЭТОГО вызова уже содержал `Named{name: "Channel"}` — `Channel[T]` НЕ
compiler-only intrinsic, как гласит комментарий в самом коде (`external
type Channel[T]`), а ОБЫЧНОЕ (хоть и compiler-recognized) имя, для
которого какой-то ДРУГОЙ, уже существующий механизм резолва вызовов
(инференс типа `Type.method(...)` call-выражения) наивно пишет в канал
«тип = имя перед точкой», не проверяя, зарегистрирован ли `Channel.new`
как настоящий конструктор. Хардкоженная схема `record_schemas
["ChannelPair"]` (используемая нормальным путём для этой деструктуризации)
осталась НЕТРОНУТОЙ — просто мой новый код перехватывал раньше с ложным
именем `"Channel"`. **Фикс:** валидация `record_schemas.contains_key(name)`
до того, как доверять каналу — `"Channel"` там не зарегистрирован (only
"ChannelPair" is), фильтр отсеивает, падаем в исходный (рабочий) fallback.

**Дефект B — generic `Vec[T]` заражает канал чужим (шаблонным) именем.**
После фикса A мега-CU показал ОДИН оставшийся FAIL:
`d422_generic_interp_dispatch.nv` — `${v}` на `Vec[int]` перестал
дисптчиться в СОБСТВЕННЫЙ `@display` вектора. Причина: `Vec[T]` — РЕАЛЬНО
объявленный generic-record (`std/src/collections/vec/core.nv:67`,
`export type Vec[T] priv { ... }`), поэтому `record_schemas` содержит
ГОЛОЕ имя `"Vec"` (шаблона), пройдя фильтр из Дефекта A. Но
`try_generic_mono_interp_dispatch` (существующий механизм, закрывший
№208/D422 ДО этого окна) требует МОНО-МАНГЛЕННОЕ имя инстанса
(`"Vec____nova_int"`), а не голое имя шаблона — с `"Vec"` вместо
моно-имени он не находит инстанс и падает мимо. **Фикс:** канал доверяем
ТОЛЬКО когда `ResolvedType::Named.args.is_empty()` — для `Vec[int]`
резолвленный тип несёт `args: [int]` (непустой), и это ОТСЕКАЕТ generic
случаи структурно, не строкой; для НЕ-generic named tuple/record
(`args` всегда пуст) канал по-прежнему срабатывает.

**Дефект C — `match_arm_bindings` расширение регрессировало 17 файлов
(ОТКАЧЕНО).** Прежде чем найти Дефект B, пытался закрыть ДРУГОЙ пробел:
для №248's фикстуры в форме `match a.div_rem(b) { Ok((q, r)) => "${q}" }`
канал не заполнялся вовсе — `match_arm_bindings` (types/mod.rs) умел
биндить ТОЛЬКО `Ok(x)` (одиночный bare-идентификатор), не `Ok((q, r))`
(вложенный tuple-паттерн в единственной payload-позиции) — `scope` не
получал тип для `q`/`r`, чекер не мог резолвить их для канала. Расширил
`match_arm_bindings` рекурсией в `Pattern::Tuple` — мега-CU СРАЗУ дал
**17 CC-FAIL** в НИКАК не связанных файлах (`vr_binop_arith_dce`,
`supervisor_stop_test`, `a_q3_println_debug_record`, и другие — все
«passing int to nova_str»). Корень: `match_arm_bindings`'s `scope`-запись
используется ШИРОКО, не только моим каналом — для GENERIC тел (напр.
`Some((k, v)) => ...` внутри `HashMap`'s ОБЩЕГО generic-метода, где `K`/`V`
ещё TypeParam, не конкретный тип) расширение писало в `scope` СЫРОЕ имя
типа-параметра (`"K"`), и это ЛОЖНОЕ значение утекало через ДРУГИЕ, уже
существующие потребители `scope`, ломая несвязанные интерполяции при
мономорфизации. **Решение: ОТКАТИЛ `match_arm_bindings` целиком назад к
исходному виду** (слишком широкий, непредсказуемый blast radius для
окна с оставшимся бюджетом) — задокументировал как отдельный, НЕ
исправленный в этом окне пробел (см. «Смежные находки»), и вместо
match-arm-формы переписал позитивную фикстуру №248 на `??`-форму
(`ro (q, r) = a.div_rem(b) ?? panic(...)`), которая проходит через
`Stmt::Let`'s СУЩЕСТВУЮЩУЮ (уже корректную) регистрацию типов
tuple-let-биндинга — тот же «значение из Result-of-tuple метода»
смысл бисекции, форма, которую канал ДЕЙСТВИТЕЛЬНО покрывает.

### Прогоны (дословно)

Изолированная фикстура (main.nv, `Frac(num int, den int)` с `@to_str()`
через StringBuilder + `@div_rem()`, `println("q=${q}")` где `q` —
результат `div_rem`):
- **ДО фикса** (nova.exe с main HEAD): `error: operand of type 'NovaTuple_Frac'
  ... where arithmetic or pointer type is required` (2 ошибки, ровно
  цитируемый в бэклоге класс).
- **ПОСЛЕ фикса**: `built: ... main.exe`, запуск — `q=5/3` / `r=0/0`
  (корректно).

Реальная миграция nova-bignum (см. «Побочная проверка» ниже) — краш
`operand of type 'NovaTuple_BigInt' where arithmetic or pointer type is
required` в `bigdecimal/core.c`/`bigfloat/core.c`/`bigrat/core.c` **исчез
после фикса** (осталась ДРУГАЯ, несвязанная ошибка — см. смежные находки).

Фикстуры (позитив+негатив), в `spec_tests/conformance`:
- `m248_named_tuple_interp_display_dispatch_pos.nv` — 3 test-блока
  (to_str()-форма; Result-of-tuple значение через `??`-unwrap let-форму —
  переписана с match-арма на эту форму после Дефекта C, см. выше;
  `#impl(Display)`-форма).
- `neg/m248_named_tuple_interp_no_display_neg.nv` — EXPECT_COMPILE_ERROR
  `E_INTERP_NO_DISPLAY` (чекер-гейт НЕ ослаблен фиксом).

Standalone-прогон (изолированные каталоги, финальная сборка):
```
PASS  m248_named_tuple_interp_display_dispatch_pos
PASS  m248_named_tuple_interp_no_display_neg  (negative)
```

---

## №145 [M-named-tuple-three-gaps]

### (1) Спека противоречит себе (D215/D222 vs D102)

**НЕ трогал `spec/` сам** (зона интегратора). D215 (`02-types.md:4702,4711`)
и D222 §2 (`02-types.md:11642`) показывают `Vec3(x: 1.0, y: 2.0, z: 3.0)` —
именованные аргументы для полей БЕЗ дефолтов, что прямо противоречит D102
(`03-syntax.md:5441`: «параметр с дефолтом передаётся только по имени,
позиционно — нельзя» + «обязательный — позиционно, опциональный — по
имени»). Канон (подтверждён владельцем 2026-07-27, framing самого бэклога):
**позиционное конструирование** (`Vec3(1.0, 2.0, 3.0)`).

**Требуемый D-амендмент** (для интегратора):
- `spec/decisions/02-types.md:4702` и `:4711` — заменить `Vec3(x: 1.0, y: 2.0, z: 3.0)`
  на `Vec3(1.0, 2.0, 3.0)` (и симметрично в теле `@plus`-примера).
- `spec/decisions/02-types.md:11642` (D222 §2, «Init via named-arg
  constructor») — переформулировать: пример должен показывать named-arg
  ТОЛЬКО для полей С дефолтом; для без-дефолтных — позиционно.
- В D221/D222 (или отдельным пунктом) — **дописать правило деструктуризации
  named tuple**, которого сейчас нет НИ В ОДНОМ D-блоке (D221 §7 сам это
  признаёт: «D221 covers ONLY record form, tuple-pattern priv в D222» — но
  D222 тоже не формулирует правило destructure-формы, только priv-access
  внутри уже-существующей формы): **круглая скобка — по позиции, фигурная —
  по имени; именованный кортеж деструктурируется ТОЛЬКО фигурной формой**
  (`{ x, y }`), позиционная (`(a, b)`) на именованном кортеже — ошибка
  компиляции со ссылкой на канон. Частичный разбор — как у записей
  (D411: обязателен явный `..`, если перечислены не все поля).

### (2) D102 не энфорсился для named-tuple конструктора — БЕЗ ИЗМЕНЕНИЙ с прошлой ревизии отчёта

**Место:** `compiler-codegen/src/types/mod.rs`, `f5_check_tuple_construct`.
Измерено ДО фикса: `Complex(re: 0.0, im: 1.0)` для `Complex(re f64,
im f64)` (без дефолтов) — PASS 1/0 (компилятор принимал то, что спека
запрещает).

**Фикс (чекер-канал, `types/mod.rs`, зона принята интегратором «сохрани
как есть», НЕ трогал):** в цикле по `CallArg::Named` внутри
`f5_check_tuple_construct`, после существующей проверки «поле существует»,
добавлена проверка «поле имеет дефолт» — иначе
`[E_TUPLE_NAMED_ARG_NO_DEFAULT]`. Именованные аргументы для полей С
дефолтом (и позиционная форма — всегда) не затронуты.

**Прогон (дословно):**
```
Complex(re: 0.0, im: 1.0), нет дефолтов:
  error: [E_TUPLE_NAMED_ARG_NO_DEFAULT] named tuple `Complex`'s field `re`
  has no default — ...  (и на `im` тоже)

Complex(0.0, 1.0) позиционно + WithDefault(1)/(1, b: 9):
  ok: ... PASS: 1  FAIL: 0

WithDefault(a: 1, b: 9) — `a` без дефолта:
  error: [E_TUPLE_NAMED_ARG_NO_DEFAULT] ... field `a` ...
```

**Миграция сайтов** (canon = позиционно): найдены и мигрированы ВСЕ 11
файлов `spec_tests/conformance`, где D102 ловил старую именованную форму
без дефолтов (первый мега-CU-прогон поймал 1 пропущенный сайт —
`t4_defaults.nv` — добавлен в список вторым проходом):
`d215_named_tuple_value.nv`, `m139_generic_value_type_struct_field_pos.nv`,
`named_tuple_singleline_ok.nv`, `p1_mut_binding_member_chain_mut_method_ok.nv`,
`p2_mut_self_field_mut_method_ok.nv`, `p3_mut_binding_index_chain_mut_method_ok.nv`,
`p5_rvalue_base_mut_no_op.nv`, `t1_basic_named_tuple.nv`, `t2_types.nv`,
`t3_methods.nv`, `t4_defaults.nv`. `std/src` — 0 находок (проверено `nova
check std/src`, канон 148/26/61 не сдвинулся). `examples/`,
`nova-polaris`/`nova-http`/`nova-tls`/`nova-bignum`/`nova-compress` —
грепом (объявление named-tuple типа) 0 находок, риска нет.

### (3) Деструктуризации не было вовсе

**Место (было):** `compiler-codegen/src/codegen/emit_c.rs`,
`pattern_bind_typed`'s `Pattern::Record`-арм. `type_name_from_path`'s
безымянный (`{ x, y }` без явного type_path) fallback резолвил имя типа
из C-типа скрутини через `debt_strip_nova_prefix_or_empty`
(`s.strip_prefix("Nova_").unwrap_or("")`) — для `"NovaTuple_X"` эта
функция ВСЕГДА возвращала пустую строку (тот же класс промаха, что и
№248 — 5-й байт `T`≠`_`), и `record_schemas.contains_key("")` промахивался,
хотя named tuples РЕАЛЬНО зарегистрированы в `record_schemas`
(`emit_named_tuple_type`, УЖЕ существующий код, не новый). Ветка
проваливалась в «sum-type record variant» фоллбэк и эмитила
`scr->payload.Variant.field` — бессмысленный доступ к структуре без поля
`payload` (измеренный CC-FAIL: «member reference type 'NovaTuple_P2' is
not a pointer... no member named payload» — дословно как в реестре).

**Фикс — ФИНАЛЬНАЯ канальная форма.** `pattern_bind_typed` получил новый
параметр `scr_nova_type: Option<&str>` (13 call-site'ов обновлены — 12
передают `None` — нет применимого канала для их сценариев, byte-parity с
прежним поведением; ТОЛЬКО `emit_record_destructure` — вызывающая функция
для `Stmt::Let`'s `Pattern::Record` — передаёт РЕАЛЬНОЕ значение из
`self.channel_named_type(decl.value.id)`, тот же валидированный (§ Дефект
A/B выше) helper, что и №248). Чекер (`f1_stmt`'s `Stmt::Let`-ветка)
пишет `resolved_types_buf[d.value.id]` **ТОЛЬКО когда scrutinee-тип —
ЗАДЕКЛАРИРОВАННЫЙ `TypeDeclKind::NamedTuple`** (`self.types.get(name)`
проверка — узкий скоуп, специально НЕ общий «любой Named-тип», чтобы не
плодить лишние source-записи для чекер-канала, раз кодогену узко нужны
только named tuples здесь). `Pattern::Record`'s `type_name_from_path`
теперь: явный `type_path` → канал-хинт (только если валиден) → исходный
(нетронутый) C-строковый фоллбэк.

Отдельно — позиционная форма `(a, b)` на именованном кортеже: канон
владельца требует явной ошибки, а не тихого прохода в легаси-кодоген.
**Место (чекер-канал, без изменений):**
`compiler-codegen/src/types/mod.rs`, новая функция
`check_positional_destructure_on_named_tuple`, вызывается из `f1_stmt`'s
`Stmt::Let`-ветки. Если scrutinee — `TypeDeclKind::NamedTuple` и pattern —
`Pattern::Tuple`, эмитит `[E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE]` со
ссылкой на канон.

**Прогон (дословно):**
```
P2(x int, y int); `ro { x, y } = p`:
  ok: ...  PASS: 1  FAIL: 0
  (запуск): 34   -- x=3,y=4, println(x,y) без разделителя

P2; `ro { y, .. } = p` (частичный разбор с явным `..`):
  ok: ...  (запуск): 4

P2; `ro { y } = p` (частичный БЕЗ `..`):
  error: [E_RECORD_PATTERN_NEEDS_REST] record-pattern binding lists 1 of 2
  field(s) of `P2` without `..` — ... (D411)
  -- Правило партиального разбора для named tuple — ТО ЖЕ, что у записей
  -- (D411, существовавший до этого окна код); частичный разбор БЕЗ явного
  -- `..` не проходит НИ у записей, НИ теперь у именованных кортежей.
  -- Брифовая формулировка «ro { y } = p берёт только нужные поля» читаю
  -- как «частичный разбор возможен», а конкретный синтаксис (нужен ли `..`)
  -- задаёт уже действующий D411 — единообразно с записями, а не как новое
  -- исключение.

P2; `ro (a, b) = p` (позиционная форма на именованном кортеже):
  error: [E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE] named tuple `P2` cannot be
  destructured with the POSITIONAL form `(…)` — it has field names...

Regression: обычный позиционный tuple (`(int, int)` из fn-возврата, НЕ
named-tuple) — `ro (a, b) = make()` — ok: ... PASS: 1 FAIL: 0 (не задет).

Дополнительно (нашлось при отладке Дефекта A): `ro { tx, rx } =
Channel[str].new(4)` — codegen НЕ регрессирует (валидация против
record_schemas отсеивает ложный канал-хинт "Channel", исходный путь через
хардкод "ChannelPair" отрабатывает как раньше). Подтверждено фикстурой
`neg/channel_elem_turbofish_str_payload_neg.nv` (уже существующая,
несвязанная с этим окном) — зелёная в финальном мега-CU.
```

Фикстуры (в `spec_tests/conformance`):
- `m145_named_tuple_destructure_and_d102_pos.nv` — 5 test-блоков (позиционное
  конструирование; именованный арг на дефолтном поле; полный `{ x, y }`;
  частичный `{ y, .. }`; переименование `{ re: real_part }`).
- `neg/m145_named_tuple_named_arg_no_default_neg.nv` — EXPECT_COMPILE_ERROR
  `E_TUPLE_NAMED_ARG_NO_DEFAULT`.
- `neg/m145_named_tuple_positional_destructure_neg.nv` — EXPECT_COMPILE_ERROR
  `E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE`.
- `neg/m145_named_tuple_partial_destructure_needs_rest_neg.nv` —
  EXPECT_COMPILE_ERROR `E_RECORD_PATTERN_NEEDS_REST`.

Standalone-прогон (финальная сборка, все 4 фикстуры разом):
```
PASS  m145_named_tuple_destructure_and_d102_pos
PASS  m145_named_tuple_named_arg_no_default_neg  (negative)
PASS  m145_named_tuple_positional_destructure_neg  (negative)
PASS  m145_named_tuple_partial_destructure_needs_rest_neg  (negative)
```

---

## Место фикса — §0 канал-дисциплина: ВЫДЕРЖАНА

№145(2) (D102-энфорс) и №145(3)-positional-guard были в чекер-канале с
первой ревизии — интегратор их принял «сохрани как есть». №248 и
№145(3)-деструктуризация **переведены на канал в этом заходе** (были
легаси-строковыми фиксами, за что первая ревизия вернула работу):

- Чекер (`types/mod.rs`) пишет `resolved_types_buf[expr.id]` = резолвленный
  Nova-тип (не C-строка) в ДВУХ узких, целевых местах: интерполируемое
  выражение (№248) и `Stmt::Let`'s `Pattern::Record`-скрутини, когда это
  именованный кортеж (№145 п.3).
- Кодоген (`emit_c.rs`) читает канал ПЕРВЫМ приоритетом через один общий
  валидирующий helper `channel_named_type` (не наращивает per-синтаксис
  разбор — читает готовый резолв); C-строковые `debt_strip_*`-хелперы
  **вернулись к исходному, дофиксовому виду** (без добавленной
  `NovaTuple_`-ветки) и остались ЧИСТО fallback'ом на случай, когда канал
  ничего не резолвил (byte-parity со всем, что канал не покрывает).
- `pattern_bind_typed` получил параметр `scr_nova_type` — механическая
  правка сигнатуры (13 call-site'ов, 12 передают `None`), не новая
  dispatch-логика.

**Итог по строкам:** `arch-ratchet` — `lines=64545 <= 64545` (ровно
канон), `infer=348 <= 348` (не сдвинулся). Достигнуто НЕ переносом строк
в другой файл, а тем, что канал реально снял НЕОБХОДИМОСТЬ в
C-строковом разборе для покрытых каналом случаев — единственные строки,
оставшиеся в `emit_c.rs`, это (а) сигнатура `pattern_bind_typed` +12
site'ов `, None`/`, scr_nova_type` (почти без роста — заменяют
существующие вызовы), (б) один общий `channel_named_type`-helper
(3 строки), (в) две точки чтения канала в `emit_interpolated_str` и одна
в `emit_record_destructure` (каждая заменяет СТАРУЮ строку на канал+
фоллбэк той же длины).

**Остаточный факт, не скрываю:** валидирующая логика внутри
`channel_named_type` (проверка `record_schemas` + `args.is_empty()`)
— это НЕ «наращивание разбора C-типа», а страховка от заражения
ШИРОКО РАЗДЕЛЯЕМОГО канала `resolved_types` посторонними записями
(см. Дефекты A/B выше) — но она ЖИВЁТ в `emit_c.rs` и стоит несколько
строк. Альтернатива — заводить ОТДЕЛЬНЫЙ, не разделяемый ни с кем канал
специально под эти два потребителя — была рассмотрена и отклонена: она
устранила бы саму ВОЗМОЖНОСТЬ заражения архитектурно (сильнее), но
стоила БЫ БОЛЬШЕ строк в `emit_c.rs` (новое поле CEmitter + два
`set_*`-вызова в `main.rs`/`test_runner.rs`, которые НЕ ратчатся, но
самому полю и его инициализации в CEmitter всё равно нужна строка) —
при бюджете «lines<=64545 РОВНО» выбрал более дешёвый по строкам путь.
Если интегратор сочтёт этого недостаточно — предложение по выделенному
каналу изложено, его легко реализовать отдельным маленьким шагом.

---

## Смежные находки (НЕ чинил, номер присвоит интегратор)

### 1. `+=`/compound-assign на named-tuple не десугарится

При прогоне реальной миграции nova-bignum (см. ниже) вскрыт ОТДЕЛЬНЫЙ,
несвязанный дефект: `mant += BigInt.one()` (где `mant` — named-tuple
`BigInt`, у которой ЕСТЬ операторный метод `@plus`) эмитится КАК СЫРОЙ C
`+=` на структуре — `mant += Nova_BigInt_static_one();` — CC-FAIL
(«invalid operands to binary expression»). Тот же КЛАСС, что уже закрытый
**№284** («+=/-=/битовые на VALUE-record с операторным методом не
десугарились»), но №284's фикс покрыл ТОЛЬКО `NovaValue_*`-ABI — named-
tuple (`NovaTuple_*`) осталась вне десугар-блока.

### 2. `Nova_BigInt_method_equal` conflicting types — уже известный маркер

Реальная миграция nova-bignum также воспроизвела `[M-static-selfreturn-
value-mangle-conflict]` (P2, OPEN, `backlog-followups.md` ~2780) — НЕ
новая находка, уже задокументирован (найден окном `p-tuple`, 2026-08-02,
bfd1194 в nova-bignum). Подтверждаю его актуальность на текущем HEAD.

### 3. `match_arm_bindings` не биндит вложенный tuple-паттерн в variant-payload

Новая находка ЭТОГО захода (Дефект C выше): `match x { Ok((a, b)) => ... }`
— `a`/`b` НИКОГДА не попадают в чекерский `scope` с конкретным типом
(`match_arm_bindings`, types/mod.rs, обрабатывает ТОЛЬКО одиночный
`Ok(bare_ident)`). Практических последствий для СУЩЕСТВУЮЩЕГО кода это не
имеет (кодоген деструктурирует такие паттерны своим, независимым от
`scope` путём) — но БЛОКИРУЕТ РАСШИРЕНИЕ любого будущего чекер-канала,
которому нужен тип pattern-bound имени внутри такого arm'а (ровно как
№248's canal попытался, до отката). Пробовал расширить — 17-файловая
регрессия (генерик-тела типа HashMap-итерации получают в `scope` СЫРОЕ
имя типа-параметра вместо конкретного типа при мономорфизации,
просачивается в НЕСВЯЗАННЫЕ интерполяции). Правильный фикс — не
"биндить `scope`", а сначала понять/задокументировать ВСЕХ существующих
потребителей `scope`, которых `match_arm_bindings`'s запись затрагивает
(шире одного окна).

---

## Побочная проверка: реальная миграция nova-bignum (тестовый носитель, НЕ часть задания)

Чтобы получить репро с полноценным compile-unit (не изолированный файл —
брифовое требование п.3 для №248), смигрировал `BigInt` в отдельном
worktree `nova-bignum` (`d:/Sources/nv-lang/nova-bignum-p248t`, ветка
`p248t-repro`, НЕ коммичена в основную репу nova-bignum — черновой тестовый
носитель, УДАЛЕНА после проверки) с `type BigInt value {...}` на `type
BigInt(sign Sign, limbs []u32)` — механическая правка ~15 сайтов
конструирования (тот же приём, что описан в двух прошлых попытках
миграции, `p-tuple-migration`/`p-tuple`).

**ДО фикса №248:** `nova test src` — CC-FAIL на 5 целях (bigint/
bigdecimal_test/bigfloat_test/bigrat_test/repro_parse_test), симптом —
дословно цитируемый в бэклоге `operand of type 'NovaTuple_BigInt' where
arithmetic or pointer type is required` (в `bigdecimal/core.c`,
`BigDecimal.to_str()`'s `"${int_part}"`, где `int_part` — `BigInt` из
`@div_rem(...)`).

**ПОСЛЕ фикса №248:** этот КЛАСС ошибки исчез из `bigdecimal/core.c`
целиком (CC-FAIL на этом файле теперь — ТОЛЬКО уже известный `Nova_BigInt_
method_equal` conflict, см. смежную находку 2). `bigfloat`/`bigrat`/
`repro_parse_test` — вскрыли смежную находку 1 (`+=` на named-tuple) как
СЛЕДУЮЩИЙ блокер (не чинил, вне периметра).

Worktree удалён после проверки (не мержил, не коммитил в nova-bignum).

---

## Финальные гейты (после перевода на канал, дословно)

**`cargo build --release`** (из `nova-cli/`) — чисто:
```
Finished `release` profile [optimized] target(s) in 0.44s
```
(кэш валиден с последней чистой пересборки — warnings только pre-existing,
не мои).

**`bash scripts/guards/arch-ratchet.sh`**:
```
arch-ratchet ok: lines=64545 <= 64545
arch-ratchet ok: infer=348 <= 348
```
**ЗЕЛЁНЫЙ.** Ровно канон, не «под каноном» — использован весь бюджет,
который был (baseline в этом дереве действительно `64545`, канон брифа
называл `64542` — расхождение унаследовано от базы, не моя находка).

**`nova check std/src`**:
```
PASS: 148  FAIL: 26  WARN: 61
```
Байт-в-байт канон, не сдвинулся.

**Мега-CU (`nova test --positive --compile-error spec_tests/conformance
--toolchain clang`)** — финальный прогон, дословно:
```
PASS: 657  FAIL: 0  SKIP: 68 (skipped)
```
Не ниже достигнутого в прошлой ревизии (657/0/68 — совпадает). По дороге
к этому результату прогон ловил регрессии ТРИЖДЫ (Дефекты A/B/C выше) —
каждая зачинена или откачена ДО этого финального прогона, ни одна не
осталась замаскированной.

Флагман (`examples/flagship/aggregator --strict-effects`) — по правилу
брифа, эту волну не гоняла, гонит интегратор при приёмке.

### Сводка по пунктам задания

| # | Пункт | Статус |
|---|---|---|
| 1 | №248 фикс + доказательство | ✅ канал, фикстуры + мега-CU зелёные |
| 2 | №145 (1) спек-противоречие | НЕ трогал spec/ (зона интегратора) — текст амендмента дан выше |
| 3 | №145 (2) D102-энфорс + миграция | ✅ канал (без изменений от прошлой ревизии), 11 сайтов мигрированы |
| 4 | №145 (3) деструктуризация + запрет позиционной | ✅ канал, все три формы подтверждены прогонами |
| 5 | Фикс в чекер-канале (§0) | ✅ ВЫДЕРЖАНА — arch-ratchet зелёный |
| 6 | `cargo build --release` чисто | ✅ |
| 7 | `nova check std/src` = 148/26/61 | ✅ байт-в-байт |
| 8 | arch-ratchet | ✅ `lines=64545<=64545`, `infer=348<=348` |
| 9 | Мега-CU | ✅ 657/0/68 |
| 10 | Флагман | НЕ гонял (правило брифа — гонит интегратор) |

**Не заявляю «№248/№145 закрыты» как абсолют** — заявляю: конкретные,
воспроизводимые дефекты найдены, зафиксированы КОНКРЕТНЫМ местом в коде
через чекер-канал, все прогоны (изолированные фикстуры + мега-CU +
std-check + arch-ratchet) подтверждают фикс явно и дословно. №145 п.1
(спек-текст) сознательно не тронут — зона интегратора, амендмент-текст
дан. Смежная находка №3 (`match_arm_bindings`) — задокументирована, не
исправлена, номер за интегратором.
