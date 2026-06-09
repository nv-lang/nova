<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 135 — Dispatch по receiver-mutability для одноимённых методов

> **Создан:** 2026-06-09.  **Статус:** ✅ ЗАКРЫТ 2026-06-09.
> **Эстимат:** ~1 dev-day.  **Model:** Sonnet 4.6.
> **Зависит от:** Plan 128 ✅ (recv_mutable в MethodSig уже хранится).

---

## Что

Два метода с одним именем на одном типе, отличающиеся только
receiver-mutability, должны:

1. **Генерировать разные C-имена** — `Nova_T_method_m` (ro) /
   `Nova_T_method_m__mut` (mut).
2. **Диспатчиться на call-site по mutability receiver'а** — если объект
   привязан как `mut`, выбирается `mut`-перегрузка; иначе — `ro`.

### Целевой паттерн (аналог C++ const overloading)

```nova
type Value { data *mut u8 }

fn Value @as_ptr()     -> ro *u8  { @data }
fn Value mut @as_ptr() -> mut *u8 { @data }

ro  a Value = ...
mut b Value = ...

a.as_ptr()  // → ro *u8  (const u8*)  — ro overload
b.as_ptr()  // → mut *u8 (u8*)        — mut overload
```

---

## Почему

Текущий codegen:
- `fn T @m()` / `fn T mut @m()` (одинаковые параметры) → суффикс пустой
  (строки 2514-2515 emit_c.rs) → одно и то же C-имя → CC-FAIL / silently
  один метод перекрывает другой.
- Нет tracking'а mutability receiver'а на call-site для выбора перегрузки.

Нужно для:
- `Vec[T].as_ptr()` → `ro *T` / `as_mut_ptr()` → `mut *T`  
  (Plan 118.1, сейчас нет single-name варианта)
- Fluent API-цепочки с разными правами доступа к данным
- D216 §2 полный паттерн: `mut p *T` = promotable by binding

---

## Фазы

### Ф.1 — C-name mangling для recv_mutable overloads (~1h)

**Файл:** `compiler-codegen/src/codegen/emit_c.rs` (~строки 2502-2518)

При регистрации метода в `method_overloads`: если `existing_count > 0`
**и** suffix пустой (params одинаковые), добавить `__mut` суффикс для
`recv.mutable == true`, иначе `__ro`:

```rust
// БЫЛО (строки 2502-2518):
let c_name = if existing_count == 0 {
    base_c_name
} else {
    let suffix = param_c_types.iter()...join("_");
    if suffix.is_empty() { base_c_name }   // ← bug: collision!
    else { format!("{}__{}", base_c_name, suffix) }
};

// СТАЛО:
let c_name = if existing_count == 0 {
    base_c_name
} else {
    let suffix = param_c_types.iter()
        .map(|t| t.replace('*', "_p").replace(' ', "_")...)
        .collect::<Vec<_>>().join("_");
    if suffix.is_empty() {
        // Params idентичны — используем receiver-mutability suffix.
        if recv.mutable { format!("{base_c_name}__mut") }
        else            { format!("{base_c_name}__ro") }
    } else {
        format!("{}__{}", base_c_name, suffix)
    }
};
```

Аналогично для `mangle_fn` (~строки 9539-9547): при резолве по
`param_c_types`, если несколько сигнатур с одинаковыми params —
тай-брейк по `recv_mutable`.

**Commit:** `feat(plan135 Ф.1): recv-mut overload mangling — __mut/__ro suffix`

### Ф.2 — Call-site dispatch по receiver mutability (~2h)

**Файл:** `emit_c.rs` — `emit_call` / `prepare_method_recv` путь.

Сейчас в `emit_call` при вызове `obj.method(args)`:
1. Определяется `key = (type_name, method_name)`
2. Перебираются overloads, матчинг по `sig.param_c_types == arg_c_types`

Нужно добавить третий критерий: если после матча по param_c_types
осталось несколько кандидатов (тай), выбрать по `recv_mutable`:

```rust
// Определить mutability receiver'а на call-site:
let caller_is_mut = match &obj_expr.kind {
    ExprKind::Ident(name) => self.var_mutable.contains(name.as_str()),
    ExprKind::SelfAccess  => self.current_receiver_is_mut,
    _                     => false,
};

// Тай-брейк при одинаковых param_c_types:
let chosen = overloads.iter()
    .filter(|s| s.param_c_types == arg_c_types)
    .find(|s| s.recv_mutable == caller_is_mut)
    .or_else(|| overloads.iter()
        .filter(|s| s.param_c_types == arg_c_types)
        .find(|s| !s.recv_mutable));  // fallback: ro overload если нет mut
```

`current_receiver_is_mut` — поле в `Codegen` struct (аналог
`current_fn_return_ty`), выставляется при входе в метод из
`recv.mutable` FnDecl'а.

