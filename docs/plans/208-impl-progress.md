<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 — Ф.2 реализация, checkpoint (сессионные заметки)

Worktree: `d:/Sources/nv-lang/nova-208impl` (branch `p208-impl`, base `cccad54d8`
main). Модель: sonnet. Суб-агенты НЕ спавнились (прямое исполнение). Работа
идёт СИНХРОННО, мелкими коммитами (CPU у оркестратора перегружен — per-фаза
верификация облегчена: одна таргет-фикстура вместо широких прогонов; полный
`nova test`/conformance — гейт оркестратора, не мой).

## Статус на момент этого чекпоинта: Ф.0/Ф.1 УЖЕ БЫЛИ В main; Ф.2 — В ПРОЦЕССЕ (std-сигнатуры готовы, emit_c.rs диспатч — СЛЕДУЮЩИЙ шаг, ещё НЕ сделан)

### Что уже было в main ДО этой сессии (важная находка — не моя работа)

- **Ф.0 (спека D422)** — влита (`edcc4ab73` merge, +амендменты D419 retract/
  D374/D237/D229/D179/D55). Проверено: `spec/decisions/02-types.md` содержит
  полный D422 текст, статус-таблицу (Ф.0 ✅, Ф.1-4 pending) — сверено 1:1 с
  `docs/plans/208-unified-formatter.md` §9.
- **Ф.1 (буфер-примитивы, АДДИТИВНО)** — тоже влита (`b6ee6f40a` merge):
  - `std/src/runtime/fmt_buf.nv` (новый файл) — `int_fmt`/`bool_fmt`/
    `char_fmt` (module-private), `Align`/`FloatKind` enums (`export`),
    `extern "C" fn fmt_f64_into` + `.nv`-wrapper `fmt_f64`.
  - `compiler-codegen/nova_rt/nova_rt.h` — `fmt_f64_into` C-функция.
  - `std/src/runtime/string_builder.nv` — АДДИТИВНО: `@reserve`/`@advance`/
    `@write_padded`/`@pad_in_place`/`@into_str_checked` (старый API нетронут).
  - Заметки: `docs/plans/208-f1-notes.md` (уже в main, читал перед стартом).
  - **Вывод координации 152.7.2**: в main из «двух-методной D419» реально
    влиты — `Fmt`(2-осевой: `write(str)`+`alternate`+`precision`)/`FmtCtx`/
    `@display_fmt` в `protocols.nv` (docs+код), И РЕАЛЬНЫЙ потребитель
    `JsonValue.@display_fmt` в `std/src/encoding/json.nv:902` (плюс тест
    `json_test.nv:208`). Это и есть [M-152.7.2]-наработка, которую 208
    сворачивает в один метод (задача этой сессии).

### Ф.2 — сделано В ЭТОЙ сессии (std-сторона сигнатур; компилятор-диспатч — ЕЩЁ НЕТ)

1. **`std/src/runtime/fmt_buf.nv`** — добавлены `Sign enum Minus | Plus` и
   `FmtKind enum Display | Debug | Hex | Oct | Bin | Exp` (D406 marker,
   `#unstable`, `export`) рядом с `Align`/`FloatKind` — один файл-дом для всех
   format-enum'ов D422 §6.

