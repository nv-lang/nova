# PROGRESS — p361-crossfile (№361, `[M-181-result-over-named-tuple-codegen]`, cross-file вариант №271)

Окно: p361-crossfile. Модель: sonnet. Ветка `p361-crossfile`, worktree
`d:/Sources/nv-lang/nova-p361`. Дата: 2026-08-05.

## Итог одной строкой

**№361 закрыт и доказан на пакете.** Корень найден в `emit_c.rs`
(`register_novaopt_decl`'s fallback-ветка через `debt_is_late_emitted_value_payload`),
фикс — расширение одного предиката, net-zero по строкам (64545 == 64545).
Проверка по назначению: копия `nova-bignum` с мигрированным `BigInt` (позиционный
кортеж) — файл, из-за которого вчера ошибочно объявили миграцию разблокированной
(`src/bigdecimal/core`), теперь **PASS**. Но **`nova test src` на пакете
целиком всё ещё КРАСНЫЙ** — три цели (`bigfloat/core`, `bigrat/core`,
`repro_parse_test`) падают на **другом, ранее задокументированном дефекте**
(«Дефект B» из `PROGRESS-p-bignum-tuple.md`: компаунд-присваивание `+=` на
именованном кортеже эмитит невалидный C, класс ошибки clang другой —
`invalid operands to binary expression`, не `conflicting types`). Этот дефект
**вне периметра №361** — не трогал, не чинил. Миграция `nova-bignum`
**НЕ полностью разблокирована** этим окном — снят ровно один блокер из двух
найденных вчера.

## Репро в реальной форме (первым делом, до анализа кода)

Однофайловая проба вчера ничего не доказала — повторил её ошибку не стал.
Скопировал `nova-bignum` (`d:/Sources/nv-lang/nova-bignum`, репу НЕ трогал) в
`scratchpad/bignum-copy`, смигрировал `BigInt` (`export type BigInt(sign Sign,
limbs []u32)`, все ~10 сайтов конструирования на позиционную форму — та же
механика, что вчерашнее окно уже отработало и признало рабочей) и прогнал
`nova test src` через ОТДЕЛЬНЫЙ wrapper (`nova-p361.sh`), указывающий на
компилятор ИЗ ЭТОГО worktree (не главную репу):

**До фикса** (`nova test src`, реальный пакет, дофиксовый компилятор):
```
CC-FAIL   src/bigdecimal/core   # .../bigdecimal\core.c:2304:18: error: conflicting types for 'Nova_BigInt_method_equal' | 1 error generated.
CC-FAIL   src/repro_parse_test  # conflicting types (2289) + invalid operands (13654), 2 errors
CC-FAIL   src/bigfloat/core     # conflicting types (2287) + invalid operands (13907), 2 errors
CC-FAIL   src/bigrat/core       # conflicting types (2355) + invalid operands (13346), 2 errors
PASS      src/bigint/core
PASS: 3  FAIL: 4  SKIP: 2
```
Это ТОТ ЖЕ симптом, что «дефект C» в `PROGRESS-p-bignum-tuple.md`: `BigInt`
(кортеж) + `BigDecimal` (record-потребитель, ничего в нём не менял) —
минимальная пара, воспроизводится СРАЗУ, без `BigRat`/`BigFloat`.

## Корень — `emit_c.rs`, `register_novaopt_decl` fallback-ветка

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`
**Функция:** `debt_is_late_emitted_value_payload` (строки 54855-54868 до фикса)
**Триггер:** `register_novares_decl` (строки 54968-54969) для
`str.to_bigdecimal() -> Result[BigDecimal, ParseNumberError]` регистрирует
`NovaOpt`-обёртку и для Ok-payload'а (`NovaValue_BigDecimal`) тоже.

`debt_is_late_emitted_value_payload("NovaValue_BigDecimal")` возвращала
`false` — предикат ловил только **generic mono** value-record'ы
(`NovaValue_…____…`, вроде `NovaValue_Box____nova_int`), а `BigDecimal` —
**обычный, не-generic** value-record (никакого `____` в C-имени). Из-за
этого `register_novaopt_decl` шёл в САМУЮ ПОСЛЕДНЮЮ, «раннюю», ветку
(строки 54497-54550 до фикса): выставляет `novaopt_early_gen = true` и сразу
(не откладывая) строит ТЕЛО eq-функции через `emit_field_eq`, вставляя его
в `novaopt_typedefs_buf` — буфер, который эмитится В САМОМ НАЧАЛЕ
`.c`-файла, до forward-declaration'ов методов.

`emit_field_eq("NovaValue_BigDecimal", ...)` в этот момент НЕ находит
`BigDecimal`'s собственный `@equal` в `method_overloads` (тот ещё не
проиндексирован — сигнатура `Result[BigDecimal,...]` сканируется РАНЬШЕ,
чем компилятор доходит до тела/методов самого `BigDecimal` в этом же
файле) → падает в structural-fallback: рекурсия по `record_schemas`,
заходит в поле `mant BigInt` → тип `NovaTuple_BigInt` → ТАМ метод
`BigInt.@equal` УЖЕ найден (модуль `bignum.bigint` — импортированный,
полностью обработан РАНЬШЕ) → эмитит прямой вызов `Nova_BigInt_method_equal(&(...), ...)`
СРАЗУ, в раннем буфере. Настоящая forward-declaration
`Nova_BigInt_method_equal` в этом же `.c`-файле (BigDecimal ссылается на
BigInt через import — компилятор всё равно per-TU переобъявляет её)
эмитится ПОЗЖЕ, в обычном проходе — implicit-декларация от раннего вызова
конфликтует с настоящей. Тот же класс ошибки, что №271, но триггер —
**вложенное поле record-потребителя**, а не сам тип с `@equal`.

## Фикс

Расширил предикат: убрал требование `c_ty.contains("____")` для
`NovaValue_`-веток — теперь ЛЮБОЙ `NovaValue_<Name>` (не только
mono-generic) считается late-emitted и уходит в отложенный буфер
(`novaopt_eq_fns_buf`, сплайсится ПОСЛЕ forward-declaration'ов методов).
Безопасно: не-generic `NovaValue_<Name>` — ВСЕГДА обычный
пользовательский value-record (generic-моно всегда несёт `____` в
C-имени по соглашению мангла), так что расширение не задевает ничего
постороннего; для схем без риска (только скаляры) поздняя эмиссия просто
меняет МЕСТО в файле, не меняет корректность.

```rust
fn debt_is_late_emitted_value_payload(c_ty: &str) -> bool {
    c_ty.starts_with("NovaValue_")
        || (c_ty.starts_with("NovaTuple_") && !c_ty.ends_with('*'))
        || Self::parse_mono_tuple_elements(c_ty).is_some_and(
            |es| es.iter().any(|e| Self::debt_is_late_emitted_value_payload(e)))
}
```

Один предикат, вызывающие места не менялись (уже были правильные —
№271-фикс вчера централизовал их). Диф — **net-zero**: было 64545 строк,
стало 64545 (комментарий над функцией и unit-тест
(`novares_late_payload_tests::gates_named_tuple_and_mono_value_record_but_not_others`,
проверка `NovaValue_Plain` была `!...` → стала `...`, ассерт перевёрнут на
корректный) уплотнены в те же строки).

**Почему это законная эмиссионная правка, а не типовая.** Порядок, в
котором C-текст видит вызов функции относительно её forward-declaration —
это факт ОДНОПРОХОДНОЙ ЭМИССИИ, у которого нет представления в чекере
(чекер не знает про C-порядок деклараций вообще). Расширение предиката не
разбирает C-имя типа СТРОКОЙ ради типовой информации — оно классифицирует
УЖЕ ГОТОВОЕ канонiческое C-имя (сформированное самим компилятором по
фиксированной грамматике) на «эмитится рано» / «эмитится поздно», ровно
тот же характер правки, что вчерашний фикс №271.

## Проба на актуальность (уникальные имена, дифференциальная)

Не полагался на одну лишь real-package пробу — собрал МИНИМАЛЬНУЮ
кросс-файловую фикстуру с заведомо уникальными именами (см. ниже) и
прогнал differentially: собрал компилятор БЕЗ фикса (временный откат
файла на родительский коммит, пересборка, прогон, восстановление,
пересборка) — фикстура даёт **CC-FAIL** с ДОСЛОВНО тем же текстом
(`conflicting types for 'Nova_P361Pair_method_equal'`); на фиксовом —
**PASS**. Это исключает «повезло с реальным пакетом, но фикс не про то».

## Фикстуры (`docs/dev/test-conventions.md`)

Однофайловой пробы недостаточно для №361 (в этом и была вчерашняя ошибка) —
сделал **межфайловую**, cross-module (два РАЗНЫХ `module`, с явным
`import`), по образцу `nova_tests/modules/cycle_a.nv`/`cycle_b.nv`
(единственный найденный в репе пример двух standalone-модулей с
взаимным/направленным import и тестами прямо в потребителе):

- `nova_tests/modules/p361_named_tuple_producer.nv` —
  `module modules.p361_named_tuple_producer`: `P361Pair(x int, y int)`
  (позиционный кортеж, зеркалит `BigInt(sign, limbs)`) + `@equal` +
  `@split(...) -> Result[(P361Pair, P361Pair), P361SplitErr]` (зеркалит
  `@div_rem`).
- `nova_tests/modules/p361_named_tuple_consumer.nv` —
  `module modules.p361_named_tuple_consumer`, `import
  nova_tests.modules.p361_named_tuple_producer.{P361Pair}`:
  `P361Holder value { p P361Pair, tag int }` (зеркалит
  `BigDecimal { mant BigInt, scale int }`) + `str
  @to_p361holder() -> Result[P361Holder, P361ParseErr]` (зеркалит
  `str @to_bigdecimal() -> Result[BigDecimal, ParseNumberError]` —
  функция, чья СИГНАТУРА триггерит раннюю регистрацию). Тест гоняет ОБЕ
  ветки (`Ok`/`Err`) плюс структурное сравнение через `Some(h) == Some(...)`
  (реально исполняет позднюю eq-функцию, не только компилирует).

Запуск фикстуры как ENTRY (обязательно — баг воспроизводится только когда
потребитель является точкой компиляции, не просто peer):
```
nova test nova_tests/modules/p361_named_tuple_consumer.nv
```
**Вердикт (фиксовый компилятор):**
```
Toolchain: clang, mode=Dev, jobs=16, paths=[...p361_named_tuple_consumer.nv]
PASS           nova_tests/modules/p361_named_tuple_consumer

===== SUMMARY =====
PASS: 1  FAIL: 0
```
**Вердикт (дофиксовый компилятор, дифференциальная проба):**
```
CC-FAIL        nova_tests/modules/p361_named_tuple_consumer  # .../p361_named_tuple_consumer.c:1406:18: error: conflicting types for 'Nova_P361Pair_method_equal' | 1 error generated.

PASS: 0  FAIL: 1
```

## Вердикт прогона ПАКЕТА `nova-bignum` (обязательная проверка по назначению)

Копия в `scratchpad/bignum-copy` (репу `nova-bignum` НЕ менял), `BigInt`
смигрирован на позиционный кортеж, остальные три типа (`BigDecimal`,
`BigRat`, `BigFloat`) — как в `main` (record/value, не трогал), прогон
через компилятор ИЗ ЭТОГО worktree (не главной репы):

```
Toolchain: clang, mode=Dev, jobs=16, paths=[...bignum-copy\src]
SKIP           src/bigrat/core_slow  # slow lane
SKIP           src/bignum            # no test blocks (compiled OK)
PASS           src/repro_direct_test
PASS           src/repro_test
CC-FAIL        src/bigfloat/core       # .../bigfloat\core.c:13913:18: error: invalid operands to binary expression ('NovaTuple_BigInt' and 'NovaTuple_BigInt') | 1 error generated.
CC-FAIL        src/repro_parse_test    # .../repro_parse_test.c:13663:18: error: invalid operands to binary expression (...) | 1 error generated.
CC-FAIL        src/bigrat/core         # .../bigrat\core.c:13355:18: error: invalid operands to binary expression (...) | 1 error generated.
PASS           src/bigint/core
PASS           src/bigdecimal/core

===== SUMMARY =====
PASS: 4  FAIL: 3  SKIP: 2 (skipped)
```

**`src/bigdecimal/core` — ИМЕННО тот файл, что вчера дал CC-FAIL
`conflicting types` (дефект C) — теперь PASS.** Ни одного `conflicting
types`-CC-FAIL нигде в пакете больше нет — №361 закрыт целиком (не только
на минимальной фикстуре). Оставшиеся 3 CC-FAIL — **другой класс clang-ошибки**
(`invalid operands to binary expression`, не `conflicting types`) — это
«Дефект B» из вчерашнего `PROGRESS-p-bignum-tuple.md`, конкретно найден
источник: `bigfloat/core.nv:338`, `mant += BigInt.one()` — компаунд-присваивание
`+=` на именованном кортеже с пользовательским `@plus` эмитит СЫРОЙ C `+=`
на структуре вместо вызова `.plus()`/`Nova_BigInt_method_plus`, что невалидно
для non-scalar struct. `bigrat/core` и `repro_parse_test` падают на ТОМ ЖЕ
дефекте не напрямую, а транзитивно — `bigrat/core_test.nv` импортирует
`bignum.bigfloat.{BigFloat, PrecisionContext}`, `repro_parse_test.nv`
импортирует `bignum.bigfloat.{BigFloat}` напрямую — оба CU утягивают
`bigfloat/core.nv`'s `+=`-код через import. **Этот дефект — ВНЕ периметра
№361** (другой код-путь: lowering компаунд-присваивания, а не эмиссия
eq-хелпера; другой класс clang-ошибки), не чинил, номер не присваивал (за
интегратором, как и «Дефект B» вчера).

## Прогоны — вердикты дословно

**Rust unit test (расширенный `debt_is_late_emitted_value_payload`):**
```
test codegen::emit_c::novares_late_payload_tests::gates_named_tuple_and_mono_value_record_but_not_others ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1230 filtered out; finished in 0.00s
```

**`cargo build --release` (nova-cli):** чисто, `Finished \`release\` profile
[optimized] target(s) in ~2m 07s` (только pre-existing warnings).

**`cargo test --release --lib` (compiler-codegen), полный прогон:**
дефолтный `cargo test` на этой машине стабильно ловит
`STATUS_STACK_OVERFLOW` на нескольких тестах в модулях `doc::test_runner`/
`doc::watch_cache` — **воспроизведено ТАКЖЕ на дофиксовом (родительский
коммит) `emit_c.rs`, идентично**, значит это НЕ регрессия от этой правки, а
предсуществующий разрыв между default test-thread stack size (~2 МБ на
Windows) и реальной потребностью checker-пути этих тестов. С
`RUST_MIN_STACK=67108864` (64 МБ) оверфлоу пропадает полностью, полный
прогон завершается:
```
test result: FAILED. 1227 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.03s
```
Все 4 фейла — **предсуществующие, не связаны с этой правкой**, подтверждено
differentially (тот же прогон на дофиксовом `emit_c.rs` даёт ИДЕНТИЧНЫЙ
результат для `codegen::emit_c::array_lit_named_tuple_box_tests::*`, дословно
тот же текст паники):
- `codegen::emit_c::array_lit_named_tuple_box_tests::emit_array_lit_int_primitive_unchanged`
  — паникует на `[Plan172.12-A8]` prelude-facade сообщении, не про eq/tuple.
- `codegen::emit_c::array_lit_named_tuple_box_tests::emit_array_lit_named_tuple_heap_box`
  — паникует на `[P67] nova_int collapse`.
- `parser::tests::if_let_pattern` — про `E_IF_LET_RETRACTED` (D184), не
  затронуто этим окном вообще.
- `test_runner::tests::p0_erased_now_dispatches_via_vtable` — про
  `E_STR_CONCAT_PLUS`/`E_READONLY_COERCE` в `nova_tests/plan72/*`, не
  затронуто.

Мой предикат (`novares_late_payload_tests`) — единственный тест,
затрагивающий изменённый код, зелёный и до, и после (тест ОБНОВЛЁН по
факту нового корректного поведения — старый ассерт `!...NovaValue_Plain`
описывал СТАРОЕ, ошибочное поведение).

**`nova check std/src` (без `NOVA_STD_PATH`-редиректа — worктри проверяет
СВОЙ std):**
```
PASS: 148  FAIL: 26  WARN: 61
```
Байт-в-байт канон.

**`arch-ratchet.sh`:**
```
arch-ratchet ok: lines=64545 <= 64545
arch-ratchet ok: infer=348 <= 348
```

**Фикстура (`nova_tests/modules/p361_named_tuple_consumer.nv`):** см. выше
— PASS на фиксовом, CC-FAIL (дословно тот же текст, что реальный пакет) на
дофиксовом.

**Пакет `nova-bignum` (копия, `nova test src`):** см. выше — `PASS: 4
FAIL: 3 SKIP: 2`; целевой файл (`bigdecimal/core`) — PASS; остаток —
отдельный, вне периметра дефект.

## Смежные находки

1. **Дефект B локализован до конкретной строки.** Вчерашний
   `PROGRESS-p-bignum-tuple.md` описывал класс («операторный десугаринг
   роняет `&`-адресацию») без точной локации в кодовой базе `emit_c.rs`.
   Это окно нашло минимальный источник в самом `nova-bignum`:
   `src/bigfloat/core.nv:338`, `mant += BigInt.one()` — компаунд-присваивание
   на именованном кортеже с пользовательским `@plus`. НЕ расследовал корень
   в `emit_c.rs`/`operator_dispatch.rs` (вне периметра задания, нулевого
   бюджета по строкам на emit_c.rs всё равно бы не хватило для НОВОГО,
   отдельного фикса без решения владельца по ratchet-базе) — только
   локализовал источник для следующего окна.
2. **`RUST_MIN_STACK` — известный разрыв инфраструктуры, не мой.** Голый
   `cargo test --release --lib` в compiler-codegen ловит
   `STATUS_STACK_OVERFLOW` детерминированно на нескольких несвязанных
   тестах (`doc::test_runner::*`, `doc::watch_cache::*`) на ЭТОЙ машине —
   предсуществующе, не связано с этим окном. `RUST_MIN_STACK=67108864`
   снимает симптом. Не чинил (тестовая инфраструктура, не часть №361), но
   отмечаю — иначе следующий агент опять ошибочно спишет это на
   собственную правку.
3. **Wrapper-скрипт для прогона worktree-компилятора на копии пакета.**
   `nova.sh` пакетов (`nova-bignum/nova.sh`) жёстко бьёт в `D:/Sources/nv-lang/nova`
   (главную репу). Для проверки правок ИЗ worktree понадобился отдельный
   wrapper (`nova-p361.sh`, живёт только в `scratchpad/bignum-copy`, не
   коммитился никуда) — `NOVA_RT_DIR`/`NOVA_CG_INCLUDE`/`NOVA_STD_PATH` на
   worktree, `NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_GC_INCLUDE_DIR` на
   главную репу (см. `docs/dev/dev-workflow.md:173`) — тот же паттерн, что
   уже задокументирован для `nova test` в самом worktree nova, просто
   перенесённый на внешний пакет.

## Вывод: разблокирована ли миграция `nova-bignum`?

**Частично.** №361 — ЗАКРЫТ, доказано на пакете (`bigdecimal/core`:
CC-FAIL → PASS, единственный файл, что вчера дал ложный вердикт
«разблокировано»). Но `nova test src` на пакете целиком **всё ещё
красный** (`PASS: 4 FAIL: 3`) — блокирует **Дефект B** (компаунд-присваивание
`+=` на именованном кортеже, `bigfloat/core.nv:338`), СОВЕРШЕННО ОТДЕЛЬНЫЙ
от №361 дефект, задокументированный вчера, не тронутый этим окном
(вне периметра задания, свой код-путь, свой класс clang-ошибки).
«Фикстура зелёная» (моя `p361_named_tuple_consumer.nv`, `bigdecimal/core`)
и «дефект закрыт» — верно; «миграция разблокирована» (пакет `nova test src`
целиком зелёный) — НЕТ, требует отдельного окна на Дефект B.

## Модель

sonnet.
