<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 167 — Vec generic forward-decl missing for body-only instantiations (D300)

> **Создан:** 2026-06-17. **Статус:** 📋 PLANNED. **P1**; ~0.5 dd, Sonnet 4.6.
>
> **Симптом:** `CC-FAIL: unknown type name 'Nova_Vec____Nova_u32_p'` в `plan153_1/*`,
> `plan118_6/t8_neg_addr_of_mut_removed`. Все 9 тестов plan153_1 падают на clang-стадии,
> хотя Nova type-check проходит (`nova check` — PASS).
>
> **Зависит от:** ничего (isolated codegen fix).
> **Разблокирует:** план153_1 (9→0 CC-FAIL), любые future `Vec[u8]`/`Vec[u16]`/`Vec[i32]` в телах.

---

## 0. Диагноз

### Два независимых манглинга для одного типа

`u32` в Nova — это value-record newtype (`type u32 value { ... }`) над `uint32_t`. Поэтому:

| Nova-тип | С-мангл (корректный) | С-мангл (проблемный) |
|---|---|---|
| `Vec[u32]` как сигнатура/поле | `Nova_Vec____uint32_t` | — |
| `Vec[u32]` в теле функции | `Nova_Vec____Nova_u32_p` | ← неправильно |

Для типа из **сигнатуры или поля** `simple_type_ref_to_c` и `apply_type_subst_to_ref`
содержат явный матч `"u32" => "uint32_t"` (добавлено Plan 152.8). Для типа из
**тела функции** (локальная переменная, inline-конструктор) путь через
`infer_expr_c_type` / `emit_mono_call` другой — и там матч может отсутствовать или
fallthrough в generic-user-type ветку `format!("Nova_{}*", other)`.

### Почему forward-decl отсутствует глобально

Pre-pass `collect_array_elem_typerefs` (строки 2690–2748 emit_c.rs) сканирует:
- `module.items` → `Item::Type` поля + `Item::Fn` params/return_type
- `peer_files.items_here` — то же

Он **не заходит в тела функций** (`FnDecl.body` / `Block.stmts`). Поэтому если
`Vec[u32]` встречается только как локальная переменная (например, в
`std/unicode/*.nv`: `mut cps = Vec[u32].with_capacity(s.byte_len())`), forward-decl
`typedef struct Nova_Vec____Nova_u32_p Nova_Vec____Nova_u32_p;` НЕ добавляется в
глобальный preamble (`user_type_fwd_decls`).

Позже mono-pass генерирует struct-определение в контексте тела функции (с отступом),
что не вводит новый C-scope но создаёт путаницу при forward-ссылке из tuple-typedef
(строка ~171 в core_api.c), сгенерированного ДО определения.

### Affected types (текущие)

- `Vec[u32]` → `Nova_Vec____Nova_u32_p` — **активно сломан** (plan153_1 9 CC-FAIL)

Потенциально та же дыра (если появятся в body-only позиции):
- `Vec[u8]`, `Vec[u16]`, `Vec[u64]`, `Vec[i8]`, `Vec[i16]`, `Vec[i32]` —
  mangle в `Nova_Vec____Nova_u8_p` и т.д.

---

## 1. Решение

### Вариант A — расширить pre-pass на тела функций (рекомендуется)

В `scan_item` (emit_c.rs ~2692) для `Item::Fn(f)` добавить walk по `f.body`:

```rust
Item::Fn(f) => {
    for p in &f.params {
        Self::collect_array_elem_typerefs(&p.ty, acc);
    }
    if let Some(r) = &f.return_type {
        Self::collect_array_elem_typerefs(r, acc);
    }
    // Plan 167: scan fn body for Vec[T] local vars
    Self::collect_array_elem_typerefs_in_body(&f.body, acc);
}
```

`collect_array_elem_typerefs_in_body` — рекурсивный walk по `Block` / `Stmt` /
`Expr`, собирающий `TypeRef` из:
- `Stmt::Let(LetDecl { ty: Some(t), .. })` — явная аннотация типа
- `ExprKind::TypeInst { .. }` — `Vec[u32].with_capacity(...)` TurboFish
- `ExprKind::RecordLit { type_ref, .. }` — `Vec[u32] { ... }`
- Рекурсивно по всем sub-expr / sub-stmt

Сканировать не нужно каждый expr глубоко — достаточно TypeRef на уровне Let-аннотаций
и TurboFish вызовов (именно они генерируют Vec[T] local vars).

### Вариант B — late forward-decl при mono-emit

Альтернатива: в месте где mono-pass впервые видит `Nova_Vec____Nova_u32_p` (внутри
тела), форсировать emit forward-decl в `late_fwd_decls` буфер, который
сплайсится в начало файла. Сложнее — требует нового буфера и места сплайса.

**Выбор: Вариант A** — хирургически чище, ограниченный scope изменений, не нужен
новый буфер.

---

## 2. Фазы

### Ф.0 — Probe: минимальный репро-тест

Создать `nova_tests/plan167/vec_u32_body.nv`:
```nova
module plan167.vec_u32_body
test "Vec[u32] local var" {
    mut v = Vec[u32].with_capacity(4)
    v.push(1 as u32)
    v.push(2 as u32)
    assert(v.len() == 2)
}
```

Запустить — ожидаем CC-FAIL с `unknown type name 'Nova_Vec____Nova_u32_p'`.

### Ф.1 — Fix: `collect_array_elem_typerefs_in_body`

В `emit_c.rs`:

1. Добавить функцию `collect_array_elem_typerefs_in_body(body: &FnBody, out: &mut Vec<TypeRef>)`
   с walk по `Block` → `Stmt` → Let.ty + Expr TypeRefs.

2. В `scan_item` для `Item::Fn` добавить вызов на `f.body`.

3. Аналогично для `peer_files` (уже итерируется в том же цикле).

Не нужно заходить в тела лямбд / методов-протоколов — они обрабатываются отдельным
mono-pass и не генерируют Vec-local vars без сигнатурного контекста.

### Ф.2 — Verify

1. `nova test nova_tests/plan167` → ранее CC-FAIL → теперь PASS.
2. `nova test nova_tests/plan153_1` → 0 CC-FAIL (было 9).
3. Полный регресс: 0 новых FAIL.

### Ф.3 — Spec + закрытие

- D300 в `spec/decisions/02-types.md` — одна секция (diagnostic + fix)
- Обновить plan-doc, README, backlog, simplifications, project-creation.txt

---

## 3. Acceptance criteria

| # | Критерий |
|---|---|
| AC1 | `nova test nova_tests/plan167` — PASS (pos: Vec[u32] local var) |
| AC2 | `nova test nova_tests/plan153_1` — 0 CC-FAIL (было 9) |
| AC3 | 0 новых FAIL в полном регрессе |
| AC4 | Тест `nova_tests/plan167/vec_u8_body.nv` — PASS (профилактика Vec[u8]) |
| AC5 | Без упрощений как для прода |

---

## 4. D300 (spec)

**D300 — Vec generic forward-decl completeness: body-site scan (Plan 167, 2026-06-17)**

Pre-pass `collect_array_elem_typerefs` расширен на тела функций (Let-аннотации +
TurboFish вызовы). Гарантирует, что `typedef struct Nova_Vec____<elem> ...;` в
глобальном preamble генерируется для всех instantiation-сайтов, включая локальные
переменные внутри функций, а не только для полей записей и сигнатур.

Без этого: `Vec[u32]` в теле `std/unicode/*.nv` генерировал
`Nova_Vec____Nova_u32_p` без forward-decl → CC-FAIL «unknown type name».
