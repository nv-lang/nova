# План: Split Buffer на StringBuilder/WriteBuffer/ReadBuffer + keyword `external`

**Статус: ✅ ЗАКРЫТ ПОЛНОСТЬЮ (2026-05-08).** Все 6 этапов выполнены:
spec, codegen, runtime, тесты, финализация, **Buffer удалён из языка**.

Финальное состояние:
- ✅ Этап 1 spec: D82 external + D26 prelude split + Q-buffer ❌ REMOVED.
- ✅ Этап 2 codegen: KwExternal token, parser, AST, dispatch table.
- ✅ Этап 3 runtime: string_builder.h / write_buffer.h / read_buffer.h.
- ✅ Этап 4 тесты: 15 + 14 + 15 = 44 теста для трёх типов.
- ✅ Этап 5 финализация: regression — 78/78 nova_tests PASS.
- ✅ Этап 6 удаление Buffer: codegen dispatch удалён (record_schemas
  + 5 групп special-case'ов), `nova_rt/buffer.h` удалён, `nova_rt.h`
  reference убран, `nova_tests/runtime/buffer.nv` удалён.
  `WriteBuffer @write_char/@write_str` добавлены для mixed text+binary
  (Plan 04 Этап 6.1). Sweep std/: 14 файлов мигрированы на StringBuilder
  (text-only); url.nv decode_query — на WriteBuffer + str.try_from
  (mixed). Q-buffer ❌ REMOVED.

**Изначальный план был:** в работе (2026-05-08). StringBuilder runtime
(`nova_rt/string_builder.h`) + `std/runtime/builtins.nv` с
external-fn декларациями + codegen dispatch для StringBuilder
методов готовы. WriteBuffer/ReadBuffer headers созданы.
**Преректы обоих планов 08 и 11 на нужном уровне:**
- Plan 08: Ф.1-Ф.5+Ф.7 закрыты (4-way auto-derive, str.from/char.try_from
  работают).
- Plan 11: Ф.1-Ф.3+Ф.4.5+Ф.6 закрыты (overload по arg-типу, mangling,
  Self в expression).

**Изначальные зависимости (для истории):**

### Зависимости от плана 08 (большая часть закрыта 2026-05-08)

1. **`str.from(c char)` через auto-derive D73** — этот план рассчитывает
   что 4-way auto-derive работает (генерирует `c.into() -> str` из
   `str.from(c char)`). Реализуется **в плане 08 Ф.3** ✅ ЗАКРЫТО.
2. **Bootstrap-table для `external fn`** — механизм lookup'а
   runtime-функций по имени (`Nova_StringBuilder_static_new`, etc.)
   архитектурно идентичен **плану 08 Ф.2** (registry built-in
   conversions) ✅ ЗАКРЫТО. Делать одну инфраструктуру, не две.
3. **18 числовых типов в WriteBuffer.@write_uN_le/be** — они
   регистрируются в плане 08 как полноценные типы с конверсиями
   (`u32 ↔ int`, `f32 ↔ f64` через `as`/`from`). До плана 08
   тип `u32` в API параметрах может работать как-попало (в bootstrap
   compiler сейчас type-erasure всё в `nova_int`).
4. **`external` keyword может быть избыточен** — после плана 08
   bootstrap-table уже работает для всех built-in conversions без
   явного `external` keyword'а. Возможно `external` нужен только
   для типов которые не покрываются registry. Решается после плана 08.

### Зависимости от плана 11 (overload по типу аргумента)

5. **Overload static-методов:** API плана 04 содержит несколько
   `T.from(...)` на одном receiver-типе:
   ```nova
   export external fn StringBuilder.from(s str)  -> Self
   export external fn StringBuilder.from(c char) -> Self
   ```
   В bootstrap'е сейчас (до плана 11 Ф.1-Ф.2) `method_receivers`
   key = только имя метода → последнее объявление **переписывает**
   первое. Без плана 11 — only одна форма работает.

6. **Overload instance-методов:** то же для `@append`:
   ```nova
   export external fn StringBuilder mut @append(s str)  -> ()
   export external fn StringBuilder mut @append(c char) -> ()
   ```
   Без плана 11 — last-wins.

7. **C-side mangling для overloaded `external` методов.** План 11 Ф.3
   требует mangling по сигнатуре (`Nova_T_method_<param_types>`).
   Для `external fn` с overload — runtime должен предоставить **обе**
   функции под mangled именами. План 04 диктует как это связано с
   Nova-side declarations.

### Что план 04 добавляет поверх 08+11

- **`external` keyword** — отдельная новая фича дизайна.
- **Три новых типа** в prelude (StringBuilder/WriteBuffer/ReadBuffer)
  — расширение D26.
- **Декларация типов как built-in opaque** (без `type X { ... }` block'а).
- **Соглашение `std/runtime/builtins.nv`** для documentation-stub'а.

После планов 08 и 11 этот план можно сильно упростить —
infrastructure (bootstrap-table, overload-resolution, auto-derive)
уже будет, остаётся только **новая семантика** (`external` keyword,
opaque типы).

**Контекст обсуждения:** разговор про Buffer API → добавить endianness-методы → выявилось семантическое смешение text+binary в одном `Buffer` → split на три специализированных типа + расширение D26 prelude + новый keyword `external` для runtime-implemented функций stdlib.

## Краткая суть

Текущий унифицированный `Buffer` (Q-buffer, ✅ closed 2026-05-07) **смешивает text-domain и binary-domain** в одном типе:
- `add_str` / `add_char` — текст;
- `add_byte` / `add_bytes` — байты;
- `try_into() -> Result[str, Utf8Error]` — UTF-8 валидация на финализации.

Это создаёт три проблемы:
1. **Нет type-safety** — программист может писать str+bytes mixed, потом `try_into()` falls.
2. **`@into() -> str` всегда fallible** даже если писали только str/char (UTF-8 invariant **уже** гарантирован).
3. **Endianness-методы для бинарных протоколов** не вписываются в один глагол с текстовыми (`add_u32_le` рядом с `add_str` — несогласованно).

**Решение:** split на три типа со специализированной семантикой + `external` keyword для stdlib runtime-функций.

## Финальная архитектура

### Три типа

```
┌─────────────────────┬──────────────┬──────────────────────────────┐
│  Тип                │  Глагол       │  Финализация                 │
├─────────────────────┼──────────────┼──────────────────────────────┤
│  StringBuilder      │  @append      │  @into() -> str (infallible) │
│  WriteBuffer        │  @write_*     │  @into() -> []byte           │
│  ReadBuffer         │  @read_*      │  (no into, view)             │
│                     │  @try_read_*  │                              │
└─────────────────────┴──────────────┴──────────────────────────────┘
```

**StringBuilder** — UTF-8 string accumulator. Auto-grow, append-only. `@into()` infallible (UTF-8 invariant поддерживается каждым `@append`).

**WriteBuffer** — binary serialization buffer. Auto-grow, append-only. Methods для byte/bytes + 18 числовых типов × LE/BE.

**ReadBuffer** — cursor-style binary reader. View над `[]byte`, advancing position. Pair `@read_*` (Fail) / `@try_read_*` (Result), auto-derive на C-runtime уровне.

`[]byte` остаётся как есть — без random-access read/write методов. (Q-binary-io не открываем — преждевременно).

### Keyword `external`

**Новая фича дизайна.** Декларирует функции stdlib с runtime-implementation (C-код в `nova_rt/`):

```nova
export external fn StringBuilder.new() -> Self
export external fn StringBuilder mut @append(s str) -> ()
```

**`external` только для функций**, не для типов. StringBuilder/WriteBuffer/ReadBuffer — built-in opaque типы (как `int`/`str`/`bool`), компилятор их знает по имени, **отдельной декларации типа нет**.

**Грамматика:**

```
fn-decl = ['export'] ['external'] 'fn' [receiver] ...
```

Порядок modifiers — `export` сначала, `external` после. По примерам OCaml/Dart/Kotlin (`external` — полное слово, согласовано с D30 «полные слова, не сокращения»).

`external fn` без тела — codegen lookup'ит в hard-coded таблице C-функций (`Nova_StringBuilder_static_new`, etc.).

### char ↔ str через D73

```nova
export external fn str.from(c char) -> Self      // 1-4 UTF-8 bytes from codepoint

// D73 auto-derive:
// fn char @into() -> str => str.from(@)
```

Replaces текущий `Buffer.add_char` и `from(char)` логику — теперь UTF-8 encode на уровне str.

## Полные API контракты

Все три типа описаны в новом файле `std/runtime/builtins.nv` как documentation-stub с `external fn` декларациями:

### StringBuilder

```nova
export external fn StringBuilder.new() -> Self
export external fn StringBuilder.with_capacity(n int) -> Self
export external fn StringBuilder.from(s str)  -> Self
export external fn StringBuilder.from(c char) -> Self

export external fn StringBuilder mut @append(s str)  -> ()
export external fn StringBuilder mut @append(c char) -> ()

export external fn StringBuilder @len()      -> int
export external fn StringBuilder @capacity() -> int
export external fn StringBuilder @clone()    -> Self
export external fn StringBuilder @into()     -> str    // infallible
```

### WriteBuffer

```nova
export external fn WriteBuffer.new() -> Self
export external fn WriteBuffer.with_capacity(n int) -> Self
export external fn WriteBuffer.from(b []byte) -> Self

export external fn WriteBuffer mut @write_byte(v byte)      -> ()
export external fn WriteBuffer mut @write_bytes(src []byte) -> ()

// Text → UTF-8 bytes (нужно для смешанных text+binary use-case'ов:
// percent-decoding URL, multi-byte sequences). Кодирует char/str как
// UTF-8 байты и пишет их в буфер. Финализация через @into() даёт
// `[]byte`; для конверсии в str — `str.try_from(bs)?` (UTF-8
// validate, D77).
//
// Use-case: url.nv decode_query — смешивает percent-decoded байты
// (могут быть multi-byte UTF-8 sequences) и обычные char'ы.
// StringBuilder не подходит — он text-only, не поддерживает raw
// byte append. WriteBuffer + write_char/write_str позволяет
// единообразно накапливать UTF-8 bytes.
export external fn WriteBuffer mut @write_char(c char)      -> ()  // 1-4 UTF-8 bytes
export external fn WriteBuffer mut @write_str(s str)        -> ()  // UTF-8 bytes напрямую

// 18 числовых × LE/BE:
export external fn WriteBuffer mut @write_u8(v u8)           -> ()
export external fn WriteBuffer mut @write_i8(v i8)           -> ()
export external fn WriteBuffer mut @write_u16_le(v u16)      -> ()
export external fn WriteBuffer mut @write_u16_be(v u16)      -> ()
export external fn WriteBuffer mut @write_u32_le(v u32)      -> ()
export external fn WriteBuffer mut @write_u32_be(v u32)      -> ()
export external fn WriteBuffer mut @write_u64_le(v u64)      -> ()
export external fn WriteBuffer mut @write_u64_be(v u64)      -> ()
export external fn WriteBuffer mut @write_i16_le(v i16)      -> ()
export external fn WriteBuffer mut @write_i16_be(v i16)      -> ()
export external fn WriteBuffer mut @write_i32_le(v i32)      -> ()
export external fn WriteBuffer mut @write_i32_be(v i32)      -> ()
export external fn WriteBuffer mut @write_i64_le(v i64)      -> ()
export external fn WriteBuffer mut @write_i64_be(v i64)      -> ()
export external fn WriteBuffer mut @write_f32_le(v f32)      -> ()
export external fn WriteBuffer mut @write_f32_be(v f32)      -> ()
export external fn WriteBuffer mut @write_f64_le(v f64)      -> ()
export external fn WriteBuffer mut @write_f64_be(v f64)      -> ()

export external fn WriteBuffer @len()      -> int
export external fn WriteBuffer @capacity() -> int
export external fn WriteBuffer @clone()    -> Self
export external fn WriteBuffer @into()     -> []byte
```

### ReadBuffer

```nova
export external fn ReadBuffer.from(b []byte) -> Self    // view, no copy

export external fn ReadBuffer @position()           -> int
export external fn ReadBuffer @remaining()          -> int
export external fn ReadBuffer @has_remaining(n int) -> bool
export external fn ReadBuffer @remaining_bytes()    -> []byte    // copy of remaining

// Throwing form (Fail[ReadBufferError])
export external fn ReadBuffer mut @read_byte()       Fail[ReadBufferError] -> byte
export external fn ReadBuffer mut @read_bytes(n int) Fail[ReadBufferError] -> []byte
export external fn ReadBuffer mut @read_u8()         Fail[ReadBufferError] -> u8
// ... 18 числовых × LE/BE (читай аналогично write_*)

// Try form (Result[T, ReadBufferError]) — auto-derived на C-runtime уровне
export external fn ReadBuffer mut @try_read_byte()       -> Result[byte, ReadBufferError]
export external fn ReadBuffer mut @try_read_bytes(n int) -> Result[[]byte, ReadBufferError]
// ... все 18 числовых × LE/BE

// Block D73 auto-derive of @into() — ReadBuffer is a view, not a value to convert
fn ReadBuffer @into() Fail[Error] -> () =>
    throw Error.new("ReadBuffer.@into() is not supported; use @remaining_bytes() instead")
```

### ReadBufferError

```nova
export type ReadBufferError 
    | UnexpectedEnd { wanted int, available int }
```

### char ↔ str

```nova
export external fn str.from(c char) -> Self    // UTF-8 encode 1-4 bytes
// D73 синтезирует: fn char @into() -> str
```

## Auto-derive read/try_read на C-runtime уровне

Программист stdlib **не пишет вручную** `@read_*` и `@try_read_*` отдельно. **Одна C-функция** на каждый числовой × LE/BE (~18 functions for read, аналогично write). Codegen эмитит **обе Nova-сигнатуры** — обе вызывают одну C-функцию:

- `@read_*` (Fail-form): C возвращает success+value или error → wrapper делает `throw` через `Nova_Fail_fail`.
- `@try_read_*` (Result-form): тот же C-результат → wrapper упаковывает в `Result.Ok(v)` / `Result.Err(e)`.

Это **минимизирует C-код в 2 раза** (~18 functions вместо 38) и поддерживает D77 «программист пишет одну форму, обе доступны».

## Naming правила (D30 расширение)

**Полные слова, не сокращения** — фиксируется в D30:

> Имена методов, типов, параметров и полей — **полные слова**, не сокращения.
> `@capacity()` не `@cap()`, `@position()` не `@pos()`, `@destination` не `@dest`.
>
> **Mainstream-исключения** (Rust/Go convention):
> - `len` — длина коллекции (вместо `length`).
> - `iter` — итератор (вместо `iterator`).
> - `idx` — index (только в локальных переменных).
>
> **Запрещены ad-hoc сокращения**: `pos`/`cap`/`dest`/`src`/`buf`/`val`/`tmp`.
>
> **Operator overloading имена** (`@plus`, `@rem`, `@neg`, ...) — фиксированы по D46, не подчиняются правилу.

## Этапы реализации

### Этап 1 — спека (~1 час)

1. **Создать D82** в `spec/decisions/08-runtime.md` — D-блок про `external` (только для функций):
   - Семантика `external fn` (declaration без тела, runtime-implementation).
   - `export external fn` для public.
   - Связь с D26 prelude, D5/D47 видимость, будущим FFI (`extern("C")`).
   - Built-in opaque types (StringBuilder/WriteBuffer/ReadBuffer) — упомянуть что они **известны компилятору как `int`/`str`**, отдельной декларации типа нет.

2. **Обновить D30** в `spec/decisions/03-syntax.md` — раздел «Полные слова, не сокращения»:
   - Правило, mainstream-исключения (`len`/`iter`/`idx`), запрет ad-hoc.

3. **Обновить D26** в `spec/decisions/08-runtime.md` — добавить:
   - `StringBuilder`, `WriteBuffer`, `ReadBuffer` как built-in opaque типы (категория рядом с примитивами).
   - `ReadBufferError` как sum-тип в prelude.
   - Reference на `std/runtime/builtins.nv` для деталей API.

4. **Обновить D5/D47** — короткое упоминание что `export external fn` валидно (порядок modifiers).

5. **Закрыть Q-buffer** в `spec/open-questions.md`:
   - Пометить `REPLACED → Q-string-builder + Q-write-buffer + Q-read-buffer`.
   - Сохранить раздел «Эволюция» с объяснением «split на три типа из-за смешения text+binary».

6. **Открыть Q-string-builder, Q-write-buffer, Q-read-buffer** как closed Q-блоки:
   - Каждый — полная спека API + обоснование + что отвергнуто.
   - Q-read-buffer: явно описать auto-derive read/try_read.

7. **Обновить Q-stdlib-minimal-api** — заменить упоминания Buffer на split.

8. **Создать `std/runtime/builtins.nv`** — documentation-stub с external-декларациями всех методов.

### Этап 2 — bootstrap codegen (~3-4 часа)

В `compiler-codegen/`:

1. **Lexer** (`src/lexer/mod.rs`, `src/lexer/token.rs`):
   - Добавить `KwExternal` token.
   - Маппинг `"external" => TokenKind::KwExternal`.
   - Display `"`external`"`.

2. **Parser** (`src/parser/mod.rs`):
   - Расширить `parse_fn_decl` распознавать `external` modifier (после `export`, перед `fn`).
   - Если `external` — body должен отсутствовать, иначе compile error.

3. **AST** (`src/ast/mod.rs` или где FnDecl):
   - Добавить `is_external: bool` flag.

4. **Codegen** (`src/codegen/emit_c.rs`):
   - Hard-coded dispatch table для `StringBuilder`, `WriteBuffer`, `ReadBuffer`:
     - Static methods (`StringBuilder.new` → `Nova_StringBuilder_static_new`).
     - Instance methods (`sb.append(s)` → `Nova_StringBuilder_method_append_str`).
     - Methods с `mut` — то же.
   - Для `is_external == true` декларации **не** генерировать Nova body — только dispatch на C-функцию по имени.
   - Auto-derive `@try_read_*` ↔ `@read_*` через wrapper-обёртки на одну C-функцию.

5. **Overload resolution** для `@append(s|c)` и `Buffer.from(s|c)`:
   - Special-case dispatch по static-type аргумента.
   - Параллель с уже работающим `Buffer.from(s)`/`Buffer.from(b)` для существующего Buffer.

6. **`str.from(char)` special-case** — codegen эмитит вызов `Nova_str_static_from_char(cp)`.

7. **`fn ReadBuffer @into()` Nova-implementation** — обычная функция с throw'ом, не external. Парсится как обычный fn-decl. Цель — блокировать D73 auto-derive.

### Этап 3 — runtime (~3-4 часа)

В `compiler-codegen/nova_rt/`:

1. **Удалить** старый `nova_rt/buffer.h` (после migration).

2. **Создать `nova_rt/string_builder.h`**:
   - Struct `Nova_StringBuilder` (тот же layout что текущий `Nova_Buffer`).
   - Methods из текущего `buffer.h` text-side (rename `add_str` → `append_str`, etc.).
   - `@into() -> nova_str` — infallible (просто transfer ownership).
   - Удалить `try_into`/`into_str_unchecked` — больше не нужны (UTF-8 invariant поддерживается типом).

3. **Создать `nova_rt/write_buffer.h`**:
   - Struct `Nova_WriteBuffer`.
   - Bytes methods (rename `add_byte` → `write_byte`, `add_bytes` → `write_bytes`).
   - **18 endianness write functions** — `write_u8`, `write_i8`, `write_u16_le/be`, `write_u32_le/be`, `write_u64_le/be`, `write_i16_le/be`, `write_i32_le/be`, `write_i64_le/be`, `write_f32_le/be`, `write_f64_le/be`. Каждая ~5 строк (memcpy + bit-shift с учётом endianness).
   - `@into() -> []byte` consume.

4. **Создать `nova_rt/read_buffer.h`** (новый файл):
   - Struct `Nova_ReadBuffer { data, len, pos }` — view над bytes.
   - `Nova_ReadBuffer_static_from_bytes(arr)` — view-конструктор.
   - Cursor metadata: `position`, `remaining`, `has_remaining`, `remaining_bytes`.
   - **18 read functions** для each (u8/i8/u16/i16/u32/i32/u64/i64/f32/f64 × LE/BE) — auto-derive обёртки в codegen, в C **одна функция** на каждое (number_type, endianness).
   - C-функция возвращает Result-like структуру:
     ```c
     typedef struct ReadResult_u32 {
         nova_bool ok;       // 1 = success, 0 = UnexpectedEnd
         uint32_t  value;
         int64_t   wanted;   // для error: wanted bytes
         int64_t   available; // для error: available bytes
     } ReadResult_u32;
     ```
   - Codegen emit обёртки:
     - `Nova_ReadBuffer_method_read_u32_be(rb)` → проверяет `ok`, throw'ит через `Nova_Fail_fail(...ReadBufferError.UnexpectedEnd...)`.
     - `Nova_ReadBuffer_method_try_read_u32_be(rb)` → упаковывает в `Result[u32, ReadBufferError]`.

5. **Расширить `nova_rt/str.h`** (или соответствующий):
   - `Nova_str_static_from_char(cp)` — UTF-8 encode 1-4 bytes из codepoint в новый `nova_str`.

### Этап 4 — тесты (~1-2 часа)

В `nova_tests/runtime/`:

1. **Удалить** `buffer.nv` (16 текущих тестов).

2. **Создать `string_builder.nv`** — text-тесты:
   - `new()`, `with_capacity`, `from(s str)`, `from(c char)`.
   - `@append(s str)`, `@append(c char)` — UTF-8 1-4 byte cases.
   - `@into() -> str` infallible.
   - `@clone()`, `@len()`, `@capacity()`.

3. **Создать `write_buffer.nv`** — binary write тесты:
   - `new()`, `with_capacity`, `from(b)`.
   - `@write_byte`, `@write_bytes`.
   - **Endianness round-trip**: `@write_u32_be(0xDEADBEEF)` → `@into() -> []byte` → проверить байты `[0xDE, 0xAD, 0xBE, 0xEF]`.
   - Все 18 числовых × LE/BE round-trip.
   - `@into() -> []byte`.

4. **Создать `read_buffer.nv`** — binary read тесты:
   - `from(b)`, `position()`, `remaining()`, `has_remaining(n)`.
   - Throwing form: `@read_u32_be()` на достаточных bytes.
   - Try form: `@try_read_u32_be()` на достаточных bytes (Ok), на недостаточных (Err with UnexpectedEnd).
   - Round-trip с WriteBuffer: write → into → from → read same value.
   - `@remaining_bytes()` после частичного чтения.
   - `@into()` — должен throw'ить `Error.new("...not supported...")`.

### Этап 5 — финализация (~30 минут)

1. Прогон всех `nova_tests/`.
2. Fix регрессий (если есть).
3. Sanity-check spec: cross-references работают.
4. Commit.

### Этап 6 — полное удаление `Buffer` из языка (~1-2 часа)

**Цель:** убрать `Buffer` из языка полностью. Это **неудачное
решение** (попытка унифицировать text+binary в одном типе) которое
правильно заменено split'ом на StringBuilder/WriteBuffer/ReadBuffer.

Никакой backward compatibility — Nova не в production, революционный
язык важнее обратной совместимости. Buffer удаляется без deprecation-
периода: одним коммитом убирается из codegen, runtime и всех ссылок
в std.

**Подзадачи:**

1. **WriteBuffer extension API.** Добавить `@write_char(c char)` и
   `@write_str(s str)` — UTF-8 кодирование char/str в byte buffer.
   Реализация в `nova_rt/write_buffer.h`:
   ```c
   void Nova_WriteBuffer_method_write_char(Nova_WriteBuffer*, nova_int codepoint);
   void Nova_WriteBuffer_method_write_str(Nova_WriteBuffer*, nova_str s);
   ```
   `write_char` UTF-8 encode'ит codepoint в 1-4 байта. `write_str`
   копирует UTF-8 bytes напрямую (str уже UTF-8).

2. **Sweep std/.** Полная миграция оставшихся файлов:
   - **Text-only** (12 файлов уже мигрированы 2026-05-08): bcrypt,
     base64, csv, hex, ini, toml, ulid, uuid, path, diff, regex,
     duration, markdown_minimal — `Buffer` → `StringBuilder`.
   - **Mixed text+binary** (url.nv `decode_query`): `Buffer` →
     `WriteBuffer` + finalize через `str.try_from(bytes)?`. Также
     `url.nv encode_query` — text-only, мигрировано.
   - **ASCII-only single byte** (testing/property.nv): `add_byte(c
     as byte)` → `append(c as int as char)`. Мигрировано.

3. **Удаление Buffer из codegen.** В `compiler-codegen/src/codegen/
   emit_c.rs`:
   - Убрать `record_schemas.insert("Buffer", ...)`.
   - Убрать method dispatch для `Nova_Buffer*` (`add_str`, `add_byte`,
     `into_str_unchecked`, etc.).
   - Убрать `Nova_Buffer_static_*` paths.

4. **Удаление runtime.** `nova_rt/buffer.h` — удалить.

5. **nova_tests/runtime/buffer.nv** — удалить или мигрировать на
   `string_builder.nv` / `write_buffer.nv` (которые уже есть).

6. **Q-buffer закрыть финально** в open-questions.md как
   ✅ REMOVED — `Buffer` удалён из языка, неудачное решение.

**Риск:** случаи где `Buffer` использовался для **смешанных** text+
binary без финализации в str — должны мигрировать на WriteBuffer
+ str.try_from. Случаи где `add_byte` для пишет non-ASCII byte
(часть UTF-8 sequence) могут потерять корректность если
конвертировать наивно.

**Cross-check:** все sweep-коммиты обязаны прогонять `run_tests.ps1
-IncludeStdlib` для catch'а регрессий. После Этапа 6 все ссылки на
Buffer в std должны исчезнуть; в codegen — только если и его сноска
ушла.

## Открытые вопросы (на момент планирования)

Все resolved для MVP, но фиксирую для будущих ревизий:

1. **`extern("C")` для FFI к чужим библиотекам** — отдельный keyword для будущего, не пересекается с `external` (Nova-runtime). Q-ffi.

2. **`external type X` для opaque types вне prelude** — отложено. Если когда-то понадобится opaque user-defined type (Channel-OS-thread, mmap'ed Region), вернуться. Сейчас — built-in только.

3. **Compile-time annotation для блокирования D73 auto-derive** (`@no_into` или подобное) — отложено. Сейчас runtime-throw достаточно. Q-no-derive.

4. **ReadBuffer view (zero-copy) `@remaining_view()`** — отложено. Сейчас copy. Связано с Q-readonly-types.

5. **ReadBufferError расширения** (`InvalidFormat`, `InvalidUtf8`) — добавлять по мере появления read-методов которые могут fail с этими причинами (например, `read_str` с UTF-8 валидацией). Сейчас одна вариант `UnexpectedEnd`.

6. **Q-overloading закрытие** — после него `@append(str|char)` overload работает чисто, special-case dispatch в codegen (Этап 2 п.5) можно убрать. До тех пор — special-case.

7. **WriteBuffer.from(s str)** — не вводим в MVP, программист использует `s.bytes()` + `WriteBuffer.from(b)`. Q-write-buffer фиксирует.

## Размер работы

**Общее время:** ~8-10 часов сосредоточенной работы.

| Этап | Время |
|---|---|
| 1. Спека | ~1 час |
| 2. Codegen | ~3-4 часа |
| 3. Runtime | ~3-4 часа |
| 4. Тесты | ~1-2 часа |
| 5. Финализация | ~30 минут |

**Большой кусок, делать рекомендуется отдельной сессией.** Step-by-step с проверкой после каждого этапа минимизирует риск регрессий.

## Связь с другими документами

- **Q-buffer** (`spec/open-questions.md`) — ✅ closed 2026-05-07, будет REPLACED.
- **D26** (`spec/decisions/08-runtime.md`) — prelude, расширяется.
- **D30** (`spec/decisions/03-syntax.md`) — naming, расширяется (полные слова).
- **D52** (`spec/decisions/02-types.md`) — kind-tokens, **не** расширяется (`external type` не вводим).
- **D5/D47** (`spec/decisions/07-modules.md`) — visibility, упоминание `export external`.
- **D73** (`spec/decisions/08-runtime.md`) — From/Into, используется для char↔str и для блокировки `@into()` через явное declaration.
- **D77** (`spec/decisions/08-runtime.md`) — TryFrom/TryInto, параллель для read/try_read auto-derive.
- **`std/runtime/builtins.nv`** — новый файл, doc-stub.
- **`compiler-codegen/nova_rt/buffer.h`** — будет удалён, split на три файла.
- **`nova_tests/runtime/buffer.nv`** — будет удалён, split на три файла.

## Эволюция обсуждения

Обсуждение прошло несколько итераций:

1. **Старт:** добавить endianness-методы в существующий Buffer.
2. **Поворот 1:** `add_*` → `append_*` для текста, `write_*` для двоичного (split по domain'у в одном Buffer).
3. **Поворот 2:** унификация на `write_*` для всего (Go-style).
4. **Поворот 3 (Rust анализ):** `bytes::BytesMut` использует `put_*` — стало кандидатом.
5. **Поворот 4:** `put_*` не нравится → откат на `write_*` + добавление `read_*` для read.
6. **Финал по naming:** `WriteBuffer.@write_*`, `ReadBuffer.@read_*`/`@try_read_*`, `StringBuilder.@append`.
7. **Поворот 5 (split типов):** идея разделить Buffer на три типа со специализированной семантикой — главный win, `@into() -> str` становится infallible.
8. **Поворот 6 (`external` keyword):** новая фича дизайна для документирования stdlib runtime-функций.
9. **Поворот 7 (без `external type`):** built-in opaque типы как примитивы, `external` только для функций.
10. **Финал:** план зафиксирован.

Эта эволюция отражает живое обсуждение — конечная архитектура **не очевидна с первого захода**, но каждый поворот имел обоснование. Фиксирую финальное состояние, не промежуточные.
