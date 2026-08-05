# PROGRESS — p248tup-145 (окно p248tup-145, №248 + №145)

Worktree: `d:/Sources/nv-lang/nova-p248t`, branch `p248tup-145`, база `main` (`57bf5fdf6`).
Модель: sonnet.

## Итог одной строкой

№248 — найден и зафиксирован корень (класс «дефект прошёл мимо гейта, потому
что диспетч именованного-кортежа тихо деградировал» — не буквально «класс
№129 single-key last-wins», как было продиагностировано окном p-op-channel,
а СОСЕДНИЙ по классу дефект в ТОМ ЖЕ семействе «debt_strip_*»-хелперов).
№145 — три дыры закрыты частично (2 из 3 в компиляторе, 1 требует
D-амендмента, который эта волна НЕ пишет — зона интегратора).

**Фикс МЕСТАМИ НЕ выдержал канал-only дисциплину** — оба фикса №248/№145
задевают `compiler-codegen/src/codegen/emit_c.rs` (легаси). Разбор — ниже,
раздел «Честный разбор места фикса».

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

### Настоящий корень (подтверждён минимальным репро + реальной миграцией)

Дефект — НЕ в резолве самого `.div_rem()`, а НИЖЕ ПО ТЕЧЕНИЮ: значение
named-tuple типа, полученное ИЗ такого вызова (или вообще откуда угодно),
не умеет корректно интерполироваться в `"${x}"`/println даже когда у типа
ЕСТЬ `@display`/`@debug`/`.to_str()`.