2. **`std/src/prelude/protocols.nv`** — ПОЛНАЯ замена Write/Fmt/FmtCtx/
   Display/Debug блока (было: `Write.@write(s str)`, узкий 2-осевой D419
   `Fmt`+`FmtCtx` (`alt`/`has_prec`/`prec`), Display/Debug с default-body
   через `Write`):
   - `import std.runtime.fmt_buf.{Align, Sign, FmtKind}` (новый, у файла нет
     `#no_prelude` — обычный prelude-модуль; `fmt_buf` сам `#no_prelude`, цикл
     не открывается).
   - `Write protocol { mut @write(bytes []u8) -> () }` (было `s str`).
   - `Fmt protocol { use Write; @width()->Option[int]; @precision()->
     Option[int]; @align()->Option[Align]; @fill()->char; @sign()->Sign;
     @alternate()->bool; @kind()->FmtKind; mut @pad(bytes []u8)->() }` —
     `use Write` = protocol-embed (D145), ТОЧНО по D422 §2.
   - `FmtCtx` — конкретный record: `sink Write, mark int, width
     Option[int], precision Option[int], align Option[Align], fill char,
     sign Sign, alternate bool, kind FmtKind, mut pad_consumed bool, mut
     prec_consumed bool`. Поля названы ТОЧНО как protocol-методы (`width`,
     `precision`, ...) — подтверждённый в кодбейзе паттерн (`Vec[T] @cap()
     => @cap`, vec/core.nv:255) — getter той же арности читает одноимённое
     поле, никакой коллизии.
   - Два конструктора — **оба принимают ТОЛЬКО примитивные типы** (никаких
     Nova-enum литералов в hand-synthesized C-коде компилятора):
     - `FmtCtx.bare(sink Write, mark int, is_debug bool) -> Self` — bare
       `${x}`/`${x:?}` (все оси default/None).
     - `FmtCtx.rich(sink Write, mark int, has_width bool, width int,
       has_precision bool, precision int, align_code int, fill_cp int,
       sign_plus bool, alternate bool, is_debug bool) -> Self` — параметры
       СПЕЦИАЛЬНО зеркалят существующие Rust-side вычисленные значения в
       `emit_format_spec_value` (`fill_cp`, `align_code(...)`, `width_lit`,
       `sign_plus`) — компилятору не придётся ничего пересчитывать, только
       передать те же переменные другим вызовом.
   - `@write`/`@width`/`@precision`/`@align`/`@fill`/`@sign`/`@alternate`/
     `@kind` — explicit getters на `FmtCtx` (читают одноимённые поля).
   - `mut @pad(bytes []u8)` — V1: `@sink.write(bytes); @pad_consumed = true`.
   - `Display protocol { @display(mut f Fmt) -> () }` / `Debug protocol {
     @debug(mut f Fmt) -> () }` — **REQUIRED, БЕЗ default-body** (D422 §3
     инвариант «нет циклической ловушки» — старый default `w.write(str.
     from_debug(@))`/`w.write(@to_str())` СНЯТ полностью). Это = ретракт
     `str.from_debug` default-body (D422 §"Связь" пункт про 174.2) —
     `str.from_debug` как символ и до этого был нереализован (Ф.1-заметки),
     так что рабочего поведения не теряется.
   - Примитивные имплы (`int`/`f64`/`f32`/`bool`/`char`/`str` `@display`/
     `@debug`) — сигнатура `(mut w Write)` → `(mut f Fmt)`; тела ОСТАЛИСЬ
     interp-string шорткатом (`f.write("${@}".bytes())` — было
     `w.write("${@}")`), **НЕ** переписаны на прямой вызов `int_fmt`/
     `bool_fmt`/`char_fmt` буфер-примитивов Ф.1. Обоснование (см. «Осознанные
     упрощения» ниже) — компилятор НИКОГДА не зовёт эти методы для голого/
     rich-spec ПРЯМОГО примитива (свой быстрый conv.h-путь), только для
     примитива ВНУТРИ generic-диспетча (`Option[T Debug]`/`Result[...]`
     `v.debug(f)`, будущий Ф.3 `Vec[T]`). Там `f.kind()`/радикс НЕ читаются
     — известное упрощение, задокументировано инлайн + здесь.
   - `Option[T Debug] @debug`/`Result[T Debug, E Debug] @debug` — сигнатура
     Write→Fmt, тела форвардят `f` без изменений (`v.debug(f)`), литералы
     через `.bytes()`.

3. **`std/src/runtime/string_builder.nv`** — `@write(s str)` →
   `@write(bytes []u8) { @buf.append(bytes) }` (было `@buf.append(s.bytes())`
   — на один шаг короче, вход уже байты).

