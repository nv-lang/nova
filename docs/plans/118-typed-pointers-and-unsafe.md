// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 118 — Typed pointers (`*T` family) + unsafe model

> **Создан 2026-05-31.**
> **Статус:** 🆕 PLANNED.
> **Приоритет:** P1 — **language addition** для production-grade FFI и
>   низкоуровневых сценариев. Plan 115 v1 (`ptr` + tuple FFI + opaque handle
>   pattern) разблокировал базовый FFI, но **type-system не различает** какой
>   указатель куда смотрит, mutable он или нет, и не оберегает от dangling
>   при работе со stack-значениями. Plan 118 закрывает это (typed `*T` family
>   + safety через `unsafe` model + null-safety через NPO).
>
>   Без Plan 118 любая user-side работа с raw memory (FFI к C-буферам,
>   memory-mapped I/O, low-level data structures) — type-unsafe. Plan 115
>   v1 даёт только opaque handles; данные через FFI cross'аются только
>   через `ptr` (typeless) или out-params, что неэргономично.
> **Оценка:** ~7-10 dev-day.
>   - *T family parser/checker: ~1.5 day
>   - `&value` operator + escape/auto-promote: ~1.5 day
>   - `unsafe { }` block + `#unsafe` attribute (D2 amend): ~1 day
>   - Auto-deref + pointer ops (arith/casts/compare): ~1 day
>   - `Option[*T]` + NPO codegen: ~1 day
>   - `*fn(...)` function pointers: ~½ day
>   - `ptr` redefine + Plan 115 D214 amend: ~½ day
>   - Regression + cross-platform + spec + docs + close: ~1-1.5 day
> **Зависимости:**
>   - **Plan 115 v1** ✅ planned (D214) — `ptr` built-in + tuple FFI + opaque
>     handle pattern. Plan 118 **переопределяет** `ptr` как `type ptr ?*unsafe ()`
>     newtype (D214 amend); Plan 115 ships первым, Plan 118 надстраивается.
>   - **Plan 114** ✅ closed (D184 master keyword refresh) — `ro`/`mut`/`consume`
>     keywords + `let` removed; Plan 118 пишется в post-114 syntax. Binding-modifier
>     rule (binding `mut` → pointer `mut` по default) — критическая mechanic
>     Plan 118.
>   - **Plan 114.4** ⏳ planned (D199/D200 — const fn + assoc const) — extracted
>     из Plan 114 Ф.9-Ф.11; orthogonal к Plan 118 (const story vs pointer story),
>     no interaction. Cross-ref только для D-block numbering coordination.
>   - **Plan 120** ✅ planned (D215) — named tuples + value/reference
>     allocation contract. Plan 118 leverages allocation contract: stack-values
>     (`()` tuples, primitives) auto-promote в heap при `&` escape; heap
>     references (records, `{}`) — `&` создаёт pointer-на-reference.
>   - **D2** ([04-effects.md#d2](../../spec/decisions/04-effects.md#d2))
>     **AMEND** — D2 v1 отменил keyword `unsafe` в пользу effect mechanism.
>     Plan 118 **восстанавливает** `unsafe { }` keyword как **syntactic sugar**
>     для built-in effect handler (`with unsafe_handler { perform UnsafeOp.* }`).
>     D2 spirit сохранён (всё — эффекты под капотом), но user-facing syntax
>     ergonomic.
>   - **Plan 113** ✅ closed (D172) — `#realtime` attribute + sync-class
>     enforcement. Pointer ops в `unsafe { }` blocks — runtime side-effects;
>     Plan 113 type-checker должен пропускать (не блокировать через
>     `#realtime` ban).
> **D-блоки:** **новый D216** (typed pointer family + unsafe model + null-safety
>   через NPO) + **D2 AMEND** (`unsafe { }` keyword restored as effect-handler
>   sugar) + **D214 AMEND** (Plan 115 `ptr` redefinition as `type ptr ?*unsafe ()`).
> **Worktree convention:** `nova-p118` (создать через worktree hook первой Bash
>   командой).
>
> **Recommended model:**
>   - **Opus 4.7 + Thinking ON** — language addition (parser + type-checker +
>     codegen для new built-in family + unsafe gating). Safety-critical
>     (pointer model errors = silent memory corruption). Cross-platform ABI
>     validation требует attention к detail.
>   - **Sonnet 4.6 НЕ рекомендую** — pointer type system + unsafe enforcement
>     errors имеют security implications; Opus required.
>
> **Workflow требования (для агента):**
>   - **Commit per phase** — после каждой Ф.N (Ф.0..Ф.8) отдельный commit
>     с conventional message `feat(plan118 Ф.N): <summary>`.
>   - **Update logs after each big task:**
>     - `docs/project-creation.txt` — sprint section про Plan 118 progress
>     - `docs/simplifications.md` — open/close `[M-118-*]` markers
>     - `nova-private/discussion-log.md` (отд. репо) — design decisions
>       (binding-mut rule, escape/auto-promote semantics, unsafe model)
>   - **Tests через release nova** — `cargo build --release` затем `./target/
>     release/nova test` (не debug build — codegen может отличаться).
>   - **Per-fix verify** — только targeted fixture, full `nova test` только
>     в конце phase.
>   - **Status section в конце plan-файла** — заполняется агентом по
>     завершении (per phase + final summary).
>   - **Safety hatches per phase preambles** — explicit decision points для
>     extract в sub-plans если scope превышает estimate (e.g., escape analysis
>     edge cases, NPO codegen complexity).
>
> **Production-grade требование:** реализация без упрощений. *T family —
>   first-class в parser/checker/codegen/runtime; unsafe model — full
>   enforcement (compile-time errors `E_UNSAFE_REQUIRED`); NPO codegen —
>   zero-cost (один pointer-word, не tagged struct); escape analysis —
>   correct для всех stack-value scenarios; cross-platform validated
>   (Linux/Windows/macOS × clang/MSVC/gcc).

---

## Зачем

### Что отсутствует в Nova сейчас (после Plan 115 v1)

После Plan 115 v1 FFI ergonomics достаточны для **opaque handles** (sqlite3
sessions, libuv listeners, rustls sessions — Plan 116):

```nova
type Sqlite3Handle(ptr)                                   // opaque, OK
external fn nova_sqlite3_open(path str) -> (Sqlite3Handle, i64)
```

Но **любая работа с typed memory** через FFI (буферы данных, struct fields,
out-params) — невозможна ergonomic'но:

```nova
// Нельзя: external fn copy_buffer(src ???, dst ???, len usize)
//        — нет типизированных pointer'ов для src/dst
// Workaround V1: использовать ptr (untyped) + manual casts в C-shim — ugly
```

Низкоуровневые сценарии без typed pointers:
1. **C-FFI с typed buffers** (libpng image data, libcurl headers, sqlite
   blob columns).
2. **Memory-mapped I/O** (registers, framebuffers, shared memory).
3. **Manual linked structures** (intrusive lists, lock-free queues, custom
   allocators).
4. **Performance-critical hot loops** (когда GC overhead measurable).
5. **Out-params для FFI** (стандартный C pattern: `int func(out int* result)`).

### Зачем typed pointers (vs только `ptr`)

| | Только `ptr` (Plan 115 v1) | `*T` family (Plan 118) |
|---|---|---|
| **Type safety** | ❌ casts wherever | ✓ compile-time type check |
| **Mutability** | ❌ нет различия ro/mut | ✓ `*ro T` / `*mut T` |
| **Auto-deref** | ❌ нет (Nova vis ptr opaque) | ✓ `p.field`, `*p` |
| **Null safety** | ❌ `ptr` всегда может быть null | ✓ `*T` non-null, `Option[*T]` для nullable |
| **FFI ergonomics** | ❌ workarounds через out-params | ✓ direct typed signatures |
| **Self-documenting** | ❌ `ptr` непонятно куда смотрит | ✓ `*ro UserData` ясно |

### Зачем `unsafe` model (вместо разрешения везде)

Без unsafe-gating typed pointers **становятся опаснее `ptr`** — пользователь
deref'ит pointer без awareness про GC-move / dangling / aliasing:

```nova
// Без unsafe-gating — silent UB:
ro buf = some_array.as_ptr()      // *ro T откуда-то
ro x = *buf                        // ← может быть dangling после GC move; UB
```

`unsafe { }` блок и `#unsafe` attribute — **explicit boundary** между
type-safe Nova code (большая часть программ) и низкоуровневым FFI/perf
кодом. Pattern проверен Rust'ом (10+ лет production), C# (15+ лет), D
(`@safe`/`@trusted`/`@system`).

### Зачем NPO (null pointer optimization)

`Option[T]` в Nova — sum-type (16 bytes на 64-bit: tag + payload). Для
`Option[*T]` это **избыточно**: pointer может быть `NULL` natively, tag
не нужен. NPO codegen:

```c
// Без NPO (16 bytes):
struct NovaOpt_ptr_Acc { int tag; void* value; };

// С NPO (8 bytes):
typedef Acc* NovaOpt_ptr_Acc;    // NULL == None, non-null == Some(ptr)
```

Это **mainstream pattern** (Rust `Option<&T>` size = `&T`); zero-cost
abstraction для FFI с C (`malloc` returns `void*`, `NULL` = OOM).

### Mainstream comparison

| Язык | Typed pointers | Unsafe model | Null safety |
|---|---|---|---|
| Rust | `*const T` / `*mut T` (raw); `&T` / `&mut T` (refs) | `unsafe { }` block + `unsafe fn` | `Option<&T>` + NPO |
| Zig | `*T` / `*const T` / `*allowzero T` / `[*]T` | Нет keyword; explicit cast intrinsics | `?*T` syntax |
| C# | `T*` (unmanaged) / `ref T` / `in T` / `out T` | `unsafe` modifier (class/method/block) | `T?` reference nullable |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | Type-based (Unsafe* prefix); scoped APIs | Optional types |
| D | `T*` / `ref T` / `scope T*` | `@safe` / `@trusted` / `@system` attributes | `Nullable!T` |
| Go | `*T` (managed); `unsafe.Pointer` (raw) | `unsafe` package import | Nil pointers (runtime) |
| Java/Kotlin | References (managed) | `sun.misc.Unsafe` package | Nullable types (Kotlin) |
| **Nova V1** (Plan 115 v1) | `ptr` (untyped) только | Нет | `ptr` nullable runtime check |
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` model + NPO | `unsafe { }` + `#unsafe` | `Option[*T]` NPO |

Nova V2 будет на уровне Rust/Swift (typed + safety boundary + null-optimized),
с GC-friendly семантикой (escape analysis с auto-promote вместо lifetime
checker).

---

## Дизайн

### 1. `*T` family типов

```nova
*T              // ro pointer (default); short form of *ro T
*ro T           // explicit readonly pointer
*mut T          // mutable pointer (can write через *p)
*unsafe T       // unsafe pointer (после арифметики; deref требует unsafe block)
```

**Размер:** все варианты — pointer-width (8 bytes на 64-bit; 4 на 32-bit
bootstrap = только 64-bit).

**ABI:** `T*` в C (compiler emits соответствующий C-type для FFI).

**Default ro:** `*T` ≡ `*ro T` — same default rule как Plan 114 для bindings
(`ro x = ...` default).

### 2. Binding mutability → pointer mutability

```nova
ro p *Acc                   // binding ro; pointer ro (cannot *p = ...)
mut p *Acc                  // binding mut; pointer mut automatically (can *p = ...)
mut p *Acc == mut p *mut Acc          // эквивалентны
ro p *mut Acc                // valid edge case: binding ro, pointee mut
                              // (cannot reassign p, BUT can *p = ...)

mut q = &acc                // pointer mut auto (no need &mut acc)
ro p = &acc                 // pointer ro auto
```

**Rule:** binding modifier пропагирует на pointer mutability **по умолчанию**.
Explicit `*mut T` / `*ro T` overrides только если нужно разойтись (`ro p
*mut T` редкий case).

### 3. Chain order (multi-level pointers)

```nova
*mut *ro Acc        // mut pointer НА (ro pointer на Acc)
                     // — *p = другой_pointer OK
                     // — **p = новое_значение ERROR (внутренний ro)

*ro *mut Acc        // ro pointer НА (mut pointer на Acc)
                     // — *p = ... ERROR (внешний ro)
                     // — **p = ... OK (внутренний mut)
```

**Rule:** modifier перед `*` относится к ЭТОМУ `*`; читать слева-направо.
Canonical Rust grammar.

### 4. `&value` operator + escape analysis с auto-promote

```nova
// Heap reference (record) — & создаёт pointer на reference
ro acc = Account { name: "Piter" }    // acc — heap reference (D32)
ro p = &acc                            // *ro Account; GC tracks acc

// Stack value (primitive, tuple) — & escape triggers auto-promote
ro x = 42_i64                          // x — stack primitive
ro p = &x                              // x auto-promoted to heap; *ro i64

// Tuple на стеке (Plan 120) — auto-promote при &
ro point = (x: 1, y: 2)               // stack tuple (D215)
ro pp = &point                         // point auto-promoted; *ro (x: i64, y: i64)

// Return-escape — тоже auto-promote
fn make_ptr() *ro i64 {
    ro x = 42
    &x                                 // x escapes; promoted to heap → safe to return
}
```

**Escape analysis algorithm:**
1. Парсер собирает все `&local_var` usages.
2. Type-checker определяет escape:
   - `&local` used только в текущем scope (no return, no closure capture, no
     store в heap reference) → **NO promote** (stack-local pointer ok)
   - `&local` returned, captured в closure, stored в record field, или
     passed в fn parameter — **PROMOTE local to heap allocation**.
3. Codegen: для promoted locals аллокация через `nova_alloc` вместо stack
   frame slot.

**Costs:** auto-promote = single heap allocation per promoted local (one-time;
GC reclaims later). Go pattern proven (escape analysis = sub-millisecond
compile overhead).

### 5. Auto-deref для `p.field`

```nova
ro acc = Account { name: "Piter", age: 30 }
ro p = &acc                            // *ro Account

p.name                                  // ✓ auto-deref → "Piter"
p.age                                   // ✓ auto-deref → 30
*p                                      // ✓ explicit deref → Account (the reference)
(*p).name                              // ✓ same as p.name
```

**Rule:** `p.field` auto-derefs **one level** (Go-style). `*p` — explicit
single-level deref. Для multi-level pointer (`**T`) — recursive deref не
делается, нужно `(*p).field` или `(**p)` явно.

**Why one-level only:** auto-deref recursion path-dependent (confusing для
reader); explicit `*` chain — predictable.

**Inside `unsafe` block только:** `p.field` и `*p` — pointer ops, require
unsafe context (см. §«unsafe model» ниже).

### 6. Pointer arithmetic → `*unsafe T`

```nova
unsafe {
    ro p1 = some_ptr + 1            // ❌ valid; результат: *unsafe T
    ro p2 = some_ptr + offset       // ❌ valid; результат: *unsafe T
    *p1                              // ❌ deref *unsafe требует ещё unsafe layer
}
```

**Rule:** `+` / `-` / `+=` / `-=` на pointer'ах:
1. **Только в `unsafe { }` блоке** — outside → `E_UNSAFE_REQUIRED`.
2. **Результат `*unsafe T`** — degrades в "unsafe variant" (alignment + bounds
   не гарантированы).
3. **`*unsafe T` deref** — требует **ещё один `unsafe` wrap** (nested), т.е.
   `*unsafe T` ops цельно opt-in.

**Units:** `p + n` смещает на `n * sizeof(T)` bytes (C/Rust convention).

### 7. Null safety: `Option[*T]` + NPO codegen

`*T` — **non-null гарантированно** (compile-time invariant).
`Option[*T]` — nullable, через **NPO codegen** zero-cost.

```nova
external fn malloc(sz usize) -> Option[*u8]
// Под капотом codegen emits:
//   uint8_t* malloc(size_t sz);   // single pointer; NULL = None, non-null = Some(ptr)

unsafe {
    ro maybe_buf = malloc(1024)
    match maybe_buf {
        Some(buf) => {              // buf: *u8 (non-null guaranteed внутри Some)
            // use buf...
        }
        None => {                   // OOM
            Fail.throw(OutOfMemory)
        }
    }
}
```

**NPO codegen rules:**
1. Compiler detects `Option[*T]` (или nested `Option[*T]` через alias).
2. Lower тип в **single C pointer** (8 bytes), не `struct { tag; payload }`
   (16 bytes).
3. Pattern match codegen: `if (ptr == NULL) None_branch else Some_branch(ptr)`.
4. Construction:
   - `Some(p)` где `p: *T` → emit `p` literally
   - `None` для `Option[*T]` → emit `NULL`
5. **API surface unchanged** — user пишет `Option[*T]` (general type), codegen
   делает NPO transparently.

**ABI compatibility:** NPO layout = C `T*` ABI directly — direct FFI без
wrappers (matches `malloc`/`fopen`/`dlopen` returns).

### 8. `unsafe { }` block model (D2 amend)

```nova
fn safe_user_code() {
    // Pointer ops запрещены — вне unsafe context
    // ro x = *p                  ← ERROR E_UNSAFE_REQUIRED
    
    unsafe {                       // explicit unsafe region
        ro x = *p                  // ✓ inside unsafe
        ro y = malloc(1024)        // ✓ external fn returning pointer
    }
    // Снаружи блока — снова safe context
}
```

**Что внутри `unsafe { }` блока разрешено:**
- Создание `&value` (pointer creation)
- Deref `*p`, auto-deref `p.field`
- Pointer arithmetic (`p + n`) — результат `*unsafe T`
- Cast `usize as *T` (reverse cast)
- Compare `<`/`>` (cross-allocation ordering)
- `&record.field` (GC compaction concern bypass)
- Cross-FFI call с pointer args (call external fn если она accepts/returns `*T`)
- Newtype construction `Handle(some_ptr)` (claims correctness)

**Что safe вне unsafe (consistency с #24 table в design discussion):**
- Объявление типов `*T` в signatures, parameters, fields
- Объявление `external fn` с pointer params
- Чтение field `acc.next` где `next *T` (просто чтение pointer value)
- Pattern match на `Option[*T]` (inspection, не deref)
- Compare `==` / `!=` (identity check)
- Newtype declaration `type Handle ptr`

**Семантика — sugar над эффектом (D2-consistent):**
```nova
// User-facing:
unsafe { ro x = *p }

// Под капотом эквивалент:
with unsafe_handler {              // built-in handler
    ro x = perform UnsafeOps.deref(p)
}
// unsafe_handler — built-in, user не пишет
```

D2 spirit preserved (всё ещё effect mechanics); user syntax ergonomic.

### 9. `#unsafe` attribute на functions

```nova
#unsafe
fn ffi_wrapper(p *T) -> T {
    *p                              // ✓ ok (whole fn unsafe context)
    unsafe { something() }          // ✓ ok (visual marker для опасной секции)
}

fn safe_caller() {
    // ffi_wrapper(p)               ← ERROR; calling #unsafe fn from safe
    unsafe {
        ro x = ffi_wrapper(p)       // ✓ wrap call в unsafe block
    }
}
```

**Rule:**
- `#unsafe` fn — body имплицитно unsafe context (pointer ops без `unsafe { }`
  wrap).
- Вызов `#unsafe` fn — **требует `unsafe { }`** wrap у caller'а (даже если
  caller тоже `#unsafe` — для visual consistency).
- **НЕТ propagation up** — каждая fn сама решает encapsulate или propagate
  (canonical Rust pattern).

### 10. `*fn(...)` function pointers для FFI

```nova
// FFI callback registration (no environment captured)
external fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }

unsafe {
    libuv_set_timer_cb(my_callback as *fn(i64) -> ())
    //                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //                  cast captureless fn → *fn (compile-time check no env)
}
```

**Types:**
- `*fn(Args) -> Ret` — raw function pointer (no environment); C ABI compatible
- `fn(Args) -> Ret` — Nova closure (vtable + environment capture)
- **Cast `fn → *fn`** только если closure captureless (compile-time check;
  иначе `E_CLOSURE_HAS_ENV`)

**Calling convention:** `*fn(...)` использует **C ABI текущей платформы**
(System V на Unix, MS x64 на Windows). Никаких explicit `extern "C"` keywords
— только один ABI поддерживается (vararg / stdcall — Q-block deferred).

### 11. `ptr` redefine (Plan 115 D214 amend)

Plan 115 v1 ввёл `ptr` как built-in primitive. Plan 118 **переопределяет**:

```nova
// Plan 118 definition (D214 amend):
type ptr ?*unsafe ()                  // newtype над nullable unsafe void-pointer
```

**Семантика:**
- `?*unsafe ()` = `Option[*unsafe ()]` — nullable, unsafe deref
- `()` (unit type) — zero-sized; pointer "ничего не указывает" — opaque
- `type ptr ...` — **newtype** (D52), distinct от `?*unsafe ()` (требует
  explicit cast)

**Migration:**
- Existing `ptr` usages в Plan 115/91.12 continue работать (type-level
  alias-like behavior)
- `type Sqlite3Handle ptr` (newtype через `ptr`) — works как раньше
- `external fn` returning `ptr` — works (ABI = `void*` = `?*unsafe ()`)
- **Tuple-by-value returns** `(Handle, i64)` — unchanged from Plan 115

### 12. Casts

```nova
// Safe casts (any context):
ro x = p as usize               // ✓ leaks address для logging/hashmap keys
p1 == p2                         // ✓ identity check

// Unsafe casts (require unsafe block):
unsafe {
    ro p = addr as *u8           // reverse cast int → pointer (memory-mapped I/O)
    ro p2 = p as *mut T          // ro → mut downgrade
    ro p3 = p as *unsafe T       // T → unsafe T (or vice versa)
    p1 < p2                      // cross-allocation ordering (UB unless same alloc)
}

// Implicit casts (compile-time auto):
*ro T → *T                       // identity (since *T == *ro T by default)
```

**Cast table:**

| From | To | Safe? | Notes |
|---|---|---|---|
| `*T` (= `*ro T`) | `usize` | ✓ | identity / debug |
| `usize` | `*T` | unsafe | reverse cast — memory-mapped I/O |
| `*ro T` | `*mut T` | unsafe | mutability upgrade |
| `*mut T` | `*ro T` | ✓ | downgrade (safe) |
| `*T` | `*unsafe T` | ✓ | downgrade alignment guarantees |
| `*unsafe T` | `*T` | unsafe | reclaim alignment (user obligation) |
| `*T1` | `*T2` (T1≠T2) | unsafe | type punning |

### 13. Comparison

```nova
// Safe ops (any context):
p1 == p2                         // ✓ identity check (robust к GC move)
p1 != p2                         // ✓
p == None                        // ✓ для Option[*T] via NPO

// Unsafe ops:
unsafe {
    p1 < p2                      // unsafe — cross-allocation UB
    p1 > p2                      // unsafe
    p1 <= p2                     // unsafe
    p1 >= p2                     // unsafe
}
```

**Rationale `<`/`>` unsafe:**
1. **Cross-allocation UB** (C/Rust): pointer ordering между разными аллокациями
   undefined.
2. **Moving GC**: наш GC может перемещать объекты (compaction); адреса
   меняются между comparisons.
3. **Same-allocation valid**: для loops по buffer ordering нужен — пользователь
   гарантирует same-allocation внутри `unsafe { }`.

### 14. `&record.field` only в unsafe

```nova
ro acc = Account { name: "Piter", age: 30 }

// Safe чтение field — OK:
ro x = acc.age                   // ✓ просто чтение, не pointer creation

// Pointer creation на field — unsafe:
unsafe {
    ro p_age = &acc.age          // ✓ *ro i64; GC compaction concern
    ro p_name = &acc.name        // ✓ *ro str
}
```

**Concern:** moving GC может двинуть `acc` → field address меняется. Pointer
становится dangling в любой момент GC trigger. Inside `unsafe { }` —
пользователь обещает no GC trigger between взятие и use.

### 15. Forbidden ops (даже в unsafe)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ ERROR E_ARRAY_INDEX_PTR_BANNED
    //                              arrays могут resize / GC move — pointer dangle
}

// `null` literal — НЕТ:
ro p ?*u8 = null                 // ❌ ERROR; use None
ro p ?*u8 = None                 // ✓ NPO emits NULL

// `undefined` — НЕТ:
mut p *u8 = undefined            // ❌ ERROR; use ?*u8 = None + init pattern
mut p ?*u8 = None                // ✓ then init: external_alloc(&mut p) where p stays ?*u8
```

**Rationale:**
- `&arr[i]` — array buffer может перемещаться (`.push` causes realloc; GC
  compaction). Безопасно нельзя; для FFI используется `arr.as_ptr_unsafe()`
  pattern (deferred Q-block).
- `null` literal — duplication с `None` (one-way-to-do); enforced via parser.
- `undefined` — uninitialized state дополнительная mental complexity; pattern
  `?*T = None + init` достаточен для FFI out-params (deferred dedicated
  `out` syntax — Q-block).

---

## Грамматика

Plan 118 — **language addition** (parser/checker/codegen). Изменения:

```ebnf
PointerType   ::= '*' PointerModifier? Type
PointerModifier ::= 'ro' | 'mut' | 'unsafe'

UnsafeBlock   ::= 'unsafe' '{' Statements '}'

AttributeUnsafe ::= '#unsafe'                    // на fn declarations

AddrOfExpr    ::= '&' Expr                       // pointer creation
DerefExpr     ::= '*' Expr                       // explicit deref

FnPointerType ::= '*fn' '(' TypeList ')' ('->' Type)?
```

**Backward compatibility:** new tokens (`*ro`, `*mut`, `*unsafe`, `unsafe`,
`#unsafe`, `&`) — не conflict'ят с existing syntax (Nova не использовал `*`
как prefix operator до Plan 118; `&` не был prefix operator).

