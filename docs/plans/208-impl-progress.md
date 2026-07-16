<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 — Ф.2 реализация, checkpoint (сессионные заметки)

Worktree: `d:/Sources/nv-lang/nova-208impl` (branch `p208-impl`, base `cccad54d8`
main). Модель: sonnet. Суб-агенты НЕ спавнились (прямое исполнение). Работа
идёт СИНХРОННО, мелкими коммитами (CPU у оркестратора перегружен — per-фаза
верификация облегчена: одна таргет-фикстура вместо широких прогонов; полный
`nova test`/conformance — гейт оркестратора, не мой).

## Статус на момент ЭТОГО чекпоинта (шаг 2/N): Ф.0/Ф.1 были в main; Ф.2 std-сигнатуры (шаг 1) + emit_c.rs диспатч (шаг 2, ЭТОТ коммит) — СДЕЛАНЫ и таргет-верифицированы; json.nv/@display_fmt-фикстуры — ЕЩЁ НЕ мигрированы (шаг 3, следующий)

### Решение владельца по СТОП-кандидату (получено, 2026-07-16)

Precision auto-truncate для composite/user-типов под rich-spec — **ДРОПНУТЬ
ОДОБРЕНО**. Rust-паритет; семантика D419 ретрактирована D422 → правка
assert'а в `d419_display_fmt_dispatch.nv` = легитимная миграция
ретрактированной семантики, НЕ ослабление теста. При миграции фикстуры
(шаг 3) — переименовать/переписать под D422-ожидание, чтобы имя не врало
про D419. Реализовано ниже (шаг 2) именно так.

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

### Ф.2 шаг 2 (ЭТОТ коммит) — `emit_c.rs` диспатч переписан на FmtCtx, план шага 1 выполнен ФАКТИЧЕСКИ (с поправками, найденными при компиляции)

Реализовано ровно по плану шага 1, с поправками, которые вскрылись только
при реальной компиляции (см. «Ловушки» ниже):

1. **`emit_interpolated_str`** — новый приватный хелпер `emit_bare_fmtctx(&mut
   self, sink: &str, is_debug: bool) -> String`, эмитящий `Nova_FmtCtx*
   {tmp} = Nova_FmtCtx_static_bare({sink}, 0, {true|false});` и возвращающий
   имя temp-переменной. `mark` = литерал `0` (НЕ звал
   `Nova_StringBuilder_method_byte_len` — это неиспользуемое значение:
   `FmtCtx.bare` никогда не приводит к `@pad_in_place`, `mark` там мёртв).
   Оба call site'а (bare non-primitive dispatch ~40593 И Option/Result
   `DeclaredBody`-роутинг ~40475-40543) теперь строят `fmt_ctx` через этот
   хелпер и передают его вместо голого `sb`. Ветка str.from/to_str-фоллбэка
   — НЕ тронута (как и планировалось).