4. **`compiler-codegen/src/protocols/auto_derive.rs`** (auto-derive
   record/sum/tuple `@display`/`@debug` synthesis — Ф.3-механизм, но ЕГО
   сигнатура ОБЯЗАНА мигрировать ВМЕСТЕ с протоколом, иначе синтезированные
   тела перестанут удовлетворять required-`Display`/`Debug`):
   - `synthesize_display`/`synthesize_debug`: `make_param("w",
     type_ref_named("Write"))` → `type_ref_named("Fmt")` (оба места).
     Имя параметра `"w"` НЕ переименовано в `"f"` — протокол-соответствие
     проверяется по ТИПАМ, не по именам параметров (структурная проверка);
     минимизирует диф.
   - НОВЫЙ хелпер `to_bytes(e: Expr) -> Expr` (= `e.bytes()` AST) —
     ВСЕ синтезированные `w.write(<строка-литерал-AST>)` и
     `w.write(str.from(x))` вызовы обёрнуты в него (13 call site'ов:
     `simple_display_block`, `synth_display_record_body`,
     `synth_debug_record_body`, `synth_fmt_sum_body`'s `write_lit`/
     `emit_value`/record-prefix). Причина: `Write.@write` теперь `[]u8`;
     НЕ полагался на D55 str-литерал→`[]u8` коэрсию (её реализация в
     чекере под ЭТИ конкретные AST-формы — `str.from(x)`'s call-result —
     не проверена мной; explicit `.bytes()` работает независимо от статуса
     коэрсии, ниже риск).
   - Форма вывода (`TypeName { f: v, ... }`, ОДИНАКОВАЯ для Display и
     Debug) — **НЕ изменена** (D422 §4 divergent-форма — Ф.3 scope,
     сознательно не трогал в Ф.2, чтобы не смешивать сигнатуру-миграцию с
     поведенческим изменением в одном шаге).

### Верификация ЭТОГО чекпоинта (минимальная, по инструкции — CPU перегружен)