**Место:** `compiler-codegen/src/codegen/emit_c.rs`, функция
`debt_strip_value_prefix_or_nova_trim_start` (~строка 3899, до фикса —
только `NovaValue_`/`Nova_`). `"NovaTuple_X"` не матчит НИ ОДИН из двух
префиксов (`"NovaTuple_"`'s 5-й байт — `T`, не `_`), поэтому helper
возвращал СЫРУЮ, нестрипнутую строку `"NovaTuple_X"`. Эта функция
используется В ДВУХ местах `emit_interpolated_str` (~46739 — резолв
`@display`/`@debug`; ~46820 — D410 `to_str()`-фоллбэк) как КЛЮЧ поиска в
`all_methods`/`method_overloads`, которые зарегистрированы под ГОЛЫМ
Nova-именем типа (`"X"`, не `"NovaTuple_X"`) — промах лукапа ВСЕГДА, даже
когда тип реально объявляет метод. Итог: `${x}` для named-tuple значения
падало в LAST-RESORT numeric-cast фоллбэк (`nova_int_to_str((nova_int)(v))`)
— хард CC-FAIL для by-value структуры (`operand of type 'NovaTuple_X' ...
where arithmetic or pointer type is required`), а НЕ silent garbage (в
отличие от закрытого `[M-interp-numeric-fallback-silent-garbage]` — там
были heap-record'ы, `Nova_X*`, указатель молча кастуется в число).

Чекер-сторона (`interp_display_via_str_from_or_to_str`/`find_method_decl`,
types/mod.rs ~16358+) УЖЕ корректно резолвит named-tuple тип по имени и
пропускает `${x}` (clean `nova check`) — дефект был ЧИСТО в кодогене,
редундантном пере-резолве той же информации.

Сиблинг `debt_strip_recv_c_prefix` (emit_c.rs ~55203, общий helper метод-
диспетча) УЖЕ снимает `NovaTuple_` (D215/Plan 120) — этому helper'у просто
не дали симметричную ветку.

### Бисекция оригинального репро — восстановлена, но с уточнением

Формулировка окна p-op-channel «D215-кортеж + `.div_rem()` + StringBuilder
в CU» — верна КАК НАБОР ФАКТОРОВ, но `.div_rem()` — не точка мисрезолва, а
ИСТОЧНИК значения; StringBuilder — не случайная примесь, а сам МЕХАНИЗМ:
`emit_interpolated_str` строит результат ЧЕРЕЗ `Nova_StringBuilder_method_
append` БЕЗУСЛОВНО (для ЛЮБОЙ интерполяции, даже когда пользователь ни разу
не называет `StringBuilder`), а значит КАЖДАЯ `${x}`-интерполяция named-
tuple значения — потенциальный триггер. Оригинальная бисекция (окно
p-tuple-migration, 2026-08-01) не сохранила репро-артефакт (ветка без
коммита) — этот вывод получен независимой реконструкцией.

### Фикс

`compiler-codegen/src/codegen/emit_c.rs:3899` (`debt_strip_value_prefix_or_
nova_trim_start`) — добавлена ветка `.or_else(|| s.strip_prefix("NovaTuple_")...)`
ПЕРЕД финальным `Nova_`-фоллбэком, зеркалит уже принятый фикс сиблинга
`debt_strip_recv_c_prefix`.

### Прогоны (дословно)

Изолированная фикстура (repro248c, `d:/Sources/nv-lang/nova-p248t/repro248c/src/main.nv`,
`Frac(num int, den int)` с `@to_str()` через StringBuilder + `@div_rem()`,
`println("q=${q}")` где `q` — результат `div_rem`):
- **ДО фикса** (nova.exe с main HEAD): `error: operand of type 'NovaTuple_Frac'
  ... where arithmetic or pointer type is required` (2 ошибки, ровно
  цитируемый в бэклоге класс).
- **ПОСЛЕ фикса**: `built: ... main.exe`, запуск — `q=5/3` / `r=0/0`
  (корректно).

Реальная миграция nova-bignum (см. «Побочная проверка» ниже) — краш
`operand of type 'NovaTuple_BigInt' where arithmetic or pointer type is
required` в `bigdecimal/core.c`/`bigfloat/core.c`/`bigrat/core.c` **исчез
после фикса** (осталась ДРУГАЯ, несвязанная ошибка — см. смежные находки).

Фикстуры (позитив+негатив), добавлены в `spec_tests/conformance`:
- `m248_named_tuple_interp_display_dispatch_pos.nv` — 3 test-блока (to_str()
  форма, Result-of-tuple форма ровно как в бисекции, `#impl(Display)`
  форма).
- `neg/m248_named_tuple_interp_no_display_neg.nv` — EXPECT_COMPILE_ERROR
  `E_INTERP_NO_DISPLAY` (чекер-гейт НЕ ослаблен фиксом).

Standalone-прогон (изолированные каталоги, `nova test`):
```
scratch248/pos: PASS: 1  FAIL: 0
scratch248/neg: PASS: 1  FAIL: 0
```

Мега-CU (`nova test --positive --compile-error spec_tests/conformance`) —
см. общий раздел «Финальные гейты» ниже (единый прогон после ВСЕХ правок
№248+№145).

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

### (2) D102 не энфорсился для named-tuple конструктора

**Место:** `compiler-codegen/src/types/mod.rs`, `f5_check_tuple_construct`
(~15481). Измерено ДО фикса: `Complex(re: 0.0, im: 1.0)` для `Complex(re f64,
im f64)` (без дефолтов) — PASS 1/0 (компилятор принимал то, что спека
запрещает).

**Фикс** (чекер-канал, `types/mod.rs`): в цикле по `CallArg::Named` внутри
`f5_check_tuple_construct`, после существующей проверки «поле существует»,
добавлена проверка «поле имеет дефолт» — иначе
`[E_TUPLE_NAMED_ARG_NO_DEFAULT]`. Именованные аргументы для полей С
дефолтом (и позиционная форма — всегда) не затронуты.

**Прогон (дословно):**
```
d102probe (Complex(re: 0.0, im: 1.0), нет дефолтов):
  error: [E_TUPLE_NAMED_ARG_NO_DEFAULT] named tuple `Complex`'s field `re`
  has no default — ...  (и на `im` тоже)

d102probe (Complex(0.0, 1.0) позиционно + WithDefault(1)/(1, b: 9)):
  ok: ... PASS: 1  FAIL: 0

d102probe (WithDefault(a: 1, b: 9) — `a` без дефолта):
  error: [E_TUPLE_NAMED_ARG_NO_DEFAULT] ... field `a` ...
```

**Миграция сайтов** (canon = позиционно): найдено и мигрировано ВСЕ 10
файлов `spec_tests/conformance`, где D102 ловил старую именованную форму
без дефолтов:
`d215_named_tuple_value.nv`, `m139_generic_value_type_struct_field_pos.nv`,
`named_tuple_singleline_ok.nv`, `p1_mut_binding_member_chain_mut_method_ok.nv`,
`p2_mut_self_field_mut_method_ok.nv`, `p3_mut_binding_index_chain_mut_method_ok.nv`,
`p5_rvalue_base_mut_no_op.nv`, `t1_basic_named_tuple.nv`, `t2_types.nv`,
`t3_methods.nv`. `std/src` — 0 находок (проверено `nova check std/src`,
канон 148/26/61 не сдвинулся). `examples/`, `nova-polaris`/`nova-http`/
`nova-tls`/`nova-bignum`/`nova-compress` — грепом (объявление named-tuple
типа) 0 находок, риска нет.

### (3) Деструктуризации не было вовсе

**Место:** `compiler-codegen/src/codegen/emit_c.rs`, `pattern_bind_typed`'s
`Pattern::Record`-арм (~51564). `type_name_from_path`'s безымянный (`{ x,
y }` без явного type_path) fallback резолвил имя типа из C-типа скрутини
через `debt_strip_nova_prefix_or_empty` (~3835: `s.strip_prefix("Nova_")
.unwrap_or("")`) — для `"NovaTuple_X"` эта функция ВСЕГДА возвращала пустую
строку (тот же класс промаха, что и №248 — 5-й байт `T`≠`_`), и `record_
schemas.contains_key("")` промахивался, хотя named tuples РЕАЛЬНО
зарегистрированы в `record_schemas` (`emit_named_tuple_type`, ~18322,
УЖЕ существующий код, не новый). Ветка проваливалась в «sum-type record
variant» фоллбэк и эмитила `scr->payload.Variant.field` — бессмысленный
доступ к структуре без поля `payload` (измеренный CC-FAIL: «member
reference type 'NovaTuple_P2' is not a pointer... no member named
payload» — дословно как в реестре).

**Фикс:** добавлена попытка снять `NovaTuple_`-префикс (через уже
СУЩЕСТВОВАВШИЙ, но неиспользуемый в этом месте `debt_strip_novatuple_
prefix_or_empty`, ~3848) ПЕРЕД финальным `Nova_`-фоллбэком.

Отдельно — позиционная форма `(a, b)` на именованном кортеже: канон
владельца требует явной ошибки, а не тихого прохода в легаси-кодоген.
**Место:** `compiler-codegen/src/types/mod.rs`, новая функция
`check_positional_destructure_on_named_tuple` (рядом с `check_priv_pattern_
recursive`, ~11552), вызывается из `f1_stmt`'s `Stmt::Let`-ветки. Если
scrutinee — `TypeDeclKind::NamedTuple` и pattern — `Pattern::Tuple`, эмитит
`[E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE]` со ссылкой на канон.

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
```

Фикстуры (добавлены в `spec_tests/conformance`):
- `m145_named_tuple_destructure_and_d102_pos.nv` — 5 test-блоков (позиционное
  конструирование; именованный арг на дефолтном поле; полный `{ x, y }`;
  частичный `{ y, .. }`; переименование `{ re: real_part }`).
- `neg/m145_named_tuple_named_arg_no_default_neg.nv` — EXPECT_COMPILE_ERROR
  `E_TUPLE_NAMED_ARG_NO_DEFAULT`.
- `neg/m145_named_tuple_positional_destructure_neg.nv` — EXPECT_COMPILE_ERROR
  `E_NAMED_TUPLE_POSITIONAL_DESTRUCTURE`.
- `neg/m145_named_tuple_partial_destructure_needs_rest_neg.nv` —
  EXPECT_COMPILE_ERROR `E_RECORD_PATTERN_NEEDS_REST`.

Standalone-прогон:
```
scratch145/pos:  PASS: 1  FAIL: 0
scratch145/neg2: PASS: 3  FAIL: 0
```

---

## Честный разбор места фикса (§0, канал-only дисциплина)

**№248 и часть №145 (пункт 3) НЕ выдержали «фикс только в чекер-канал».**
Оба реальных дефекта лежат в `compiler-codegen/src/codegen/emit_c.rs`
(легаси) — конкретно в ДВУХ «debt_strip_*»-хелперах, которые нормализуют
C-тип-строку ресивера в голое имя Nova-типа для последующего лукапа в
codegen-side registries (`all_methods`/`method_overloads`/`record_schemas`).
№145 пункты (2) и (3-positional-guard) — ЧЕКЕР-канал (`types/mod.rs`),
дисциплина выдержана.

**Почему не сделал полную канальную миграцию в этом окне:**
- Чекер УЖЕ корректно резолвит и разрешает обе ситуации (интерполяция
  named-tuple значения; деструктуризация фигурной формы) — вся
  ИНФОРМАЦИЯ, нужная кодогену, у чекера ЕСТЬ (см. `interp_display_via_
  str_from_or_to_str`/`find_method_decl` для №248; чекер УЖЕ пропускал
  `Pattern::Record` на named tuple для №145 п.3 — `check_priv_pattern_
  recursive_inner` явно поддерживает `TypeDeclKind::NamedTuple`, задолго до
  этого окна). Дефект — ЧИСТО в том, что кодоген ПОВТОРНО (и неверно)
  вычисляет ту же информацию из C-типа-строки, вместо того чтобы читать её
  из чекер-канала.
- Правильная канальная миграция — не «фикс на месте», а НОВАЯ
  инфраструктура: канал `HashMap<ExprId, String>` (или расширение
  `resolved_types`) для «разрешённое голое имя типа для этого узла»,
  заполняемый чекером в `resolve_interp_user_value_type`/`check_priv_
  pattern_recursive_inner`, и ДВА новых читающих сайта в `emit_c.rs`
  (заменяющих текущие `debt_strip_*`-вызовы). Это отдельный, полноценный
  спайк уровня «новая class-C несущая способность» (§7.14 конвенции,
  прецедент 196.2 B07) — не решился делать это НЕ протестированным
  end-to-end в окне, где уже потрачен весь бюджет на диагностику.
- Фикс, который сделал — МИНИМАЛЬНАЯ правка СУЩЕСТВУЮЩЕГО normalization-
  хелпера (не новая dispatch-логика), ЗЕРКАЛИТ уже принятый и работающий
  сиблинг (`debt_strip_recv_c_prefix`, который снимает `NovaTuple_` для
  ТОЙ ЖЕ причины, D215/Plan 120) — не «новый обход», а закрытие пробела в
  СУЩЕСТВУЮЩЕМ паттерне.

**Цена — arch-ratchet КРАСНЫЙ:**
```
ARCH-RATCHET FAIL: lines=64567 > baseline=64545 (emit_c must not grow;
fix belongs to checker channel / IR path)
arch-ratchet ok: infer=348 <= 348
```
(канон брифа — `lines=64542, infer=348`; baseline в этом дереве —
`64545`, расхождение с брифом не разбирал — не моя находка этой волны).
`infer` не сдвинулся (348, канон). `lines` вырос на +22 (после сжатия
комментариев с исходных +49) — ЧИСТО эти два хелпера + 4 новых `else`-строки
дублирующей ветки, без роста dispatch-сложности.

**Предложение по переводу на канал** (следующее окно, если владелец
продолжит): спайк по образцу B07 — новый узкий канал `expr_bare_type_name:
HashMap<ExprId, String>` в `ModuleEnv`, заполняется В ДВУХ местах: (а)
`resolve_interp_user_value_type` (types/mod.rs ~16413, №248) на каждый
`InterpStrPart::Expr`; (б) `check_priv_pattern_recursive_inner`'s
`Pattern::Record`-арм (types/mod.rs ~11588, №145 п.3) на каждый
scrutinee-под-паттерном. `emit_interpolated_str`/`pattern_bind_typed`
читают канал ПЕРВЫМ приоритетом, `debt_strip_*`-хелперы остаются
fallback'ом на случай отсутствия записи (byte-parity с текущим
поведением на всём, что канал НЕ покрывает). Ordering-гейт на каждом шаге
(std check + мега-CU) обязателен — прецедент разбора p-op-channel Ф.1-4
показывает, что даже «простое» расширение резолва в `f1_expr`-соседних
местах регрессировало трижды.

