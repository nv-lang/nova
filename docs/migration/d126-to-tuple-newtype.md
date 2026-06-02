# Migration Guide — D126 retract → `type X(ptr)` tuple-newtype

> **Audience:** authors migrating `external type X` declarations (D126) to
> the post-retract Nova syntax.
>
> **Created:** 2026-06-01 (Plan 91.12 V2). Authoritative reference for
> D126 hard retract.
>
> **Status of D126:** 🔴 RETRACTED 2026-06-01 (plain form). See
> [spec/decisions/03-syntax.md §D126](../../spec/decisions/03-syntax.md#d126-external-type--opaque-типы-без-body).

## TL;DR

```diff
- external type MyHandle           // plain D126 (retracted)
+ type MyHandle(ptr)                 // Plan 115 D214 tuple-newtype
```

```diff
- external type MyHandle consume    // D163 FFI opaque-consume (still works)
+ external type MyHandle consume    // unchanged — D163 supported
```

## Background

`external type X` (D126, Plan 62.D.bis, 2026-05-18) was a bootstrap form for
opaque runtime types: the Nova-level declaration carried only the name
(plus optional generics), and the implementation lived in `nova_rt/*.h`.

Through a year of operation три проблемы накопились:

1. **No type-system handshake.** Codegen had to special-case по имени
   (`emit_oncecell_instance`/`emit_lazy_instance`) для generic opaque
   types, потому что `external type X[T]` не нёс никакой структурной
   информации. Это вело к hardcoded per-type routing в Rust codebase.

2. **Better alternatives появились.** Plan 115 D214 (2026-06-01) ввёл
   `ptr` distinct typedef + tuple newtype `type X(ptr)` constructor +
   `.0` access. Это даёт **тот же** opaque-handle effect через стандартный
   type-system mechanism — без special-cases в codegen.

3. **D163 покрывает FFI use case.** `external type X consume` (Plan 100.5,
   D163) — для FFI resource handles типа `File consume`/`Mutex consume` —
   решает auto-cleanup + linearity tracking. Plain `external type X`
   (без consume) — bridge bootstrap, который больше не нужен.

После migration всех 5 stdlib D126 типов (см. ниже) plain `external type X`
retracted hard.

## Migration patterns

### Pattern 1 — Pure Nova record (для тонких обёрток над `[]u8`/cursor state)

**When:** тип — это тонкий wrapper над `[]u8` (buffer/cursor) или другой
Nova primitive. Не требует C-side struct.

**Before (D126):**

```nova
// std/prelude/collections.nv
export external type WriteBuffer
export external type ReadBuffer

// std/runtime/write_buffer.nv (декларации методов)
export external fn WriteBuffer.new() -> Self
export external fn WriteBuffer mut @write_byte(v u8) -> @
// ... 27 методов через external fn

// C runtime
// nova_rt/write_buffer.h — ~30 inline C функций
```

**After (Plan 91.12 V1):**

```nova
// std/prelude/collections.nv
export import std.runtime.write_buffer.{WriteBuffer}

// std/runtime/write_buffer.nv (тип + все методы на Nova-body)
#no_prelude
module runtime.write_buffer

export type WriteBuffer { mut buf []u8 }

export fn WriteBuffer.new() -> Self => { buf: []u8.with_capacity(16) }
export fn WriteBuffer mut @write_byte(v u8) -> @ { @buf.push(v); @ }
// ... 27 методов на Nova-body над `[]u8` primitives
```

**C runtime:** удалён (`nova_rt/write_buffer.h` deleted; Nova codegen
emits the struct via standard `emit_record_type`).

**Канонические примеры:**
- [StringBuilder] (Plan 109 D179) — `type StringBuilder consume { mut buf []u8 }`
- [WriteBuffer] (Plan 91.12 V1) — `type WriteBuffer { mut buf []u8 }` (non-consume, @into consume)
- [ReadBuffer] (Plan 91.12 V1) — `type ReadBuffer { ro data []u8, mut pos int }`

### Pattern 2 — Tuple newtype над `ptr` (для runtime-backed C structs)

**When:** тип имеет C-side struct в `nova_rt/*.h` (atomic state, fiber-aware
primitives, etc) и его нельзя reimplement в pure Nova без unsafe-y operations.

**Before (D126):**

```nova
// std/runtime/sync.nv
export external type Condvar
export external type OnceCell[T]
export external type Lazy[T]

export external fn Condvar.new() -> Self
export external fn Condvar @wait(m mut Mutex)
// ... methods на external fn

// C runtime
// nova_rt/sync_condvar.h — Nova_Condvar struct + methods
```

**After (Plan 91.12 V2):**

```nova
// std/runtime/sync.nv
export type Condvar(ptr)              // non-generic
export type OnceCell[T](ptr)          // generic — Plan 115 D214 form
export type Lazy[T](ptr)              // generic

// Methods ОСТАЮТСЯ через external fn:
export external fn Condvar.new() -> Self
export external fn Condvar @wait(m mut Mutex)
// ... unchanged API surface
```

**C runtime:** ОСТАЁТСЯ — `nova_rt/sync_condvar.h`, `sync_primitives.h`.
ABI unchanged. Codegen suppresses Nova-level `typedef nova_ptr Nova_Condvar`
emission (would conflict с runtime header's struct typedef) — see
`compiler-codegen/src/codegen/emit_c.rs` §`RUNTIME_BACKED_NEWTYPES`.

**Generic case (OnceCell[T]/Lazy[T]):** codegen routes Newtype kind per-T
mono to the same `emit_oncecell_instance`/`emit_lazy_instance` helpers
that previously fired для Opaque kind. The Newtype declaration carries no
body; per-T mono path emits the actual C struct + methods.

**Канонические примеры:**
- [OnceCell[T]] (Plan 91.12 V2) — `type OnceCell[T](ptr)`
- [Lazy[T]] (Plan 91.12 V2) — `type Lazy[T](ptr)`
- [Condvar] (Plan 91.12 V2) — `type Condvar(ptr)` (non-generic)

#### Generic user FFI handle (Plan 91.12 V2 followup, 2026-06-02)

Generic newtype над `ptr` поддерживается для user-level FFI:

```nova
type Region[T](ptr)                   // phantom T
type DualHandle[T, U](ptr)

ro r = Region[int](some_ptr)
ro d = DualHandle[int, str](raw)
ro inner = r.0                        // .0 access OK
```

Все monomorphizations (`Region[int]`, `Region[str]`) share C ABI
(`Nova_Region ≡ nova_ptr`); T — type-system fiction (compile-time
phantom discrimination, zero runtime overhead). См. spec D214
§«Generic opaque handle».

**Inner non-ptr types** (2026-06-02 followup, CLOSED):

```nova
type Counter[T](int)              // tagged int counter
type Tag[T](str)                  // typed string wrapper
type Flag[T](bool)                // typed bool flag
type Measure[T](f64)              // tagged f64 (e.g. seconds vs meters)
type Tagged[T, U](int)            // multi-param phantom
```

Все mono'd instances share single typedef over inner C type (phantom T
discrimination, identical runtime ABI). Use cases: typed counters,
Email/UserId strings, Visible/Hidden flags, measurement units.

**Inner uses generic param** (`type Wrap[T](T)`) — **REJECTED**:

```nova
// ✗ E_GENERIC_NEWTYPE_INNER_USES_PARAM
type Wrap[T](T)

// ✓ Use record form для per-T mono
type Wrap[T] { value T }
```

Tuple newtype = transparent typedef; per-T storage variance — это
record-semantics, не newtype.

### Pattern 3 — `external type X consume` (D163 FFI resource handle)

**When:** FFI resource handle (File, Socket, custom DB connection)
с auto-cleanup. **No change required** — D163 path unchanged after V2.

```nova
// User code — unchanged after Plan 91.12 V2:
external type FileResource consume
external type SqliteConnection consume

external fn FileResource.open(path str) -> Self
external fn FileResource consume @close() -> ()
```

This остаётся valid и encourages для FFI use case — `consume` keyword
триггерит linearity tracking + must-consume D131 obligation на user.

## Migration steps (user-side)

For each `external type X` (plain form, no consume) declaration:

### Step 1 — Decide migration pattern

- Тип — wrapper над `[]u8`/cursor/primitives? → **Pattern 1** (pure Nova record).
- Тип имеет C-side struct в `nova_rt/`? → **Pattern 2** (tuple newtype over ptr).
- FFI resource handle с cleanup? → **No change** (D163 still works).

### Step 2 — Replace declaration

**Pattern 1:**
```diff
- external type MyBuffer
+ type MyBuffer { mut buf []u8 }  // + методы на Nova-body
```

**Pattern 2:**
```diff
- external type MyHandle
+ type MyHandle(ptr)               // ABI-compatible с предыдущим void*
```

### Step 3 — Methods stay as external fn

External fn методы НЕ требуют migration:

```nova
external fn MyHandle.new() -> Self                    // unchanged
external fn MyHandle @some_op(arg int) -> bool         // unchanged
```

C ABI идентичен (handle = void* either way).

### Step 4 — Update import/re-export if needed

If your type was re-exported through prelude facade:

```diff
// std/prelude/collections.nv (или ваш module facade)
- export external type MyBuffer
+ export import std.runtime.my_buffer.{MyBuffer}
```

### Step 5 — Rebuild + test

`cargo build --release -p nova-cli` затем `nova test --filter <your-prefix>`.

## Migration steps (Compiler/stdlib-side, Plan 91.12 V2 reference)

For stdlib types использовавшие Pattern 2 (sync types), 4 codegen-level
изменения нужны были (см. `compiler-codegen/src/codegen/`):

1. **`emit_c.rs` `RUNTIME_BACKED_NEWTYPES` list:** для имён OnceCell/Lazy/
   Condvar emit_type_decl Newtype-branch skip'ает `typedef nova_ptr Nova_X`
   emission (conflict с runtime struct typedef иначе).

2. **`emit_c.rs` `emit_generic_type_instance` Newtype-branch:** routes
   `type X[T](ptr)` к existing `emit_oncecell_instance`/`emit_lazy_instance`
   helpers (per-T mono pipeline).

3. **`emit_c.rs` `type_decls` registration Newtype-branch:** runtime-backed
   newtypes регистрируются в `generic_types` + `generic_type_templates`
   (предусловие для `Lazy[int].new(closure)` static-method dispatch).

4. **`external_registry.rs` `from_module` type_decls collection:**
   `TypeDeclKind::Newtype(_) if !generics.is_empty() || is_runtime_backed`
   попадает в `type_decls` (parallel to `TypeDeclKind::Opaque`).

## Diagnostic

If you write plain `external type X` после Plan 91.12 V2 retract, the
type-checker emits:

```
[E_EXTERNAL_TYPE_RETRACTED] `external type` (D126) retracted by Plan 91.12 V2
(2026-06-01). Replace `external type X` with `type X(ptr)` (tuple-newtype
opaque-handle pattern, Plan 115 D214). C runtime backing preserved через
`external fn` методы — ABI unchanged.
Migration guide: docs/migration/d126-to-tuple-newtype.md.
For FFI opaque consume-types оставайся на `external type X consume` (D163,
supported).
```

## See also

- [Plan 91.12 plan doc](../plans/91.12-d126-retract.md) — V1+V2 closure.
- [Plan 115 plan doc](../plans/115-ptr-type-and-tuple-ffi.md) — D214 ptr + tuple newtype.
- [Plan 109 plan doc](../plans/109-stringbuilder-nova-type.md) — pure Nova record pattern.
- [spec D126](../../spec/decisions/03-syntax.md#d126-external-type--opaque-типы-без-body) — retract notice.
- [spec D214](../../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — tuple newtype.
- [spec D163](../../spec/decisions/02-types.md#d163-ffi-consume-integration--type-driven-без-отдельного-keywordа) — FFI consume types (unchanged).

[StringBuilder]: ../../std/runtime/string_builder.nv
[WriteBuffer]: ../../std/runtime/write_buffer.nv
[ReadBuffer]: ../../std/runtime/read_buffer.nv
[OnceCell[T]]: ../../std/runtime/sync.nv
[Lazy[T]]: ../../std/runtime/sync.nv
[Condvar]: ../../std/runtime/sync.nv
