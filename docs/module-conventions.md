<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Module conventions — как проектировать модули и интегрировать C-библиотеки

> **Нормативно.** Изменения/отклонения — только по согласованию с владельцем (см.
> [conventions-governance.md](conventions-governance.md)). Это **дизайн-конвенция** (как устроен модуль и его граница с C),
> дополняет: [nv-coding-style.md](nv-coding-style.md) (стиль `.nv`-кода), [ffi-cookbook.md](ffi-cookbook.md) (механика FFI:
> CStr/указатели/`unsafe`/примеры libsqlite3 и т.п.), [compiler-conventions.md](compiler-conventions.md) (§3 «не хардкодить
> stdlib», §5 spec-first). **Канонический пример** — `std/net/` (TcpNet-семейство); **референс-планы** —
> [179](plans/179-time-system-rework.md)/[179.1](plans/179.1-civil-time.md)/[180](plans/180-io-fs-os.md).

## Применимость (scope)

Про **любой** Nova-модуль, не только stdlib: app-код, third-party-библиотеки, **биндинги C-библиотек** (ровно как
`my_app/sqlite3.nv` в [ffi-cookbook](ffi-cookbook.md)).

- **Универсально (любой модуль):** §0–§1 эффект-семейство (мокабельный плумбинг + фасад), §2 value/must-consume-типы +
  byte-first, §3 структурный `Result`-домен ошибок, §4.2–§4.5 `extern "C"`-маппинг (`CStr`/`(*u8,len)`/value-records/errno),
  `#cfg`-platform-split, §5 нейминг, §6 тесты/docs, §7 чек-лист.
- **Только std / собственный рантайм Nova** (помечено по тексту): `extern "nova" fn` (§4.1) — функции рантайма Nova; **user/
  third-party код в рантайм не добавляет → использует только `extern "C"`**. Async park/wake (`nova_sched_park`, §4.6) —
  runtime-внутреннее; **user-FFI обычно синхронный или через `blocking { }`** ([Plan 83.3](plans/83.3-blocking-effect-threadpool.md)/D50),
  а не своя park/wake. `#stable(since)` (§6) — маркер стабильности публичного std-API.

---

## 0. Главный принцип: эффект — это плумбинг, юзер ходит через типы

I/O-, OS- и ресурсные подсистемы строятся как **семейство эффектов**:

