<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 134 — Удалить встроенный тип `ptr`; заменить на `*()`

> **Создан:** 2026-06-09.  **Статус:** 📋 PLANNED.
> **Эстимат:** ~½ dev-day.  **Model:** Sonnet 4.6.
> **Зависит от:** Plan 118 (типизированные указатели `*T`).

---

## Что

Убрать `ptr` как встроенный тип из компилятора Nova. Заменить везде на `*()`.

`*()` = pointer-to-unit = `void*` в C — органично вписывается в существующую
систему типов `*T` без спецкейса в компиляторе.

### До / После

```nova
// БЫЛО
type FileHandle { ro value ptr }
external fn sqlite3_open(path str) -> (ptr, int)
ro p ptr = 0 as ptr

// СТАЛО
type FileHandle { ro value *() }
external fn sqlite3_open(path str) -> (*(), int)
ro p *() = 0 as *()
```

---

## Почему

- `ptr` = `void*` появился до Plan 118 (`*T`). Теперь `*()` выражает то же самое
  через существующую систему типов — без магии и спецкейса.
- `*()` самодокументируется: «pointer to nothing = opaque». Читателю не нужно
  знать что `ptr` — это алиас.
- Убирает `nova_ptr` typedef и весь связанный спецкод из compiler.
- Если нужно сокращение — `type ptr = *()` одной строкой в user/stdlib коде.

---

## Фазы

### Ф.1 — Compiler: `*()` → `void*`, убрать `ptr` (~1h)

- **Ф.1.1** `nova_rt.h`: убрать `typedef void* nova_ptr;`

- **Ф.1.2** `emit_c.rs`: добавить arm в C-type lookup для `*()`:
  ```rust
  // TypeRef::Pointer { inner: TypeRef::Unit } → "void*"
  ```
  Убрать маппинг `"ptr" => "nova_ptr"` и все `nova_ptr` упоминания.

- **Ф.1.3** `external_registry.rs`: убрать `"ptr" => "nova_ptr"`.

- **Ф.1.4** `emit_c.rs` ~line 15888: `NullPtrLit` генерирует `((nova_ptr)0)` →
  исправить на `((void*)0)` или через typed null `(*(())null)`.

- **Ф.1.5** Все cast-правила для `ptr` (`ptr as u64`, `u64 as ptr`, `ptr as bool` → error и т.д.)
  ~lines 28372-28381 в emit_c.rs — переключить на `*()`.

- **Ф.1.6** `emit_c.rs` ~line 23393: `nova_ptr_to_debug_str` — проверить, обновить.

- **Ф.1.7** NEG fixture: `let x ptr = ...` → error «type `ptr` removed; use `*()`»
- **Ф.1.8** POS fixture: `*()` в type position → компилируется, C-тип `void*`
- **Ф.1.9** POS fixture: `0 as *()` → null pointer cast работает

**Commit:** `feat(plan134 Ф.1): *() → void*, remove ptr builtin type`

### Ф.2 — Migration sweep (~1h)

**`nova_tests/plan115/`** (~6 файлов):
- `t1_ptr_casts_ok.nv` — `ptr` → `*()`
- `t1_ptr_arithmetic_neg.nv` — `ptr` → `*()`
- `t2_external_fn_tuple_ok.nv` — `ptr` → `*()`
- Остальные plan115 файлы с `ptr`

**`examples/ffi/`**:
- `ptr_basics.nv` — `ptr` → `*()`; переименовать в `ptr_unit_ok.nv` если нужно
- `sqlite_mini.nv` — `type Db { ro value *() }`, `type Stmt { ro value *() }`,
  `external fn nova_fn_sqlite3_open(...) -> (*(), int)`

**`std/runtime/sync.nv`**:
- `OnceCell[T](ptr)` → `OnceCell[T](*())`
- `Lazy[T](ptr)` → `Lazy[T](*())`
- `Condvar(ptr)` → `Condvar(*())`

**Commit:** `refactor(plan134 Ф.2): migrate ptr → *() (plan115 tests + examples + sync.nv)`

### Ф.3 — Spec + docs (~30 min)

- **Ф.3.1** `spec/decisions/02-types.md` — убрать `ptr` из таблицы типов.
  Добавить: «opaque pointer = `*()` (pointer to unit type → `void*` in C)».
- **Ф.3.2** `docs/plans/115-ffi-foundational.md` — пометить `ptr` type как
  superseded by `*()`.
- **Ф.3.3** `docs/simplifications.md` + project-creation.txt + discussion-log.

**Commit:** `docs(plan134 Ф.3): spec — ptr removed, *() documented as void* equivalent`

---

## Acceptance criteria

- **A-134.a** — `ptr` в Nova-коде → compile error «type `ptr` removed; use `*()`»
- **A-134.b** — `*()` в type position → C-тип `void*`
- **A-134.c** — `0 as *()` → null pointer, работает
- **A-134.d** — `nova_ptr` typedef удалён из nova_rt.h
- **A-134.e** — plan115 тесты PASS с `*()`
- **A-134.f** — `OnceCell[T](*())` / `Lazy[T](*())` / `Condvar(*())` компилируются
- **A-134.g** — 0 regressions в full `nova test`

---

## C-кодогенерация

```
*()  →  void*
```

Cast rules (миграция с `ptr`):
- `*() as u64` / `u64 as *()` → разрешено (через `(uintptr_t)` cast)
- `*() as int` / `int as *()` → разрешено (через `(intptr_t)` cast)
- `*() as bool` → запрещено (E_PTR_CAST_INVALID_TARGET)
- `*() as str` → запрещено

## Что НЕ меняется

- `*T` / `*mut T` — типизированные указатели (Plan 118) — без изменений
- `*u8` — байтовый pointer (часто используется в FFI вместо `void*`) — без изменений
- `null as *T` syntax — без изменений

## Followup

- `[M-134-stdlib-ptr-alias]` — если `*()` везде читается шумно, рассмотреть
  `type ptr = *()` в prelude (опционально, пользовательское решение).