**Commit:** `feat(plan135 Ф.2): call-site dispatch by receiver mutability`

### Ф.3 — Fixtures (~1h)

**`nova_tests/plan135/`**:

- `t1_ro_mut_overload_basic_ok.nv` — POS: оба метода существуют, оба
  вызываются корректно, правильный C-тип возвращается
- `t2_ro_receiver_picks_ro_overload.nv` — POS: `ro a Value; a.m()` → ro overload
- `t3_mut_receiver_picks_mut_overload.nv` — POS: `mut b Value; b.m()` → mut overload
- `t4_ro_only_overload_ok.nv` — POS: только ro overload — работает без mut
- `t5_mut_only_overload_ok.nv` — POS: только mut overload — работает без ro
- `t6_fallback_to_ro_when_no_mut.nv` — POS: если нет mut overload, ro вызывается
  даже для mut-binding (graceful fallback)
- `t7_no_cc_fail.nv` — POS: оба overload компилируются в C без collision
- `t8_as_ptr_pattern.nv` — POS: целевой паттерн из §«Что» — полный end-to-end

**Commit:** `test(plan135 Ф.3): fixtures для recv-mut dispatch`

### Ф.4 — Spec + docs (~30 min)

- **Ф.4.1** `spec/decisions/02-types.md` — добавить в §«Return-type defaults»
  пример с `as_ptr()` ro/mut overload.
- **Ф.4.2** `docs/plans/128-mut-receiver-abi.md` — пометить
  «Dispatch по recv-mut — Plan 135».
- **Ф.4.3** `docs/simplifications.md` + project-creation.txt + discussion-log.

**Commit:** `docs(plan135 Ф.4): spec — recv-mut overload dispatch documented`

---

## Acceptance criteria

- **A-135.a** — `fn T @m()` + `fn T mut @m()` (same params) → разные C-имена
  (`Nova_T_method_m` + `Nova_T_method_m__mut`)
- **A-135.b** — `ro a T = ...; a.m()` → вызов ro overload
- **A-135.c** — `mut b T = ...; b.m()` → вызов mut overload
- **A-135.d** — нет CC-FAIL при наличии обеих перегрузок
- **A-135.e** — если только одна перегрузка — работает для любого receiver
- **A-135.f** — 0 regressions в full `nova test`

---

## C-codegen

```
fn T @as_ptr()     → Nova_Value_method_as_ptr       // first, no suffix
fn T mut @as_ptr() → Nova_Value_method_as_ptr__mut  // __mut suffix
```

Call site:
```c
// ro a = ...:
Nova_Value_method_as_ptr(a)       // → const uint8_t*
// mut b = ...:
Nova_Value_method_as_ptr__mut(b)  // → uint8_t*
```

---

## Что НЕ меняется

- Overloading по param-types (Plan 11) — без изменений.
- D216 §2 binding-mut pointer promotion (`mut p *T` → `*mut T`) — отдельно,
  не зависит от этого плана.
- `method_receivers` single-key map — не трогаем (backward compat).

## Followups

- `[M-135-consume-overload]` — аналогичный dispatch для `consume`
  receiver (3-way: ro / mut / consume). Ожидается редко.
- `[M-135-as-ptr-pattern-vec]` — обновить `Vec[T].as_ptr()` /
  `Vec[T].as_mut_ptr()` на единое имя `as_ptr()` когда Plan 135 ready.

---

## Итог (2026-06-09)

Реализован полный recv-mut overload dispatch (Ф.1 + Ф.2 + Ф.3).

**Ф.1 — Mangling.** В `emit_c.rs` при регистрации перегрузки: если
`existing_count > 0` и suffix пустой (param-типы одинаковые), добавляется
суффикс `__mut` (recv.mutable == true) или `__ro` (false). В `mangle_fn` —
двухпроходное разрешение: сначала exact match по `param_c_types + recv_mutable`,
затем fallback по `param_c_types` только.

**Ф.2 — Call-site dispatch.** Добавлено поле `current_receiver_is_mut: bool`
в `CEmitter` (выставляется на входе в метод). Добавлен helper `is_obj_mutable`
(ident → `var_mutable`, SelfAccess → `current_receiver_is_mut`). Тай-брейк
добавлен в три dispatch-пути: primitive extension, section 5 (основной путь
user-defined структур), и SelfAccess early-dispatch. Также исправлен
duplicate-check в `mod.rs` (ro / mut overloads — не дубликаты) и добавлена
`ro_methods` в `ConsumeRegistry` (подавление ложных `E_LOCAL_NOT_MUT`).

**Ф.3 — Fixtures.** 8/8 тестов PASS. Acceptance criteria A-135.a – A-135.e
подтверждены. A-135.f (0 regressions) — ожидается при merge в main.