---

## Фазы

### Ф.0 — GATE: design freeze + D216/D2/D214 amend drafts + worktree (~½ dev-day)

> **Critical decision point:** Plan 118 — major language addition. Все
> 15 sections дизайна — frozen. Изменения после Ф.0 → новый sub-plan.

- **Ф.0.1** Worktree `nova-p118` создать через standard hook; register
  немедленно первой Bash командой.
- **Ф.0.2** Draft D216 в `spec/decisions/02-types.md` (после D52 type forms).
- **Ф.0.3** Draft D2 amend в `spec/decisions/04-effects.md` («D2 historical:
  no unsafe keyword; Plan 118 restores `unsafe { }` as effect-handler sugar»).
- **Ф.0.4** Draft D214 amend в `spec/decisions/02-types.md`
  («`ptr` redefined as newtype над `?*unsafe ()`»).
- **Ф.0.5** Audit existing pointer-related code:
  - `ptr` usages в Plan 115/91.12 (migrate compat check)
  - `nova_rt/*.h` C-side pointer types (FFI ABI verification)
  - Existing `external fn` signatures (compat verification)
- **Ф.0.6** Acceptance A1-A15 финализированы.
- **Ф.0.7** Commit `feat(plan118 Ф.0): GATE — design freeze + D216/D2/D214
  amend drafts`.

