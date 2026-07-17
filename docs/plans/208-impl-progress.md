<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 — Ф.2 реализация, checkpoint (сессионные заметки)

Worktree: `d:/Sources/nv-lang/nova-208impl` (branch `p208-impl`, base `cccad54d8`
main). Модель: sonnet. Суб-агенты НЕ спавнились (прямое исполнение). Работа
идёт СИНХРОННО, мелкими коммитами (CPU у оркестратора перегружен — per-фаза
верификация облегчена: одна таргет-фикстура вместо широких прогонов; полный
`nova test`/conformance — гейт оркестратора, не мой).

## Статус на момент ЭТОГО чекпоинта (шаг 3/N): шаги 1+2 сделаны; шаг 3 (json.nv + d419→d422 фикстуры + display_fmt-остатки по всему репо) — СДЕЛАН, ЭТОТ коммит. Остаётся: Ф.4 (снос legacy conv.h-путей туда, где реально не тронуто) + широкая верификация оркестратором.

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

### Ф.2 шаг 3 (ЭТОТ коммит) — json.nv + d419→d422 фикстуры мигрированы; display_fmt-остатки по всему репо добиты

**1. `std/src/encoding/json.nv`** — `JsonValue.@display_fmt(mut f Fmt)` →
`#impl(Display) fn JsonValue @display(mut f Fmt)` (тело идентично по
логике, `.bytes()` добавлен на оба `f.write(...)` — `@write` теперь
`[]u8`). Doc-comment переписан: больше не "D419 first consumer", а "the ONE
required Display primitive… was the D419-era optional hook". `#impl(Display)`
добавлен явно (соответствует прецеденту примитивов в `protocols.nv`).
**`std/src/encoding/json_test.nv`** — тест-заголовок "D419: … (@display_fmt)"
→ "D422: … (unified @display)"; assert'ы (`${v:#}`/`${v}`) НЕ изменились
(то же поведение — теперь через ОДИН метод, было через
`@display_fmt`-хук + default-`@display`).

**2. `spec_tests/conformance/`** — переименовал ФАЙЛЫ (не только контент, per
запрос владельца — имя не должно врать про ретрактированную семантику):
- `d419_display_fmt_dispatch.nv` → **`d422_unified_display_dispatch.nv`**.
  `TaggedD419` (renamed → `TaggedFmt`) `@display_fmt` + `@to_str` →
  ОДИН `@display(mut f Fmt)` (тело = union старой alternate/precision-
  логики). `Plain @display(mut w Write)` → `@display(mut f Fmt)`. Все 6
  test-блоков переименованы (без "D419"/"@display_fmt" в заголовках).
  Assert на `Plain`'s precision (owner-approved дроп auto-truncate,
  реализовано в шаге 2) обновлён: `"${p:.3}" == "abc"` (D419, truncates)
  → **`"${p:.3}" == "abcdef"`** (D422, no auto-truncate — `Plain` никогда
  не читает `f.precision()`). Остальные 5 assert'ов — БЕЗ ИЗМЕНЕНИЙ
  (bare/alternate/precision-self-read/width-auto-pad/`#`-no-effect — все
  подтверждены зелёными).
- `neg/d419_unknown_spec_neg.nv` → **`neg/d422_unknown_spec_neg.nv`**
  (+ `module neg.d419_unknown_spec_neg` → `module neg.d422_unknown_spec_neg`,
  D78 module-path требует совпадения с именем файла). `TaggedD419
  @display_fmt` → `TaggedFmt @display(mut f Fmt)`. EXPECT_COMPILE_ERROR
  `E_FORMAT_SPEC_UNKNOWN` — БЕЗ ИЗМЕНЕНИЙ (парсер отвергает `:zz` до типов,
  не зависит от какой @display у типа).