---

## Смежные находки (НЕ чинил, номер присвоит интегратор)

### 1. `+=`/compound-assign на named-tuple не десугарится (реальная реграсс-находка, НЕ №248/№145)

При прогоне реальной миграции nova-bignum (см. ниже) вскрыт ОТДЕЛЬНЫЙ,
несвязанный дефект: `mant += BigInt.one()` (где `mant` — named-tuple
`BigInt`, у которой ЕСТЬ операторный метод `@plus`) эмитится КАК СЫРОЙ C
`+=` на структуре — `mant += Nova_BigInt_static_one();` — CC-FAIL
(«invalid operands to binary expression»). Тот же КЛАСС, что уже закрытый
**№284** («+=/-=/битовые на VALUE-record с операторным методом не
десугарились»), но №284's фикс покрыл ТОЛЬКО `NovaValue_*`-ABI — named-
tuple (`NovaTuple_*`) осталась вне десугар-блока. Место (не проверял
глубже, не мой периметр этой волны): та же санкционированная десугар-
логика compound-assign в `emit_c.rs`, что чинил №284.

### 2. `Nova_BigInt_method_equal` conflicting types — УЖЕ известный маркер

Реальная миграция nova-bignum также воспроизвела `[M-static-selfreturn-
value-mangle-conflict]` (P2, OPEN, `backlog-followups.md` ~2780) — НЕ
новая находка, уже задокументирован (найден окном `p-tuple`, 2026-08-02,
bfd1194 в nova-bignum). Подтверждаю его актуальность на текущем HEAD.

