---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

[English](ffi-cookbook.md) | **Русский**

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Nova FFI Cookbook

> **Scope.** Механика границы `.nv` ↔ native: `extern "C"`, opaque/typed
> указатели, `CStr`, tuple-by-value, C-ABI-проверка, и **как подключить
> native-артефакты к сборке** (`[ffi]` / `[ffi.staticlib]`). Foundational FFI —
> Plan 115 D214; typed-pointer family (`*T`, `*mut T`, `Option[*T]`-NPO) —
> **влит** (Plan 118/138.5/174.x; секции ниже — уже не «preview»).
>
> **Как сделать МОДУЛЬ** (layout пакета, `nova.toml`, стабильность, тесты) —
> общий гайд [authoring-a-module](authoring-a-module.md) (native-backed —
> его §7). Дизайн-конвенции модуля (эффект-плумбинг, типы, ошибки) —
> [module-conventions](../dev/module-conventions.md). Именование внешних пакетов
> (`nova-<пакет>`) — [D78-амендмент Plan 195](../../spec/decisions/07-modules.md#именование-внешних-пакетов-репозиториев-амендмент-plan-192-2026-07-10).
>
> ⚠️ **Plan 134 (2026-06-09): встроенный тип `ptr` удалён.** Используйте `*()`
> (указатель на unit-тип = `void*` в C) везде, где раньше был `ptr`. Компилятор
> эмитит `E_TYPE_REMOVED_PTR_USE_UNIT_PTR` для `ptr` в позиции типа.

Этот cookbook показывает, как привязать Nova-код к сторонним C-библиотекам —
sqlite3, libpng, libcurl — с помощью фундаментальных FFI-примитивов,
введённых в Плане 115.

## Краткий справочник

| Потребность | Инструмент | Spec |
|---|---|---|
| Opaque-указатель | `*()` (указатель на unit = `void*`) | [D214](../../spec/decisions/02-types.md#d214) / [Plan 134](../plans/134-remove-ptr-type.md) |
| Литерал NULL | `0 as *()` | D214 амендмент Plan 134 |
| Типизированный хендл | запись `type X { ro value *() }` | D214 §3 |
| Многозначный возврат | `(T1, T2)` tuple-by-value | D214 §2 |
| Объявление external fn | `external fn name(args) -> ret` | [D82](../../spec/decisions/03-syntax.md#d82) |
| Очистка ресурса | метод `consume close()` + `defer` | [D90 / D131](../../spec/decisions/03-syntax.md#d90) |

## Правила модификаторов указателей (FINAL — Plan 138.5)

При записи FFI-сигнатур с pointer/typed wrappers модификатор pointee пишется **постфиксом**, сразу после `*` (`*mut T` / `*ro T` / `*unsafe T`). **Prefix перед `*` запрещён** (`mut * T` / `ro * T` / `unsafe * T` → `E_POINTER_PREFIX_MODIFIER`). Перепривязываемость указателя — это **binding** (`let` / `mut`), не тип.

Краткая шпаргалка:

- `*T` ≡ `*ro T` — pointer к read-only T (default)
- `*mut T` — pointer к writable T (caller может изменить pointee)
- `*unsafe T` — pointer к possibly-uninit T (MaybeUninit analog); сам указатель non-null
- `Option[*T]` — **nullable** pointer (NPO, 8 байт); это замена старому `unsafe * T`
- `Option[*unsafe T]` — FFI nullable-uninit pointer (None = null, Some = non-null ptr к uninit)
- `*mut *ro Acc` — postfix chain (writable-target ptr к read-only-target ptr к Acc)
- `mut p *mut T` — binding mut (p re-pointable) + pointee mut; `let q *ro T` — fixed binding + ro pointee

Полные правила (arrow→box model, value-T composition §V3.1/§V3.2) — см. [`docs/guide/typed-pointers.md`](typed-pointers.md). Spec — [D216 §1 FINAL](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) + [Plan 138.5](../plans/138.5-d216-v2-v3-simplification.md).

## Послойный FFI-паттерн

```
LAYER 1  Nova public API           Database.open(path)
   ↓
LAYER 2  Nova wrapper              construct typed handle from raw return
   ↓
LAYER 3  external fn declaration   typed handle + tuple return
            external fn nova_fn_sqlite3_open(path str) -> (*(), int)
   ↓
LAYER 4  C shim                    ~5-10 lines, adapts out-param → struct
            _NovaTuple_2_8_nova_ptr_8_nova_int
            nova_fn_sqlite3_open(nova_str path) { ... }
   ↓
LAYER 5  Actual C library          sqlite3_open(path, &db_out)
```


## Типизированные хендлы (канон 2026-07-09)

Объявляйте каждый C-хендл как newtype с префиксом `C` **в** extern-сигнатурах —
никогда не пропускайте голый `int`/`*()`-хендл через Nova-код:

```nova
type CBrotliHandle(int)
extern "C" fn brotli_dec_new() -> CBrotliHandle
extern "C" fn brotli_dec_feed(h CBrotliHandle, p *u8, len int) -> int
```

Lowering: newtype-over-int → `typedef nova_int Nova_CBrotliHandle;` — C-шим
ABI не трогается; Nova-сторона бесплатно получает номинальную типизацию.
Проверка null через `(h as int) == 0`. Нормативное правило: module-conventions
§4а. Известный пробел: методы на newtype-приёмнике ошибочно диспатчатся по
имени ([M-newtype-receiver-method-dispatch]) — пока не исправлено, вызывайте
типизированные externs напрямую.

## Plan 115 V1 setup

В Nova V1 есть эти фундаментальные куски (коммит `<plan-115-merge>`):
- `*()` (указатель на unit) эмитится как `void*` в C-выводе (Plan 134; раньше
  `ptr` с `typedef void* nova_ptr`).
- Tuple-by-value возвраты из external fn — используют монотипизированные
  typedef'ы `_NovaTuple_<arity>_<L_i>_<T_i>...` (механизм Plan 59).
- D82 амендирован (Plan 115): пользовательский `external fn` разрешён в любом
  модуле — больше не ограничен `std.runtime.*`.

Доставлено начиная с V1 (не «future» — уже влито):
- **Конструктор tuple-newtype `type X(*())`** ✅ (`[M-115-newtype-constructor]`)
  — каноническая форма; single-field-record больше не нужен.
- **Пайплайн сборки пользовательских шимов** ✅ — задаётся не CLI-флагом, а
  декларативно в `nova.toml`: `[ffi]` (готовые `.c`-шимы + системные `libs`) и
  `[ffi.staticlib]` (собираемый staticlib, Plan 195). При `import` модуля
  артефакты компилируются/линкуются автоматически — перекомпилировать
  Nova-компилятор не надо. См. [«Build pipeline»](#build-pipeline--ffi-и-ffistaticlib-манифест) ниже.
- **Typed-pointer family** ✅ (`*T`/`*mut T`/`Option[*T]`-NPO/`CStr`) — Plan
  118/138.5; см. секции ниже.

Всё ещё followup:
- Автогенерация биндингов из C-заголовков — `[M-115-bindgen-tool]`
  (`nova bindgen header.h`, отдельный tooling-план).

## Пример 1 — привязка libsqlite3

Полный пример, покрывающий open, exec, prepare, step, finalize, close.

### C-шим (`compiler-codegen/nova_rt/sqlite3_ffi.h`)

```c
/* sqlite3_ffi.h — Nova binding shim for libsqlite3.
 *
 * Compile + link target: libsqlite3 must be available.
 * Plan 115 V1 ships header-only inline wrappers — link with -lsqlite3.
 */

#ifndef NOVA_SQLITE3_FFI_H
#define NOVA_SQLITE3_FFI_H

#include <sqlite3.h>

/* Forward-declare Nova mono'd tuple types matching what Nova codegen emits. */
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_ptr_8_nova_int
typedef struct _NovaTuple_2_8_nova_ptr_8_nova_int {
    nova_ptr f0;
    nova_int f1;
} _NovaTuple_2_8_nova_ptr_8_nova_int;
#endif

/* Open a database. Returns (db_handle, return_code).
 * rc == 0 (SQLITE_OK) on success. */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_sqlite3_open(nova_str path) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    sqlite3* db = NULL;
    /* sqlite3_open expects C-string; Nova str.ptr may not be NUL-terminated. */
    char path_buf[1024];
    if (path.len >= sizeof(path_buf)) { r.f0 = NULL; r.f1 = SQLITE_TOOBIG; return r; }
    memcpy(path_buf, path.ptr, path.len);
    path_buf[path.len] = '\0';
    int rc = sqlite3_open(path_buf, &db);
    r.f0 = (nova_ptr)db;
    r.f1 = (nova_int)rc;
    return r;
}

/* Close a database. Returns sqlite3 rc. */
static inline nova_int nova_fn_sqlite3_close(nova_ptr db) {
    return (nova_int)sqlite3_close((sqlite3*)db);
}

/* Execute SQL (no result set). Returns rc. */
static inline nova_int nova_fn_sqlite3_exec(nova_ptr db, nova_str sql) {
    char buf[4096];
    if (sql.len >= sizeof(buf)) return SQLITE_TOOBIG;
    memcpy(buf, sql.ptr, sql.len);
    buf[sql.len] = '\0';
    char* errmsg = NULL;
    int rc = sqlite3_exec((sqlite3*)db, buf, NULL, NULL, &errmsg);
    sqlite3_free(errmsg);
    return (nova_int)rc;
}

/* Prepare statement. Returns (stmt_handle, rc). */
static inline _NovaTuple_2_8_nova_ptr_8_nova_int
nova_fn_sqlite3_prepare(nova_ptr db, nova_str sql) {
    _NovaTuple_2_8_nova_ptr_8_nova_int r;
    sqlite3_stmt* stmt = NULL;
    int rc = sqlite3_prepare_v2((sqlite3*)db, sql.ptr, (int)sql.len, &stmt, NULL);
    r.f0 = (nova_ptr)stmt;
    r.f1 = (nova_int)rc;
    return r;
}

/* Step. Returns rc (SQLITE_ROW = 100, SQLITE_DONE = 101). */
static inline nova_int nova_fn_sqlite3_step(nova_ptr stmt) {
    return (nova_int)sqlite3_step((sqlite3_stmt*)stmt);
}

/* Column int value. */
static inline nova_int nova_fn_sqlite3_column_int(nova_ptr stmt, nova_int col) {
    return (nova_int)sqlite3_column_int((sqlite3_stmt*)stmt, (int)col);
}

/* Finalize statement. */
static inline nova_int nova_fn_sqlite3_finalize(nova_ptr stmt) {
    return (nova_int)sqlite3_finalize((sqlite3_stmt*)stmt);
}

#endif /* NOVA_SQLITE3_FFI_H */
```

### Nova-привязка (`my_app/sqlite3.nv`)

```nova
module my_app.sqlite3

// Typed handles — V1 record form (tuple newtype `type X(*())`).
type Db { ro value *() }
type Stmt { ro value *() }

// External declarations matching the C shim.
external fn nova_fn_sqlite3_open(path str) -> (*(), int)
external fn nova_fn_sqlite3_close(db *()) -> int
external fn nova_fn_sqlite3_exec(db *(), sql str) -> int
external fn nova_fn_sqlite3_prepare(db *(), sql str) -> (*(), int)
external fn nova_fn_sqlite3_step(stmt *()) -> int
external fn nova_fn_sqlite3_column_int(stmt *(), col int) -> int
external fn nova_fn_sqlite3_finalize(stmt *()) -> int

// SQLite return codes (extract subset).
const SQLITE_OK   int = 0
const SQLITE_ROW  int = 100
const SQLITE_DONE int = 101

type DbError | OpenFailed(int) | ExecFailed(int) | PrepareFailed(int)

// Open database, wrap raw ptr в typed Db handle.
fn Db.open(path str) Fail[DbError] -> Db {
    ro (raw, rc) = nova_fn_sqlite3_open(path)
    if rc != SQLITE_OK { Fail.throw(DbError.OpenFailed(rc)) }
    Db { value: raw }
}

// Execute SQL (no result set).
fn Db @exec(sql str) Fail[DbError] -> () {
    ro rc = nova_fn_sqlite3_exec(self.value, sql)
    if rc != SQLITE_OK { Fail.throw(DbError.ExecFailed(rc)) }
}

// Close. consume — после @close handle invalid (D131).
fn Db consume @close() -> () {
    nova_fn_sqlite3_close(self.value)
}

// Example usage:
//
//   ro db = Db.open("/tmp/test.db")!
//   db.@exec("CREATE TABLE users (id INT, name TEXT)")!
//   db.@exec("INSERT INTO users VALUES (1, 'Alice')")!
//   defer db.@close()
//   ...
```

### Ключевые паттерны

- **Типизированный хендл.** `type Db { ro value *() }` делает `Db` номинально
  отличным от сырого `*()` — передача не того хендла = ошибка компиляции.
- **Кортежный возврат.** `nova_fn_sqlite3_open` возвращает `(*(), int)`. Nova
  деструктурирует: `ro (raw, rc) = nova_fn_sqlite3_open(path)`.
- **Очистка.** `fn Db consume @close()` — инвалидирует хендл, предотвращает
  use-after-close через consume-bit (D131). Комбинируйте с `defer
  db.@close()` для устойчивости к утечкам.
- **Отображение ошибок.** Оборачивайте C-коды возврата в Nova-тип-сумму для
  безопасной по типам обработки ошибок.

## Пример 2 — libpng (чтение PNG в буфер пикселей)

```nova
module my_app.png

type PngFile { ro value *() }
type PngInfo { ro value *() }

external fn nova_fn_png_create_read_struct() -> *()
external fn nova_fn_png_create_info_struct(png *()) -> *()
external fn nova_fn_png_init_io(png *(), fp *()) -> int
external fn nova_fn_png_read_info(png *(), info *()) -> int
external fn nova_fn_png_get_image_width(png *(), info *()) -> int
external fn nova_fn_png_get_image_height(png *(), info *()) -> int
external fn nova_fn_png_destroy_read_struct(png *(), info *()) -> ()

fn PngFile.from_handle(p *()) -> PngFile => PngFile { value: p }

fn read_image_dimensions(file_handle *()) -> (int, int) {
    ro png = nova_fn_png_create_read_struct()
    ro info = nova_fn_png_create_info_struct(png)
    nova_fn_png_init_io(png, file_handle)
    nova_fn_png_read_info(png, info)
    ro w = nova_fn_png_get_image_width(png, info)
    ro h = nova_fn_png_get_image_height(png, info)
    nova_fn_png_destroy_read_struct(png, info)
    (w, h)
}
```

(C-шим повторяет паттерн sqlite3 — см. `sqlite3_ffi.h`.)

## Пример 3 — libcurl (синхронный HTTP GET)

```nova
module my_app.curl

type CurlHandle { ro value *() }
type CurlResult | Success | Failed(int)

external fn nova_fn_curl_easy_init() -> *()
external fn nova_fn_curl_easy_setopt_url(h *(), url str) -> int
external fn nova_fn_curl_easy_setopt_write_to_buffer(h *()) -> int
external fn nova_fn_curl_easy_perform(h *()) -> int
external fn nova_fn_curl_easy_cleanup(h *()) -> ()
external fn nova_fn_curl_get_response_body() -> str

fn CurlHandle.new() -> CurlHandle {
    ro raw = nova_fn_curl_easy_init()
    CurlHandle { value: raw }
}

fn CurlHandle @get(url str) -> (CurlResult, str) {
    nova_fn_curl_easy_setopt_url(self.value, url)
    nova_fn_curl_easy_setopt_write_to_buffer(self.value)
    ro rc = nova_fn_curl_easy_perform(self.value)
    ro body = nova_fn_curl_get_response_body()
    if rc == 0 { (CurlResult.Success, body) }
    else       { (CurlResult.Failed(rc), body) }
}

fn CurlHandle consume @close() -> () {
    nova_fn_curl_easy_cleanup(self.value)
}
```

## Шпаргалка по ABI

Для кортежных возвратов external fn C ABI определяется раскладкой элементов:

| Кортеж | Sys V AMD64 | Windows x64 MSVC | macOS ARM64 |
|---|---|---|---|
| `(*(), i32)` (12 байт) | регистры (`rax:rdx`) | скрытый out-ptr (`rcx`) | `X0:X1` |
| `(*(), int)` (16 байт) | регистры (`rax:rdx`) | скрытый out-ptr | `X0:X1` |
| `(*(), *())` (16 байт) | регистры | скрытый out-ptr | `X0:X1` |
| `(*(), int, int)` (24 байта) | скрытый out-ptr | скрытый out-ptr | скрытый out-ptr |
| Больше | скрытый out-ptr | скрытый out-ptr | скрытый out-ptr |

Nova не переопределяет calling convention — C-компилятор выбирает сам на
основе платформенного ABI. C-сторонний шим и Nova-сторонняя декларация должны
давать совпадающую раскладку структуры (гард `#ifndef NOVA_TUPLE_TYPEDEF_<m>`
из Plan 115 D214 гарантирует единственное определение).

## Соображения о безопасности

- **Владение.** Nova GC **не** отслеживает значения `*()` — это домен FFI.
  Сопоставьте каждый `_open()` / `_init()` / `_alloc()` с
  `_close()` / `_destroy()` / `_free()`. Используйте `consume`-методы и
  `defer` для устойчивости к утечкам.
- **Время жизни.** `*()` из C-библиотеки действителен только до
  соответствующего cleanup-вызова. Nova на этапе компиляции этого не
  гарантирует; полагайтесь на паттерн (consume + defer).
- **Проверка null.** Всегда проверяйте возвращаемые значения на null
  (`0 as *()`) перед использованием. Многие C-библиотеки возвращают NULL при
  сбое аллокации.
- **Потокобезопасность.** У большинства C-библиотек есть контракты
  потокобезопасности. Если Nova порождает fibers, трогающие хендл, убедитесь,
  что хендл либо потокобезопасен, либо привязан к одному fiber.

## Typed pointers + модель unsafe (Plan 118 — влито)

> **Статус:** влито (Plan 118 → 138.5 FINAL D216; unsafe fn keyword — 118.1.7;
> C-ABI checker — 174.6). Reference doc: [`docs/guide/typed-pointers.md`](typed-pointers.md).
> Plan: [`docs/plans/118-typed-pointers-and-unsafe.md`](../plans/118-typed-pointers-and-unsafe.md).
> Ниже — эволюция FFI-паттернов от opaque `*()` к typed `*T`; оба варианта
> компилируются сегодня (opaque — legacy-совместимый, typed — предпочтительный).

FFI-паттерны от opaque `*()` перешли к typed pointer family `*T` для
type-safe FFI с buffers / structs / nullable returns:

```nova
// Plan 115 V1 / Plan 134 (current — works today):
external fn nova_sqlite3_open(path str) -> (*(), int)

ro (h, rc) = nova_sqlite3_open(path)
if rc != 0 { Fail.throw(DbError.OpenFailed(rc)) }

// Plan 118 V2 (typed + nullable + NPO):
external fn sqlite3_open(path str) -> (Option[Sqlite3Handle], i64)
type Sqlite3Handle(*sqlite3)               // tuple newtype, zero-overhead

unsafe {
    match sqlite3_open(path) {
        (Some(h), 0) => use_handle(h),
        (None, rc)   => Fail.throw(DbError.OpenFailed(rc)),
        (Some(_), rc) => Fail.throw(DbError.OpenFailed(rc)),  // C bug
    }
}
```

**Ключевые улучшения:**

| | Plan 115 V1 / Plan 134 (`*()`) | Plan 118 V2 (*T family) |
|---|---|---|
| Type safety | ❌ opaque `*()` cast вручную | ✓ compile-time pointee check |
| Mutability | ❌ нет различия | ✓ `*ro T` / `*mut T` |
| Null safety | ❌ `0 as *()` runtime check | ✓ `Option[*T]` + NPO zero-cost |
| FFI buffer | ❌ untyped `*()` + manual offset | ✓ `*ro u8` / `*mut u8` typed |
| Callback registration | ❌ N/A | ✓ `*fn(Args) -> Ret` |

**Путь миграции:**
- `ptr` → `*()` (Plan 134 — ошибка компиляции на голый `ptr` в позиции типа)
- `0 as ptr` → `0 as *()`
- литералы `null ptr` (уже отозваны Plan 118 A23) → `0 as *()`
- Record-обёртки хендлов `type X { ro value *() }` → tuple
  newtype `type X(*)()` или `type X(*T)` для zero-overhead ABI

См. [`docs/guide/typed-pointers.md`](typed-pointers.md) для полной reference
documentation и [`examples/typed_pointers/`](../../examples/typed_pointers/)
для minimal working samples.

## Plan 118.1 — FFI intrinsics (foundation)

### CStr-хендл (type-safe const char*)

```nova
import std.ffi.cstr.{CStr}

// External fn principal pattern — typed handle вместо bare *u8
external fn c_strlen(s CStr) -> i64
external fn c_printf(fmt CStr) -> i32
```

Backing-тип CStr: `*u8` (Plan 118 typed pointer). ABI маршалится в
`const char*` / `uint8_t*`.

**Методы конвертации (`@to_cstr`, на основе копирования, Plan 199 / D418, 2026-07-11):**

```nova
ro s = "hello"
ro c = s.to_cstr()              // GC-allocs a fresh byte_len()+1 NUL-terminated copy (panics on embedded NUL)

// Zero-alloc overload — copy into a caller-provided buffer (hot FFI paths):
ro buf = unsafe { RawMem.alloc(64) }
ro c2 = s.to_cstr(buf, 64)      // copies ≤63 bytes + '\0'; TRUNCATES if longer, no scan

// Direct usage в FFI call:
ro n = c_strlen(s.to_cstr())
```

Чисто-Nova реализация в `std/src/ffi/cstr.nv`: `str` не несёт гарантии
хвостового NUL (D418 отзывает D26 §Nul-termination), поэтому ОБА оверлоада
`to_cstr` КОПИРУЮТ — zero-copy пути нет. `to_cstr()` аллоцирует свежий
GC-управляемый буфер на `byte_len()+1` байт, копирует байты и дописывает `\0`
— после O(n)-скана на встроенные NUL ([M-118.1-cstr-nul-check]).
`to_cstr(buf, buf_size)` копирует в буфер вызывающего, ограничиваясь
`buf_size - 1` + терминатор (усекая, без скана — явный hot-path «я владею
буфером»; `as_cstr`/`as_cstr_unchecked` ВЫВЕДЕНЫ, имя `to_` честно называет
копирование).

### addr_of / addr_of_mut (создание указателей в стиле Zig)

```nova
unsafe {
    ro x = 42
    ro p = addr_of(x)         // *T pointer к local
    assert(p.read() == 42)
}

unsafe {
    mut buf = 0
    ro p = addr_of_mut(buf)   // *T (mut binding required)
    p.write(100)               // codegen TBD per Ф.4
}
```

Эквивалентно оператору `&x` (UnOp::AddrOf), дешугарится rewriter'ом.
Используйте, когда явный синтаксис вызова функции улучшает читаемость FFI.
Та же enforce-логика: требуется unsafe-контекст, запрет `#realtime`, валидация
lvalue (E_AMP_LITERAL / E_AMP_RECORD_LITERAL / E_ARRAY_INDEX_PTR_BANNED).

### Intrinsics RawMem (массовые операции с памятью)

```nova
import std.runtime.raw_mem.{RawMem}

unsafe {
    RawMem.copy(src, dst, n_bytes)              // memmove-safe
    RawMem.copy_nonoverlapping(src, dst, n)     // memcpy fast-path
    RawMem.fill(dst, byte_value, n)             // memset
    ro cmp = RawMem.compare(a, b, n)            // memcmp
}
```

### Типизированные read/write на primitive `*T`

```nova
unsafe {
    ro p = addr_of(some_int)
    ro v = p.read()                  // typed primitive read
    p.write(100)                     // typed write (на *mut T)
    ro v_vol = p.read_volatile()     // MMIO read
    p.write_volatile(0xDEAD)         // MMIO write
}
```

### Cross-refs

- Spec D216 (typed pointers) + §22 (CStr type) — `spec/decisions/02-types.md`
- Spec D418 §`str` без NUL-терминатора; copy-based `CStr`/`to_cstr`
  (отзывает D26 §«Nul-termination») — `spec/decisions/08-runtime.md`
- Plan-doc — `docs/plans/118.1-ffi-intrinsics-and-cstring.md`,
  `docs/plans/199-str-drop-nul-termination.md`

## unsafe fn — объявление и вызов unsafe-функций (Plan 118.1.7)

> Plan 118.1.7 мигрирует с атрибута `#unsafe fn` на ключевое слово `unsafe fn`
> (согласовано по типам с TypeRef::Unsafe из Plan 118.5 и типом fn-ptr
> `*unsafe fn(...)` из Plan 118.1.6). `#unsafe fn` теперь — жёсткая ошибка
> (`E_UNSAFE_ATTR_DEPRECATED`).

### Объявление unsafe Nova-функции

```nova
// unsafe fn — body has implicit unsafe context (pointer ops allowed without unsafe {})
export unsafe fn read_first_byte(p *u8) -> u8 {
    // No `unsafe { }` needed here — the body of an unsafe fn is implicitly
    // unsafe, so the raw-pointer read below is permitted directly.
    p.read()
}
```

### Объявление unsafe external (C) функции

```nova
// external unsafe fn — requires unsafe {} at call site
// pointee-mut written postfix: `*mut u8` = writable target (FINAL, Plan 138.5)
external unsafe fn RawMem.copy(src *u8, dst *mut u8, n int) -> ()
external unsafe fn RawMem.fill(dst *mut u8, byte_value u8, n int) -> ()
```

### Вызов unsafe-функции

```nova
// Caller MUST wrap in unsafe {}  — E_UNSAFE_CALL_REQUIRES_WRAP otherwise
unsafe {
    RawMem.copy(src_ptr, dst_ptr, n)
    ro b = read_first_byte(src_ptr)
}
```

### unsafe fn как тип указателя на функцию

```nova
// addr_of(unsafe fn) propagates unsafe to fn-ptr type: *unsafe fn(...)
unsafe fn risky(p *u8) -> () { /* ... */ }
ro fn_ptr = addr_of(risky)   // type: *unsafe fn(p *u8) -> ()

// Calling via unsafe fn pointer also requires unsafe {}
unsafe { fn_ptr(some_ptr) }
```

### Cross-refs

- D216 §9 (синтаксис ключевого слова unsafe fn) — `spec/decisions/02-types.md`
- D2 (модель эффекта unsafe, Plan 118.1.7 amend) — `spec/decisions/04-effects.md`
- Plan-doc — `docs/plans/118.1.7-unsafe-fn-keyword-syntax.md`

## C-ABI типы для `extern "C" fn` (Plan 174.6 / D282 rule 2 + D353)

> **Статус:** Plan 174.6 M1/M2 (2026-07-04). Чекер валидирует каждую
> сигнатуру `extern "C" fn` (параметры **и** возврат) против рекурсивного
> C-ABI списка типов; не-C-ABI типы → `E_FFI_NON_C_ABI_TYPE`. Spec:
> [D282 rule 2](../../spec/decisions/08-runtime.md#d282) + [D353](../../spec/decisions/08-runtime.md#d353).

### Что может пересекать границу `extern "C" fn`

Набор определяется рекурсивно:

```
C_ABI  ::= Scalar | RawPtr | FnPtr | Option[RawPtr] | Tuple[C_ABI…] | ValueRecord{ C_ABI… }
Scalar ::= int | uint | i8..i64 | u8..u64 | f32 | f64 | bool | char
RawPtr ::= *T | *() | CStr
FnPtr  ::= *extern "C" fn(C_ABI…) -> C_ABI
```

| Категория | C-ABI? | Примечания |
|---|---|---|
| `int` / `uint` | ✅ | адресной ширины (`nova_int` = `intptr_t`, `nova_uint` = `uintptr_t`); **≠** `i64`/`u64` |
| `i8`..`i64`, `u8`..`u64`, `f32`, `f64`, `bool` | ✅ | фиксированной ширины скаляры |
| `char` | ✅ | `uint32_t` кодпоинт (валидность — инвариант Nova; Rust `improper_ctypes` флагует аналог) |
| `*T`, `*()`, `CStr` | ✅ | любой pointee; рекурсия останавливается на адресе. `*()` = `void*`, `CStr` = `const char*`/`*u8` |
| `str` | ✅ | value-record `{ptr,len}` (D139) — POD-структура. **НЕ NUL-терминирована**; для C-строк используйте `s.as_ptr()`/`s.byte_len()` или `s.to_cstr()` |
| value-record `type X value {…}` | ✅ при всех C-ABI полях | C-структура по значению; ключевое слово `value` **обязательно** (без него — heap GC-record, по ссылке, **не** C-ABI) |
| named-tuple `type X(a T, b U)` / анонимный кортеж `(T, U)` | ✅ при всех C-ABI элементах | по значению; многозначные возвраты |
| циклический value-record `type Node value {val int, next *Node}` | ✅ | `*Node` — сырой указатель → рекурсия завершается |
| `Option[*T]` (любой указатель) | ✅ | NPO: `None` = 0, `Some(p)` = `p` (zero-cost nullable-указатель) |
| `*extern "C" fn(…) -> …` | ✅ | C-колбэк (см. ниже); типы сигнатуры сами C-ABI |
| `-> ()` (возврат верхнего уровня) | ✅ | снижается до C `void` |
| `()` как **параметр/элемент** | ❌ | в C нет unit-типа |
| `Vec[T]`, ссылки на heap-record | ❌ | GC-управляемые, не POD |
| `Result[T,E]`, `Option[non-ptr]`, другие суммы | ❌ | раскладка tag+payload не C-ABI |
| голый `fn(…)` closure, Nova-ABI `*fn(…)` | ❌ | fat/несущий handler; настоящий C-колбэк обязан быть помечен `*extern "C" fn` |

**Допущение о раскладке (S8).** value-record/кортеж, передаваемый по значению,
предполагает Nova-раскладку == C-раскладку (порядок полей + выравнивание).
Nova эмитит структуру в порядке объявления; совпадение со структурой
**внешней** C-библиотеки — контракт автора (рассогласование пока не ловится
компилятором — follow-up `[M-174.6-ffi-struct-layout]`).

### Колбэки — `*extern "C" fn` (компаратор qsort, обработчик libuv)

Nova-функция, передаваемая в C как колбэк, обязана быть помечена
`*extern "C" fn(...)`. Коэрция `fn → *extern "C" fn` принимается **тогда**,
когда fn (1) C-ABI во всех аргументах/возврате, (2) captureless (без env) и
(3) **effect-free** (не объявляет эффектов вообще). Причина (3): C вызывает
колбэк **без handler-frame Nova в стеке**, поэтому любая эффект-операция
(`Fail`, `IO`, пользовательский алгебраический эффект) не имеет места для
разрешения → unsound. Нарушения → `E_FFI_NON_C_ABI_TYPE` /
`E_CLOSURE_HAS_ENV` / `E_CALLBACK_THROWS_OVER_C_ABI`.

```nova
module my_app.sortdemo

// C: void qsort(void* base, size_t n, size_t sz,
//               int (*cmp)(const void*, const void*));
extern "C" fn qsort(base *(), n uint, size uint,
                    cmp *extern "C" fn(*(), *()) -> i32) -> ()

// A captureless, effect-free free fn — coerces to the C callback type.
fn cmp_i32(a *(), b *()) -> i32 {
    // read both operands via typed pointers (unsafe), compare … (elided)
    0
}

fn sort_it(buf *(), n uint) {
    // `cmp_i32 as *extern "C" fn(...)` — accepted (captureless + effect-free + C-ABI)
    qsort(buf, n, 4 as uint, cmp_i32 as *extern "C" fn(*(), *()) -> i32)
}
```

Обработчик в стиле libuv, хранимый в record-хендле (D353/M2 — тег легален как
**поле** value-record, и его сигнатура всё равно валидируется как C-ABI):

```nova
module my_app.uvdemo

// A handle that carries a C-ABI callback pointer (validated by the checker).
type UvTimer value {
    handle *()
    on_tick *extern "C" fn(*()) -> ()
}

extern "C" fn uv_timer_start(t *(), cb *extern "C" fn(*()) -> (),
                             timeout u64, repeat u64) -> i32

fn on_tick(h *()) -> () { /* … effect-free … */ }
```

**Что отклоняется** (каждый — отдельный neg-кейс `extern "C" fn`):

```nova
extern "C" fn bad1(v Vec[int]) -> int              // E_FFI_NON_C_ABI_TYPE (GC)
extern "C" fn bad2(r Result[int, str]) -> int      // E_FFI_NON_C_ABI_TYPE (tagged union)
extern "C" fn bad3(x Option[int]) -> int           // E_FFI_NON_C_ABI_TYPE (no NPO)
extern "C" fn bad4(x ()) -> int                    // E_FFI_NON_C_ABI_TYPE (unit param)
extern "C" fn bad5(cb *fn(i64) -> i64) -> int      // E_FFI_NON_C_ABI_TYPE (Nova-ABI *fn)
// coercions:
//   (fn(x i64) => x*2) as *extern "C" fn(i64)->i64  → E_CLOSURE_HAS_ENV  (env)
//   throwing_fn as *extern "C" fn(...)              → E_CALLBACK_THROWS_OVER_C_ABI (Fail)
//   effectful_fn as *extern "C" fn(...)             → E_CALLBACK_THROWS_OVER_C_ABI (any effect)
```

### Владение / пиннинг через границу

Указатели Nova, отданные C, **заимствуются только на время вызова**. Boehm-GC
не сканирует C-`malloc`'нутую память, поэтому Nova-указатель, **удерживаемый**
C после вызова (хранимый в C-структуре, захваченный регистрацией колбэка),
может быть собран → use-after-free. Чтобы держать Nova-объект живым между
вызовами, запиньте его (держите живой Nova-ссылки на время C-видимой жизни
объекта; выделенный pinning-API — follow-up). Это повторяет правило времени
жизни `str.as_ptr()` (D294): указатель действителен, только пока `str` жива.

---

## Build pipeline — манифест `[ffi]` и `[ffi.staticlib]`

Всё выше — как *написать* границу `.nv` ↔ native. Этот раздел — как *подключить*
native-артефакты к сборке, чтобы `import` модуля тянул их **автоматически**, без
правок компилятора. Декларация — в `nova.toml` пакета. Полное «как сделать
модуль» — [authoring-a-module §7](authoring-a-module.md#7-native-backed-модуль-частный-случай).

### `[ffi]` — готовые `.c`-шимы и системные `.lib` (Plan 115 D214)

Для тонкого C-шима и линковки уже-собранной системной библиотеки:

```toml
[ffi]
c_shims      = ["native/sqlite3_shim.c"]            # компилируются и линкуются
include_dirs = ["native/", "third_party/sqlite3/"]  # → clang -I
libs         = ["sqlite3"]                          # → clang -lsqlite3 / sqlite3.lib
```

Пути — относительно `nova.toml`; резолвятся в абсолютные перед вызовом clang.
`.h`-only inline-шимы включаются force-include (`-include`), `.c` — как
compilation unit. Секция `[ffi]` может быть пустой (`FFI-aware`-маркер).

### `[ffi.staticlib]` — RETRACTED (Plan 195)

**Ретрактировано владельцем 2026-07-10 (Plan 195).** Секция существовала
(Plan 195) как обобщение хардкода `detect_tls`/`tls-cache`/`-lbcrypt -lntdll`
на манифест-механизм, собирающий native-артефакт cargo'ом/make на лету. Она
позволяла пользовательскому native-модулю требовать Rust/cargo как часть
своей сборки — противоречит канону тулчейна (**компилятор Nova + clang**,
`.nv → .c → бинарь`, БЕЗ Rust/cargo). `compiler-codegen/tls_shim/`
(Rust-staticlib, rustls) — единственный пользователь механизма — заменён на
`compiler-codegen/nova_rt/tls_c_shim.c` (mbedTLS, обычный `[ffi]`-путь).
`FfiStaticlibConfig`/`resolve_ffi_staticlib`/`[ffi.staticlib]`-парсинг убраны
из `manifest.rs`/`test_runner.rs` целиком.

**Канон native-модуля теперь — только `[ffi]` выше**: `.c`-шим (компилит
clang, он в тулчейне) + опционально готовая `.lib`/`.a` (линкуется, не
собирается — vcpkg/системный пакет/vendored-копия, см. `detect_brotli`/
`detect_boehm`/`detect_mbedtls` в `test_runner.rs` для паттерна условной
линковки библиотеки, ГОТОВОЙ заранее, а не строящейся build-скриптом).

**Эталон.** `std/tls` (`nova_rt/tls_c_shim.c` + vcpkg mbedTLS) — реальный
пример: mbedTLS ставится через `vcpkg install` (см. `compiler-codegen/
vcpkg.json`, gitignored per-checkout), `tls_c_shim.c` компилируется/линкуется
УСЛОВНО по факту использования `tls_*`-символов (тот же D337-механизм, что у
brotli), без манифест-декларации в `std/nova.toml` вообще — линковка целиком
в `test_runner.rs::build_command` (как `net.c`/`brotli_shim.c`).

---

## Followups

| Маркер | Что | Статус |
|---|---|---|
| `[M-115-newtype-constructor]` | конструктор tuple-newtype `type X(ptr)` + доступ `.0` | ✅ CLOSED 2026-06-01 (доставлен канонический синтаксис) |
| `[M-115-ffi-build-pipeline]` | пайплайн сборки/линковки пользовательских шимов | ✅ CLOSED — реализован декларативно через `nova.toml` `[ffi]` (готовые шимы/libs, Plan 115). `[ffi.staticlib]` (собираемый staticlib, Plan 195) RETRACTED владельцем (Plan 195) — native-модуль обязан собираться БЕЗ Rust/cargo. См. [«Build pipeline»](#build-pipeline--ffi-и-ffistaticlib-манифест) |
| `[M-115-bindgen-tool]` | `nova bindgen header.h` авто-генерируемые биндинги | 🟡 deferred (major tooling, отдельный план) |
| `[M-115-d126-deprecation]` | аудит миграции `external type X` D126 | ✅ CLOSED: Plan 91.12 V2 hard retract — `external type X` теперь жёсткая ошибка E_EXTERNAL_TYPE_RETRACTED (последовательность: newtype-constructor ✓ → Plan 91.12 Pattern B → D126 retract выполнен) |
| `[M-115-tuple-gc-types]` | GC-tracked типы в элементах кортежа в возвратах external fn | 🟢 CLOSED as by-design (граница extern "C" корректно исключает Nova-типизированные контейнеры) |
| `[M-115-external-fn-method]` | external fn-метод на приёмнике | 🟢 CLOSED as not needed (свободной fn + Nova-side обёртки достаточно) |
| `[M-115-examples-ffi-real-build]` | реальная линковка libsqlite3 через vcpkg | 🟡 deferred (V1 поставляет встроенный mini-sqlite-эквивалент в `nova_rt/sqlite_mini_ffi.h` — доказывает end-to-end FFI-механизм без внешней зависимости; реальная линковка → CI step) |
| `[M-115-null-ptr-to-option-after-npo]` | hard-retract `null ptr` после Plan 118 Option[*T] NPO | ✅ CLOSED Plan 134 (2026-06-09) — `ptr` удалён; используйте `*()` и `0 as *()` |
