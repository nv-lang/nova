<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 134 — Удалить встроенный тип `ptr`; заменить на `*()`

> **Создан:** 2026-06-09.  **Статус:** ✅ CLOSED 2026-06-09 (Ф.1–Ф.3 merged в main);
> refinement-проход 2026-06-14 (ветка `plan-134`) — ранний `nova check`-reject
> через `E_TYPE_UNKNOWN` + миграционный hint, fixture-полировка, doc-финализация.
> **Эстимат:** ~½ dev-day.  **Model:** Sonnet 4.6 (impl) + Opus 4.8 (refinement).
> **Зависит от:** Plan 118 (типизированные указатели `*T`).
>
> **Production-grade, без упрощений:** нет TODO/заглушек; `ptr` ловится на
> этапе type-check (не откладывается до codegen), C-кодоген эмитит `void*`,
> casts работают, регрессий нет (0 net-new FAIL vs main baseline).

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

- **A-134.a** ✅ — `ptr`/`nova_ptr` в Nova-коде в type-позиции → `nova check`
  error `[E_TYPE_UNKNOWN] type \`ptr\` is removed — use \`*()\` …` + hint
  (`type ptr = *()`). Срабатывает на этапе type-check (`types/mod.rs`
  `walk_typeref` guard), не откладывается до codegen. Defensive codegen-time
  зеркало (`emit_c.rs` / `external_registry.rs`) на случай обхода checker'а.
  Verify: `nova_tests/plan134/t1_ptr_removed_neg.nv` PASS.
- **A-134.b** ✅ — `*()` в type position → C-тип `void*`
  (`t2_unit_ptr_type_ok` PASS).
- **A-134.c** ✅ — `(0 as *())` → null pointer, работает
  (`t3_null_unit_ptr_ok` PASS).
- **A-134.d** ✅ — `nova_ptr` typedef удалён из `nova_rt.h`
  (осталась только debug-helper `nova_ptr_to_debug_str(const void*)` —
  это имя функции, не тип).
- **A-134.e** ✅ — plan115 тесты PASS с `*()` (11/0).
- **A-134.f** ✅ — `OnceCell[T](*())` / `Lazy[T](*())` / `Condvar(*())`
  компилируются и работают (plan103_5 20/0, plan91_12 9/0, plan103_4 25/0).
- **A-134.g** ✅ — 0 net-new FAIL vs main baseline (plan118 37/3, syntax 47/7,
  plan62 29/7 — все FAIL pre-existing и identical к main bin'арю; basics 8/0).
- **A-134.h** ✅ — **production-grade, без упрощений**: нет TODO/заглушек;
  полная миграция `ptr`/`nova_ptr` по compiler + std + examples + nova_tests
  (остатки — только prose-комментарии и поле-имя `ptr` в `str{ptr,len}`).

### Cast-matrix verification (A-134.c расширенный)

`nova_tests/plan134/t4_unit_ptr_cast_ok.nv` PASS — round-trip
`int ↔ *()`, `u64 ↔ *()`, `i64 ↔ *()`, `(0 as *())` → 0, wide-address bits.

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