---

## Побочная проверка: реальная миграция nova-bignum (тестовый носитель, НЕ часть задания)

Чтобы получить репро с полноценным compile-unit (не изолированный файл —
брифовое требование п.3 для №248), смигрировал `BigInt` в отдельном
worktree `nova-bignum` (`d:/Sources/nv-lang/nova-bignum-p248t`, ветка
`p248t-repro`, НЕ коммичена в основную репу nova-bignum — черновой тестовый
носитель) с `type BigInt value {...}` на `type BigInt(sign Sign, limbs
[]u32)` — механическая правка ~15 сайтов конструирования (тот же приём,
что описан в двух прошлых попытках миграции, `p-tuple-migration`/`p-tuple`).

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

**Не мержил, не коммитил** — тестовый носитель для проверки в мега-CU-
масштабе, worktree можно удалить после приёмки.

---

## Финальные гейты

Все прогоны — на этом дереве (`nova-p248t`), после ВСЕХ правок №248+№145
(включая миграцию 11 файлов conformance на позиционное конструирование,
D102-фикс, деструктуризация-фикс, 4 новые фикстуры).

**`cargo build --release`** (из `nova-cli/`) — чисто, без ошибок (только
pre-existing warnings, не мои). Финальный прогон: `Finished release
profile [optimized] target(s) in 0.35s` (кэш валиден с предыдущей чистой
сборки).

