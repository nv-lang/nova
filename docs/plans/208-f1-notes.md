<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 208 Ф.1 — сессионные заметки (checkpoint)

Worktree: `d:/Sources/nv-lang/nova-208f1` (branch `plan208-f1`, base `edcc4ab73`).
Модель: sonnet. Суб-агенты НЕ спавнились (прямое исполнение).

## Статус: Ф.1 ЗАВЕРШЕНА (все 3 пункта)

### 1. Буфер-примитивы (.nv) — `std/src/runtime/fmt_buf.nv` (НОВЫЙ файл)

- `type FmtSpec { width, radix, upper, zero_pad, sign_plus, alt }` + `FmtSpec.new()`
  (module-private — не публичный API, как требует D422 §5).
- `fn int_fmt(v int, buf *mut u8, cap int, spec FmtSpec) -> int` — digit-loop,
  радикс 10/16/8/2, zero-pad, sign, alt-prefix (`0x`/`0o`/`0b`). Радикс !=10
  реинтерпретирует биты как unsigned (Rust `{:x}` of -1 == "ff..ff"), матчит
  существующий `nova_fmt_int_radix_body` (conv.h).
- `fn bool_fmt` / `fn char_fmt` (UTF-8 encode, invalid→U+FFFD).
- `export type Align enum Left | Right | Center` (`#unstable`) — нужен
  cross-module для StringBuilder-аменда.
- `export type FloatKind enum Shortest | Fixed | Sci` (`#unstable`).
- `extern "C" fn fmt_f64_into(...)` (литеральное имя, D282) + `.nv`-wrapper
  `fn fmt_f64(...)` конвертящий `FloatKind`→`int` на границе.
- Файл `#no_prelude` (избегает re-open цикла prelude→collections→string_builder,
  раз Align импортируется string_builder.nv).
- Тесты — ВНУТРИ этого же файла (`test {...}` блоки в конце), НЕ отдельный
  `fmt_buf_test.nv` peer: `std/src/runtime/` держит один-файл-один-модуль
  (в отличие от `vec/`), так что peer-файл был бы ОТДЕЛЬНЫМ модулем
  (`E_D78_MODULE_PATH_MISMATCH` подтвердил это на практике) и не видел бы
  non-exported примитивы без экспорта. Инлайн-тесты — прецедент в std
  (`hashmap.nv`/`range.nv`/`vec_iter.nv`/`duration.nv` тоже мешают impl+test).

### 2. `fmt_f64_into` (C-extern, буфер-форма) — `compiler-codegen/nova_rt/nova_rt.h`

Добавлен СРАЗУ ПОСЛЕ `nova_f32_shortest` (рядом с существующим dtoa).
`static inline nova_int fmt_f64_into(uint8_t* buf, nova_int cap, double v,
nova_int kind, nova_int prec)` — kind 0=Shortest (делегирует в существующий
`nova_f64_shortest`), 1=Fixed (`%.*f`), 2=Sci (`%.*e`); truncates defensively
if `cap` < rendered length. `static inline` (не отдельный non-static symbol) —
избегает multiple-definition при линковке нескольких .c TU, включающих
nova_rt.h (driver.c/effects.c/eventloop.c/runtime.c).

### 3. `StringBuilder` аменд (D179, аддитивно) — `std/src/runtime/string_builder.nv`

Добавлены (существующие методы НЕ тронуты):
- `mut @reserve(n int) -> *mut u8` — `@buf.reserve(n)` + возврат
  `@buf.ptr().offset(@buf.len())` (указатель на spare capacity).
- `mut @advance(n int) -> ()` — ребилдит `@buf` через публичный мост
  `[]u8.new(ptr, len, cap)` (data/len/cap — priv поля `collections.vec`,
  это единственный санкционированный способ).
- `mut @write_padded(bytes []u8, width int, fill char, align Align) -> ()` —
  известная длина, БЕЗ сдвига (append в правильном порядке: fill/content по
  align).
- `mut @pad_in_place(mark int, width int, fill char, align Align) -> ()` —
  стриминговый композит, memmove (`RawMem.copy`, overlap-safe) + вставка fill
  для Right/Center; Left — только trailing append (без сдвига).
- `consume @into_str_checked() -> Result[str, Utf8Error]` — делегирует в
  `[]u8 @decode_utf8()` (НЕ `@to_str()` — см. ловушка ниже).
- Приватные хелперы: `utf8_char_count`, `char_utf8_len`, `char_utf8_bytes`.
- Новые импорты: `std.runtime.fmt_buf.{Align}`, `std.runtime.raw_mem.{RawMem}`,
  `std.prelude.core.{Result}`, `std.runtime.string.{Utf8Error}`.

**Тесты** — отдельный peer-файл `std/src/runtime/string_builder_test.nv`
(module `runtime.string_builder_test`, ОТДЕЛЬНЫЙ модуль — методы все
`export`, так что explicit-import работает штатно, паттерн как
`sync.nv`/`sync_test.nv`).

## Ловушки, найденные и обойдённые (задокументированы инлайн в коде)