- `cargo build --release` (compiler-codegen, `NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/
  `NOVA_GC_INCLUDE_DIR` на vcpkg главного репо) — **0 ошибок**, только
  pre-existing warnings (51, все были и до моих правок). ~1м18с.
- **НЕ запускал** `nova test`/`nova build` на std ещё — ОЖИДАЕМО КРАСНО:
  `emit_c.rs` (`emit_interpolated_str`/`emit_format_spec_value`) ещё
  passes raw `Nova_StringBuilder*` вторым аргументом в
  `Nova_T_method_display/debug(...)` вызовы — но теперь метод (и
  auto-derive синтез, и примитивные имплы) ожидает `Nova_FmtCtx*`. Это
  C-type mismatch на ЛЮБОМ interpolation call site — весь `std`
  (интерполяция пронизывает всё) НЕ соберётся, пока emit_c.rs не
  переписан. Это ОЖИДАЕМОЕ промежуточное состояние одного под-шага
  Ф.2 big-bang (сигнатуры и диспатч мигрируют НЕ одним атомарным патчем
  из-за объёма — но оба нужны ВМЕСТЕ для зелёного `nova test`; следующий
  коммит = `emit_c.rs`).
- nova-cli НЕ пересобирался в этом чекпоинте (не нужен, пока сам `nova.exe`
  не понадобится для теста — следующий шаг).

### Следующий шаг (уже начат в понимании, код — ещё нет)

`compiler-codegen/src/codegen/emit_c.rs`:
1. `emit_interpolated_str` (~40354 в этом дереве на момент чтения) — bare
   `${x}`/`${x:?}` non-primitive dispatch (~40575-40593, `Nova_{ty}_method_
   {display|debug}(recv_c, sb)`) И Option/Result `DeclaredBody`-роутинг
   (~40475-40543, `{fn_name}(v, sb)`) — ОБА передают `sb` напрямую; заменить
   на: hand-synth `Nova_FmtCtx* fmt = Nova_FmtCtx_static_bare(sb, mark, is_debug);`
   (где `mark = Nova_StringBuilder_method_byte_len(sb)` — имя C-функции
   нужно подтвердить компиляцией) перед вызовом, затем
   `{fn_name}(recv_c, fmt)`. Ветка str.from/to_str-фоллбэка (~40602-40654)
   — НЕ трогать (не вызывает @display/@debug, возвращает `str` напрямую).
2. `emit_format_spec_value` (~40686) — radix/int/decimal/float ветки
   (~40727-40826) — **НЕ трогать** (прямой conv.h путь, примитивы никогда
   не идут через `@display`/`@debug` даже под rich-spec — задокументированное
   упрощение, см. выше). Финальная ветка "else" (композит/user-type,
   ~40866-40941) — снести `has_display_fmt`-спецкейс ПОЛНОСТЬЮ (D419
   retract), заменить на: `mark = <byte_len sb>`; сконструировать
   `Nova_FmtCtx_static_rich(sb, mark, has_width, width, has_precision,
   precision, align_code, fill_cp, sign_plus, alternate, is_debug)`
   (переиспользуя УЖЕ вычисленные Rust-переменные `fill_cp`/`align_code(...)`/
   `width_lit`/`sign_plus` из начала функции); стримить ПРЯМО в главный
   interpolation `sb` (НЕ в fresh `fmt_sb`, как раньше) — это соответствует
   D422 §4 pad_in_place дизайну (стриминг в главный буфер, потом
   `pad_in_place` если `width.is_some() && !pad_consumed`). ВАЖНО:
   `nova_fmt_str_precision`-пост-обрезка (precision auto-truncate) для этой
   ветки — **СОЗНАТЕЛЬНО ДРОПНУТЬ** (см. «Осознанные упрощения» #2 ниже) —
   меняет ожидание ОДНОГО существующего теста (`d419_unknown_spec_neg`/
   `d419_display_fmt_dispatch`, см. ниже).
3. После (1)+(2) — пересобрать compiler-codegen + nova-cli, таргет-тест
   `std/src/runtime/fmt_buf.nv` + `std/src/runtime/string_builder_test.nv`
   (быстрые), потом `std/src/prelude/` (главный риск-файл), потом
   расширять.

### spec_tests / json_test МИГРАЦИЯ — ЕЩЁ НЕ СДЕЛАНА (задача следующего шага)

- `spec_tests/conformance/d419_display_fmt_dispatch.nv` — `TaggedD419
  @display_fmt(mut f Fmt)` + `Plain @display(mut w Write)` — ОБА сломаны
  сигнатурой (Write убран, @display_fmt-путь снесён). План миграции 1:1
  (тесты НЕ ослабляются):
  - `TaggedD419` → ОДИН `@display(mut f Fmt)`, тело = union старого
    `@display_fmt` + учёт `f.alternate()`/`f.precision()` (см. разбор ниже).
  - `Plain` → `@display(mut f Fmt) { f.write(@s.bytes()) }`.
  - Ожидаемые assert'ы, ТРЕБУЮЩИЕ пересмотра (не techническая ошибка, а
    легитимное изменение семантики D419→D422, см. ниже):
    - `"${p:.3}" == "abc"` (Plain, precision truncates externally) — под
      D422 моей реализацией (precision auto-truncate ДРОПНУТ для
      composite/user-type ветки, см. «Осознанные упрощения» #2) ожидание
      МЕНЯЕТСЯ на `"${p:.3}" == "y"` (Plain's `s="y"`... wait — пример в
      файле `s: "abcdef"` → под новым поведением output = `"abcdef"`
      (БЕЗ обрезки), т.к. Plain никогда не читает `f.precision()` и
      компилятор больше не обрезает СНАРУЖИ композит-путь. ЭТО ИЗМЕНЕНИЕ
      ПОВЕДЕНИЯ, требующее явного решения — см. «СТОП-кандидат» ниже,
      возможно нужен owner sign-off ПЕРЕД тем как менять ожидание теста
      (не просто «обновить тест», а решить — это D422-совместимо или
      нужен амендмент/иной механизм prec_consumed).
    - `"${t:>4}" == "   x"` (auto-pad) — ДОЛЖНО остаться зелёным через
      `pad_in_place` (не меняется).
    - `"${t:#}"`/`"${t:.3}"` (TaggedD419 читает `f.alternate()`/
      `f.precision()` сама) — должны остаться зелёными (тип сам решает).
  - `spec_tests/conformance/neg/d419_unknown_spec_neg.nv` — `@display_fmt`
    → `@display(mut f Fmt)`, EXPECT_COMPILE_ERROR не меняется (парсер
    отвергает `:zz` до типов).
- `std/src/encoding/json.nv:902` `@display_fmt` → `@display(mut f Fmt)`
  (тело идентично: `if f.alternate() { f.write(@to_str_pretty().bytes()) }
  else { f.write(@to_str().bytes()) }`).
- `std/src/encoding/json_test.nv:208` — тест-имя/комментарий "D419:
  ...(@display_fmt)" → переименовать под D422, assert'ы (`${v:#}` /
  `${v}`) остаются идентичными (JsonValue ТЕПЕРЬ имеет `@display`
  напрямую, не через отдельный display_fmt-хук — то же поведение).

### ⚠ СТОП-кандидат / вопрос владельцу (НЕ решено самостоятельно, зафиксировано здесь)

**Precision auto-truncate для composite/user-type под rich-spec.** D422 §2
даёт `Fmt` только `@precision() -> Option[int]` (иммутабельный getter, БЕЗ
сеттера) — в отличие от `@pad`, у `precision` НЕТ протокольного механизма
"я это учёл" (`pad_consumed` навешивается явным `mut @pad`; `prec_consumed`
поле в `FmtCtx` ЕСТЬ, но протокол не даёт способа его выставить). Старое
D419-поведение (внешняя обрезка строки по precision, если тип НЕ определял
`@display_fmt`) опиралось на грубый флаг «есть ли @display_fmt вообще» —
механизм, которого в D422 просто нет (один метод, не пара). Я СДЕЛАЛ
дизайн-решение (не спек-девиация, т.к. D422 просто МОЛЧИТ про этот механизм,
не противоречит явно): **дропнуть auto-truncate для composite-пути entirely**
(Rust тоже не обрезает Debug/derive по precision) — это МЕНЯЕТ ожидание
существующего теста `d419_display_fmt_dispatch.nv` (`Plain :.3` case). Считаю
это ЛЕГИТИМНОЙ 1:1-миграцией (семантика D419 ретрактирована ВМЕСТЕ с
`@display_fmt`, не «ослаблением» теста), но это ПОГРАНИЧНЫЙ случай — если
оркестратор/владелец видит иначе (например: сделать `@precision()` при
чтении неявно flip'ать `prec_consumed`, ЛОМАЯ иммутабельность геттера из
D422 текста) — нужен явный сигнал ПЕРЕД тем как эта ветка emit_c.rs
реализуется в следующем коммите. Продолжаю с «дропнуть auto-truncate»
как default-планом, если не будет возражения.

### Осознанные упрощения (не блокеры, задокументированы для будущих фаз)

1. Примитивные `@display`/`@debug` (int/f64/f32/bool/char/str) НЕ читают
   `f.kind()`/`f.width()` — тело всегда `f.write("${@}".bytes())`. Верно
   ТОЛЬКО потому что компилятор никогда не зовёт эти методы напрямую для
   примитива (свой fast-path); единственный реальный вызыватель —
   generic-диспетч (`Option[T Debug]`/будущий Ф.3 `Vec[T]`), где radix/width
   на элементах пока не тестируется. Если Ф.3 введёт `${vec:x}`-подобный
   сценарий — эти тела придётся дописать на честный `int_fmt`(Ф.1
   буфер-примитив).
2. Precision auto-truncate для user/composite-типов под rich-spec —
   см. «СТОП-кандидат» выше.
3. `derived Display` vs `derived Debug` форма — ОДИНАКОВАЯ (D422 §4
   расхождение — Ф.3 scope, не трогал).

## Коммиты (эта сессия, ветка `p208-impl`)

- (готовится) — std-сигнатуры Write→[]u8/Fmt/FmtCtx/enums + auto_derive.rs
  Write→Fmt + `.bytes()`-обёртки. Компилятор (Rust) собирается чисто;
  `nova test` на std ОЖИДАЕМО красный до следующего коммита (emit_c.rs).