**`nova check std/src`** — дословно:
```
===== SUMMARY =====
PASS: 148  FAIL: 26  WARN: 61
```
Канон **148/26/61** — байт-в-байт, не сдвинулся.

**`bash scripts/guards/arch-ratchet.sh`** — дословно:
```
ARCH-RATCHET FAIL: lines=64567 > baseline=64545 (emit_c must not grow;
fix belongs to checker channel / IR path)
arch-ratchet ok: infer=348 <= 348
```
**КРАСНЫЙ по `lines`** (+22 сверх baseline `64545` этого дерева; канон
брифа называл `64542` — расхождение baseline'а с брифом не моё, не
разбирал). `infer` — канон 348, не сдвинулся. Разбор причины и
предложение по переводу на канал — см. раздел «Честный разбор места
фикса» выше. **Не заявляю это «приемлемо» — докладываю интегратору как
факт, решение (принять / потребовать канальную миграцию перед вливанием)
за ним.**

**Мега-CU (`nova test --positive --compile-error spec_tests/conformance
--toolchain clang`)** — прогнан ДВАЖДЫ (первый прогон поймал ОДИН
пропущенный сайт миграции — `t4_defaults.nv`, зафиксирован, второй прогон
чистый). Финальный (второй) прогон, дословно:
```
===== SUMMARY =====
PASS: 657  FAIL: 0  SKIP: 68 (skipped)
```
Мега-CU зелёный. Флагман (`examples/flagship/aggregator --strict-effects`)
— по правилу брифа п.7, эту волну не гоняла, гонит интегратор при приёмке
(риск для флагмана расцениваю как низкий: грепом по `examples/` и всем
sibling-репам — `nova-polaris`/`nova-http`/`nova-tls`/`nova-bignum`/
`nova-compress` — named-tuple типов не найдено ни одного, D102-фикс их не
может задеть; №248-фикс — чистое расширение существующего normalization-
хелпера, не сужает ничего уже рабочего).