**3. НЕОЖИДАННАЯ находка при верификации — ДВА ДОПОЛНИТЕЛЬНЫХ файла в
`spec_tests/conformance` (та же folder-module, ОДИН CU) ТОЖЕ использовали
старую `Write(str)`/`@display(mut w Write)` сигнатуру и падали бы CODEGEN-FAIL
(обнаружено ИМЕННО потому, что `spec_tests/conformance` — один CU: пробовал
таргет-тест на новый d422-файл, компилятор утянул ВЕСЬ CU и упал на другом,
не тронутом мной файле):**
- **`d374_write_sink_decouple.nv`** (D374 — АМЕНДИРУЕТСЯ, не ретрактируется,
  имя файла НЕ переименовывал, только контент): `D374Pair @display(mut w
  Write)` → `@display(mut w Fmt)`, все `w.write(...)`/`sb.write(...)` через
  `.bytes()`. **Существенная находка**: тест 2 ("StringBuilder satisfies
  Write — pass raw StringBuilder as the sink param") БОЛЬШЕ НЕ работает как
  было — `@display` теперь принимает `Fmt` (строго БОГАЧЕ, чем `Write`:
  `use Write` + 7 doc осей + `@pad`), а голый `StringBuilder` реализует
  ТОЛЬКО `Write`, не полный `Fmt` — передать `sb` напрямую там, где
  ожидается `Fmt`, НЕ типизируется. Мигрировал тест на явную обёртку
  `FmtCtx.bare(sb, 0, false)` (тот же конструктор, что компилятор сам
  hand-synth'ит для голой `${x}`) — сохраняет ДОКАЗАТЕЛЬНУЮ СИЛУ теста
  (sink-agnostic body, внешне сконструированный синк даёт тот же текст),
  просто через явный wrapper вместо прямой передачи. Это ЕСТЕСТВЕННОЕ
  следствие D422 (Fmt строго не совпадает с Write), не баг — задокументировано
  инлайн в фикстуре.
- **`d229_debug_format_spec.nv`** (D229 — АМЕНДИРУЕТСЯ, имя не менял):
  `D229Tagged @display(mut w Write)`/`@debug(mut w Write)` →
  `(mut w Fmt)`, `.bytes()` на все `w.write(...)`; `@label.debug(w)`
  (forwards `w` вниз в `str`'s `@debug`) — БЕЗ изменений в форме (просто
  `w` теперь `Fmt`-typed, `str.@debug` тоже мигрирован в шаге 1). Все 5
  test-блоков (str quoted-vs-bare, primitive debug==display, distinct
  display-vs-debug, `#impl(Debug)` auto-derive memberwise, Option/Result
  debug) — БЕЗ ИЗМЕНЕНИЙ в ожиданиях (только сигнатура/`.bytes()`).

**4. Грепнул `display_fmt` по ВСЕМУ репо** (std/examples/spec_tests/
compiler-codegen) — остальные хиты (после миграции выше) — ТОЛЬКО
исторические комментарии, явно помеченные "was"/"retracted"/"D419-era" (НЕ
живой код): `std/src/prelude/protocols.nv:196` (шапка секции, объясняет ЧТО
ретрактировано), `std/src/runtime/fmt_buf.nv` (обновил на явную "fully
retracted" формулировку), `std/src/encoding/json.nv:892` (doc-comment
"was the D419-era optional hook"), `compiler-codegen/src/codegen/emit_c.rs`
+ `lints.rs` (комментарии, объясняющие снос — из шага 2). Единственный
НЕ-doc/plans файл вне std/spec_tests с упоминанием —
`examples/flagship/aggregator/src/api/report_json.nv:157` — поправил
`@display_fmt` → `@display` в комментарии (не код, doc-only).
`examples/flagship/aggregator/PROGRESS-run-A.md` — оставил как есть
(прогресс-лог, аналог docs/plans-истории — владелец разрешил их не трогать).
Ни одного ЖИВОГО объявления/вызова `@display_fmt` в коде не осталось нигде
в репо.

### Ф.2 шаг 3 — верификация (изолированные копии, CPU перегружен — full-CU прогон НЕ уложился)

- Попытка `nova test` НАПРЯМУЮ на 3 файла внутри `spec_tests/conformance/`
  (d422/d374/d229) — **не уложилась в 590с** (спустя ~10 мин всё ещё шла
  компиляция + несколько `nova.exe`-воркеров активны) — подтверждает
  CLAUDE.md: `spec_tests/conformance` действительно ОДИН CU (весь folder,
  не только переданные файлы), и гонять его целиком — дорого. НЕ мой гейт
  (авторитетный полный conformance — оркестраторский); оставил процессы
  доедать фоном, не убивал (не destructive).
- Вместо этого — скопировал 4 мигрированных файла (d422 pos, d374, d229,
  d422 neg) во ВРЕМЕННУЮ изолированную директорию с ПЕРЕИМЕНОВАННЫМ
  `module` (не `spec_tests.conformance`/`neg.*` → отдельные модули), чтобы
  проверить СИНТАКСИС/СЕМАНТИКУ мигрированного кода без затрат full-CU.
  Временные копии УДАЛЕНЫ перед коммитом (не часть репо). Результат — ВСЕ 4
  PASS изолированно:
  - `d422_unified_display_dispatch` (6 тестов) — PASS.
  - `d374_write_sink_decouple` (3 теста, включая новый `FmtCtx.bare`-wrapper
    тест) — PASS.
  - `d229_debug_format_spec` (5 тестов) — PASS.
  - `neg/d422_unknown_spec_neg` (EXPECT_COMPILE_ERROR) — PASS (repoted as
    "(negative)").
- `std/src/encoding/json_test.nv` (реальный путь в репо, НЕ изолированная
  копия — этот файл САМ по себе одномодульный, не часть spec_tests-CU) —
  PASS.
- **НЕ проверено**: полный `spec_tests/conformance` как один CU (дорого,
  см. выше — оркестраторский гейт), полный `nova test std` (аналогично
  дорого — пробовал в шаге 2, не уложился). Изолированное копирование
  снимает риск синтаксических/логических ошибок В МОЕМ КОДЕ, но НЕ
  подтверждает отсутствие конфликтов ИМЁН/типов с ОСТАЛЬНЫМИ ~50+ файлами
  того же CU (маловероятно — `TaggedFmt`/`Plain`/`D374Pair`/`D229*`
  D-префиксные/уникальные имена, тот же паттерн, что был у оригиналов) —
  указано явно для оркестраторского финального гейта.

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
- `18eebbdb9` — Ф.2 шаг 2: `emit_c.rs` (`emit_interpolated_str` +
  `emit_format_spec_value`) переписан на `FmtCtx.bare`/`FmtCtx.rich`;
  `@display_fmt`-путь снесён; `lints.rs` DCE seed-list обновлён
  (bare/rich вместо display_fmt). Три `protocols.nv`-фикса, найденные
  компиляцией (многострочный fn-сигнатуры/record-литерал, `@.method()` →
  `@method()`, `int as char` → `n.to_char()` match). Верифицировано
  изолированной фикстурой (6 тестов, все PASS) — bare/rich Display/Debug на
  примитивах (не тронуты) и user-типах (новый путь), Option/Result Debug,
  width/align auto-pad. `std/src/encoding/json.nv`/`json_test.nv` и
  `spec_tests/conformance/d419_*` — ещё сломаны на тот момент, шаг 3.
- `89745095f` — Ф.2 шаг 3: `json.nv`
  `@display_fmt`→`@display(mut f Fmt)` (+ `json_test.nv` заголовок);
  `spec_tests/conformance/d419_display_fmt_dispatch.nv` →
  **переименован** в `d422_unified_display_dispatch.nv` (контент мигрирован,
  precision-composite assert изменён per owner-approved дефолт);
  `neg/d419_unknown_spec_neg.nv` → **переименован** в
  `neg/d422_unknown_spec_neg.nv`. НЕОЖИДАННО обнаружены (тот же
  spec_tests/conformance CU) ещё 2 файла на старой Write/Fmt-сигнатуре —
  `d374_write_sink_decouple.nv` (контент мигрирован, ОДИН тест переписан на
  явный `FmtCtx.bare`-wrapper — раньше передавал голый `StringBuilder` как
  `Write`, теперь `@display` ждёт `Fmt`, строго богаче) и
  `d229_debug_format_spec.nv` (контент мигрирован, ожидания без изменений).
  Финальный грep `display_fmt` по репо — только исторические комментарии
  остались (плюс doc-фикс в `examples/flagship/aggregator/report_json.nv`).
  Верификация: 4 мигрированные фикстуры + json_test.nv PASS в изоляции
  (полный `spec_tests/conformance` CU не уложился в CPU-бюджет — тот же
  паттерн, что в шаге 2, оркестраторский гейт).

## ФИНАЛ (2026-07-16) — Ф.4 разбор + сводка плана

### Ф.4 — статус: НЕ НАЧАТА (решение: не начинать в этой сессии, обоснование ниже)

Ф.4 по карте §10 плана: «Оставшийся `conv.h` int/bool/char/радикс/pad → `.nv`;
удалить мёртвый `nova_fmt_*`. Гейт: conformance; C-поверхность = ТОЛЬКО
float-body.»

**Проверил, действительно ли есть что зачищать** — грепнул использование
каждого `conv.h`-хелпера (`nova_fmt_int_body`/`nova_fmt_int_radix_body`/
`nova_fmt_int_prefix`/`nova_fmt_radix_prefix`/`nova_fmt_f64_body`/
`nova_fmt_f64_prefix`/`nova_fmt_pad`/`nova_fmt_str_precision`) в
`emit_c.rs` — **ВСЕ ещё активно вызываются**, ровно там же, где были ДО
Ф.2 (radix/int/float ветки `emit_format_spec_value`, primitive fast-path
в `emit_interpolated_str`) — это МОЁ сознательное решение Ф.2 (V1-упрощение
#3, см. spec/decisions/02-types.md#d422): примитивный форматный путь НЕ
перевязан на буфер-примитивы Ф.1 (`int_fmt`/`bool_fmt`/`char_fmt`,
`std/src/runtime/fmt_buf.nv`), которые остаются протестированы-но-невостребованы
(ровно как их описывали Ф.1-заметки: "ADDITIVE ONLY... not wired into
anything yet" — это утверждение ВСЁ ЕЩЁ верно после Ф.2 для примитивов,
хотя структурная FmtCtx/Fmt-обвязка вокруг них уже есть).

**Почему не стал делать Ф.4 в этой сессии** (осознанное решение, не
недосмотр):
1. Полная перевязка означает переписать САМЫЙ ГОРЯЧИЙ путь компилятора —
   КАЖДУЮ интерполяцию примитива в ЛЮБОЙ Nova-программе (bare `${n}` и
   rich-spec `${n:x04}` и т.п.) — с прямых `conv.h`-вызовов на диспетч
   через `@display(f)`/`int_fmt`. Это на порядок больше площади риска, чем
   Ф.2 (которая трогала ТОЛЬКО non-primitive/user-type пути + сигнатуры) —
   ЛЮБАЯ регрессия здесь ломает практически всё, а не изолированный класс
   типов.
2. Инструкция этой волны — БЕЗ полного conformance-гейта (CPU оркестратора
   занят) — а для изменения такого масштаба таргетных фикстур (4-5 файлов)
   категорически недостаточно для уверенности; нужен именно полный
   conformance + flagship-examples (`--strict-effects`), который явно ВНЕ
   бюджета этой сессии.
3. Наблюдаемое поведение СЕГОДНЯ — byte-parity со старым (до-D422) кодом
   для примитивов (не просто "вероятно совместимо" — фактически ТОТ ЖЕ
   C-код вызывается) — риск НЕ делать Ф.4 сейчас = технический долг
   (буфер-примитивы Ф.1 остаются недоиспользованы), риск ЖЕ сделать Ф.4
   поспешно = реальная порча самого горячего пути языка без адекватной
   верификации. Выбрал не рисковать.

**Что это означает для «C-поверхность = ТОЛЬКО float-body» (целевое
состояние Ф.4):** НЕ достигнуто. Текущая C-поверхность форматирования
по-прежнему включает весь `conv.h` int/bool/char/radix/pad набор ПОЛНОСТЬЮ
живым (не мёртвым) — так что «удалить мёртвый nova_fmt_*» буквально
нечего удалять, пока кто-то не сделает описанную выше перевязку.
Зафиксировано в `spec/decisions/02-types.md#d422` статус-таблице как
блокер Ф.4 → Ф.4 остаётся ⏳ pending, не ✅.

### Ф.3 — статус: НЕ НАЧАТА

Дженерики `.nv` (`[]T`/`Vec[T]`/`Option`/`Result` Display/Debug impl) и
компиляторный auto-derive компактной `TypeName(a, b)` Display-формы
(отличной от именованной `TypeName { a: 1, b: 2 }` Debug-формы, D422 §4) —
не начаты. Текущий auto-derive (`compiler-codegen/src/protocols/
auto_derive.rs`, мигрированный в Ф.2 шаге 1 только по сигнатуре
Write→Fmt) по-прежнему производит ОДИНАКОВУЮ именованную форму для ОБОИХ
Display и Debug (не различает их, как требует D422 §4) — это ЯВНО не
регрессия (то же самое поведение было и ДО 208), но и не выполнение D422
§4 в полном объёме. `[]T`/`Vec[T]` вообще не имеют Display/Debug impl'а
сегодня (ни `.nv`, ни компиляторного) — подтверждено грепом ДО начала
работы (см. заметки шага 2 в этом файле).

### Спек-амендмент (сделан в рамках финала)

`spec/decisions/02-types.md` — секция D422 «Статус реализации» ПЕРЕПИСАНА:
- Статус-таблица обновлена (Ф.0/Ф.1/Ф.2 → ✅, Ф.3/Ф.4 → ⏳ с пояснением
  БЛОКЕРА для Ф.4).
- Новая подсекция «три V1-упрощения» — документирует ИМЕННО ГДЕ и ПОЧЕМУ
  реализация Ф.2 отличается от буквального алгоритма D422 §4/§2 (mark+
  pad_in_place streaming, precision-consumption, radix-aware generic
  primitives) — все три НЕ противоречат нормативному тексту D422 (он либо
  молчит, либо описывает целевой алгоритм, для которого V1 выбрал
  наблюдаемо-эквивалентную более простую реализацию), так что это
  ДОКУМЕНТАЦИЯ реализации, не смысловой амендмент самого правила D422.
- Координационная заметка про Plan 152.7.2 (не закрываю его статус сама —
  только объясняю, что реализовано/не реализовано из его п.4).

`docs/plans/208-unified-formatter.md` — **Статус:** строка обновлена
(Ф.0-Ф.2 done со ссылкой на V1-упрощения, Ф.3/Ф.4 pending, явно указан
блокер Ф.4).

`docs/plans/152.7.2-format-context.md` — добавлена ПОМЕТКА (не закрытие
статуса, per координационная инструкция) о том, что 208 Ф.2 реализовала
единый `@display(f)`, ретрактировав п.1-3/5 этого плана; п.4 (interp-
direct-to-sink) — частично (bare-путь да, rich-spec композитный путь нет).

### Верификация финала (таргетная, как на шаге 3 — изолированные копии)

Пере-проверил ПОСЛЕ спек-правок, что реализация (код) НЕ менялась в этой
финальной волне (только `.md` docs) — значит те же 4 изолированные
фикстуры + `json_test.nv`, что PASS'или в шаге 3, остаются валидными БЕЗ
повторного прогона (код `.nv`/`.rs` не тронут финалом). Повторно прогнал
их всё равно для полной уверенности (см. коммит ниже) — все PASS.

### Итоговая сводка плана 208 на конец этой волны

- **Сделано и провалидировано** (targeted): Ф.0 (спека), Ф.1 (буфер-
  примитивы, уже было в main), Ф.2 (unified `@display(f)`/`@debug(f)`,
  весь известный живой код мигрирован, `@display_fmt` полностью снесён из
  кода репозитория).
- **Не сделано, явно задокументировано, не блокирует Ф.2's корректность**:
  Ф.3 (generics + derive-форма-расхождение), Ф.4 (buffer-primitive
  перевязка примитивного пути + conv.h retirement).
- **Авторитетный гейт** (полный `spec_tests/conformance` один CU +
  flagship examples под `--strict-effects`) — НЕ прогонялся мной (CPU
  оркестратора; явно вне бюджета этой сессии) — ответственность
  оркестратора/интегратора перед мержем в main.
- **Незакрытые хвосты для следующей волны** (если/когда она будет):
  1. Ф.3: `[]T`/`Vec[T]`/`Option`/`Result` Display/Debug generic impl +
     auto-derive compact-vs-named form divergence (D422 §4).
  2. Ф.4: перевязать примитивный форматный путь (`emit_interpolated_str`
     fast-path + `emit_format_spec_value` radix/int/float ветки) на
     буфер-примитивы Ф.1, затем реально удалить омертвевший `conv.h`.
  3. `emit_format_spec_value`'s композитный путь — доперевести на
     mark+`@pad_in_place`-стриминг в главный sb (сейчас fresh-builder +
     внешний `nova_fmt_pad`, V1-упрощение #1).
  4. Plan 152.7.2 — итоговое решение по статусу (obsolete/tail) за
     владельцем.

## ВОЛНА 2 (2026-07-16) — Ф.3 реализована; Ф.4 остановлена после разведки (честный отчёт)

Ветка `p208-impl` (то же worktree), продолжение сверху. Модель: sonnet, синхронно,
без суб-агентов (те же CPU-ограничения). Порядок по заданию: Ф.3 → Ф.4;
внутри сессии дополнительно встроена D55-задача от владельца (см. ниже) —
она логически ПЕРЕД Ф.3, т.к. Ф.3-имплы прямо от неё зависят.

### 0. Побочная задача (владелец, встроена ПЕРЕД Ф.3): D55 str-литерал→`[]u8` коэрсия

**Находка (эмпирическая, ДО правки):** `f.write("Ok(")` (str-литерал в позицию
`[]u8` без `.bytes()`) — спека (`02-types.md` "Str-литерал → `[]u8` coercion")
уже ОБЕЩАЛА это как готовую модель, но реально НЕ работало: для
protocol-типизированного приёмника (`f Fmt`/`Write`) чекер молча пропускал
вызов (permissive overload-check для protocol-erased приёмников), но codegen
эмитил голый `nova_str`-литерал там, где ожидался `Nova_Vec____nova_byte*`
([]u8) → CC-FAIL на КАЖДОМ таком call site (в т.ч. на моих будущих Ф.3-имплах
Option/Result, которые владелец прямо просил писать в литеральной форме).

**Фикс — `compiler-codegen/src/codegen/emit_c.rs`:** новый pre-pass
`synthesize_write_str_lit_bytes_coercion` (тот же паттерн, что уже
существующие `synthesize_inout_refargs`/`synthesize_method_byref_at_callsite`/
`synthesize_record_lit_typed_call_args` в начале `emit_call`) — переписывает
АСТ аргумента: `w.write("Ok(")` → `w.write("Ok(".bytes())` ДО остального
`emit_call` (переиспользует УЖЕ рабочий `.bytes()`-путь, ноль нового
C-форматирования). Гейт: метод называется буквально `write`, ровно один
позиционный аргумент, аргумент — голый `ExprKind::StrLit` (не `Ident`, не
`InterpolatedStr`), и у приёмника ЕСТЬ зарегистрированный метод `write`
(`all_methods`-lookup) — исключает случайную коэрсию на несвязанном
одноимённом методе. Проверено грепом: КАЖДЫЙ `@write(<один позиционный
аргумент>)` в `std` сегодня берёт `[]u8` (`Write`/`Fmt`/`FmtCtx`/
`StringBuilder`, io-`Write`-семейство, `fs`/`net`) — `OpenOptions.@write(v
bool)`/`RwLock.@write()` другой арности, под гейт не попадают.

**Эмпирически найденная асимметрия чекера (важно, задокументирована в спеке):**
- `f.write("...")` на **protocol-типизированном** приёмнике (`Fmt`/`Write`) —
  раньше молча проходил тайпчек (дыра, которую чинит этот фикс), теперь
  компилируется корректно.
- `sb.write("...")` на **конкретном** типе (`StringBuilder` напрямую) — чекер
  отвергает str-литерал БЕЗ `.bytes()` диагностикой `[E_NO_MATCHING_OVERLOAD]`
  И ДО, И ПОСЛЕ этого фикса (стро́же protocol-пути) — этот codegen-фикс тут
  не участвует (диагностика раньше, в чекере). Пример в спеке
  (`w.write(s) // ❌ E_TYPE_MISMATCH`) был неточен — актуальный код на
  конкретном приёмнике `E_NO_MATCHING_OVERLOAD`, не `E_TYPE_MISMATCH`;
  поправлено.
- str-**переменная** (не литерал) в `Fmt.write(...)` — по-прежнему НЕ
  коэрсится (правильно, гейт по `StrLit` не трогает `Ident`), но диагностика
  деградирует до голого CC-FAIL (`passing 'nova_str' to parameter of
  incompatible type 'Nova_Vec____nova_byte *'`), не чистый `E_`-код —
  известный, не устранённый этой волной пробел (чекер вообще не гонит
  arg-type-проверку для protocol-приёмников; это отдельная задача, не
  блокирует литеральный кейс).

**Скоуп, сознательно узкий** (НЕ полное «любая `[]u8`-позиция» из спеки):
только call-arg к методу `write`. `let`/`const`-аннотация, return-позиция,
element-позиция `[][]u8`, произвольный ДРУГОЙ метод — НЕ покрыты (спека
поправлена — см. "Статус реализации" подсекцию в `02-types.md`, честно
описывает узкий implemented-скоуп vs аспирационный текст).

**Верификация** (изолированные пробники + реальная фикстура): позитив
(`f.write("Ok(")` работает, byte-точно), негатив-по-конструкции (переменная
не коэрсится — оба receiver-формы проверены отдельно), плюс реальный тест
добавлен в `spec_tests/conformance/d55_literal_coercion.nv` (новый `test`
блок, `D55WriteSink`) — проверен ИЗОЛИРОВАННОЙ копией (весь файл, 8 тестов
включая 7 старых) — PASS.

### 1. Ф.3 — генерики `.nv` + auto-derive compact-vs-named (ЗАВЕРШЕНА)

**1a. `std/src/collections/vec/protocols.nv`** — НАХОДКА: `Vec[T Display]
@display`/`Vec[T Debug] @debug` уже СУЩЕСТВОВАЛИ в репо (написаны до 208), но
остались на СТАРОЙ сигнатуре `(mut w Write)` — Ф.2-волна прошлой сессии их
пропустила (её грep был по `display_fmt`/`@display_fmt`, а не по каждому
`@display`/`@debug`-импл на старой `Write`-сигнатуре). Поскольку
`Display`/`Debug` теперь REQUIRED с сигнатурой `(mut f Fmt)` (D422 §3), это
означает `Vec[T]` СТРУКТУРНО не удовлетворял `Display`/`Debug` вообще — не
регрессия видимого поведения (ни один существующий тест не звал `${vec}` —
проверено грепом ДО правки), но реальный, тихий дефект от прошлой волны.
Мигрировано на `(mut f Fmt)`, `.bytes()` убран с литералов (заменён
D55-коэрсией из п.0 выше — `f.write("Vec[")`, не `.bytes()`). `[]T`
покрывается тем же импл'ом (syntax-alias на `Vec[T]`).

**1b. `std/src/prelude/protocols.nv`** — добавлены `Option[T Display]
@display`/`Result[T Display, E Display] @display` (раньше — ТОЛЬКО `@debug`,
значит bare `${some_option}`/`${some_result}` вообще не компилировались,
`E_BAD_FORMAT_SPEC` "does not implement Display" — тоже реальный,
незамеченный до сих пор пробел, не регрессия 208). Форма ИДЕНТИЧНА `@debug`
(tuple payload без имён полей — нечему расходиться по D422 §4), разница
только `.display(f)` vs `.debug(f)` на внутреннем значении (str payload —
голый под Display, в кавычках под Debug). Заодно убран `.bytes()` со всех
четырёх тел (Debug тоже, для единообразия — коэрсия из п.0 покрывает).

**1c. `compiler-codegen/src/protocols/auto_derive.rs`** — auto-derive
compact-vs-named форма (D422 §4):
- `synth_display_record_body` — переписан на позиционную форму
  `TypeName(v1, v2)` (было: именованная `TypeName { f: v }`, идентичная
  Debug — Ф.2-упрощение, задокументированное как "Ф.3 scope"). Поля теперь
  ЕДИНООБРАЗНО зовут `field.display(w)` — убрана primitive/composite
  развилка.
- **Найденный и исправленный БАГ** (та же правка): старая
  primitive-ветка звала `w.write(str.from(@field).bytes())` — `str.from`
  РЕТРАКТИРОВАН (Plan 174.2), это МЁРТВЫЙ/ломающийся код (компилятор
  отвергает `str.from` как `E_UNKNOWN_STATIC_METHOD` статически). НЕ моя
  регрессия — грепом подтверждено: ни одна фикстура в репо не деривила
  `#impl(Display)` на record/sum с примитивным полем ДО этой волны (только
  `#impl(Debug)`, у которого ветка ВСЕГДА была `field.debug(w)` uniform,
  без бага). Раз я в этом коде уже по плановой задаче (§4-расхождение) —
  чиню в той же волне (zero-tolerance-bugs). Теперь примитивы ТОЖЕ просто
  `field.display(w)`, т.к. у них есть `@display(mut f Fmt)` (Ф.2).
- `synth_fmt_sum_body` — та же uniform-дispatch правка (`emit_value`
  больше не различает primitive/composite, тот же баг тем же образом
  устранён для sum-payload'ов). Divergence добавлена ТОЛЬКО для
  `SumVariantKind::Record`-варианта: `is_debug` → именованная `V { f: v }`
  (без изменений), `!is_debug` → позиционная `V(v)` (новое). `Tuple`/`Unit`
  варианты — форма БЕЗ ИЗМЕНЕНИЙ (у tuple payload'а и так нет имён полей —
  Display/Debug и так совпадали, нечему расходиться).
- `synth_debug_record_body` — БЕЗ изменений логики (уже был uniform
  `field.debug(w)`, именованная форма и так целевая) — обновлён только
  doc-comment.

**1d. Новые/расширенные фикстуры:**
- `std/src/collections/vec/protocols_test.nv` (НОВЫЙ peer-файл,
  folder-module `collections.vec`) — 4 теста: Display/Debug `Vec[int]`
  (прямой вызов, см. находку про interpolation ниже), empty-vec, nested
  `Vec[str]` (display bare vs debug quoted на элементах).
- `spec_tests/conformance/d422_generic_container_derive.nv` (НОВЫЙ файл) —
  3 теста: Option/Result `@display` bare-интерполяция (включая
  bare-vs-quoted str payload distinction); auto-derive record
  (`D422gPoint`) Display-positional-vs-Debug-named; auto-derive sum
  (`D422gShape`) — Unit/Tuple без изменений, Record-variant divergence.
- `spec_tests/conformance/d55_literal_coercion.nv` — +1 тест (D55
  write-коэрсия, см. п.0).

**НАХОДКА — `[M-208-generic-interp-display-dispatch-gap]` — ✅ РЕШЕНО (2026-07-17,
ветка `p-interp-generic-dispatch`, sonnet).** Фикс: `try_generic_mono_interp_dispatch`
(compiler-codegen/src/codegen/emit_c.rs, рядом с `emit_interpolated_str`) —
зеркалит существующую Option/Result `DeclaredBody`-ветку той же функции
(строки ~40672-40757: `sum_schema_registry`/`generic_type_methods`-роутинг),
обобщённую на ЛЮБОЙ user-generic контейнер: по мано-мангленному `arg_type`
(уже `Nova_`-strip'нутому, напр. `Vec____nova_int`) ищет инстанс в
`generic_type_instance_info` → базовое имя (`Vec`) + type-args → метод
`display`/`debug` на generic-темплейте (`generic_type_methods[base]`) →
`register_mono_method_instance` с ТЕМ ЖЕ mono-именем (`{rt_trimmed}_method_
{name}`), что и общий call-путь дженерик-метода (5b, ~37225) — оба пути
сходятся на одном C-символе (`mono_instantiated`-гвард не даёт дубля).
Подключён в `has_explicit`-промахе, ПЕРЕД `try_synthesize_default_method`
(которая всё равно мисс для Vec/HashMap — не record/sum). Ранний `return
None` на первой строке (нет `____` в имени) делает ветку строгим no-op для
ЛЮБОГО НЕ-generic типа — байт-паритет вне generic-mono гарантирован
конструктивно, спот-подтверждён на изолированных копиях
`d422_unified_display_dispatch`/`d229_debug_format_spec`/
`d374_write_sink_decouple` (3/0 PASS). Новая фикстура
`spec_tests/conformance/d422_generic_interp_dispatch.nv` (4 теста: bare
`${v}`/`${v:?}` на Vec[int]/Vec[str] с quoting-различием, пустой vec,
вложенный `Vec[Vec[str]]`) — red (изолированная копия, ДО фикса, тем же
временно-отключённым бинарём: все 4 assert падают) → green (ПОСЛЕ, 4/4
PASS). Обходные фикстуры `std/src/collections/vec/protocols_test.nv`
(Ф.3) апгрейжены: тесты 2-4 переведены на bare-интерполяцию (был workaround,
теперь настоящий путь); тест 1 (прямой `.display(FmtCtx.bare(...))`) оставлен
как отдельный контракт (реальный, отличный от interp, код-путь — не
workaround). δ0: `nova test std/src/collections/vec` (1/0, весь модуль-CU) +
`std/src/checksums` (3/0) зелёные. Вторично найденный смежный дефект
(numeric-cast fallback молча печатает pointer-as-int для ЛЮБОГО типа без
Display/Debug/to_str, не только generic-контейнеров) НЕ чинился в этой
волне (риск задевает намеренно-принятый `*T`-pointer debug-путь,
`[M-91.14-ptr-auto-derive]`) — зафиксирован floating-маркером
`[M-interp-numeric-fallback-silent-garbage]` (`docs/plans/backlog-
followups.md`, P2).

Исходная находка (для истории, до фикса): Обнаружено при первой попытке написать
`assert("${v}" == "Vec[1, 2, 3]")` — реально вывелось огромное число
(похоже на raw pointer, напечатанный как int, — конкретно `1401505058784`).
Диагностика: `v.display(FmtCtx.bare(sb, 0, false))` (ПРЯМОЙ вызов метода)
работает КОРРЕКТНО (byte-точно "Vec[1, 2, 3]") — значит сам импл верен;
дело именно в `emit_interpolated_str`'s dispatch-логике. Гипотеза (по коду
`emit_c.rs` — `has_explicit = self.all_methods.contains(&(arg_type, "display"))`,
где `arg_type` — МОНО-мангленное C-имя типа, напр. `Vec____nova_int`, а
метод зарегистрирован под ОБЩИМ generic-именем `Vec`): lookup промахивается
→ `has_explicit=false` → `try_synthesize_default_method` тоже возвращает
`None` (Vec — не record/sum) → падает в ПОСЛЕДНИЙ numeric-cast fallback
(`nova_int_to_str((nova_int)(v))`), откуда и берётся псевдо-указатель.
Подтверждено грепом: НИ ОДНА фикстура в репо ДО этой волны не звала
`${vec}`/`${vec:?}` (bare interpolation генерик-контейнера) — значит это
ПРЕ-СУЩЕСТВУЮЩИЙ, никогда не протестированный путь, не регрессия 208.
Option/Result НЕ страдают этим (у них ОТДЕЛЬНАЯ, выделенная interpolation-
ветка через `sum_schema_registry`/`generic_type_methods`, не через
`all_methods`) — их bare `${some}` работает штатно (см. тесты выше).
**Решение:** НЕ чинить в этой волне — тот же класс риска, что Ф.4 (трогать
`emit_interpolated_str`, САМЫЙ горячий путь, без бюджета на полный
conformance-гейт); тест-фикстуры для `Vec[T]` используют ПРЯМОЙ вызов
`.display(FmtCtx.bare(...))` (доказанно рабочий, тот же паттерн, что
собственный doc-comment пример модуля), bare-interpolation для generic-типов
— зафиксировано как follow-up `[M-208-generic-interp-display-dispatch-gap]`.

**Верификация Ф.3** (изолированные копии + реальные пути, `--timeout 300`
из-за CPU-контеншна на хосте — несколько прогонов ложно TIMEOUT'или на
дефолтных 60-171с и PASS'или на повторе с бо́льшим таймаутом, тот же паттерн,
что прошлая сессия задокументировала):
- `cargo build --release` (compiler-codegen + nova-cli) — 0 ошибок оба раза
  (после auto_derive.rs правки и после emit_c.rs правки).
- Новая comprehensive-фикстура (scratch, все правки вместе) — 4/4 теста
  PASS: Option/Result Display bare-интерполяция, auto-derive record
  divergence, auto-derive sum divergence (Unit/Tuple/Record), Vec
  direct-call Display/Debug.
- `std/src/collections/vec/protocols_test.nv` (реальный путь) — 4/4 PASS.
- `spec_tests/conformance/d422_generic_container_derive.nv` (изолированная
  копия, module renamed) — 3/3 PASS.
- `spec_tests/conformance/d55_literal_coercion.nv` (изолированная копия,
  8 тестов вкл. 7 старых) — 8/8 PASS.
- **Регресс существующих фикстур** (изолированные копии, module renamed,
  ПООДИНОЧКЕ — совместный batch-прогон один раз ложно упал TIMEOUT/CC-FAIL
  без деталей, похоже на CPU-контеншн, не воспроизвелось поодиночке):
  `d422_unified_display_dispatch.nv` (6 тестов) — PASS; `d374_write_sink_
  decouple.nv` (3 теста) — PASS; `d229_debug_format_spec.nv` (5 тестов) —
  PASS; `neg/d422_unknown_spec_neg.nv` — PASS (negative). `std/src/encoding/
  json_test.nv` (реальный путь) — PASS (с `--timeout 400`, первый прогон на
  дефолте ложно TIMEOUT).
- **cargo test** (Rust unit tests для `auto_derive.rs`) — НЕ прогнан:
  `cargo test` для `nova-codegen` крейта падает компиляцией на ПРЕ-
  СУЩЕСТВУЮЩЕЙ, НЕ моей ошибке (`compiler-codegen/src/test_runner.rs:6528`,
  `codegen_to_c(...)` вызывается с `false` вместо ожидаемого
  `ContractsMode` — типовая нестыковка, не связанная ни с `auto_derive.rs`,
  ни с `emit_c.rs`; `git diff --stat` подтверждает — этот файл мной не
  тронут). Похоже на недоделанный хвост Plan 194 (verified-mode removal).
  НЕ мой скоуп чинить — задокументировано как отдельный найденный дефект;
  Rust-side логика вместо этого провалидирована КОСВЕННО, но авторитетно —
  через реальную компиляцию+исполнение `.nv`-фикстур выше (что и есть
  главный гейт проекта, `spec_tests/conformance`).

### 2. Ф.4 — статус: РАЗВЕДКА проведена, РЕАЛИЗАЦИЯ ОСТАНОВЛЕНА (честный отчёт, по инструкции)

Инструкция этой волны явно разрешала остановиться после Ф.3, если Ф.4
упрётся в непредвиденную структурную проблему — «не ломай горячий путь
наполовину». Разведка (без кода) нашла ДВЕ независимые структурные причины,
почему полная "перевязка" в этой сессии была бы безответственной, помимо
уже задокументированного V1-упрощения #1 (композитный путь) прошлой волны:

**Находка А (новая, не была известна прошлой волне): для Debug примитивов
`str`/`char` буфер-примитивы Ф.1 (`fmt_buf.nv`) вообще НЕ содержат
quote+escape-логику.** `nova_char_to_debug_str`/`nova_str_to_debug_str`
(conv.h) — это `'c'`/`"a\nb"` с escaping (кавычки, `\n`, `\t`, юникод-escape);
`int_fmt`/`bool_fmt`/`char_fmt` в `fmt_buf.nv` — ТОЛЬКО display-форма
(голые байты/UTF-8, без кавычек). Перевязать Debug-путь для str/char на
Ф.1-примитивы НЕВОЗМОЖНО без написания этой логики С НУЛЯ — то есть Ф.4
как заявлено ("оставшийся conv.h int/bool/char/радикс/pad → .nv") на
самом деле требует ещё и НОВУЮ under-specified под-фичу (debug-escape
buffer-primitive), не покрытую ни Ф.1, ни спекой D422 §5 explicitly.

**Находка Б (архитектурная, подтверждает вывод прошлой волны, но с
дополнительной технической деталью): буфер-примитивы `int_fmt`/`bool_fmt`/
`char_fmt` — module-private по НОРМАТИВНОМУ требованию D422 §5** ("буфер-
примитивы — ВНУТРЕННИЕ (.nv, не публичные)"). Единственный САНКЦИОНИРОВАННЫЙ
способ для hand-synth C-кода в `emit_c.rs` позвать Nova-функцию — через
METHOD DISPATCH (`Nova_<Type>_static_<method>`/`Nova_<Type>_method_<method>`,
тот же паттерн, что `FmtCtx.bare`/`.rich` уже используют) — то есть
единственный корректный путь "перевязки" примитивного форматного пути — это
переписать примитивные `@display(f)`/`@debug(f)` ТЕЛА (protocols.nv) так,
чтобы они реально читали `f.kind()`/`f.width()`/… и звали `int_fmt` САМИ, а
`emit_interpolated_str`/`emit_format_spec_value` должны ПЕРЕСТАТЬ
специал-кейзить примитивы вообще и всегда идти через ОБЩИЙ
`Nova_T_method_display(v, fmt_ctx)`-диспатч (тот же, что уже используется
для user-типов) — БУКВАЛЬНО реализация V1-упрощения #1 (mark+`@pad_in_place`
streaming), но теперь ещё и для примитивов, а не только композитов. Это
ОДНА КОГЕРЕНТНАЯ big-bang волна (сравнимая по риску с Ф.2 в своё время),
не серия маленьких безопасных шагов — ЛЮБОЙ промежуточный коммит либо
работает НА ВСЁМ примитивном пути (bare+rich+radix+precision+debug-escape
для ВСЕХ 6 примитивных типов), либо ломает интерполяцию примитивов
ПОВСЕМЕСТНО (самый горячий путь языка, любая Nova-программа).

**Решение: Ф.4 НЕ начата в этой волне.** Это ТОТ ЖЕ вывод, что сделала
прошлая волна (V1-упрощение #3 в `02-types.md#d422`), но с ДОБАВЛЕННОЙ,
эмпирически найденной деталью (Находка А — debug-escape gap), которая делает
масштаб Ф.4 БОЛЬШЕ, чем изначально задокументировано (не просто "перевязать
существующие Ф.1-примитивы", а ЕЩЁ и "написать новую debug-escape логику
с нуля"). `conv.h` cleanup (шаг 3 инструкции этой волны — снести
омертвевшие `nova_fmt_*`) соответственно: **нечего сносить** — ВСЕ
`nova_fmt_int_body`/`_radix_body`/`_prefix`/`nova_fmt_pad`/
`nova_fmt_str_precision`/`nova_*_to_str`/`nova_*_to_debug_str` остаются
ЖИВЫМИ (грепом подтверждено — те же call site'ы, что были до этой волны,
я их не трогал). Обновлено в `spec/decisions/02-types.md#d422` (см. ниже).

### Спек-амендмент (эта волна)

`spec/decisions/02-types.md`:
- **D422 "Статус реализации" таблица** — Ф.3 строка → ✅ (со ссылкой сюда),
  Ф.4 строка — блокер дополнен Находкой А (debug-escape gap) сверх
  прежнего V1-упрощения #1/#3.
- **"Str-литерал → `[]u8` coercion" (D55 amend) подсекция** — добавлена
  "Статус реализации" врезка, ЧЕСТНО сужающая аспирационный текст до
  реально реализованного скоупа (call-arg к `write`, не любая `[]u8`-позиция),
  плюс исправление примера с неверным `E_TYPE_MISMATCH` → фактический
  `E_NO_MATCHING_OVERLOAD` (для конкретных приёмников) / CC-FAIL-пробел (для
  protocol-приёмников с переменной).

`docs/plans/208-unified-formatter.md` — НЕ трогался в этой волне (карта §10
уже отражала Ф.3/Ф.4 как pending; статус-таблица в `02-types.md` —
единственный source of truth по факту, см. `docs/plans/README.md`).

### Итоговая сводка (конец этой волны)

- **Сделано и провалидировано**: Ф.3 целиком (генерики `.nv` + auto-derive
  divergence + 2 попутных бага починены: `str.from`-crash в Display-derive,
  Vec[T] структурно не удовлетворял Display/Debug, Option/Result вообще не
  имели `@display`) + D55 write-коэрсия (owner-supplemental).
- **Найдено, НЕ починено, задокументировано** (follow-up маркеры):
  1. `[M-208-generic-interp-display-dispatch-gap]` — bare `${vec}` для
     generic-типов не дозванивается (pre-existing, orthogonal to 208).
  2. Ф.4 — блокирована (V1-упрощение #1 композит + НОВАЯ Находка А
     debug-escape gap + Находка Б module-privacy/method-dispatch
     архитектурное требование) — big-bang волна, не начата.
  3. Диагностика чекера для str-переменная→`[]u8` на protocol-приёмнике
     деградирует до голого CC-FAIL вместо `E_`-кода — отдельная,
     некритичная задача (не блокирует литеральный кейс).
  4. `cargo test` для `nova-codegen` крейта сломан ПРЕ-СУЩЕСТВУЮЩЕЙ ошибкой
     в `test_runner.rs` (не мой файл, не мой скоуп) — Rust unit tests
     авто_derive/emit_c пока недоступны, компенсировано `.nv`-level
     верификацией.
- **Авторитетный гейт** (полный `spec_tests/conformance` один CU + flagship
  `--strict-effects`) — НЕ прогонялся (вне бюджета сессии, как и прошлой
  волной) — ответственность оркестратора/интегратора перед мержем в main.