### Ф.1 — `*T` family parser/checker + ptr redefine (~1.5 dev-day)

> **Safety hatch:** если parser disambiguation `*T` vs multiplication
> оказывается non-trivial (контекстная грамматика), extract в Plan 118.1
> «*T parser foundations». Decision point: конец Ф.1.2.

- **Ф.1.1** Parser: tokenize `*ro` / `*mut` / `*unsafe` / `*` prefixes
  для types. Disambiguation от `a * b` multiplication через context (type
  position vs expression position).
- **Ф.1.2** Parser: chain `*mut *ro T` — recursive PointerType production.
- **Ф.1.3** Parser: `*fn(Args) -> Ret` function pointer type.
- **Ф.1.4** Type-checker: register `*T` family as distinct primitive types;
  default `*T` ≡ `*ro T`.
- **Ф.1.5** Type-checker: binding-mut rule (`mut p *T` → `*mut T` default).
- **Ф.1.6** Type-checker: `*T` valid в parameter, return, field, generic
  positions.
- **Ф.1.7** Codegen: emit `T*` C type для `*T`; cross-platform ABI verification.
- **Ф.1.8** `ptr` redefine: `type ptr ?*unsafe ()` newtype; existing
  `ptr` usages compat verification.
- **Ф.1.9** Tests T1 series.
- **Ф.1.10** Commit `feat(plan118 Ф.1): *T family parser/checker + ptr redefine`.

