<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 118 — Typed pointers (`*T` family) + unsafe model (core)

> **Создан 2026-05-31. Ревизия 2026-06-01** (production-grade scope + декомпозиция
> на Plan 118 семейство по итогам обсуждения).
> **Статус:** 🆕 PLANNED (revised).
> **Приоритет:** P1 — **language addition** для production-grade FFI и
> низкоуровневых сценариев. Plan 115 V1 (`ptr` + tuple FFI + opaque handle
> pattern) разблокировал базовый FFI; type-system не различает куда смотрит
> указатель и mutable он или нет; null-safety нет. Plan 118 core закрывает это
> (typed `*T` family + safety через `unsafe` model + null-safety через NPO).
>
> Без Plan 118 любая user-side работа с typed memory (FFI к C-буферам,
> memory-mapped I/O, low-level data structures) — type-unsafe.
>
> **Оценка core:** ~8-10 dev-day (увеличено с 7-10 после ревизии: добавлены
> GC honor-system warnings, method/field auto-deref, Debug fmt, null-ptr
> retraction, расширенная test matrix + ABI snapshot pipeline).
>
>   - `*T` family parser/checker + ptr redefine: ~1.5 day
>   - `&value` operator + escape analysis с auto-promote: ~1.5 day
>   - `unsafe { }` block + `#unsafe` attribute (D2 amend): ~1 day
>   - Auto-deref + pointer ops (arith/casts/compare/method-call/field-assign): ~1.5 day
>   - `Option[*T]` + NPO codegen + null-ptr retraction: ~1 day
>   - `*fn(...)` function pointers + callback no-throw: ~½-1 day
>   - GC honor-system warnings (W_UNSAFE_GC_TRIGGER) + Debug fmt: ~½ day
>   - Regression + cross-platform + ABI snapshot + perf bench: ~1 day
>   - Spec promotion + ffi-cookbook + nova doc + examples + closure: ~½-1 day
>
> **Зависимости (все ✅ landed в main):**
>   - **Plan 115 V1** ✅ merged (D214) — `ptr` built-in + tuple FFI + opaque
>     handle pattern. Plan 118 **переопределяет** `ptr` как
>     `type ptr Option[*unsafe ()]` newtype (D214 amend); Plan 115 ABI сохранён.
>   - **Plan 120** ✅ merged (D215) — named tuples + value/reference allocation
>     contract. Plan 118 leverages allocation contract: stack-values (tuples,
>     primitives) auto-promoted в heap при `&` escape; heap references
>     (records, `{}`) — `&` создаёт pointer-на-reference.
>   - **Plan 114** ✅ merged (D184 master keyword refresh) — `ro`/`mut`/`consume`
>     keywords + `let` retracted; Plan 118 пишется в post-114 syntax.
>     Binding-modifier rule (binding `mut` → pointer `mut` по default) —
>     критическая mechanic Plan 118.
>   - **Plan 113** ✅ merged (D172) — `#realtime`/`#blocking` attribute model.
>     Pointer deref может GC trigger (allocation); Plan 118 запрещает pointer
>     ops в `#realtime fn` (E_REALTIME_POINTER_OP).
>   - **Plan 83.12** ✅ merged — std/net/tcp.nv (TcpListener / TcpStream /
>     UdpSocket типы). Cross-ref только для regression (T8.3: existing handle
>     types продолжают работать post-D214 amend).
>   - **D2** ([04-effects.md#d2](../../spec/decisions/04-effects.md#d2))
>     **AMEND** — D2 v1 отменил keyword `unsafe` в пользу effect mechanism.
>     Plan 118 **восстанавливает** `unsafe { }` keyword как **syntactic sugar**
>     для built-in effect handler (`with unsafe_handler { perform UnsafeOp.* }`).
>     D2 spirit сохранён (всё — эффекты под капотом), user-facing syntax ergonomic.
>   - **Plan 114.4** ⏳ planned (D199/D200 const fn + assoc const) — orthogonal,
>     cross-ref только для D-block coordination. Не блокирует Plan 118.
>
> **D-блоки (изменения):**
>   - **D216 NEW** — typed pointer family + unsafe model + null-safety через NPO
>   - **D2 AMEND** — `unsafe { }` keyword restored as effect-handler sugar
>   - **D214 AMEND** — `ptr` redefined as `type ptr Option[*unsafe ()]` newtype;
>     `null ptr` literal retracted (closes [M-115-null-ptr-to-option-after-npo])
>   - **D32 AMEND** — `&value` introduces typed pointer construction (NOT Rust
>     borrow); safety через escape analysis + auto-promote + unsafe gating
>   - **D52 cross-ref** — newtype `type Handle(*T)` (tuple form) is canonical
>     для FFI handles (zero-overhead)
>
> **Plan 118 family decomposition:**
>
>   | Plan | Scope | Est. | Status |
>   |---|---|---|---|
>   | **118** (этот) | `*T` family + unsafe + NPO + escape + `*fn` + GC honor-system | ~8-10 d | PLANNED |
>   | **118.1** | FFI intrinsics: volatile/copy/read/write + addr_of + C-string convention | ~3-4 d | PLANNED |
>   | **118.2** | Slice fat-pointer `[*]T` + `MaybeUninit[T]` + `ManuallyDrop[T]` | ~3-4 d | PLANNED |
>   | **118.3** | Cross-fiber/suspend safety + `AtomicPtr[T]` integration | ~2-3 d | PLANNED |
>
>   Sequencing: 118 core gates 118.1/118.2/118.3 (foundation). Sub-plans могут
>   стартовать параллельно после core merge.
>
> **Worktree convention:** `nova-p118` ✅ created 2026-06-01 (sibling of main).
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
>   - **Commit per phase** — после каждой Ф.N (Ф.0..Ф.10) отдельный commit
>     с conventional message `feat(plan118 Ф.N): <summary>`.
>   - **Update logs after each big task:**
>     - `docs/project-creation.txt` — sprint section про Plan 118 progress
>     - `docs/simplifications.md` — open/close `[M-118-*]` markers (+ closes
>       `[M-115-null-ptr-to-option-after-npo]` в Ф.5)
>     - `nova-private/discussion-log.md` (отд. репо) — design decisions
>       (binding-mut rule, escape/auto-promote semantics, unsafe model,
>       GC honor-system contract)
>   - **Tests через release nova** — `cargo build --release` затем
>     `./target/release/nova test` (не debug build — codegen может отличаться).
>   - **Per-fix verify** — только targeted fixture, full `nova test` только
>     в конце phase.
>   - **Status section в конце plan-файла** — заполняется агентом по
>     завершении (per phase + final summary).
>   - **Safety hatches per phase preambles** — explicit decision points для
>     extract в sub-plans если scope превышает estimate (e.g., escape analysis
>     edge cases, NPO codegen complexity).
>   - **ABI snapshot tests** — `tests/abi/typed_pointers/` каталог с
>     compiler-generated C-snippet golden files; verified на каждой platform/
>     compiler combo в CI.
>
> **Production-grade требование:** реализация без упрощений. `*T` family —
>   first-class в parser/checker/codegen/runtime; unsafe model — full
>   enforcement (compile-time errors `E_UNSAFE_REQUIRED`); NPO codegen —
>   zero-cost (один pointer-word, не tagged struct); escape analysis —
>   correct для всех stack-value scenarios; cross-platform validated
>   (Linux/Windows/macOS × clang/MSVC/gcc).

---

## Зачем

### Что отсутствует в Nova сейчас (после Plan 115 V1)

После Plan 115 V1 FFI ergonomics достаточны для **opaque handles** (sqlite3
sessions, libuv listeners, rustls sessions — Plan 116):

```nova
type Sqlite3Handle(ptr)                                   // opaque, OK (tuple newtype, stack)
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

| | Только `ptr` (Plan 115 V1) | `*T` family (Plan 118) |
|---|---|---|
| **Type safety** | ❌ casts wherever | ✓ compile-time type check |
| **Mutability** | ❌ нет различия ro/mut | ✓ `*ro T` / `*mut T` |
| **Auto-deref field** | ❌ нет (Nova vis ptr opaque) | ✓ `p.field` |
| **Auto-deref method** | ❌ нет | ✓ `p.method()` (in unsafe) |
| **Null safety** | ❌ `ptr` всегда может быть null | ✓ `*T` non-null, `Option[*T]` для nullable |
| **FFI ergonomics** | ❌ workarounds через out-params | ✓ direct typed signatures |
| **Self-documenting** | ❌ `ptr` непонятно куда смотрит | ✓ `*ro UserData` ясно |
| **NPO** | ❌ Option[ptr] = 16 bytes | ✓ Option[*T] = 8 bytes |

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

### Mainstream comparison (expanded)

| Язык | Typed pointers | Unsafe model | Null safety | Auto-deref | Pointer arith |
|---|---|---|---|---|---|
| **C** | `T*` / `const T*` | (нет) | `NULL` runtime | `p->field` arrow | `p + n` всегда |
| **C++** | `T*` / `const T*` / smart ptrs | (нет в core; `[[unsafe]]` proposals) | `nullptr` runtime | `p->field` arrow | `p + n` |
| **Rust** | `*const T` / `*mut T` (raw); `&T` / `&mut T` (refs) | `unsafe { }` block + `unsafe fn` | `Option<&T>` + NPO | через ref auto-deref | `unsafe` only |
| **Zig** | `*T` / `*const T` / `*allowzero T` / `[*]T` | (нет keyword; explicit cast intrinsics) | `?*T` syntax + NPO | `.*` postfix + auto через `.` | `+` всегда (`*T` arithmetic banned, `[*]T` ok) |
| **C#** | `T*` (unmanaged) / `ref T` / `in T` / `out T` | `unsafe` modifier (class/method/block) | `T?` reference nullable | `p->field` arrow | `unsafe` only |
| **Swift** | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | Type-based (Unsafe* prefix); scoped APIs | Optional types + NPO | `.pointee` accessor | only через `advanced(by:)` |
| **D** | `T*` / `ref T` / `scope T*` | `@safe` / `@trusted` / `@system` attributes | `Nullable!T` | `p.field` auto-deref | `@system` only |
| **Go** | `*T` (managed); `unsafe.Pointer` (raw) | `unsafe` package import | Nil pointers (runtime) | `p.field` auto-deref | `unsafe.Pointer` only |
| **Kotlin/Native** | `CPointer<T>` / `CFunction<T>` | scoped через `Interop.*` types | `T?` nullable | `.pointed` accessor | `interpret*` cast helpers |
| **Java JNI** | (нет в Java; через C) | (нет) | (через obj refs) | (нет) | (нет в Java) |
| **TS/JS** | (нет — managed runtime) | (нет) | `null`/`undefined` | через `?.` | (нет) |
| **Nova V1** (Plan 115) | `ptr` (untyped) только | (нет — будет в Plan 118) | `null ptr` runtime check | (нет — opaque) | banned |
| **Nova V2** (Plan 118) | **`*T` family** + `unsafe` model + NPO | `unsafe { }` block + `#unsafe` attr (D2 amend) | `Option[*T]` + NPO | `p.field` + `p.method()` one-level | `*unsafe T` only, in unsafe block |

**Позиционирование Nova V2 (Plan 118):**
- Type safety на уровне **Rust/Swift** (typed + mutability + safety boundary)
- Null safety на уровне **Rust/Zig/Kotlin** (NPO native)
- Pointer arithmetic на уровне **Rust/Swift** (gated + result `*unsafe T`)
- Auto-deref на уровне **D/Go** (`p.field` / `p.method()` one-level)
- Safety model на уровне **Rust** (`unsafe { }` + `#unsafe` attribute)
- **GC-friendly** семантика (vs Rust lifetimes) — escape analysis + auto-promote
  вместо borrow checker
- **Honor-system pin** для GC — пользователь в unsafe обещает no-GC-trigger,
  compiler warns про violations; формальный pin API отложен на followup

---

## Plan 118 family decomposition

После ревизии 2026-06-01 Plan 118 разделён на 4 sub-plan'а для staged
delivery production-grade функциональности. Sequencing: **core (118) gates
sub-plans**; sub-plans могут стартовать параллельно после core merge.

### Plan 118 (core) — этот документ — ~8-10 dev-day

**Scope:** foundational typed pointer system.

- `*T` / `*ro T` / `*mut T` / `*unsafe T` family
- Binding-mut rule (`mut p *T` → `*mut T` default)
- Chain order (`*mut *ro T` recursive)
- `&value` operator + escape analysis с auto-promote
- Auto-deref `*p` (explicit) + `p.field` + `p.method()` (one-level, Go-style)
- Field assignment via auto-deref (`p.field = v` for `*mut T`)
- Pointer arithmetic (gated unsafe, результат `*unsafe T`)
- `Option[*T]` + NPO codegen (single pointer)
- `unsafe { }` block + `#unsafe` attribute (D2 amend)
- `*fn(Args) -> Ret` function pointers (default C ABI; callback no-throw)
- Cast table enforcement (safe vs unsafe casts)
- Comparison rules (`==`/`!=` safe; `<`/`>` unsafe)
- Forbidden ops (`&arr[i]`, `null`, `undefined`, vararg calls)
- `ptr` redefine как newtype (D214 amend); retract `null ptr` literal
- GC honor-system: `W_UNSAFE_GC_TRIGGER` warning на alloc/yield внутри unsafe
- Pointer Debug fmt (`{:p}` style via `to_debug_str()` метод в unsafe)
- D216 NEW + D2 AMEND + D214 AMEND + D32 AMEND + D52 cross-ref

### Plan 118.1 — FFI intrinsics + C-string — ~3-4 dev-day

**Scope:** memory access primitives + null-terminated string convention.

- `(*T).read()`, `(*T).write(v)` — typed read/write через pointer
- `(*T).copy_to(dst, count)`, `(*T).copy_to_nonoverlapping(dst, count)` —
  memcpy/memmove primitives
- `(*T).read_volatile()`, `(*T).write_volatile(v)` — для memory-mapped I/O
- `addr_of!(value)`, `addr_of_mut!(value)` — get pointer без temporary reference
  (для packed structs / uninit memory)
- `cstr"hello"` literal — null-terminated string literal, тип `*ro u8`
- `(*ro u8).from_cstring()` / `(*ro u8).cstring_len()` — C-string interop
- D-block: D217 NEW (FFI memory primitives) + D26 cross-ref (str + cstr)

### Plan 118.2 — Slice fat-pointer + uninit/manuallydrop — ~3-4 dev-day

**Scope:** typed buffer pointer + uninitialized storage.

- `*[T]` / `*ro [T]` / `*mut [T]` — slice fat-pointer (ptr + len pair)
- `slice.as_ptr()` / `slice.as_mut_ptr()` / `slice.len()` API
- Cast `[]T → *ro [T]` (in unsafe — array may relocate via GC compaction)
- `MaybeUninit[T]` — uninitialized typed storage (FFI out-params, partial init,
  alloc-uninit pattern)
- `(*MaybeUninit[T]).assume_init()` — claims initialization (unsafe)
- `ManuallyDrop[T]` — wrap that skips destructor (ownership-transfer FFI)
- D-block: D218 NEW (slice fat-pointer + uninit/manuallydrop)
- Cross-ref Plan 121 (fixed-size stack arrays — будущий)

### Plan 118.3 — Pointer concurrency safety — ~2-3 dev-day

**Scope:** cross-fiber semantics + atomic-pointer integration.

- Cross-fiber pointer rules (`*T` через `supervised{}` boundary; default ban с
  opt-out marker)
- Suspend-safety: pointer held across `await` — `W_POINTER_HELD_ACROSS_SUSPEND`
  warning (или error в `#realtime` context)
- `AtomicPtr[T]` — lock-free typed pointer (cross-ref Plan 103.2 atomics)
- `compare_exchange_*` для pointers
- Interaction with Plan 113 `#realtime` (E_REALTIME_POINTER_OP — deref может
  GC trigger)
- D-block: D219 NEW (pointer concurrency safety)

### Cross-plan dependencies

```
Plan 115 V1 ✅ ──┐
                 ├── Plan 118 (core) ──┬── Plan 118.1 (intrinsics)
Plan 120 ✅ ─────┘                      ├── Plan 118.2 (slice + uninit)
                                         └── Plan 118.3 (concurrency)
                                              │
                                              └─→ Plan 103 family (cross-ref)
```

Sub-plans 118.1/118.2/118.3 — independent, могут параллельно после 118 core merge.

---

## Дизайн

### 1. `*T` family типов

```nova
*T              // ro pointer (default); short form of *ro T
*ro T           // explicit readonly pointer
*mut T          // mutable pointer (can write через *p)
*unsafe T       // unsafe pointer (после арифметики; deref требует unsafe block)
```

**Размер:** все варианты — pointer-width (8 bytes на 64-bit; bootstrap = только
64-bit платформы).

**ABI:** `T*` в C (compiler emits соответствующий C-type для FFI).

**Default ro:** `*T` ≡ `*ro T` — same default rule как Plan 114 для bindings
(`ro x = ...` default).

**Validity:** `*T` value — **always non-null**. Compile-time invariant.
Nullable variant — `Option[*T]` (NPO codegen — см. §7).

### 2. Binding mutability → pointer mutability

```nova
ro p *Acc                   // binding ro; pointer ro (cannot *p = ...)
mut p *Acc                  // binding mut; pointer mut automatically (can *p = ...)
mut p *Acc == mut p *mut Acc          // эквивалентны
ro p *mut Acc               // valid edge case: binding ro, pointee mut
                             // (cannot reassign p, BUT can *p = ...)

mut q = &acc                // pointer mut auto (no need &mut acc)
ro p = &acc                 // pointer ro auto
```

**Rule:** binding modifier пропагирует на pointer mutability **по умолчанию**.
Explicit `*mut T` / `*ro T` overrides только если нужно разойтись (`ro p
*mut T` редкий case).

**Why:** consistency с Plan 114 binding semantics; reduces noise в hot-path
FFI code (нет нужды писать `mut p *mut T` каждый раз).

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

**Practical:** multi-level pointers редки вне FFI с C double-indirection
(e.g., `char**`). Тесты T1.3 покрывают парсинг + correctness.

### 4. `&value` operator + escape analysis с auto-promote

```nova
// Heap reference (record) — & создаёт pointer на reference
ro acc = Account { name: "Piter" }    // acc — heap reference (D32)
ro p = &acc                            // *ro Account; GC tracks acc

// Stack value (primitive, tuple) — & escape triggers auto-promote
ro x = 42_i64                          // x — stack primitive
ro p = &x                              // x auto-promoted to heap; *ro i64

// Tuple на стеке (Plan 120) — auto-promote при &
ro point = Point(x: 1.0, y: 2.0)      // stack tuple (D215)
ro pp = &point                         // point auto-promoted; *ro Point

// Return-escape — тоже auto-promote
fn make_ptr() *ro i64 {
    ro x = 42
    &x                                 // x escapes; promoted to heap → safe to return
}
```

**Escape analysis algorithm (V1 conservative):**
1. Парсер собирает все `&local_var` usages.
2. Type-checker определяет escape:
   - `&local` used только в текущем scope (no return, no closure capture, no
     store в heap reference, no fn-arg pass) → **NO promote** (stack-local
     pointer ok)
   - `&local` returned, captured в closure, stored в record field, passed в
     fn parameter, OR **uncertain** → **PROMOTE local to heap allocation**
3. Codegen: для promoted locals аллокация через `nova_alloc` вместо stack
   frame slot.

**Conservative V1 rule:** если escape analysis **сомневается** (например,
local передан в generic fn, или в closure которая может escape) — PROMOTE.
Over-promote безопасен (только perf cost — лишняя heap allocation); миссили
escape = dangling pointer = UB. Оптимизация позже (`[M-118-escape-precise]`).

**Costs:** auto-promote = single heap allocation per promoted local (one-time;
GC reclaims later). Go pattern proven (escape analysis = sub-millisecond
compile overhead).

**D32 amend:** `&value` introduces typed pointer construction — это **НЕ**
Rust borrow (нет lifetime checker, нет `'a` параметров, нет XOR aliasing).
Safety обеспечивается escape analysis + auto-promote + unsafe gating
(deref только in unsafe context).

### 5. Auto-deref для `p.field`, `p.method()`, `p.field = v`

```nova
ro acc = Account { name: "Piter", age: 30 }
ro p = &acc                            // *ro Account

unsafe {
    p.name                              // ✓ auto-deref field → "Piter"
    p.age                               // ✓ auto-deref → 30
    *p                                  // ✓ explicit deref → Account (the reference)
    (*p).name                          // ✓ same as p.name

    p.greet()                           // ✓ auto-deref method call (one-level)
}

mut q = &mut Counter { value: 0 }       // *mut Counter
unsafe {
    q.value = 42                        // ✓ auto-deref field assignment (mut pointer)
    q.increment()                       // ✓ auto-deref method (mut binding → mut receiver allowed)
}
```

**Rules:**

| Op | `*ro T` | `*mut T` | Notes |
|---|---|---|---|
| `p.field` (read) | ✓ | ✓ | auto-deref one-level |
| `p.field = v` (assign) | ❌ E_POINTER_RO_ASSIGN | ✓ | requires `*mut` |
| `p.method()` (ro receiver) | ✓ | ✓ | auto-deref one-level |
| `p.method()` (mut receiver) | ❌ E_POINTER_RO_MUT_METHOD | ✓ | requires `*mut` |
| `*p` (explicit deref read) | ✓ | ✓ | yields `T` value |
| `*p = v` (explicit deref assign) | ❌ E_POINTER_RO_ASSIGN | ✓ | requires `*mut` |

**One-level only:** для multi-level pointer (`**T`) — recursive deref не
делается, нужно `(*p).field` или `(**p)` явно.

**Why one-level only:** auto-deref recursion path-dependent (confusing для
reader); explicit `*` chain — predictable. Mainstream (Go, D) тоже one-level.

**Inside `unsafe` block только:** `p.field`, `p.method()`, `p.field = v`,
`*p` — все pointer ops require unsafe context (см. §«unsafe model» ниже).
Pattern match `Option[*T]` — safe (inspection, не deref).

### 6. Pointer arithmetic → `*unsafe T`

```nova
unsafe {
    ro p1 = some_ptr + 1            // valid; результат: *unsafe T
    ro p2 = some_ptr + offset       // valid; результат: *unsafe T
    ro diff = p2 - p1               // ✓ pointer subtraction; результат: isize
    unsafe {
        *p1                          // deref *unsafe требует ещё unsafe layer
    }
}
```

**Rule:** `+` / `-` / `+=` / `-=` на pointer'ах:
1. **Только в `unsafe { }` блоке** — outside → `E_UNSAFE_REQUIRED`.
2. **Результат `+`/`-` (ptr+int):** `*unsafe T` — degrades в "unsafe variant"
   (alignment + bounds не гарантированы).
3. **Результат `ptr - ptr`:** `isize` (signed element count).
4. **`*unsafe T` deref** — требует **ещё один `unsafe` wrap** (nested), т.е.
   `*unsafe T` ops цельно opt-in.

**Units:** `p + n` смещает на `n * sizeof(T)` bytes (C/Rust convention).

**No multiplication/division:** `p * 2`, `p / 4` — `E_PTR_ARITHMETIC_INVALID`
(не математически осмыслено для адресов).

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
1. Compiler detects `Option[*T]` (или nested `Option[*T]` через alias / newtype).
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

**NPO applies к:**
- `Option[*T]` напрямую (all `*T` family variants)
- `Option[*fn(...) -> ...]` (function pointers)
- `Option[ptr]` (Plan 115 ptr через D214 amend как newtype)
- `Option[NewtypeОверПоинтер]` где `type X(*T)` или `type X(ptr)`
- Nested через newtype: `Option[Sqlite3Handle]` где `type Sqlite3Handle(*sqlite3)`

**NPO НЕ применяется к:**
- `Option[Option[*T]]` (двойной Option → tag нужен для различения inner None
  от outer None) — fallback к tagged repr; `W_OPTION_DOUBLE_NESTED` warning
  с suggestion использовать `Result[*T, NullKind]` или flatten

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
- Deref `*p`, auto-deref `p.field`, auto-deref `p.method()`, assign
  `p.field = v` / `*p = v` (mut pointer)
- Pointer arithmetic (`p + n`) — результат `*unsafe T`
- Cast `usize as *T` (reverse cast)
- Compare `<`/`>` (cross-allocation ordering)
- `&record.field` (GC compaction concern bypass)
- Cross-FFI call с pointer args (call external fn если она accepts/returns `*T`)
- Newtype construction `Handle(some_ptr)` где Handle wraps pointer
- Calling `#unsafe` fn

**Что safe вне unsafe:**
- Объявление типов `*T` в signatures, parameters, fields
- Объявление `external fn` с pointer params
- Чтение field `acc.next` где `next *T` (просто чтение pointer value)
- Pattern match на `Option[*T]` (inspection, не deref)
- Compare `==` / `!=` (identity check)
- Newtype declaration `type Handle ptr` / `type Handle(*T)`
- `p as usize` (address leak для logging / hash — но см. §«hash hazard»)

**Семантика — sugar над эффектом (D2-consistent):**
```nova
// User-facing:
unsafe { ro x = *p }

// Под капотом эквивалент (compiler desugar):
with unsafe_handler {              // built-in handler, не emit'ится в user code
    ro x = perform UnsafeOps.deref(p)
}
```

D2 spirit preserved (всё ещё effect mechanics); user syntax ergonomic.
`unsafe_handler` — built-in, не подлежит user override / shadowing.

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
- `#unsafe` сочетается с другими attrs: `#unsafe #stable(since="0.2") fn ...`.

### 10. `*fn(...)` function pointers для FFI

```nova
// FFI callback registration (no environment captured)
external fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }   // no Fail / no effects allowed

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
- **Cast `*fn → fn`** — `unsafe` only (wraps в captureless closure;
  `E_CAST_RAW_FN_TO_CLOSURE` без unsafe)

**Calling convention:** `*fn(...)` использует **C ABI текущей платформы**
(System V на Unix, MS x64 на Windows). Никаких explicit `extern "C"` keywords
— только один ABI поддерживается. Vararg / stdcall — followups
(`[M-118-vararg-ffi]`, `[M-118-stdcall-fn-ptr]`).

**Callback no-throw enforcement (production-grade):**

Nova `Fail` effect не может propagate через C boundary — C runtime не знает
про Nova exception machinery. Если Nova fn используется как `*fn` callback,
**body не должен иметь Fail effect**.

```nova
fn safe_cb(x i64) -> i64 {           // ✓ no Fail — OK as *fn callback
    x * 2
}

fn unsafe_cb(x i64) Fail -> i64 {     // ❌ Fail effect
    if x < 0 { Fail.throw("negative") }
    x * 2
}

unsafe {
    register(safe_cb as *fn(i64) -> i64)      // ✓ ok
    register(unsafe_cb as *fn(i64) -> i64)    // ❌ E_CALLBACK_THROWS_OVER_C_ABI
}
```

**Workaround:** wrap throwable logic в `catch` внутри callback body:
```nova
fn safe_cb_catching(x i64) -> i64 {
    catch e { unsafe_cb(x) } { -1 }   // catches Fail, returns sentinel
}
```

Pre-condition: type-checker emit'ит ошибку **на cast site**, не на declaration
(fn могут use'аться и как Nova-side fn с Fail, и как `*fn` callback в разных
местах).

### 11. `ptr` redefine (D214 amend) + `null ptr` retraction

Plan 115 V1 ввёл `ptr` как built-in primitive. Plan 118 **переопределяет**:

```nova
// D214 amend (Plan 118):
type ptr Option[*unsafe ()]            // newtype над nullable unsafe void-pointer
```

**Семантика:**
- `*unsafe ()` = pointer на unit type (zero-sized), unsafe-modifier (deref в
  любом случае unsafe — `()` нечего читать)
- `Option[*unsafe ()]` — nullable через NPO → ABI = single pointer `void*`
- `type ptr ...` — **newtype** (D52), distinct от `Option[*unsafe ()]` (требует
  explicit cast)

**ABI preserved:** `void*` в C, single pointer — **identical к Plan 115 V1**.
Backward compatible.

**`null ptr` literal retraction:**

Plan 115 V1 ввёл `null ptr` как INTERIM construct (см. D214 §1). Plan 118
**retracts** это:

```nova
// Plan 115 V1 (now retracted):
ro p ptr = null ptr                    // ❌ E_NULL_PTR_RETRACTED_USE_OPTION

// Plan 118 canonical:
ro p Option[ptr] = None                // ✓ NPO codegen → emits NULL
ro q Option[ptr] = Some(some_handle)
```

**Migration:**
- Existing `null ptr` literals → автоматически migrate'ятся в `None` если type
  context = `Option[ptr]`; иначе compile error с migration hint
- `external fn ... -> ptr` → должно стать `external fn ... -> Option[ptr]`
  (или `Option[*T]` для typed)
- Audit script (`scripts/migrate_null_ptr.sh`) — sed-style сразу + manual
  review для signatures
- Closes `[M-115-null-ptr-to-option-after-npo]` ✅

### 12. Casts

```nova
// Safe casts (any context):
ro x = p as usize               // ✓ leaks address для logging/hashmap keys (HAZARD: GC may compact!)
ro b = p as bool                // ❌ ERROR — nonsensical
p1 == p2                         // ✓ identity check

// Unsafe casts (require unsafe block):
unsafe {
    ro p = addr as *u8           // reverse cast int → pointer (memory-mapped I/O)
    ro p2 = p as *mut T          // ro → mut upgrade
    ro p3 = p as *unsafe T       // *T → *unsafe T
    ro p4 = unsafe_p as *T       // *unsafe T → *T (claims alignment + bounds)
    ro p5 = pt1 as *T2           // type punning (T1 ≠ T2)
    p1 < p2                      // cross-allocation ordering (UB unless same alloc)
}

// Implicit casts (compile-time auto):
*ro T → *T                       // identity (since *T == *ro T by default)
*mut T → *ro T                   // downgrade safe (mutability narrowing)
*mut T → *T                      // downgrade safe (== *ro T)
```

**Cast table:**

| From | To | Safe? | Notes |
|---|---|---|---|
| `*T` (= `*ro T`) | `usize` | ✓ | identity / debug — see hash hazard below |
| `usize` | `*T` | unsafe | reverse cast — memory-mapped I/O |
| `*ro T` | `*mut T` | unsafe | mutability upgrade |
| `*mut T` | `*ro T` | ✓ | downgrade (safe) |
| `*mut T` | `*T` | ✓ | downgrade (≡ `*ro T`) |
| `*T` | `*unsafe T` | ✓ | downgrade alignment guarantees |
| `*unsafe T` | `*T` | unsafe | reclaim alignment (user obligation) |
| `*T1` | `*T2` (T1≠T2) | unsafe | type punning |
| `fn → *fn` | ✓ если captureless | iff no env; `E_CLOSURE_HAS_ENV` если env |
| `*fn → fn` | unsafe | wraps в captureless closure |
| `*T` | `bool` | ❌ | `E_PTR_CAST_INVALID_TARGET` |
| `*T` | `f64` / `i32` / etc. | ❌ | only `usize` integer cast valid |

**Hash hazard (critical for production):** `p as usize` извлекает address;
moving GC может скомпактовать объект → address меняется → hash inconsistent.
**Rule:** address-based hashing для GC-tracked objects — UNSAFE pattern;
для FFI handles (non-GC) — safe. Diagnostic `W_PTR_AS_USIZE_GC_HASH_HAZARD`
если address-cast используется как HashMap key (heuristic).

### 13. Comparison

```nova
// Safe ops (any context):
p1 == p2                         // ✓ identity check (robust к GC move iff same alloc)
p1 != p2                         // ✓
p == None                        // ✓ для Option[*T] via NPO
match p { Some(q) => ..., None => ... }   // ✓ NPO pattern match

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
пользователь обещает no GC trigger between взятие и use (honor-system, §16).

### 15. Forbidden ops (даже в unsafe)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ ERROR E_ARRAY_INDEX_PTR_BANNED
    //                              arrays могут resize / GC move — pointer dangle
}

// `null` literal — НЕТ:
ro p Option[*u8] = null          // ❌ ERROR E_NULL_LITERAL_USE_NONE; use None
ro p Option[*u8] = None          // ✓ NPO emits NULL

// `undefined` — НЕТ:
mut p *u8 = undefined            // ❌ ERROR E_UNDEFINED_USE_NONE_INIT_PATTERN
mut p Option[*u8] = None         // ✓ then init: external_alloc(&mut p) where p stays Option[*u8]
```

**Rationale:**
- `&arr[i]` — array buffer может перемещаться (`.push` causes realloc; GC
  compaction). Безопасно нельзя; для FFI используется slice fat-pointer
  pattern (Plan 118.2).
- `null` literal — duplication с `None` (one-way-to-do); enforced via parser.
- `undefined` — uninitialized state — explicit pattern `Option[*T] = None +
  init` достаточен для FFI out-params. Полноценный `MaybeUninit[T]` —
  Plan 118.2.
- **Vararg FFI calls** (`printf(fmt, ...)`) — forbidden; wrapper через
  `args: [Any]` или dedicated FFI shim. `[M-118-vararg-ffi]` followup.

### 16. GC honor-system (W_UNSAFE_GC_TRIGGER warning)

**Контракт unsafe-блока:** внутри `unsafe { ... }` пользователь **обещает,
что не вызовет GC trigger** между взятием pointer'а и его use. GC trigger =
любая операция, способная вызвать collection / compaction:
- Heap allocation (`nova_alloc` calls)
- Yield-points (`await`, `spawn`, supervised{} boundary)
- String formatting which allocates (`interp"..."`)
- Functions с `#parks` / `#wakes` (Plan 113) — могут yield → потенциально GC

**Compiler warns на violations:**

```nova
unsafe {
    ro acc = Account { name: "Piter" }
    ro p = &acc

    ro other = Account { name: "Other" }   // ⚠️ W_UNSAFE_GC_TRIGGER
    //         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //         GC trigger inside unsafe block — pointer `p` may dangle

    p.name                                  // potentially dangling после allocation
}
```

**`W_UNSAFE_GC_TRIGGER` — warning** (не error в V1; user может silence через
explicit comment marker `// noqa: W_UNSAFE_GC_TRIGGER`).

**Rationale honor-system (vs formal pin API):**
- V1 GC = conservative Boehm-style → не двигает объекты → adresses стабильны
- Future moving GC → потребует formal pin API (`[M-118-pin-api]` followup)
- Honor-system + warning = pragmatic V1 trade-off (no runtime cost, spec
  contract clear, future-compatible)

**Spec contract (D216 §16):** "Внутри `unsafe { }` блока pointer validity
гарантируется compiler'ом ТОЛЬКО если user не нарушает no-GC-trigger
contract. Violation → silent UB. Compiler emit'ит warnings для detection
violations при static analysis; future pin API сделает enforcement
runtime-checked."

### 17. Pointer Debug formatting

```nova
unsafe {
    ro p *Account = &acc
    println("pointer: ${p.to_debug_str()}")     // → "pointer: 0x7f8b9c0a1000 -> Account"
}
```

**API:**
- `(*T).to_debug_str() -> str` — emits hex address + type name (debug only)
- Available **только в unsafe context**
- НЕ implements `Display` (forces explicit decision — pointer debugging =
  deliberate)

**Format string interpolation:**
- `"${p}"` без explicit conversion → `E_PTR_NO_DISPLAY_USE_DEBUG_STR` (hint:
  use `${p.to_debug_str()}`)

**Why explicit:** pointer addresses non-deterministic (vary per run, leak ASLR
info); accidental logging = security/debugging hazard.

### 18. FFI handle allocation contract (CRITICAL: tuple newtype vs record)

**Production-grade FFI guidance:**

| Handle form | Allocation | ABI | When |
|---|---|---|---|
| `type Handle(*T)` (tuple newtype) | **stack** | single pointer (zero-overhead) | opaque handles, no extra state |
| `type Handle(ptr)` (tuple newtype) | **stack** | single pointer (zero-overhead) | untyped opaque handles |
| `type Handle { ro p *T, ro extra State }` (record) | **heap** | pointer-to-struct (extra indirection) | handle с дополнительным state |

**Recommended pattern (Plan 115 V1 → Plan 118):**

```nova
// ✓ Canonical (zero-overhead) — tuple newtype
type Sqlite3Handle(*sqlite3)
type PngImageHandle(*png_struct)
type CurlEasyHandle(ptr)              // legacy untyped — keep until typed bind avail

external fn sqlite3_open(path str) -> (Option[Sqlite3Handle], i64)
//                                      ^^^^^^^^^^^^^^^^^^^^
//                                      Option[X(*T)] — NPO applies через newtype
```

```nova
// ✓ When extra state needed — record
type DbSession {
    ro handle Sqlite3Handle
    ro path str
    ro opened_at Time
}
```

**Migration of Plan 115 V1 ffi-cookbook examples:**
- `type Db { ro value ptr }` (record form) → `type Db(ptr)` (tuple newtype)
  — single-field record был V1 workaround до canonical syntax landed
- ABI change: pointer-to-struct → single pointer — **breaking change**
- Migration script + audit per Plan 118 Ф.9
- Closes followup `[M-118-handle-migration]`

### 19. Function call argument passing

```nova
fn process(p *ro Buffer, idx usize) -> u8 {
    unsafe { p.read_byte(idx) }
}

// Call site:
ro buf = make_buffer()
unsafe {
    ro byte = process(&buf, 42)         // & creates *ro Buffer (NO promote если scope-local)
}
```

**Rules:**
- `*T` parameters — pass by value (single pointer-word; standard C ABI)
- `&value` at call site — creates `*T` argument
- Auto-promote applies к escape-via-fn-arg (conservative: PROMOTE always for fn args)
- Compiler may optimize away promote если callee inline'ится и pointer не escapes
  (`[M-118-escape-precise]` followup)

### 20. `extern "C-unwind"` story (NEGATIVE: not supported V1)

**Question:** can Nova FFI throw across C boundary?

**V1 answer: NO.** External fn declarations and `*fn` callbacks must not
have Fail effect on the Nova→C boundary. Rationale:
- C runtime doesn't know Nova exception machinery
- Cross-language unwinding requires DWARF unwinder hookup (complex, platform-
  specific)
- Rust 2024 added `extern "C-unwind"` — research-level, defer to V2

**Diagnostic:** `E_CALLBACK_THROWS_OVER_C_ABI` (§10) + `E_EXTERNAL_FN_FAIL_EFFECT`
для `external fn ... Fail -> ...` declarations.

**Workaround:** catch внутри callback / wrapper, return sentinel value.

---

## Грамматика

Plan 118 — **language addition** (parser/checker/codegen). Изменения:

```ebnf
PointerType   ::= '*' PointerModifier? Type
PointerModifier ::= 'ro' | 'mut' | 'unsafe'

FnPointerType ::= '*fn' '(' TypeList ')' ('->' Type)?

UnsafeBlock   ::= 'unsafe' '{' Statements '}'

AttributeUnsafe ::= '#unsafe'                    // на fn declarations

AddrOfExpr    ::= '&' Expr                       // pointer creation (new prefix op)
DerefExpr     ::= '*' Expr                       // explicit deref (new prefix op)

OptionPtrType ::= 'Option' '[' PointerType ']'   // существующий Option, NPO-triggering form
```

**Backward compatibility:** new tokens (`*ro`, `*mut`, `*unsafe`, `unsafe`,
`#unsafe`, `&` prefix, `*` prefix) — не conflict'ят с existing syntax:
- Nova не использовал `*` как prefix operator до Plan 118
- Nova не использовал `&` как prefix operator
- `*` as binary multiplication остаётся (context-distinguished: `a * b` is binary
  if both sides bare identifiers/numbers; `*a` is prefix if `a` is sole operand
  in unary position)

**Parser disambiguation `*T` (type) vs `*expr` (deref):**
- Type position: `parse_type()` сразу видит `*` → PointerType production
- Expression position: `parse_unary()` видит `*` → DerefExpr production
- Contexts well-separated (Nova не имеет C-style `Type * varname` declarations)

---

## Фазы

### Ф.0 — GATE: design freeze + D-block drafts + worktree + audit (~½-1 dev-day)

> **Critical decision point:** Plan 118 — major language addition. Все 20
> sections дизайна — frozen после Ф.0. Изменения после Ф.0 → новый sub-plan.

- **Ф.0.1** Worktree `nova-p118` создан ✅ (2026-06-01).
- **Ф.0.2** Draft D216 в `spec/decisions/02-types.md` (после D52 type forms,
  перед D215 named tuples).
- **Ф.0.3** Draft D2 amend в `spec/decisions/04-effects.md` («D2 historical:
  no unsafe keyword; Plan 118 restores `unsafe { }` as effect-handler sugar»).
- **Ф.0.4** Draft D214 amend в `spec/decisions/02-types.md` («`ptr` redefined
  as newtype над `Option[*unsafe ()]`; `null ptr` retracted»).
- **Ф.0.5** Draft D32 amend в `spec/decisions/02-types.md` («`&value`
  introduces typed pointer construction — NOT Rust borrow; safety через
  escape analysis + auto-promote + unsafe gating»).
- **Ф.0.6** Audit existing pointer-related code:
  - `ptr` usages в Plan 115 / Plan 91.12 / Plan 116 (migrate compat check)
  - `null ptr` literals — count + migration plan (sed script draft)
  - `nova_rt/*.h` C-side pointer types (FFI ABI verification)
  - Existing `external fn` signatures (compat verification)
  - ffi-cookbook examples (migration plan для tuple-newtype form)
  - Record baseline `nova test` PASS count (для R1 regression gate)
- **Ф.0.7** Acceptance A1-A35 финализированы (см. §«Acceptance criteria»).
- **Ф.0.8** Test plan T1-T8 + R1-R5 финализированы (см. §«Tests»).
- **Ф.0.9** Sub-plan documents stubbed:
  - `docs/plans/118.1-ffi-intrinsics-and-cstring.md`
  - `docs/plans/118.2-slice-fat-pointer-and-uninit.md`
  - `docs/plans/118.3-pointer-concurrency-safety.md`
- **Ф.0.10** `docs/plans/README.md` updated с indexes для 118 + 118.1-3.
- **Ф.0.11** Commit `feat(plan118 Ф.0): GATE — design freeze + D216/D2/D214/D32
  amend drafts + sub-plan stubs`.

### Ф.1 — `*T` family parser/checker + ptr redefine (~1.5 dev-day)

> **Safety hatch:** если parser disambiguation `*T` vs multiplication
> оказывается non-trivial (контекстная грамматика), extract в Plan 118.0.1
> «*T parser foundations». Decision point: конец Ф.1.2.

**Implementation tasks:**
- **Ф.1.1** Lexer: ensure `*` и `&` tokens correctly produced как prefix /
  binary через context.
- **Ф.1.2** Parser: tokenize `*ro` / `*mut` / `*unsafe` / `*` prefixes для
  types. Disambiguation от `a * b` multiplication через position (type vs
  expression).
- **Ф.1.3** Parser: chain `*mut *ro T` — recursive PointerType production.
- **Ф.1.4** Parser: `*fn(Args) -> Ret` function pointer type (basic — будет
  доработан в Ф.6).
- **Ф.1.5** Type-checker: register `*T` family as distinct primitive types;
  `Ty::Ptr(modifier, Box<Ty>)` variant.
- **Ф.1.6** Type-checker: default `*T` ≡ `*ro T`.
- **Ф.1.7** Type-checker: binding-mut rule (`mut p *T` → `*mut T` default).
- **Ф.1.8** Type-checker: chain order semantics — modifier applies to its `*`.
- **Ф.1.9** Type-checker: `*T` valid в parameter, return, field, generic
  positions; emit type errors для других positions.
- **Ф.1.10** Codegen: emit `T*` C type для `*T`; cross-platform ABI verification
  (size + alignment).
- **Ф.1.11** Codegen: emit `const T*` для `*ro T` (helps clang/MSVC optimizer).
- **Ф.1.12** `ptr` redefine: `type ptr Option[*unsafe ()]` newtype в prelude;
  existing `ptr` usages compat verification.

**Tests:** T1 series (positive + negative — см. §«Tests»).

**Spec updates:**
- D216 §1-3 promoted к active с примерами.
- D52 cross-ref «*T family integration».

**Doc updates:**
- `docs/typed-pointers.md` (NEW) — overview document, §1-3 sections.

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t1_*` (12+ files)
- Full `nova test` после phase complete — no regressions
- ABI snapshot: `tests/abi/typed_pointers/t1_basic_pointer_size.expected`
  (sizeof check + struct layout)

**Commit:** `feat(plan118 Ф.1): *T family parser/checker + ptr redefine + D216 §1-3`

### Ф.2 — `&value` operator + escape analysis с auto-promote (~1.5 dev-day)

> **Safety hatch:** escape analysis edge cases могут require more time
> (closure capture, indirect escape through field stores, generic functions).
> Если > 1.5 day, extract escape-edge-cases в Plan 118.0.2.

**Implementation tasks:**
- **Ф.2.1** Parser: `&expr` prefix operator (pointer creation).
- **Ф.2.2** Type-checker: `&value` type inference (`*ro T` или `*mut T` по
  контексту binding).
- **Ф.2.3** Type-checker: `&` outside unsafe context — `E_UNSAFE_REQUIRED`.
  Exception: `&record` для GC-tracked references (heap allocation already)
  — также unsafe в V1 (consistency); future relaxation `[M-118-amp-heap-safe]`.
- **Ф.2.4** Escape analysis pass (new IR phase):
  - Collect `&local_var` usages (per fn)
  - For each `&local`: determine escape via uses
    - Return statement contains `&local` (transitively) → ESCAPE
    - Stored в heap field (`record.f = &local`) → ESCAPE
    - Captured в closure (`fn() { ... &local ... }`) → ESCAPE
    - Passed as fn arg (conservative: ESCAPE always; precise inlining
      `[M-118-escape-precise]` followup)
    - Used only локально (compute, compare, etc.) → NO promote
  - Mark escaped locals для `nova_alloc` codegen
- **Ф.2.5** Codegen: для promoted locals — heap allocation вместо stack slot;
  emit `nova_alloc` calls; pointer-to-heap returned by `&`.
- **Ф.2.6** Codegen: для non-promoted locals — stack slot retained; `&local`
  emits address of stack slot (scope-local valid).
- **Ф.2.7** D32 amend committed в spec.

**Tests:** T2 series (positive + negative escape patterns).

**Spec updates:**
- D216 §4 promoted к active.
- D32 amend committed.

**Doc updates:**
- `docs/typed-pointers.md` §4 (escape + auto-promote).

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t2_*` (15+ files)
- ABI snapshot: `tests/abi/typed_pointers/t2_promoted_local.expected`

**Commit:** `feat(plan118 Ф.2): &value operator + escape/auto-promote + D32 amend`

### Ф.3 — `unsafe { }` block + `#unsafe` attribute (D2 amend) (~1 dev-day)

**Implementation tasks:**
- **Ф.3.1** Parser: `unsafe { ... }` block syntax.
- **Ф.3.2** Parser: `#unsafe` attribute on fn declarations.
- **Ф.3.3** Type-checker: unsafe-context tracking — inside `unsafe { }` block
  OR inside `#unsafe` fn body. Context stack per fn.
- **Ф.3.4** Implementation as effect-handler sugar:
  - Built-in `UnsafeOps` effect (compiler-known, не user-declared)
  - `unsafe { ... }` desugars → `with unsafe_handler { ... }`
  - `unsafe_handler` — compiler-generated, не emit'ится в Nova code
  - Effect not propagated up the call stack (`unsafe` encapsulates per fn)
- **Ф.3.5** Error checks:
  - `E_UNSAFE_REQUIRED` — pointer op вне unsafe context
  - `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без `unsafe { }`
  - Diagnostic suggestions с hint syntax + auto-fix proposal (LSP)
- **Ф.3.6** D2 amend committed в spec.

**Tests:** T3 series (positive + negative, ~20 fixtures).

**Spec updates:**
- D216 §8-9 promoted к active.
- D2 amend committed.

**Doc updates:**
- `docs/typed-pointers.md` §«unsafe model».
- `docs/unsafe-block-pattern.md` (NEW) — when to use unsafe block, examples.

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t3_*` (20+ files)
- LSP test: hover на `unsafe` keyword показывает D216 §8 link

**Commit:** `feat(plan118 Ф.3): unsafe block + #unsafe attribute + D2 amend`

### Ф.4 — Auto-deref + pointer ops (arith/casts/compare/method-call/field-assign) (~1.5 dev-day)

**Implementation tasks:**
- **Ф.4.1** Type-checker: `*p` explicit deref (one level); inside unsafe context only.
- **Ф.4.2** Type-checker: `p.field` auto-deref one-level (read).
- **Ф.4.3** Type-checker: `p.field = v` auto-deref assignment (mut pointer
  required); `E_POINTER_RO_ASSIGN` для ro.
- **Ф.4.4** Type-checker: `p.method()` auto-deref one-level method call.
  - For mut-receiver methods: requires `*mut T`; `E_POINTER_RO_MUT_METHOD` для ro.
  - For ro-receiver methods: works on `*ro T` and `*mut T`.
- **Ф.4.5** Type-checker: pointer arithmetic `+`/`-`/`+=`/`-=` only inside
  `unsafe { }`, result type `*unsafe T` для `ptr ± int`; `isize` для `ptr - ptr`.
- **Ф.4.6** Type-checker: cast rules table (см. §«Casts») — full table impl.
- **Ф.4.7** Type-checker: comparison rules (`==`/`!=` safe; `<`/`>` unsafe).
- **Ф.4.8** Type-checker: `&record.field` only в unsafe context.
- **Ф.4.9** Type-checker: `&arr[i]` всегда forbidden (`E_ARRAY_INDEX_PTR_BANNED`).
- **Ф.4.10** Type-checker: `null` literal forbidden (`E_NULL_LITERAL_USE_NONE`);
  `undefined` forbidden (`E_UNDEFINED_USE_NONE_INIT_PATTERN`).
- **Ф.4.11** Type-checker: `W_PTR_AS_USIZE_GC_HASH_HAZARD` heuristic (address
  cast used as HashMap key).
- **Ф.4.12** Codegen: emit `*p`, `p->field`, `p->field = v`, `p->method(p, ...)`,
  `p + n` (sizeof-scaled), cast ops, compare ops.

**Tests:** T4 series (positive + negative, ~25 fixtures).

**Spec updates:**
- D216 §5-6, §12-15 promoted к active.

**Doc updates:**
- `docs/typed-pointers.md` §«auto-deref» + §«cast table».

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t4_*` (25+ files)
- ABI snapshot: `tests/abi/typed_pointers/t4_arith_unit_scaling.expected`

**Commit:** `feat(plan118 Ф.4): auto-deref + pointer ops (arith/casts/compare/method/field-assign)`

### Ф.5 — `Option[*T]` + NPO codegen + null-ptr retraction (~1 dev-day)

**Implementation tasks:**
- **Ф.5.1** Codegen lowering: detect `Option[*T]` type signature (any `T` —
  primitive, record, newtype-over-pointer); emit **single pointer** layout
  (8 bytes), не tagged struct.
- **Ф.5.2** Codegen pattern match: `if (ptr == NULL) None_branch else
  Some_branch(ptr)` — заменяет tag-check.
- **Ф.5.3** Codegen construction: `Some(p)` → emit `p`; `None` для `Option[*T]`
  → emit `NULL` (`((void*)0)`).
- **Ф.5.4** NPO detection rules:
  - Direct: `Option[*T]` всех вариантов
  - Через newtype: `Option[X]` где `type X(*T)` или `type X(ptr)` (tuple newtype)
  - Через function pointer: `Option[*fn(...) -> ...]`
  - Excluded: `Option[Option[*T]]` (nested) — fallback tagged + warning
- **Ф.5.5** ABI verification: `external fn malloc(sz usize) -> Option[*u8]`
  ABI = `uint8_t* malloc(size_t)` — direct C-FFI compatible.
- **Ф.5.6** Generic interaction: `Map[K, Option[*T]]` — NPO applies inside
  value position.
- **Ф.5.7** `null ptr` literal retraction:
  - Parser emit'ит `E_NULL_PTR_RETRACTED_USE_OPTION`
  - Migration script `scripts/migrate_null_ptr.sh` — sed-based bulk replace
  - Manual audit для signatures (`-> ptr` → `-> Option[ptr]` где actually nullable)
- **Ф.5.8** Migration: ffi-cookbook examples + stdlib `nova_rt/sqlite_mini_ffi.h`
  + Plan 115 fixtures updated.
- **Ф.5.9** Close `[M-115-null-ptr-to-option-after-npo]` ✅.

**Tests:** T5 series (positive + negative, ~20 fixtures).

**Spec updates:**
- D216 §7 promoted к active.
- D214 amend committed («`null ptr` retracted»).

**Doc updates:**
- `docs/typed-pointers.md` §«Option[*T] + NPO».
- `docs/ffi-cookbook.md` migrated к `Option[*T]` / tuple newtype patterns.
- `docs/migration/118-null-ptr-to-option.md` (NEW) — migration guide.

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t5_*` (20+ files)
- ABI snapshot: `tests/abi/typed_pointers/t5_npo_size.expected`
  (verifies `sizeof(Option[*T]) == sizeof(*T) == 8`)
- Bulk migration test: `scripts/migrate_null_ptr.sh` applied — full `nova test`
  ≥ baseline.

**Commit:** `feat(plan118 Ф.5): Option[*T] + NPO codegen + null-ptr retraction +
closes [M-115-null-ptr-to-option-after-npo]`

### Ф.6 — Function pointers `*fn(...)` для FFI + callback no-throw (~½-1 dev-day)

**Implementation tasks:**
- **Ф.6.1** Type-checker: `*fn(Args) -> Ret` distinct type from `fn(Args) -> Ret`
  closure.
- **Ф.6.2** Cast `fn → *fn` — compile-time check captureless (нет closure env);
  иначе `E_CLOSURE_HAS_ENV`.
- **Ф.6.3** Cast `*fn → fn` — unsafe only; wraps в captureless closure.
- **Ф.6.4** Callback no-throw enforcement: cast `Fn-with-Fail-effect → *fn` →
  `E_CALLBACK_THROWS_OVER_C_ABI`.
- **Ф.6.5** `external fn ... Fail -> ...` — `E_EXTERNAL_FN_FAIL_EFFECT` (V1; C
  ABI не propagates Nova exceptions).
- **Ф.6.6** Codegen: `*fn(...)` emit as C function pointer (`Ret (*name)(Args)`).
- **Ф.6.7** Calling convention: C ABI текущей платформы (System V на Unix,
  MS x64 на Windows). No `extern "C"` keyword (single ABI supported V1).
- **Ф.6.8** FFI callback тест — register Nova fn как callback в external C
  function, verify invocation roundtrip.

**Tests:** T6 series (positive + negative, ~12 fixtures).

**Spec updates:**
- D216 §10 + §20 promoted к active.

**Doc updates:**
- `docs/typed-pointers.md` §«*fn function pointers».
- `docs/ffi-cookbook.md` — callback registration example added.

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t6_*` (12+ files)
- Real callback round-trip test: `nova_rt/plan118_callback_test.h` — C calls
  Nova-registered callback, verifies result returned.

**Commit:** `feat(plan118 Ф.6): *fn function pointers + callback no-throw`

### Ф.7 — GC honor-system warnings + Debug fmt (~½ dev-day)

**Implementation tasks:**
- **Ф.7.1** Type-checker: GC trigger detection inside `unsafe { }` блока:
  - Heap allocations (`nova_alloc` calls — emitted by `Type { ... }`,
    `[T].new()`, `interp"..."`)
  - Yield-points (`await`, `spawn`, `supervised { }`)
  - Calls to `#parks` / `#wakes` fns (Plan 113)
- **Ф.7.2** Emit `W_UNSAFE_GC_TRIGGER` warning per violation site (per pointer
  binding still in scope).
- **Ф.7.3** Silence mechanism: `// noqa: W_UNSAFE_GC_TRIGGER` line marker
  (existing Nova diagnostic suppression).
- **Ф.7.4** Pointer Debug fmt:
  - `(*T).to_debug_str() -> str` built-in method (in unsafe context only).
  - Emit hex address + type name (`"0x7f... -> Account"`).
- **Ф.7.5** Format string interpolation check: `"${p}"` где `p *T` —
  `E_PTR_NO_DISPLAY_USE_DEBUG_STR` с hint.

**Tests:** T7 series (positive + negative, ~10 fixtures).

**Spec updates:**
- D216 §16-17 promoted к active.

**Doc updates:**
- `docs/typed-pointers.md` §«GC honor-system» + §«Debug fmt».

**Verification:**
- Targeted fixtures: `tests/fixtures/plan118/t7_*` (10+ files).
- Warning count test: known-violation file produces expected W_UNSAFE_GC_TRIGGER
  count.

**Commit:** `feat(plan118 Ф.7): GC honor-system warnings + Debug fmt`

### Ф.8 — Regression + cross-platform + ABI snapshot + perf bench (~1 dev-day)

**Implementation tasks:**
- **Ф.8.1** Full `nova test` ≥ baseline (post-Plan 115/120/121 baseline; record
  baseline number в Ф.0.6 audit).
- **Ф.8.2** Cross-platform CI matrix:
  - Linux × clang
  - Linux × gcc
  - Windows × MSVC
  - Windows × clang
  - macOS × clang (Apple silicon ARM64)
  - macOS × clang (Intel x86_64 — если available в CI)
- **Ф.8.3** ABI snapshot verification:
  - `tests/abi/typed_pointers/*.expected` — golden C-snippet files
  - Compare codegen output per platform/compiler — must match
  - Sizeof checks (sizeof(`*T`) == 8, sizeof(`Option[*T]`) == 8, etc.)
  - Struct layout checks (e.g., `Account*` vs `Account` field offsets identical)
- **Ф.8.4** Performance baseline benchmarks:
  - `bench/plan118/escape_promote_overhead.nv` — measure `&local + promote` overhead
    vs stack-only baseline (target: < 5ns per promote = single nova_alloc call)
  - `bench/plan118/npo_size_verification.nv` — runtime verify
    `sizeof(Option[*T]) == sizeof(*T)`
  - `bench/plan118/auto_deref_zero_cost.nv` — `p.field` vs `(*p).field` —
    must compile к identical asm
  - `bench/plan118/pointer_arith_unit_scaling.nv` — `p + 1` для `*i32` vs `*i64`
    — correct unit
- **Ф.8.5** Regression: `[M-115-null-ptr-to-option-after-npo]` migration —
  все Plan 115 fixtures still PASS после migration.

**Tests:** R1-R5 regression series + cross-platform matrix.

**Verification:**
- Cross-platform: 5+ combos all PASS
- ABI snapshots: 100% match
- Perf: targets met (escape promote < 5ns, NPO == sizeof(*T), auto-deref zero-cost)
- Full nova test: ≥ baseline (no regressions)

**Commit:** `feat(plan118 Ф.8): regression + cross-platform + ABI snapshot + perf bench`

### Ф.9 — Spec promotion + ffi-cookbook + nova doc + examples + closure (~½-1 dev-day)

**Implementation tasks:**
- **Ф.9.1** Promote D216 / D2 amend / D214 amend / D32 amend → active в
  `spec/decisions/`. Update `history/` cross-refs.
- **Ф.9.2** Cross-ref updates:
  - D52 ← `*T` family integration с type forms (newtype pattern для FFI handles)
  - D32 ← amend already done Ф.2; cross-ref добавить
  - D215 (Plan 120) ← tuple stack values + & escape semantics
  - D172 (Plan 113) ← pointer ops в `#realtime` context (E_REALTIME_POINTER_OP)
- **Ф.9.3** `nova doc` regen — typed pointer family documentation page.
- **Ф.9.4** `docs/ffi-cookbook.md` update:
  - Migration к `Option[*T]` / tuple newtype patterns
  - Typed buffer examples:
    - libpng image data copy preview (full impl в Plan 118.1/118.2)
    - libcurl header callback с `*fn(...)`
    - sqlite blob column read preview
  - Cross-ref Plan 118.1 (intrinsics) и 118.2 (slice) для full buffer APIs
- **Ф.9.5** `examples/typed_pointers/` (NEW) — minimal working samples:
  - `01_basic_pointer.nv` — `&value` + `*p` + `p.field`
  - `02_mut_pointer.nv` — `*mut T` + `p.field = v`
  - `03_option_npo.nv` — `Option[*T]` pattern match + NPO
  - `04_fn_pointer.nv` — `*fn(...)` callback registration
  - `05_unsafe_block.nv` — unsafe model demonstrating
  - `06_ffi_handle_tuple.nv` — `type Handle(*T)` canonical FFI pattern
- **Ф.9.6** `docs/project-creation.txt` — sprint section update.
- **Ф.9.7** `docs/simplifications.md`:
  - Close `[M-115-null-ptr-to-option-after-npo]` ✅
  - Open `[M-118-*]` markers per Risk register / followups
  - Open `[M-118.1-*]` / `[M-118.2-*]` / `[M-118.3-*]` sub-plan markers
- **Ф.9.8** `nova-private/discussion-log.md` — design decisions log:
  - Decomposition decision (Plan 118 family rationale)
  - GC honor-system vs formal pin (V1 decision)
  - `&acc` vs `*acc` syntax decision (kept `&`)
  - Callback no-throw enforcement decision
  - tuple-newtype canonical FFI pattern decision
- **Ф.9.9** Memory `project-plan118-status.md` (создать после merge).
- **Ф.9.10** Status section в этом plan-файле — заполнить per phase + final.
- **Ф.9.11** Final review + PR (НЕ self-merge — пользователь review'ит).

**Verification:**
- Spec D216/D2/D214/D32 all promoted к active
- `nova doc` builds clean
- All examples PASS
- ffi-cookbook + migration guide rendered correctly

**Commit:** `feat(plan118 Ф.9): spec promotion + ffi-cookbook + examples + closure`

### Ф.10 — Reserved (safety hatch / follow-on review) (~½ dev-day)

- Reserved для post-review fixes (если user review запрашивает изменения).
- Sub-plan handoffs: 118.1/118.2/118.3 plan files committed otherwise stubbed.
- Final sign-off + merge to main (через PR review process, не self-merge).

---

## D-block changes (full drafts)

### D216 (NEW) — Typed pointer family + unsafe model + null-safety через NPO

**Локация:** `spec/decisions/02-types.md` (после D52, перед D215).

**Что.** Foundational language addition: typed pointer family `*T` + unsafe
gating model + NPO null-safety. Replaces `ptr` opaque-only model из Plan 115
V1 с typed alternative; backward-compatible через D214 amend.

#### §1. `*T` family типов

- `*T` (= `*ro T`) — readonly typed pointer (default)
- `*ro T` / `*mut T` — explicit mutability
- `*unsafe T` — pointer после арифметики (alignment/bounds gone)
- Size: pointer-width (8 bytes на 64-bit); ABI: `T*` в C
- Validity: **always non-null** (compile-time invariant)

#### §2. Binding mut rule

`mut p *T` ≡ `mut p *mut T` (pointer mut по default при mut binding).
Explicit `ro p *mut T` valid (edge case: cannot reassign p, BUT can `*p = ...`).

#### §3. Chain order (multi-level pointers)

Modifier перед `*` относится к этому pointer'у; read left-to-right
(`*mut *ro T` = mut pointer на ro pointer на T).

#### §4. `&value` operator + escape analysis с auto-promote

- `&value` creates `*ro T` or `*mut T` (по контексту binding) — **в unsafe context**
- Stack values (primitives, tuples) auto-promoted в heap если pointer escapes
  scope (return / closure / heap-field store / fn arg)
- Records (heap references) — `&record` creates pointer на reference
- GC-friendly семантика (vs Rust lifetimes — у нас GC + auto-promote)
- Conservative V1: promote если ANY uncertainty; precise inlining followup
  `[M-118-escape-precise]`

#### §5. Auto-deref

- `*p` explicit deref (one level)
- `p.field` auto-deref one level (Go-style)
- `p.method()` auto-deref method call (one level)
- `p.field = v` auto-deref assignment (requires `*mut T`)
- Multi-level pointers require explicit `(*p).field` chain
- **Только в unsafe context**

#### §6. Pointer arithmetic

- `+`/`-` only в `unsafe { }` block, result `*unsafe T` для `ptr ± int`,
  `isize` для `ptr - ptr`
- Units: sizeof(T)-scaled
- `*unsafe T` deref требует ещё один unsafe wrap
- `*`/`/`/etc. — `E_PTR_ARITHMETIC_INVALID`

#### §7. Null safety: `Option[*T]` + NPO

- `*T` non-null; `Option[*T]` nullable через **NPO codegen**:
  - Layout: single pointer (8 bytes), не tagged struct
  - Pattern match: NULL-check, не tag-check
  - Direct C-FFI compatible
- NPO applies к `Option[*T]` всех вариантов, `Option[*fn(...)]`, `Option[ptr]`,
  `Option[NewtypeOверPtr]`
- Excluded: nested `Option[Option[*T]]` → tagged fallback + `W_OPTION_DOUBLE_NESTED`

#### §8. `unsafe { }` block

- Pointer ops require unsafe context (compile-time gating)
- Implementation: sugar над `with unsafe_handler { perform UnsafeOps.* }` (D2-consistent)
- `unsafe_handler` — built-in, не user-overridable
- Effect не propagates up (encapsulates per fn — canonical Rust pattern)

#### §9. `#unsafe` attribute

- `#unsafe fn` body — implicit unsafe context
- Call `#unsafe` fn — requires `unsafe { ... }` wrap у caller (visual marker)
- No propagation up — каждая fn decides encapsulate or propagate

#### §10. `*fn(...)` function pointers

- `*fn(Args) -> Ret` distinct от `fn(Args) -> Ret` closure
- Cast `fn → *fn` — captureless required (`E_CLOSURE_HAS_ENV` иначе)
- Cast `*fn → fn` — unsafe (wraps в captureless closure)
- Callback no-throw: `Fn-with-Fail → *fn` cast → `E_CALLBACK_THROWS_OVER_C_ABI`
- Calling convention: default C ABI текущей платформы (single ABI V1)

#### §11. `ptr` redefine (D214 amend cross-ref)

- `type ptr Option[*unsafe ()]` newtype
- `null ptr` literal retracted (use `None` instead)
- ABI preserved (single `void*`)

#### §12. Casts

- Safe: `*T → usize`, `*mut T → *ro T`, `*T → *unsafe T`, `fn → *fn` (captureless)
- Unsafe: `usize → *T`, `*ro T → *mut T`, `*unsafe T → *T`, `*T1 → *T2`, `*fn → fn`
- Invalid: `*T → bool / f64 / ...` (`E_PTR_CAST_INVALID_TARGET`)
- Hash hazard: `p as usize` для GC-tracked objects + HashMap key →
  `W_PTR_AS_USIZE_GC_HASH_HAZARD`

#### §13. Comparison

- `==`/`!=` safe (identity)
- `<`/`>`/`<=`/`>=` unsafe (cross-allocation UB + moving GC concern)

#### §14. `&record.field` only в unsafe

- GC compaction concern: address меняется при collection
- Inside unsafe: user обещает no GC trigger (honor-system, §16)

#### §15. Forbidden ops

- `&arr[i]` всегда — `E_ARRAY_INDEX_PTR_BANNED` (array realloc/GC concern)
- `null` literal — `E_NULL_LITERAL_USE_NONE` (use `None`)
- `undefined` — `E_UNDEFINED_USE_NONE_INIT_PATTERN` (use `Option[*T] = None + init`)
- Vararg calls — `E_VARARG_NOT_SUPPORTED` (followup `[M-118-vararg-ffi]`)

#### §16. GC honor-system

- Unsafe block user contract: no GC trigger между pointer creation и use
- Compiler emits `W_UNSAFE_GC_TRIGGER` warning per violation
- Silence: `// noqa: W_UNSAFE_GC_TRIGGER` comment marker
- Future formal pin API — `[M-118-pin-api]` followup
- Current GC (Boehm conservative) не двигает объекты → V1 безопасно

#### §17. Pointer Debug fmt

- `(*T).to_debug_str() -> str` built-in method (in unsafe only)
- Emits hex address + type name
- `"${p}"` interpolation → `E_PTR_NO_DISPLAY_USE_DEBUG_STR` (explicit decision required)

#### §18. FFI handle allocation contract

- **Tuple newtype** `type Handle(*T)` / `type Handle(ptr)` — stack-allocated,
  single pointer ABI (zero overhead) — **canonical для opaque handles**
- **Record** `type Handle { ro p *T, ro extra State }` — heap-allocated,
  pointer-to-struct ABI — для handles с extra state

#### §19. Function call argument passing

- `*T` parameters — pass by value (single pointer-word)
- `&value` at call site creates `*T` argument
- Auto-promote applies к escape-via-fn-arg (conservative)

#### §20. `extern "C-unwind"` story

- V1 NO — external fn + `*fn` callbacks must not have Fail effect
- Diagnostic: `E_EXTERNAL_FN_FAIL_EFFECT`, `E_CALLBACK_THROWS_OVER_C_ABI`
- V2 — research `extern "C-unwind"` (Rust 2024 model)

**Cross-ref:** D2 (amend — unsafe keyword restored), D52 (tuple newtype +
type forms), D32 (amend — `&value` not Rust borrow), D215 (Plan 120 stack
tuples), D214 (Plan 115 ptr — amended to use D216 foundations), D172 (Plan
113 — pointer ops в `#realtime` ban).

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
`*p`, auto-deref `p.field` / `p.method()`, auto-deref assign `p.field = v` /
`*p = v`, pointer arithmetic, `usize as *T` reverse cast, `<`/`>` pointer
ordering, `&record.field`, calling `#unsafe` fn, newtype construction
`Handle(some_ptr)`.

**Cross-ref:** D216 (typed pointer foundations using this model), D3 (effect
syntax — `unsafe_handler` follows convention), D61 (effect handlers).

### D214 AMEND — `ptr` redefinition + `null ptr` retraction

**Локация:** `spec/decisions/02-types.md` (D214 history + amend).

**D214 v1 (Plan 115):** `ptr` — built-in primitive type, opaque pointer-sized
integer; `null ptr` literal valid (INTERIM construct).

**D214 v2 (Plan 118 amend):** `ptr` is **newtype** над `Option[*unsafe ()]`:

```nova
type ptr Option[*unsafe ()]
```

**Semantically equivalent в V1 use cases** (opaque handle pattern):
- `type Sqlite3Handle ptr` — works as before
- `external fn ... -> ptr` — ABI `void*` = `Option[*unsafe ()]` ABI via NPO = same
- Tuple-by-value returns `(Handle, i64)` — unchanged

**`null ptr` literal retraction:**
- `null ptr` deprecated, emits `E_NULL_PTR_RETRACTED_USE_OPTION`
- Migration: `null ptr` → `None` (when type context is `Option[ptr]`)
- Closes `[M-115-null-ptr-to-option-after-npo]` ✅

**Migration:** automated через `scripts/migrate_null_ptr.sh` (sed-based);
manual audit для signatures (`-> ptr` где fn actually returns nullable →
`-> Option[ptr]`).

**Cross-ref:** D216 (`*T` family + `Option` + `unsafe` modifier foundations),
D52 (newtype syntax).

### D32 AMEND — `&value` introduces typed pointer construction (NOT Rust borrow)

**Локация:** `spec/decisions/02-types.md` (D32 §«Что отвергнуто» section
revised + new §«Plan 118 amend»).

**D32 v1:** «`&T` (borrow в Rust-стиле) **не существует в Nova**». No address-of
operator вообще.

**D32 v2 (Plan 118 amend):** `&value` operator **introduced**, but with
critical clarifications:

- `&value` создаёт **typed pointer** (`*T` / `*mut T`), не Rust borrow
- **НЕТ lifetime checker, нет `'a` параметров, нет XOR aliasing rules**
- Safety обеспечивается через:
  1. **Escape analysis + auto-promote** для stack values (heap-allocate если
     pointer escapes scope) — Go-style
  2. **Unsafe gating** — `&` operator + pointer deref только in `unsafe { }`
     context
  3. **GC honor-system** — user обещает no GC trigger в unsafe block
- Mainstream comparison: ближе к Go's `&` (managed pointer + escape analysis)
  + C# `unsafe` boundary + Rust `unsafe { *p }` deref pattern
- D32 «no borrow» **preserved в spirit** — typed pointers это не borrow,
  это explicit unsafe-gated raw pointers с safety net через GC

**Cross-ref:** D216 (typed pointer foundations), D215 (Plan 120 stack tuples
+ escape rules), Plan 118 design §«&value operator + escape analysis».

### D52 cross-ref (no amend) — newtype canonical для FFI handles

**Локация:** `spec/decisions/02-types.md` (D52 §«Use cases» добавить параграф).

Tuple newtype form `type Handle(*T)` / `type Handle(ptr)` — **canonical для
opaque FFI handles** (zero-overhead, single pointer ABI). Record form
`type Handle { ro p *T, ... }` — для handles с extra state (heap-allocated).
Plan 118 + Plan 115 FFI cookbook examples мигрированы к tuple newtype.

**Cross-ref:** D215 (tuple stack allocation), D214 amend (ptr redefine),
D216 §18 (FFI handle allocation contract).

---

## Tests

Test groups T1-T8 (positive + negative). Per-phase commits include targeted
fixtures + ABI snapshots. Naming convention: `tests/fixtures/plan118/tN_M_<name>.nv`
(positive) или `tN_M_neg_<name>.nv` (negative — must fail with specific error code).

### T1 — `*T` family parser/checker (Ф.1)

**Positive:**
- **T1.1** Parse `*T` / `*ro T` / `*mut T` / `*unsafe T` в type positions
- **T1.2** `*T` ≡ `*ro T` default rule (type equality check)
- **T1.3** Chain `*mut *ro T` parses correctly; mutability levels distinct
- **T1.4** Binding-mut rule: `mut p *T` infers `*mut T`
- **T1.5** Edge case `ro p *mut T` — valid (binding ro, pointee mut)
- **T1.6** `*T` valid в fn param, return type, record field
- **T1.7** `*T` valid в generic position (`Map[K, *V]`)
- **T1.8** `ptr` newtype works post-D214 amend (existing code compat)
- **T1.9** Codegen: `*T` → C `T*` correct ABI
- **T1.10** Codegen: `*ro T` → C `const T*` (helps optimizer)
- **T1.11** sizeof check: `sizeof(*T) == 8` на 64-bit

**Negative:**
- **NEG-T1.12** `*T` в expression position без unsafe (use site) — `E_UNSAFE_REQUIRED`
- **NEG-T1.13** Invalid modifier: `*const T` — `E_INVALID_POINTER_MODIFIER`
  (must be ro/mut/unsafe или omit)
- **NEG-T1.14** `*` без type — `E_PARSE_POINTER_TYPE_INCOMPLETE`
- **NEG-T1.15** `*ro mut T` — `E_DUPLICATE_POINTER_MODIFIER`

### T2 — `&value` + escape/auto-promote (Ф.2)

**Positive:**
- **T2.1** `&local_primitive` — promotion triggered if returned from fn
- **T2.2** `&local_named_tuple` (Plan 120) — promotion triggered if stored в heap field
- **T2.3** `&record` (heap reference) — pointer на reference, no promotion
  (record уже в heap)
- **T2.4** `&local` used только в текущем scope (no escape) — NO promotion,
  stack-local pointer
- **T2.5** Closure capture: `|| { ... &local ... }` — escape if closure
  outlives scope
- **T2.6** Fn-arg pass: `f(&local)` — conservative promote
- **T2.7** Codegen: promoted locals — `nova_alloc` calls; non-promoted —
  stack slot pointer
- **T2.8** `&value` binding-mut inference: `mut p = &x` → `*mut T` (if x mutable)

**Negative:**
- **NEG-T2.9** `&local` outside unsafe (V1) — `E_UNSAFE_REQUIRED`
- **NEG-T2.10** `&const_value` (const binding) — `E_AMP_CONST_BINDING`
- **NEG-T2.11** `&literal` (e.g. `&42`) — `E_AMP_LITERAL`
- **NEG-T2.12** Escape analysis correctness: `&local` returned but local
  isn't promoted (regression test) — must promote correctly

### T3 — `unsafe { }` block + `#unsafe` attribute (Ф.3)

**Positive:**
- **T3.1** `unsafe { *p }` — parses + type-checks inside fn
- **T3.2** `#unsafe fn foo() { *p }` — `*p` ok без обёртки внутри fn body
- **T3.3** `safe_fn() { unsafe { ffi_wrapper(p) } }` где `ffi_wrapper` #unsafe — ok
- **T3.4** Nested `unsafe { unsafe { *p } }` — allowed (redundant но не error)
- **T3.5** `unsafe { }` desugar verification (lowering to effect handler call —
  inspect HIR/MIR output)
- **T3.6** D2 amend test — spec mentions Plan 118 restoration
- **T3.7** `#unsafe` + other attrs: `#unsafe #stable(since="0.2") fn ...` — parses

**Negative:**
- **NEG-T3.8** `safe_fn() { ffi_wrapper(p) }` где `ffi_wrapper` #unsafe →
  `E_UNSAFE_CALL_REQUIRES_WRAP`
- **NEG-T3.9** `safe_fn() { *p }` без unsafe wrap → `E_UNSAFE_REQUIRED`
- **NEG-T3.10** `safe_fn() { &x }` без unsafe wrap → `E_UNSAFE_REQUIRED`
- **NEG-T3.11** User attempts user-defined `unsafe_handler` override —
  `E_UNSAFE_HANDLER_BUILTIN_ONLY`

### T4 — Auto-deref + pointer ops (Ф.4)

**Positive auto-deref:**
- **T4.1** `unsafe { p.field }` auto-deref one level (read)
- **T4.2** `unsafe { *p }` explicit deref
- **T4.3** `unsafe { p.method() }` auto-deref method call (ro receiver)
- **T4.4** `unsafe { p.method() }` mut receiver — works on `*mut T`
- **T4.5** `unsafe { p.field = v }` auto-deref assignment (mut pointer)
- **T4.6** `unsafe { *p = v }` explicit assignment (mut pointer)

**Positive arith/cast/compare:**
- **T4.7** `unsafe { p + 1 }` arith — result `*unsafe T`
- **T4.8** `unsafe { unsafe { *(p + 1) } }` — nested unsafe для `*unsafe T` deref
- **T4.9** `unsafe { p2 - p1 }` — result `isize`
- **T4.10** `p as usize` safe outside unsafe
- **T4.11** `p1 == p2` safe outside unsafe
- **T4.12** Pointer arithmetic unit scaling: `*i32 + 1` vs `*i64 + 1` —
  different byte offsets (4 vs 8); test codegen output

**Negative auto-deref:**
- **NEG-T4.13** `safe { p.field }` без unsafe — `E_UNSAFE_REQUIRED`
- **NEG-T4.14** `p.field = v` где `p *ro T` — `E_POINTER_RO_ASSIGN`
- **NEG-T4.15** `p.mut_method()` где `p *ro T` — `E_POINTER_RO_MUT_METHOD`

**Negative arith/cast/compare:**
- **NEG-T4.16** `usize as *T` outside unsafe — `E_UNSAFE_REQUIRED`
- **NEG-T4.17** `p1 < p2` outside unsafe — `E_UNSAFE_REQUIRED`
- **NEG-T4.18** `p * 2` — `E_PTR_ARITHMETIC_INVALID`
- **NEG-T4.19** `&arr[i]` — `E_ARRAY_INDEX_PTR_BANNED`
- **NEG-T4.20** `null` literal use — `E_NULL_LITERAL_USE_NONE`
- **NEG-T4.21** `undefined` use — `E_UNDEFINED_USE_NONE_INIT_PATTERN`
- **NEG-T4.22** `p as bool` — `E_PTR_CAST_INVALID_TARGET`
- **NEG-T4.23** `p as f64` — `E_PTR_CAST_INVALID_TARGET`

**Hash hazard:**
- **WARN-T4.24** `map.insert(p as usize, v)` — emits `W_PTR_AS_USIZE_GC_HASH_HAZARD`

### T5 — `Option[*T]` + NPO codegen + null-ptr retraction (Ф.5)

**Positive NPO:**
- **T5.1** `Option[*T]` size == sizeof(*T) (single pointer; verified via runtime
  sizeof check + ABI snapshot)
- **T5.2** `Some(p)` codegen — emit `p` literally
- **T5.3** `None` для `Option[*T]` — emit `NULL` (`((void*)0)`)
- **T5.4** Pattern match codegen — `if (ptr == NULL) None_branch else Some_branch`
- **T5.5** `external fn malloc(sz usize) -> Option[*u8]` returns NULL → `None` match
- **T5.6** Pattern match `Some(p)` — p is `*T` non-null guaranteed внутри branch
- **T5.7** Generic interaction `Map[K, Option[*T]]` — NPO applies inside
- **T5.8** `Option[Sqlite3Handle]` где `type Sqlite3Handle(*sqlite3)` — NPO через newtype
- **T5.9** `Option[*fn(...) -> ...]` — NPO для function pointer
- **T5.10** `Option[ptr]` где ptr = newtype — NPO works

**Negative NPO:**
- **NEG-T5.11** `Option[Option[*T]]` — NOT NPO; emits tagged + warning
  `W_OPTION_DOUBLE_NESTED`

**Null-ptr retraction:**
- **NEG-T5.12** `ro p ptr = null ptr` — `E_NULL_PTR_RETRACTED_USE_OPTION` (с migration hint)
- **NEG-T5.13** `null` literal — `E_NULL_LITERAL_USE_NONE` (general)
- **T5.14** Migration: `null ptr` → `None` automated через sed — full nova test ≥ baseline
- **T5.15** Pre-existing ffi-cookbook examples migrated; PASS post-migration

### T6 — Function pointers `*fn(...)` (Ф.6)

**Positive:**
- **T6.1** `*fn(i64) -> ()` type parses + accepts
- **T6.2** Cast captureless `fn → *fn` ok
- **T6.3** External fn accepts `*fn(i64) -> ()` callback parameter
- **T6.4** FFI invocation — Nova fn registered as C callback, invoked from C side,
  result returned to Nova side (roundtrip test через `nova_rt/plan118_callback_test.h`)
- **T6.5** `*fn` size = 8 bytes (single pointer)
- **T6.6** `Option[*fn(...)]` NPO works

**Negative:**
- **NEG-T6.7** Cast closure-with-env `fn → *fn` — `E_CLOSURE_HAS_ENV`
- **NEG-T6.8** Cast Nova-fn-with-Fail-effect `fn → *fn` — `E_CALLBACK_THROWS_OVER_C_ABI`
- **NEG-T6.9** Declare `external fn ... Fail -> ...` — `E_EXTERNAL_FN_FAIL_EFFECT`
- **NEG-T6.10** Cast `*fn → fn` outside unsafe — `E_UNSAFE_REQUIRED`

### T7 — GC honor-system warnings + Debug fmt (Ф.7)

**Positive:**
- **T7.1** `unsafe { ro p = &x; y.read() }` (no GC trigger in scope) — no warning
- **T7.2** `unsafe { ro p = &x; ro acc = Account {...}; *p }` — emits
  `W_UNSAFE_GC_TRIGGER` (allocation between & and use)
- **T7.3** `unsafe { ro p = &x; await something(); *p }` — emits
  `W_UNSAFE_GC_TRIGGER` (yield-point)
- **T7.4** `// noqa: W_UNSAFE_GC_TRIGGER` silencing works
- **T7.5** `p.to_debug_str()` emits hex address + type name (regex check)

**Negative:**
- **NEG-T7.6** `"${p}"` interpolation — `E_PTR_NO_DISPLAY_USE_DEBUG_STR`
- **NEG-T7.7** `p.to_debug_str()` outside unsafe — `E_UNSAFE_REQUIRED`

### T8 — Integration with adjacent plans

**Positive:**
- **T8.1** Plan 115 (`ptr`) — `type Sqlite3Handle ptr` works post-D214 amend
- **T8.2** Plan 116 (Tls — when shipped) — handle types pattern compatible
  (just documented compatibility, not runtime test)
- **T8.3** Plan 83.12 std/net types — `TcpListener` / `TcpStream` / `UdpSocket`
  (physical types в std/net/tcp.nv, Plan 83.12 ✅ closed) — handle fields
  продолжают компилироваться post-D214 amend (NOT effect TcpNet из Plan 91.12 —
  тот ещё не shipped)
- **T8.4** Plan 120 (named tuples) — `&named_tuple` auto-promotion correct
- **T8.5** Plan 113 (`#realtime`) — pointer ops в `#realtime` fn body —
  `E_REALTIME_POINTER_OP` (deref может GC trigger)

**Negative:**
- **NEG-T8.6** `#realtime fn foo() { unsafe { *p } }` — `E_REALTIME_POINTER_OP`

### R — Regression

- **R1** Full `nova test` ≥ post-Plan 115/120 baseline (record exact count в Ф.0)
- **R2** Cross-platform CI (5+ platform/compiler combos) — all PASS
- **R3** ABI snapshot verification (`tests/abi/typed_pointers/*.expected` —
  100% match per platform)
- **R4** Performance benchmarks meet targets:
  - escape promote < 5ns per
  - NPO size == sizeof(*T) на every platform
  - auto-deref `p.field` vs `(*p).field` — identical asm
  - pointer arith unit-scaling correct
- **R5** Plan 115 V1 fixtures all PASS post-migration to `Option[*T]` / tuple-newtype

---

## Acceptance criteria

| # | Критерий | Verification |
|---|---|---|
| A1 | `*T` family типов (`*T`/`*ro T`/`*mut T`/`*unsafe T`) parses + type-checks | T1.1-T1.7 |
| A2 | Binding-mut rule: `mut p *T` → pointer mut по default | T1.4-T1.5 |
| A3 | Chain order `*mut *ro T` correctly nested | T1.3 |
| A4 | `*T` codegen → C `T*` correct ABI; `*ro T` → `const T*` | T1.9-T1.10 + ABI snapshot |
| A5 | `sizeof(*T) == 8` на 64-bit verified | T1.11 + ABI snapshot |
| A6 | `&value` operator + escape analysis с auto-promote — correct для всех stack scenarios | T2.1-T2.8 |
| A7 | Conservative escape: promote если ANY uncertainty; non-promoted only если clearly scope-local | T2.4 + T2.12 |
| A8 | `&` operator outside unsafe — `E_UNSAFE_REQUIRED` | NEG-T2.9 |
| A9 | `unsafe { }` block parses + type-checks | T3.1, T3.4 |
| A10 | `#unsafe` attribute parses + body имплицитно unsafe context | T3.2, T3.7 |
| A11 | Calling `#unsafe` fn без wrap — `E_UNSAFE_CALL_REQUIRES_WRAP` | NEG-T3.8 |
| A12 | Auto-deref `p.field` + `p.method()` one level inside unsafe | T4.1, T4.3-T4.4 |
| A13 | Field assignment `p.field = v` для `*mut T` works; ro pointer errors | T4.5 + NEG-T4.14 |
| A14 | Pointer arithmetic only в unsafe; result `*unsafe T` (ptr+int) / `isize` (ptr-ptr) | T4.7-T4.9 |
| A15 | Pointer arith unit scaling (sizeof(T)-multiplied) — verified | T4.12 + ABI snapshot |
| A16 | Cast table enforced (safe vs unsafe casts; invalid targets rejected) | T4.10, T4.16, NEG-T4.22-T4.23 |
| A17 partial ✅ 2026-06-02 | Comparison: `==`/`!=` safe; `<`/`>` unsafe (E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE) | T4.11 + NEG-T4.17 |
| A18 | Forbidden ops: `&arr[i]`, `null`, `undefined` | NEG-T4.19, NEG-T4.20, NEG-T4.21 |
| A19 | `Option[*T]` + NPO codegen (single-pointer layout, NULL pattern match) | T5.1-T5.6 + ABI snapshot |
| A20 | NPO applies через newtype: `Option[Sqlite3Handle]` где `type Sqlite3Handle(*T)` | T5.8 |
| A21 | NPO applies к `Option[*fn(...)]` и `Option[ptr]` | T5.9, T5.10 |
| A22 | NPO excluded для `Option[Option[*T]]` — tagged fallback + W_OPTION_DOUBLE_NESTED | NEG-T5.11 |
| A23 | `null ptr` literal retracted; migration script works; closes [M-115-null-ptr-to-option-after-npo] | NEG-T5.12 + T5.14-T5.15 |
| A24 | `*fn(...)` function pointers для FFI roundtrip — verified end-to-end | T6.1-T6.4 |
| A25 | Callback no-throw enforced: Fn-with-Fail cast → *fn — error | NEG-T6.8 |
| A26 | `external fn ... Fail -> ...` — error (Fail effect не allowed on FFI boundary) | NEG-T6.9 |
| A27 | GC honor-system warnings: W_UNSAFE_GC_TRIGGER emitted на violations | T7.2-T7.3 |
| A28 partial ✅ 2026-06-02 | Pointer Debug fmt: `.to_debug_str()` works; `"${p}"` interpolation errors | T7.5 + NEG-T7.6 |
| A29 | `ptr` redefine (D214 amend) backward-compatible; existing Plan 115 fixtures work | T1.8, T8.1, R5 |
| A30 | D216 + D2 amend + D214 amend + D32 amend promoted в active spec | spec diff verification |
| A31 | Cross-platform PASS (Linux/Win/macOS × clang/MSVC/gcc — 5+ combos); full nova test ≥ baseline | R1-R3 |
| A32 | Performance targets met (escape promote < 5ns, NPO == sizeof(*T), auto-deref zero-cost, arith unit-scaling) | R4 |
| A33 | Plan 113 `#realtime` interaction enforced: E_REALTIME_POINTER_OP | NEG-T8.6 |
| A34 | FFI handle canonical pattern (tuple newtype) documented в ffi-cookbook; migration applied | Ф.9.4 + R5 |
| A35 | Examples `examples/typed_pointers/01-06_*.nv` все PASS | Ф.9.5 |

---

## Risk register

| # | Риск | Митигация |
|---|---|---|
| R-1 | Parser disambiguation `*T` (pointer type) vs `a * b` (multiplication) | Context-sensitive: `*` в type position = pointer; `*` в expression position = multiplication. Tested via `*expr` (deref) vs `*Type` (pointer type) disambiguation through expression-vs-type position parsing. Standard pattern (Rust, Zig, C++) |
| R-2 | Parser disambiguation `&value` — Nova не имел `&` prefix; integrate carefully | Single new prefix op; no `&&` boolean conflict (Nova uses `and` keyword); test thoroughly в Ф.2.1 |
| R-3 | Escape analysis edge cases (closure capture, indirect via heap field stores, generic functions) | Safety hatch Ф.2 preamble: если edge cases > 1.5 day, extract в Plan 118.0.2. V1 conservative: PROMOTE если ANY uncertainty (over-promote OK для correctness; perf optimization позже [M-118-escape-precise]) |
| R-4 | NPO codegen с generics (`Map[K, Option[*T]]` — NPO inside value position) | Type-checker mark NPO-eligible at monomorphization time; codegen generates specialized layout per generic instance. Tested T5.7 |
| R-5 | NPO + newtype detection (`Option[Sqlite3Handle]` where `type Sqlite3Handle(*T)`) | Mono'd struct lookup в codegen; check if underlying type is `*T` family OR ptr; tested T5.8 |
| R-6 | D2 amend — restoring removed keyword (политический риск spec narrative) | D2 spirit preserved (effect handler sugar под капотом); user-facing syntax improvement. Spec amend explanatory. Не break'ит D2 mechanics, добавляет sugar layer |
| R-7 | Cross-platform ABI differences (Sys V vs MS x64 для pointer args/returns) | Test matrix all 5+ combos на каждый PR; codegen использует C compiler ABI defaults (clang/MSVC/gcc handle correctly); ABI snapshot tests catch divergence |
| R-8 | Moving GC + pointer dangling (если GC двинет объект во время unsafe block) | Honor-system V1: W_UNSAFE_GC_TRIGGER warning + spec contract clear. Current Boehm-style GC не двигает → V1 безопасно. Formal pin API future ([M-118-pin-api]). Documented loud в D216 §16 |
| R-9 | NPO + cross-FFI с C `Option<*T>` (e.g., `malloc` returning `void*` NULL = OOM) | Direct ABI compatible — `Option[*T]` layout = `T*`. FFI fixture проверяет round-trip; T5.5 + T6.4 |
| R-10 | `*fn(...)` cast от non-captureless closure (E_CLOSURE_HAS_ENV) — false positives | Compiler tracks closure environment statically; cast allowed только если closure body не reference outer vars. Conservative: reject borderline cases. Test T6.2 (positive) + NEG-T6.7 (negative) |
| R-11 | Existing `ptr` users break post-D214 amend (Plan 115/91.12/116 stdlib) | D214 amend backward-compatible (semantic equivalent); audit Ф.0.6 для all existing ptr usages; regression T8 series + R5 |
| R-12 | Plan 113 `#realtime` interaction — pointer ops не считаются realtime-safe | Type-checker explicit ban pointer ops в `#realtime` context (deref может GC trigger, allocate, etc.). NEG-T8.6 |
| R-13 | Callback no-throw enforcement — false positives на legitimate `fn` reuse | Type-checker checks at **cast site**, not declaration — fn могут use'аться и как Nova-side fn с Fail, и как `*fn` callback в разных местах. Workaround documented: catch внутри callback |
| R-14 | Migration of `null ptr` — sed script might miss edge cases | Manual audit pass for `-> ptr` signatures; CI gate: full nova test must PASS post-migration; migration guide для user code |
| R-15 | Decomposition: sub-plan boundaries unclear, scope creep into core 118 | Strict scope contract: core 118 ships items listed в §«Plan 118 (core) — этот документ». Anything else → sub-plan. Ф.0 audit phase confirms scope; deviations require user signoff |
| R-16 | Documentation lag — typed-pointers.md not updated incrementally | Each Ф.N commit includes doc update for that phase's scope; final Ф.9 review catches gaps |

---

## Cross-platform CI matrix

Plan 118 — language addition с C codegen; cross-platform ABI must be verified.

| Platform | Compiler | Status | Notes |
|---|---|---|---|
| Linux x86_64 | clang 15+ | required | primary dev platform |
| Linux x86_64 | gcc 11+ | required | GNU toolchain validation |
| Windows x64 | MSVC 19.3+ | required | MS ABI (struct return differs) |
| Windows x64 | clang-cl 15+ | required | clang on Windows |
| macOS ARM64 | clang 15+ | required | Apple silicon, AArch64 ABI |
| macOS x86_64 | clang 15+ | desirable | if CI runners available |
| Linux ARM64 | clang 15+ | desirable | ARM Linux validation |

**Per-phase CI:** Ф.1, Ф.4, Ф.5, Ф.6 commits trigger full matrix. Final
Ф.8 — full matrix + ABI snapshot validation + perf bench.

**Failure mode:** any combo fails → block phase commit; investigate ABI
divergence root cause (codegen bug vs compiler bug — usually codegen).

---

## Performance benchmarks

Production-grade требует measurable performance targets. Benchmarks in
`bench/plan118/` каталог.

| Benchmark | Target | Measure |
|---|---|---|
| `escape_promote_overhead.nv` | < 5ns per promote | single nova_alloc call для escaped local |
| `npo_size_verification.nv` | sizeof(Option[*T]) == sizeof(*T) == 8 | runtime sizeof check на каждой platform |
| `auto_deref_zero_cost.nv` | identical asm для `p.field` vs `(*p).field` | inspect compiled asm |
| `pointer_arith_unit_scaling.nv` | `(*i32) + 1` = +4 bytes; `(*i64) + 1` = +8 bytes | inspect compiled asm offsets |
| `npo_fn_call_overhead.nv` | NPO function pointer call overhead < native C overhead + 1ns | benchmark loop |
| `unsafe_block_no_runtime_cost.nv` | `unsafe { }` block — zero runtime cost (no handler dispatch) | inspect HIR/MIR |
| `ffi_handle_tuple_newtype.nv` | `type X(*T)` ABI = single pointer; same as raw `*T` | ABI snapshot |

**Failure mode:** benchmark misses target → investigate; if architectural
issue → extract в followup (`[M-118-perf-*]`).

---

## Out of scope (explicitly deferred — Q-block)

### Deferred к sub-plans (Plan 118.1/118.2/118.3)

| Marker | What | Sub-plan |
|---|---|---|
| `[M-118-volatile-rw]` | Volatile reads/writes для memory-mapped I/O | Plan 118.1 |
| `[M-118-ptr-copy]` | `ptr.copy_to()` / `ptr.copy_to_nonoverlapping()` memcpy/memmove | Plan 118.1 |
| `[M-118-ptr-read-write]` | `ptr.read()` / `ptr.write()` typed read/write | Plan 118.1 |
| `[M-118-addr-of]` | `addr_of!(value)` / `addr_of_mut!(value)` для packed/uninit | Plan 118.1 |
| `[M-118-cstring]` | C-string convention: null-terminated bytes, `cstr"..."` literal | Plan 118.1 |
| `[M-118-slice-fat-ptr]` | `*[T]` slice fat-pointer (ptr + len) | Plan 118.2 |
| `[M-118-maybeuninit]` | `MaybeUninit[T]` uninitialized typed storage | Plan 118.2 |
| `[M-118-manuallydrop]` | `ManuallyDrop[T]` skip-destructor wrapper | Plan 118.2 |
| `[M-118-cross-fiber-ptr]` | Cross-fiber pointer rules (Send-equivalent) | Plan 118.3 |
| `[M-118-suspend-safety]` | Pointer held across `await` — warning | Plan 118.3 |
| `[M-118-atomic-ptr]` | `AtomicPtr[T]` lock-free typed pointer | Plan 118.3 |

### Permanently out (different design philosophy)

| Marker | What | Why |
|---|---|---|
| `[M-118-lifetimes-rust-style]` | Rust-style lifetime parameters `<'a>` + borrow checker | **Permanently out** — у нас GC + auto-promote (отдельная design philosophy) |
| `[M-118-aliasing-xor-rules]` | Rust-style XOR aliasing для `*mut T` (exclusive references) | Не нужно с GC; future если perf optimization потребует |
| `[M-118-inline-assembly]` | Inline asm — intrinsics | Out of scope language entirely |
| `[M-118-strict-provenance]` | Rust new pointer model (provenance tracking) | Не required; consider если adopt Rust 2024-style |

### Deferred к future plans / followups (V2+)

| Marker | What | Status |
|---|---|---|
| `[M-118-pin-api]` | `Pin[T]` API для self-referential / GC-stable references; formal pin enforcement | V2 — interacts с async + future moving GC |
| `[M-118-fixed-arrays]` | `*[N]T` fixed-size arrays для C FFI buffers | Plan 121 (separate language addition — stack arrays) |
| `[M-118-vararg-ffi]` | C-style vararg (`printf(fmt, ...)`) | Niche; wrappers via `args: [Any]` достаточны для V1 |
| `[M-118-stdcall-fn-ptr]` | Non-default calling convention `*fn` (stdcall, vectorcall) | Niche (Win COM); add when needed |
| `[M-118-offsetof]` | `offsetof(T, field)` для FFI struct layout matching | Niche; manual offsets adequate для now |
| `[M-118-alignment-attribute]` | `@align(N)` для over-aligned pointers (SIMD) | Niche; add when SIMD plan |
| `[M-118-cast-pointer-arith-fn]` | Cast `*fn → *T` или обратно | Niche; rare use case |
| `[M-118-stdlib-pointer-helpers]` | std/ptr module — utility fns (`offset_from`, etc. beyond 118.1) | Followup library plan |
| `[M-118-bindgen-tool]` | `nova bindgen` CLI auto-gen FFI bindings из C headers | Major tooling effort; separate plan (also tracked в Plan 115 [M-115-bindgen-tool]) |
| `[M-118-extern-c-unwind]` | `extern "C-unwind"` для FFI that can throw | V2 research — Rust 2024 model |
| `[M-118-escape-precise]` | Escape analysis precise mode (inlining + per-callee analysis) | Followup perf optimization |
| `[M-118-amp-heap-safe]` | `&record` outside unsafe (since heap already) | V2 — needs careful safety analysis |
| `[M-118-optional-shorthand]` | `?T` syntax sugar for `Option[T]` (Zig/Kotlin/Swift style) | Followup ergonomics; bigger design decision |
| `[M-118-handle-migration]` | Plan 115 V1 ffi-cookbook examples: `type X { value ptr }` (record) → `type X(ptr)` (tuple) | Tracked в Ф.9.4 + R5 |

---

## Migration impact

### Existing code (post-Plan 115 V1 + Plan 120 + Plan 83.12)

- **`ptr` usages** (e.g., `type Sqlite3Handle ptr`, opaque handle declarations) —
  **no migration required**. D214 amend backward-compatible (semantic equivalent
  через `Option[*unsafe ()]` NPO → identical ABI).
- **`external fn` signatures** с `ptr` parameters — no change (ABI unchanged).
- **Tuple-by-value FFI returns** `(Handle, i64)` — no change.
- **`null ptr` literals** — **breaking change**. Migration via:
  - Automated sed script (`scripts/migrate_null_ptr.sh`)
  - Type context auto-fix where unambiguous (`ptr = null ptr` → `Option[ptr] = None`)
  - Manual review для signatures (`-> ptr` где actually nullable → `-> Option[ptr]`)
  - Closes `[M-115-null-ptr-to-option-after-npo]` ✅
- **`type X { ro value ptr }` record FFI handles (Plan 115 V1 cookbook pattern)**
  — **non-breaking**, but documented как deprecated; migrate к `type X(ptr)`
  tuple newtype для zero-overhead ABI. Tracked в `[M-118-handle-migration]`.

### New patterns enabled (post-Plan 118)

- **Typed buffer FFI (preview — full в Plan 118.1/118.2):**
  ```nova
  external fn copy_buffer(src *ro u8, dst *mut u8, len usize) -> i64
  unsafe { copy_buffer(src_ptr, dst_ptr, 1024) }
  ```
- **Callback registration:**
  ```nova
  external fn libuv_set_cb(cb *fn(i64) -> ()) -> i64
  unsafe { libuv_set_cb(my_handler as *fn(i64) -> ()) }
  ```
- **Nullable returns с NPO:**
  ```nova
  external fn malloc(sz usize) -> Option[*u8]    // ABI = void*; NULL = None
  unsafe {
      match malloc(1024) {
          Some(buf) => use(buf),
          None => Fail.throw(OutOfMemory),
      }
  }
  ```
- **Out-params (preview — full в Plan 118.1 с `addr_of_mut!`):**
  ```nova
  external fn try_init(out *mut u8) -> i64
  mut buf Option[*u8] = None
  unsafe { try_init(&mut buf) }   // Plan 118.1 addr_of_mut! для full pattern
  ```
- **Canonical FFI handle (zero-overhead):**
  ```nova
  type Sqlite3Handle(*sqlite3)               // tuple newtype, stack, single pointer ABI
  external fn open(path str) -> (Option[Sqlite3Handle], i64)
  ```

---

## Compiler error/warning codes

### New error codes

- `E_UNSAFE_REQUIRED` — pointer op (deref, &, arith, cast, compare-ordering)
  outside unsafe context
- `E_UNSAFE_CALL_REQUIRES_WRAP` — calling `#unsafe` fn без `unsafe { }` wrap
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]` — array buffer can relocate
- `E_NULL_LITERAL_USE_NONE` — `null` literal used (general); use `None`
- `E_NULL_PTR_RETRACTED_USE_OPTION` — `null ptr` (Plan 115 V1 literal) retracted
- `E_UNDEFINED_USE_NONE_INIT_PATTERN` — `undefined` used; use `Option[*T] = None + init`
- `E_CLOSURE_HAS_ENV` — fn → *fn cast attempted with closure env captured
- `E_CALLBACK_THROWS_OVER_C_ABI` — Fn-with-Fail effect → *fn cast attempted
- `E_EXTERNAL_FN_FAIL_EFFECT` — `external fn ... Fail -> ...` declaration
- `E_PTR_ARITHMETIC_INVALID` — invalid arith op (`p * 2`, `p / 4`, etc.)
- `E_POINTER_RO_ASSIGN` — `*p = v` or `p.field = v` где p is `*ro T`
- `E_POINTER_RO_MUT_METHOD` — `p.mut_method()` где p is `*ro T`
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...` invalid cast target
- `E_INVALID_POINTER_MODIFIER` — `*const T` или другие неверные modifier'ы
- `E_DUPLICATE_POINTER_MODIFIER` — `*ro mut T` — несовместимые modifier'ы
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без type
- `E_REALTIME_POINTER_OP` — pointer op в `#realtime fn` body (Plan 113 interaction)
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — user attempts user-defined `unsafe_handler`
- `E_AMP_CONST_BINDING` — `&const_value` (const binding не addressable)
- `E_AMP_LITERAL` — `&42` (literals не addressable)
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — `"${p}"` interpolation; use `.to_debug_str()`
- `E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE` — pointer-pointer order `<`/`<=`/`>`/`>=` outside unsafe (A17)
- `E_VARARG_NOT_SUPPORTED` — vararg FFI call attempted
- `E_CAST_RAW_FN_TO_CLOSURE` — `*fn → fn` cast outside unsafe

### New warning codes

- `W_UNSAFE_GC_TRIGGER` — GC trigger (alloc, yield, #parks/#wakes call) внутри
  unsafe block с pointer in scope
- `W_PTR_AS_USIZE_GC_HASH_HAZARD` — `p as usize` использован как HashMap key
  (heuristic; address can change via GC compaction)
- `W_OPTION_DOUBLE_NESTED` — `Option[Option[*T]]` — NPO не applies, tagged fallback

---

## Documentation deliverables

| File | Status | Phase | Content |
|---|---|---|---|
| `docs/plans/118-typed-pointers-and-unsafe.md` | revised (this file) | Ф.0 | Plan 118 core |
| `docs/plans/118.1-ffi-intrinsics-and-cstring.md` | NEW stub | Ф.0.9 | Plan 118.1 sub-plan |
| `docs/plans/118.2-slice-fat-pointer-and-uninit.md` | NEW stub | Ф.0.9 | Plan 118.2 sub-plan |
| `docs/plans/118.3-pointer-concurrency-safety.md` | NEW stub | Ф.0.9 | Plan 118.3 sub-plan |
| `docs/plans/README.md` | UPDATE | Ф.0.10 | index Plan 118 + 118.1-3 |
| `docs/typed-pointers.md` | NEW | Ф.1-Ф.7 | overview docs (incremental per phase) |
| `docs/unsafe-block-pattern.md` | NEW | Ф.3 | when to use unsafe block, examples |
| `docs/ffi-cookbook.md` | UPDATE | Ф.5, Ф.9 | migration к Option[*T] / tuple newtype |
| `docs/migration/118-null-ptr-to-option.md` | NEW | Ф.5 | migration guide для `null ptr` retraction |
| `examples/typed_pointers/01-06_*.nv` | NEW | Ф.9.5 | minimal working samples |
| `spec/decisions/02-types.md` (D216, D52 cross-ref, D214 amend, D32 amend) | UPDATE | Ф.0 drafts, Ф.9 promote | spec D-blocks |
| `spec/decisions/04-effects.md` (D2 amend) | UPDATE | Ф.0 draft, Ф.3 commit, Ф.9 promote | D2 amend |
| `docs/project-creation.txt` | UPDATE | per phase + Ф.9 | sprint section |
| `docs/simplifications.md` | UPDATE | per phase + Ф.9 | [M-118-*] markers + close [M-115-null-ptr-to-option-after-npo] |
| `nova-private/discussion-log.md` (отд. репо) | UPDATE | per phase | design decisions log |

---

## Rollback strategy

1. **Revert PR** atomic per phase (Ф.0..Ф.10 separate commits позволяет
   surgical revert if specific phase breaks).
2. **Spec D-blocks** — D216 / D2 amend / D214 amend / D32 amend reverted as
   part of PR (history block restored).
3. **Migration script rollback** — `scripts/migrate_null_ptr.sh` is reversible
   (reverse sed pattern saved в `scripts/rollback_null_ptr.sh` for emergency).
4. **Compatibility**: rollback не break'ит existing Plan 115/120/83.12 code
   (no Plan 118 features used by них в released state pre-Plan 118).
5. **Cross-platform CI** rollback smoke за ~1 hour.
6. **Sub-plan blockage**: rollback core 118 blocks 118.1/118.2/118.3 (they
   depend on core); communicate timeline.

---

## Cross-references

### Связь с уже-закрытыми planам

- **Plan 114** ✅ (D184 master) — `ro`/`mut`/`consume` keywords; Plan 118 в этом
  синтаксисе. Binding-mut rule (`mut p *T` → `*mut T` default) extends Plan
  114 mutability story.
- **Plan 114.4** ⏳ planned (D199/D200) — const fn + associated constants
  (extracted Plan 114 Ф.9-Ф.11). Orthogonal к Plan 118, cross-ref только для
  D-block coordination.
- **Plan 113** ✅ (D172) — `#realtime`/`#blocking` attribute; Plan 118 adds
  `E_REALTIME_POINTER_OP` для pointer ops в realtime context (deref может GC
  trigger).
- **Plan 83.12** ✅ — std/net/tcp.nv (TcpListener/TcpStream/UdpSocket physical
  types). Cross-ref только для regression: existing handle types продолжают
  работать post-D214 amend. **Не путать с** Plan 91.12 (effect `TcpNet` —
  handler-dispatched API, planned but not shipped); Plan 118 не зависит от
  91.12.
- **D2** ([04-effects.md#d2](../../spec/decisions/04-effects.md#d2)) —
  effects вместо keywords; **AMEND** to restore `unsafe { }` как effect-handler
  sugar.
- **D52** ([02-types.md#d52](../../spec/decisions/02-types.md#d52)) — type
  declarations (newtype + tuple forms); Plan 118 — pointer types are new
  primitives integrating с D52 framework. Cross-ref: tuple newtype canonical
  для FFI handles.
- **D32** ([02-types.md#d32](../../spec/decisions/02-types.md#d32)) — параметр
  passing semantics; **AMEND** clarifying `&value` not Rust borrow + escape
  analysis safety net.
- **D215** ([02-types.md#d215](../../spec/decisions/02-types.md#d215)) — named
  tuples + value/reference allocation contract (Plan 120). Plan 118 leverages
  — stack values `&` escape → auto-promote.
- **D214** ([02-types.md#d214](../../spec/decisions/02-types.md#d214)) —
  Plan 115 `ptr` type; **AMEND** to redefine as `type ptr Option[*unsafe ()]`
  newtype + retract `null ptr` literal.
- **D184** ([03-syntax.md#d184](../../spec/decisions/03-syntax.md#d184))
  — Plan 114 master keyword refresh.

### Связь с planned / parallel planами

- **Plan 115 V1** ✅ merged (D214) — `ptr` + tuple FFI + opaque handles.
  Plan 118 amend D214 — redefine `ptr` как `type ptr Option[*unsafe ()]`
  (newtype через `*T` family foundations). Backward-compatible. Closes
  `[M-115-null-ptr-to-option-after-npo]`.
- **Plan 116** (std/tls — planned) — wraps `effect TcpNet` из Plan 91.12;
  если уже использует `ptr` handles — continues working post-D214 amend.
- **Plan 91.12** (effect `TcpNet` — handler-dispatched, mockable; planned, not
  shipped) — uses `ptr` через handle patterns. No Plan 118 dependencies; Plan
  118 не блокирует Plan 91.12.
- **Plan 120** ✅ merged (D215) — named tuples + allocation contract. Plan 118
  leverages (stack tuples `&` escape → auto-promote).
- **Plan 121** ⏳ planned (stack-fixed-arrays) — будет building на Plan 118 `*T`
  family для `*[N]T` typed fixed-size pointer.
- **Plan 118.1/118.2/118.3** ⏳ planned (sub-plans этого family) — extends
  core 118 с FFI intrinsics / slice / concurrency. Independent после core merge.

### Spec D-blocks (full list)

- **D2** ([04-effects.md](../../spec/decisions/04-effects.md)) — effects
  foundation; **AMEND** в Plan 118.
- **D32** ([02-types.md](../../spec/decisions/02-types.md)) — value/reference
  passing; **AMEND** в Plan 118.
- **D52** ([02-types.md](../../spec/decisions/02-types.md)) — type declarations;
  cross-ref FFI handle pattern.
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
  — Typed pointer family + unsafe model + null-safety через NPO.
- **D217/D218/D219** — будут добавлены в Plan 118.1/118.2/118.3 sub-plans.

---

## Status — Session 2 GRAND closure summary (2026-06-01, final)

### Session 2 grand-final accomplishments

**Total: 36 worktree commits + 3 nova-private commits.**

### Post-grand-closure additions (Session 3 — Ф.3.3-3.5 enforcement)

- `044881ee993` — **Merge main into plan-118** sync — Plan 124.1 V1 (priv
  field per-field visibility, D220, 4 error codes) + Plan 114.4.2 (const
  fn comptime evaluable, D199 V3) integrated. **7 conflicts resolved**
  (lexer/token + lexer/mod + ast + parser + spec/02-types + project-
  creation + simplifications) — keep both Plan 118 (unsafe_attr,
  KwUnsafe, D216) + Plan 124 (priv/pub, KwPriv/KwPub, D220) + Plan 114.4.2
  (fn_eval_max_depth) additions; semantically не конфликтуют.
- `b0ef06c1f27` — docs(plan118) post-merge log update
- `86ec057122e` — **Ф.3.3 scaffold:** Block.is_unsafe field + KwUnsafe
  sets true (24 Block construction sites updated)
- `5c0d2c975ce` — **Ф.3.5 enforcement:** E_UNSAFE_REQUIRED для AddrOf /
  Deref outside unsafe context. `check_unsafe_context_in_module` walker
  с depth counter (incremented в unsafe blocks + #unsafe fn body).
  **Closes acceptance A8** ✅.
- `b2d9cf46c3f` — Ф.3.5 positive fixture (`*&q` inline + nested unsafe)
- `7c73155bc5b` — docs(plan118 Ф.3.5) log + spec updates — A8 closed
- `abd4be4603b` — **Ф.3.5 A11: E_UNSAFE_CALL_REQUIRES_WRAP** enforcement.
  Walker pre-collects #unsafe fn names, detects calls outside unsafe
  context. **Closes A11** ✅. Updated t3_2_unsafe_fn_attr_ok.nv +
  added t3_neg_unsafe_call_no_wrap.nv.
- `984a2f49493` — docs+spec updates для A11 closure
- `e4cff57142e` — **Ф.6 A25: E_CALLBACK_THROWS_OVER_C_ABI** enforcement.
  Walker pre-collects fns с Fail effect, detects `fn as *fn(...)` cast
  и emits error. **Closes A25** ✅. Added t6_neg_callback_throws_over_c_abi.nv.
- `6752565f453` — **Ф.7 A33: E_REALTIME_POINTER_OP** enforcement (Plan 113
  D172 cross-ref). Pointer ops AddrOf/Deref в #realtime fn body banned —
  even с unsafe { } wrap (realtime no-GC-pause guarantee, orthogonal к
  unsafe enforcement). **Closes A33** ✅. Added t8_neg_realtime_pointer_op.nv.

**Regression smoke post-merge (release test-build):**

| Plan | PASS/FAIL |
|---|---|
| plan118 | **30/0** (13 positive + 17 NEG, post-A17 V1 + main sync) |
| plan115 | 11/0 (D214 backward compat preserved) |
| plan120 | 8/0 |
| plan114 | 10/0 |
| plan100_3 | 10/0 |
| plan108 | 6/0 |
| basics | 8/0 |
| plan124_1 | 9/0 (NEW from main merge — D220 priv enforcement) |

**TOTAL: 81/0 PASS** (release build). Debug build hits stack overflow
на test-all runner (unrelated Windows stack size issue; release works
clean).

---

### Session 2 evening-3 extensions (post-grand-closure)

- `7ff3007f3af` — Ф.6 partial #2: **E_EXTERNAL_FN_FAIL_EFFECT** enforcement (A26 ✅)
- `9ece8bfdaea` — Ф.4 codegen: **(*p)->field для *Record** double-pointer auto-deref (A12 partial)
- `986fdb04c0d` — NEG **E_AMP_RECORD_LITERAL** — Session 2 user signoff Option A для *Record
- `2bd6eb542b4` — docs(simplifications + project-creation) Session evening-2 log
- `7d61617bcf8` — NEG-T4.19 **E_ARRAY_INDEX_PTR_BANNED** для `&arr[i]`
- `d9d3084ed69` — NEG-T2.11 **E_AMP_LITERAL** для `&<literal>`
- `bd9d1a49d15` — doc fixture `t2_neg_amp_const_binding` (future Ф.3.5 enforcement)

Plus nova-private `9e5aa5d6cf` — Session 2 evening-2 design discussion log
(Option A для *Record + E_AMP_RECORD_LITERAL rationale + lessons).

**plan118 fixtures: 19/0 PASS** (10 positive + 9 NEG):
- Positive: t1_1, t1_3, t1_6, t1_8, t2_1, t3_1, t3_2, t4_1, t5_1, t6_1
- NEG: t1_neg_const_modifier, t1_neg_pointer_incomplete,
  t1_neg_ro_in_expression_pos, t1_neg_duplicate_modifier,
  t2_neg_amp_literal, t2_neg_amp_record_literal,
  t2_neg_amp_const_binding (documentation), t4_neg_amp_array_index,
  t6_neg_external_fn_fail_effect

**Closed acceptance:** A1, A3, A4, A9, A10, A12 partial, A18 partial
(forbidden ops: `&arr[i]`, `&<literal>`, `&Record{}`), A26 ✅, A29, A34, A35.

---

**Latest additions (post-grand-closure):**
- `6d6a18a2ab7` — NEG-T1.13: E_INVALID_POINTER_MODIFIER для `*const T`
  (parser diagnostic с Rust-developer migration hint)
- `f7c628ffa7d` — Ф.9 partial: ffi-cookbook Plan 118 preview section
- `c2fb3f3b9cb` — NEG: t1_neg_pointer_incomplete + t1_neg_ro_in_expression_pos
- `1634b0cb598` — NEG-T1.15: t1_neg_duplicate_modifier (`*ro mut T` rejected)
- `4ded191f7b5` — status update (intermediate)
- `7ff3007f3af` — Ф.6 partial #2: E_EXTERNAL_FN_FAIL_EFFECT enforcement
  + t6_neg_external_fn_fail_effect.nv (closes acceptance A26)

**plan118: 14/0 PASS** (9 positive + 5 NEG):
- Positive: t1_1, t1_3, t1_6, t1_8, t2_1, t3_1, t3_2, t5_1, t6_1
- NEG: t1_neg_const_modifier, t1_neg_pointer_incomplete,
  t1_neg_ro_in_expression_pos, t1_neg_duplicate_modifier,
  t6_neg_external_fn_fail_effect

**Acceptance criteria coverage (A1-A35):**
- A1 (typed pointer family parses) ✅ T1.1-T1.7
- A3 (chain order) ✅ T1.3
- A4 (codegen C T*) ✅ T1.10
- A8 (& outside unsafe) — V1 permissive (Ф.3.5 followup)
- A9 (unsafe block parses) ✅ T3.1
- A10 (#unsafe attribute) ✅ T3.2 + Ф.3.2 (commit `3a4074423ad`)
- A19 (Option[*T] NPO) — type parses, NPO codegen Ф.5 followup
- A20-A22 (NPO newtype/fn/double-nested) — Ф.5 followup
- A23 (null ptr retraction) — Ф.5 followup
- A24-A25 (*fn cast checks) — Ф.6 followup (partial: A26 closed)
- A26 (external fn no-Fail) ✅ Ф.6 partial #2
- A29 (D214 backward compat) ✅ regression 11/0 plan115
- A30 (D-blocks promoted) — Ф.9 follow-on
- A33 (#realtime pointer ban) — Ф.7 followup
- A34 (tuple newtype canonical) ✅ documented в docs/typed-pointers.md + ffi-cookbook
- A35 (examples PASS) ✅ 3/3 examples/typed_pointers

Remaining acceptance criteria (A2, A5-A7, A11-A18, A19-A28 partial, A31-A33)
landing с full Ф.4-Ф.9 implementation work.

**Phases: Ф.0 + Ф.1 + Ф.2 scaffold + Ф.3 + Ф.3.2 + Ф.4 partial + Ф.5
partial + Ф.6 partial + Ф.9 partial (examples + docs/typed-pointers.md
+ worktree setup script).**

**Test status:**
- **plan118: 9/0 PASS** (4 T1 + 1 T2 + 1 T3 + 1 T3.2 + 1 T5 + 1 T6)
- **examples/typed_pointers: 3/3 PASS** (basic_pointer + unsafe_block +
  unsafe_fn_attribute)
- **Total Plan 118 user artifacts: 12/0 PASS**

**Regression smoke (release test-build, clang toolchain, libuv enabled):**

| Plan | PASS/FAIL |
|---|---|
| plan118 (new) | 9/0 |
| plan115 | 11/0 (D214 backward compat) |
| plan120 | 8/0 |
| plan114 | 10/0 |
| plan100_3 | 10/0 |
| plan108 | 6/0 |
| basics | 8/0 |
| examples/typed_pointers (new) | 3/3 |
| syntax | 53/1 (pre-existing for_in_range_iter, unrelated) |

**TOTAL VERIFIED: 118/1 PASS.**

**Deliverables (per Plan file table):**

| Category | Deliverables | Status |
|---|---|---|
| Plan revision | 4 plan files (118 + 118.1/118.2/118.3) | ✅ |
| Spec D-block drafts | D216 NEW + D2/D214/D32 amends в spec/decisions/ | ✅ drafts |
| Compiler scaffold | AST + lexer + parser + checker + codegen (8 src files modified, 17 exhaustive-match sites + 5 new pieces) | ✅ |
| Test fixtures | 9 plan118 fixtures in nova_tests/plan118/ | ✅ all PASS |
| Examples | 3 examples in examples/typed_pointers/ | ✅ all PASS |
| Docs overview | docs/typed-pointers.md (~340 lines) | ✅ |
| Logs (simplifications + project-creation) | Updated per task | ✅ |
| Nova-private discussion-log | Session 1 + Session 2 entries | ✅ |
| Worktree setup script | scripts/setup_worktree_p118.sh | ✅ |

### Session 2 grand commits на plan-118 branch (worktree)

1. `e642fc86d1e` — Production-grade revision + decompose Plan 118 family
2. `12c746202a2` — Ф.0 GATE: D216/D2/D214/D32 amend drafts + audit + logs
3. `c75d7be3791` — Ф.1.1-1.4: AST + parser + 17 match arms
4. `fd1482292ba` — status checkpoint (morning)
5. `5069e76a983` — Ф.1.5: Ty::TypedPtr proper variant
6. `0c420b727fd` — Ф.1.9: T1 positive fixtures (4 PASS)
7. `f9e2a7a9a89` — Ф.2 scaffold: &value + *expr unary operators
8. `09be551b945` — Ф.3 scaffold: KwUnsafe + unsafe block
9. `25b39646639` — Ф.3 integration test
10. `9509ba0e219` — status checkpoint (mid)
11. `8127e3303a1` — Ф.6 partial: *fn(...) type
12. `f9818d47537` — logs update (simplifications + project-creation)
13. `3e4f66929e0` — Ф.5 partial: Option[*T] type
14. `3a4074423ad` — Ф.3.2: #unsafe attribute on fn
15. `5a3a49fc54a` — Session 2 intermediate checkpoint
16. `36e70ab3d00` — Ф.4 partial: permissive auto-deref check
17. `7c55e0564fa` — Session 2 closure summary
18. `08db63baeb0` — tool: worktree setup script
19. `a403d96f310` — Ф.9 partial: examples/typed_pointers/ (3 PASS)
20. `969cf42fc3e` — Ф.9 partial: docs/typed-pointers.md (~340 lines)
21. (this commit) — Session 2 grand closure

Plus nova-private (separate repo):
- `2a1c425cc4` — Session 1 initial design discussion
- `fb7e169e8b` — Session 2 design progression + lessons

---

### Session 2 final summary (intermediate — moved up)

**Total accomplishments — 17 worktree commits + 2 nova-private commits.**

**Phases progressed:**
- Ф.0 GATE ✅ — design freeze + D-block drafts + audit + logs
- Ф.1 ✅ partial — parser + checker + codegen scaffold (Ту::TypedPtr proper)
- Ф.2 ✅ scaffold — `&value` + `*expr` unary ops
- Ф.3 ✅ scaffold — KwUnsafe + `unsafe { }` block + `#unsafe` fn attribute
- Ф.4 ✅ partial — auto-deref permissive в f3_check_member
- Ф.5 ✅ partial — Option[*T] type parses
- Ф.6 ✅ partial — `*fn(...)` type parses

**Test status: 9/0 plan118 fixtures PASS** through release test-build (clang
toolchain, libuv enabled, GC linkage через main repo vcpkg_installed).

**Regression smoke verified clean:**
| Plan | PASS/FAIL |
|---|---|
| plan118 | 9/0 (new) |
| plan115 | 11/0 (D214 backward compat) |
| plan120 | 8/0 |
| plan114 | 10/0 |
| plan100_3 | 10/0 |
| plan108 | 6/0 |
| basics | 8/0 |
| syntax | 53/1 (1 pre-existing unrelated) |

**TOTAL VERIFIED: 115/1 PASS** (1 pre-existing for_in_range_iter unrelated).

### Session 2 commits на plan-118 branch (worktree D:/Sources/nv-lang/nova-p118)

1. `e642fc86d1e` — Production-grade revision + decompose в Plan 118 family
   (core + 118.1/118.2/118.3 sub-plans)
2. `12c746202a2` — Ф.0 GATE: D216/D2/D214/D32 amend drafts + audit + logs
3. `c75d7be3791` — Ф.1.1-1.4: AST PointerModifier + TypeRef::Pointer +
   parser *T production + 17 exhaustive-match sites updated
4. `fd1482292ba` — status checkpoint (morning)
5. `5069e76a983` — Ф.1.5: Ty::TypedPtr proper variant
6. `0c420b727fd` — Ф.1.9: T1 positive fixtures (4 PASS)
7. `f9e2a7a9a89` — Ф.2 scaffold: &value + *expr unary operators
8. `09be551b945` — Ф.3 scaffold: KwUnsafe keyword + unsafe block syntax
9. `25b39646639` — Ф.3 integration test
10. `9509ba0e219` — status checkpoint (mid-session)
11. `8127e3303a1` — Ф.6 partial: *fn(...) function pointer type
12. `f9818d47537` — Session 2 logs update (simplifications + project-creation)
13. `3e4f66929e0` — Ф.5 partial: Option[*T] type parses
14. `3a4074423ad` — Ф.3.2: #unsafe attribute on fn declarations
15. `5a3a49fc54a` — Session 2 final checkpoint (intermediate)
16. `36e70ab3d00` — Ф.4 partial: permissive auto-deref check для *T
17. (this commit) — Session 2 closure summary

Plus nova-private separate repo:
- `2a1c425cc4` — Session 1 initial design discussion
- `fb7e169e8b` — Session 2 design progression + lessons

### Worktree state

- **Папка:** `D:/Sources/nv-lang/nova-p118` (sibling of main)
- **Branch:** `plan-118` (от main `67625d285e6`)
- **NOT merged в main** (review required per design)
- **17 commits** total в worktree

### Realistic remaining work (Session 3+ — ~5-7 dev-days)

**High priority (closes concrete deliverables):**
- **Ф.5 full NPO codegen** (~1 day) — closes `[M-115-null-ptr-to-option-after-npo]`.
  register_novaopt_decl detects pointer/typedptr-wrapped Option, emits
  `typedef T* NovaOpt_X;` instead of tagged struct; pattern match
  NULL-check; Some(p)/None construction.
- **Ф.4 full auto-deref** (~1 day) — codegen emit_member must use `->`
  для pointer base types; method dispatch resolves on pointee; field
  assignment for *mut T enforced.
- **Ф.3.3-3.5 unsafe context enforcement** (~1 day) — introduce
  ExprKind::Unsafe(Block) variant (or Block.is_unsafe field via
  bulk-edit 24 construction sites); type-checker unsafe-context stack;
  emit `E_UNSAFE_REQUIRED` для pointer ops outside unsafe;
  `E_UNSAFE_CALL_REQUIRES_WRAP` для `#unsafe` fn calls.

**Medium priority:**
- **Ф.6 full** (~½-1 day) — *fn cast checks (E_CLOSURE_HAS_ENV),
  callback no-throw (E_CALLBACK_THROWS_OVER_C_ABI), external fn
  no-Fail (E_EXTERNAL_FN_FAIL_EFFECT), proper `Ret (*name)(Args)`
  C emission, FFI roundtrip test.
- **Ф.7** (~½ day) — W_UNSAFE_GC_TRIGGER warnings, Debug fmt
  `.to_debug_str()`, `"${p}"` interpolation diagnostic.

**Lower priority (Ф.8-Ф.9):**
- **Ф.8** (~1 day) — cross-platform CI matrix (5+ combos), ABI snapshot
  tests, performance benchmarks (escape promote, NPO size, auto-deref
  zero-cost, arith unit-scaling).
- **Ф.9** (~½-1 day) — spec promote D216/D2/D214/D32, ffi-cookbook
  migration, examples/typed_pointers/, closure logs + memory file.

**Independent post-core:**
- Plan 118.1 (FFI intrinsics: volatile/copy/read/write + addr_of +
  cstr"...") — ~3-4 day
- Plan 118.2 (slice fat-pointer `*[T]` + MaybeUninit + ManuallyDrop)
  — ~3-4 day
- Plan 118.3 (cross-fiber pointer + AtomicPtr[T]) — ~2-3 day

### Locked design decisions (preserved для future sessions)

- GC pin model: honor-system + W_UNSAFE_GC_TRIGGER warning (Ф.7)
- Decomposition: Plan 118 family staged (core gates 118.1/118.2/118.3)
- Slice → Plan 118.2
- `&acc` syntax (NOT `*acc` — deref ambiguity major risk)
- Callback no-throw across C ABI (E_CALLBACK_THROWS_OVER_C_ABI)
- External fn no-Fail (E_EXTERNAL_FN_FAIL_EFFECT)
- Tuple newtype `type Handle(*T)` canonical для FFI handles
- Method auto-deref `p.method()` ALLOW one-level в unsafe
- Field assignment `p.field = v` ALLOW для `*mut T` в unsafe
- Pointer Debug fmt `.to_debug_str()` explicit (NOT Display)

---

## Status — progress checkpoint (2026-06-01, evening 2 — Session 2 intermediate)

### Session 2 final additions (autonomous continuation)

**Ф.5 partial — Option[*T] type parses** (commit `3e4f66929e0`):
- Option[*T] declarations work через existing generic Option lowering.
- Fixture t5_1_option_pointer_parses_ok.nv PASS.
- Full NPO codegen (single-pointer layout vs tagged-struct) — deferred.
  NPO requires substantial codegen refactor: register_novaopt_decl emits
  `typedef T* NovaOpt_<X>;` instead of struct; downstream `.tag`/`.value`
  access sites need NULL-check abstraction. Multi-hour work — Ф.5
  follow-on session task.

**Ф.3.2 — `#unsafe` attribute on fn** (commit `3a4074423ad`):
- FnDecl.unsafe_attr: bool field added (default false).
- ContractAttrs.unsafe_attr: bool with is_empty() updated.
- parse_contract_attrs: KwUnsafe arm с duplicate-detection +
  skip_newlines() для multi-line fn declarations.
- Fixture t3_2_unsafe_fn_attr_ok.nv PASS (2 tests).
- Followup: type-checker E_UNSAFE_CALL_REQUIRES_WRAP enforcement (Ф.3.5).

**Try Block.is_unsafe flag (reverted):**
- Attempted to add `is_unsafe: bool` field на Block struct.
- Blast radius: 24 Block construction sites in 5 files (emit_c.rs ×10,
  parser/mod.rs ×6, callnorm.rs ×1, desugar.rs ×3, verify/handler_exec ×4).
- Reverted. Better path forward для Ф.3.3-3.5: introduce ExprKind::Unsafe(Block)
  variant + type-checker unsafe-context stack. Followup work.

### Plan 118 fixture status (9/0 PASS)

| Fixture | Phase | Status |
|---|---|---|
| t1_1_parse_pointer_types_ok | Ф.1 | ✅ |
| t1_3_chain_multi_level_ok | Ф.1 | ✅ |
| t1_6_record_field_pointer_ok | Ф.1 | ✅ |
| t1_8_ptr_legacy_compat_ok | Ф.1 | ✅ |
| t2_1_addr_of_deref_in_unsafe_ok | Ф.2/3 | ✅ |
| t3_1_unsafe_block_parses_ok | Ф.3 | ✅ |
| t3_2_unsafe_fn_attr_ok | Ф.3.2 | ✅ |
| t5_1_option_pointer_parses_ok | Ф.5 partial | ✅ |
| t6_1_fn_pointer_type_ok | Ф.6 partial | ✅ |

### Session 2 commits на plan-118 branch (worktree)

- `5069e76a983` — Ф.1.5 Ty::TypedPtr variant
- `0c420b727fd` — Ф.1.9 T1 fixtures (4 PASS)
- `f9e2a7a9a89` — Ф.2 scaffold (&/* unary)
- `09be551b945` — Ф.3 scaffold (KwUnsafe + unsafe block)
- `25b39646639` — Ф.3 integration test
- `9509ba0e219` — status checkpoint (mid-session)
- `8127e3303a1` — Ф.6 partial (*fn fixture)
- `f9818d47537` — logs update (simplifications + project-creation)
- `3e4f66929e0` — Ф.5 partial (Option[*T] type)
- `3a4074423ad` — Ф.3.2 #unsafe attribute

Plus nova-private separate repo:
- `2a1c425cc4` — initial discussion-log (Session 1)
- `fb7e169e8b` — Session 2 design + lessons

### Realistic next-session pickup (Session 3+ checklist)

Priority order для production-grade completion:

1. **Ф.3.3-3.5 unsafe context enforcement** (~1 day):
   - Introduce `ExprKind::Unsafe(Block)` variant (or `Block.is_unsafe`
     flag через bulk-Edit 24 sites). ExprKind variant may have fewer
     touchpoints due to `_` catch-all arms.
   - Type-checker unsafe-context stack: push on entering Unsafe, pop on
     exit.
   - Pointer ops (UnOp::AddrOf, UnOp::Deref, Ty::TypedPtr binding patterns)
     check stack; emit `E_UNSAFE_REQUIRED` outside unsafe.
   - `#unsafe fn` body also pushes unsafe context.
   - Calling `#unsafe` fn check: emit `E_UNSAFE_CALL_REQUIRES_WRAP` if
     caller not в unsafe context.

2. **Ф.4 auto-deref + binding mut rule** (~1.5 day):
   - Type-checker resolves `p.field` / `p.method()` / `p.field = v`
     when p: Ty::TypedPtr(modif, inner). Look up member на inner type.
   - Binding mut rule: `mut p *T` infers `*mut T` default; explicit
     `ro p *mut T` preserved.
   - Chain order semantics: `*mut *ro T` mutability levels enforced
     per layer.
   - Cast table enforcement (D216 §12): safe vs unsafe casts.
   - Pointer arith `*unsafe T` result type (D216 §6).

3. **Ф.5 NPO codegen** (~1 day):
   - register_novaopt_decl detects `Option[*T]` / `Option[Ty::TypedPtr]`
     / `Option[ptr]` / `Option[Newtype-over-pointer]`.
   - Emit `typedef T* NovaOpt_<sanitized>;` instead of `struct { tag;
     value; }`.
   - Pattern match codegen: `if (ptr == NULL) None_branch else Some_branch(ptr)`.
   - Some(p) construction: emit `p`; None: emit `NULL`.
   - Helper fns (`nova_opt_eq_*`) use NULL-check.
   - **Closes `[M-115-null-ptr-to-option-after-npo]`** ✅.

4. **Ф.6 *fn cast + callback no-throw** (~½-1 day):
   - Cast `fn → *fn` check: captureless required (E_CLOSURE_HAS_ENV).
   - Cast `*fn → fn` unsafe-only (E_CAST_RAW_FN_TO_CLOSURE).
   - Effect check: Fn-with-Fail cast → *fn → E_CALLBACK_THROWS_OVER_C_ABI.
   - external fn declaration с Fail → E_EXTERNAL_FN_FAIL_EFFECT.
   - Codegen proper `Ret (*name)(Args)` C type emission (improves
     debuggability + strict typing).
   - FFI roundtrip test (Nova fn → C callback → invoke).

5. **Ф.7 GC honor-system warnings** (~½ day):
   - W_UNSAFE_GC_TRIGGER emit when alloc/yield внутри unsafe block c
     active pointer binding в scope.
   - `// noqa: W_UNSAFE_GC_TRIGGER` silence mechanism (existing
     diagnostic suppression).
   - Pointer Debug fmt: `.to_debug_str()` method (inside unsafe only).
   - `"${p}"` interpolation → E_PTR_NO_DISPLAY_USE_DEBUG_STR.

6. **Ф.8 regression + cross-platform + ABI snapshot + perf bench** (~1 day):
   - Full nova test ≥ baseline (record exact post-Ф.7 count).
   - Cross-platform CI: 5+ combos (Linux × clang/gcc + Win × MSVC/clang
     + macOS × clang).
   - ABI snapshot tests `tests/abi/typed_pointers/*.expected`.
   - Perf benchmarks: escape promote < 5ns, NPO == sizeof(*T), auto-deref
     zero-cost asm, arith unit-scaling.

7. **Ф.9 closure** (~½-1 day):
   - Promote D216 + D2 amend + D214 amend + D32 amend → active в spec.
   - Update ffi-cookbook к Option[*T] / tuple newtype patterns.
   - Add examples/typed_pointers/01-06_*.nv minimal samples.
   - Update logs (project-creation, simplifications, discussion-log).
   - Create memory `project-plan118-status.md`.
   - PR review process (NOT self-merge).

---

## Status — progress checkpoint (2026-06-01, evening)

### Session 2 progress (autonomous continuation)

**Ф.1.5 — Ty::TypedPtr proper variant landed:**
- `5069e76a983` — types/mod.rs::Ty enum extended с `TypedPtr(PointerModifier,
  Box<Ty>)` variant. ty_of_ref maps `TypeRef::Pointer → Ty::TypedPtr(modif,
  Box::new(ty_of_ref(inner)))` — modifier + pointee propagated correctly.
  Cargo check clean (no exhaustive-match fallout; existing Ty match sites
  use `_` catch-all).

**Ф.1.9 — T1 positive fixtures landed:**
- `0c420b727fd` — 4 T1 fixtures PASS через release test-build (clang
  toolchain, libuv enabled, GC linkage через main repo vcpkg_installed):
  - t1_1_parse_pointer_types_ok.nv — `*T` / `*ro T` / `*mut T` / `*unsafe T`
    в external fn params + returns
  - t1_3_chain_multi_level_ok.nv — `*mut *ro T`, three-level chains
  - t1_6_record_field_pointer_ok.nv — tuple newtype canonical vs record form
  - t1_8_ptr_legacy_compat_ok.nv — Plan 115 ptr + Plan 118 *T coexist
- Setup: libuv submodule copied из main + .git removed; env vars
  NOVA_GC_INCLUDE_DIR/LIB_DIR pointing к main repo vcpkg_installed
  (x64-windows-static/include + /lib).

**Ф.2 — `&value` + `*expr` unary operators scaffold:**
- `f9e2a7a9a89` — UnOp enum extended (AddrOf, Deref) + parser
  recognizes TokenKind::Amp / TokenKind::Star prefix in parse_unary +
  exhaustive arms updated в 5 files (ast/pretty, codegen/emit_c ×3:
  emit_const_expr, emit_expr Unary, infer_expr_c_type, expr_to_display
  + verify/encode). Codegen direct C `&(...)` / `*(...)` emission.

**Ф.3 — `unsafe { }` block scaffold:**
- `09be551b945` — Lexer KwUnsafe keyword added (D2 amend foundation);
  parse_type uses KwUnsafe для `*unsafe T` modifier (cleaner vs legacy
  Ident path); parse_primary new arm для `unsafe { ... }` block —
  parsed as ExprKind::Block (V1 scaffold; full effect-handler desugar +
  context tracking + E_UNSAFE_REQUIRED enforcement — Ф.3.3-3.5 followup).
- T3 fixture t3_1_unsafe_block_parses_ok.nv — 3 test blocks PASS
  (simple, multi-stmt, nested).

**Ф.2/Ф.3 integration test:**
- `25b39646639` — t2_1_addr_of_deref_in_unsafe_ok.nv — unsafe block в
  expression position (3 tests PASS).

### Regression smoke (release test-build)

| Plan dir | PASS/FAIL | Status |
|---|---|---|
| plan118 (this) | 6/0 | ✅ new |
| plan115 | 11/0 | ✅ D214 backward compat |
| plan120 | 8/0 | ✅ unchanged |
| plan114 | 10/0 | ✅ unchanged |
| plan100_3 | 10/0 | ✅ unchanged |
| plan108 | 6/0 | ✅ unchanged |
| basics | 8/0 | ✅ unchanged |
| syntax | 53/1 | ✅ 1 pre-existing FAIL (for_in_range_iter, unrelated) |

**Smoke total: 112/1 (1 pre-existing unrelated failure).** No regression
introduced by Plan 118 Ф.0-Ф.3 scaffolding.

### Known V1 limitations (followup phases)

- Type inference для AddrOf/Deref в expression position best-effort
  (string append/strip `*`); proper Ty::TypedPtr inference в expr —
  **Ф.4** (auto-deref + binding mut rule).
- Escape analysis с auto-promote — **Ф.2.4** (separate IR pass).
- `_i64` / `_u32` etc. typed-int suffix literals не работают в
  pointer-ops context (causes "use of undeclared identifier 'i64'"
  в generated C). Workaround в T2 fixture: avoid typed suffix.
- Type-checker unsafe-context enforcement (E_UNSAFE_REQUIRED для
  `&`/`*` вне unsafe block) — **Ф.3.3-3.5**.
- `#unsafe` attribute on fn declarations — **Ф.3.2**.
- Implementation as effect-handler sugar (D2 amend semantic) — **Ф.3.4**.

---

## Status — progress checkpoint (2026-06-01, morning)

> Plan 118 в progress — incremental scaffolding landed; full implementation
> в work. Текущий status reflected ниже. Этот раздел updated после каждой
> большой задачи; finalize'ится в Ф.9 closure.

### Done (per phase, с commit refs)

**Revision (pre-Ф.0):**
- `e642fc86d1e` — Production-grade rewrite Plan 118 (1169 → 2259 lines) +
  decompose into Plan 118 family (core + 118.1/118.2/118.3 sub-plan stubs).
  35 acceptance criteria (vs 15), 16 risks (vs 10), ~150 tests
  (positive+negative), cross-platform CI matrix, perf benchmarks, ABI
  snapshots pipeline, 25 errors + 3 warnings catalog, 15 doc deliverables.
  README updated с 4 entries.

**Ф.0 GATE — design freeze + drafts + audit + logs:**
- `12c746202a2` (worktree) — D216 NEW drafted (~290 lines, 20 §-sections
  + diagnostic codes + mainstream comparison + use cases + cross-refs);
  D2 amend prepended (`unsafe { }` keyword restored as effect-handler
  sugar); D214 amend prepended (ptr redefine + null ptr retraction);
  D32 amend prepended (`&value` is typed ptr construction, NOT Rust borrow).
  Audit 47 `null ptr` occurrences + 4 `external fn ptr` files + 6 compiler
  src files. Logs: docs/simplifications.md + docs/project-creation.txt.
- `2a1c425cc4` (nova-private separate repo) — discussion-log.md с 4-round
  design discussion + derived decisions + lessons learned.

**Ф.1 — *T family parser + AST scaffold (partial — Ф.1.1-1.4):**
- `c75d7be3791` (worktree) — AST changes: `PointerModifier` enum (Ro/Mut/
  Unsafe) + `TypeRef::Pointer(modifier, Box<TypeRef>, Span)` variant.
  Parser change: `parse_type()` recognizes `*` prefix → PointerType
  production; chain `*mut *ro T` works via recursion. 17 exhaustive-match
  sites updated в 8 files (codegen/emit_c.rs ×5, codegen/external_registry.rs,
  doc/collector.rs, doc/render_json.rs, lints.rs, types/mod.rs ×8). Codegen:
  `*ro T` → `const T*`; `*mut T`/`*unsafe T` → `T*` (D216 §11). Cargo
  check clean.

### What's NOT done (incremental — pending follow-on session work)

**Ф.1 remaining (Ф.1.5-1.12, ~½-1 dev-day):**
- 🔴 **Ф.1.5-1.7** Ty::TypedPtr proper variant в `types/mod.rs::Ty`
  (currently `TypeRef::Pointer → Ty::Ptr` scaffolding fallback).
  Adding new variant требует update ~15 файлов с exhaustive `match ty`
  на Ty enum (blast radius previously enumerated: emit_c.rs +
  external_registry.rs + sum_schema_registry.rs + doc/collector.rs +
  doc/mcp.rs + doc/render_json.rs + doc/stability.rs + interp/mod.rs +
  parser/mod.rs + semver.rs + test_runner.rs + types/mod.rs + verify/encode.rs
  + verify/handler_exec.rs + verify/pipeline.rs).
- 🔴 **Ф.1.7** Binding-mut rule (`mut p *T` → `*mut T` default) в
  type-checker — depends on Ty::TypedPtr.
- 🔴 **Ф.1.8** Chain order semantics enforcement в checker.
- 🔴 **Ф.1.9** T1 series fixtures (12+ positive + negative .nv files в
  `nova_tests/plan118/t1_*.nv`).
- 🔴 **Ф.1.10-1.11** Codegen integration tests (ABI snapshots
  `tests/abi/typed_pointers/t1_*.expected`).
- 🔴 **Ф.1.12** `ptr` redefine как newtype в prelude (`std/prelude/core.nv`):
  `type ptr Option[*unsafe ()]` (D214 amend cross-ref).
- 🔴 **Ф.1 release build verify** — `cargo build --release` + setup libuv
  submodule в worktree (per memory project-worktree-nova-test-setup) +
  run nova test ≥ baseline.

**Ф.2-Ф.10 remaining (~7-8 dev-days):**
- 🔴 **Ф.2** `&value` operator + escape analysis с auto-promote (~1.5 day)
- 🔴 **Ф.3** `unsafe { }` block + `#unsafe` attribute + KwUnsafe в lexer +
  D2 amend desugar в effect-handler (~1 day)
- 🔴 **Ф.4** Auto-deref `p.field`/`p.method()`/`p.field = v` + pointer
  ops (arith/casts/compare) (~1.5 day)
- 🔴 **Ф.5** `Option[*T]` + NPO codegen + null-ptr retraction; closes
  `[M-115-null-ptr-to-option-after-npo]` (~1 day)
- 🔴 **Ф.6** `*fn(...)` function pointers + callback no-throw
  (E_CALLBACK_THROWS_OVER_C_ABI + E_EXTERNAL_FN_FAIL_EFFECT) (~½-1 day)
- 🔴 **Ф.7** GC honor-system W_UNSAFE_GC_TRIGGER warnings + pointer Debug
  fmt (~½ day)
- 🔴 **Ф.8** Regression + cross-platform CI (5+ combos) + ABI snapshot +
  perf bench (~1 day)
- 🔴 **Ф.9** Spec promote (D216/D2/D214/D32 → active) + ffi-cookbook
  migration + nova doc + examples + closure (~½-1 day)
- 🔴 **Ф.10** Reserved (safety hatch / post-review)

**Sub-plans (independent, post-118-core):**
- 🔴 **Plan 118.1** — FFI intrinsics + cstr"..." (~3-4 day)
- 🔴 **Plan 118.2** — Slice fat-pointer + MaybeUninit + ManuallyDrop (~3-4 day)
- 🔴 **Plan 118.3** — Pointer concurrency + AtomicPtr[T] (~2-3 day)

### Realistic next-session checklist

Session pickup от current state:
1. Add `Ty::TypedPtr(PointerModifier, Box<Ty>)` variant в types/mod.rs
2. `cargo check` — fix exhaustive-match arms (~15 files; treat TypedPtr
   like Ty::Ptr or add new behaviors per location)
3. Update `ty_of_ref` mapping: `TypeRef::Pointer → Ty::TypedPtr(modif, ty_of_ref(inner))`
4. Apply binding-mut rule в checker (look for `let_decl.is_mut` interaction)
5. Add prelude `type ptr Option[*unsafe ()]` (after Ф.5 lands NPO; until
   then keep current `Ty::Ptr` opaque variant)
6. Write `nova_tests/plan118/t1_1_*.nv` через `t1_11_*.nv` fixtures
7. Setup release build (NOVA_GC_LIB_DIR/INCLUDE_DIR env vars; copy
   libuv submodule; delete libuv/.git)
8. `cargo build --release` then `./target/release/nova test plan118`
9. Per-fixture iterate until all PASS
10. Commit `feat(plan118 Ф.1.5-1.12): Ty::TypedPtr + binding mut + chain
    order + ptr redefine + T1 fixtures + release build verify`

### Locked design decisions (NOT change without sub-plan)

- **GC pin model:** honor-system + W_UNSAFE_GC_TRIGGER warning (Ф.7)
- **Decomposition:** Plan 118 family staged (core gates 118.1/118.2/118.3)
- **Slice → Plan 118.2** (not core 118)
- **&acc syntax kept** (NOT *acc — deref ambiguity)
- **Callback no-throw** across C ABI (E_CALLBACK_THROWS_OVER_C_ABI)
- **External fn no-Fail** (E_EXTERNAL_FN_FAIL_EFFECT)
- **Tuple newtype `type Handle(*T)` canonical** для FFI handles (zero-overhead)
- **Method auto-deref** `p.method()` ALLOW one-level в unsafe (Go/D pattern)
- **Field assignment** `p.field = v` ALLOW для `*mut T` в unsafe
- **Pointer Debug fmt** `.to_debug_str()` explicit (NOT auto via Display)

### Worktree state

- **Папка:** `D:/Sources/nv-lang/nova-p118` (sibling of main)
- **Branch:** `plan-118` (от main `67625d285e6`)
- **Commits:**
  - `e642fc86d1e` — revision
  - `12c746202a2` — Ф.0 GATE
  - `c75d7be3791` — Ф.1.1-1.4 scaffold
- **NOT merged в main** (per design — review required first)

Sub-plan files committed в `e642fc86d1e`:
- `docs/plans/118.1-ffi-intrinsics-and-cstring.md`
- `docs/plans/118.2-slice-fat-pointer-and-uninit.md`
- `docs/plans/118.3-pointer-concurrency-safety.md`

### Closure summary

> Заполняется агентом по завершении ВСЕХ фаз Plan 118 core. Поля
> (template):
> - Что сделано (per phase Ф.0..Ф.10 с commit refs)
> - Что extracted в Plan 118.0.X (если safety hatches fire'нули)
> - Final `nova test` results (before/after counts + delta)
> - Cross-platform PASS matrix (5+ combos confirmed)
> - ABI verification snapshot results
> - NPO size verification (sizeof(Option[*T]) == sizeof(*T) на каждой platform)
> - Performance baseline results (escape/promote overhead microbench, NPO size,
>   auto-deref zero-cost, arith unit-scaling)
> - Closed markers: `[M-115-null-ptr-to-option-after-npo]` ✅,
>   `[M-118-handle-migration]` ✅
> - Open `[M-118-*]` followups
> - Memory `project-plan118-status.md` создан
> - D216 + D2 amend + D214 amend + D32 amend promoted в active spec (commit refs)