### Сводка по пунктам задания

| # | Пункт | Статус |
|---|---|---|
| 1 | №248 фикс + доказательство (изолированная фикстура + мега-CU) | ✅ фикс есть, изолированная и мега-CU фикстуры зелёные |
| 2 | №145 (1) спек-противоречие | НЕ трогал spec/ (зона интегратора) — текст амендмента дан выше |
| 3 | №145 (2) D102-энфорс + миграция | ✅ фикс есть, 11 сайтов conformance мигрированы, std/examples/sibling-репы — 0 находок |
| 4 | №145 (3) деструктуризация (полная+частичная) + запрет позиционной | ✅ все три формы реализованы и подтверждены прогонами |
| 5 | Фикс в чекер-канале (§0) | ⚠️ ЧАСТИЧНО — №145(2) и positional-guard в канале; №248 и №145(3)-деструктуризация задевают emit_c.rs (легаси), обоснование и план миграции — выше |
| 6 | `cargo build --release` чисто | ✅ |
| 7 | `nova check std/src` = 148/26/61 | ✅ байт-в-байт |
| 8 | arch-ratchet | ❌ КРАСНЫЙ по lines (+22), infer в каноне |
| 9 | Мега-CU (интегратор гонит, но я тоже прогнал) | ✅ 657/0/68 |
| 10 | Флагман | НЕ гонял (правило брифа — гонит интегратор) |

**Не заявляю «№248/№145 закрыты»** — заявляю: конкретные, воспроизводимые
дефекты найдены, зафиксированы конкретным местом в коде, оба прогона
(изолированный + мега-CU) подтверждают фикс явно; ЕДИНСТВЕННОЕ открытое
несоответствие дисциплине — размещение фикса (emit_c.rs вместо
чекер-канала для части объёма), доложено честно с планом миграции.
Решение «мержить как есть / держать до канальной миграции» — за
интегратором.