### Ф.2 — `&value` operator + escape analysis с auto-promote (~1.5 dev-day)

> **Safety hatch:** escape analysis edge cases могут require more time
> (closure capture, indirect escape through field stores). Если > 1.5 day,
> extract escape-edge-cases в Plan 118.2.

- **Ф.2.1** Parser: `&expr` prefix operator (pointer creation).
- **Ф.2.2** Type-checker: `&value` type inference (`*ro T` или `*mut T`
  по контексту binding).
- **Ф.2.3** Escape analysis:
  - Collect `&local_var` usages
  - Determine escape: return / closure-capture / store в heap field / fn arg
  - Mark escaped locals для promote
- **Ф.2.4** Codegen: для promoted locals — heap allocation вместо stack
  slot; emit `nova_alloc` calls.
- **Ф.2.5** Tests T2 series (positive + negative escape patterns).
- **Ф.2.6** Commit `feat(plan118 Ф.2): &value operator + escape/auto-promote`.

### Ф.3 — `unsafe { }` block + `#unsafe` attribute (D2 amend) (~1 dev-day)

- **Ф.3.1** Parser: `unsafe { ... }` block syntax.
- **Ф.3.2** Parser: `#unsafe` attribute on fn declarations.
- **Ф.3.3** Type-checker: unsafe-context tracking — inside `unsafe { }` block
  OR inside `#unsafe` fn body.