2. **`emit_format_spec_value`** — `has_display_fmt`-спецкейс СНЕСЁН
   ПОЛНОСТЬЮ (D419 retract). НО архитектура — НЕ "стриминг в главный sb +
   pad_in_place", как я планировал в прошлом чекпоинте, а **проще и ближе к
   до-D422 коду**: рендер по-прежнему идёт в СВЕЖИЙ `fmt_sb` (не в главный
   interpolation `sb`), просто теперь оборачивается в `FmtCtx.rich(...)`
   (несёт width/precision/align/fill/sign/alternate/kind — тип МОЖЕТ их
   прочитать), а padding по-прежнему навешивается СНАРУЖИ через
   существующий `nova_fmt_pad(...)` (тот же C-хелпер, что и все остальные
   ветки этой функции) — `mark`=`0` (фиктивно, `fmt_sb` пуст, эта ветка
   НИКОГДА не зовёт `@pad_in_place`). **Почему передумал** (см. код/комментарий
   на месте): сохранение "render fresh sb + external nova_fmt_pad"-архитектуры
   даёт ИДЕНТИЧНОЕ поведение для ЛЮБОГО существующего/будущего теста (никто
   пока не зовёт `f.pad()` сам → `pad_consumed` всегда false на практике →
   "стримить в главный sb + условный pad_in_place" и "рендерить в fresh sb +
   безусловный внешний nova_fmt_pad" дают ОДИН И ТОТ ЖЕ результат), но НАМНОГО
   меньше риска (не трогает общий `(core, precision_consumed)`-контракт
   функции, которым ТАКЖЕ пользуются radix/int/float-ветки). Реальное
   `pad_in_place`/`pad_consumed`-based type-driven padding — оставлено
   std-side-only плумбингом (см. «Осознанные упрощения» #4, новый пункт).
   `precision_consumed` для этой ветки — теперь ВСЕГДА `true` (было: `true`
   только если `has_display_fmt` сработал) → **owner-одобренное** дропание
   auto-truncate для composite/user-типов (см. решение владельца выше).
3. **`type_ref_to_c`/`extract_protocol_type_name`** (emit_c.rs ~4114/~8666) —
   обновил ДВА стилевых комментария (D419→D422 фрейминг), функционально не
   менял (erasure `Fmt`→`Nova_FmtCtx*`/`Write`→`Nova_StringBuilder*` уже был
   верным и остался).
4. **`lints.rs`** DCE seed-list (~1246-1268, `ExprKind::InterpolatedStr`
   ветка `collect_expr`) — убрал `"display_fmt"` (больше НИКОГДА не
   эмитится хэнд-синтом), добавил `"bare"`, `"rich"` (новые FmtCtx-конструктора)
   + `"width"`/`"align"`/`"fill"`/`"sign"`/`"kind"`/`"pad"` (getter-имена —
   консервативный over-seed на случай, если DCE когда-нибудь научится
   отслеживать их иначе; сейчас они и так достижимы из пользовательского
   `.nv`-кода как обычный видимый AST-вызов, но список и раньше был
   "harmless over-approximation").

**Ловушки, найденные ТОЛЬКО компиляцией (`nova check`), не предугаданы
заранее** — задокументированы инлайн в `protocols.nv`:
- Многострочный `fn Type.ctor(` — парсер требует ПЕРВЫЙ параметр на ТОЙ ЖЕ
  строке, что открывающая `(` (голый перевод строки сразу после `(` —
  синтаксическая ошибка); закрывающая `)` перед `-> RetType` тоже не может
  стоять на своей строке после голого параметра без запятой (нужна либо
  запятая, либо `)` на той же строке, что последний параметр). Тот же
  запрет — внутри record-литерала: `field:` не может быть на отдельной
  строке от своего if/else-выражения-значения.
- `@.method()` — невалидный синтаксис (`E_SELF_DOT_INVALID`); self-как-целое
  метод-вызов — `@method()` (без точки), не `@.method()`. Только для полей
  — `@field`.
- `int as char` запрещён (D54) — нужен `n.to_char()` (`Result[char,_]`),
  паттерн `match n.to_char() { Ok(c) => c, Err(_) => ' ' }` (прецедент —
  `std/src/encoding/hex.nv::digit()`). `FmtCtx.rich`'s `fill`-поле теперь
  строится этим паттерном (комментарий на месте объясняет, почему `Err`
  практически недостижим — `fill_cp` всегда валидный codepoint,
  round-tripped из реального Rust `char`).
- `str` не имеет `.bytes()` ВИДИМОГО внутри `prelude.protocols` САМОГО
  (файл — часть prelude, auto-import пролога отключён внутри
  `std.prelude.*`, cycle protection) при ИЗОЛИРОВАННОЙ проверке ОДНОГО
  файла (`nova check std/src/prelude/protocols.nv` — standalone) —
  `E_UNKNOWN_METHOD`. Это НЕ реальная ошибка (при полной сборке std как
  единого CU `.bytes()` резолвится нормально — подтверждено `nova test`
  на фикстурах, использующих ЭТИ САМЫЕ тела), а артефакт narrow single-file
  `nova check`-инвокации, которая не пуллит транзитивный граф модулей.
  **Вывод для будущих чекпоинтов**: не использовать `nova check <один
  .nv-файл-из-prelude>` как гейт — только `nova test <файл-потребитель>`
  (тянет весь нужный граф) или полный `nova test std`/`build`.

**Ф.2 шаг 2 — верификация (минимальная, синхронно, CPU перегружен):**
- `cargo build --release` (compiler-codegen) — 0 ошибок. ~19с (инкрементально).
- `cargo build --release` (nova-cli) — 0 ошибок. ~3м44с.
- `nova test std/src/runtime/fmt_buf.nv` — PASS (не задевает Fmt/Display
  напрямую — fmt_buf.nv `#no_prelude`, но подтверждает, что базовая
  сборка/линковка std не сломана в целом).
- **Собственная фикстура** (`pa_scratch/fmt_smoke*.nv`, временная, УДАЛЕНА
  перед коммитом — не часть репо) — 6 тестов, все PASS после итерации на
  ловушках выше:
  - примитивы: голая интерполяция (`${n}`/`${fl}`/`${b}`/`${c}`/`${s}`/`${n:?}`)
    — БЕЗ ИЗМЕНЕНИЙ (проверено byte-for-byte равенство ожидаемым строкам).
  - rich-spec на примитивах (`${42:x}`==`"2a"`, `${255:#X}`==`"0xFf"`→
    поправил на `"0xFF"` (мой тест был неверен, не баг: `0x`-префикс всегда
    lowercase — D422 §5 doc-comment это явно фиксирует), `${42:04}`==`"0042"`,
    `${3.14159:.2}`==`"3.14"`, `${hi:>6}`==`"    hi"`) — БЕЗ ИЗМЕНЕНИЙ (тот
    же conv.h прямой путь, не тронут).
  - user-type `@display(mut f Fmt)` bare (`${p}` на `Point{1,2}` →
    `"Point(1, 2)"`) — PASS, изолированный смоук отдельно подтвердил.
  - user-type `@debug(mut f Fmt)` bare (`${p:?}` → `"Point { x: 1, y: 2 }"`)
    — PASS, изолированный смоук отдельно подтвердил.
  - user-type width/align auto-pad под rich-spec (`${p:>20}` →
    `byte_len()==20`) — PASS, изолированный смоук отдельно подтвердил
    (это самый рискованный путь — новая `FmtCtx.rich`-обёртка + внешний
    `nova_fmt_pad` — сработал корректно).
  - Option/Result `@debug` DeclaredBody-роутинг (`${Some(5):?}` →
    `"Some(5)"`, `${Ok(7):?}` (Result[int,str]) → `"Ok(7)"`) — PASS,
    изолированные смоуки отдельно подтвердили новую `emit_bare_fmtctx`
    обёртку на этом отдельном call site.
  - ОДИН ложный TIMEOUT (144с) на полном комбинированном файле при
    ПЕРВОМ прогоне после фикса assert'а — НЕ воспроизвёлся при повторе
    (PASS за штатное время) и НЕ воспроизвёлся ни в одном из 5 изолированных
    под-фикстур по отдельности → похоже на CPU-contention флуктуацию
    (оркестратор явно предупреждал про перегруз), не баг. Отмечаю как
    НЕ ПОЛНОСТЬЮ исключённый риск — если увидим повторяющиеся зависания на
    похожем коде в дальнейших шагах, вернуться к этому.
- **НЕ проверял**: `std/src/encoding/json.nv`/`json_test.nv` (ИЗВЕСТНО
  сломаны — `@display_fmt` больше ничего не диспетчит; JsonValue не
  Display без `@display`) — шаг 3, ниже. `spec_tests/conformance/
  d419_display_fmt_dispatch.nv`/`neg/d419_unknown_spec_neg.nv` — та же
  ситуация, шаг 3. Полный `nova test std`/`nova check std` — НЕ гонял
  целиком (пробовал `nova check std` целиком — не уложился в 590с,
  прервал; это ОТДЕЛЬНО от вопроса корректности — просто дорого гонять
  целиком в каждом чекпоинте; авторитетный гейт = оркестраторский полный
  conformance).

### Ф.2 шаг 3 — ОСТАЁТСЯ (json.nv + d419-фикстуры миграция на D422)

- `spec_tests/conformance/d419_display_fmt_dispatch.nv` — `TaggedD419
  @display_fmt(mut f Fmt)` + `Plain @display(mut w Write)` — ОБА сломаны
  сигнатурой (Write убран, @display_fmt-путь снесён). План миграции 1:1
  (тесты НЕ ослабляются):
  - `TaggedD419` → ОДИН `@display(mut f Fmt)`, тело = union старого
    `@display_fmt` + учёт `f.alternate()`/`f.precision()` (см. разбор ниже).
  - `Plain` → `@display(mut f Fmt) { f.write(@s.bytes()) }`.
  - Ожидаемые assert'ы, ТРЕБУЮЩИЕ пересмотра (не техническая ошибка, а
    легитимное изменение семантики D419→D422, ОДОБРЕНО владельцем
    2026-07-16 — см. «Решение владельца» в шапке чекпоинта):
    - `"${p:.3}" == "abc"` (`Plain{s: "abcdef"}`, precision truncates
      externally под D419) — под D422 (precision auto-truncate ДРОПНУТ для
      composite/user-type ветки — реализовано в шаге 2, `precision_consumed`
      теперь безусловно `true` в `emit_format_spec_value`'s "else" ветке)
      ожидание МЕНЯЕТСЯ на `"${p:.3}" == "abcdef"` (БЕЗ обрезки — `Plain`
      никогда не читает `f.precision()`, и компилятор больше не обрезает
      СНАРУЖИ композит-путь). Owner-approved, реализовать при миграции
      фикстуры.
    - `"${t:>4}" == "   x"` (auto-pad) — остаётся зелёным (внешний
      `nova_fmt_pad`, не меняется — подтверждено изолированным смоуком в
      шаге 2, тот же механизм).
    - `"${t:#}"`/`"${t:.3}"` (TaggedD419 читает `f.alternate()`/
      `f.precision()` сама в своём объединённом `@display`) — остаются
      зелёными (тип сам решает, `FmtCtx.rich` доносит обе оси корректно —
      подтверждено smoke-тестом в шаге 2 на структурно эквивалентном коде).
  - Owner-note: при переименовании — заголовок теста ("D419: ...") и,
    возможно, имя файла (`d419_display_fmt_dispatch.nv` →
    `d422_unified_display_dispatch.nv`?) переписать под D422-ожидание,
    чтобы имя не врало про ретрактированный D419 (явно запрошено
    владельцем).
  - `spec_tests/conformance/neg/d419_unknown_spec_neg.nv` — `@display_fmt`
    → `@display(mut f Fmt)`, EXPECT_COMPILE_ERROR не меняется (парсер
    отвергает `:zz` до типов); возможно тоже переименовать файл под D422.
- `std/src/encoding/json.nv:902` `@display_fmt` → `@display(mut f Fmt)`
  (тело идентично: `if f.alternate() { f.write(@to_str_pretty().bytes()) }
  else { f.write(@to_str().bytes()) }`).
- `std/src/encoding/json_test.nv:208` — тест-имя/комментарий "D419:
  ...(@display_fmt)" → переименовать под D422, assert'ы (`${v:#}` /
  `${v}`) остаются идентичными (JsonValue ТЕПЕРЬ имеет `@display`
  напрямую, не через отдельный display_fmt-хук — то же поведение).

### ✅ СТОП-кандидат — РЕШЁН владельцем 2026-07-16 (был открыт в шаге 1)

**Precision auto-truncate для composite/user-type под rich-spec.** Дефолт
(«дропнуть auto-truncate entirely для composite-пути», Rust-паритет) —
**ОДОБРЕН**. Правка assert'а в `d419_display_fmt_dispatch.nv` = легитимная
миграция ретрактированной D419-семантики, НЕ ослабление теста. Реализовано
в шаге 2 (`emit_format_spec_value`'s composite-ветка теперь безусловно
`precision_consumed = true`). Фикстура сама (шаг 3) — ещё не переписана,
план см. выше.

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
   ДРОПНУТ, owner-approved (см. выше, реализовано в шаге 2).
3. `derived Display` vs `derived Debug` форма — ОДИНАКОВАЯ (D422 §4
   расхождение — Ф.3 scope, не трогал).
4. **(новое, шаг 2)** `@pad`/`pad_consumed` type-driven padding — std-side
   API существует (`FmtCtx.mut @pad(bytes []u8)`, устанавливает
   `pad_consumed`), но `emit_format_spec_value`'s composite-ветка НЕ читает
   `pad_consumed` обратно (не эмитит условный C `if`) — padding всегда идёт
   через безусловный внешний `nova_fmt_pad`, ТОЧНО как до D422. Поведенчески
   нейтрально (ни один существующий/известный тип не зовёт `f.pad()` сам),
   но если Ф.3+ введёт тип, который ХОЧЕТ управлять своим паддингом сам —
   этот путь придётся дописать (стриминг в главный `sb` + `pad_in_place` +
   чтение `pad_consumed` после C-вызова, как было в первоначальном плане
   шага 1 до того, как я его упростил).

## Коммиты (эта сессия, ветка `p208-impl`)

- `72e5523a3` — Ф.2 шаг 1: std-сигнатуры Write([]u8)/Fmt(use Write+оси)/
  FmtCtx/Sign+FmtKind, Display/Debug -> required (mut f Fmt) без default-body.
  auto_derive.rs Write→Fmt + `.bytes()`-обёртки. Компилятор (Rust) собирается
  чисто; `nova test` на std ОЖИДАЕМО красный на тот момент (emit_c.rs ещё не
  переписан).
- (готовится, ЭТОТ коммит) — Ф.2 шаг 2: `emit_c.rs` (`emit_interpolated_str`
  + `emit_format_spec_value`) переписан на `FmtCtx.bare`/`FmtCtx.rich`;
  `@display_fmt`-путь снесён; `lints.rs` DCE seed-list обновлён
  (bare/rich вместо display_fmt). Три `protocols.nv`-фикса, найденные
  компиляцией (многострочный fn-сигнатуры/record-литерал, `@.method()` →
  `@method()`, `int as char` → `n.to_char()` match). Верифицировано
  изолированной фикстурой (6 тестов, все PASS) — bare/rich Display/Debug на
  примитивах (не тронуты) и user-типах (новый путь), Option/Result Debug,
  width/align auto-pad. `std/src/encoding/json.nv`/`json_test.nv` и
  `spec_tests/conformance/d419_*` — ещё сломаны, шаг 3 (следующий).