- **Эффект — внутренний dispatch-точка**, юзер его **не вызывает напрямую** (как `TcpNet`/`AddrNet`,
  [net/effect.nv §21-40](../std/net/effect.nv#L21)). Это даёт **мокабельность**: тест подменяет реальную подсистему
  handler'ом (`with Fs = mem_fs() { … }`) → детерминизм без диска/сети/часов и **без DI-плумбинга**. Это сильнее Go (нужен
  `afero`/интерфейс вручную), Rust (trait-abstraction), Java/Node (global monkey-patch).
- **User-facing API — методы на типах + free-fns** (фасад). `Timestamp.now()` => `Time.timestamp()`; `File.open(path)` =>
  `Fs.open(…)`. Эффект виден в effect-row сигнатуры (`fn … Fs -> …`), но в теле — обычный последовательный код.
- **Когда эффект нужен:** операция импурна и её разумно подменять в тестах (часы, fs, env, сеть, рандом). **Когда НЕ нужен:**
  чистая алгоритмика (календарная арифметика, парсинг, кодировки) — это обычные `.nv`-функции без эффекта.

## 1. Анатомия модуля (по net-прецеденту)

```nova
// 1) ЭФФЕКТ — плумбинг (юзер не зовёт). Опы названы по возвращаемому типу / descriptive (НЕ user-verb).
type Fs effect {
    open(path Path, opts OpenOptions) -> Result[File, IoError]
    stat(path Path, follow bool) -> Result[Metadata, IoError]
    // …
}

// 2) ДЕФОЛТНЫЙ handler — тонкая typed-обёртка над extern-примитивами (см. §4).
export fn real_fs() -> Effect[Fs] {
    effect Fs {
        open(path, opts) => { … fs_open(path.as_cstr(), opts.flags(), opts.mode()) … }
        // …
    }
}

// 3) MOCK-handler — для детерминированных тестов.
export fn mem_fs() -> Effect[Fs] { … in-memory … }

// 4) USER-FACING сахар на типах + free-fns.
export fn File.open(path Path) Fs -> Result[File, IoError] => Fs.open(path, OpenOptions.read())
export fn read_to_string(path Path) Fs -> Result[str, IoError] => …
```

**Именование эффект-опов** (по `AddrNet.loopback`/`v4`, `Time.timestamp`/`monotonic`): по тому, **что оп возвращает/делает**,
а не привилегированным user-глаголом. `.now()`/`.open()` — это ergonomic-сахар **на типе**, не имя эффект-опа. Эффект-оп с
явным receiver-аргументом (`read(f File, …)`) — нормально, юзер его не видит.

## 2. Типы модуля

- **Мелкие неизменяемые значения** (`Duration`/`Timestamp`/`Offset`/`IoError`/`Metadata`) — **value-record** `type X value { ro f T }`
  (D215: stack, zero-GC, copy, структурное `==`). См. [nv-coding-style §15](nv-coding-style.md). Не делать heap-record для
  single-/few-field-скаляров.
- **Ресурсы** (`File`, dir-handle, lock, subprocess `Child`) — **must-consume линейные** ([Plan 80](plans/80-must-consume-linear.md),
  D133): `@close(self) -> Result[…]` — единственный способ разрядить обязательство; незакрытый = **compile-error**; double-close/
  use-after невозможны. `@close()`/`@wait()` **возвращает `Result`** (ошибка close/flush — например `ENOSPC` — видна, не глотается).
  Это строго лучше Go (`defer Close()` глотает), Rust (`Drop` глотает), Java/Kotlin (suppressed). Если Plan 80 ещё не готов —
  affine `consume` + runtime-check как явный fallback (зафиксировать в плане).
- **Byte-first I/O.** `str` = **UTF-8-validated immutable** → НЕ байтовый буфер. Сырой I/O — **`[]u8`**; `str` появляется
  **только через fallible декод** (`str.from_utf8(bytes) -> Result[str, Utf8Error]`; невалид → ошибка, не паника/не lossy-по-дефолту).
  Никогда не «читать в `str`».
- **Числа времени/размеров** — типизированные обёртки, не сырые int (mtime → `Timestamp`, не epoch-int).

## 3. Ошибки

- **`Result[T, XError]`** на всех fallible-операциях. **Один структурный `XError` на домен** (Rust `io::Error`-урок), а не
  flat sum без контекста: `type IoError value { ro kind ErrorKind, ro raw_os int, ro op str, ro path Option[Path], ro source Option[*IoError] }`.
- **`ErrorKind` — OPEN sum-type** (последний вариант `Other(int)` → wildcard-arm обязателен) с доменными вариантами
  (`NotFound`/`PermissionDenied`/`AlreadyExists`/…). `@to_str()` для сообщений (platform-stable строки на C-стороне,
  [net.c:45-55](../compiler-codegen/nova_rt/net.c#L45)).
- Не плодить дублирующие error-типы для родственных доменов — переиспользовать/проецировать (напр. net `NetError` → проекция на `IoError`-kinds).
- Конструкторы/parse **fallible**, без default-panicking (`try_`-конвенция, §5).

## 4. Интеграция с C-библиотеками (граница `.nv` ↔ C)

### 4.1. Две формы extern (D282) — что когда

- **`extern "nova" fn`** — функция **собственного рантайма Nova** (Nova-ABI; codegen зовёт `nova_fn_<name>`; регистрируется в
  `ExternalRegistry`). Для хуков, что **берут/возвращают Nova-типы** (receiver, `Duration`-value-record, `str`) или должны быть
  известны компилятору (как `runtime.*`/`sync.*`). Пример: `extern "nova" fn Mutex mut @try_lock_for(timeout Duration) -> bool`.
- **`extern "C" fn`** — C-символ по **литеральному имени** (НЕ регистрируется как `nova_fn_`; типы **обязаны** быть C-нативными).
  Для тонкого FFI-слоя к C-библиотеке/рантайм-C. Пример: `extern "C" fn fs_open(path CStr, flags int, mode int) -> int`.
- **Важно:** FFI-keyword выбирает только **имя символа + проверку C-нативности типов** — **никакой** suspend/GC/effect-семантики
  он не несёт (suspension — поведение самой C-функции). Скаляр-хук (`() -> int`) → `extern "C"` (проще, литеральное имя).

### 4.2. Именование extern и расположение

- `extern "C"`-функции — **module-private** (без `export`), в выделенном `ffi.nv`-слое (как [std/net/ffi.nv](../std/net/ffi.nv)).
- Имя: **`<resource>_<action>`** snake_case, **без Nova-префикса** (как `tcp_listener_bind`/`socket_addr_loopback`; D282/[02-types.md §FFI](../spec/decisions/02-types.md)).
  Лидирующий `_` у глобального C-символа **нельзя** (зарезервирован C-стандартом).

### 4.3. Маппинг типов на границе (самое важное)

| Что | Как передавать | Почему |
|---|---|---|
| **Путь / C-строка / env-ключ** | **`CStr`** (NUL-terminated; [std/ffi/cstr.nv](../std/ffi/cstr.nv)). Строить из байт через `CStr.from_bytes(...) -> Result` с **reject interior-NUL → ошибка** | OS-API (`open`/`getenv`/`uv_fs_open`) берут `const char*` **без длины**; `str` НЕ годится (UTF-8-only). Ровно `CString::new` (Rust)/`BytePtrFromString` (Go) — NUL-терминация и проверка **в языке**, не в C |
| **Байт-данные (read/write payload)** | **`(*u8, int len)`** | Совпадает с syscall `read(fd, buf, count)`/`uv_buf_t{base,len}` (НЕ NUL-terminated, длина явная, NUL внутри допустим) |
| **`str`** | **НЕ передавать** через границу (кроме genuinely-text, что на syscall-границе ~не бывает) | `str` UTF-8-only; пути/данные — произвольные байты |
| **Результат-агрегат** (stat, exit-status) | **C-ABI value-record** by-value ([Plan 178](plans/178-ffi-abi-types.md)): `fs_stat(path CStr, follow bool) -> CStatBuf` | value-records/туплы C-ABI-совместимы |
| **errno** | возврат **`< 0` == `-errno`** | конвенция libuv/syscall |

### 4.4. Максимизировать nv-sourcing — логика в `.nv`, не в C

Правило (см. memory `feedback-maximize-nv-sourcing`, [compiler-conventions §3](compiler-conventions.md)): **в C — только
непортируемые примитивы** (syscall-обёртки, libuv-park). Вся типизация, парсинг, **кодировки/конверсии — в Nova**:

- `read_to_string` = `read([]u8)` (C) + `str.from_utf8` (Nova), не C-декод.
- **WTF-8 ↔ UTF-16** (Windows-пути) — в Nova через [std/encoding/utf16.nv](../std/encoding/utf16.nv) (Plan 152.6: `is_high/low_surrogate`/
  `decode_surrogate_pair`/`@encode_utf16`), **не** в C-шиме; C получает уже `[]u16`/wide-`CStr`.
- Календарь, форматирование, нормализация — `.nv`.

### 4.5. Платформенные различия — через `#cfg`/суффикс, не `#ifdef` в C

Разные ОС → разные extern/реализации через **conditional compilation** ([Plan 42.12](plans/42.12-cfg-conditional-compilation.md)/D99):
**filename-suffix** `_posix.nv`/`_unix.nv`/`_windows.nv`/`_linux.nv`/`_macos.nv` **или** `#cfg(target_os = "…")`. Пример: POSIX
`fs_open(path CStr, …)` (байты verbatim — `pathname` на POSIX это **байты без кодировки**) живёт в `*_posix.nv`; Windows
`fs_open_w(path *u16, …)` — в `*_windows.nv`. Платформенный `#ifdef` оставлять в C **только** для самих syscall-обёрток.

### 4.6. Async + cancel (по net-паттерну)

Блокирующие/I/O-хуки **паркуют фибру** через libuv (как [net.c:1-24](../compiler-codegen/nova_rt/net.c#L1): GC-heap state →
`stop_cb` register → `nova_sched_park()` → libuv-callback + `nova_sched_wake()` → resume). User-API выглядит блокирующим
(`file.read(buf) -> Result`), но не блокирует OS-поток. **Cancel — честно best-effort:** queued-операция отменяется чисто
(`uv_cancel`); уже-летящий syscall на threadpool **не прерывается** (как Go/tokio/Java) — abandon-result + well-defined состояние
ресурса. **Не врать** про mid-syscall-cancel. `Blocking`-эффект ([Plan 83.3](plans/83.3-blocking-effect-threadpool.md)/D50) —
только для CPU-bound-обёрток, не дефолтный I/O-путь. **Лайфтайм:** буферы/`CStr`, переданные в async libuv-вызов, должны
пережить park — GC-root на стеке припаркованной фибры это обеспечивает.

## 5. Нейминг (выжимка; полное — [nv-coding-style](nv-coding-style.md))

- Конструктор — **`X.new(...)`** (как `Vec.new`/`Barrier.new`; `.of` зарезервирован за variadic у `Vec`). Валидирующий конструктор → `Result`.
- Fallible-вариант — **`try_`-префикс**, возвращает `Result` (`try_from`/`try_parse`/`try_plus`); ошибки парсинга — **`Parse<X>Error`** (как `ParseIntError`).
- Конверсии — `from`/`try_from`/`Into`/`TryInto` (D77). Чтения-в-новое на immutable-value — **bare-verb** (`@trim_ascii`/`@normalize`), не `-ed` (sorted/normalized — это для mut/non-mut-пар, которых в Nova нет; различие — через `mut`+тип-возврата).
- Метод предпочтительнее свободной функции (фасад, [nv-coding-style §3](nv-coding-style.md)); эффект — явно у `export`, выводимо у private ([§11](nv-coding-style.md)).
- **Числовые типы.** Индексы / длины / offset / счётчики — **`int`** (не `u64`/`usize`); `u8`/`u16` — для байт/UTF-16. Полное правило + анти-паттерны — [nv-coding-style §24](nv-coding-style.md).

## 6. Spec / D-block / тесты / docs

- **Spec-first** ([compiler-conventions §5](compiler-conventions.md)): D-block **до** кода. Новые публичные семантики — свой
  D-block; коды ошибок — в error-index (§6 compiler-conventions). Не править hot-spec-файлы в одиночку, если они в зоне другого плана.
- **Тесты** ([test-conventions](test-conventions.md)): pos **и** neg в `nova_tests/<module>/{,neg/}`, классификация по `EXPECT_*`-маркеру
  (не по папке). Для эффект-модулей — **mock-handler-тест** (детерминизм без реального ресурса) обязателен. `nova test` (byte-baseline)
  — **не** гейт корректности (memory `feedback-nova-tests-not-correctness-gate`): гейт = targeted pos+neg + аргумент звучности.
- **`#stable(since = "X")`** на публичном API; внутренние extern-примитивы — без `export`/`#stable`.
- **docs/**: модуль с нетривиальной моделью — свой guide (как `strings-internals.md`), с таблицей «Nova ↔ Go/Rust/…» и differentiators.

## 7. Чек-лист нового I/O/OS/ресурс-модуля

1. Эффект-семейство: `type X effect { … }` (опы по возвращаемому типу) + `real_x()` handler + `mock_x()` для тестов.
2. User-API — методы на типах + free-fns (фасад); эффект в effect-row, не в имени.
3. Мелкие значения — `value`-record; ресурсы — must-consume (`@close() -> Result`).
4. Ошибки — один структурный `XError {kind, raw_os, op, …}` + OPEN `ErrorKind` + `@to_str()`.
5. byte-first: raw = `[]u8`; `str` только через `from_utf8 -> Result`.
6. C-граница: `ffi.nv` (module-private `extern "C"`, `<resource>_<action>`); путь → `CStr` (NUL-term + reject-interior-NUL), данные → `(*u8,len)`, агрегаты → C-ABI value-record, errno `<0`.
7. Конверсии/кодировки — в Nova (utf16/unicode), не в C; платформенное — через `#cfg`/суффикс.
8. Async — park/wake (net.c-паттерн); cancel честно best-effort; лайфтайм буферов через GC-root.
9. Spec-first D-block; pos+neg + mock-тесты; `#stable`; docs-guide.

## 8. Резюме одной фразой

**Эффект-плумбинг (мокабельный) + type-методы-фасад; value/must-consume-типы; один структурный `Result`-ошибочный домен;
byte-first; тонкий `extern "C"`-`ffi.nv`-слой (путь→`CStr`, данные→`(*u8,len)`, агрегаты→value-record, errno`<0`); вся логика
и кодировки — в `.nv`, платформенное — через `#cfg`; async — park/wake.**