- **Ф.3.4** Implementation as effect-handler sugar:
  - Built-in `UnsafeOps` effect (compiler-known, не user-declared)
  - `unsafe { ... }` desugars → `with unsafe_handler { ... }`
  - `unsafe_handler` — compiler-generated, не emit'ится в Nova code
- **Ф.3.5** Error checks:
  - `E_UNSAFE_REQUIRED` — pointer op вне unsafe context
  - `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без `unsafe { }`
  - Diagnostic suggestions с hint syntax
- **Ф.3.6** D2 amend committed в spec.
- **Ф.3.7** Tests T3 series.
- **Ф.3.8** Commit `feat(plan118 Ф.3): unsafe block + #unsafe attribute + D2 amend`.

### Ф.4 — Auto-deref + pointer ops (arith/casts/compare) (~1 dev-day)

- **Ф.4.1** Type-checker: `*p` explicit deref (one level); `p.field` auto-deref
  one level. Inside unsafe context only.
- **Ф.4.2** Type-checker: pointer arithmetic `+`/`-`/`+=`/`-=` only inside
  `unsafe { }`, result type `*unsafe T`.
- **Ф.4.3** Type-checker: cast rules table (см. §«Casts»).
- **Ф.4.4** Type-checker: comparison rules (`==`/`!=` safe; `<`/`>` unsafe).
- **Ф.4.5** Type-checker: `&record.field` only в unsafe context.
- **Ф.4.6** Type-checker: `&arr[i]` всегда forbidden (`E_ARRAY_INDEX_PTR_BANNED`).
- **Ф.4.7** Type-checker: `null` literal forbidden (`E_NULL_LITERAL_USE_NONE`);
  `undefined` forbidden (`E_UNDEFINED_USE_NONE_INIT_PATTERN`).
- **Ф.4.8** Codegen: emit `*p`, `p->field`, `p + n` (sizeof-scaled), cast ops.
- **Ф.4.9** Tests T4 series.
- **Ф.4.10** Commit `feat(plan118 Ф.4): auto-deref + pointer ops`.

### Ф.5 — `Option[*T]` + NPO codegen (~1 dev-day)

- **Ф.5.1** Codegen lowering: detect `Option[*T]` type signature; emit
  **single pointer** layout (8 bytes), не tagged struct.
- **Ф.5.2** Codegen pattern match: `if (ptr == NULL) None_branch else
  Some_branch(ptr)` — заменяет tag-check.
- **Ф.5.3** Codegen construction: `Some(p)` → `p`; `None` для `Option[*T]`
  → `NULL`.
- **Ф.5.4** ABI verification: `external fn malloc(sz usize) -> Option[*u8]`
  ABI = `uint8_t* malloc(size_t)` — direct C-FFI compatible.
- **Ф.5.5** Generic interaction: `Map[K, Option[*T]]` — NPO applies inside.
- **Ф.5.6** Tests T5 series.
- **Ф.5.7** Commit `feat(plan118 Ф.5): Option[*T] + NPO codegen`.

### Ф.6 — Function pointers `*fn(...)` для FFI (~½ dev-day)

- **Ф.6.1** Type-checker: `*fn(Args) -> Ret` distinct type from `fn(Args) -> Ret`
  closure.
- **Ф.6.2** Cast `fn → *fn` — compile-time check captureless (нет closure env);
  иначе `E_CLOSURE_HAS_ENV`.
- **Ф.6.3** Codegen: `*fn(...)` emit as C function pointer
  (`Ret (*name)(Args)`).
- **Ф.6.4** Calling convention: C ABI текущей платформы (System V на Unix,
  MS x64 на Windows). No `extern "C"` keyword (single ABI supported).
- **Ф.6.5** FFI callback тест — register Nova fn как callback в external C
  function, verify invocation.
- **Ф.6.6** Tests T6 series.
- **Ф.6.7** Commit `feat(plan118 Ф.6): *fn(...) function pointers`.

### Ф.7 — Regression + cross-platform validation (~1 dev-day)

- **Ф.7.1** Full `nova test` ≥ baseline (post-Plan 115/116/120 baseline).
- **Ф.7.2** Cross-platform CI matrix:
  - Linux × clang
  - Linux × gcc
  - Windows × MSVC
  - Windows × clang
  - macOS × clang
- **Ф.7.3** ABI verification fixtures — typed pointer tests на каждой
  platform identical results.
- **Ф.7.4** Performance baseline:
  - Microbenchmark `&value + auto-promote` overhead vs stack-only baseline
  - NPO `Option[*T]` size verification (sizeof == sizeof(*T))
- **Ф.7.5** Tests T7 series (regression + perf + ABI).
- **Ф.7.6** Commit `feat(plan118 Ф.7): regression + cross-platform`.

### Ф.8 — Spec + docs + close (~½ dev-day)

- **Ф.8.1** Promote D216 / D2 amend / D214 amend → active в spec/decisions/.
- **Ф.8.2** Cross-ref:
  - D52 — `*T` family integration с type forms
  - D32 — value/reference allocation + escape/auto-promote
  - D215 (Plan 120) — tuple stack values + & escape semantics
  - D172 (Plan 113) — `#realtime` interaction (pointer ops не считается
    `#realtime`-allowed, deref может GC trigger)
- **Ф.8.3** `nova doc` regen — typed pointer family documentation page.
- **Ф.8.4** `docs/ffi-cookbook.md` update — typed buffer examples:
  - libpng image data copy (`*ro u8 src`, `*mut u8 dst`, length)
  - libcurl header callback (`*fn(...)` registration)
  - sqlite blob column read (`*ro u8 + len`)
- **Ф.8.5** `examples/typed_pointers/` — minimal working samples.
- **Ф.8.6** `docs/project-creation.txt` — sprint section.
- **Ф.8.7** `docs/simplifications.md` — close `[M-118-*]` markers.
- **Ф.8.8** `nova-private/discussion-log.md` — design decisions log.
- **Ф.8.9** Memory `project-plan118-status.md`.
- **Ф.8.10** Status section в этом plan-файле.
- **Ф.8.11** Final merge в `main` через PR review.
- **Ф.8.12** Commit `feat(plan118 Ф.8): spec + docs + close`.

---

## D-block changes

### D216 (NEW) — Typed pointer family + unsafe model + NPO

**Локация:** `spec/decisions/02-types.md` (после D52 type forms).

**Что.** Foundational language addition:

1. **`*T` family типов** — typed pointers:
   - `*T` (= `*ro T`) — readonly typed pointer (default)
   - `*ro T` / `*mut T` — explicit mutability
   - `*unsafe T` — pointer после арифметики (alignment/bounds gone)
   - Size: pointer-width; ABI: `T*` в C

2. **Binding mut rule:** `mut p *T` ≡ `mut p *mut T` (pointer mut по default
   при mut binding). Explicit `ro p *mut T` valid (edge case).

3. **Chain order:** modifier перед `*` относится к этому pointer'у; read
   left-to-right (`*mut *ro T` = mut pointer на ro pointer на T).

4. **`&value` operator + escape analysis с auto-promote:**
   - `&value` creates `*ro T` or `*mut T` (по контексту binding)
   - Stack values (primitives, tuples) auto-promoted в heap если pointer
     escapes scope (return / closure / heap-field store)
   - Records (heap references) — `&record` creates pointer на reference
   - GC-friendly семантика (vs Rust lifetimes — у нас GC + auto-promote)