1. **Модель модулей `std/src/runtime/`**: папка НЕ «один модуль» (в отличие
   от `vec/`) — каждый файл тут ОТДЕЛЬНЫЙ модуль (`runtime.X`). Peer-тест
   `fmt_buf_test.nv` был бы модулем `runtime.fmt_buf_test`
   (E_D78_MODULE_PATH_MISMATCH это подтвердил), который НЕ видит
   non-exported символы без явного экспорта. Решение: тесты для
   non-exported примитивов — инлайн в том же файле; тесты для exported
   StringBuilder-методов — отдельный peer-файл с explicit import.
2. **Record-spread без type context**: `ro spec = { ...FmtSpec.new(), width: 4 }`
   даёт `codegen error: cannot determine type for spread` — нужен явный
   тип-конструктор слева: `ro spec = FmtSpec { ...FmtSpec.new(), width: 4 }`.
3. **[M-174.1-to-str-name-collision-codegen-bug]** (уже задокументирован в
   `std/src/runtime/string/core.nv`, ПРЕ-СУЩЕСТВУЮЩИЙ баг, не мой): вызов
   `@buf.to_str()` внутри `string_builder.nv` мискодогенился
   (`Nova_Nova_Vec____nova_byte_method_to_str` вернул `nova_str` вместо
   `Result`-обёртки) — потому что тот же CU (весь `std`) содержит другие
   несвязанные `T.to_str() -> str` (NetError, SocketAddr, …). Обошёл через
   задокументированный воркараунд-twin `@decode_utf8()` (идентичное тело,
   другое имя) вместо `@to_str()`.
4. **`consume`-обязательный `StringBuilder`**: тесты обязаны писать
   `consume sb = StringBuilder.new()`, не `mut sb = …` (D133/D180
   E_CONSUME_KEYWORD_MISSING) — `consume`-биндинг уже позволяет звать
   `mut`-методы без доп. `mut` на самой переменной.

## Верификация (targeted, БЕЗ полного conformance — по инструкции)

- `cargo build --release` (compiler-codegen + nova-cli): 0 ошибок (только
  pre-existing warnings, не мои).
- `nova test std/src/runtime/fmt_buf.nv` — PASS (int_fmt decimal/zero_pad/
  radix-hex-upper-alt/oct/bin/negative-bitcast/cap-truncation, bool_fmt,
  char_fmt 1-4 byte UTF-8, fmt_f64_into Shortest/Fixed/Sci, fmt_f64 wrapper).
- `nova test std/src/runtime/string_builder_test.nv` — PASS (reserve/advance
  zero-copy protocol, write_padded left/right/center/no-op, pad_in_place
  right/left/center/no-op, into_str_checked Ok/Err).
- Оба файла — PASS и под `--mode release`.
- `nova test std/src/runtime/ std/src/collections/vec/` — PASS: 4, FAIL: 0
  (широкий smoke, включая существующий `sync_test`/`vec/access`).
- Регресс существующих format-тестов: `nova_tests/plan154_1/*`,
  `nova_tests/protocols/comparison/display.nv` СЕЙЧАС падают на
  `E_UNKNOWN_STATIC_METHOD str.from` — но ЭТО ПРЕ-СУЩЕСТВУЮЩЕЕ (проверено:
  тот же результат на нетронутом `nova.exe` из ГЛАВНОГО репо на том же
  коммите, str.from был ретрактирован раньше по Plan 174.2) — НЕ регресс
  Ф.1. `plan91_14/t10_pos_debug_in_concat_with_display` и
  `unicode/neg/plan152_7_*` — PASS (без изменений).

## Хэши / файлы

Коммиты — по шагам (см. `git log`). Изменённые/новые файлы:
- `compiler-codegen/nova_rt/nova_rt.h` (аддитивно, +~35 строк)
- `std/src/runtime/fmt_buf.nv` (новый)
- `std/src/runtime/string_builder.nv` (аддитивно)
- `std/src/runtime/string_builder_test.nv` (новый)
- `docs/plans/208-f1-notes.md` (этот файл)

## Неопределённости / открытые вопросы для Ф.2

- `FmtSpec` — НЕ то же самое, что будущий компилятор-facing `FormatSpec`/`Fmt`
  (Ф.2). Узкий, только под `int_fmt`. Ф.2 решит, как их состыковать
  (возможно `FormatSpec` будет строить `FmtSpec` перед вызовом `int_fmt`).
- `Sign`/`FmtKind` энумы НЕ введены в Ф.1 (вне скоупа — Fmt-протокол-оси,
  Ф.2). `Align`/`FloatKind` введены (нужны прямо сейчас).
- `@pad_in_place`/`@write_padded` принимают `Align` — Ф.2 `Fmt.@align()`
  должен возвращать тот же тип (`Option[Align]` per §9) — переиспользовать
  этот же `runtime.fmt_buf.Align`, не заводить второй.
- `[M-174.1-to-str-name-collision-codegen-bug]` остаётся ОТКРЫТЫМ
  (pre-existing, не мой скоуп) — Ф.2/дальнейшие волны, вызывающие
  `[]u8 @to_str()`/`@decode_utf8()` внутри общего CU с пользовательскими
  `T.to_str()`, должны знать про воркараунд.