5. **Auto-deref:**
   - `*p` explicit deref (one level)
   - `p.field` auto-deref one level (Go-style)
   - Multi-level pointers require explicit `(*p).field` chain
   - Только в unsafe context

6. **Pointer arithmetic:**
   - `+`/`-` only в `unsafe { }` block, result `*unsafe T`
   - Units: sizeof(T)-scaled
   - `*unsafe T` deref требует ещё один unsafe wrap

7. **Null safety:** `*T` non-null; `Option[*T]` nullable через **NPO codegen**:
   - Layout: single pointer (8 bytes), не tagged struct
   - Pattern match: NULL-check, не tag-check
   - Direct C-FFI compatible

8. **Comparison:** `==`/`!=` safe (identity); `<`/`>` unsafe (cross-allocation
   UB + moving GC concern).

9. **Casts:** см. cast table в §«Casts»; `p as usize` safe, `usize as *T`
   unsafe.

10. **Forbidden:** `null` literal (use `None`); `undefined` (use `?*T = None`);
    `&arr[i]` (array realloc/GC concern).

**Cross-ref:** D2 (amend — unsafe keyword restored), D52 (tuple newtype +
type forms), D32 (value/reference allocation), D215 (Plan 120 stack tuples),
D214 (Plan 115 ptr — amended to use D216 foundations).

### D2 AMEND — `unsafe { }` keyword restored as effect-handler sugar

**Локация:** `spec/decisions/04-effects.md` (D2 history block + amend).

**D2 v1 (historical):** keyword `unsafe` отменён в пользу effect mechanism
(вместе с `async`/`throws`).

**D2 v2 (Plan 118 amend):** keyword `unsafe { }` **restored** как **syntactic
sugar** для built-in effect handler. Под капотом:

```nova
unsafe { expr }
// ≡
with unsafe_handler { perform UnsafeOps.<op>(expr) }
```

**Rationale:**
- D2 spirit (всё — эффекты) **preserved** — `unsafe` is an effect handler
  internally
- User-facing syntax ergonomic (Rust-familiar `unsafe { }` block)
- `#unsafe` attribute на fn — analogous to handler-scoped function (caller
  must `unsafe { ... }` wrap call)
- **No effect propagation** up the call stack — `unsafe` block encapsulates
  (canonical Rust pattern)

**Affected ops** (require unsafe handler): pointer creation `&value`, deref
`*p`, auto-deref `p.field`, pointer arithmetic, `usize as *T` reverse cast,
`<`/`>` pointer ordering, `&record.field`, calling `#unsafe` fn, newtype
construction `Handle(some_ptr)`.

**Cross-ref:** D216 (typed pointer foundations using this model), D3 (effect
syntax — `unsafe_handler` follows convention), D61 (effect handlers).

### D214 AMEND — `ptr` redefinition (Plan 115)

**Локация:** `spec/decisions/02-types.md` (D214 history + amend).

**D214 v1 (Plan 115):** `ptr` — built-in primitive type, opaque pointer-sized
integer.

**D214 v2 (Plan 118 amend):** `ptr` is **newtype** над `?*unsafe ()`:

```nova
type ptr ?*unsafe ()
```

**Semantically equivalent в V1 use cases** (opaque handle pattern):
- `type Sqlite3Handle ptr` — works as before
- `external fn ... -> ptr` — ABI `void*` = `?*unsafe ()` ABI = same
- Tuple-by-value returns `(Handle, i64)` — unchanged

**Migration:** **none required** для existing code; `ptr` semantics expanded
(now formally nullable + unsafe), but use cases backward-compatible.

**Cross-ref:** D216 (`*T` family + `?` Option + `unsafe` modifier foundations),
D52 (newtype syntax).

---

## Tests

### T1 — `*T` family parser/checker

- **T1.1** Parse `*T` / `*ro T` / `*mut T` / `*unsafe T` в type positions.
- **T1.2** `*T` ≡ `*ro T` default rule.
- **T1.3** Chain `*mut *ro T` parses correctly; mutability levels distinct.
- **T1.4** Binding-mut rule: `mut p *T` infers `*mut T`; `ro p *mut T` valid.
- **T1.5** `*T` valid в fn param, return type, record field, generic.
- **NEG-T1.6** `*T` outside type position → parse error.
- **T1.7** `ptr` newtype works (existing code compat).
- **T1.8** Codegen: `*T` → C `T*` correct ABI.

### T2 — `&value` + escape/auto-promote

- **T2.1** `&local_primitive` — promotion triggered if returned.
- **T2.2** `&local_tuple` (Plan 120 named tuple) — promotion triggered if
  stored in heap field.
- **T2.3** `&record` (heap reference) — pointer на reference, no promotion
  (record уже в heap).
- **T2.4** `&local` used только в текущем scope (no escape) — NO promotion,
  stack-local pointer.
- **T2.5** Closure capture: `|| { ... &local ... }` — escape if closure
  outlives scope.
- **T2.6** Codegen: promoted locals — `nova_alloc` calls; non-promoted —
  stack slot pointer.

### T3 — `unsafe { }` block + `#unsafe` attribute

- **T3.1** `unsafe { *p }` — parses + type-checks inside fn.
- **T3.2** `#unsafe fn foo() { *p }` — `*p` ok без обёртки внутри fn body.
- **T3.3** `safe_fn() { ffi_wrapper(p) }` где `ffi_wrapper` #unsafe →
  `E_UNSAFE_CALL_REQUIRES_WRAP`.
- **T3.4** `safe_fn() { unsafe { ffi_wrapper(p) } }` — ok.
- **NEG-T3.5** `safe_fn() { *p }` без unsafe wrap → `E_UNSAFE_REQUIRED`.
- **T3.6** `unsafe { }` desugar verification (lowering to effect handler call).
- **T3.7** D2 amend test — spec mentions Plan 118 restoration.

### T4 — Auto-deref + pointer ops

- **T4.1** `unsafe { p.field }` auto-deref one level.
- **T4.2** `unsafe { *p }` explicit deref.
- **T4.3** `unsafe { p + 1 }` arith — result `*unsafe T`.
- **T4.4** `unsafe { unsafe { *(p + 1) } }` — nested unsafe для `*unsafe T`
  deref.
- **T4.5** `p as usize` safe outside unsafe.
- **NEG-T4.6** `usize as *T` outside unsafe → `E_UNSAFE_REQUIRED`.
- **T4.7** `p1 == p2` safe outside unsafe.
- **NEG-T4.8** `p1 < p2` outside unsafe → `E_UNSAFE_REQUIRED`.
- **NEG-T4.9** `&arr[i]` — `E_ARRAY_INDEX_PTR_BANNED`.
- **NEG-T4.10** `null` literal use → `E_NULL_LITERAL_USE_NONE`.
- **NEG-T4.11** `undefined` use → `E_UNDEFINED_USE_NONE_INIT_PATTERN`.

### T5 — `Option[*T]` + NPO codegen

- **T5.1** `Option[*T]` size == sizeof(*T) (single pointer; verified via
  `sizeof` Nova builtin or C interop).
- **T5.2** `Some(p)` codegen — emit `p` literally.
- **T5.3** `None` для `Option[*T]` — emit `NULL`.
- **T5.4** Pattern match codegen — NULL-check, не tag-check.
- **T5.5** `external fn malloc(sz usize) -> Option[*u8]` returns NULL → `None`
  match.
- **T5.6** Pattern match Some(p) — p is `*T` non-null guaranteed внутри branch.
- **T5.7** Generic interaction `Map[K, Option[*T]]` — NPO applies inside
  value position.

### T6 — Function pointers `*fn(...)`

- **T6.1** `*fn(i64) -> ()` type parses + accepts.
- **T6.2** Cast captureless `fn → *fn` ok.
- **NEG-T6.3** Cast closure-with-env `fn → *fn` → `E_CLOSURE_HAS_ENV`.
- **T6.4** External fn accepts `*fn(i64) -> ()` callback parameter.
- **T6.5** FFI invocation — Nova fn registered as C callback, invoked from
  C side, returns to Nova side.

### T7 — Regression + cross-platform + perf

- **T7.1** Full `nova test` ≥ baseline (post-Plan 115/116/120).
- **T7.2** Cross-platform PASS (Linux/Win/macOS × clang/MSVC/gcc).
- **T7.3** ABI verification: `*T` ABI identical к C `T*` на all platforms.
- **T7.4** `Option[*T]` NPO layout verification (perf microbenchmark — single
  pointer access vs two-field struct access).
- **T7.5** Escape/auto-promote overhead — sub-microsecond per call.
- **T7.6** No regressions в Plan 115 v1 (`ptr` opaque handle pattern works).

### T8 — Integration with adjacent plans

- **T8.1** Plan 115 (`ptr`) — `type Sqlite3Handle ptr` works post-D214 amend.
- **T8.2** Plan 116 (Tls) — `type RustlsSession ptr` continues работать
  (если уже использует).
- **T8.3** Plan 91.12 (TcpNet) — `type TcpListenerHandle ptr` works.
- **T8.4** Plan 120 (named tuples) — `&named_tuple` auto-promotion correct.
- **T8.5** Plan 113 (`#realtime`) — pointer ops в `unsafe { }` not allowed
  в `#realtime` context (consistency: deref может GC trigger).

### Regression

- **R1** Full `nova test` ≥ post-Plan 120 baseline.
- **R2** Cross-platform CI (5 platform/compiler combos).
- **R3** ABI snapshot verification (FFI fixtures stable).

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `*T` family типов (`*T`/`*ro T`/`*mut T`/`*unsafe T`) parses + type-checks | T1 series |
| A2 | Binding-mut rule: `mut p *T` → pointer mut по default | T1.4 |
| A3 | Chain order `*mut *ro T` correctly nested | T1.3 |
| A4 | `&value` operator + escape analysis с auto-promote | T2 series |
| A5 | Auto-deref `p.field` one level + explicit `*p` | T4.1 + T4.2 |
| A6 | Pointer arithmetic в `unsafe { }` → `*unsafe T` | T4.3 + T4.4 |
| A7 | `Option[*T]` + NPO codegen (single-pointer layout) | T5 series |
| A8 | `unsafe { }` block + `#unsafe` attribute (D2 amend) | T3 series + spec diff |
| A9 | `*fn(...)` function pointers для FFI | T6 series |
| A10 | Cast table enforced (safe vs unsafe casts) | T4.5-T4.6 |
| A11 | Comparison: `==`/`!=` safe, `<`/`>` unsafe | T4.7-T4.8 |
| A12 | Forbidden ops: `&arr[i]`, `null`, `undefined` | T4.9-T4.11 |
| A13 | `ptr` redefine как newtype над `?*unsafe ()` (D214 amend) | T1.7 + T8.1 |
| A14 | D216 + D2 amend + D214 amend promoted в active spec | spec diff |
| A15 | Cross-platform PASS (Linux/Win/macOS × clang/MSVC/gcc); full nova test ≥ baseline | R1 + R2 |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| R-1 | Parser disambiguation `*T` (pointer type) vs `a * b` (multiplication) | Context-sensitive: `*` в type position = pointer; `*` в expression position = multiplication. Tested via `*expr` (deref) vs `*Type` (pointer type) disambiguation through expression-vs-type position parsing. Standard pattern (Rust, Zig, C++) |
| R-2 | Escape analysis edge cases (closure capture, indirect via heap field stores, generic functions) | Safety hatch Ф.2 preamble: если edge cases > 1.5 day, extract в Plan 118.2. V1 conservative: PROMOTE если ANY uncertainty (over-promote OK для correctness; perf optimization позже) |
| R-3 | NPO codegen с generics (`Map[K, Option[*T]]` — NPO inside value position) | Type-checker mark NPO-eligible at monomorphization time; codegen generates specialized layout per generic instance. Tested T5.7 |
| R-4 | D2 amend — restoring removed keyword (политический риск) | D2 spirit preserved (effect handler sugar под капотом); user-facing syntax improvement. Spec amend explanatory. Не break'ит D2 mechanics, добавляет sugar layer |
| R-5 | Cross-platform ABI differences (Sys V vs MS x64 для pointer args/returns) | Test matrix all 5 combos на каждый PR; codegen использует C compiler ABI defaults (clang/MSVC/gcc handle correctly) |
| R-6 | Moving GC + pointer dangling (если GC двинет объект во время unsafe block) | Document clearly: unsafe block — пользователь обещает no GC trigger. Future: pin API (out-of-scope V1). Diagnostic `#[gc_pin]` attribute followup |
| R-7 | NPO + cross-FFI с C `Option<*T>` (e.g., `malloc` returning `void*` NULL = OOM) | Direct ABI compatible — `Option[*T]` layout = `T*`. FFI fixture проверяет round-trip |
| R-8 | `*fn(...)` cast от non-captureless closure (E_CLOSURE_HAS_ENV) — false positives | Compiler tracks closure environment statically; cast allowed только если closure body не reference outer vars. Conservative: reject borderline cases |
| R-9 | Existing `ptr` users break post-D214 amend (Plan 115/91.12 stdlib) | D214 amend backward-compatible (semantic equivalent); audit Ф.0.5 для all existing ptr usages; regression T8 series |
| R-10 | Plan 113 `#realtime` interaction — pointer ops не считаются realtime-safe | Type-checker explicit ban pointer ops в `#realtime` context (deref может GC trigger, allocate, etc.) |

---

## Out of scope (explicitly deferred — Q-block)

| Маркер | Что | Когда |
|---|---|---|
| `[M-118-lifetimes-rust-style]` | Rust-style lifetime parameters `<'a>` + borrow checker | **Permanently out** — у нас GC + auto-promote (отдельная design philosophy) |
| `[M-118-aliasing-xor-rules]` | Rust-style XOR aliasing для `*mut T` (exclusive references) | Не нужно с GC; future если perf optimization потребует |
| `[M-118-as-ptr-slice]` | `arr.as_ptr() -> *T` для arrays/slices в FFI | V2 — нужен scoped pin API или unsafe escape hatch; вне V1 |
| `[M-118-with-ptr-scoped]` | `arr.with_ptr(\|p\| ...)` scoped FFI pin | V2 — alternative к as_ptr |
| `[M-118-atomic-pointers]` | `AtomicPtr[T]` для lock-free | Sub-plan позже; coordinate с Plan 103 family |
| `[M-118-fixed-arrays]` | `*[N]T` fixed-size arrays для C FFI buffers | Plan 121 (separate language addition) |
| `[M-118-vararg-ffi]` | C-style vararg (`printf(fmt, ...)`) | Niche; wrappers via `args: [Any]` достаточны для V1 |
| `[M-118-volatile-rw]` | Volatile reads/writes для memory-mapped I/O | Niche (embedded); add when needed |
| `[M-118-offsetof]` | `offsetof(T, field)` для FFI struct layout matching | Niche; manual offsets adequate для now |
| `[M-118-strict-provenance]` | Rust new pointer model (provenance tracking) | Не required; consider если adopt Rust 2024-style |
| `[M-118-alignment-attribute]` | `@align(N)` для over-aligned pointers (SIMD) | Niche; add when SIMD plan |
| `[M-118-inline-assembly]` | Inline asm — intrinsics | Out of scope language entirely |
| `[M-118-cast-pointer-arith-fn]` | Cast `*fn → *T` или обратно | Niche; rare use case |
| `[M-118-undefined-pattern]` | Dedicated `undefined` / `MaybeUninit<T>` syntax | V2 если pattern `?*T = None + init` insufficient |
| `[M-118-out-params-syntax]` | Dedicated `out` parameter syntax (`external fn foo(out p *T)`) | V2 — FFI ergonomics improvement |
| `[M-118-pin-api]` | `Pin[T]` API для self-referential / GC-stable references | Future — interacts с async + GC |
| `[M-118-stdlib-pointer-helpers]` | std/ptr module — utility fns (`offset_from`, `read_volatile`, etc.) | Followup library plan |
| `[M-118-bindgen-tool]` | `nova bindgen` CLI auto-gen FFI bindings из C headers | Major tooling effort; separate plan |

---

## Migration impact

### Existing code (post-Plan 115 v1 + Plan 116 + Plan 91.12)

- **`ptr` usages** (e.g., `type Sqlite3Handle ptr`) — **no migration required**.
  D214 amend backward-compatible (semantic equivalent через `?*unsafe ()`).
- **`external fn` signatures** с `ptr` parameters — no change (ABI unchanged).
- **Tuple-by-value FFI returns** `(Handle, i64)` — no change.

### New patterns enabled (post-Plan 118)

- Typed buffer FFI:
  ```nova
  external fn copy_buffer(src *ro u8, dst *mut u8, len usize) -> i64
  unsafe { copy_buffer(src_arr.unsafe_data_ptr(), dst_arr.unsafe_data_mut_ptr(), 1024) }
  ```
- Callback registration:
  ```nova
  external fn libuv_set_cb(cb *fn(i64) -> ()) -> i64
  unsafe { libuv_set_cb(my_handler as *fn(i64) -> ()) }
  ```
- Out-params via `?*T`:
  ```nova
  external fn try_init(out *u8) -> i64        // returns 0 on success, fills *out
  mut buf ?*u8 = None
  unsafe { try_init(&mut buf) }
  ```

### Compiler error renames / additions

- `E_UNSAFE_REQUIRED` — new error code
- `E_UNSAFE_CALL_REQUIRES_WRAP` — new
- `E_ARRAY_INDEX_PTR_BANNED` — new
- `E_NULL_LITERAL_USE_NONE` — new
- `E_UNDEFINED_USE_NONE_INIT_PATTERN` — new
- `E_CLOSURE_HAS_ENV` — new (for `*fn` cast)
- `W_REALTIME_POINTER_OP` — new warning (pointer op в `#realtime` context)

---

## Rollback strategy

1. **Revert PR** atomic per phase (Ф.0..Ф.8 separate commits).
2. **Spec D-blocks** — D216 / D2 amend / D214 amend reverted as part of PR.
3. **Compatibility**: rollback не break'ит existing Plan 115/116/91.12 code
   (no Plan 118 features used by них in any released state).
4. **Cross-platform CI** rollback smoke за ~1 hour.

---

## Cross-references

### Связь с уже-закрытыми planам

- **Plan 114** ✅ (D184 master) — `ro`/`mut`/`consume` keywords; Plan 118 в этом
  синтаксисе. Binding-mut rule (`mut p *T` → `*mut T` default) extends Plan
  114 mutability story.
- **Plan 114.4** ⏳ planned (D199/D200) — const fn + associated constants
  (extracted Plan 114 Ф.9-Ф.11). Orthogonal к Plan 118, cross-ref только для
  D-block coordination.
- **Plan 113** ✅ (D172) — `#realtime` attribute; Plan 118 adds `W_REALTIME_POINTER_OP`
  warning для pointer ops в realtime context (deref может GC trigger).
- **D2** ([04-effects.md#d2](../../spec/decisions/04-effects.md#d2)) —
  effects вместо keywords; **AMEND** to restore `unsafe { }` как effect-handler
  sugar.
- **D52** ([02-types.md#d52](../../spec/decisions/02-types.md#d52)) — type
  declarations (newtype + tuple forms); Plan 118 — pointer types are new
  primitives integrating с D52 framework.
- **D32** ([05-memory.md#d32](../../spec/decisions/05-memory.md#d32)) —
  value/reference allocation; Plan 118 amend Plan 120 (D215) — stack values
  + & escape rules.

### Связь с planned / parallel planами

- **Plan 115 v1** ✅ planned (D214) — `ptr` + tuple FFI + opaque handles.
  Plan 118 amend D214 — redefine `ptr` як `type ptr ?*unsafe ()` (newtype
  через `*T` family foundations). Backward-compatible.
- **Plan 116** (std/tls) — uses `ptr` для rustls sessions (continues working
  post-D214 amend).
- **Plan 91.12** (std/net) — uses `ptr` через D126 + Pattern B (Plan 115).
  No changes required.
- **Plan 120** (named tuples) — stack tuples auto-promote при `&` escape
  (Plan 118 ⇄ Plan 120 interaction).
- **Plan 121** (fixed arrays — future) — будет building на Plan 118 `*T`
  family для `*[N]T` typed fixed-size pointer.

### Spec D-blocks

- **D2** ([04-effects.md](../../spec/decisions/04-effects.md)) — effects
  foundation; **AMEND** в Plan 118.
- **D32** ([05-memory.md](../../spec/decisions/05-memory.md)) — value/reference
  allocation.
- **D52** ([02-types.md](../../spec/decisions/02-types.md)) — type declarations.
- **D172** ([06-concurrency.md](../../spec/decisions/06-concurrency.md))
  — `#realtime` attribute (cross-ref).
- **D184** ([03-syntax.md](../../spec/decisions/03-syntax.md))
  — Plan 114 master keyword refresh.
- **D199/D200** ([03-syntax.md](../../spec/decisions/03-syntax.md) / [02-types.md](../../spec/decisions/02-types.md))
  — Plan 114.4 const fn + assoc const (planned, orthogonal).
- **D214** ([02-types.md](../../spec/decisions/02-types.md))
  — Plan 115 `ptr` type; **AMEND** в Plan 118.
- **D215** ([02-types.md](../../spec/decisions/02-types.md))
  — Plan 120 named tuples + allocation contract.
- **D216** (NEW, [02-types.md](../../spec/decisions/02-types.md))
  — Typed pointer family + unsafe model + NPO.

---

## Status — closure summary

> Заполняется агентом по завершении Plan 118. Поля:
> - Что сделано (per phase Ф.0..Ф.8 с commit refs)
> - Что extracted в Plan 118.1/118.2 (если safety hatches fire'нули)
> - Final `nova test` results + cross-platform PASS matrix
> - ABI verification snapshot results
> - NPO size verification (sizeof(Option[*T]) == sizeof(*T) на каждой platform)
> - Performance baseline (escape/promote overhead microbench)
> - Ссылки на commits
> - Memory `project-plan118-status.md` создан
> - `docs/project-creation.txt` sprint section updated
> - `docs/simplifications.md` updated с закрытыми/open `[M-118-*]` markers
> - `nova-private/discussion-log.md` updated с design decisions
> - D216 + D2 amend + D214 amend promoted в active spec (commit refs)
